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
//! server SUPERVISES a pinned `apr` binary as a child process, and the binary
//! is PINNED (an explicit path, probed at startup), never resolved from `PATH`.
//!
//! # The task lifecycle follows chess-mcp's model
//!
//! [`task_store`] carries the full rationale; the shape here is its consumer:
//!
//! 1. the handler clears the handoff, resolves the owner, and MINTS the task
//!    itself via `mint_for_request` — so it holds the canonical id BEFORE it
//!    dispatches anything;
//! 2. the run's inputs go into the task's envelope, not into the dispatch
//!    payload;
//! 3. the work is dispatched and the handler returns a task-shaped `working`
//!    value; pmcp's create gate consults the armed handoff and returns THIS
//!    task rather than minting a second one;
//! 4. whoever finishes the work performs the terminal write
//!    ([`AprenderTaskStore::finish`]), guarded so a straggler cannot overwrite
//!    a verdict that already landed;
//! 5. a dispatch that fails compensates immediately, so a task never wedges in
//!    `working` waiting for work that was never started.
//!
//! Today "dispatch" is a spawned in-process waiter, which is what a long-lived
//! stdio server needs. The serverless deployment replaces step 3's dispatch
//! with a Step Functions execution and step 4's writer with a finalizer Lambda,
//! and swaps [`InMemoryTaskBackend`] for a DynamoDB backend behind the same
//! seam. Steps 1, 2 and 5 do not change — which is the point of doing it this
//! way now rather than later.
//!
//! # One job at a time
//!
//! [`RunningJobs`] refuses a second submit while one runs. This is a RESOURCE
//! policy about THIS process's CPU — training saturates it at ~4 GB RSS — so
//! per-process state is the CORRECT scope for it, unlike task state, which
//! must be durable and shared. It carries no correctness weight for the task
//! pairing: the handoff does that.
//!
//! # Fail on the REQUEST before blaming the run
//!
//! Submit runs `apr setfit train --dry-run` synchronously first — the CLI's own
//! pre-flight — so a bad request is refused at the MCP boundary in seconds
//! instead of surfacing minutes later as a failed job. Note the cost: pmcp
//! dispatches requests through a single worker
//! (`Server::spawn_request_worker` — "request handling stays serialized"), so
//! this window blocks every other request too, which is why the budget is
//! deliberately tight.

mod task_store;

pub use task_store::{
    AprenderTaskStore, BackendError, CancelSink, InMemoryTaskBackend, StoredTask, TaskBackend,
};

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use pmcp::server::typed_tool::TypedTool;
use pmcp::types::capabilities::ServerCapabilities;
use pmcp::types::{CallToolResult, Content, TaskStatus, TaskSupport, ToolExecution};
use pmcp::RequestHandlerExtra;
use pmcp::Server;
use tokio::sync::Notify;

pub use args::{StatusArgs, TrainArgs};

/// The server identity both the stdio runner and any transport wrapper report.
pub const SERVER_NAME: &str = "aprender-setfit-train";

/// The one training tool.
pub const TOOL_TRAIN: &str = "train";

/// The polling companion for clients without MCP Tasks support.
pub const TOOL_STATUS: &str = "train_status";

/// The owner bucket an UNAUTHENTICATED request binds to.
///
/// This mirrors pmcp's private `V1_UNAUTHENTICATED_OWNER`, and it has to: the
/// handler mints under this owner and pmcp's create gate looks the handoff up
/// under whatever ITS `resolve_owner` returned. If the two ever disagree, the
/// gate mints a second task and the client polls an id nobody updates — which
/// the E2E's task-id correlation assertion is what would catch. With an auth
/// provider configured both sides use the authenticated subject instead and
/// this constant stops mattering.
pub const UNAUTHENTICATED_OWNER: &str = "local";

/// TTL requested for minted training tasks: generous next to the measured
/// envelope (127 s wall for the 8-shot reference train on an M-series host),
/// because an expired task discards a finished artifact's result.
const TASK_TTL_MS: u64 = 3_600_000;

/// Budget for the synchronous `--dry-run` pre-flight — also the worst case for
/// how long this server can answer nothing else. Tight on purpose.
const DRY_RUN_TIMEOUT_SECS: u64 = 30;

/// Envelope version for every JSON payload this server emits.
const STATUS_SCHEMA_VERSION: u64 = 1;

/// Cap on each captured output stream in a failure message.
const OUTPUT_TAIL_BYTES: usize = 2_000;

mod args {
    // The JsonSchema derive expands serde_json::json!, which expands to
    // .unwrap() internally — kept this narrow so the ban covers everything else.
    #![allow(clippy::disallowed_methods)]

    use schemars::JsonSchema;
    use serde::Deserialize;

    /// Arguments for [`super::TOOL_TRAIN`].
    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct TrainArgs {
        /// The full twelve-knob SetFit training configuration, passed VERBATIM
        /// to `apr setfit train --config`. This server validates nothing about
        /// it on purpose: the CLI's single validating constructor is the one
        /// implementation of config legality, and a second validator here could
        /// only drift from it. Its SIZE is bounded (see `MAX_CONFIG_BYTES`) —
        /// that is a transport bound, a different question from legality.
        pub config: serde_json::Value,
    }

    /// Arguments for [`super::TOOL_STATUS`].
    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct StatusArgs {
        /// The `task_id` a `train` call returned.
        pub task_id: String,
    }
}

/// The bound this reading surface owes on a client-supplied document, mirroring
/// the predict sibling's `MAX_REQUEST_BODY_BYTES`. `config` is a
/// `serde_json::Value`, so `deny_unknown_fields` cannot reach inside it and
/// nothing else would stop a caller handing us a 500 MB object to parse, copy
/// and write to disk.
pub const MAX_CONFIG_BYTES: usize = 1_048_576;

/// Everything the operator provisions; nothing here comes from the client.
///
/// The client varies the training CONFIG; the server owns which dataset, which
/// selection, which encoder checkout and where artifacts land — the thin-server
/// philosophy applied to training.
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
    /// STARTUP"; the likeliest missing piece was the one it did not look for.
    /// `--help` is free — no model load, no I/O.
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
                 `setfit train --help` ({}): build it with `--features setfit` — setfit is NOT \
                 in apr-cli's default feature set, so a plain `cargo build --release` produces \
                 an apr this server cannot use.\n{}",
                self.apr_bin.display(),
                out.status,
                tail(&out.stderr, OUTPUT_TAIL_BYTES)
            )),
            Ok(_) => Ok(()),
        }
    }
}

/// The one training run this process will admit at a time, and the handle that
/// stops it.
#[derive(Debug)]
struct RunningJob {
    task_id: String,
    cancel: Arc<Notify>,
}

/// Single-flight admission plus the cancel relay.
///
/// Per-process by design: it is about this container's CPU, not about task
/// state. A second container admitting its own job is correct behaviour, not a
/// bug — which is exactly why task state lives in the store instead.
#[derive(Debug, Default)]
pub struct RunningJobs {
    current: Mutex<Option<RunningJob>>,
}

impl RunningJobs {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn guard(&self) -> MutexGuard<'_, Option<RunningJob>> {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Admit `task_id`, or refuse naming the job already running.
    fn try_admit(&self, task_id: &str) -> Result<Arc<Notify>, String> {
        let mut current = self.guard();
        if let Some(running) = current.as_ref() {
            return Err(format!(
                "a training job is already running ({}); this server trains one model at a \
                 time — poll `{TOOL_STATUS}` and resubmit when it finishes",
                running.task_id
            ));
        }
        let cancel = Arc::new(Notify::new());
        *current = Some(RunningJob {
            task_id: task_id.to_string(),
            cancel: Arc::clone(&cancel),
        });
        Ok(cancel)
    }

    /// Release the slot, but only if `task_id` still owns it.
    fn release(&self, task_id: &str) {
        let mut current = self.guard();
        if current.as_ref().is_some_and(|r| r.task_id == task_id) {
            *current = None;
        }
    }
}

impl CancelSink for RunningJobs {
    fn cancel(&self, task_id: &str) {
        let current = self.guard();
        if let Some(running) = current.as_ref().filter(|r| r.task_id == task_id) {
            // `notify_one`, NOT `notify_waiters`: the latter wakes only waiters
            // already registered and stores no permit, and `run_child` registers
            // on the first poll of its `select!` — after the spawn is scheduled.
            // A cancel landing in that window would be silently dropped.
            running.cancel.notify_one();
        }
    }
}

/// The owner this request's task belongs to.
///
/// Derived from the request rather than assumed, because the owner is the
/// output of a per-era decision, not a constant: the authenticated subject when
/// there is one, [`UNAUTHENTICATED_OWNER`] otherwise. Assuming a constant is
/// how a store silently matches nothing the moment a client authenticates.
fn resolve_owner(extra: &RequestHandlerExtra) -> String {
    extra.auth_context().map_or_else(
        || UNAUTHENTICATED_OWNER.to_string(),
        |ctx| ctx.subject.clone(),
    )
}

/// The base `apr setfit train` invocation. One builder, so the dry run and the
/// real run cannot drift.
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
        // A dying server must not orphan a CPU-saturating trainer. This is also
        // what makes cancellation-by-drop safe.
        .kill_on_drop(true);
    cmd
}

/// `apr setfit train --json` writes ONE pretty-printed report and nothing else
/// to stdout, so the whole buffer is the document. If stdout ever gains a
/// second writer this must fail loudly rather than quietly find something else
/// that parses.
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

/// What a failing child said — BOTH streams. Keeping only stderr is defect
/// #2418, which `aprender-mcp`'s subprocess module already paid for once: a
/// failing `apr` can still have written its `--json` report to stdout.
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

/// A job's terminal verdict.
#[derive(Debug)]
enum Outcome {
    Completed(serde_json::Value),
    Failed(String),
    Cancelled,
}

impl Outcome {
    const fn phase(&self) -> &'static str {
        match self {
            Self::Completed(_) => "completed",
            Self::Failed(_) => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    /// The MCP task status a verdict maps onto. `Failed` is a real status the
    /// SDK's state machine accepts, so a task client can tell a failed run from
    /// a successful one by STATUS and need not parse the payload.
    const fn task_status(&self) -> TaskStatus {
        match self {
            Self::Completed(_) => TaskStatus::Completed,
            Self::Failed(_) => TaskStatus::Failed,
            Self::Cancelled => TaskStatus::Cancelled,
        }
    }
}

/// The ONE status shape every surface serves — `train_status`'s result and the
/// terminal task result alike — so no two doors can tell different stories.
fn run_payload(
    task_id: &str,
    artifact_path: &str,
    phase: &str,
    report: Option<&serde_json::Value>,
    error: Option<&str>,
) -> serde_json::Value {
    #[derive(serde::Serialize)]
    struct Payload<'a> {
        schema_version: u64,
        task_id: &'a str,
        phase: &'a str,
        artifact_path: &'a str,
        report: Option<&'a serde_json::Value>,
        error: Option<&'a str>,
    }
    serde_json::to_value(Payload {
        schema_version: STATUS_SCHEMA_VERSION,
        task_id,
        phase,
        artifact_path,
        report,
        error,
    })
    .unwrap_or(serde_json::Value::Null)
}

/// What a run needs handed to whoever finishes it. Today the in-process waiter
/// already holds these; it is written anyway because the serverless finalizer
/// will have nothing else — and a side channel that only exists on the path
/// that does not need it would be dead on the path that does.
fn run_envelope(config_path: &str, artifact_path: &str) -> serde_json::Value {
    #[derive(serde::Serialize)]
    struct Envelope<'a> {
        config_path: &'a str,
        artifact_path: &'a str,
    }
    serde_json::to_value(Envelope {
        config_path,
        artifact_path,
    })
    .unwrap_or(serde_json::Value::Null)
}

fn terminal_result(payload: &serde_json::Value, failed: bool) -> CallToolResult {
    let content = vec![Content::Text {
        text: payload.to_string(),
    }];
    if failed {
        CallToolResult::error(content)
    } else {
        CallToolResult::new(content)
    }
}

/// The CLI's own request checks, run synchronously. `None` means accepted.
async fn preflight(
    paths: &TrainerPaths,
    config_path: &Path,
    artifact_path: &Path,
) -> Option<String> {
    let mut dry = train_command(paths, config_path, artifact_path);
    dry.arg("--dry-run");
    match tokio::time::timeout(Duration::from_secs(DRY_RUN_TIMEOUT_SECS), dry.output()).await {
        Err(_) => Some(format!(
            "pre-flight (--dry-run) exceeded {DRY_RUN_TIMEOUT_SECS}s"
        )),
        Ok(Err(e)) => Some(format!("cannot spawn {}: {e}", paths.apr_bin.display())),
        // BOTH streams: `Command::output()` re-pipes stdout regardless of what
        // the builder asked for, and a signal kill leaves stderr empty — which
        // rendered as a refusal with no reason at all.
        Ok(Ok(out)) if !out.status.success() => Some(failure_detail(&out.stdout, &out.stderr)),
        Ok(Ok(_)) => None,
    }
}

/// Supervise one training child to its terminal state.
///
/// `wait_with_output` keeps both pipes draining while the child runs — a child
/// that fills a pipe nobody reads deadlocks, and this makes that tokio's
/// guarantee rather than a local invariant. Cancellation drops that future and
/// `kill_on_drop` reaps the child.
async fn run_child(
    paths: &TrainerPaths,
    config_path: &Path,
    artifact_path: &Path,
    cancel: &Notify,
) -> Outcome {
    let mut cmd = train_command(paths, config_path, artifact_path);
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => return Outcome::Failed(format!("cannot spawn {}: {e}", paths.apr_bin.display())),
    };
    let output = tokio::select! {
        output = child.wait_with_output() => output,
        () = cancel.notified() => return Outcome::Cancelled,
    };
    match output {
        Ok(out) if out.status.success() => parse_json_report(&out.stdout).map_or_else(
            || {
                Outcome::Failed(
                    "the trainer exited 0 but printed no parseable --json report".to_string(),
                )
            },
            Outcome::Completed,
        ),
        Ok(out) => Outcome::Failed(format!(
            "the trainer exited with {}: {}",
            out.status,
            failure_detail(&out.stdout, &out.stderr)
        )),
        Err(e) => Outcome::Failed(format!("waiting on the trainer failed: {e}")),
    }
}

/// Turn a dispatch failure into a FAILED task, then report it to the caller.
///
/// A task minted for work that was never started must never be left `working`:
/// the client would poll a task nobody will ever finish. Mirrors chess's
/// `compensate_dispatch_failure`.
async fn compensate(
    store: &AprenderTaskStore,
    task_id: &str,
    owner: &str,
    artifact_path: &str,
    reason: String,
) -> pmcp::Error {
    let payload = run_payload(task_id, artifact_path, "failed", None, Some(&reason));
    let _ = store
        .finish(
            task_id,
            owner,
            TaskStatus::Failed,
            terminal_result(&payload, true),
        )
        .await;
    pmcp::Error::validation(format!("training request refused: {reason}"))
}

/// Mint, stash, dispatch, answer — the handler.
async fn submit(
    paths: Arc<TrainerPaths>,
    store: Arc<AprenderTaskStore>,
    running: Arc<RunningJobs>,
    extra: &RequestHandlerExtra,
    config: serde_json::Value,
) -> pmcp::Result<serde_json::Value> {
    // (a) Drop any arm left by an earlier call that never opened the create
    // gate — a plain, non-task-augmented submit does exactly that.
    store.clear_handoff();

    // (b) The transport bound, before anything is parsed further or written.
    let config_bytes = serde_json::to_vec_pretty(&config)
        .map_err(|e| pmcp::Error::internal(format!("config serialization: {e}")))?;
    if config_bytes.len() > MAX_CONFIG_BYTES {
        return Err(pmcp::Error::validation(format!(
            "config is {} bytes; this server accepts at most {MAX_CONFIG_BYTES}",
            config_bytes.len()
        )));
    }

    let owner = resolve_owner(extra);

    // (c) Mint FIRST: the id is what the artifact path, the envelope and any
    // out-of-band dispatch are all keyed on.
    let task = store
        .mint_for_request(&owner, Some(TASK_TTL_MS))
        .await
        .map_err(|e| pmcp::Error::internal(format!("cannot mint a training task: {e}")))?;
    let task_id = task.task_id.clone();
    let artifact_path = paths.output_dir.join(format!("{task_id}.apr"));
    let artifact_display = artifact_path.display().to_string();

    // (d) Single-flight. Refused here rather than before the mint so the
    // refusal names the RUNNING job and this task compensates cleanly.
    let cancel = match running.try_admit(&task_id) {
        Ok(cancel) => cancel,
        Err(message) => {
            return Err(compensate(&store, &task_id, &owner, &artifact_display, message).await)
        }
    };

    // (e) The config travels as a FILE because the CLI is file-first, and is
    // kept beside the artifact so a finished run is reconstructible. The
    // envelope carries the same inputs for whoever finishes the work — the
    // finalizer, once this dispatch is a state machine.
    let config_path = paths.output_dir.join(format!("{task_id}.config.json"));
    if let Err(e) = tokio::fs::write(&config_path, &config_bytes).await {
        running.release(&task_id);
        let message = format!("cannot write {}: {e}", config_path.display());
        return Err(compensate(&store, &task_id, &owner, &artifact_display, message).await);
    }
    if let Err(e) = store
        .put_envelope(
            &task_id,
            &owner,
            run_envelope(&config_path.display().to_string(), &artifact_display),
        )
        .await
    {
        running.release(&task_id);
        let message = format!("cannot record the run envelope: {e}");
        return Err(compensate(&store, &task_id, &owner, &artifact_display, message).await);
    }

    // (f) Pre-flight: the CLI's own four request checks, in its own words.
    if let Some(reason) = preflight(&paths, &config_path, &artifact_path).await {
        running.release(&task_id);
        return Err(compensate(&store, &task_id, &owner, &artifact_display, reason).await);
    }

    // (g) Dispatch. In-process today; a Step Functions execution in the
    // serverless deployment. Either way the task id is already known, which is
    // the whole point of minting in the handler.
    let waiter = Arc::clone(&paths);
    let waiter_store = Arc::clone(&store);
    let waiter_running = Arc::clone(&running);
    let waiter_task = task_id.clone();
    let waiter_owner = owner.clone();
    let waiter_artifact = artifact_display.clone();
    tokio::spawn(async move {
        let outcome = run_child(&waiter, &config_path, &artifact_path, &cancel).await;
        waiter_running.release(&waiter_task);
        let (report, error) = match &outcome {
            Outcome::Completed(report) => (Some(report), None),
            Outcome::Failed(message) => (None, Some(message.as_str())),
            Outcome::Cancelled => (None, Some("cancelled by the client (tasks/cancel)")),
        };
        let payload = run_payload(
            &waiter_task,
            &waiter_artifact,
            outcome.phase(),
            report,
            error,
        );
        let failed = !matches!(outcome, Outcome::Completed(_));
        // Tolerated, not unwrapped: the task can have expired or already been
        // cancelled, and `finish` deliberately no-ops on a second terminal write.
        let _ = waiter_store
            .finish(
                &waiter_task,
                &waiter_owner,
                outcome.task_status(),
                terminal_result(&payload, failed),
            )
            .await;
    });

    // (h) The task-shaped answer. `taskId` + `status` are what open pmcp's
    // create gate; the armed handoff is what makes the store return THIS task
    // instead of minting a second one. A plain call gets this value verbatim
    // and can poll `train_status` with the same id.
    let mut value = run_payload(&task_id, &artifact_display, "working", None, None);
    if let Some(object) = value.as_object_mut() {
        object.insert("taskId".to_string(), serde_json::Value::String(task_id));
        object.insert(
            "status".to_string(),
            serde_json::Value::String("working".to_string()),
        );
        object.insert("ttl".to_string(), serde_json::Value::from(TASK_TTL_MS));
    }
    Ok(value)
}

/// Read a run's status back out of the task store.
async fn status(
    store: &AprenderTaskStore,
    extra: &RequestHandlerExtra,
    task_id: &str,
) -> pmcp::Result<serde_json::Value> {
    use pmcp::server::task_store::TaskStore;
    let owner = resolve_owner(extra);
    let task = store
        .get(task_id, &owner)
        .await
        .map_err(|e| pmcp::Error::validation(format!("no task {task_id} on this server: {e}")))?;
    // A terminal task's payload is the result the worker wrote — one shape,
    // both doors. A working task has no result yet, so synthesize the same
    // shape from the envelope, which holds the artifact path.
    if let Ok(result) = store.get_result(task_id, &owner).await {
        if let Some(Content::Text { text }) = result.content.first() {
            if let Ok(payload) = serde_json::from_str::<serde_json::Value>(text) {
                return Ok(payload);
            }
        }
    }
    let artifact = store
        .get_envelope(task_id, &owner)
        .await
        .ok()
        .flatten()
        .and_then(|envelope| {
            envelope
                .get("artifact_path")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();
    let phase = if task.status.is_terminal() {
        // Terminal with no readable result: report the status honestly rather
        // than claiming `working` forever.
        format!("{:?}", task.status).to_lowercase()
    } else {
        "working".to_string()
    };
    Ok(run_payload(task_id, &artifact, &phase, None, None))
}

/// Assemble the server: two tools and the task store, nothing else.
///
/// # Errors
///
/// `pmcp::Error` if the builder refuses the configuration.
pub fn build_server(
    paths: Arc<TrainerPaths>,
    store: Arc<AprenderTaskStore>,
    running: Arc<RunningJobs>,
    name: &str,
    version: &str,
) -> pmcp::Result<Server> {
    let train_paths = Arc::clone(&paths);
    let train_store = Arc::clone(&store);
    let train_running = Arc::clone(&running);
    // `TypedTool::new` derives the schema (with `$ref`s inlined) and
    // deserializes the arguments itself — the same pipeline
    // `tool_typed_with_description` uses for `train_status`, so the two tools
    // cannot advertise schemas built different ways. The explicit registration
    // stays because only `TypedTool` carries `with_execution`.
    let train_tool = TypedTool::new(TOOL_TRAIN, move |args: TrainArgs, extra| {
        let paths = Arc::clone(&train_paths);
        let store = Arc::clone(&train_store);
        let running = Arc::clone(&train_running);
        Box::pin(async move { submit(paths, store, running, &extra, args.config).await })
    })
    .with_description(
        "Start a SetFit few-shot training run on this server's curated dataset and \
         encoder. Returns an MCP task: poll tasks/get for status and tasks/result \
         for the trainer's report, or poll train_status with the same task_id. One \
         job runs at a time.",
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Optional));

    let status_store = Arc::clone(&store);
    let server = Server::builder()
        .name(name)
        .version(version)
        .capabilities(ServerCapabilities::tools_only())
        .tool(TOOL_TRAIN, train_tool)
        .tool_typed_with_description::<StatusArgs, _, _>(
            TOOL_STATUS,
            "Report a training task's phase, and on completion the trainer's full \
             --json report (artifact path + sha256, provenance, resolved config). \
             Takes the task_id that `train` returned.",
            move |args: StatusArgs, extra| {
                let store = Arc::clone(&status_store);
                async move { status(&store, &extra, &args.task_id).await }
            },
        )
        .task_store(store)
        .build()?;
    Ok(server)
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally
mod tests {
    use super::*;

    #[test]
    fn train_args_refuse_unknown_keys() {
        let err =
            serde_json::from_value::<TrainArgs>(serde_json::json!({ "config": {}, "shots": 8 }))
                .expect_err("unknown key must be refused");
        assert!(err.to_string().contains("shots"), "{err}");
    }

    #[test]
    fn status_args_require_a_task_id() {
        serde_json::from_value::<StatusArgs>(serde_json::json!({}))
            .expect_err("task_id is not optional — there is no 'latest' across containers");
    }

    #[test]
    fn a_pretty_printed_report_is_the_whole_document() {
        let report = parse_json_report(b"{\n  \"command\": \"setfit train\"\n}\n")
            .expect("pretty report parses");
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

    #[test]
    fn single_flight_refuses_a_second_admit_and_names_the_first() {
        let running = RunningJobs::new();
        running.try_admit("task-a").expect("first admit");
        let refusal = running.try_admit("task-b").expect_err("second refused");
        assert!(refusal.contains("task-a"), "{refusal}");
        running.release("task-a");
        running.try_admit("task-b").expect("admit after release");
    }

    #[test]
    fn release_only_frees_the_slot_its_own_task_holds() {
        let running = RunningJobs::new();
        running.try_admit("task-a").expect("admit");
        // A straggler from a previous run must not free the current job's slot.
        running.release("task-stale");
        assert!(
            running.try_admit("task-b").is_err(),
            "task-a still holds the slot"
        );
        running.release("task-a");
        running.try_admit("task-b").expect("now free");
    }

    #[test]
    fn every_verdict_maps_to_a_distinct_task_status() {
        assert_eq!(
            Outcome::Completed(serde_json::Value::Null).task_status(),
            TaskStatus::Completed
        );
        assert_eq!(
            Outcome::Failed(String::new()).task_status(),
            TaskStatus::Failed,
            "a failed run must be distinguishable by STATUS, not only by payload"
        );
        assert_eq!(Outcome::Cancelled.task_status(), TaskStatus::Cancelled);
    }

    #[test]
    fn the_payload_shape_is_one_shape() {
        let report = serde_json::json!({ "artifact_sha256": "ab" });
        let done = run_payload("t-1", "/tmp/x.apr", "completed", Some(&report), None);
        assert_eq!(done["schema_version"], 1);
        assert_eq!(done["task_id"], "t-1");
        assert_eq!(done["report"]["artifact_sha256"], "ab");
        assert!(done["error"].is_null());
        let working = run_payload("t-1", "/tmp/x.apr", "working", None, None);
        // Same keys, whichever door serves it.
        let (mut a, mut b): (Vec<_>, Vec<_>) = (
            done.as_object().expect("obj").keys().collect(),
            working.as_object().expect("obj").keys().collect(),
        );
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b);
    }

    #[test]
    fn a_failed_run_is_an_error_result() {
        let payload = run_payload("t-1", "/tmp/x.apr", "failed", None, Some("boom"));
        assert!(terminal_result(&payload, true).is_error);
        assert!(!terminal_result(&payload, false).is_error);
    }
}
