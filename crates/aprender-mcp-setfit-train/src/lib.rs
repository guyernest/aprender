//! Thin single-algorithm MCP TRAINING server: SetFit few-shot training exposed
//! as an async MCP Task (spec 2025-11-25), plus a polling tool for clients that
//! do not speak tasks.
//!
//! # Training goes through the CLI door, not a third ingest sequence
//!
//! The predict server (`aprender-mcp-setfit`) calls core IN-PROCESS because
//! predict's one door is a public library API (`VerifiedSetFitModel::classify`,
//! D-09/OPS-03). Training's one door is different: it is the `apr setfit train`
//! ADAPTER, whose attested-ingest sequence is deliberately `pub(crate)` in
//! apr-cli — `read_attested_canonical`'s own doc says widening it was rejected
//! precisely so there can never be "two readers of benchmark-manifest.json".
//! An in-process trainer here would be that second (third) reader. So this
//! server SUPERVISES a pinned `apr` binary as a child process instead: the same
//! validation, the same atomic artifact write, the same `--json` report — and
//! the same job shape that later runs unchanged inside a Lambda container or as
//! a SageMaker container entrypoint.
//!
//! The binary is PINNED (an explicit path, validated at startup), never
//! resolved from `PATH` — four coexisting `apr` binaries on one dev box is the
//! documented failure mode this rule exists for.
//!
//! # The task is paired by OBSERVATION, not by guessing
//!
//! pmcp mints the store task id AFTER the tool handler returns, so a handler
//! can never know the id its own call will be given. The SDK has no hook for
//! COMPLETION — its s50 example says so outright — but it does have one for
//! CREATION and CANCELLATION, and it is the [`TaskStore`] the application
//! already supplies: dispatch calls `store.create(owner, ttl)` and
//! `store.cancel(task_id, owner)` on YOUR store, inside the same request
//! future as the handler.
//!
//! [`TrainingTaskStore`] is that hook. `create` records the minted id and the
//! owner dispatch actually resolved against the job this submit admitted, and
//! `cancel` relays straight into the child's kill. The waiter then writes the
//! terminal state itself when the child exits. There is no polling loop and no
//! pairing heuristic.
//!
//! An earlier revision paired by "bind the store task to the oldest job with no
//! task id" and leaned on single-flight to make that unambiguous. It is not:
//! `submit_job` also runs for PLAIN (non-task-augmented) calls, which mint no
//! store task at all, so such a job stayed unbound forever and the NEXT
//! task-augmented call adopted it — serving the previous run's report under the
//! new task's id. Single-flight bounds "one job RUNNING", never "one job
//! unbound in history"; those are different invariants.
//!
//! Observing the owner rather than assuming it also removes a second trap: the
//! owner is not a constant but the output of a per-era decision (`"local"` only
//! for an unauthenticated 2025-11-25 client; the OAuth subject with a provider,
//! `""` on 2026-07-28). A hardcoded bucket silently matches nothing the moment
//! a client authenticates or negotiates v2.
//!
//! # One job at a time
//!
//! [`JobRegistry`] refuses a second submit while one runs: training saturates
//! the CPU, and the deploy target is one container per job. This is a RESOURCE
//! policy — with the pairing observed, it no longer carries any correctness
//! weight.
//!
//! # Fail on the REQUEST before blaming the run
//!
//! Submit runs `apr setfit train --dry-run` SYNCHRONOUSLY first — the CLI's own
//! pre-flight (config validated as a whole, device resolved, output refused,
//! data/selection strictly replayed) — so a bad request is refused at the MCP
//! boundary in seconds, as a tool error, instead of surfacing minutes later as
//! a failed job. Note the cost: pmcp dispatches requests through a single
//! worker (`Server::spawn_request_worker` — "request handling stays
//! serialized"), so this window blocks every other request too. That is why the
//! pre-flight budget is minutes-free and deliberately tight.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use pmcp::server::task_store::{
    StoreConfig, TaskInputDelivery, TaskInputSnapshot, TaskStore, TaskStoreError,
};
use pmcp::server::typed_tool::TypedTool;
use pmcp::types::capabilities::ServerCapabilities;
use pmcp::types::mrtr::{InputRequests, InputResponses};
use pmcp::types::tasks::Task;
use pmcp::types::{CallToolResult, Content, TaskStatus, TaskSupport, ToolExecution};
use pmcp::Server;
use tokio::sync::{oneshot, Mutex, Notify};

pub use args::{StatusArgs, TrainArgs};

/// The server identity both the stdio runner and any transport wrapper report.
pub const SERVER_NAME: &str = "aprender-setfit-train";

/// The one training tool.
pub const TOOL_TRAIN: &str = "train";

/// The polling companion for clients without MCP Tasks support.
pub const TOOL_STATUS: &str = "train_status";

/// TTL requested for minted training tasks: generous next to the measured
/// envelope (127 s wall for the 8-shot reference train on an M-series host),
/// because an expired task discards a finished artifact's result.
const TASK_TTL_MS: u64 = 3_600_000;

/// Budget for the synchronous `--dry-run` pre-flight. It replays manifests and
/// deliberately does NOT open the encoder, so it is a seconds-scale step — and
/// because pmcp serializes request handling, this is also the worst case for
/// how long the server can answer nothing else. Tight on purpose.
const DRY_RUN_TIMEOUT_SECS: u64 = 30;

/// How long the waiter will wait for dispatch to mint this submit's task
/// before concluding there is none. `store.create` runs milliseconds after the
/// handler returns while training runs for minutes, so this only ever matters
/// for a child that fails almost instantly.
///
/// A PLAIN call does not resolve it early. Nothing clears the pending slot on
/// the no-task path — only the next `begin` overwrites it — so a plain
/// submit's waiter always burns this full timeout before returning. That is
/// harmless (the job's terminal state is already recorded by then) but it is
/// not what an earlier revision of this comment claimed, and the difference
/// matters if the value is ever raised.
const BIND_GRACE: Duration = Duration::from_secs(5);

/// Envelope version for every JSON payload this server emits (status tool
/// result, submit handle and mirrored task result alike — one shape, three
/// doors).
const STATUS_SCHEMA_VERSION: u64 = 1;

/// Cap on each captured output stream in a failure message: enough to carry a
/// refusal, not a whole training log.
const OUTPUT_TAIL_BYTES: usize = 2_000;

/// TRANSPORT bound on the client-supplied `config` document, mirroring
/// `aprender::setfit::MAX_REQUEST_BODY_BYTES` (1 MiB), which the predict
/// sibling enforces for the same reason: a reading surface owes a size bound
/// even when it owes no semantic validation.
///
/// Without it a single call could hand this server an arbitrarily large JSON
/// object, which it would parse, re-serialize into a second full copy, and
/// then WRITE INTO `output_dir` before the CLI ever saw it — an OOM or a
/// filled disk on the container deploy target the README describes. The
/// twelve-knob SetFit config is a few hundred bytes; this is four orders of
/// magnitude of headroom.
const MAX_CONFIG_BYTES: usize = 1_048_576;

/// The `taskId` the submit handle carries purely to satisfy pmcp's create gate.
/// Dispatch discards it and mints the canonical id, so it is written to say so.
const GATE_TASK_ID: &str = "handler-fabricated-discarded";

mod args {
    // The JsonSchema derive expands serde_json::json!, which expands to
    // .unwrap() internally — same scoped exception as aprender-mcp-setfit's
    // args module, kept this narrow so the ban still covers everything else.
    #![allow(clippy::disallowed_methods)]

    use schemars::JsonSchema;
    use serde::Deserialize;

    /// Arguments for [`super::TOOL_TRAIN`].
    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct TrainArgs {
        /// The full twelve-knob SetFit training configuration, passed through
        /// VERBATIM to `apr setfit train --config`. This server validates
        /// nothing about it on purpose: the CLI's single validating
        /// constructor is the one implementation of config legality, and a
        /// second validator here could only drift from it.
        pub config: serde_json::Value,
    }

    /// Arguments for [`super::TOOL_STATUS`].
    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct StatusArgs {
        /// A `job_id` returned by `train`. Omitted: the most recent job.
        #[serde(default)]
        pub job_id: Option<String>,
    }
}

/// Everything the operator provisions; nothing here comes from the client.
///
/// The client varies the training CONFIG; the server owns which dataset,
/// which selection, which encoder checkout and where artifacts land. That is
/// the thin-server philosophy applied to training: a business-curated
/// connector to one training recipe, not a general job runner.
#[derive(Debug, Clone)]
pub struct TrainerPaths {
    /// The pinned `apr` binary (must carry the `setfit` feature).
    pub apr_bin: PathBuf,
    /// Attested benchmark directory, as written by `apr data tweet-eval-stance`.
    pub data: PathBuf,
    /// The selection manifest, as written by `apr data select`.
    pub selection: PathBuf,
    /// Pinned all-MiniLM-L6-v2 checkout (the CLI never downloads).
    pub model_dir: PathBuf,
    /// Where per-job config files and trained artifacts are written.
    pub output_dir: PathBuf,
}

impl TrainerPaths {
    /// Refuse a misconfigured server at STARTUP, naming the first missing
    /// piece — not at the first submit, minutes into someone's workflow.
    ///
    /// # Errors
    ///
    /// A human-readable message naming the path and what was expected of it.
    ///
    /// Every message names BOTH doors — the flag and its environment variable.
    /// The documented Lambda/transport-wrapper deployment configures this
    /// server entirely through `APRENDER_SETFIT_TRAIN_*` with no argv at all,
    /// so a refusal that names only `--model-dir` sends that operator grepping
    /// their container spec for a flag nobody ever passed. `resolve()` in the
    /// stdio runner already names both; these messages used to name one.
    pub fn validate(&self) -> Result<(), String> {
        if !self.apr_bin.is_file() {
            return Err(format!(
                "--apr-bin (APRENDER_SETFIT_TRAIN_APR_BIN) {} is not a file; point it at a \
                 pinned, setfit-featured `apr` (the $APR that scripts/apr_bin.sh exports)",
                self.apr_bin.display()
            ));
        }
        if !self.data.is_dir() {
            return Err(format!(
                "--data (APRENDER_SETFIT_TRAIN_DATA) {} is not a directory; expected an \
                 attested benchmark dir (benchmark-manifest.json + split JSONL)",
                self.data.display()
            ));
        }
        if !self.selection.is_file() {
            return Err(format!(
                "--selection (APRENDER_SETFIT_TRAIN_SELECTION) {} is not a file; expected a \
                 selection-manifest.json",
                self.selection.display()
            ));
        }
        if !self.model_dir.is_dir() {
            return Err(format!(
                "--model-dir (APRENDER_SETFIT_TRAIN_MODEL_DIR) {} is not a directory; expected \
                 a pinned all-MiniLM-L6-v2 checkout",
                self.model_dir.display()
            ));
        }
        std::fs::create_dir_all(&self.output_dir).map_err(|e| {
            format!(
                "--output-dir (APRENDER_SETFIT_TRAIN_OUTPUT_DIR) {} cannot be created: {e}",
                self.output_dir.display()
            )
        })?;
        self.probe_setfit_subcommand()
    }

    /// `is_file()` is not enough: `setfit` is deliberately NOT in apr-cli's
    /// `default` feature set, so the `apr` a plain `cargo build --release`
    /// produces — the command the repo's own docs give — has no `setfit`
    /// subcommand at all. That binary passes every check above, the server
    /// starts, advertises `train`, and then dies on EVERY submit inside the
    /// pre-flight with clap's `unrecognized subcommand`.
    ///
    /// This function's whole promise is "name the first missing piece at
    /// STARTUP, not at the first submit"; the likeliest missing piece was the
    /// one it did not look for. `--help` is free (no model load, no I/O).
    fn probe_setfit_subcommand(&self) -> Result<(), String> {
        let probe = std::process::Command::new(&self.apr_bin)
            .args(["setfit", "train", "--help"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output();
        match probe {
            Err(e) => Err(format!(
                "--apr-bin (APRENDER_SETFIT_TRAIN_APR_BIN) {} could not be executed: {e}",
                self.apr_bin.display()
            )),
            Ok(out) if !out.status.success() => Err(format!(
                "--apr-bin (APRENDER_SETFIT_TRAIN_APR_BIN) {} does not answer \
                 `setfit train --help` ({}): build it with \
                 `--features setfit` — setfit is NOT in apr-cli's default feature set, so a \
                 plain `cargo build --release` produces an apr this server cannot use.\n{}",
                self.apr_bin.display(),
                out.status,
                tail(&out.stderr, OUTPUT_TAIL_BYTES)
            )),
            Ok(_) => Ok(()),
        }
    }
}

/// A job's terminal verdict. `None` on the [`Job`] means still running — so
/// "which phase" and "what came out of it" are ONE field, and the states that
/// used to be representable but meaningless (completed-with-an-error,
/// failed-with-a-report) no longer exist.
#[derive(Debug)]
enum JobOutcome {
    /// Exit 0: the parsed `--json` report.
    Completed(serde_json::Value),
    /// Non-zero exit, or a supervision failure: what the CLI said.
    Failed(String),
    /// Killed by a relayed `tasks/cancel`.
    Cancelled,
}

impl JobOutcome {
    const fn phase(&self) -> &'static str {
        match self {
            Self::Completed(_) => "completed",
            Self::Failed(_) => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    const fn is_failure(&self) -> bool {
        !matches!(self, Self::Completed(_))
    }
}

/// The store task dispatch minted for one submit, as OBSERVED in
/// [`TrainingTaskStore::create`] — never assumed.
#[derive(Debug, Clone)]
struct Binding {
    task_id: String,
    owner_id: String,
}

/// One training run, from submit to terminal state.
#[derive(Debug)]
struct Job {
    id: String,
    started_unix_ms: u64,
    finished_unix_ms: Option<u64>,
    /// `None` while running; the child's verdict once terminal.
    outcome: Option<JobOutcome>,
    artifact_path: PathBuf,
    /// The store task this job was paired with, if the call was task-augmented.
    binding: Option<Binding>,
    /// Fired to kill the child (relayed `tasks/cancel`).
    cancel: Arc<Notify>,
}

/// The one status shape every surface serves — `train_status`'s result, the
/// submit handle, and the terminal task result are all THIS, so no two doors
/// can tell different stories about the same run.
#[derive(Debug, serde::Serialize)]
struct StatusPayload<'a> {
    schema_version: u64,
    job_id: &'a str,
    /// The paired store task, so a task client can query `train_status` about
    /// its own run and a polling client can discover its task id. Absent for a
    /// plain call, which mints no task.
    task_id: Option<&'a str>,
    phase: &'static str,
    started_unix_ms: u64,
    finished_unix_ms: Option<u64>,
    artifact_path: String,
    report: Option<&'a serde_json::Value>,
    error: Option<&'a str>,
    /// Present only on the submit handle: the two keys pmcp's create gate
    /// requires to recognize a task-shaped value. The id is the handler's and
    /// is DISCARDED — the store mints the canonical one — so it is deliberately
    /// self-describing rather than plausible.
    #[serde(rename = "taskId", skip_serializing_if = "Option::is_none")]
    gate_task_id: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ttl: Option<u64>,
}

impl Job {
    fn status_payload(&self) -> StatusPayload<'_> {
        // `phase` comes from `JobOutcome::phase()` in EVERY arm. An earlier
        // revision hardcoded "completed" here while `phase()` also defined it,
        // so the one mapping lived in two places and no test forced them to
        // agree — changing either silently diverged from the other.
        let phase = self.outcome.as_ref().map_or("running", JobOutcome::phase);
        let (report, error) = match self.outcome.as_ref() {
            None => (None, None),
            Some(JobOutcome::Completed(report)) => (Some(report), None),
            Some(JobOutcome::Failed(message)) => (None, Some(message.as_str())),
            Some(JobOutcome::Cancelled) => (None, Some("cancelled by the client (tasks/cancel)")),
        };
        StatusPayload {
            schema_version: STATUS_SCHEMA_VERSION,
            job_id: &self.id,
            task_id: self.binding.as_ref().map(|b| b.task_id.as_str()),
            phase,
            started_unix_ms: self.started_unix_ms,
            finished_unix_ms: self.finished_unix_ms,
            artifact_path: self.artifact_path.display().to_string(),
            report,
            error,
            gate_task_id: None,
            status: None,
            ttl: None,
        }
    }

    /// Infallible in practice (string keys, no custom serializers); Null rather
    /// than a panic if that ever changes.
    fn status_value(&self) -> serde_json::Value {
        serde_json::to_value(self.status_payload()).unwrap_or(serde_json::Value::Null)
    }
}

/// A successful admission: what the caller needs to drive the run.
#[derive(Debug)]
struct Admitted {
    job_id: String,
    artifact_path: PathBuf,
    cancel: Arc<Notify>,
    /// Resolves when [`TrainingTaskStore::create`] pairs this submit with the
    /// task dispatch minted for it. Resolves to `Err` if the sender is dropped
    /// — which is exactly what a plain, non-task-augmented call does.
    binding: oneshot::Receiver<Binding>,
}

/// The submit awaiting its store task, if any. Single-flight means at most one.
#[derive(Debug)]
struct Pending {
    job_id: String,
    tx: oneshot::Sender<Binding>,
}

/// The single-flight job book. All jobs are kept (a finished job's report stays
/// queryable for the life of the server); at most one is running.
#[derive(Default)]
pub struct JobRegistry {
    jobs: Mutex<Vec<Job>>,
    pending: Mutex<Option<Pending>>,
    seq: AtomicU64,
}

impl JobRegistry {
    /// Admit a job, or refuse because one is already running (single-flight).
    ///
    /// # Errors
    ///
    /// The running job's id, so the caller can poll it instead of retrying.
    async fn begin(&self, output_dir: &Path) -> Result<Admitted, String> {
        let mut jobs = self.jobs.lock().await;
        if let Some(active) = jobs.iter().find(|j| j.outcome.is_none()) {
            return Err(format!(
                "a training job is already running ({}); this server trains one model at a \
                 time — poll `{TOOL_STATUS}` and resubmit when it finishes",
                active.id
            ));
        }
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let started = unix_ms();
        let job_id = format!("job-{started}-{seq}");
        let artifact_path = output_dir.join(format!("{job_id}.apr"));
        let cancel = Arc::new(Notify::new());
        jobs.push(Job {
            id: job_id.clone(),
            started_unix_ms: started,
            finished_unix_ms: None,
            outcome: None,
            artifact_path: artifact_path.clone(),
            binding: None,
            cancel: Arc::clone(&cancel),
        });
        let (tx, binding) = oneshot::channel();
        // Replacing any previous slot drops its sender, which resolves that
        // waiter's receiver as Err — the correct answer for a submit whose
        // call never became a task.
        *self.pending.lock().await = Some(Pending {
            job_id: job_id.clone(),
            tx,
        });
        Ok(Admitted {
            job_id,
            artifact_path,
            cancel,
            binding,
        })
    }

    /// Record a job's terminal verdict. Unknown ids are ignored (a job can only
    /// finish once; the waiter is the sole caller).
    async fn finish(&self, job_id: &str, outcome: JobOutcome) {
        let mut jobs = self.jobs.lock().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
            job.finished_unix_ms = Some(unix_ms());
            job.outcome = Some(outcome);
        }
    }

    /// The status payload for `job_id`, or the most recent job when `None`.
    async fn status(&self, job_id: Option<&str>) -> Option<serde_json::Value> {
        let jobs = self.jobs.lock().await;
        match job_id {
            Some(id) => jobs.iter().find(|j| j.id == id).map(Job::status_value),
            None => jobs.last().map(Job::status_value),
        }
    }

    /// The submit response: this job's status payload plus the two keys pmcp's
    /// create gate requires to recognize a task-shaped value (`taskId` and
    /// `status`), and the TTL it carries onto the minted task.
    ///
    /// Building it from the SAME payload every other door serves is what makes
    /// "one shape, three doors" true rather than aspirational — a hand-rolled
    /// handle here is how the submit response drifts from `train_status`.
    async fn submit_handle(&self, job_id: &str) -> Option<serde_json::Value> {
        let jobs = self.jobs.lock().await;
        let job = jobs.iter().find(|j| j.id == job_id)?;
        let mut payload = job.status_payload();
        // Dispatch DISCARDS this id and mints its own, so it is deliberately
        // self-describing rather than plausible: anything that echoes it back
        // is reading the wrong field.
        payload.gate_task_id = Some(GATE_TASK_ID);
        payload.status = Some("working");
        payload.ttl = Some(TASK_TTL_MS);
        Some(serde_json::to_value(payload).unwrap_or(serde_json::Value::Null))
    }

    /// The terminal payload for `job_id` and whether it is a failure.
    async fn terminal_payload(&self, job_id: &str) -> Option<(serde_json::Value, bool)> {
        let jobs = self.jobs.lock().await;
        let job = jobs.iter().find(|j| j.id == job_id)?;
        let failed = job.outcome.as_ref()?.is_failure();
        Some((job.status_value(), failed))
    }

    /// Pair the submit awaiting a task with the one dispatch just minted.
    /// Exact: the slot names the job, so nothing is inferred from ordering.
    async fn bind_pending(&self, task_id: &str, owner_id: &str) {
        let Some(pending) = self.pending.lock().await.take() else {
            return;
        };
        let binding = Binding {
            task_id: task_id.to_string(),
            owner_id: owner_id.to_string(),
        };
        if let Some(job) = self
            .jobs
            .lock()
            .await
            .iter_mut()
            .find(|j| j.id == pending.job_id)
        {
            job.binding = Some(binding.clone());
        }
        // The waiter may already be gone (a child that failed before the task
        // was minted); its terminal state is still in `jobs` either way.
        let _ = pending.tx.send(binding);
    }

    /// Relay a store-side cancellation into a child kill.
    async fn cancel_by_task(&self, task_id: &str) {
        let jobs = self.jobs.lock().await;
        if let Some(job) = jobs.iter().find(|j| {
            j.outcome.is_none() && j.binding.as_ref().is_some_and(|b| b.task_id == task_id)
        }) {
            // `notify_one`, NOT `notify_waiters`: the latter wakes only waiters
            // ALREADY registered and stores no permit, and there is a real
            // window where none is. `run_child` registers on the first poll of
            // its `select!`, which is after `tokio::spawn` schedules it and
            // after the synchronous `cmd.spawn()` — while `bind_pending` has
            // already made the job cancellable the instant the handler
            // returned. A cancel landing in that window was silently dropped:
            // the store said `cancelled`, the trainer ran to completion, and
            // the two doors disagreed about one run. `notify_one` latches.
            job.cancel.notify_one();
        }
    }
}

/// A [`TaskStore`] decorator that watches the two lifecycle events pmcp drives
/// through the store, and forwards everything else untouched.
///
/// This is the pairing hook described in the crate doc. It deliberately
/// implements EVERY trait method rather than inheriting defaults: the SDK's
/// defaults are refusals (`set_result` answers "store does not support terminal
/// results"), so an unforwarded method would not be a passthrough — it would
/// silently disable the inner store's real implementation.
pub struct TrainingTaskStore {
    inner: Arc<dyn TaskStore>,
    registry: Arc<JobRegistry>,
}

impl TrainingTaskStore {
    /// Wrap `inner`, reporting creations and cancellations to `registry`.
    #[must_use]
    pub fn new(inner: Arc<dyn TaskStore>, registry: Arc<JobRegistry>) -> Self {
        Self { inner, registry }
    }
}

impl std::fmt::Debug for TrainingTaskStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrainingTaskStore").finish_non_exhaustive()
    }
}

#[async_trait]
impl TaskStore for TrainingTaskStore {
    // --- the two hooks -----------------------------------------------------

    async fn create(&self, owner_id: &str, ttl: Option<u64>) -> Result<Task, TaskStoreError> {
        let task = self.inner.create(owner_id, ttl).await?;
        self.registry.bind_pending(&task.task_id, owner_id).await;
        // The store only FILTERS expired records out of reads; it frees them
        // here. This is the process's one recurring event, so it is where the
        // sweep belongs — no timer, and no unbounded growth either.
        let _ = self.inner.cleanup_expired().await;
        Ok(task)
    }

    async fn cancel(&self, task_id: &str, owner_id: &str) -> Result<Task, TaskStoreError> {
        // The relay is NOT behind `?`. The store's verdict is about the RECORD
        // (Expired past `TASK_TTL_MS`, NotFound after a sweep, an already
        // terminal transition); the child is a separate fact, and there is no
        // other cancel road — a plain submit mints no task and `train_status`
        // is read-only. Short-circuiting left a CPU-saturating trainer alive
        // with `outcome: None`, so single-flight then refused every later
        // submit for the life of the process. `cancel_by_task` only ever
        // selects a RUNNING job whose binding names this exact task, so
        // relaying on the error path cannot kill someone else's run.
        let outcome = self.inner.cancel(task_id, owner_id).await;
        self.registry.cancel_by_task(task_id).await;
        outcome
    }

    // --- pure delegation ---------------------------------------------------

    async fn get(&self, task_id: &str, owner_id: &str) -> Result<Task, TaskStoreError> {
        self.inner.get(task_id, owner_id).await
    }

    async fn update_status(
        &self,
        task_id: &str,
        owner_id: &str,
        status: TaskStatus,
        message: Option<String>,
    ) -> Result<Task, TaskStoreError> {
        self.inner
            .update_status(task_id, owner_id, status, message)
            .await
    }

    async fn list(
        &self,
        owner_id: &str,
        cursor: Option<&str>,
    ) -> Result<(Vec<Task>, Option<String>), TaskStoreError> {
        self.inner.list(owner_id, cursor).await
    }

    async fn cleanup_expired(&self) -> Result<usize, TaskStoreError> {
        self.inner.cleanup_expired().await
    }

    fn config(&self) -> &StoreConfig {
        self.inner.config()
    }

    async fn set_result(
        &self,
        task_id: &str,
        owner_id: &str,
        result: CallToolResult,
    ) -> Result<(), TaskStoreError> {
        self.inner.set_result(task_id, owner_id, result).await
    }

    async fn get_result(
        &self,
        task_id: &str,
        owner_id: &str,
    ) -> Result<CallToolResult, TaskStoreError> {
        self.inner.get_result(task_id, owner_id).await
    }

    fn supports_results(&self) -> bool {
        self.inner.supports_results()
    }

    async fn deliver_task_inputs(
        &self,
        task_id: &str,
        owner_id: &str,
        responses: InputResponses,
    ) -> Result<TaskInputDelivery, TaskStoreError> {
        self.inner
            .deliver_task_inputs(task_id, owner_id, responses)
            .await
    }

    async fn task_input_snapshot(
        &self,
        task_id: &str,
        owner_id: &str,
    ) -> Result<TaskInputSnapshot, TaskStoreError> {
        self.inner.task_input_snapshot(task_id, owner_id).await
    }

    async fn record_input_requests(
        &self,
        task_id: &str,
        owner_id: &str,
        requests: InputRequests,
    ) -> Result<Task, TaskStoreError> {
        self.inner
            .record_input_requests(task_id, owner_id, requests)
            .await
    }

    async fn set_error(
        &self,
        task_id: &str,
        owner_id: &str,
        error: serde_json::Value,
    ) -> Result<(), TaskStoreError> {
        self.inner.set_error(task_id, owner_id, error).await
    }

    async fn get_error(
        &self,
        task_id: &str,
        owner_id: &str,
    ) -> Result<serde_json::Value, TaskStoreError> {
        self.inner.get_error(task_id, owner_id).await
    }

    fn supports_inputs(&self) -> bool {
        self.inner.supports_inputs()
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// The base `apr setfit train` invocation — every knob the operator owns.
/// One builder so the dry-run and the real run cannot drift.
fn train_command(
    paths: &TrainerPaths,
    config_path: &Path,
    artifact_path: &Path,
) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(&paths.apr_bin);
    cmd.arg("setfit")
        .arg("train")
        .arg("--config")
        .arg(config_path)
        .arg("--data")
        .arg(&paths.data)
        .arg("--selection")
        .arg(&paths.selection)
        .arg("--model-dir")
        .arg(&paths.model_dir)
        .arg("--output")
        .arg(artifact_path)
        .arg("--json")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A dying server must not orphan a CPU-saturating trainer. This is
        // also what makes cancellation-by-drop safe below.
        .kill_on_drop(true);
    cmd
}

/// `apr setfit train --json` writes ONE pretty-printed report and nothing else
/// to stdout (`report_json` uses `to_string_pretty`), so the whole buffer is
/// the document.
///
/// An earlier revision also scanned backwards for the last parseable LINE, to
/// tolerate "a stray leading line". That fallback could never fire — a
/// pretty-printed report's last line is `}` — so it was a bandaid attached to
/// no wound, and its unit test only ever exercised a compact form this CLI does
/// not emit. If stdout ever gains a second writer, this must fail loudly rather
/// than quietly find something else that parses.
fn parse_json_report(stdout: &[u8]) -> Option<serde_json::Value> {
    serde_json::from_slice(stdout).ok()
}

/// At most the last `max` bytes of a captured stream, on a char boundary.
fn tail(stream: &[u8], max: usize) -> String {
    let text = String::from_utf8_lossy(stream);
    let text = text.trim();
    if text.len() <= max {
        return text.to_string();
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &text[start..])
}

/// What a failing child said — BOTH streams.
///
/// Keeping only stderr is defect #2418, which `aprender-mcp`'s subprocess
/// module already paid for once: a failing `apr` can still have written its
/// `--json` report to stdout, and reporting only stderr threw it away.
fn failure_detail(stdout: &[u8], stderr: &[u8]) -> String {
    let err = tail(stderr, OUTPUT_TAIL_BYTES);
    let out = tail(stdout, OUTPUT_TAIL_BYTES);
    match (err.is_empty(), out.is_empty()) {
        (false, false) => format!("{err}\n--- stdout ---\n{out}"),
        (false, true) => err,
        (true, false) => out,
        (true, true) => "the trainer produced no output".to_string(),
    }
}

/// The CLI's own four request checks, run synchronously. `None` means the
/// request is accepted; `Some` is the refusal, in the CLI's words.
async fn preflight(
    paths: &TrainerPaths,
    config_path: &Path,
    artifact_path: &Path,
) -> Option<String> {
    let mut dry = train_command(paths, config_path, artifact_path);
    // No `.stdout(Stdio::null())` here: `Command::output()` re-pipes BOTH
    // streams unconditionally before spawning, so nulling stdout is a no-op
    // that only made the refusal below LOOK justified in throwing stdout away.
    dry.arg("--dry-run");
    match tokio::time::timeout(Duration::from_secs(DRY_RUN_TIMEOUT_SECS), dry.output()).await {
        Err(_) => Some(format!(
            "pre-flight (--dry-run) exceeded {DRY_RUN_TIMEOUT_SECS}s"
        )),
        Ok(Err(e)) => Some(format!("cannot spawn {}: {e}", paths.apr_bin.display())),
        // BOTH streams, via the same helper the real run uses. Reporting only
        // stderr is defect #2418 again, and it has a second bite here: a
        // non-zero exit with an empty stderr (a signal kill, an OOM) rendered
        // the refusal as the literal `training request refused: ` — no reason
        // at all. `failure_detail`'s (true, true) arm exists for exactly that.
        Ok(Ok(out)) if !out.status.success() => Some(failure_detail(&out.stdout, &out.stderr)),
        Ok(Ok(_)) => None,
    }
}

/// Admit, pre-flight, spawn, and return the submit handle.
///
/// The handle is [`StatusPayload`] plus the two keys pmcp's create gate needs:
/// a task-augmented client gets a store-minted task id back and polls
/// `tasks/get`; a plain client gets this payload as an ordinary result and
/// polls `train_status` with its `job_id`. All three doors serve one shape.
async fn submit_job(
    paths: Arc<TrainerPaths>,
    registry: Arc<JobRegistry>,
    store: Arc<dyn TaskStore>,
    config: serde_json::Value,
) -> pmcp::Result<serde_json::Value> {
    let admitted = registry
        .begin(&paths.output_dir)
        .await
        .map_err(pmcp::Error::validation)?;
    let job_id = admitted.job_id.clone();

    // The config travels as a FILE because the CLI is file-first — and the
    // file is kept beside the artifact, so a finished run's exact requested
    // config is always reconstructible.
    let config_path = paths.output_dir.join(format!("{job_id}.config.json"));
    let refusal = match serde_json::to_vec_pretty(&config) {
        Err(e) => Some(format!("config serialization: {e}")),
        Ok(bytes) if bytes.len() > MAX_CONFIG_BYTES => Some(format!(
            "config document is {} bytes, over the {MAX_CONFIG_BYTES}-byte transport \
             bound; the SetFit training config is twelve knobs, not a payload",
            bytes.len()
        )),
        Ok(bytes) => match tokio::fs::write(&config_path, &bytes).await {
            Err(e) => Some(format!("cannot write {}: {e}", config_path.display())),
            Ok(()) => preflight(&paths, &config_path, &admitted.artifact_path).await,
        },
    };
    if let Some(message) = refusal {
        registry
            .finish(&job_id, JobOutcome::Failed(message.clone()))
            .await;
        return Err(pmcp::Error::validation(format!(
            "training request refused: {message}"
        )));
    }

    // Snapshot the answer BEFORE the child can move the job, so the handle
    // this call returns always describes a running job.
    let handle = registry.submit_handle(&job_id).await.ok_or_else(|| {
        pmcp::Error::internal(format!("job {job_id} vanished before it answered"))
    })?;

    // The real run, supervised. The waiter owns the child end to end: pipes,
    // exit status, cancellation, the ONE `finish`, and the terminal write into
    // the store once it knows which task this submit became.
    let Admitted {
        artifact_path,
        cancel,
        binding,
        ..
    } = admitted;
    let waiter_job_id = job_id;
    tokio::spawn(async move {
        let outcome = run_child(&paths, &config_path, &artifact_path, &cancel).await;
        registry.finish(&waiter_job_id, outcome).await;
        // No task was minted for a plain call: the sender was dropped, this
        // resolves Err, and nothing is written to the store. Correct, and the
        // reason the old "adopt the oldest unbound job" pairing was wrong.
        let Ok(Ok(binding)) = tokio::time::timeout(BIND_GRACE, binding).await else {
            return;
        };
        let Some((payload, failed)) = registry.terminal_payload(&waiter_job_id).await else {
            return;
        };
        publish_terminal(store.as_ref(), &binding, &payload, failed).await;
    });

    Ok(handle)
}

/// Write a finished job's verdict onto its task. Every call is tolerated
/// rather than unwrapped: a task can expire, or already be `Cancelled` (whose
/// transition to `Completed` the store's state machine correctly refuses), and
/// the registry still holds the truth for `train_status` either way.
async fn publish_terminal(
    store: &dyn TaskStore,
    binding: &Binding,
    payload: &serde_json::Value,
    failed: bool,
) {
    let content = vec![Content::Text {
        text: payload.to_string(),
    }];
    let result = if failed {
        CallToolResult::error(content)
    } else {
        CallToolResult::new(content)
    };
    // Both writes are attempted unconditionally. An early return on a failed
    // `set_result` left the task in `working` FOREVER — a polling client never
    // stops — which is strictly worse than a terminal state with no result.
    // A store that legitimately refuses (an already-`Cancelled` record) ignores
    // both, and the registry still holds the truth for `train_status`.
    let _ = store
        .set_result(&binding.task_id, &binding.owner_id, result)
        .await;
    let _ = store
        .update_status(
            &binding.task_id,
            &binding.owner_id,
            TaskStatus::Completed,
            None,
        )
        .await;
}

/// Supervise one training child to its terminal state.
///
/// `wait_with_output` is what keeps both pipes draining while the child runs —
/// a child that fills a pipe nobody reads deadlocks, and this is tokio's
/// guarantee rather than a local invariant to re-derive. Cancellation drops
/// that future, and `kill_on_drop` reaps the child.
async fn run_child(
    paths: &TrainerPaths,
    config_path: &Path,
    artifact_path: &Path,
    cancel: &Notify,
) -> JobOutcome {
    let mut cmd = train_command(paths, config_path, artifact_path);
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            return JobOutcome::Failed(format!("cannot spawn {}: {e}", paths.apr_bin.display()))
        }
    };

    let output = tokio::select! {
        output = child.wait_with_output() => output,
        () = cancel.notified() => return JobOutcome::Cancelled,
    };

    match output {
        Ok(out) if out.status.success() => parse_json_report(&out.stdout).map_or_else(
            || {
                JobOutcome::Failed(
                    "the trainer exited 0 but printed no parseable --json report".to_string(),
                )
            },
            JobOutcome::Completed,
        ),
        Ok(out) => JobOutcome::Failed(format!(
            "the trainer exited with {}: {}",
            out.status,
            failure_detail(&out.stdout, &out.stderr)
        )),
        Err(e) => JobOutcome::Failed(format!("waiting on the trainer failed: {e}")),
    }
}

/// Assemble the server: two tools and the task store, nothing else.
///
/// `store` must be the [`TrainingTaskStore`] — it is both what pmcp dispatches
/// `tasks/*` through AND what pairs each submit with its task.
///
/// # Errors
///
/// `pmcp::Error` if the builder refuses the configuration.
pub fn build_server(
    paths: Arc<TrainerPaths>,
    registry: Arc<JobRegistry>,
    store: Arc<dyn TaskStore>,
    name: &str,
    version: &str,
) -> pmcp::Result<Server> {
    let train_paths = Arc::clone(&paths);
    let train_registry = Arc::clone(&registry);
    let train_store = Arc::clone(&store);
    // `TypedTool::new` derives the schema (with `$ref`s inlined) and
    // deserializes the arguments itself — the same pipeline
    // `tool_typed_with_description` uses for `train_status` below, so the two
    // tools cannot advertise schemas built different ways. The explicit
    // registration stays because only `TypedTool` carries `with_execution`.
    let train_tool = TypedTool::new(TOOL_TRAIN, move |args: TrainArgs, _extra| {
        let paths = Arc::clone(&train_paths);
        let registry = Arc::clone(&train_registry);
        let store = Arc::clone(&train_store);
        Box::pin(async move { submit_job(paths, registry, store, args.config).await })
    })
    .with_description(
        "Start a SetFit few-shot training run on this server's curated dataset and \
         encoder. Returns an MCP task handle (poll tasks/get) for task-augmented \
         calls, and a job_id for train_status either way. One job runs at a time.",
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Optional));

    let status_registry = Arc::clone(&registry);
    Server::builder()
        .name(name)
        .version(version)
        .capabilities(ServerCapabilities::tools_only())
        .tool(TOOL_TRAIN, train_tool)
        .tool_typed_with_description::<StatusArgs, _, _>(
            TOOL_STATUS,
            "Report a training job's phase, and on completion the trainer's full \
             --json report (artifact path + sha256, provenance, resolved config). \
             Without job_id: the most recent job.",
            move |args, _extra| {
                let registry = Arc::clone(&status_registry);
                async move {
                    registry
                        .status(args.job_id.as_deref())
                        .await
                        .ok_or_else(|| {
                            pmcp::Error::validation(match args.job_id {
                                Some(id) => format!("no job named {id} on this server"),
                                None => "no training job has been submitted yet".to_string(),
                            })
                        })
                }
            },
        )
        .task_store(store)
        .build()
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally
mod tests {
    use super::*;

    fn registry() -> JobRegistry {
        JobRegistry::default()
    }

    #[test]
    fn train_args_refuse_unknown_keys() {
        let err = serde_json::from_value::<TrainArgs>(serde_json::json!({
            "config": {},
            "shots": 8
        }))
        .expect_err("unknown key must be refused");
        assert!(err.to_string().contains("shots"), "{err}");
    }

    #[test]
    fn status_args_default_to_latest() {
        let args: StatusArgs = serde_json::from_value(serde_json::json!({}))
            .expect("empty status args are the latest-job query");
        assert!(args.job_id.is_none());
    }

    #[test]
    fn a_pretty_printed_report_is_the_whole_document() {
        let stdout = b"{\n  \"command\": \"setfit train\"\n}\n";
        let report = parse_json_report(stdout).expect("pretty report parses");
        assert_eq!(report["command"], "setfit train");
        assert!(
            parse_json_report(b"warming up\n{\"command\":\"x\"}\n").is_none(),
            "a second writer on stdout must fail loudly, not be scanned around"
        );
    }

    #[test]
    fn a_failure_keeps_both_streams() {
        let detail = failure_detail(b"{\"partial\":true}", b"error: refused");
        assert!(detail.contains("error: refused"), "{detail}");
        assert!(
            detail.contains("{\"partial\":true}"),
            "stdout must survive a failure (#2418): {detail}"
        );
        assert_eq!(failure_detail(b"", b""), "the trainer produced no output");
    }

    #[test]
    fn tail_clips_on_char_boundaries() {
        let clipped = tail("héllo wörld".repeat(400).as_bytes(), 64);
        assert!(clipped.starts_with('…'));
        assert!(clipped.len() <= 64 + '…'.len_utf8());
    }

    #[tokio::test]
    async fn single_flight_refuses_a_second_admit_and_names_the_first() {
        let registry = registry();
        let dir = std::env::temp_dir();
        let first = registry.begin(&dir).await.expect("first admit");
        let refusal = registry
            .begin(&dir)
            .await
            .expect_err("second admit refused");
        assert!(refusal.contains(&first.job_id), "{refusal}");
        registry
            .finish(&first.job_id, JobOutcome::Failed("test".into()))
            .await;
        registry.begin(&dir).await.expect("admit after terminal");
    }

    #[tokio::test]
    async fn binding_pairs_the_submit_that_is_actually_pending() {
        let registry = registry();
        let dir = std::env::temp_dir();
        let admitted = registry.begin(&dir).await.expect("admit");
        registry.bind_pending("task-a", "owner-1").await;
        let binding = admitted.binding.await.expect("the waiter is told its task");
        assert_eq!(binding.task_id, "task-a");
        assert_eq!(binding.owner_id, "owner-1");
        let status = registry.status(None).await.expect("status");
        assert_eq!(status["task_id"], "task-a", "the pairing is observable");
        // The slot is consumed: a second create cannot re-pair the same job.
        registry.bind_pending("task-b", "owner-1").await;
        assert_eq!(
            registry.status(None).await.expect("status")["task_id"],
            "task-a"
        );
    }

    #[tokio::test]
    async fn a_plain_call_leaves_no_binding_for_a_later_task_to_adopt() {
        let registry = registry();
        let dir = std::env::temp_dir();
        // A plain (non-task-augmented) submit: admitted, finished, never bound.
        let plain = registry.begin(&dir).await.expect("plain admit");
        registry
            .finish(
                &plain.job_id,
                JobOutcome::Completed(serde_json::json!({"a":1})),
            )
            .await;
        // A later task-augmented submit mints a task. It must bind to ITSELF.
        let augmented = registry.begin(&dir).await.expect("second admit");
        registry.bind_pending("task-late", "owner-1").await;
        let bound = augmented.binding.await.expect("bound");
        assert_eq!(bound.task_id, "task-late");
        let plain_status = registry.status(Some(&plain.job_id)).await.expect("plain");
        assert!(
            plain_status["task_id"].is_null(),
            "the plain job must NOT adopt a later call's task: {plain_status}"
        );
        let (payload, failed) = registry
            .terminal_payload(&plain.job_id)
            .await
            .expect("plain job is terminal");
        assert!(!failed);
        assert_eq!(payload["report"]["a"], 1, "each job keeps its own report");
    }

    #[tokio::test]
    async fn the_submit_handle_is_the_status_payload_plus_the_gate_keys() {
        let registry = registry();
        let dir = std::env::temp_dir();
        let job = registry.begin(&dir).await.expect("admit");
        let handle = registry
            .submit_handle(&job.job_id)
            .await
            .expect("handle for a live job");
        // The two keys pmcp's create gate requires, or no task is ever minted.
        assert_eq!(handle["taskId"], GATE_TASK_ID);
        assert_eq!(handle["status"], "working");
        assert_eq!(handle["ttl"], serde_json::json!(TASK_TTL_MS));
        // ...and the SAME payload train_status serves, so a plain client that
        // gets this back reads the identical shape it will later poll.
        let status = registry.status(Some(&job.job_id)).await.expect("status");
        for key in [
            "schema_version",
            "job_id",
            "task_id",
            "phase",
            "started_unix_ms",
            "artifact_path",
        ] {
            assert_eq!(
                handle[key], status[key],
                "handle and status differ at {key}"
            );
        }
        assert!(
            status.get("taskId").is_none() && status.get("ttl").is_none(),
            "the gate keys belong to the submit handle only: {status}"
        );
    }

    #[tokio::test]
    async fn status_reports_one_shape_for_every_phase() {
        let registry = registry();
        let dir = std::env::temp_dir();
        let job = registry.begin(&dir).await.expect("admit");
        let running = registry.status(None).await.expect("latest");
        assert_eq!(running["job_id"], serde_json::json!(job.job_id));
        assert_eq!(running["phase"], "running");
        assert_eq!(running["schema_version"], 1);
        assert!(running["report"].is_null() && running["error"].is_null());
        registry.finish(&job.job_id, JobOutcome::Cancelled).await;
        let cancelled = registry.status(Some(&job.job_id)).await.expect("by id");
        assert_eq!(cancelled["phase"], "cancelled");
        assert!(cancelled["error"]
            .as_str()
            .is_some_and(|e| e.contains("tasks/cancel")));
        assert!(registry.status(Some("job-nope")).await.is_none());
    }

    #[tokio::test]
    async fn cancel_only_fires_for_the_running_job_that_owns_the_task() {
        let registry = registry();
        let dir = std::env::temp_dir();
        let job = registry.begin(&dir).await.expect("admit");
        registry.bind_pending("task-a", "owner-1").await;
        // An unrelated task id must not kill this job; a notify with no
        // waiters is a no-op, so we assert on the selection instead.
        registry.cancel_by_task("task-other").await;
        assert_eq!(
            registry.status(None).await.expect("status")["phase"],
            "running"
        );
        registry.finish(&job.job_id, JobOutcome::Cancelled).await;
        // Terminal jobs are never selected for cancellation.
        registry.cancel_by_task("task-a").await;
        assert_eq!(
            registry.status(None).await.expect("status")["phase"],
            "cancelled"
        );
    }
}
