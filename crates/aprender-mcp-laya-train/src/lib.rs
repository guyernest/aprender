//! The Laya training MCP server's library: three tools, MCP Tasks, a single-flight
//! lease and the pure finalize decision.
//!
//! This crate is transport and bookkeeping. It never trains anything, never
//! recomputes a metric, a gate clause or a probability, and never talks to AWS:
//! those live behind the [`Dispatcher`], [`DatasetSource`] and
//! [`TaskBackend`] seams, with local implementations here and the AWS ones in
//! `aprender-mcp-laya-train-lambda` (plan 14-04). The task store, the in-memory
//! backend and the mint handoff are the SetFit training server's, reused rather
//! than copied (OPS-03).
//!
//! # The tools
//!
//! | Tool | What it does |
//! |------|--------------|
//! | `validate_dataset` | INSPECTS a dataset prefix: every data rule, plus whether `train` will need `epochs`. Never refuses for their absence (D-21). |
//! | `train` | Validates for submission, takes the single-flight lease, mints an MCP task and dispatches the run. Takes exactly `dataset_uri`, `name` and an optional `epochs` (D-06). |
//! | `train_status` | The one status payload, with download links minted at read time. |
//!
//! # One context, one backend
//!
//! [`ServerCtx::new`] builds the task store from the SAME `Arc<dyn TaskBackend>`
//! it keeps as `ctx.backend`, because the store's backend field is private and
//! the single-flight lock must address the same records. The store and the lock
//! can therefore never point at two backends.
//!
//! # The submit order
//!
//! (a) clear the handoff; (b) guard the arguments; (c) fetch and validate the
//! dataset; (d) resolve the owner; (e) pre-check the lock; (f) mint; (g) acquire
//! the lock, compensating the minted task on a lost race; (h) stash the handles
//! in the envelope; (i) dispatch, treating an AMBIGUOUS start as "maybe started"
//! and releasing the lock only on a confirmed non-start; (j) answer `working`.

pub mod finalize;
pub mod layout;
pub mod lock;
pub mod validate;

pub use aprender_mcp_setfit_train::{
    mint_id, AprenderTaskStore, BackendError, CancelSink, InMemoryTaskBackend, StoredTask,
    TaskBackend,
};

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use pmcp::server::task_store::TaskStore;
use pmcp::server::typed_tool::TypedTool;
use pmcp::types::capabilities::ServerCapabilities;
use pmcp::types::{Content, TaskStatus, TaskSupport, ToolExecution};
use pmcp::RequestHandlerExtra;
use pmcp::Server;
use serde::{Deserialize, Serialize};

use crate::finalize::payload_result;
use crate::layout::{
    inputs_string, run_prefix, ExecutionInput, Phase, StatusPayload, MAX_REQUEST_BYTES,
    SCHEMA_EXECUTION_INPUT, SERVER_NAME, TASK_TTL_MS, TOOL_STATUS, TOOL_TRAIN, TOOL_VALIDATE,
};
use crate::lock::{now_secs, AcquireError, LOCK_OWNER};
use crate::validate::{
    validate_files, validate_for_submit, DatasetFiles, Refusal, ValidationPolicy,
};

pub use args::{EpochRule, StatusArgs, TrainArgs, ValidateArgs};

/// The owner an UNAUTHENTICATED request binds to when the transport supplies no
/// auth context at all (stdio). Mirrors pmcp's private `V1_UNAUTHENTICATED_OWNER`
/// and the SetFit server's constant: the handler mints under this owner and
/// pmcp's create gate looks the handoff up under whatever its own owner
/// resolution returned, so the two must agree.
pub const UNAUTHENTICATED_OWNER: &str = "local";

pub(crate) mod args {
    // The JsonSchema derive expands serde_json::json!, which expands to
    // .unwrap() internally; kept this narrow so the ban covers everything else.
    #![allow(clippy::disallowed_methods)]

    use schemars::JsonSchema;
    use serde::Deserialize;

    use crate::layout::DATASETS_PREFIX;
    use crate::validate::Refusal;

    /// Shots per class at or below which the recipe fixes the epoch count.
    pub const EPOCH_THRESHOLD: u64 = 16;

    /// The epoch rule, read from the gate contract's `recipe` block by
    /// `ValidationPolicy::from_contract` (never literals here).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct EpochRule {
        /// `epochs_at_most_16_per_class`.
        pub fixed: u32,
        /// `epochs_above_16_min`.
        pub lo: u32,
        /// `epochs_above_16_max`.
        pub hi: u32,
    }

    /// Arguments for `train`: exactly three knobs, nothing else (D-06). Seeds,
    /// stopping and the gate bars stay contract-owned.
    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct TrainArgs {
        /// `s3://<this server's bucket>/datasets/<name>[/<name>...]/`, a prefix
        /// holding `task.json`, `train.jsonl` and `eval.jsonl` (and optionally
        /// `shift.jsonl`).
        pub dataset_uri: String,
        /// The model's name: lower-case letters, digits and hyphens, 1 to 48
        /// characters, not starting with a hyphen. Every output is keyed by it.
        pub name: String,
        /// Training epochs. Accepted only above 16 shots per class, in the
        /// contract's range, and then required; at 16 shots per class or fewer
        /// the recipe fixes it and passing one is refused.
        #[serde(default)]
        pub epochs: Option<u32>,
    }

    /// Arguments for `validate_dataset`: only the dataset. It has no `epochs`
    /// argument, so it can never apply the epochs refusal (D-21).
    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct ValidateArgs {
        /// `s3://<this server's bucket>/datasets/<name>[/<name>...]/`.
        pub dataset_uri: String,
    }

    /// Arguments for `train_status`.
    #[derive(Debug, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct StatusArgs {
        /// The `taskId` a `train` call returned.
        pub task_id: String,
    }

    /// The epoch count to hand the trainer, per `recipe.epoch_rule` and the
    /// trainer's own wording (`scripts/laya_train/contract.py` `resolve_epochs`).
    ///
    /// `None` at 16 shots per class or fewer, where the recipe fixes the count and
    /// a caller's `epochs` is REFUSED rather than ignored; above 16 the caller's
    /// value, which is required and must lie in `[lo, hi]`.
    pub fn resolve_epochs(
        shots_per_class: u64,
        epochs: Option<u32>,
        rule: &EpochRule,
    ) -> Result<Option<u32>, Refusal> {
        let EpochRule { fixed, lo, hi } = *rule;
        if shots_per_class <= EPOCH_THRESHOLD {
            return match epochs {
                None => Ok(None),
                Some(e) => Err(Refusal::new(
                    "epochs",
                    format!(
                        "at <= {EPOCH_THRESHOLD} shots/class the recipe fixes epochs to {fixed}; \
                         epochs {e} is not accepted"
                    ),
                )),
            };
        }
        match epochs {
            None => Err(Refusal::new(
                "epochs",
                format!(
                    "above {EPOCH_THRESHOLD} shots/class ({shots_per_class}) epochs is required, \
                     in [{lo}, {hi}]"
                ),
            )),
            Some(e) if !(lo..=hi).contains(&e) => Err(Refusal::new(
                "epochs",
                format!("epochs {e} is outside [{lo}, {hi}] at {shots_per_class} shots/class"),
            )),
            Some(e) => Ok(Some(e)),
        }
    }

    /// `^[a-z0-9][a-z0-9-]{0,47}$`, as a byte loop (no regex dependency): the name
    /// becomes part of every output key (D-09, T-14-01).
    pub fn check_name(name: &str) -> Result<(), Refusal> {
        let bytes = name.as_bytes();
        let first_ok = bytes
            .first()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        let rest_ok = bytes
            .iter()
            .skip(1)
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-');
        if first_ok && rest_ok && bytes.len() <= 48 {
            Ok(())
        } else {
            Err(Refusal::new(
                "name",
                "name must match ^[a-z0-9][a-z0-9-]{0,47}$: lower-case letters, digits and \
                 hyphens, 1 to 48 characters, not starting with a hyphen",
            ))
        }
    }

    /// The normalized prefix `datasets/<segs>/` for `uri`, which must be
    /// `s3://<bucket>/datasets/<seg>[/<seg>]{0,3}[/]` (D-07, T-14-02): the server's
    /// own bucket, the one datasets prefix, 1 to 4 well-formed segments, and no `%`.
    pub fn parse_dataset_uri(uri: &str, bucket: &str) -> Result<String, Refusal> {
        let refuse = || {
            Refusal::new(
                "dataset-uri",
                format!(
                    "dataset_uri must be s3://{bucket}/{DATASETS_PREFIX}<name>[/<name>...]/ with 1 \
                     to 4 segments of [a-z0-9._-], each starting with a letter or digit and at \
                     most 64 characters"
                ),
            )
        };
        if uri.contains('%') {
            return Err(refuse());
        }
        let rest = uri.strip_prefix("s3://").ok_or_else(refuse)?;
        let (uri_bucket, path) = rest.split_once('/').ok_or_else(refuse)?;
        if uri_bucket != bucket {
            return Err(refuse());
        }
        let segments = path.strip_prefix(DATASETS_PREFIX).ok_or_else(refuse)?;
        let segments = segments.strip_suffix('/').unwrap_or(segments);
        let parts: Vec<&str> = segments.split('/').collect();
        if parts.is_empty() || parts.len() > 4 || !parts.iter().all(|p| crate::layout::segment_ok(p)) {
            return Err(refuse());
        }
        Ok(format!("{DATASETS_PREFIX}{}/", parts.join("/")))
    }
}

/// Why a dispatch did not start a run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DispatchError {
    /// The start provably did not happen (a validation or authorization failure
    /// before any execution existed): safe to release the lock and compensate.
    #[error("dispatch rejected: {0}")]
    Rejected(String),
    /// The start MAY have happened (a timeout, a 5xx, a lost response): the lock
    /// must not be released until [`Dispatcher::confirm`] says it did not.
    #[error("dispatch ambiguous: {0}")]
    Ambiguous(String),
}

/// What [`Dispatcher::confirm`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirm {
    /// An execution with the deterministic name exists.
    Started,
    /// None exists.
    NotStarted,
}

/// What a run needs handed to whoever executes it. Handles and hashes only:
/// never dataset content, a token or a URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSpec {
    /// The minted task id (also the deterministic execution name).
    pub task_id: String,
    /// The caller's auth subject.
    pub owner: String,
    /// The validated model name.
    pub name: String,
    /// The normalized dataset key prefix, `datasets/<segs>/`.
    pub dataset_prefix: String,
    /// The run key prefix, `runs/<name>/<task_id>/`.
    pub run_prefix: String,
    /// The caller's epochs when the recipe takes one (above 16 shots per class).
    pub epochs: Option<u32>,
    /// File name to sha256 hex of the bytes the pre-filter validated.
    pub inputs_sha256: BTreeMap<String, String>,
    /// sha256 (64 hex) of the embedded gate contract that judged the pre-filter.
    pub gate_contract_sha256: String,
}

impl RunSpec {
    /// The Step Functions execution input for this run.
    #[must_use]
    pub fn execution_input(&self) -> ExecutionInput {
        ExecutionInput {
            schema: SCHEMA_EXECUTION_INPUT.to_string(),
            task_id: self.task_id.clone(),
            owner: self.owner.clone(),
            name: self.name.clone(),
            dataset_prefix: self.dataset_prefix.clone(),
            run_prefix: self.run_prefix.clone(),
            epochs: self.epochs.map(|e| e.to_string()).unwrap_or_default(),
            inputs_sha256: inputs_string(&self.inputs_sha256),
            gate_contract_sha256: self.gate_contract_sha256.clone(),
        }
    }
}

/// Where the training work is started and its outputs are linked.
///
/// The request side validates, mints and answers; WHO runs the job and WHERE is
/// behind this trait. The AWS implementation (plan 14-04) starts a Step Functions
/// execution named after the task id and presigns S3 links.
#[pmcp::async_trait]
pub trait Dispatcher: Send + Sync {
    /// The server bucket the `s3://` URIs name.
    fn bucket(&self) -> &str;

    /// Start the run. See [`DispatchError`] for the two failure classes.
    async fn dispatch(&self, spec: &RunSpec) -> Result<(), DispatchError>;

    /// After an ambiguous dispatch: did an execution with the deterministic name
    /// (the task id) start? An `Err` means "could not find out".
    async fn confirm(&self, task_id: &str) -> Result<Confirm, String>;

    /// A time-limited download link for `<run_prefix><path>`, minted at read time
    /// and never stored. `run_prefix` is the key form, `runs/<name>/<task_id>/`.
    async fn link(&self, run_prefix: &str, path: &str) -> Option<String>;
}

/// Where a dataset's files are read from.
#[pmcp::async_trait]
pub trait DatasetSource: Send + Sync {
    /// Read the four files under `dataset_prefix` (`datasets/<segs>/`). A file
    /// that does not exist is `None`, not an error.
    async fn fetch(&self, dataset_prefix: &str) -> Result<DatasetFiles, Refusal>;
}

/// Reads a dataset from a directory: `<root>/<dataset_prefix>/<file>`. For tests
/// and any stdio harness; the AWS source reads S3.
#[derive(Debug, Clone)]
pub struct LocalDatasetSource {
    root: PathBuf,
}

impl LocalDatasetSource {
    /// A source rooted at `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

#[pmcp::async_trait]
impl DatasetSource for LocalDatasetSource {
    async fn fetch(&self, dataset_prefix: &str) -> Result<DatasetFiles, Refusal> {
        let dir = self.root.join(dataset_prefix.trim_end_matches('/'));
        let read = |file: &'static str| {
            let path = dir.join(file);
            async move {
                match tokio::fs::read(&path).await {
                    Ok(bytes) => Ok(Some(bytes)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(_) => Err(Refusal::new("dataset-read", format!("cannot read {file}"))),
                }
            }
        };
        Ok(DatasetFiles {
            task_json: read("task.json").await?,
            train: read("train.jsonl").await?,
            eval: read("eval.jsonl").await?,
            shift: read("shift.jsonl").await?,
        })
    }
}

/// Everything a handler needs, built so the task store and the single-flight
/// lock address ONE backend.
pub struct ServerCtx {
    /// The task store, built from `backend` in [`ServerCtx::new`].
    pub store: Arc<AprenderTaskStore>,
    /// The backend the store was built from, and the lock's.
    pub backend: Arc<dyn TaskBackend>,
    /// Starts runs and mints links.
    pub dispatcher: Arc<dyn Dispatcher>,
    /// Reads datasets.
    pub source: Arc<dyn DatasetSource>,
}

impl std::fmt::Debug for ServerCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerCtx").finish_non_exhaustive()
    }
}

impl ServerCtx {
    /// The one constructor. It builds the store from `backend` (plus the cancel
    /// sink when given) and keeps the same `Arc` as `ctx.backend`, because
    /// `AprenderTaskStore`'s backend is private: a component that needs both the
    /// store and the raw backend (the lock, `attach_result_if_absent`) can only
    /// get the same one through here.
    #[must_use]
    pub fn new(
        backend: Arc<dyn TaskBackend>,
        dispatcher: Arc<dyn Dispatcher>,
        source: Arc<dyn DatasetSource>,
        cancel_sink: Option<Arc<dyn CancelSink>>,
    ) -> Arc<Self> {
        let mut store = AprenderTaskStore::new(Arc::clone(&backend));
        if let Some(sink) = cancel_sink {
            store = store.with_cancel_sink(sink);
        }
        Arc::new(Self {
            store: Arc::new(store),
            backend,
            dispatcher,
            source,
        })
    }
}

/// The owner this request's task belongs to: the authenticated subject when
/// there is one, [`UNAUTHENTICATED_OWNER`] otherwise (same derivation as the
/// SetFit server, so every door resolves it the same way).
fn resolve_owner(extra: &RequestHandlerExtra) -> String {
    extra.auth_context().map_or_else(
        || UNAUTHENTICATED_OWNER.to_string(),
        |ctx| ctx.subject.clone(),
    )
}

fn refuse(refusal: &Refusal) -> pmcp::Error {
    pmcp::Error::validation(refusal.to_string())
}

fn single_flight(holder: &str) -> pmcp::Error {
    pmcp::Error::validation(format!(
        "REFUSED single-flight: task {holder} is running on this deployment; one job at a time (D-11)"
    ))
}

/// Move a minted-but-never-started task to `failed`, so it is never left
/// `working` for work that will not run.
async fn compensate(ctx: &ServerCtx, task_id: &str, owner: &str, payload: StatusPayload) {
    let _ = ctx
        .store
        .finish(
            task_id,
            owner,
            TaskStatus::Failed,
            payload_result(&payload, true),
        )
        .await;
}

fn failed_payload(task_id: &str, name: &str, run_prefix_uri: &str, reason: &str) -> StatusPayload {
    let mut payload = StatusPayload::base(task_id, name, run_prefix_uri, Phase::Failed);
    payload.error = Some(reason.to_string());
    payload
}

/// The envelope: handles only (never dataset content, a token or a URL).
#[derive(Serialize, Deserialize)]
struct Envelope {
    dataset_uri: String,
    name: String,
    run_prefix: String,
}

/// Mint, lock, dispatch, answer: the `train` handler.
async fn submit(
    ctx: &ServerCtx,
    extra: &RequestHandlerExtra,
    args: TrainArgs,
) -> pmcp::Result<serde_json::Value> {
    // (a) Drop any arm left by an earlier call that never opened the create gate.
    ctx.store.clear_handoff();

    // (b) Guard the arguments, then the owner (fail closed before any I/O).
    args::check_name(&args.name).map_err(|r| refuse(&r))?;
    let bucket = ctx.dispatcher.bucket();
    let dataset_prefix =
        args::parse_dataset_uri(&args.dataset_uri, bucket).map_err(|r| refuse(&r))?;
    let owner = resolve_owner(extra);
    if owner == LOCK_OWNER {
        return Err(refuse(&Refusal::new("owner", "reserved")));
    }

    // (c) Fetch and validate for SUBMISSION: the only place the epoch rule refuses.
    let policy = ValidationPolicy::shared().map_err(pmcp::Error::internal)?;
    let files = ctx
        .source
        .fetch(&dataset_prefix)
        .await
        .map_err(|r| refuse(&r))?;
    let (summary, epochs) =
        validate_for_submit(&files, args.epochs, policy).map_err(|r| refuse(&r))?;

    // (e) Pre-check the lock: refuse without minting when a live holder exists.
    if let Some(holder) = lock::peek(ctx.backend.as_ref()).await {
        return Err(single_flight(&holder));
    }

    // (f) Mint: the id keys the lock, the envelope, the run prefix and the dispatch.
    let task = ctx
        .store
        .mint_for_request(&owner, Some(TASK_TTL_MS))
        .await
        .map_err(|e| pmcp::Error::internal(format!("cannot mint a training task: {e}")))?;
    let task_id = task.task_id.clone();
    let key_prefix = run_prefix(&args.name, &task_id);
    let run_prefix_uri = format!("s3://{bucket}/{key_prefix}");

    // (g) Acquire the lease; a lost race compensates the minted task.
    match lock::acquire(ctx.backend.as_ref(), &task_id, &owner, now_secs()).await {
        Ok(()) => {}
        Err(AcquireError::Held(held)) => {
            let error = single_flight(&held.holder);
            let reason = format!("single-flight: task {} holds the lock", held.holder);
            compensate(
                ctx,
                &task_id,
                &owner,
                failed_payload(&task_id, &args.name, &run_prefix_uri, &reason),
            )
            .await;
            return Err(error);
        }
        Err(AcquireError::Backend(message)) => {
            let reason = format!("cannot take the single-flight lock: {message}");
            compensate(
                ctx,
                &task_id,
                &owner,
                failed_payload(&task_id, &args.name, &run_prefix_uri, &reason),
            )
            .await;
            return Err(pmcp::Error::internal(reason));
        }
    }

    // (h) The envelope carries handles only.
    let envelope = Envelope {
        dataset_uri: args.dataset_uri.clone(),
        name: args.name.clone(),
        run_prefix: run_prefix_uri.clone(),
    };
    let stashed = match serde_json::to_value(&envelope) {
        Ok(value) => ctx
            .store
            .put_envelope(&task_id, &owner, value)
            .await
            .map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    if let Err(message) = stashed {
        let reason = format!("cannot record the run envelope: {message}");
        lock::release(ctx.backend.as_ref(), &task_id).await;
        compensate(
            ctx,
            &task_id,
            &owner,
            failed_payload(&task_id, &args.name, &run_prefix_uri, &reason),
        )
        .await;
        return Err(pmcp::Error::internal(reason));
    }

    // (i) Dispatch. An AMBIGUOUS start is "maybe started": the lock is released only
    // on a confirmed non-start.
    let spec = RunSpec {
        task_id: task_id.clone(),
        owner: owner.clone(),
        name: args.name.clone(),
        dataset_prefix,
        run_prefix: key_prefix,
        epochs,
        inputs_sha256: summary.inputs_sha256.clone(),
        gate_contract_sha256: policy.gate_contract_sha256.clone(),
    };
    let not_started = match ctx.dispatcher.dispatch(&spec).await {
        Ok(()) => None,
        Err(DispatchError::Rejected(reason)) => Some(reason),
        Err(DispatchError::Ambiguous(reason)) => match ctx.dispatcher.confirm(&task_id).await {
            Ok(Confirm::Started) => None,
            Ok(Confirm::NotStarted) => Some(reason),
            Err(confirm_error) => {
                // Cannot tell. Leave the task Working and the lock held: the
                // finalizer's reconcile sweep (14-05) resolves it.
                eprintln!(
                    "laya-train dispatch unconfirmed task {task_id}: {reason}; confirm failed: {confirm_error}"
                );
                None
            }
        },
    };
    if let Some(reason) = not_started {
        lock::release(ctx.backend.as_ref(), &task_id).await;
        compensate(
            ctx,
            &task_id,
            &owner,
            failed_payload(&task_id, &args.name, &run_prefix_uri, &reason),
        )
        .await;
        return Err(pmcp::Error::validation(format!(
            "REFUSED dispatch: {reason}"
        )));
    }

    // (j) The task-shaped answer: `taskId` + `status` open pmcp's create gate, and
    // the armed handoff makes the store return THIS task rather than minting another.
    Ok(working_answer(&task_id, &args.name, &run_prefix_uri))
}

fn working_answer(task_id: &str, name: &str, run_prefix_uri: &str) -> serde_json::Value {
    let mut value = StatusPayload::base(task_id, name, run_prefix_uri, Phase::Working).to_value();
    if let Some(object) = value.as_object_mut() {
        object.insert("taskId".to_string(), serde_json::Value::from(task_id));
        object.insert("status".to_string(), serde_json::Value::from("working"));
        object.insert("ttl".to_string(), serde_json::Value::from(TASK_TTL_MS));
    }
    value
}

/// The `validate_dataset` handler: INSPECTION. Never refuses `epochs`.
async fn validate_dataset(ctx: &ServerCtx, args: ValidateArgs) -> pmcp::Result<serde_json::Value> {
    #[derive(Serialize)]
    struct Answer {
        valid: bool,
        #[serde(flatten)]
        summary: validate::DatasetSummary,
    }
    let dataset_prefix = args::parse_dataset_uri(&args.dataset_uri, ctx.dispatcher.bucket())
        .map_err(|r| refuse(&r))?;
    let policy = ValidationPolicy::shared().map_err(pmcp::Error::internal)?;
    let files = ctx
        .source
        .fetch(&dataset_prefix)
        .await
        .map_err(|r| refuse(&r))?;
    let summary = validate_files(&files, policy).map_err(|r| refuse(&r))?;
    serde_json::to_value(Answer {
        valid: true,
        summary,
    })
    .map_err(|e| pmcp::Error::internal(e.to_string()))
}

/// The `train_status` handler. The one status payload; links are minted here, at
/// read time, and never written back.
async fn status(
    ctx: &ServerCtx,
    extra: &RequestHandlerExtra,
    task_id: &str,
) -> pmcp::Result<serde_json::Value> {
    let owner = resolve_owner(extra);
    let task =
        ctx.store.get(task_id, &owner).await.map_err(|e| {
            pmcp::Error::validation(format!("no task {task_id} on this server: {e}"))
        })?;

    // A stored result is THE payload: both doors serve one document.
    let mut unreadable = false;
    if let Ok(result) = ctx.store.get_result(task_id, &owner).await {
        unreadable = true;
        if let Some(Content::Text { text }) = result.content.first() {
            if let Ok(mut payload) = serde_json::from_str::<StatusPayload>(text) {
                let bucket_prefix = format!("s3://{}/", ctx.dispatcher.bucket());
                if let Some(key_prefix) = payload.run_prefix.strip_prefix(&bucket_prefix) {
                    let key_prefix = key_prefix.to_string();
                    for link in &mut payload.manifest {
                        link.url = ctx.dispatcher.link(&key_prefix, &link.path).await;
                    }
                }
                return Ok(payload.to_value());
            }
        }
    }

    // No usable stored result: synthesize the same shape from the envelope at
    // READ time, and never write it back.
    //   Working                    -> the working answer
    //   Cancelled                  -> phase cancelled (verdict null, manifest empty): the
    //                                 store's cancel writes no result
    //   any other terminal status  -> phase failed `no stored result` (no code path
    //                                 writes one; it must not read as working forever)
    let envelope = ctx
        .store
        .get_envelope(task_id, &owner)
        .await
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_value::<Envelope>(value).ok());
    let (name, uri) = envelope.map_or_else(
        || (String::new(), String::new()),
        |e| (e.name, e.run_prefix),
    );
    let payload = match task.status {
        _ if unreadable => failed_payload(task_id, &name, &uri, "stored result is unreadable"),
        TaskStatus::Cancelled => StatusPayload::base(task_id, &name, &uri, Phase::Cancelled),
        status if status.is_terminal() => failed_payload(task_id, &name, &uri, "no stored result"),
        _ => StatusPayload::base(task_id, &name, &uri, Phase::Working),
    };
    Ok(payload.to_value())
}

/// The streamable-HTTP config this server serves: `max_request_bytes` narrowed
/// from pmcp's 4 MB default to [`MAX_REQUEST_BYTES`], since a request is three
/// short arguments and pmcp rejects an oversized body with HTTP 413 before any
/// JSON parsing. Sessions are left on; a serverless wrapper must use `stateless()`.
#[must_use]
pub fn http_config() -> pmcp::server::streamable_http_server::StreamableHttpServerConfig {
    pmcp::server::streamable_http_server::StreamableHttpServerConfig {
        max_request_bytes: MAX_REQUEST_BYTES,
        ..Default::default()
    }
}

/// Assemble the server: the three tools and the task store, nothing else.
///
/// # Errors
///
/// `pmcp::Error` if the builder refuses the configuration.
pub fn build_server(ctx: Arc<ServerCtx>, version: &str) -> pmcp::Result<Server> {
    let train_ctx = Arc::clone(&ctx);
    let train_tool = TypedTool::new(TOOL_TRAIN, move |args: TrainArgs, extra| {
        let ctx = Arc::clone(&train_ctx);
        Box::pin(async move { submit(&ctx, &extra, args).await })
    })
    .with_description(
        "Start a Laya fine-tuning run on a dataset under this server's datasets/ prefix \
         (task.json, train.jsonl, eval.jsonl, optionally shift.jsonl). Takes dataset_uri, \
         name and, only above 16 shots per class, epochs. Returns an MCP task: poll \
         tasks/get and tasks/result, or poll train_status with the same task id. One job \
         runs at a time.",
    )
    .with_execution(ToolExecution::new().with_task_support(TaskSupport::Optional));

    let validate_ctx = Arc::clone(&ctx);
    let status_ctx = Arc::clone(&ctx);
    Server::builder()
        .name(SERVER_NAME)
        .version(version)
        .capabilities(ServerCapabilities::tools_only())
        .tool(TOOL_TRAIN, train_tool)
        .tool_typed_with_description::<ValidateArgs, _, _>(
            TOOL_VALIDATE,
            "Inspect a dataset prefix without training: runs every data rule and reports the \
             class counts, shots per class and whether `train` will need `epochs` (and its \
             range). It reports whether `train` will need epochs; it never refuses for their \
             absence.",
            move |args: ValidateArgs, _extra| {
                let ctx = Arc::clone(&validate_ctx);
                async move { validate_dataset(&ctx, args).await }
            },
        )
        .tool_typed_with_description::<StatusArgs, _, _>(
            TOOL_STATUS,
            "Report a training task's phase (working, completed, gate_failed, failed, \
             cancelled), and once a gate report exists the verdict and the run's files, each \
             with a time-limited download link. Takes the taskId that `train` returned.",
            move |args: StatusArgs, extra| {
                let ctx = Arc::clone(&status_ctx);
                async move { status(&ctx, &extra, &args.task_id).await }
            },
        )
        .task_store(Arc::clone(&ctx.store) as Arc<dyn TaskStore>)
        .build()
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use pmcp::server::auth::AuthContext;

    use super::*;
    use crate::finalize::{
        attach_result_if_absent, decision_result, finalize_run, fixtures, AttachOutcome,
        FinalizeInput,
    };
    use crate::validate::test_data::dataset;

    const BUCKET: &str = "fixture-bucket";

    /// A dispatcher that records every RunSpec, answers confirm from a script
    /// and returns `https://presigned.example/<path>` links.
    pub(crate) struct FakeDispatcher {
        pub specs: Mutex<Vec<RunSpec>>,
        pub dispatch_script: Mutex<VecDeque<Result<(), DispatchError>>>,
        pub confirm_script: Mutex<VecDeque<Result<Confirm, String>>>,
    }

    impl FakeDispatcher {
        pub fn new() -> Arc<Self> {
            Arc::new(Self {
                specs: Mutex::new(Vec::new()),
                dispatch_script: Mutex::new(VecDeque::new()),
                confirm_script: Mutex::new(VecDeque::new()),
            })
        }

        pub fn recorded(&self) -> Vec<RunSpec> {
            self.specs.lock().expect("lock").clone()
        }
    }

    #[pmcp::async_trait]
    impl Dispatcher for FakeDispatcher {
        fn bucket(&self) -> &str {
            BUCKET
        }

        async fn dispatch(&self, spec: &RunSpec) -> Result<(), DispatchError> {
            self.specs.lock().expect("lock").push(spec.clone());
            self.dispatch_script
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or(Ok(()))
        }

        async fn confirm(&self, _task_id: &str) -> Result<Confirm, String> {
            self.confirm_script
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or(Err("no scripted answer".to_string()))
        }

        async fn link(&self, _run_prefix: &str, path: &str) -> Option<String> {
            Some(format!("https://presigned.example/{path}"))
        }
    }

    /// A temp directory removed on drop.
    pub(crate) struct TempRoot(pub PathBuf);

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub(crate) struct Harness {
        pub ctx: Arc<ServerCtx>,
        pub dispatcher: Arc<FakeDispatcher>,
        pub backend: Arc<dyn TaskBackend>,
        pub files: DatasetFiles,
        _root: TempRoot,
    }

    impl Harness {
        /// A 3-class dataset with `per_class` train rows per class written under
        /// `<tmp>/datasets/tracer/`, over a fresh in-memory backend.
        pub fn new(per_class: usize) -> Self {
            Self::with_backend(per_class, Arc::new(InMemoryTaskBackend::new()))
        }

        pub fn with_backend(per_class: usize, backend: Arc<dyn TaskBackend>) -> Self {
            let root = std::env::temp_dir().join(format!("laya-train-test-{}", mint_id()));
            let dir = root.join("datasets").join("tracer");
            std::fs::create_dir_all(&dir).expect("mkdir");
            let files = dataset(per_class);
            std::fs::write(
                dir.join("task.json"),
                files.task_json.as_deref().expect("task"),
            )
            .expect("write");
            std::fs::write(
                dir.join("train.jsonl"),
                files.train.as_deref().expect("train"),
            )
            .expect("write");
            std::fs::write(dir.join("eval.jsonl"), files.eval.as_deref().expect("eval"))
                .expect("write");
            let dispatcher = FakeDispatcher::new();
            let ctx = ServerCtx::new(
                Arc::clone(&backend),
                Arc::clone(&dispatcher) as Arc<dyn Dispatcher>,
                Arc::new(LocalDatasetSource::new(&root)),
                None,
            );
            Self {
                ctx,
                dispatcher,
                backend,
                files,
                _root: TempRoot(root),
            }
        }

        pub async fn train(
            &self,
            owner: &str,
            name: &str,
            epochs: Option<u32>,
        ) -> pmcp::Result<serde_json::Value> {
            submit(
                &self.ctx,
                &extra(owner),
                TrainArgs {
                    dataset_uri: format!("s3://{BUCKET}/datasets/tracer/"),
                    name: name.to_string(),
                    epochs,
                },
            )
            .await
        }
    }

    pub(crate) fn extra(owner: &str) -> RequestHandlerExtra {
        RequestHandlerExtra::default().with_auth_context(Some(AuthContext::new(owner)))
    }

    pub(crate) fn message(error: &pmcp::Error) -> String {
        match error {
            pmcp::Error::Validation(m) => m.clone(),
            other => other.to_string(),
        }
    }

    /// The committed fixtures, bound to this run's task id and model name.
    pub(crate) fn passing_input(spec: &RunSpec) -> FinalizeInput {
        let mut input = fixtures::input();
        input.bucket = BUCKET.to_string();
        input.execution = spec.execution_input();
        if let Some(outcome) = input.outcome.as_mut() {
            outcome.task_id.clone_from(&spec.task_id);
        }
        if let Some(pack) = input.pack_outcome.as_mut() {
            pack.task_id.clone_from(&spec.task_id);
        }
        if let Some(tm) = input.training_manifest.as_mut() {
            tm.task_id.clone_from(&spec.task_id);
        }
        for entry in &mut input.pack_manifest {
            if entry.path == "fixture-model.apr" {
                entry.path = layout::apr_entry_path(&spec.name);
            }
        }
        input
    }

    #[tokio::test]
    async fn tracer_train_dispatches_and_answers_working() {
        let h = Harness::new(20);
        let answer = h
            .train("alice", "tracer-model", Some(4))
            .await
            .expect("accepted");
        let id = answer["taskId"].as_str().expect("taskId").to_string();
        assert_eq!(answer["status"], "working");
        assert_eq!(answer["phase"], "working");
        assert_eq!(
            answer["run_prefix"],
            format!("s3://{BUCKET}/runs/tracer-model/{id}/")
        );
        assert_eq!(answer["ttl"], TASK_TTL_MS);
        let specs = h.dispatcher.recorded();
        assert_eq!(specs.len(), 1, "exactly one dispatch");
        let spec = &specs[0];
        assert_eq!(spec.task_id, id);
        assert_eq!(spec.owner, "alice");
        assert_eq!(spec.name, "tracer-model");
        assert_eq!(spec.dataset_prefix, "datasets/tracer/");
        assert_eq!(spec.run_prefix, format!("runs/tracer-model/{id}/"));
        assert_eq!(spec.epochs, Some(4));
    }

    #[tokio::test]
    async fn tracer_run_spec_binds_the_validated_bytes() {
        use aprender_decide::digest::sha256_hex;
        let h = Harness::new(20);
        h.train("alice", "tracer-model", Some(4))
            .await
            .expect("accepted");
        let spec = h.dispatcher.recorded().remove(0);
        assert_eq!(spec.inputs_sha256.len(), 3);
        assert_eq!(
            spec.inputs_sha256["task.json"],
            sha256_hex(h.files.task_json.as_deref().expect("task"))
        );
        assert_eq!(
            spec.inputs_sha256["train.jsonl"],
            sha256_hex(h.files.train.as_deref().expect("train"))
        );
        assert_eq!(
            spec.inputs_sha256["eval.jsonl"],
            sha256_hex(h.files.eval.as_deref().expect("eval"))
        );
        assert_eq!(
            spec.gate_contract_sha256,
            sha256_hex(validate::GATE_CONTRACT_YAML.as_bytes())
        );
        let exec = spec.execution_input();
        assert_eq!(exec.epochs, "4");
        assert!(layout::parse_inputs_string(&exec.inputs_sha256).is_ok());
    }

    #[tokio::test]
    async fn tracer_shared_backend() {
        let h = Harness::new(20);
        let answer = h
            .train("alice", "tracer-model", Some(4))
            .await
            .expect("accepted");
        let id = answer["taskId"].as_str().expect("taskId");
        let ctx_backend = &h.ctx.backend;
        let minted = ctx_backend
            .get("alice", id)
            .await
            .expect("get")
            .expect("task in the shared backend");
        assert_eq!(minted.task.status, TaskStatus::Working);
        let lock_record = ctx_backend
            .get(LOCK_OWNER, lock::LOCK_TASK_ID)
            .await
            .expect("get")
            .expect("lock record in the same backend");
        assert_eq!(lock_record.task.status, TaskStatus::Working);
        // And the harness's own handle is the very same backend.
        assert!(h
            .backend
            .get(LOCK_OWNER, lock::LOCK_TASK_ID)
            .await
            .expect("get")
            .is_some());
    }

    #[tokio::test]
    async fn tracer_finalize_then_status_has_links_and_stored_result_has_none() {
        let h = Harness::new(20);
        let answer = h
            .train("alice", "tracer-model", Some(4))
            .await
            .expect("accepted");
        let id = answer["taskId"].as_str().expect("taskId").to_string();
        let spec = h.dispatcher.recorded().remove(0);

        // Working until the finalizer writes.
        let working = status(&h.ctx, &extra("alice"), &id).await.expect("status");
        assert_eq!(working["phase"], "working");

        let decision = finalize_run(&passing_input(&spec));
        assert_eq!(decision.phase, Phase::Completed);
        h.ctx
            .store
            .finish(&id, "alice", decision.status, decision_result(&decision))
            .await
            .expect("finish");
        assert!(lock::release(h.ctx.backend.as_ref(), &id).await);

        let read = status(&h.ctx, &extra("alice"), &id).await.expect("status");
        assert_eq!(read["phase"], "completed");
        assert_eq!(read["verdict"]["pass"], true);
        assert_eq!(read["verdict"]["apr_sha256"], fixtures::sha("apr"));
        let manifest = read["manifest"].as_array().expect("manifest");
        assert_eq!(
            manifest.len(),
            fixtures::training_manifest().entries.len() + fixtures::pack_entries().len()
        );
        for entry in manifest {
            let path = entry["path"].as_str().expect("path");
            assert_eq!(entry["url"], format!("https://presigned.example/{path}"));
        }

        // The stored payload is the same document with every url null.
        let stored = h
            .ctx
            .store
            .get_result(&id, "alice")
            .await
            .expect("stored result");
        let Some(Content::Text { text }) = stored.content.first() else {
            panic!("stored result has no text");
        };
        let stored: serde_json::Value = serde_json::from_str(text).expect("json");
        let mut expected = read.clone();
        for entry in expected["manifest"].as_array_mut().expect("manifest") {
            entry["url"] = serde_json::Value::Null;
        }
        assert_eq!(stored, expected);
        assert!(stored["manifest"]
            .as_array()
            .expect("manifest")
            .iter()
            .all(|e| e["url"].is_null()));
    }

    #[tokio::test]
    async fn tracer_lock_is_free_after_finalize() {
        let h = Harness::new(20);
        let answer = h
            .train("alice", "tracer-model", Some(4))
            .await
            .expect("accepted");
        let id = answer["taskId"].as_str().expect("taskId").to_string();
        // While the first runs, a second is refused.
        let refused = h
            .train("alice", "second-model", Some(4))
            .await
            .expect_err("single flight");
        assert!(
            message(&refused).starts_with("REFUSED single-flight:"),
            "{}",
            message(&refused)
        );
        let spec = h.dispatcher.recorded().remove(0);
        let decision = finalize_run(&passing_input(&spec));
        h.ctx
            .store
            .finish(&id, "alice", decision.status, decision_result(&decision))
            .await
            .expect("finish");
        assert!(lock::release(h.ctx.backend.as_ref(), &id).await);
        assert_eq!(lock::peek(h.ctx.backend.as_ref()).await, None);
        let second = h
            .train("alice", "second-model", Some(4))
            .await
            .expect("accepted after finalize");
        assert_ne!(second["taskId"], answer["taskId"]);
        assert_eq!(h.dispatcher.recorded().len(), 2);
    }

    #[test]
    fn tracer_readme_links_the_monorepo() {
        let readme = include_str!("../README.md");
        assert!(readme.contains("paiml/aprender"));
    }

    #[test]
    fn tracer_server_builds() {
        let h = Harness::new(5);
        build_server(Arc::clone(&h.ctx), "0.0.0-test").expect("the server builds");
    }

    // ------------------------------------------------------------------
    // Task 2: the argument refusals and the single-flight lease
    // ------------------------------------------------------------------

    fn rule() -> EpochRule {
        ValidationPolicy::from_contract()
            .expect("the embedded contract parses")
            .epoch_rule
    }

    #[test]
    fn args_refuse_unknown_keys() {
        for key in ["seeds", "stopping", "device", "config", "bars"] {
            let mut doc = serde_json::json!({ "dataset_uri": "s3://b/datasets/x/", "name": "m" });
            doc[key] = serde_json::json!(1);
            let err = serde_json::from_value::<TrainArgs>(doc).expect_err("unknown key refused");
            assert!(err.to_string().contains(key), "{err}");
        }
        serde_json::from_value::<TrainArgs>(serde_json::json!({
            "dataset_uri": "s3://b/datasets/x/", "name": "m", "epochs": 4
        }))
        .expect("the three knobs parse");
    }

    #[test]
    fn args_require_name_and_dataset() {
        serde_json::from_value::<TrainArgs>(
            serde_json::json!({ "dataset_uri": "s3://b/datasets/x/" }),
        )
        .expect_err("name is required");
        serde_json::from_value::<TrainArgs>(serde_json::json!({ "name": "m" }))
            .expect_err("dataset_uri is required");
        let plain: TrainArgs = serde_json::from_value(
            serde_json::json!({ "dataset_uri": "s3://b/datasets/x/", "name": "m" }),
        )
        .expect("epochs is optional");
        assert!(plain.epochs.is_none());
    }

    #[test]
    fn args_validate_and_status_are_closed() {
        serde_json::from_value::<ValidateArgs>(
            serde_json::json!({ "dataset_uri": "s3://b/datasets/x/" }),
        )
        .expect("dataset only");
        serde_json::from_value::<ValidateArgs>(
            serde_json::json!({ "dataset_uri": "s3://b/datasets/x/", "epochs": 4 }),
        )
        .expect_err("validate_dataset has no epochs argument (D-21)");
        serde_json::from_value::<StatusArgs>(serde_json::json!({}))
            .expect_err("task_id is required");
        serde_json::from_value::<StatusArgs>(serde_json::json!({ "task_id": "t", "x": 1 }))
            .expect_err("unknown key refused");
    }

    #[test]
    fn args_epoch_numbers_come_from_the_contract() {
        let contract: serde_yaml::Value =
            serde_yaml::from_str(validate::GATE_CONTRACT_YAML).expect("contract parses");
        let recipe = &contract["recipe"];
        let read = |key: &str| {
            u32::try_from(
                recipe[key]
                    .as_u64()
                    .unwrap_or_else(|| panic!("recipe.{key} missing")),
            )
            .expect("fits u32")
        };
        let rule = rule();
        assert_eq!(rule.fixed, read("epochs_at_most_16_per_class"));
        assert_eq!(rule.lo, read("epochs_above_16_min"));
        assert_eq!(rule.hi, read("epochs_above_16_max"));
        assert_eq!((rule.fixed, rule.lo, rule.hi), (12, 4, 12));
    }

    #[test]
    fn args_epochs_above_16() {
        let rule = rule();
        for bad in [0, 3, 13, 100] {
            let err = args::resolve_epochs(20, Some(bad), &rule).expect_err("outside the range");
            assert_eq!(err.rule, "epochs", "{err}");
            assert!(err.detail.contains("[4, 12]"), "{err}");
        }
        for ok in [4, 8, 12] {
            assert_eq!(args::resolve_epochs(20, Some(ok), &rule), Ok(Some(ok)));
        }
        let err = args::resolve_epochs(20, None, &rule).expect_err("required above 16");
        assert_eq!(err.rule, "epochs");
        assert!(err.detail.contains("[4, 12]"), "{err}");
        assert_eq!(
            args::resolve_epochs(17, Some(4), &rule),
            Ok(Some(4)),
            "17 is above 16"
        );
    }

    #[test]
    fn args_epochs_at_or_below_16_are_fixed_not_ignored() {
        let rule = rule();
        let err = args::resolve_epochs(16, Some(12), &rule).expect_err("fixed, not ignored");
        assert_eq!(err.rule, "epochs");
        assert!(err.detail.contains("fixes epochs to 12"), "{err}");
        assert_eq!(args::resolve_epochs(16, None, &rule), Ok(None));
        assert_eq!(args::resolve_epochs(1, None, &rule), Ok(None));
        assert!(args::resolve_epochs(1, Some(4), &rule).is_err());
    }

    #[test]
    fn name_accepts_the_slug_grammar() {
        let longest = "a".repeat(48);
        for ok in ["a", "0", "motivation-v1", "a-b-c", longest.as_str()] {
            assert!(args::check_name(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn name_refuses_everything_else() {
        let too_long = "a".repeat(49);
        for bad in [
            "",
            "A",
            "-x",
            "a_b",
            "a/b",
            "a..b",
            "a.b",
            "a b",
            "ab\n",
            "naïve",
            "名前",
            too_long.as_str(),
        ] {
            let err = args::check_name(bad).expect_err(bad);
            assert_eq!(err.rule, "name", "{bad}");
        }
    }

    #[test]
    fn uri_accepts_the_datasets_grammar() {
        assert_eq!(
            args::parse_dataset_uri("s3://b/datasets/x/", "b").as_deref(),
            Ok("datasets/x/")
        );
        assert_eq!(
            args::parse_dataset_uri("s3://b/datasets/x", "b").as_deref(),
            Ok("datasets/x/")
        );
        assert_eq!(
            args::parse_dataset_uri("s3://b/datasets/a/b/c/d/", "b").as_deref(),
            Ok("datasets/a/b/c/d/")
        );
        assert_eq!(
            args::parse_dataset_uri("s3://b/datasets/v1.2_x-y/", "b").as_deref(),
            Ok("datasets/v1.2_x-y/")
        );
    }

    #[test]
    fn uri_refuses_everything_else() {
        let long_seg = format!("s3://b/datasets/{}/", "a".repeat(65));
        for bad in [
            "s3://other/datasets/x/",
            "s3://bb/datasets/x/",
            "s3://b",
            "s3://b/",
            "s3://b/runs/x/",
            "s3://b/datasets/",
            "s3://b/datasets//x/",
            "s3://b/datasets/x//",
            "s3://b/datasets/../x/",
            "s3://b/datasets/./x/",
            "s3://b/datasets/%2e%2e/",
            "s3://b/datasets/x%2fy/",
            "s3://b/datasets/a/b/c/d/e/",
            "s3://b/datasets/Upper/",
            "s3://b/datasets/.hidden/",
            "s3://b/datasets/x y/",
            "s3://b/datasets/x?versionId=1",
            "s3://b/datasets/x#frag",
            "http://b/datasets/x/",
            "b/datasets/x/",
            "",
            long_seg.as_str(),
        ] {
            let err = args::parse_dataset_uri(bad, "b").expect_err(bad);
            assert_eq!(err.rule, "dataset-uri", "{bad}");
            assert!(
                err.detail.contains("s3://b/datasets/"),
                "names the server bucket: {err}"
            );
        }
    }

    async fn task_count(h: &Harness, owner: &str) -> usize {
        h.backend.list(owner, None).await.expect("list").0.len()
    }

    async fn only_task(h: &Harness, owner: &str) -> StoredTask {
        let (mut records, _) = h.backend.list(owner, None).await.expect("list");
        assert_eq!(records.len(), 1, "exactly one task for {owner}");
        records.remove(0)
    }

    fn stored_phase(record: &StoredTask) -> String {
        let Some(result) = record.result.as_ref() else {
            return "no result".to_string();
        };
        let Some(Content::Text { text }) = result.content.first() else {
            return "no text".to_string();
        };
        let payload: serde_json::Value = serde_json::from_str(text).expect("json");
        payload["phase"].as_str().unwrap_or("?").to_string()
    }

    #[tokio::test]
    async fn submit_single_flight_refuses_naming_the_holder_and_mints_nothing() {
        let h = Harness::new(20);
        let first = h
            .train("alice", "first-model", Some(4))
            .await
            .expect("accepted");
        let holder = first["taskId"].as_str().expect("taskId").to_string();
        let before = (task_count(&h, "alice").await, task_count(&h, "bob").await);
        let err = h
            .train("bob", "second-model", Some(4))
            .await
            .expect_err("refused");
        let text = message(&err);
        assert!(
            text.starts_with(&format!(
                "REFUSED single-flight: task {holder} is running on this deployment"
            )),
            "{text}"
        );
        assert_eq!(
            before,
            (task_count(&h, "alice").await, task_count(&h, "bob").await),
            "the pre-check mints nothing"
        );
        assert_eq!(h.dispatcher.recorded().len(), 1, "nothing was dispatched");
        assert_eq!(
            lock::peek(h.backend.as_ref()).await.as_deref(),
            Some(holder.as_str())
        );
    }

    /// A backend whose FIRST read of the lock record answers "absent" while a rival
    /// takes the lock behind it: the pre-check passes and the acquire then loses.
    struct RacyBackend {
        inner: Arc<InMemoryTaskBackend>,
        fired: std::sync::atomic::AtomicBool,
    }

    #[pmcp::async_trait]
    impl TaskBackend for RacyBackend {
        async fn put_new(&self, owner_id: &str, record: StoredTask) -> Result<(), BackendError> {
            self.inner.put_new(owner_id, record).await
        }

        async fn get(
            &self,
            owner_id: &str,
            task_id: &str,
        ) -> Result<Option<StoredTask>, BackendError> {
            if owner_id == LOCK_OWNER && !self.fired.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                lock::acquire(
                    self.inner.as_ref(),
                    "RIVAL",
                    "rival-owner",
                    lock::now_secs(),
                )
                .await
                .expect("the rival takes the lock");
                return Ok(None);
            }
            self.inner.get(owner_id, task_id).await
        }

        async fn cas(
            &self,
            owner_id: &str,
            task_id: &str,
            expected_version: u64,
            record: StoredTask,
        ) -> Result<(), BackendError> {
            self.inner
                .cas(owner_id, task_id, expected_version, record)
                .await
        }

        async fn list(
            &self,
            owner_id: &str,
            cursor: Option<&str>,
        ) -> Result<(Vec<StoredTask>, Option<String>), BackendError> {
            self.inner.list(owner_id, cursor).await
        }

        async fn sweep_expired(&self) -> Result<usize, BackendError> {
            self.inner.sweep_expired().await
        }
    }

    #[tokio::test]
    async fn submit_single_flight_lost_race_leaves_the_minted_task_failed() {
        let inner = Arc::new(InMemoryTaskBackend::new());
        // The rival's own task record, live, so its lock is live.
        inner
            .put_new(
                "rival-owner",
                StoredTask {
                    task: pmcp::types::tasks::Task::new("RIVAL", TaskStatus::Working),
                    result: None,
                    envelope: None,
                    version: 1,
                    expires_at_secs: None,
                },
            )
            .await
            .expect("rival task");
        let racy = Arc::new(RacyBackend {
            inner: Arc::clone(&inner),
            fired: std::sync::atomic::AtomicBool::new(false),
        });
        let h = Harness::with_backend(20, racy);
        let err = h
            .train("alice", "loser", Some(4))
            .await
            .expect_err("lost the race");
        assert!(
            message(&err).starts_with("REFUSED single-flight: task RIVAL "),
            "{}",
            message(&err)
        );
        let minted = only_task(&h, "alice").await;
        assert_eq!(
            minted.task.status,
            TaskStatus::Failed,
            "failed, not working"
        );
        assert_eq!(stored_phase(&minted), "failed");
        assert!(h.dispatcher.recorded().is_empty(), "nothing was dispatched");
        let state = lock::read(inner.as_ref()).await.expect("lock state");
        assert_eq!(
            state.holder, "RIVAL",
            "the loser must not release the winner's lock"
        );
        assert!(!state.released);
    }

    #[tokio::test]
    async fn submit_epochs_enforced() {
        let h = Harness::new(20);
        let err = h
            .train("alice", "m", None)
            .await
            .expect_err("epochs required above 16");
        let text = message(&err);
        assert!(text.starts_with("REFUSED epochs:"), "{text}");
        assert!(text.contains("[4, 12]"), "{text}");
        assert_eq!(task_count(&h, "alice").await, 0, "nothing minted");
        assert!(
            lock::read(h.backend.as_ref()).await.is_none(),
            "no lock taken"
        );
        assert!(h.dispatcher.recorded().is_empty());
        h.train("alice", "m", Some(4))
            .await
            .expect("accepted with epochs 4");
    }

    #[tokio::test]
    async fn submit_epochs_refused_at_16_shots() {
        let h = Harness::new(16);
        let err = h
            .train("alice", "m", Some(12))
            .await
            .expect_err("fixed at 16");
        assert!(
            message(&err).starts_with("REFUSED epochs:"),
            "{}",
            message(&err)
        );
        let ok = h
            .train("alice", "m", None)
            .await
            .expect("fixed epochs need none");
        assert_eq!(ok["status"], "working");
        assert_eq!(h.dispatcher.recorded()[0].epochs, None);
    }

    #[tokio::test]
    async fn submit_bad_arguments_mint_nothing() {
        let h = Harness::new(20);
        let err = h
            .train("alice", "Bad_Name", Some(4))
            .await
            .expect_err("name");
        assert!(
            message(&err).starts_with("REFUSED name:"),
            "{}",
            message(&err)
        );
        let err = submit(
            &h.ctx,
            &extra("alice"),
            TrainArgs {
                dataset_uri: "s3://other/datasets/tracer/".to_string(),
                name: "m".to_string(),
                epochs: Some(4),
            },
        )
        .await
        .expect_err("uri");
        assert!(
            message(&err).starts_with("REFUSED dataset-uri:"),
            "{}",
            message(&err)
        );
        let err = submit(
            &h.ctx,
            &extra("alice"),
            TrainArgs {
                dataset_uri: format!("s3://{BUCKET}/datasets/absent/"),
                name: "m".to_string(),
                epochs: Some(4),
            },
        )
        .await
        .expect_err("no such dataset");
        assert!(
            message(&err).starts_with("REFUSED task-missing:"),
            "{}",
            message(&err)
        );
        assert_eq!(task_count(&h, "alice").await, 0);
        assert!(lock::read(h.backend.as_ref()).await.is_none());
    }

    #[tokio::test]
    async fn lock_owner_reserved() {
        let h = Harness::new(20);
        let err = h
            .train(LOCK_OWNER, "m", Some(4))
            .await
            .expect_err("the reserved owner is refused");
        assert!(
            message(&err).starts_with("REFUSED owner:"),
            "{}",
            message(&err)
        );
        assert_eq!(
            task_count(&h, LOCK_OWNER).await,
            0,
            "nothing minted, no lock written"
        );
        assert!(lock::read(h.backend.as_ref()).await.is_none());
        assert!(h.dispatcher.recorded().is_empty());
    }

    #[tokio::test]
    async fn lock_holder_owner_after_submit() {
        let h = Harness::new(20);
        let answer = h.train("alice", "m", Some(4)).await.expect("accepted");
        let state = lock::read(h.backend.as_ref()).await.expect("lock state");
        assert_eq!(state.holder, answer["taskId"].as_str().expect("taskId"));
        assert_eq!(state.holder_owner, "alice");
        assert!(!state.released);
    }

    #[tokio::test]
    async fn submit_dispatch_rejected() {
        let h = Harness::new(20);
        h.dispatcher
            .dispatch_script
            .lock()
            .expect("lock")
            .push_back(Err(DispatchError::Rejected("start refused".to_string())));
        let err = h.train("alice", "m", Some(4)).await.expect_err("rejected");
        assert!(message(&err).contains("start refused"), "{}", message(&err));
        assert_eq!(lock::peek(h.backend.as_ref()).await, None, "lock released");
        assert!(
            lock::read(h.backend.as_ref())
                .await
                .expect("state")
                .released
        );
        let minted = only_task(&h, "alice").await;
        assert_eq!(minted.task.status, TaskStatus::Failed);
        assert_eq!(stored_phase(&minted), "failed");
        // The deployment is free again.
        h.train("alice", "m", Some(4))
            .await
            .expect("a later submit is accepted");
    }

    #[tokio::test]
    async fn submit_dispatch_ambiguous_started() {
        let h = Harness::new(20);
        h.dispatcher
            .dispatch_script
            .lock()
            .expect("lock")
            .push_back(Err(DispatchError::Ambiguous("gateway timeout".to_string())));
        h.dispatcher
            .confirm_script
            .lock()
            .expect("lock")
            .push_back(Ok(Confirm::Started));
        let answer = h
            .train("alice", "m", Some(4))
            .await
            .expect("started after all");
        let id = answer["taskId"].as_str().expect("taskId");
        assert_eq!(answer["status"], "working");
        assert_eq!(lock::peek(h.backend.as_ref()).await.as_deref(), Some(id));
        assert_eq!(
            only_task(&h, "alice").await.task.status,
            TaskStatus::Working
        );
    }

    #[tokio::test]
    async fn submit_dispatch_ambiguous_not_started() {
        let h = Harness::new(20);
        h.dispatcher
            .dispatch_script
            .lock()
            .expect("lock")
            .push_back(Err(DispatchError::Ambiguous("gateway timeout".to_string())));
        h.dispatcher
            .confirm_script
            .lock()
            .expect("lock")
            .push_back(Ok(Confirm::NotStarted));
        let err = h
            .train("alice", "m", Some(4))
            .await
            .expect_err("not started");
        assert!(
            message(&err).contains("gateway timeout"),
            "{}",
            message(&err)
        );
        assert_eq!(
            lock::peek(h.backend.as_ref()).await,
            None,
            "released only after NotStarted"
        );
        assert_eq!(only_task(&h, "alice").await.task.status, TaskStatus::Failed);
    }

    #[tokio::test]
    async fn submit_dispatch_unconfirmed() {
        let h = Harness::new(20);
        h.dispatcher
            .dispatch_script
            .lock()
            .expect("lock")
            .push_back(Err(DispatchError::Ambiguous(
                "connection reset".to_string(),
            )));
        h.dispatcher
            .confirm_script
            .lock()
            .expect("lock")
            .push_back(Err("describe failed".to_string()));
        let answer = h
            .train("alice", "m", Some(4))
            .await
            .expect("answers working");
        let id = answer["taskId"].as_str().expect("taskId").to_string();
        assert_eq!(answer["status"], "working");
        assert_eq!(
            only_task(&h, "alice").await.task.status,
            TaskStatus::Working
        );
        assert_eq!(
            lock::peek(h.backend.as_ref()).await.as_deref(),
            Some(id.as_str())
        );
        let err = h
            .train("alice", "other", Some(4))
            .await
            .expect_err("single flight");
        assert!(
            message(&err).starts_with(&format!("REFUSED single-flight: task {id} ")),
            "{}",
            message(&err)
        );
    }

    #[tokio::test]
    async fn validate_reports_epochs_above_16_through_the_tool() {
        let h = Harness::new(20);
        let args = || ValidateArgs {
            dataset_uri: format!("s3://{BUCKET}/datasets/tracer/"),
        };
        let above = validate_dataset(&h.ctx, args()).await.expect("valid");
        assert_eq!(above["valid"], true);
        assert_eq!(above["shots_per_class"], 20);
        assert_eq!(above["epochs_required"], true);
        assert_eq!(above["epochs_range"], serde_json::json!([4, 12]));
        assert!(above["epochs_fixed"].is_null());
        assert_eq!(task_count(&h, "alice").await, 0, "inspection mints nothing");
        let at = Harness::new(16);
        let at = validate_dataset(&at.ctx, args()).await.expect("valid");
        assert_eq!(at["epochs_required"], false);
        assert_eq!(at["epochs_fixed"], 12);
        assert!(at["epochs_range"].is_null());
    }

    // ------------------------------------------------------------------
    // Task 3: the cancelled-task rule and the payload invariants
    // ------------------------------------------------------------------

    async fn started(h: &Harness) -> (String, RunSpec) {
        let answer = h
            .train("alice", "tracer-model", Some(4))
            .await
            .expect("accepted");
        let id = answer["taskId"].as_str().expect("taskId").to_string();
        let spec = h.dispatcher.recorded().remove(0);
        (id, spec)
    }

    async fn record(h: &Harness, id: &str) -> StoredTask {
        h.backend
            .get("alice", id)
            .await
            .expect("get")
            .expect("record")
    }

    fn stored_text(record: &StoredTask) -> Option<String> {
        match record.result.as_ref()?.content.first() {
            Some(Content::Text { text }) => Some(text.clone()),
            _ => None,
        }
    }

    #[tokio::test]
    async fn status_cancelled_unfinalized() {
        let h = Harness::new(20);
        let (id, _) = started(&h).await;
        let cancelled = h.ctx.store.cancel(&id, "alice").await.expect("cancel");
        assert_eq!(cancelled.status, TaskStatus::Cancelled);
        let before = record(&h, &id).await;
        assert!(
            before.result.is_none(),
            "the store's cancel writes no result"
        );
        let answer = status(&h.ctx, &extra("alice"), &id).await.expect("status");
        assert_eq!(answer["phase"], "cancelled");
        assert!(answer["verdict"].is_null());
        assert_eq!(answer["manifest"], serde_json::json!([]));
        assert!(answer["error"].is_null());
        assert_eq!(answer["name"], "tracer-model");
        assert_eq!(
            answer["run_prefix"],
            format!("s3://{BUCKET}/runs/tracer-model/{id}/")
        );
        let after = record(&h, &id).await;
        assert_eq!(before.version, after.version, "the read wrote nothing");
        assert!(after.result.is_none());
    }

    #[tokio::test]
    async fn status_terminal_without_result_reads_failed() {
        let h = Harness::new(20);
        let (id, _) = started(&h).await;
        let mut rec = record(&h, &id).await;
        let expected = rec.version;
        rec.task.status = TaskStatus::Failed;
        rec.version += 1;
        h.backend
            .cas("alice", &id, expected, rec)
            .await
            .expect("cas");
        let before = record(&h, &id).await.version;
        let answer = status(&h.ctx, &extra("alice"), &id).await.expect("status");
        assert_eq!(answer["phase"], "failed");
        assert_eq!(answer["error"], "no stored result");
        assert_eq!(record(&h, &id).await.version, before, "read time only");
    }

    #[tokio::test]
    async fn attach_cancelled() {
        let h = Harness::new(20);
        let (id, spec) = started(&h).await;
        h.ctx.store.cancel(&id, "alice").await.expect("cancel");
        // Before the attach, tasks/result has nothing to serve.
        assert!(h.ctx.store.get_result(&id, "alice").await.is_err());

        let mut input = passing_input(&spec);
        input.aborted = true;
        let decision = finalize_run(&input);
        assert_eq!(decision.phase, Phase::Cancelled);
        let outcome =
            attach_result_if_absent(h.backend.as_ref(), "alice", &id, decision_result(&decision))
                .await
                .expect("attach");
        assert_eq!(outcome, AttachOutcome::Attached);

        let after = record(&h, &id).await;
        assert_eq!(
            after.task.status,
            TaskStatus::Cancelled,
            "the status is unchanged"
        );
        let stored = h
            .ctx
            .store
            .get_result(&id, "alice")
            .await
            .expect("tasks/result now serves it");
        let Some(Content::Text { text }) = stored.content.first() else {
            panic!("no text");
        };
        let payload: serde_json::Value = serde_json::from_str(text).expect("json");
        assert_eq!(payload["schema"], layout::SCHEMA_STATUS);
        assert_eq!(payload["phase"], "cancelled");
        // train_status now returns the stored payload.
        let mut answer = status(&h.ctx, &extra("alice"), &id).await.expect("status");
        for entry in answer["manifest"].as_array_mut().expect("manifest") {
            assert!(entry["url"].is_string(), "links are minted at read time");
            entry["url"] = serde_json::Value::Null;
        }
        assert_eq!(
            answer, payload,
            "the same stored document, plus read-time links"
        );
    }

    #[tokio::test]
    async fn attach_second_noop() {
        let h = Harness::new(20);
        let (id, spec) = started(&h).await;
        h.ctx.store.cancel(&id, "alice").await.expect("cancel");
        let mut input = passing_input(&spec);
        input.aborted = true;
        let first = finalize_run(&input);
        assert_eq!(
            attach_result_if_absent(h.backend.as_ref(), "alice", &id, decision_result(&first))
                .await
                .expect("attach"),
            AttachOutcome::Attached
        );
        let before = record(&h, &id).await;
        let other = finalize_run(&passing_input(&spec));
        assert_eq!(
            attach_result_if_absent(h.backend.as_ref(), "alice", &id, decision_result(&other))
                .await
                .expect("attach"),
            AttachOutcome::AlreadyPresent
        );
        let after = record(&h, &id).await;
        assert_eq!(before.version, after.version);
        assert_eq!(
            stored_text(&before),
            stored_text(&after),
            "nothing replaces a stored result"
        );
        assert_eq!(after.task.status, TaskStatus::Cancelled);
    }

    #[tokio::test]
    async fn attach_refuses_working() {
        let h = Harness::new(20);
        let (id, spec) = started(&h).await;
        let decision = finalize_run(&passing_input(&spec));
        let before = record(&h, &id).await;
        assert_eq!(
            attach_result_if_absent(h.backend.as_ref(), "alice", &id, decision_result(&decision))
                .await
                .expect("attach"),
            AttachOutcome::NotTerminal
        );
        let after = record(&h, &id).await;
        assert_eq!(before.version, after.version, "nothing written");
        assert!(after.result.is_none());
        assert_eq!(after.task.status, TaskStatus::Working);
        assert_eq!(
            attach_result_if_absent(
                h.backend.as_ref(),
                "alice",
                "no-such-task",
                decision_result(&decision)
            )
            .await
            .expect("attach"),
            AttachOutcome::NotFound
        );
        assert_eq!(
            attach_result_if_absent(h.backend.as_ref(), "bob", &id, decision_result(&decision))
                .await
                .expect("attach"),
            AttachOutcome::NotFound,
            "another owner's task is not found"
        );
    }

    #[tokio::test]
    async fn payload_one_shape() {
        let h = Harness::new(20);
        let (id, spec) = started(&h).await;
        let keys = |v: &serde_json::Value| -> Vec<String> {
            let mut k: Vec<String> = v.as_object().expect("object").keys().cloned().collect();
            k.sort();
            k
        };
        let mut shapes = Vec::new();
        // working (read at status time)
        shapes.push((
            "working",
            status(&h.ctx, &extra("alice"), &id).await.expect("status"),
        ));
        // every finalize phase
        let mut crash = passing_input(&spec);
        crash.outcome.as_mut().expect("outcome").exit_class = layout::ExitClass::Crash;
        let mut gate_fail = passing_input(&spec);
        gate_fail.outcome.as_mut().expect("outcome").exit_class = layout::ExitClass::GateFail;
        gate_fail.gate_report.as_mut().expect("report")["pass"] = serde_json::json!(false);
        gate_fail.pack_outcome = None;
        let mut aborted = passing_input(&spec);
        aborted.aborted = true;
        for (label, input) in [
            ("completed", passing_input(&spec)),
            ("gate_failed", gate_fail),
            ("failed", crash),
            ("cancelled", aborted),
        ] {
            let decision = finalize_run(&input);
            shapes.push((label, decision.payload.to_value()));
        }
        // the read-time cancelled answer
        h.ctx.store.cancel(&id, "alice").await.expect("cancel");
        shapes.push((
            "read-time cancelled",
            status(&h.ctx, &extra("alice"), &id).await.expect("status"),
        ));
        let reference = keys(&shapes[0].1);
        for (label, shape) in &shapes {
            assert_eq!(&keys(shape), &reference, "{label} has a different key set");
        }
        // And the submit answer is the same document plus taskId, status and ttl.
        let submit_keys = keys(&working_answer(&id, "n", "u"));
        assert!(reference.iter().all(|k| submit_keys.contains(k)));
        assert_eq!(submit_keys.len(), reference.len() + 3);
    }
}
