//! The Laya training finalizer: the ONE writer of a run's terminal record,
//! `verdict.json`, `manifest.json`, retention tags and lock release.
//!
//! It is re-driven from three directions, so no guarantee rests on any one of
//! them being delivered:
//!
//! 1. the state machine's own `Finalize` payload (the normal end, and the `Catch` end);
//! 2. an EventBridge `Step Functions Execution Status Change` event for
//!    `ABORTED | TIMED_OUT | FAILED` (best effort, may repeat);
//! 3. a scheduled reconcile sweep, `{"source":"reconcile"}`, every 10 minutes.
//!
//! # The order of one finalization
//!
//! 1. compute is terminal (the training job and every pack task of the run);
//! 2. the DynamoDB record is written (the arbiter: first writer wins);
//! 3. `verdict.json`;
//! 4. `manifest.json`;
//! 5. the retention tags, each read back;
//! 6. the single-flight lock is released, LAST.
//!
//! A failure or crash at 3 to 5 returns `Err` with the lock STILL HELD, and the
//! next invocation resumes from the stored record and redoes 3 to 6. The held
//! lock is the durable "finalization incomplete" marker: a replacement `train`
//! is refused until the run is both over and published.
//!
//! Nothing is recomputed here (D-05): the decision is `finalize_run` over the
//! files the jobs wrote. Nothing is deleted: expiry is the lifecycle rule's job.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aprender_mcp_laya_train::finalize::{
    attach_result_if_absent, decision_result, finalize_run, AttachOutcome, FinalizeDecision,
    FinalizeInput,
};
use aprender_mcp_laya_train::layout::{
    self, segment_ok, Contracts, ExecutionInput, Images, Manifest, ManifestEntry, Outcome,
    PackOutcome, Phase, StatusPayload, TrainingManifest, RETENTION_EXPIRE, RETENTION_KEEP,
    SCHEMA_EXECUTION_INPUT, SCHEMA_MANIFEST, TASK_TTL_MS,
};
use aprender_mcp_laya_train::lock;
use aprender_mcp_laya_train::{AprenderTaskStore, StoredTask, TaskBackend};
use pmcp::server::task_store::TaskStore;
use pmcp::types::{CallToolResult, Content, TaskStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::compute::{
    compute_terminal, stop_live_compute, ExecutionReader, PackTasks, TrainingJobs,
};
use crate::objects::{ObjectError, ObjectStore};
use crate::sfn::execution_arn;

/// A lock whose execution is absent this long after it was taken is an unconfirmed
/// dispatch (14-01's `submit` left it held because `confirm` could not answer).
pub const DISPATCH_CONFIRM_GRACE_SECS: u64 = 900;

/// The EventBridge `detail-type` of a Step Functions status change.
pub const EVENT_DETAIL_TYPE: &str = "Step Functions Execution Status Change";

/// Largest JSON object the finalizer reads from a job-written key.
const MAX_JSON_BYTES: usize = 1_048_576;

/// Longest stage error the finalizer hands the decision (the decision clips it again).
const MAX_STAGE_ERROR_CHARS: usize = 2_048;

/// The object names that inventory a run's other objects and so cannot list
/// themselves. None is a manifest entry, but each is a retention-tag target when it
/// exists (D-19, IN-01): the three writer inventories are retagged with the run's
/// retention tag, so a kept run keeps them with the objects they list.
const SELF_LISTING: [&str; 5] = [
    layout::VERDICT,
    layout::MANIFEST,
    layout::TRAINING_MANIFEST,
    layout::PARTIAL_MANIFEST,
    layout::PACK_MANIFEST,
];

// ------------------------------------------------------------------- errors

/// Why a finalization did not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizerError {
    /// The input is not one of the three shapes, names another state machine or
    /// another run's prefix. Nothing was written.
    Refused(String),
    /// The records disagree about who owns the run. Nothing was written and the
    /// lock is left as it was; the FinalizerErrors alarm (14-08) is how a human hears.
    Integrity(String),
    /// A backend, S3, tagging or lock-release step failed. The lock is held; a
    /// retry or the sweep resumes from the stored record.
    Store(String),
    /// A compute read or stop failed. Nothing was released.
    Compute(String),
}

impl std::fmt::Display for FinalizerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(m) => write!(f, "refused: {m}"),
            Self::Integrity(m) => write!(f, "integrity: {m}"),
            Self::Store(m) => write!(f, "store: {m}"),
            Self::Compute(m) => write!(f, "compute: {m}"),
        }
    }
}

impl std::error::Error for FinalizerError {}

fn refused(message: impl Into<String>) -> FinalizerError {
    FinalizerError::Refused(message.into())
}

fn integrity(message: impl Into<String>) -> FinalizerError {
    FinalizerError::Integrity(message.into())
}

fn store_err(message: impl std::fmt::Display) -> FinalizerError {
    FinalizerError::Store(message.to_string())
}

// -------------------------------------------------------------------- input

/// The state machine's `Finalize` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateMachinePayload {
    /// The execution that is finalizing itself.
    pub execution_arn: String,
    /// When it started, in epoch seconds.
    pub start_secs: u64,
    /// What it was started with.
    pub input: ExecutionInput,
    /// The `Catch` error text (`<Error>: <Cause>`), bounded, when one was caught.
    pub error: Option<String>,
}

/// An EventBridge status-change event, reduced to what the finalizer trusts.
///
/// The event's own `input` is deliberately ignored: the execution input is read
/// with `DescribeExecution`, so a forged event cannot name another run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEvent {
    /// The machine the event names.
    pub state_machine_arn: String,
    /// The execution it names.
    pub execution_arn: String,
    /// `ABORTED`, `TIMED_OUT` or `FAILED`.
    pub status: String,
}

/// The three shapes the finalizer accepts; anything else is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizerInput {
    /// The state machine's `Finalize` payload.
    StateMachine(StateMachinePayload),
    /// An EventBridge status-change event.
    Event(StatusEvent),
    /// The scheduled sweep.
    Reconcile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPayload {
    #[allow(dead_code)]
    source: String,
    execution_arn: String,
    start_time: serde_json::Value,
    input: serde_json::Value,
    #[serde(default)]
    error: Option<RawError>,
}

#[derive(Deserialize)]
struct RawError {
    #[serde(rename = "Error", alias = "error", default)]
    error: Option<String>,
    #[serde(rename = "Cause", alias = "cause", default)]
    cause: Option<String>,
}

#[derive(Deserialize)]
struct RawEvent {
    #[serde(rename = "detail-type")]
    detail_type: String,
    detail: RawDetail,
}

#[derive(Deserialize)]
struct RawDetail {
    #[serde(rename = "executionArn")]
    execution_arn: String,
    #[serde(rename = "stateMachineArn")]
    state_machine_arn: String,
    status: String,
}

/// Days from 1970-01-01 to a civil date (proleptic Gregorian).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Epoch seconds from an RFC 3339 timestamp (`2026-10-28T10:00:00.123Z`, or with a
/// numeric offset).
fn parse_rfc3339_secs(text: &str) -> Option<u64> {
    let (date, rest) = text.split_once(['T', 't'])?;
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (clock, offset_secs) = if let Some(clock) = rest.strip_suffix(['Z', 'z']) {
        (clock, 0_i64)
    } else {
        let at = rest.rfind(['+', '-'])?;
        let (clock, off) = rest.split_at(at);
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let (oh, om) = off[1..].split_once(':')?;
        (
            clock,
            sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60),
        )
    };
    let clock = clock.split('.').next()?;
    let mut hms = clock.split(':');
    let hour: i64 = hms.next()?.parse().ok()?;
    let minute: i64 = hms.next()?.parse().ok()?;
    let second: i64 = hms.next()?.parse().ok()?;
    if hms.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let secs = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second
        - offset_secs;
    u64::try_from(secs).ok()
}

/// A start time as epoch seconds: an RFC 3339 string, or a number of seconds.
fn parse_time_secs(value: &serde_json::Value) -> Option<u64> {
    match value {
        serde_json::Value::String(text) => parse_rfc3339_secs(text),
        serde_json::Value::Number(n) => n.as_u64().or_else(|| {
            n.as_f64()
                .filter(|f| f.is_finite() && *f >= 0.0)
                // Whole seconds are all the TTL comparison needs.
                .map(|f| f.floor() as u64)
        }),
        _ => None,
    }
}

fn parse_execution_input(value: &serde_json::Value) -> Result<ExecutionInput, FinalizerError> {
    let parsed = match value {
        serde_json::Value::String(text) => serde_json::from_str(text),
        other => serde_json::from_value(other.clone()),
    };
    parsed.map_err(|e| refused(format!("the execution input is not the closed input: {e}")))
}

fn bounded_error(error: &RawError) -> String {
    let text = format!(
        "{}: {}",
        error.error.as_deref().unwrap_or("error"),
        error.cause.as_deref().unwrap_or("no cause recorded")
    );
    text.chars().take(MAX_STAGE_ERROR_CHARS).collect()
}

impl FinalizerInput {
    /// Parse one invocation payload into one of the three shapes.
    ///
    /// # Errors
    ///
    /// [`FinalizerError::Refused`] for anything else, naming what is wrong (never
    /// the payload's content).
    pub fn parse(value: &serde_json::Value) -> Result<Self, FinalizerError> {
        let object = value
            .as_object()
            .ok_or_else(|| refused("the input is not a JSON object"))?;
        match object.get("source").and_then(serde_json::Value::as_str) {
            Some("state-machine") => {
                let raw: RawPayload = serde_json::from_value(value.clone())
                    .map_err(|e| refused(format!("not a Finalize payload: {e}")))?;
                let start_secs = parse_time_secs(&raw.start_time)
                    .ok_or_else(|| refused("start_time is not a time"))?;
                Ok(Self::StateMachine(StateMachinePayload {
                    execution_arn: raw.execution_arn,
                    start_secs,
                    input: parse_execution_input(&raw.input)?,
                    error: raw.error.as_ref().map(bounded_error),
                }))
            }
            Some("reconcile") if object.len() == 1 => Ok(Self::Reconcile),
            Some("reconcile") => Err(refused("a reconcile input carries no other field")),
            Some("aws.states") => {
                let raw: RawEvent = serde_json::from_value(value.clone())
                    .map_err(|e| refused(format!("not a status-change event: {e}")))?;
                if raw.detail_type != EVENT_DETAIL_TYPE {
                    return Err(refused(format!(
                        "detail-type {:?} is not {EVENT_DETAIL_TYPE:?}",
                        raw.detail_type
                    )));
                }
                if !matches!(
                    raw.detail.status.as_str(),
                    "ABORTED" | "TIMED_OUT" | "FAILED"
                ) {
                    return Err(refused(format!(
                        "status {} is not finalized by an event",
                        raw.detail.status
                    )));
                }
                Ok(Self::Event(StatusEvent {
                    state_machine_arn: raw.detail.state_machine_arn,
                    execution_arn: raw.detail.execution_arn,
                    status: raw.detail.status,
                }))
            }
            _ => Err(refused(
                "the input is not a Finalize payload, a status event or a reconcile",
            )),
        }
    }
}

// ------------------------------------------------------------------ context

/// Everything the finalizer needs, built so the store and the lock address ONE
/// backend (14-01's rule: the store's own backend is private).
pub struct FinalizerCtx {
    /// The backend the store was built from, and the lock's.
    pub backend: Arc<dyn TaskBackend>,
    /// The task store, built from `backend`, with NO cancel sink: the finalizer's
    /// own `cancel` (an ABORTED event for a still-working record) must not
    /// re-stop an execution that is already over.
    pub store: AprenderTaskStore,
    /// The server bucket's objects.
    pub objects: Arc<dyn ObjectStore>,
    /// Step Functions.
    pub executions: Arc<dyn ExecutionReader>,
    /// SageMaker.
    pub training_jobs: Arc<dyn TrainingJobs>,
    /// ECS.
    pub pack_tasks: Arc<dyn PackTasks>,
    /// The server bucket (for the status payload's `s3://` prefix).
    pub bucket: String,
    /// The one state machine this finalizer serves.
    pub state_machine_arn: String,
    /// Epoch seconds now.
    pub now: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl std::fmt::Debug for FinalizerCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FinalizerCtx").finish_non_exhaustive()
    }
}

impl FinalizerCtx {
    /// The one constructor: builds the store from `backend` and keeps the same `Arc`.
    #[must_use]
    pub fn new(
        backend: Arc<dyn TaskBackend>,
        objects: Arc<dyn ObjectStore>,
        executions: Arc<dyn ExecutionReader>,
        training_jobs: Arc<dyn TrainingJobs>,
        pack_tasks: Arc<dyn PackTasks>,
        bucket: impl Into<String>,
        state_machine_arn: impl Into<String>,
    ) -> Self {
        Self {
            store: AprenderTaskStore::new(Arc::clone(&backend)),
            backend,
            objects,
            executions,
            training_jobs,
            pack_tasks,
            bucket: bucket.into(),
            state_machine_arn: state_machine_arn.into(),
            now: Arc::new(lock::now_secs),
        }
    }

    /// Replace the clock (tests).
    #[must_use]
    pub fn with_clock(mut self, now: Arc<dyn Fn() -> u64 + Send + Sync>) -> Self {
        self.now = now;
        self
    }

    fn now(&self) -> u64 {
        (self.now)()
    }
}

// ------------------------------------------------------------------ reports

/// What one finalization did. Carries no credential and no object content.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FinalizeReport {
    /// The task finalized (empty for a sweep that found nothing).
    pub task_id: String,
    /// The phase of the winning payload; `None` while compute is awaited.
    pub phase: Option<Phase>,
    /// True when the objects were (re-)tagged `laya-retention=keep`.
    pub keep: bool,
    /// How many objects were tagged and read back.
    pub tagged: usize,
    /// True when `StopTrainingJob` was issued.
    pub stopped_training_job: bool,
    /// How many ECS `StopTask` calls were issued.
    pub stopped_pack_tasks: usize,
    /// Relative paths a writer manifest listed that S3 does not hold, dropped
    /// before the decision (D-11). Paths only, never object content.
    pub dropped: Vec<String>,
    /// True when compute was not terminal: nothing was written, the lock is held.
    pub awaiting_compute: bool,
    /// How the record was settled: `finished`, `attached`, `reused` or `record_missing`.
    pub record: String,
    /// True when this call released the lock.
    pub released: bool,
    /// True when this invocation's decision let the records decide `completed` or
    /// `gate_failed` over a state-machine stage error (D-08); the payload then
    /// carries the `stage-error-overridden` note.
    pub stage_error_overridden: bool,
    /// The sweep's own account, when the input was a reconcile.
    pub reconcile: Option<Box<ReconcileReport>>,
}

/// What one sweep did.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ReconcileReport {
    /// The lock holder's task id, when the lock was held.
    pub holder: Option<String>,
    /// `lock-free`, `running`, `cancel-retry`, `dispatch-grace`,
    /// `dispatch-unconfirmed`, `awaiting-compute` or `finalized`.
    pub action: String,
    /// The holder's finalization, when the sweep ran one.
    pub finalized: Option<FinalizeReport>,
    /// True when the sweep re-issued `StopExecution` for the holder.
    pub stopped_execution: bool,
    /// Executions that were not the holder's and were stopped.
    pub rogue_stopped: Vec<String>,
}

// ------------------------------------------------------------- run objects

/// What the jobs wrote, parsed.
#[derive(Default)]
struct RunObjects {
    outcome: Option<Outcome>,
    gate_report: Option<serde_json::Value>,
    pack_outcome: Option<PackOutcome>,
    training_manifest: Option<TrainingManifest>,
    pack_manifest: Vec<ManifestEntry>,
    /// Every path a writer manifest lists, the `.apr` included.
    writer_paths: BTreeSet<String>,
    /// Paths a writer manifest listed that S3 does not hold, removed from the
    /// entries above (sorted, de-duplicated).
    dropped: Vec<String>,
}

/// Read one JSON object under the byte cap. Absent, oversize and unparseable all read
/// as `None` (the decision then says the file is missing); only a failed call is an error.
async fn read_json<T: serde::de::DeserializeOwned>(
    ctx: &FinalizerCtx,
    key: &str,
) -> Result<Option<T>, FinalizerError> {
    match ctx.objects.get(key, MAX_JSON_BYTES).await {
        Ok(None) => Ok(None),
        Ok(Some(bytes)) => match serde_json::from_slice::<T>(&bytes) {
            Ok(value) => Ok(Some(value)),
            Err(_) => {
                tracing::warn!(
                    "laya finalizer: {key} is not the expected JSON; treated as missing"
                );
                Ok(None)
            }
        },
        Err(ObjectError::Oversize) => {
            tracing::warn!(
                "laya finalizer: {key} exceeds {MAX_JSON_BYTES} bytes; treated as missing"
            );
            Ok(None)
        }
        Err(ObjectError::Failed(message)) => Err(store_err(format!("reading {key}: {message}"))),
    }
}

async fn read_run_objects(
    ctx: &FinalizerCtx,
    prefix: &str,
    task_id: &str,
) -> Result<RunObjects, FinalizerError> {
    // A file that names another task is another run's file: treated as missing.
    let own = |actual: &str, key: &str| {
        let ok = actual == task_id;
        if !ok {
            tracing::warn!("laya finalizer: {key} names another task; treated as missing");
        }
        ok
    };
    let key = |name: &str| format!("{prefix}{name}");

    let outcome: Option<Outcome> = read_json(ctx, &key(layout::OUTCOME))
        .await?
        .filter(|o: &Outcome| own(&o.task_id, layout::OUTCOME));
    let gate_report =
        read_json(ctx, &format!("{prefix}{}gate-report.json", layout::RUN_DIR)).await?;
    let pack_outcome: Option<PackOutcome> = read_json(ctx, &key(layout::PACK_OUTCOME))
        .await?
        .filter(|p: &PackOutcome| own(&p.task_id, layout::PACK_OUTCOME));
    let mut training_manifest: Option<TrainingManifest> =
        read_json(ctx, &key(layout::TRAINING_MANIFEST))
            .await?
            .filter(|m: &TrainingManifest| own(&m.task_id, layout::TRAINING_MANIFEST));
    if training_manifest.is_none() {
        training_manifest = read_json(ctx, &key(layout::PARTIAL_MANIFEST))
            .await?
            .filter(|m: &TrainingManifest| own(&m.task_id, layout::PARTIAL_MANIFEST));
    }
    let pack_manifest: Vec<ManifestEntry> = read_json(ctx, &key(layout::PACK_MANIFEST))
        .await?
        .unwrap_or_default();

    let writer_paths = writer_paths_of(training_manifest.as_ref(), &pack_manifest);
    Ok(RunObjects {
        outcome,
        gate_report,
        pack_outcome,
        training_manifest,
        pack_manifest,
        writer_paths,
        dropped: Vec::new(),
    })
}

/// Every path the writer manifests list, the `.apr` included.
fn writer_paths_of(
    training_manifest: Option<&TrainingManifest>,
    pack_manifest: &[ManifestEntry],
) -> BTreeSet<String> {
    training_manifest
        .iter()
        .flat_map(|m| m.entries.iter())
        .chain(pack_manifest.iter())
        .map(|entry| entry.path.clone())
        .collect()
}

/// D-11 (WR-02): finalization is total over the shapes of the run's objects. A
/// job lists `outcome.json` and `pack-outcome.json` BEFORE it uploads them, so a
/// failed last upload leaves an entry whose object S3 does not hold. Every such
/// entry is dropped here, once, from one listing of the run prefix, so the
/// decision, the stored payload, manifest.json, the read-time links and the tag
/// targets are all built from objects that exist; the dropped paths are kept for
/// the report and the payload's note. A listing failure is a transient store error
/// (the lock stays held and the next pass retries), like every other call.
async fn drop_absent_entries(
    ctx: &FinalizerCtx,
    prefix: &str,
    objects: &mut RunObjects,
) -> Result<(), FinalizerError> {
    let listed = ctx
        .objects
        .list(prefix)
        .await
        .map_err(|e| store_err(format!("listing {prefix}: {e}")))?;
    let present: BTreeSet<&str> = listed
        .iter()
        .filter_map(|(key, _)| key.strip_prefix(prefix))
        .collect();
    let mut dropped: BTreeSet<String> = BTreeSet::new();
    let mut exists = |entry: &ManifestEntry| {
        let found = present.contains(entry.path.as_str());
        if !found {
            dropped.insert(entry.path.clone());
        }
        found
    };
    if let Some(manifest) = objects.training_manifest.as_mut() {
        manifest.entries.retain(&mut exists);
    }
    objects.pack_manifest.retain(&mut exists);
    objects.writer_paths =
        writer_paths_of(objects.training_manifest.as_ref(), &objects.pack_manifest);
    objects.dropped = dropped.into_iter().collect();
    Ok(())
}

// ------------------------------------------------------------------ helpers

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn result_text(result: &CallToolResult) -> Option<&str> {
    match result.content.first() {
        Some(Content::Text { text }) => Some(text.as_str()),
        _ => None,
    }
}

/// The payload that won, as the exact text that was stored.
struct Winner {
    text: String,
    payload: StatusPayload,
}

impl Winner {
    fn from_result(result: &CallToolResult, task_id: &str) -> Result<Self, FinalizerError> {
        let text = result_text(result)
            .ok_or_else(|| integrity("the stored result is not a text payload"))?;
        let payload: StatusPayload = serde_json::from_str(text)
            .map_err(|_| integrity("the stored result is not a status payload"))?;
        if payload.task_id != task_id {
            return Err(integrity("the stored result names another task"));
        }
        Ok(Self {
            text: text.to_string(),
            payload,
        })
    }

    fn from_decision(decision: &FinalizeDecision) -> Self {
        Self {
            text: decision.payload.to_value().to_string(),
            payload: decision.payload.clone(),
        }
    }

    /// D-19: only a completed run whose gate passed is kept.
    fn keep(&self) -> bool {
        self.payload.phase == Phase::Completed
            && self.payload.verdict.as_ref().is_some_and(|v| v.pass)
    }
}

/// Who a finalization is about.
struct Subject {
    task_id: String,
    owner: String,
    name: String,
    /// `runs/<name>/<task_id>/`.
    run_prefix: String,
    /// When the execution started, in epoch seconds (the lock's, for an orphan).
    start_secs: u64,
}

struct Trigger {
    subject: Subject,
    /// `None` for the unconfirmed-dispatch path, where no execution ever ran.
    exec: Option<ExecutionInput>,
    stage_error: Option<String>,
    /// True for an `ABORTED` execution.
    aborted: bool,
}

fn subject_from(exec: &ExecutionInput, start_secs: u64) -> Result<Subject, FinalizerError> {
    if exec.schema != SCHEMA_EXECUTION_INPUT {
        return Err(refused("the execution input has another schema"));
    }
    if !segment_ok(&exec.name) || !segment_ok(&exec.task_id) {
        return Err(refused("the execution input names an unsafe path segment"));
    }
    if exec.run_prefix != layout::run_prefix(&exec.name, &exec.task_id) {
        return Err(refused("run_prefix is not runs/<name>/<task_id>/"));
    }
    Ok(Subject {
        task_id: exec.task_id.clone(),
        owner: exec.owner.clone(),
        name: exec.name.clone(),
        run_prefix: exec.run_prefix.clone(),
        start_secs,
    })
}

async fn read_record(
    ctx: &FinalizerCtx,
    subject: &Subject,
) -> Result<Option<StoredTask>, FinalizerError> {
    ctx.backend
        .get(&subject.owner, &subject.task_id)
        .await
        .map_err(|e| store_err(format!("reading the task record: {e}")))
}

/// The decision for a run that never started: a failed `dispatch-unconfirmed`, or
/// a cancelled one when the client had already cancelled it.
fn orphan_decision(ctx: &FinalizerCtx, subject: &Subject, aborted: bool) -> FinalizeDecision {
    let uri = format!("s3://{}/{}", ctx.bucket, subject.run_prefix);
    let (phase, status) = if aborted {
        (Phase::Cancelled, TaskStatus::Cancelled)
    } else {
        (Phase::Failed, TaskStatus::Failed)
    };
    let mut payload = StatusPayload::base(&subject.task_id, &subject.name, &uri, phase);
    if !aborted {
        payload.error = Some(
            "dispatch-unconfirmed: no execution was found for this task after the confirmation grace"
                .to_string(),
        );
    }
    FinalizeDecision {
        phase,
        status,
        is_error: !aborted,
        payload,
        manifest: Manifest {
            schema: SCHEMA_MANIFEST.to_string(),
            task_id: subject.task_id.clone(),
            name: subject.name.clone(),
            images: Images {
                train: None,
                pack: None,
            },
            contracts: Contracts {
                prefilter_gate: None,
                train_gate: None,
                pack_gate: None,
                pack_parity: None,
            },
            entries: Vec::new(),
            unlisted: 0,
        },
        keep: false,
        manifest_partial: false,
        stage_error_overridden: false,
    }
}

fn decide(
    ctx: &FinalizerCtx,
    trigger: &Trigger,
    objects: &RunObjects,
    aborted: bool,
) -> FinalizeDecision {
    match &trigger.exec {
        None => orphan_decision(ctx, &trigger.subject, aborted),
        Some(exec) => finalize_run(&FinalizeInput {
            bucket: ctx.bucket.clone(),
            execution: exec.clone(),
            stage_error: trigger.stage_error.clone(),
            aborted,
            outcome: objects.outcome.clone(),
            gate_report: objects.gate_report.clone(),
            pack_outcome: objects.pack_outcome.clone(),
            training_manifest: objects.training_manifest.clone(),
            pack_manifest: objects.pack_manifest.clone(),
            dropped: objects.dropped.clone(),
        }),
    }
}

// --------------------------------------------------------------- the record

/// Settle the DynamoDB record, the arbiter, and return the payload that won.
///
/// (a) terminal with a stored result: that result is the decision;
/// (b) still working: `finish` it with `decision`;
/// (c) terminal with NO result: a client `tasks/cancel` leaves exactly this, so a
///     `Cancelled` record gets `aborted_decision` attached by compare-and-swap
///     (result only when none is stored); any other status is an integrity fault.
///
/// A `finish` that no-ops because a cancel landed after the first read leaves
/// (Cancelled, no result), which is (c) on the next pass; an attach that answers
/// `AlreadyPresent` means another invocation won, so its payload is reused.
async fn settle_record(
    ctx: &FinalizerCtx,
    subject: &Subject,
    mut record: StoredTask,
    decision: &FinalizeDecision,
    aborted_decision: &FinalizeDecision,
) -> Result<(Winner, &'static str), FinalizerError> {
    let mut how = "reused";
    for _ in 0..3 {
        if let Some(result) = &record.result {
            return Winner::from_result(result, &subject.task_id).map(|w| (w, how));
        }
        if !record.task.status.is_terminal() {
            ctx.store
                .finish(
                    &subject.task_id,
                    &subject.owner,
                    decision.status,
                    decision_result(decision),
                )
                .await
                .map_err(|e| store_err(format!("finishing the task: {e}")))?;
            how = "finished";
        } else if record.task.status == TaskStatus::Cancelled {
            let attached = attach_result_if_absent(
                ctx.backend.as_ref(),
                &subject.owner,
                &subject.task_id,
                decision_result(aborted_decision),
            )
            .await
            .map_err(|e| store_err(format!("attaching the cancelled payload: {e}")))?;
            match attached {
                AttachOutcome::Attached => how = "attached",
                AttachOutcome::AlreadyPresent => {}
                AttachOutcome::NotTerminal | AttachOutcome::NotFound => {
                    return Err(integrity("the record changed under the attach"));
                }
            }
        } else {
            return Err(integrity(format!(
                "task {} is {:?} with no stored result",
                subject.task_id, record.task.status
            )));
        }
        record = read_record(ctx, subject)
            .await?
            .ok_or_else(|| integrity("the task record vanished while it was settled"))?;
    }
    Err(integrity("the task record could not be settled"))
}

// -------------------------------------------------------------- publication

/// Write `bytes` at `key` unless the object already holds exactly them.
async fn put_if_different(
    ctx: &FinalizerCtx,
    key: &str,
    bytes: &[u8],
) -> Result<(), FinalizerError> {
    match ctx.objects.get(key, MAX_JSON_BYTES).await {
        Ok(Some(existing)) if existing == bytes => return Ok(()),
        Ok(_) | Err(ObjectError::Oversize) => {}
        Err(ObjectError::Failed(message)) => {
            return Err(store_err(format!("reading {key}: {message}")))
        }
    }
    ctx.objects
        .put(key, bytes.to_vec(), RETENTION_EXPIRE)
        .await
        .map_err(|e| store_err(format!("writing {key}: {e}")))
}

/// Steps 3 to 6: verdict.json, manifest.json, the tags, the lock. Idempotent: every
/// step that already holds its end state writes nothing.
async fn publish(
    ctx: &FinalizerCtx,
    subject: &Subject,
    winner: &Winner,
    base: &Manifest,
    writer_paths: &BTreeSet<String>,
    report: &mut FinalizeReport,
) -> Result<(), FinalizerError> {
    let prefix = &subject.run_prefix;

    // (3) verdict.json: the winning payload's own text, byte for byte.
    let verdict_bytes = winner.text.clone().into_bytes();
    let verdict_key = format!("{prefix}{}", layout::VERDICT);
    put_if_different(ctx, &verdict_key, &verdict_bytes).await?;

    // (4) manifest.json: every writer entry the decision kept, plus verdict.json.
    // Written once: a valid manifest.json that already lists exactly this
    // verdict.json is the first writer's and is kept as it is.
    let manifest_key = format!("{prefix}{}", layout::MANIFEST);
    let verdict_entry = ManifestEntry {
        path: layout::VERDICT.to_string(),
        size: verdict_bytes.len() as u64,
        sha256: sha256_hex(&verdict_bytes),
    };
    let adopted = match ctx.objects.get(&manifest_key, MAX_JSON_BYTES).await {
        Ok(Some(bytes)) => serde_json::from_slice::<Manifest>(&bytes).ok().filter(|m| {
            m.task_id == subject.task_id && m.entries.iter().any(|e| *e == verdict_entry)
        }),
        Ok(None) | Err(ObjectError::Oversize) => None,
        Err(ObjectError::Failed(message)) => {
            return Err(store_err(format!("reading {manifest_key}: {message}")))
        }
    };
    let manifest = if let Some(existing) = adopted {
        existing
    } else {
        let mut entries: BTreeMap<String, ManifestEntry> = winner
            .payload
            .manifest
            .iter()
            .map(|link| {
                (
                    link.path.clone(),
                    ManifestEntry {
                        path: link.path.clone(),
                        size: link.size,
                        sha256: link.sha256.clone(),
                    },
                )
            })
            .collect();
        entries.insert(layout::VERDICT.to_string(), verdict_entry);
        let listed = ctx
            .objects
            .list(prefix)
            .await
            .map_err(|e| store_err(format!("listing {prefix}: {e}")))?;
        let unlisted = listed
            .iter()
            .filter_map(|(key, _)| key.strip_prefix(prefix.as_str()))
            .filter(|path| !writer_paths.contains(*path) && !SELF_LISTING.contains(path))
            .count() as u64;
        let fresh = Manifest {
            schema: SCHEMA_MANIFEST.to_string(),
            task_id: subject.task_id.clone(),
            name: subject.name.clone(),
            images: base.images.clone(),
            contracts: base.contracts.clone(),
            entries: entries.into_values().collect(),
            unlisted,
        };
        let manifest_bytes = serde_json::to_vec_pretty(&fresh)
            .map_err(|e| store_err(format!("serializing manifest.json: {e}")))?;
        put_if_different(ctx, &manifest_key, &manifest_bytes).await?;
        fresh
    };

    // (5) retention tags, each read back (D-19). The targets are every manifest.json
    // entry, manifest.json, and every self-listing inventory (IN-01: a kept run keeps
    // its inventories too), restricted to what a listing taken AFTER verdict.json
    // and manifest.json were written holds (D-11): a key S3 does not hold cannot be
    // tagged, and an adopted manifest.json, written by an earlier pass, may name an
    // object that has gone since. Never a key no inventory names.
    let keep = winner.keep();
    let want = if keep {
        RETENTION_KEEP
    } else {
        RETENTION_EXPIRE
    };
    let present: BTreeSet<String> = ctx
        .objects
        .list(prefix)
        .await
        .map_err(|e| store_err(format!("listing {prefix}: {e}")))?
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    let mut targets: BTreeSet<String> = manifest
        .entries
        .iter()
        .map(|e| format!("{prefix}{}", e.path))
        .collect();
    targets.insert(manifest_key);
    targets.extend(SELF_LISTING.iter().map(|name| format!("{prefix}{name}")));
    targets.retain(|key| present.contains(key));
    for key in &targets {
        let current = ctx
            .objects
            .get_tag(key)
            .await
            .map_err(|e| store_err(format!("reading the tag of {key}: {e}")))?;
        if current.as_deref() != Some(want) {
            ctx.objects
                .put_tag(key, want)
                .await
                .map_err(|e| store_err(format!("tagging {key}: {e}")))?;
        }
    }
    for key in &targets {
        let found = ctx
            .objects
            .get_tag(key)
            .await
            .map_err(|e| store_err(format!("reading back the tag of {key}: {e}")))?;
        if found.as_deref() != Some(want) {
            return Err(store_err(format!("{key} did not read back as {want}")));
        }
    }
    report.keep = keep;
    report.tagged = targets.len();

    // (6) the lock, LAST, and only when it is really free afterwards.
    report.released = lock::release(ctx.backend.as_ref(), &subject.task_id).await;
    if let Some(state) = lock::read(ctx.backend.as_ref()).await {
        if state.holder == subject.task_id && !state.released {
            return Err(store_err("the single-flight lock could not be released"));
        }
    }
    Ok(())
}

// --------------------------------------------------------- one finalization

async fn finalize_task(
    ctx: &FinalizerCtx,
    trigger: Trigger,
) -> Result<FinalizeReport, FinalizerError> {
    let subject = &trigger.subject;
    let mut report = FinalizeReport {
        task_id: subject.task_id.clone(),
        ..FinalizeReport::default()
    };

    // The lock's recorded owner must be the run's owner, or the records disagree.
    if let Some(state) = lock::read(ctx.backend.as_ref()).await {
        if state.holder == subject.task_id && !state.released && state.holder_owner != subject.owner
        {
            return Err(integrity(format!(
                "the lock holder's owner differs from the owner of task {}",
                subject.task_id
            )));
        }
    }

    // NotFound is split, never conflated: an absent record is EXPIRED only when the
    // execution is older than the task TTL; inside the TTL it is an integrity fault.
    let mut record = read_record(ctx, subject).await?;
    let expired = record.is_none();
    if expired
        && (trigger.exec.is_none()
            || ctx.now() <= subject.start_secs.saturating_add(TASK_TTL_MS / 1000))
    {
        return Err(integrity(format!(
            "task {} has no record inside its TTL",
            subject.task_id
        )));
    }

    // An ABORTED execution for a record nobody cancelled: readers must see
    // `cancelled` at once, whatever compute is still doing.
    if trigger.aborted {
        if let Some(rec) = &record {
            if !rec.task.status.is_terminal() {
                if let Err(e) = ctx.store.cancel(&subject.task_id, &subject.owner).await {
                    tracing::warn!("laya finalizer: cancel of {} failed: {e}", subject.task_id);
                }
                record = read_record(ctx, subject).await?;
            }
        }
    }

    // The compute gate (D-11, D-12): outputs are read, and the lock released, only
    // when nothing of the run is still billing.
    let state = compute_terminal(ctx, &subject.task_id)
        .await
        .map_err(FinalizerError::Compute)?;
    if !state.terminal {
        let stopped = stop_live_compute(ctx, &subject.task_id, &state).await;
        report.stopped_training_job = stopped.training_job;
        report.stopped_pack_tasks = stopped.pack_tasks;
        report.awaiting_compute = true;
        if stopped.errors.is_empty() {
            return Ok(report);
        }
        return Err(FinalizerError::Compute(stopped.errors.join("; ")));
    }

    let objects = if trigger.exec.is_some() {
        let mut objects = read_run_objects(ctx, &subject.run_prefix, &subject.task_id).await?;
        drop_absent_entries(ctx, &subject.run_prefix, &mut objects).await?;
        objects
    } else {
        RunObjects::default()
    };
    if !objects.dropped.is_empty() {
        tracing::warn!(
            "laya finalizer: task {} lists {} object(s) S3 does not hold; dropped from the inventory",
            subject.task_id,
            objects.dropped.len()
        );
    }
    report.dropped.clone_from(&objects.dropped);
    let cancelled = record
        .as_ref()
        .is_some_and(|rec| rec.task.status == TaskStatus::Cancelled);
    let decision = decide(ctx, &trigger, &objects, trigger.aborted || cancelled);
    let aborted_decision = decide(ctx, &trigger, &objects, true);
    report.stage_error_overridden = decision.stage_error_overridden;
    if decision.stage_error_overridden {
        tracing::warn!(
            "laya finalizer: task {} reported a stage error, but its records decided {}",
            subject.task_id,
            decision.phase.as_str()
        );
    }

    let winner = match record {
        Some(rec) => {
            let (winner, how) =
                settle_record(ctx, subject, rec, &decision, &aborted_decision).await?;
            report.record = how.to_string();
            winner
        }
        None => {
            report.record = "record_missing".to_string();
            // First writer wins in S3 as well: an existing verdict.json is the decision.
            let key = format!("{}{}", subject.run_prefix, layout::VERDICT);
            match ctx.objects.get(&key, MAX_JSON_BYTES).await {
                Ok(Some(bytes)) => {
                    let text = String::from_utf8(bytes)
                        .map_err(|_| integrity("the existing verdict.json is not text"))?;
                    let payload: StatusPayload = serde_json::from_str(&text)
                        .map_err(|_| integrity("the existing verdict.json is not a payload"))?;
                    Winner { text, payload }
                }
                Ok(None) | Err(ObjectError::Oversize) => Winner::from_decision(&decision),
                Err(ObjectError::Failed(message)) => {
                    return Err(store_err(format!("reading {key}: {message}")))
                }
            }
        }
    };
    report.phase = Some(winner.payload.phase);
    publish(
        ctx,
        subject,
        &winner,
        &decision.manifest,
        &objects.writer_paths,
        &mut report,
    )
    .await?;
    Ok(report)
}

// ------------------------------------------------------------------- handle

/// The state machine this finalizer serves owns exactly the executions
/// `laya-<task_id>`; anything else is refused.
fn check_execution(
    ctx: &FinalizerCtx,
    exec: &ExecutionInput,
    execution_arn_seen: &str,
) -> Result<(), FinalizerError> {
    let derived = execution_arn(&ctx.state_machine_arn, &exec.task_id)
        .ok_or_else(|| refused("the configured state machine ARN is malformed"))?;
    if derived != execution_arn_seen {
        return Err(refused(
            "the execution is not laya-<task_id> of the configured state machine",
        ));
    }
    Ok(())
}

/// Finalize one invocation. See the module doc for the order and the three shapes.
///
/// # Errors
///
/// [`FinalizerError::Refused`] for a shape or machine that is not this finalizer's
/// (nothing written); [`FinalizerError::Integrity`] when the records disagree
/// (nothing written, lock untouched); [`FinalizerError::Store`] and
/// [`FinalizerError::Compute`] when a step failed (lock held, retry or sweep resumes).
pub async fn handle(
    ctx: &FinalizerCtx,
    input: FinalizerInput,
) -> Result<FinalizeReport, FinalizerError> {
    match input {
        FinalizerInput::StateMachine(payload) => {
            check_execution(ctx, &payload.input, &payload.execution_arn)?;
            let subject = subject_from(&payload.input, payload.start_secs)?;
            finalize_task(
                ctx,
                Trigger {
                    subject,
                    exec: Some(payload.input),
                    stage_error: payload.error,
                    aborted: false,
                },
            )
            .await
        }
        FinalizerInput::Event(event) => {
            if event.state_machine_arn != ctx.state_machine_arn {
                return Err(refused("the event is for another state machine"));
            }
            let info = ctx
                .executions
                .describe(&event.execution_arn)
                .await
                .map_err(FinalizerError::Compute)?
                .ok_or_else(|| refused("the event names an execution that does not exist"))?;
            if !info.is_terminal() {
                return Err(refused(
                    "the event names an execution that is still running",
                ));
            }
            let exec: ExecutionInput =
                parse_execution_input(&serde_json::Value::String(info.input.clone()))?;
            check_execution(ctx, &exec, &event.execution_arn)?;
            let subject = subject_from(&exec, info.start_secs)?;
            finalize_task(
                ctx,
                Trigger {
                    subject,
                    exec: Some(exec),
                    stage_error: None,
                    aborted: event.status == "ABORTED",
                },
            )
            .await
        }
        FinalizerInput::Reconcile => {
            let sweep = reconcile(ctx).await?;
            let mut report = sweep.finalized.clone().unwrap_or_default();
            report.reconcile = Some(Box::new(sweep));
            Ok(report)
        }
    }
}

// ---------------------------------------------------------------- reconcile

/// The execution name's task id when `arn` is `...:execution:<machine>:laya-<id>`.
fn execution_name(arn: &str) -> Option<&str> {
    arn.rsplit(':').next()
}

/// The scheduled sweep (every 10 minutes, plan 14-08): makes best-effort event
/// delivery and the best-effort cancel sink durable.
///
/// It starts from the lock. Free: nothing. Held by T: `DescribeExecution laya-T` —
/// absent past [`DISPATCH_CONFIRM_GRACE_SECS`] is a failed `dispatch-unconfirmed`;
/// `RUNNING` with T cancelled re-issues `StopExecution` and the residual compute
/// stops (a durable cancel); `RUNNING` otherwise is left alone; terminal goes
/// through the same path as that status's event (a lost event, a failed Finalize
/// invocation, or a crash between the record and the release). Then every RUNNING
/// execution of the machine that is not the holder's is stopped (a rogue start).
///
/// # Errors
///
/// The holder's finalization error, or a failed stop, after the rest of the sweep
/// has run.
pub async fn reconcile(ctx: &FinalizerCtx) -> Result<ReconcileReport, FinalizerError> {
    let mut report = ReconcileReport::default();
    // The RUNNING list is taken BEFORE the lock is read. `train` takes the lock, then
    // starts the execution, so every execution in this snapshot already had its lock
    // taken before the read below and is named by it (or has since finished). Reading
    // the lock first would let a `train` that starts between the two calls look rogue.
    let running = ctx.executions.list_running(&ctx.state_machine_arn).await;
    let lock_state = lock::read(ctx.backend.as_ref())
        .await
        .filter(|state| !state.released);
    report.holder = lock_state.as_ref().map(|state| state.holder.clone());
    report.action = "lock-free".to_string();

    let holder_result = match &lock_state {
        Some(state) => reconcile_holder(ctx, state, &mut report).await,
        None => Ok(()),
    };

    // Rogue executions: RUNNING, and not the lock holder's.
    let mut stop_errors: Vec<String> = Vec::new();
    match running {
        Ok(running) => {
            let holder_name = report
                .holder
                .as_ref()
                .map(|id| format!("{}{id}", crate::sfn::EXECUTION_NAME_PREFIX));
            for arn in running {
                if holder_name.as_deref() == execution_name(&arn) {
                    continue;
                }
                tracing::warn!("laya finalizer: stopping a rogue execution {arn}");
                match ctx.executions.stop(&arn).await {
                    Ok(()) => report.rogue_stopped.push(arn),
                    Err(e) => stop_errors.push(format!("StopExecution {arn}: {e}")),
                }
            }
        }
        Err(e) => stop_errors.push(format!("ListExecutions: {e}")),
    }

    holder_result?;
    if stop_errors.is_empty() {
        Ok(report)
    } else {
        Err(FinalizerError::Compute(stop_errors.join("; ")))
    }
}

async fn reconcile_holder(
    ctx: &FinalizerCtx,
    state: &lock::LockState,
    report: &mut ReconcileReport,
) -> Result<(), FinalizerError> {
    let holder = &state.holder;
    let arn = execution_arn(&ctx.state_machine_arn, holder)
        .ok_or_else(|| refused("the configured state machine ARN is malformed"))?;
    let info = ctx
        .executions
        .describe(&arn)
        .await
        .map_err(FinalizerError::Compute)?;

    let Some(info) = info else {
        // No execution of that name.
        let age = ctx.now().saturating_sub(state.acquired_at_secs);
        if age <= DISPATCH_CONFIRM_GRACE_SECS {
            report.action = "dispatch-grace".to_string();
            return Ok(());
        }
        report.action = "dispatch-unconfirmed".to_string();
        let Some(subject) = orphan_subject(ctx, state).await? else {
            return Ok(());
        };
        let finalized = finalize_task(
            ctx,
            Trigger {
                subject,
                exec: None,
                stage_error: None,
                aborted: false,
            },
        )
        .await?;
        report.finalized = Some(finalized);
        return Ok(());
    };

    if !info.is_terminal() {
        // Running: nothing, unless the client cancelled and the stop was lost.
        let record = ctx
            .backend
            .get(&state.holder_owner, holder)
            .await
            .map_err(|e| store_err(format!("reading the holder's record: {e}")))?;
        let cancelled = record.is_some_and(|rec| rec.task.status == TaskStatus::Cancelled);
        if !cancelled {
            report.action = "running".to_string();
            return Ok(());
        }
        report.action = "cancel-retry".to_string();
        let mut errors = Vec::new();
        match ctx.executions.stop(&arn).await {
            Ok(()) => report.stopped_execution = true,
            Err(e) => errors.push(format!("StopExecution: {e}")),
        }
        let compute = compute_terminal(ctx, holder)
            .await
            .map_err(FinalizerError::Compute)?;
        errors.extend(stop_live_compute(ctx, holder, &compute).await.errors);
        return if errors.is_empty() {
            Ok(())
        } else {
            Err(FinalizerError::Compute(errors.join("; ")))
        };
    }

    // Terminal: the same path as that status's event.
    let exec = parse_execution_input(&serde_json::Value::String(info.input.clone()))?;
    check_execution(ctx, &exec, &arn)?;
    let subject = subject_from(&exec, info.start_secs)?;
    let finalized = finalize_task(
        ctx,
        Trigger {
            subject,
            exec: Some(exec),
            stage_error: None,
            aborted: info.status == "ABORTED",
        },
    )
    .await?;
    report.action = if finalized.awaiting_compute {
        "awaiting-compute"
    } else {
        "finalized"
    }
    .to_string();
    report.finalized = Some(finalized);
    Ok(())
}

/// The envelope `submit` stashed on the holder's record: the run's name and prefix.
#[derive(Deserialize)]
struct StoredEnvelope {
    name: String,
}

/// Who the unconfirmed-dispatch path is about, from the lock and the holder's record.
async fn orphan_subject(
    ctx: &FinalizerCtx,
    state: &lock::LockState,
) -> Result<Option<Subject>, FinalizerError> {
    let record = ctx
        .backend
        .get(&state.holder_owner, &state.holder)
        .await
        .map_err(|e| store_err(format!("reading the holder's record: {e}")))?;
    let Some(record) = record else {
        // Gone: the lock's liveness check already reads it as free.
        return Ok(None);
    };
    let name = record
        .envelope
        .and_then(|value| serde_json::from_value::<StoredEnvelope>(value).ok())
        .map(|e| e.name)
        .filter(|name| segment_ok(name))
        .ok_or_else(|| integrity("the unconfirmed holder's record carries no usable name"))?;
    Ok(Some(Subject {
        task_id: state.holder.clone(),
        owner: state.holder_owner.clone(),
        run_prefix: layout::run_prefix(&name, &state.holder),
        name,
        start_secs: state.acquired_at_secs,
    }))
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;

    use aprender_mcp_laya_train::lock::LOCK_OWNER;
    use aprender_mcp_laya_train::{BackendError, CancelSink, InMemoryTaskBackend};
    use pmcp::server::task_store::TaskStoreError;
    use pmcp::types::tasks::Task;
    use serde_json::{json, Value};

    use super::*;
    use crate::compute::fakes::{FakeExecutionReader, FakeJobs, FakePacks};
    use crate::compute::training_job_name;
    use crate::objects::fakes::{FakeObjects, Op};

    const OUTCOME: &str = include_str!("../../aprender-mcp-laya-train/fixtures/outcome-v1.json");
    const TRAINING_MANIFEST: &str =
        include_str!("../../aprender-mcp-laya-train/fixtures/training-manifest-v1.json");
    const PACK_OUTCOME: &str =
        include_str!("../../aprender-mcp-laya-train/fixtures/pack-outcome-v1.json");
    const GATE_REPORT: &str =
        include_str!("../../aprender-mcp-laya-train/fixtures/gate-report-v1.json");

    const TASK_ID: &str = "00000000-0000-4000-8000-000000000001";
    const NAME: &str = "fixture-model";
    const OWNER: &str = "alice";
    const BUCKET: &str = "fixture-bucket";
    const MACHINE: &str = "arn:aws:states:us-east-1:123456789012:stateMachine:laya-train-dev";
    const T0: u64 = 1_800_000_000;
    const TTL_SECS: u64 = TASK_TTL_MS / 1000;

    fn sha(label: &str) -> String {
        sha256_hex(label.as_bytes())
    }

    fn parse(text: &str) -> Value {
        serde_json::from_str(text).expect("fixture json")
    }

    // ------------------------------------------------------------ the backend

    /// An in-memory backend that logs the finalizer's record and release writes
    /// into the shared log, and can fail the next lock release or run a client
    /// cancel just before the finalizer's `finish` applies.
    struct TestBackend {
        inner: Arc<InMemoryTaskBackend>,
        log: Arc<Mutex<Vec<String>>>,
        fail_release: AtomicBool,
        cancel_race: AtomicBool,
    }

    impl TestBackend {
        fn push(&self, line: &str) {
            self.log.lock().expect("lock").push(line.to_string());
        }
    }

    #[pmcp::async_trait]
    impl TaskBackend for TestBackend {
        async fn put_new(&self, owner_id: &str, record: StoredTask) -> Result<(), BackendError> {
            self.inner.put_new(owner_id, record).await
        }

        async fn get(
            &self,
            owner_id: &str,
            task_id: &str,
        ) -> Result<Option<StoredTask>, BackendError> {
            self.inner.get(owner_id, task_id).await
        }

        async fn cas(
            &self,
            owner_id: &str,
            task_id: &str,
            expected_version: u64,
            record: StoredTask,
        ) -> Result<(), BackendError> {
            let is_lock = owner_id == LOCK_OWNER;
            let is_release = is_lock && record.task.status.is_terminal();
            let is_record = !is_lock && record.result.is_some();
            if is_release && self.fail_release.swap(false, Ordering::SeqCst) {
                return Err(BackendError::Store(TaskStoreError::Internal {
                    message: "injected lock release failure".to_string(),
                }));
            }
            if is_record && self.cancel_race.swap(false, Ordering::SeqCst) {
                let store = AprenderTaskStore::new(Arc::clone(&self.inner) as Arc<dyn TaskBackend>);
                store
                    .cancel(task_id, owner_id)
                    .await
                    .expect("client cancel");
            }
            let done = self
                .inner
                .cas(owner_id, task_id, expected_version, record)
                .await;
            if done.is_ok() {
                if is_release {
                    self.push("release");
                }
                if is_record {
                    self.push("record");
                }
            }
            done
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

    // --------------------------------------------------------------- the files

    struct Files {
        outcome: Option<Value>,
        gate_report: Option<Value>,
        training_manifest: Option<Value>,
        partial_manifest: Option<Value>,
        pack_outcome: Option<Value>,
        pack_manifest: Option<Value>,
        extra_objects: Vec<String>,
    }

    fn pack_manifest_json() -> Value {
        json!([
            {"path": "fixture-model.apr", "size": 4_194_304, "sha256": sha("apr")},
            {"path": "pack-outcome.json", "size": 640, "sha256": sha("pack-outcome")},
            {"path": "pack.log", "size": 1_024, "sha256": sha("pack-log")},
            {"path": "verify.log", "size": 2_048, "sha256": sha("verify-log")},
        ])
    }

    /// A passing, accepted run built from the committed fixtures.
    fn passing() -> Files {
        Files {
            outcome: Some(parse(OUTCOME)),
            gate_report: Some(parse(GATE_REPORT)),
            training_manifest: Some(parse(TRAINING_MANIFEST)),
            partial_manifest: None,
            pack_outcome: Some(parse(PACK_OUTCOME)),
            pack_manifest: Some(pack_manifest_json()),
            extra_objects: Vec::new(),
        }
    }

    /// A gate failure: no pack ran.
    fn gate_failed() -> Files {
        let mut files = passing();
        files.outcome.as_mut().expect("outcome")["exit_class"] = json!("gate_fail");
        files.gate_report.as_mut().expect("report")["pass"] = json!(false);
        files.pack_outcome = None;
        files.pack_manifest = None;
        files
    }

    fn entry_paths(manifest: &Value) -> Vec<String> {
        let list = manifest.get("entries").unwrap_or(manifest);
        list.as_array()
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r["path"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    // --------------------------------------------------------------- the rig

    struct Rig {
        backend: Arc<TestBackend>,
        objects: Arc<FakeObjects>,
        executions: Arc<FakeExecutionReader>,
        jobs: Arc<FakeJobs>,
        packs: Arc<FakePacks>,
        clock: Arc<AtomicU64>,
        log: Arc<Mutex<Vec<String>>>,
        ctx: FinalizerCtx,
        owner: String,
    }

    impl Rig {
        fn new() -> Self {
            Self::with_owner(OWNER)
        }

        fn with_owner(owner: &str) -> Self {
            let log: Arc<Mutex<Vec<String>>> = Arc::default();
            let backend = Arc::new(TestBackend {
                inner: Arc::new(InMemoryTaskBackend::new()),
                log: Arc::clone(&log),
                fail_release: AtomicBool::new(false),
                cancel_race: AtomicBool::new(false),
            });
            let objects = Arc::new(FakeObjects::with_log(Arc::clone(&log)));
            let executions = Arc::new(FakeExecutionReader::default());
            let jobs = Arc::new(FakeJobs::default());
            let packs = Arc::new(FakePacks::default());
            let clock = Arc::new(AtomicU64::new(T0));
            let tick = Arc::clone(&clock);
            let ctx = FinalizerCtx::new(
                Arc::clone(&backend) as Arc<dyn TaskBackend>,
                Arc::clone(&objects) as Arc<dyn ObjectStore>,
                Arc::clone(&executions) as Arc<dyn ExecutionReader>,
                Arc::clone(&jobs) as Arc<dyn TrainingJobs>,
                Arc::clone(&packs) as Arc<dyn PackTasks>,
                BUCKET,
                MACHINE,
            )
            .with_clock(Arc::new(move || tick.load(Ordering::SeqCst)));
            Self {
                backend,
                objects,
                executions,
                jobs,
                packs,
                clock,
                log,
                ctx,
                owner: owner.to_string(),
            }
        }

        fn prefix() -> String {
            layout::run_prefix(NAME, TASK_ID)
        }

        fn key(rel: &str) -> String {
            format!("{}{rel}", Self::prefix())
        }

        fn arn() -> String {
            execution_arn(MACHINE, TASK_ID).expect("arn")
        }

        fn exec_input(&self) -> ExecutionInput {
            ExecutionInput {
                schema: SCHEMA_EXECUTION_INPUT.to_string(),
                task_id: TASK_ID.to_string(),
                owner: self.owner.clone(),
                name: NAME.to_string(),
                dataset_prefix: "datasets/fixture/".to_string(),
                run_prefix: Self::prefix(),
                epochs: String::new(),
                inputs_sha256: layout::inputs_string(
                    &[
                        ("task.json", sha("data-task")),
                        ("train.jsonl", sha("data-train")),
                        ("eval.jsonl", sha("data-eval")),
                    ]
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                ),
                gate_contract_sha256: sha("prefilter-gate"),
            }
        }

        /// A Working task holding the lock, as `submit` leaves one.
        async fn start(&self) {
            let record = StoredTask {
                task: Task::new(TASK_ID, TaskStatus::Working),
                result: None,
                envelope: Some(json!({
                    "dataset_uri": format!("s3://{BUCKET}/datasets/fixture/"),
                    "name": NAME,
                    "run_prefix": format!("s3://{BUCKET}/{}", Self::prefix()),
                })),
                version: 1,
                expires_at_secs: Some(lock::now_secs() + TTL_SECS),
            };
            self.backend
                .put_new(&self.owner, record)
                .await
                .expect("mint");
            lock::acquire(self.backend.as_ref(), TASK_ID, &self.owner, T0)
                .await
                .expect("lock");
        }

        /// The execution as Step Functions would describe it.
        fn set_execution(&self, status: &str) {
            let input = serde_json::to_string(&self.exec_input()).expect("input");
            self.executions.set(&Self::arn(), status, &input, T0);
        }

        fn seed(&self, files: &Files) {
            let mut seeded: BTreeSet<String> = BTreeSet::new();
            let mut put = |rel: &str, value: &Value| {
                self.objects
                    .seed(&Self::key(rel), &serde_json::to_vec(value).expect("json"));
                seeded.insert(rel.to_string());
            };
            if let Some(v) = &files.outcome {
                put("outcome.json", v);
            }
            if let Some(v) = &files.gate_report {
                put("run/gate-report.json", v);
            }
            if let Some(v) = &files.training_manifest {
                put("training-manifest.json", v);
            }
            if let Some(v) = &files.partial_manifest {
                put("partial-manifest.json", v);
            }
            if let Some(v) = &files.pack_outcome {
                put("pack-outcome.json", v);
            }
            if let Some(v) = &files.pack_manifest {
                put("pack-manifest.json", v);
            }
            let listed: Vec<String> = [
                &files.training_manifest,
                &files.partial_manifest,
                &files.pack_manifest,
            ]
            .into_iter()
            .flatten()
            .flat_map(entry_paths)
            .collect();
            for rel in listed
                .into_iter()
                .chain(files.extra_objects.iter().cloned())
            {
                if !seeded.contains(&rel) {
                    self.objects.seed(&Self::key(&rel), b"object bytes");
                    seeded.insert(rel);
                }
            }
        }

        async fn payload(&self, error: Option<Value>) -> Result<FinalizeReport, FinalizerError> {
            let mut value = json!({
                "source": "state-machine",
                "execution_arn": Self::arn(),
                "start_time": T0,
                "input": self.exec_input(),
            });
            if let Some(error) = error {
                value["error"] = error;
            }
            handle(&self.ctx, FinalizerInput::parse(&value)?).await
        }

        async fn event(&self, status: &str) -> Result<FinalizeReport, FinalizerError> {
            handle(
                &self.ctx,
                FinalizerInput::parse(&event_json(status, MACHINE))?,
            )
            .await
        }

        async fn sweep(&self) -> Result<ReconcileReport, FinalizerError> {
            reconcile(&self.ctx).await
        }

        async fn record(&self) -> StoredTask {
            self.backend
                .get(&self.owner, TASK_ID)
                .await
                .expect("get")
                .expect("record")
        }

        async fn lock_state(&self) -> lock::LockState {
            lock::read(self.backend.as_ref()).await.expect("lock state")
        }

        async fn lock_released(&self) -> bool {
            self.lock_state().await.released
        }

        async fn stored_payload(&self) -> StatusPayload {
            let rec = self.record().await;
            let text = result_text(rec.result.as_ref().expect("a stored result")).expect("text");
            serde_json::from_str(text).expect("status payload")
        }

        fn manifest(&self) -> Manifest {
            serde_json::from_slice(
                &self
                    .objects
                    .bytes(&Self::key("manifest.json"))
                    .expect("manifest"),
            )
            .expect("manifest json")
        }

        /// Every object manifest.json names, plus manifest.json itself.
        fn published_keys(&self) -> Vec<String> {
            let mut keys: Vec<String> = self
                .manifest()
                .entries
                .iter()
                .map(|e| Self::key(&e.path))
                .collect();
            keys.push(Self::key("manifest.json"));
            keys
        }

        fn assert_all_tagged(&self, want: &str) {
            for key in self.published_keys() {
                assert_eq!(self.objects.tag(&key).as_deref(), Some(want), "{key}");
            }
        }

        fn log(&self) -> Vec<String> {
            self.log.lock().expect("lock").clone()
        }

        /// The finalizer's S3 mutations (puts and tags), not the backend's.
        fn s3_writes(&self) -> Vec<String> {
            self.objects.log()
        }

        /// A client `tasks/cancel` through the real store, with a recording sink.
        async fn cancel_through_store(&self) -> Arc<RecordingSink> {
            let sink = Arc::new(RecordingSink::default());
            let store = AprenderTaskStore::new(Arc::clone(&self.backend) as Arc<dyn TaskBackend>)
                .with_cancel_sink(Arc::clone(&sink) as Arc<dyn CancelSink>);
            store.cancel(TASK_ID, &self.owner).await.expect("cancel");
            sink
        }
    }

    #[derive(Default)]
    struct RecordingSink {
        seen: Mutex<Vec<String>>,
    }

    impl CancelSink for RecordingSink {
        fn cancel(&self, task_id: &str) {
            self.seen.lock().expect("lock").push(task_id.to_string());
        }
    }

    fn event_json(status: &str, machine: &str) -> Value {
        json!({
            "version": "0",
            "id": "11111111-2222-3333-4444-555555555555",
            "detail-type": EVENT_DETAIL_TYPE,
            "source": "aws.states",
            "account": "123456789012",
            "time": "2026-10-29T10:00:00Z",
            "region": "us-east-1",
            "resources": [Rig::arn()],
            "detail": {
                "executionArn": Rig::arn(),
                "stateMachineArn": machine,
                "name": format!("laya-{TASK_ID}"),
                "status": status,
            }
        })
    }

    fn phase_of(rec: &StoredTask) -> Phase {
        let text = result_text(rec.result.as_ref().expect("result")).expect("text");
        serde_json::from_str::<StatusPayload>(text)
            .expect("payload")
            .phase
    }

    /// A rig with the run started, its files seeded and its compute finished.
    async fn ready(files: &Files) -> Rig {
        let rig = Rig::new();
        rig.start().await;
        rig.seed(files);
        rig.jobs.set(&training_job_name(TASK_ID), "Completed");
        rig.set_execution("RUNNING");
        rig
    }

    // ------------------------------------------------------------------ tracer

    #[tokio::test]
    async fn tracer_completed() {
        let rig = ready(&passing()).await;
        let report = rig.payload(None).await.expect("finalized");

        assert_eq!(report.phase, Some(Phase::Completed));
        assert!(report.keep, "{report:?}");
        assert!(report.released);
        let rec = rig.record().await;
        assert_eq!(rec.task.status, TaskStatus::Completed);
        assert!(!rec.result.as_ref().expect("result").is_error);
        let payload = rig.stored_payload().await;
        assert_eq!(payload.phase, Phase::Completed);
        assert!(payload.manifest.iter().all(|l| l.url.is_none()));

        // verdict.json is the stored payload, byte for byte.
        let stored_text = result_text(rec.result.as_ref().expect("result")).expect("text");
        assert_eq!(
            rig.objects
                .bytes(&Rig::key("verdict.json"))
                .expect("verdict"),
            stored_text.as_bytes()
        );

        // manifest.json: images, contracts, listing, unlisted.
        let manifest = rig.manifest();
        assert_eq!(manifest.schema, SCHEMA_MANIFEST);
        let tm: TrainingManifest = serde_json::from_str(TRAINING_MANIFEST).expect("tm");
        let po: PackOutcome = serde_json::from_str(PACK_OUTCOME).expect("po");
        assert_eq!(manifest.images.train, Some(tm.train_image_digest.clone()));
        assert_eq!(manifest.images.pack, Some(po.pack_image_digest.clone()));
        assert_eq!(manifest.contracts.train_gate, Some(tm.gate_contract_sha256));
        assert_eq!(manifest.contracts.pack_gate, Some(po.gate_contract_sha256));
        assert_eq!(
            manifest.contracts.pack_parity,
            Some(po.parity_contract_sha256)
        );
        assert_eq!(
            manifest.contracts.prefilter_gate,
            Some(sha("prefilter-gate"))
        );
        assert_eq!(manifest.unlisted, 0);
        assert_eq!(
            manifest.entries.len(),
            12,
            "11 writer entries and verdict.json"
        );
        assert!(manifest
            .entries
            .iter()
            .any(|e| e.path == "verdict.json" && e.sha256 == sha256_hex(stored_text.as_bytes())));
        assert!(manifest
            .entries
            .iter()
            .any(|e| e.path == "fixture-model.apr"));

        // D-19: every manifest entry plus both files read back keep, and so do the
        // two writer inventories that exist (IN-01).
        rig.assert_all_tagged(RETENTION_KEEP);
        for inventory in ["training-manifest.json", "pack-manifest.json"] {
            assert_eq!(
                rig.objects.tag(&Rig::key(inventory)).as_deref(),
                Some(RETENTION_KEEP),
                "{inventory}"
            );
        }
        assert_eq!(
            report.tagged, 15,
            "12 entries, manifest.json and two inventories"
        );
        assert!(report.dropped.is_empty());

        // The lock is free.
        assert!(rig.lock_released().await);

        // Publication order: record, verdict.json, manifest.json, tags, release LAST.
        let log = rig.log();
        let at = |needle: &str| {
            log.iter()
                .position(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("{needle} missing from {log:?}"))
        };
        let last_tag = log
            .iter()
            .rposition(|l| l.starts_with("tag:"))
            .expect("tags");
        assert!(
            at("record")
                < at("put:runs/fixture-model/00000000-0000-4000-8000-000000000001/verdict.json")
        );
        assert!(
            at("verdict.json")
                < at("put:runs/fixture-model/00000000-0000-4000-8000-000000000001/manifest.json")
        );
        assert!(
            at("put:runs/fixture-model/00000000-0000-4000-8000-000000000001/manifest.json")
                < at("tag:")
        );
        assert!(last_tag < at("release"));
        assert_eq!(at("release"), log.len() - 1, "the release is the last step");
    }

    // ------------------------------------------------------------ input shapes

    #[test]
    fn fin_time_parsing() {
        assert_eq!(
            parse_rfc3339_secs("2026-10-28T10:00:00Z"),
            Some(1_793_181_600)
        );
        assert_eq!(
            parse_rfc3339_secs("2026-10-28T10:00:00.123Z"),
            Some(1_793_181_600)
        );
        assert_eq!(
            parse_rfc3339_secs("2026-10-28T12:00:00+02:00"),
            Some(1_793_181_600)
        );
        assert_eq!(parse_rfc3339_secs("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339_secs("not a time"), None);
        assert_eq!(parse_time_secs(&json!(1_793_181_600)), Some(1_793_181_600));
        assert_eq!(
            parse_time_secs(&json!(1_793_181_600.75)),
            Some(1_793_181_600)
        );
        assert_eq!(parse_time_secs(&json!(null)), None);
    }

    #[tokio::test]
    async fn fin_bad_shape() {
        let rig = ready(&passing()).await;
        let bad = [
            json!({"hello": 1}),
            json!([1, 2]),
            json!({"source": "reconcile", "extra": 1}),
            json!({"source": "state-machine"}),
            json!({"source": "state-machine", "execution_arn": "x", "start_time": 1,
                   "input": rig.exec_input(), "surprise": true}),
            json!({"source": "aws.states", "detail-type": "EC2 Instance State-change Notification",
                   "detail": {"executionArn": "x", "stateMachineArn": MACHINE, "status": "FAILED"}}),
            event_json("SUCCEEDED", MACHINE),
            event_json("RUNNING", MACHINE),
        ];
        for value in bad {
            let err = FinalizerInput::parse(&value).expect_err("refused");
            assert!(
                matches!(err, FinalizerError::Refused(_)),
                "{value}: {err:?}"
            );
        }
        assert_eq!(
            FinalizerInput::parse(&json!({"source": "reconcile"})).expect("sweep"),
            FinalizerInput::Reconcile
        );
        assert!(rig.s3_writes().is_empty(), "nothing written");
        assert!(!rig.lock_released().await);
    }

    #[tokio::test]
    async fn fin_foreign_machine() {
        let rig = ready(&passing()).await;
        rig.set_execution("ABORTED");
        let other = "arn:aws:states:us-east-1:123456789012:stateMachine:somebody-else";
        let err = handle(
            &rig.ctx,
            FinalizerInput::parse(&event_json("ABORTED", other)).expect("shape"),
        )
        .await
        .expect_err("refused");
        assert!(matches!(err, FinalizerError::Refused(_)), "{err:?}");
        assert!(rig.s3_writes().is_empty());
        assert!(!rig.lock_released().await, "the lock is unchanged");
        assert_eq!(rig.record().await.task.status, TaskStatus::Working);
        assert!(rig.executions.stops().is_empty());
    }

    #[tokio::test]
    async fn fin_wrong_prefix() {
        let rig = ready(&passing()).await;
        let mut input = rig.exec_input();
        input.run_prefix = format!("runs/other-model/{TASK_ID}/");
        let value = json!({"source": "state-machine", "execution_arn": Rig::arn(),
                           "start_time": T0, "input": input});
        let err = handle(&rig.ctx, FinalizerInput::parse(&value).expect("shape"))
            .await
            .expect_err("refused");
        assert!(matches!(err, FinalizerError::Refused(_)), "{err:?}");
        let mut climbing = rig.exec_input();
        climbing.name = "..".to_string();
        climbing.run_prefix = layout::run_prefix("..", TASK_ID);
        let value = json!({"source": "state-machine", "execution_arn": Rig::arn(),
                           "start_time": T0, "input": climbing});
        let err = handle(&rig.ctx, FinalizerInput::parse(&value).expect("shape"))
            .await
            .expect_err("refused");
        assert!(matches!(err, FinalizerError::Refused(_)), "{err:?}");
        assert!(rig.s3_writes().is_empty());
        assert!(!rig.lock_released().await);
    }

    // ------------------------------------------------------ every way a run ends

    #[tokio::test]
    async fn fin_gate_fail() {
        let rig = ready(&gate_failed()).await;
        let report = rig.payload(None).await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::GateFailed));
        let rec = rig.record().await;
        assert_eq!(rec.task.status, TaskStatus::Completed);
        assert!(!rec.result.as_ref().expect("result").is_error);
        assert!(!report.keep);
        rig.assert_all_tagged(RETENTION_EXPIRE);
        assert!(rig
            .objects
            .snapshot()
            .values()
            .all(|(_, t)| t.as_deref() != Some("keep")));
        assert!(rig.lock_released().await);
    }

    #[tokio::test]
    async fn fin_waived() {
        let mut files = gate_failed();
        files.gate_report.as_mut().expect("report")["pilot_waiver"] =
            json!({"id": "W-1", "waives": "ece_post"});
        files.pack_outcome = Some(parse(PACK_OUTCOME));
        files.pack_outcome.as_mut().expect("pack")["waived"] = json!("W-1");
        files.pack_manifest = Some(pack_manifest_json());
        let rig = ready(&files).await;
        let report = rig.payload(None).await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Completed));
        let payload = rig.stored_payload().await;
        let verdict = payload.verdict.expect("verdict");
        assert_eq!(verdict.waived.as_deref(), Some("W-1"));
        assert!(!verdict.pass);
        assert!(rig
            .manifest()
            .entries
            .iter()
            .any(|e| e.path == "fixture-model.apr"));
        assert!(!report.keep, "D-19 keeps GATE PASS only");
        rig.assert_all_tagged(RETENTION_EXPIRE);
        assert!(rig.lock_released().await);
    }

    #[tokio::test]
    async fn fin_pack_refused() {
        let mut files = passing();
        {
            let pack = files.pack_outcome.as_mut().expect("pack");
            pack["accepted"] = json!(false);
            pack["apr_sha256"] = Value::Null;
            pack["exit_code"] = json!(3);
            pack["deploy_eligible"] = json!(false);
            pack["verify_line"] = Value::Null;
        }
        let rig = ready(&files).await;
        let report = rig.payload(None).await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Failed));
        let rec = rig.record().await;
        assert_eq!(rec.task.status, TaskStatus::Failed);
        assert!(rec.result.as_ref().expect("result").is_error);
        let manifest = rig.manifest();
        assert!(
            !manifest.entries.iter().any(|e| e.path.ends_with(".apr")),
            "a refused pack never lists a model"
        );
        assert!(manifest
            .entries
            .iter()
            .any(|e| e.path == "run/gate-report.json"));
        assert!(manifest.entries.iter().any(|e| e.path == "verify.log"));
        rig.assert_all_tagged(RETENTION_EXPIRE);
        assert!(rig.lock_released().await);
    }

    #[tokio::test]
    async fn fin_catch_error() {
        let rig = ready(&Files {
            outcome: None,
            gate_report: None,
            training_manifest: None,
            partial_manifest: None,
            pack_outcome: None,
            pack_manifest: None,
            extra_objects: Vec::new(),
        })
        .await;
        let cause = "x".repeat(5_000);
        let error = json!({"Error": "States.TaskFailed", "Cause": cause});
        if let FinalizerInput::StateMachine(p) = FinalizerInput::parse(&json!({
            "source": "state-machine", "execution_arn": Rig::arn(), "start_time": T0,
            "input": rig.exec_input(), "error": error
        }))
        .expect("shape")
        {
            assert!(p.error.expect("error").chars().count() <= 2_048);
        } else {
            panic!("not a state-machine payload");
        }
        let report = rig.payload(Some(error)).await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Failed));
        let payload = rig.stored_payload().await;
        let text = payload.error.expect("error text");
        assert!(text.starts_with("States.TaskFailed"), "{text}");
        assert!(rig.record().await.result.expect("result").is_error);
        assert!(rig.lock_released().await);
    }

    /// A passing run whose Train job exited 1 (an upload failed after the records
    /// were complete) arrives through CatchFinalize WITH a stage error: the records
    /// decide `completed` and the kept GATE PASS is still kept, but the stored
    /// payload and verdict.json say the run was rescued. The same records through
    /// the status event carry no note.
    #[tokio::test]
    async fn fin_override_note_reaches_verdict() {
        use aprender_mcp_laya_train::finalize::NOTE_STAGE_ERROR_OVERRIDDEN;

        let rig = ready(&passing()).await;
        let error = json!({"Error": "States.TaskFailed",
                           "Cause": "{\"TrainingJobStatus\":\"Failed\"}"});
        let report = rig.payload(Some(error)).await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Completed));
        assert!(
            report.keep,
            "a verified, pack-accepted GATE PASS is still kept"
        );
        assert!(report.stage_error_overridden);
        assert!(report.released);
        rig.assert_all_tagged(RETENTION_KEEP);
        let payload = rig.stored_payload().await;
        assert_eq!(payload.phase, Phase::Completed);
        assert_eq!(payload.error, None);
        assert_eq!(payload.notes, [NOTE_STAGE_ERROR_OVERRIDDEN.to_string()]);
        let rec = rig.record().await;
        assert!(!rec.result.as_ref().expect("result").is_error);
        let stored = result_text(rec.result.as_ref().expect("result")).expect("text");
        let verdict = rig
            .objects
            .bytes(&Rig::key("verdict.json"))
            .expect("verdict");
        assert_eq!(
            verdict,
            stored.as_bytes(),
            "verdict.json is the stored text"
        );
        let verdict = String::from_utf8(verdict).expect("utf8");
        assert!(verdict.contains(NOTE_STAGE_ERROR_OVERRIDDEN), "{verdict}");
        assert!(
            !verdict.contains("TrainingJobStatus"),
            "no stage error text in a note"
        );

        // The same records through the event path, on a fresh rig: no note.
        let rig = ready(&passing()).await;
        rig.set_execution("FAILED");
        let report = rig.event("FAILED").await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Completed));
        assert!(report.keep);
        assert!(!report.stage_error_overridden);
        assert!(rig.stored_payload().await.notes.is_empty());
        let verdict = rig
            .objects
            .bytes(&Rig::key("verdict.json"))
            .expect("verdict");
        let verdict = String::from_utf8(verdict).expect("utf8");
        assert!(!verdict.contains("stage-error-overridden"), "{verdict}");
        assert!(
            !verdict.contains("\"notes\""),
            "an empty notes list is not written"
        );
    }

    // ------------------------------------------- listed but absent (D-11, WR-02)

    fn catch_error() -> Value {
        json!({"Error": "States.TaskFailed", "Cause": "{\"TrainingJobStatus\":\"Failed\"}"})
    }

    /// The listed-but-absent notes of a payload.
    fn absent_notes(payload: &StatusPayload) -> Vec<&str> {
        payload
            .notes
            .iter()
            .filter(|n| n.starts_with("listed-but-absent"))
            .map(String::as_str)
            .collect()
    }

    /// A passing run whose job listed `rel` and then failed to upload it (removed
    /// after seeding, so the writer manifest still lists it), delivered by the
    /// state-machine payload with and without the Catch's stage error. Every
    /// delivery must finish on the FIRST call with the lock released, report
    /// exactly `rel` as dropped, list it nowhere, tag the rest, and name it in one
    /// note of the stored payload and verdict.json.
    async fn assert_listed_but_absent(rel: &str, error_needle: &str) {
        for error in [None, Some(catch_error())] {
            let caught = error.is_some();
            let rig = ready(&passing()).await;
            assert!(
                rig.objects.bytes(&Rig::key(rel)).is_some(),
                "{rel} was seeded"
            );
            rig.objects.remove(&Rig::key(rel));
            let report = rig
                .payload(error)
                .await
                .unwrap_or_else(|e| panic!("{rel} caught={caught}: the first call failed: {e}"));
            assert!(
                report.released,
                "{rel} caught={caught}: the lock is released"
            );
            assert!(rig.lock_released().await);
            assert_eq!(report.dropped, [rel.to_string()], "caught={caught}");
            assert_eq!(report.phase, Some(Phase::Failed), "{rel} caught={caught}");

            let payload = rig.stored_payload().await;
            assert!(
                payload
                    .error
                    .as_deref()
                    .is_some_and(|e| e.contains(error_needle)),
                "{rel} caught={caught}: {:?}",
                payload.error
            );
            assert!(
                payload.manifest.iter().all(|l| l.path != rel),
                "{rel}: the stored payload lists an object S3 does not hold"
            );
            assert!(
                rig.manifest().entries.iter().all(|e| e.path != rel),
                "{rel}: manifest.json lists an object S3 does not hold"
            );
            let notes = absent_notes(&payload);
            assert_eq!(notes.len(), 1, "{rel} caught={caught}: {:?}", payload.notes);
            assert!(
                notes[0].ends_with(&format!("dropped: {rel}")),
                "{}",
                notes[0]
            );

            let stored = result_text(rig.record().await.result.as_ref().expect("result"))
                .expect("text")
                .to_string();
            let verdict = rig
                .objects
                .bytes(&Rig::key("verdict.json"))
                .expect("verdict");
            assert_eq!(
                verdict,
                stored.as_bytes(),
                "verdict.json is the stored text"
            );
            assert_eq!(stored.matches("listed-but-absent").count(), 1, "{stored}");

            // Every other object is tagged with the run's retention tag.
            rig.assert_all_tagged(RETENTION_EXPIRE);
            assert!(!report.keep);
            assert_eq!(
                rig.objects.tag(&Rig::key(rel)),
                None,
                "{rel} was never written"
            );
        }
    }

    #[tokio::test]
    async fn fin_listed_but_absent_outcome() {
        assert_listed_but_absent("outcome.json", "").await;
        // Without an outcome to read, the stage error is the text when there is one,
        // and "outcome.json is missing" when there is none.
        for (error, needle) in [
            (None, "outcome.json is missing"),
            (Some(catch_error()), "States.TaskFailed"),
        ] {
            let rig = ready(&passing()).await;
            rig.objects.remove(&Rig::key("outcome.json"));
            rig.payload(error).await.expect("finalized");
            let text = rig.stored_payload().await.error.expect("error");
            assert!(text.contains(needle), "{text}");
        }
    }

    #[tokio::test]
    async fn fin_listed_but_absent_pack_outcome() {
        assert_listed_but_absent("pack-outcome.json", "pack-outcome.json is missing").await;
    }

    #[tokio::test]
    async fn fin_listed_but_absent_report() {
        assert_listed_but_absent("run/gate-report.json", "gate-report.json is missing").await;
    }

    /// manifest.json was written by a first writer (an earlier pass, possibly an
    /// earlier finalizer) and lists an object that is gone since. The adopted
    /// branch keeps manifest.json as written, tags what exists, never a key no
    /// inventory names, and does not fail.
    #[tokio::test]
    async fn fin_adopted_manifest_with_absent_entry() {
        let mut files = passing();
        files.extra_objects = vec!["run/stray.bin".to_string()];
        let rig = ready(&files).await;
        rig.payload(None).await.expect("the first writer");
        let manifest_bytes = rig
            .objects
            .bytes(&Rig::key("manifest.json"))
            .expect("manifest");
        assert!(rig.manifest().entries.iter().any(|e| e.path == "train.log"));
        assert_eq!(rig.manifest().unlisted, 1, "the stray object is unlisted");
        assert_eq!(
            rig.objects.tag(&Rig::key("run/stray.bin")).as_deref(),
            Some(RETENTION_EXPIRE)
        );

        // The listed object goes, and a tag is lost, so the next pass has work to do.
        rig.objects.remove(&Rig::key("train.log"));
        rig.objects
            .put_tag(&Rig::key("fixture-model.apr"), RETENTION_EXPIRE)
            .await
            .expect("lose a tag");
        let report = rig
            .payload(None)
            .await
            .expect("the adopted branch must not fail");
        assert_eq!(report.phase, Some(Phase::Completed));
        assert!(report.keep);
        assert!(rig.lock_released().await);
        assert_eq!(
            report.dropped,
            ["train.log".to_string()],
            "this pass's read"
        );
        assert_eq!(
            rig.objects
                .bytes(&Rig::key("manifest.json"))
                .expect("manifest"),
            manifest_bytes,
            "an adopted manifest.json is kept as written"
        );
        for entry in rig
            .manifest()
            .entries
            .iter()
            .filter(|e| e.path != "train.log")
        {
            assert_eq!(
                rig.objects.tag(&Rig::key(&entry.path)).as_deref(),
                Some(RETENTION_KEEP),
                "{}",
                entry.path
            );
        }
        assert_eq!(
            rig.objects.tag(&Rig::key("run/stray.bin")).as_deref(),
            Some(RETENTION_EXPIRE),
            "a key no inventory names is never a tag target"
        );
        assert!(rig.objects.bytes(&Rig::key("train.log")).is_none());
    }

    /// IN-01 (D-19): every self-listing inventory that exists carries the run's
    /// retention tag: keep for a kept GATE PASS, expire otherwise (even one that was
    /// tagged keep before).
    #[tokio::test]
    async fn fin_kept_run_retags_inventories() {
        let rig = ready(&passing()).await;
        let report = rig.payload(None).await.expect("finalized");
        assert!(report.keep);
        for inventory in [
            "training-manifest.json",
            "pack-manifest.json",
            "manifest.json",
            "verdict.json",
        ] {
            assert_eq!(
                rig.objects.tag(&Rig::key(inventory)).as_deref(),
                Some(RETENTION_KEEP),
                "{inventory}"
            );
        }
        assert!(rig
            .objects
            .bytes(&Rig::key("partial-manifest.json"))
            .is_none());

        // A partial inventory of a kept run is kept too.
        let mut files = passing();
        files.partial_manifest = files.training_manifest.clone();
        let rig = ready(&files).await;
        assert!(rig.payload(None).await.expect("finalized").keep);
        assert_eq!(
            rig.objects
                .tag(&Rig::key("partial-manifest.json"))
                .as_deref(),
            Some(RETENTION_KEEP)
        );

        // A failed run's inventories read back expire, whatever they carried.
        let mut files = passing();
        files.pack_outcome.as_mut().expect("pack")["accepted"] = json!(false);
        files.pack_outcome.as_mut().expect("pack")["apr_sha256"] = Value::Null;
        let rig = ready(&files).await;
        for inventory in ["training-manifest.json", "pack-manifest.json"] {
            rig.objects
                .put_tag(&Rig::key(inventory), RETENTION_KEEP)
                .await
                .expect("tag");
        }
        let report = rig.payload(None).await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Failed));
        for inventory in ["training-manifest.json", "pack-manifest.json"] {
            assert_eq!(
                rig.objects.tag(&Rig::key(inventory)).as_deref(),
                Some(RETENTION_EXPIRE),
                "{inventory}"
            );
        }
    }

    #[tokio::test]
    async fn fin_abort_event() {
        let rig = ready(&passing()).await;
        rig.set_execution("ABORTED");
        rig.jobs.set(&training_job_name(TASK_ID), "Stopped");
        let report = rig.event("ABORTED").await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Cancelled));
        assert!(
            *rig.executions.describes.lock().expect("lock") >= 1,
            "DescribeExecution read"
        );
        let rec = rig.record().await;
        assert_eq!(
            rec.task.status,
            TaskStatus::Cancelled,
            "readers see cancelled"
        );
        assert_eq!(phase_of(&rec), Phase::Cancelled);
        assert!(rig
            .manifest()
            .entries
            .iter()
            .all(|e| !e.path.ends_with(".apr")));
        assert!(!report.keep);
        assert!(rig.lock_released().await);
    }

    #[tokio::test]
    async fn fin_timeout_event() {
        let mut files = passing();
        files.outcome.as_mut().expect("outcome")["exit_class"] = json!("timeout");
        files.gate_report = None;
        files.pack_outcome = None;
        files.pack_manifest = None;
        let rig = ready(&files).await;
        rig.set_execution("TIMED_OUT");
        rig.jobs.set(&training_job_name(TASK_ID), "Stopped");
        let report = rig.event("TIMED_OUT").await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Failed));
        assert_eq!(rig.record().await.task.status, TaskStatus::Failed);
        assert!(rig
            .stored_payload()
            .await
            .error
            .expect("error")
            .contains("runtime cap"));
        assert!(rig.lock_released().await);
    }

    // ------------------------------------------------- duplicates and first writer

    #[tokio::test]
    async fn fin_duplicate() {
        let rig = ready(&passing()).await;
        let first = rig.payload(None).await.expect("first");
        let verdict = rig
            .objects
            .bytes(&Rig::key("verdict.json"))
            .expect("verdict");
        let manifest = rig
            .objects
            .bytes(&Rig::key("manifest.json"))
            .expect("manifest");
        let stored = rig.record().await.result.expect("result");
        let writes = rig.s3_writes();
        let second = rig.payload(None).await.expect("second");
        assert_eq!(first.phase, Some(Phase::Completed));
        assert_eq!(second.phase, Some(Phase::Completed));
        assert_eq!(
            rig.objects
                .bytes(&Rig::key("verdict.json"))
                .expect("verdict"),
            verdict
        );
        assert_eq!(
            rig.objects
                .bytes(&Rig::key("manifest.json"))
                .expect("manifest"),
            manifest
        );
        assert_eq!(
            result_text(&rig.record().await.result.expect("result")),
            result_text(&stored)
        );
        assert_eq!(
            rig.s3_writes(),
            writes,
            "a duplicate invocation writes nothing"
        );
        rig.assert_all_tagged(RETENTION_KEEP);
    }

    #[tokio::test]
    async fn fin_first_writer_wins() {
        let rig = ready(&passing()).await;
        rig.payload(None).await.expect("completed first");
        let verdict = rig
            .objects
            .bytes(&Rig::key("verdict.json"))
            .expect("verdict");
        let stored = rig.record().await.result.expect("result");

        // A late FAILED event, with outputs that would now decide `failed`, and a
        // keep tag that was lost in the meantime.
        let mut crashed = passing();
        crashed.outcome.as_mut().expect("outcome")["exit_class"] = json!("crash");
        rig.seed(&crashed);
        rig.objects
            .put_tag(&Rig::key("fixture-model.apr"), "expire")
            .await
            .expect("lose a tag");
        rig.set_execution("FAILED");
        let report = rig.event("FAILED").await.expect("reused");

        assert_eq!(
            report.phase,
            Some(Phase::Completed),
            "the stored phase wins"
        );
        assert_eq!(
            rig.objects
                .bytes(&Rig::key("verdict.json"))
                .expect("verdict"),
            verdict,
            "verdict.json is not rewritten with `failed`"
        );
        assert_eq!(
            result_text(&rig.record().await.result.expect("result")),
            result_text(&stored)
        );
        rig.assert_all_tagged(RETENTION_KEEP);
        assert!(rig.lock_released().await);
    }

    #[tokio::test]
    async fn fin_tag_failure() {
        let rig = ready(&passing()).await;
        rig.objects.fail_nth(Op::PutTag, 1);
        let err = rig.payload(None).await.expect_err("tagging failed");
        assert!(matches!(err, FinalizerError::Store(_)), "{err:?}");
        assert!(!rig.lock_released().await, "the lock is still held");
        assert_eq!(
            rig.record().await.task.status,
            TaskStatus::Completed,
            "the record is durable"
        );
        let report = rig.payload(None).await.expect("resumed");
        assert!(report.keep);
        rig.assert_all_tagged(RETENTION_KEEP);
        assert!(rig.lock_released().await);
    }

    // -------------------------------------------------------- NotFound and integrity

    #[tokio::test]
    async fn fin_record_expired() {
        let rig = Rig::new();
        // The lock is held by T, but its task record is long gone.
        lock::acquire(rig.backend.as_ref(), TASK_ID, OWNER, T0)
            .await
            .expect("lock");
        rig.seed(&passing());
        rig.jobs.set(&training_job_name(TASK_ID), "Completed");
        rig.clock.store(T0 + TTL_SECS + 100, Ordering::SeqCst);
        let report = rig.payload(None).await.expect("finalized");
        assert_eq!(report.record, "record_missing");
        assert_eq!(report.phase, Some(Phase::Completed));
        assert!(rig.objects.bytes(&Rig::key("verdict.json")).is_some());
        assert!(rig.objects.bytes(&Rig::key("manifest.json")).is_some());
        assert!(rig.lock_released().await);
        assert!(rig
            .backend
            .get(OWNER, TASK_ID)
            .await
            .expect("get")
            .is_none());
    }

    #[tokio::test]
    async fn fin_record_missing_within_ttl() {
        let rig = Rig::new();
        lock::acquire(rig.backend.as_ref(), TASK_ID, OWNER, T0)
            .await
            .expect("lock");
        rig.seed(&passing());
        rig.jobs.set(&training_job_name(TASK_ID), "Completed");
        rig.clock.store(T0 + TTL_SECS - 100, Ordering::SeqCst);
        let err = rig.payload(None).await.expect_err("integrity");
        assert!(matches!(err, FinalizerError::Integrity(_)), "{err:?}");
        assert!(rig.s3_writes().is_empty(), "nothing written to S3");
        assert!(!rig.lock_released().await, "the lock is left held");
    }

    #[tokio::test]
    async fn fin_owner_mismatch() {
        let rig = ready(&passing()).await;
        let version = rig.record().await.version;
        // A record for the same task id also exists under the owner the input
        // names, so only the owner check (not a missing record) can stop the run.
        let decoy = StoredTask {
            task: Task::new(TASK_ID, TaskStatus::Working),
            result: None,
            envelope: None,
            version: 1,
            expires_at_secs: Some(lock::now_secs() + TTL_SECS),
        };
        rig.backend.put_new("bob", decoy).await.expect("decoy");
        let mut input = rig.exec_input();
        input.owner = "bob".to_string();
        let value = json!({"source": "state-machine", "execution_arn": Rig::arn(),
                           "start_time": T0, "input": input});
        let err = handle(&rig.ctx, FinalizerInput::parse(&value).expect("shape"))
            .await
            .expect_err("integrity");
        assert!(matches!(err, FinalizerError::Integrity(_)), "{err:?}");
        assert!(rig.s3_writes().is_empty());
        assert!(!rig.lock_released().await);
        let rec = rig.record().await;
        assert_eq!(rec.version, version, "T's record is untouched");
        assert_eq!(rec.task.status, TaskStatus::Working);
        let decoy = rig
            .backend
            .get("bob", TASK_ID)
            .await
            .expect("get")
            .expect("decoy");
        assert_eq!(
            decoy.task.status,
            TaskStatus::Working,
            "the other owner's record is untouched"
        );
        assert!(decoy.result.is_none());
    }

    #[tokio::test]
    async fn fin_terminal_no_result() {
        let rig = Rig::new();
        let record = StoredTask {
            task: Task::new(TASK_ID, TaskStatus::Failed),
            result: None,
            envelope: None,
            version: 1,
            expires_at_secs: Some(lock::now_secs() + TTL_SECS),
        };
        rig.backend.put_new(OWNER, record).await.expect("record");
        lock::acquire(rig.backend.as_ref(), TASK_ID, OWNER, T0)
            .await
            .expect("lock");
        rig.seed(&passing());
        rig.jobs.set(&training_job_name(TASK_ID), "Completed");
        let err = rig.payload(None).await.expect_err("integrity");
        assert!(matches!(err, FinalizerError::Integrity(_)), "{err:?}");
        assert!(rig.s3_writes().is_empty());
        assert!(!rig.lock_released().await);
    }

    #[tokio::test]
    async fn fin_partial_inventory() {
        let mut files = passing();
        files.outcome.as_mut().expect("outcome")["exit_class"] = json!("timeout");
        files.gate_report = None;
        files.pack_outcome = None;
        files.pack_manifest = None;
        let mut partial = parse(TRAINING_MANIFEST);
        partial["complete"] = json!(false);
        partial["entries"] = json!([
            {"path": "outcome.json", "size": 312, "sha256": sha("outcome")},
            {"path": "train.log", "size": 100, "sha256": sha("train-log")},
            {"path": "data/task.json", "size": 275, "sha256": sha("data-task")},
            {"path": "data/train.jsonl", "size": 9_216, "sha256": sha("data-train")},
        ]);
        files.training_manifest = None;
        files.partial_manifest = Some(partial);
        files.extra_objects = vec!["data/eval.jsonl".to_string(), "run/stray.bin".to_string()];
        let rig = ready(&files).await;
        let report = rig.payload(None).await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Failed));
        let manifest = rig.manifest();
        let mut paths: Vec<&str> = manifest.entries.iter().map(|e| e.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(
            paths,
            [
                "data/task.json",
                "data/train.jsonl",
                "outcome.json",
                "train.log",
                "verdict.json"
            ]
        );
        assert_eq!(manifest.unlisted, 2, "two objects no writer manifest lists");
        assert!(rig.stored_payload().await.manifest_partial);
        assert!(rig.lock_released().await);
    }

    // ---------------------------------------------------------------- client cancel

    /// A server over the SAME backend, answering `train_status` over loopback HTTP
    /// for the unauthenticated owner.
    async fn train_status_over_http(backend: Arc<dyn TaskBackend>) -> Value {
        use crate::dataset::fakes::FakeReader;
        use crate::sfn::fakes::{FakeExecutions, FakePresign};
        use crate::{S3DatasetSource, SfnDispatcher};

        let dispatcher = Arc::new(SfnDispatcher::new(
            Arc::new(FakeExecutions::default()),
            Arc::new(FakePresign),
            BUCKET,
            MACHINE,
        ));
        let ctx = aprender_mcp_laya_train::ServerCtx::new(
            backend,
            dispatcher as Arc<dyn aprender_mcp_laya_train::Dispatcher>,
            Arc::new(S3DatasetSource::new(FakeReader::default(), BUCKET)),
            None,
        );
        let server = aprender_mcp_laya_train::build_server(ctx, "0.0.0").expect("server");
        let http_server = pmcp::server::streamable_http_server::StreamableHttpServer::with_config(
            "127.0.0.1:0".parse().expect("addr"),
            Arc::new(tokio::sync::Mutex::new(server)),
            crate::server_config(),
        );
        let (bound, _handle) = http_server.start().await.expect("binds");
        let url = format!("http://{bound}");
        let http = reqwest::Client::new();
        let rpc = |id: u64, method: &str, params: Value| {
            let http = http.clone();
            let url = url.clone();
            let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
            async move {
                let text = http
                    .post(url)
                    .header("content-type", "application/json")
                    .header("accept", "application/json, text/event-stream")
                    .body(body.to_string())
                    .send()
                    .await
                    .expect("transport")
                    .text()
                    .await
                    .expect("body");
                let payload = text
                    .lines()
                    .find_map(|line| line.strip_prefix("data: "))
                    .unwrap_or(&text)
                    .to_string();
                serde_json::from_str::<Value>(&payload).expect("json body")
            }
        };
        let init = rpc(
            1,
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {},
                   "clientInfo": {"name": "fin", "version": "0"}}),
        )
        .await;
        assert!(init["error"].is_null(), "{init}");
        let answer = rpc(
            2,
            "tools/call",
            json!({"name": "train_status", "arguments": {"task_id": TASK_ID}}),
        )
        .await;
        let text = answer["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("a text answer: {answer}"));
        serde_json::from_str(text).expect("payload json")
    }

    #[tokio::test]
    async fn fin_cancel_first() {
        // The unauthenticated owner, so the loopback server reads the same task.
        let rig = Rig::with_owner("local");
        rig.start().await;
        rig.seed(&passing());
        rig.jobs.set(&training_job_name(TASK_ID), "Stopped");
        rig.set_execution("ABORTED");
        let sink = rig.cancel_through_store().await;
        assert_eq!(*sink.seen.lock().expect("lock"), [TASK_ID.to_string()]);
        let cancelled = rig.record().await;
        assert_eq!(cancelled.task.status, TaskStatus::Cancelled);
        assert!(cancelled.result.is_none(), "the cancel leaves no result");

        let report = rig.event("ABORTED").await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Cancelled));
        assert_eq!(report.record, "attached");
        let rec = rig.record().await;
        assert_eq!(
            rec.task.status,
            TaskStatus::Cancelled,
            "the status is unchanged"
        );

        // tasks/result: the store's own get_result.
        let stored = rig
            .ctx
            .store
            .get_result(TASK_ID, "local")
            .await
            .expect("a stored result");
        let payload: StatusPayload =
            serde_json::from_str(result_text(&stored).expect("text")).expect("payload");
        assert_eq!(payload.schema, layout::SCHEMA_STATUS);
        assert_eq!(payload.phase, Phase::Cancelled);
        assert!(payload.manifest.iter().all(|l| l.url.is_none()));

        // train_status: the library's handler serves that stored payload.
        let served = train_status_over_http(Arc::clone(&rig.backend) as Arc<dyn TaskBackend>).await;
        assert_eq!(served["phase"], "cancelled", "{served}");
        assert_eq!(served["schema"], layout::SCHEMA_STATUS);
        assert_eq!(served["task_id"], TASK_ID);

        // verdict.json says cancelled and expires; the lock is free.
        let verdict: Value = serde_json::from_slice(
            &rig.objects
                .bytes(&Rig::key("verdict.json"))
                .expect("verdict"),
        )
        .expect("json");
        assert_eq!(verdict["phase"], "cancelled");
        assert_eq!(
            rig.objects.tag(&Rig::key("verdict.json")).as_deref(),
            Some(RETENTION_EXPIRE)
        );
        assert!(rig.lock_released().await);

        // A duplicate ABORTED event leaves the stored result byte-identical.
        let before = result_text(&rec.result.expect("result")).map(str::to_string);
        rig.event("ABORTED").await.expect("duplicate");
        let after = rig.record().await.result;
        assert_eq!(
            result_text(&after.expect("result")).map(str::to_string),
            before
        );
        assert!(rig
            .objects
            .snapshot()
            .values()
            .all(|(_, t)| t.as_deref() != Some("keep")));
    }

    #[tokio::test]
    async fn fin_cancel_late() {
        let rig = ready(&passing()).await;
        rig.cancel_through_store().await;
        // The execution was not stopped: its Finalize payload carries a completed run.
        let report = rig.payload(None).await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Cancelled));
        let rec = rig.record().await;
        assert_eq!(rec.task.status, TaskStatus::Cancelled);
        assert_eq!(phase_of(&rec), Phase::Cancelled);
        assert!(rig
            .manifest()
            .entries
            .iter()
            .all(|e| !e.path.ends_with(".apr")));
        assert!(rig
            .objects
            .snapshot()
            .values()
            .all(|(_, t)| t.as_deref() != Some("keep")));
        assert!(!report.keep);
        assert!(rig.lock_released().await);
    }

    #[tokio::test]
    async fn fin_cancel_race() {
        let rig = ready(&passing()).await;
        // The client's cancel lands just before the finalizer's finish applies.
        rig.backend.cancel_race.store(true, Ordering::SeqCst);
        let report = rig.payload(None).await.expect("finalized");
        assert_eq!(report.phase, Some(Phase::Cancelled), "never completed");
        let rec = rig.record().await;
        assert_eq!(rec.task.status, TaskStatus::Cancelled);
        assert_eq!(phase_of(&rec), Phase::Cancelled);
        let verdict_puts = rig
            .s3_writes()
            .iter()
            .filter(|l| l.ends_with("/verdict.json") && l.starts_with("put:"))
            .count();
        assert_eq!(verdict_puts, 1, "verdict.json is written once");
        let verdict: Value = serde_json::from_slice(
            &rig.objects
                .bytes(&Rig::key("verdict.json"))
                .expect("verdict"),
        )
        .expect("json");
        assert_eq!(verdict["phase"], "cancelled");
        assert!(rig
            .objects
            .snapshot()
            .values()
            .all(|(_, t)| t.as_deref() != Some("keep")));
        assert!(rig.lock_released().await);
    }

    // ---------------------------------------------------------------- compute gate

    #[tokio::test]
    async fn fin_compute_gate_table() {
        let rig = Rig::new();
        let table = [
            (None, true),
            (Some("Completed"), true),
            (Some("Failed"), true),
            (Some("Stopped"), true),
            (Some("InProgress"), false),
            (Some("Stopping"), false),
            (Some("SomethingNew"), false),
        ];
        for (status, terminal) in table {
            rig.jobs.status.lock().expect("lock").clear();
            if let Some(status) = status {
                rig.jobs.set(&training_job_name(TASK_ID), status);
            }
            let state = compute_terminal(&rig.ctx, TASK_ID).await.expect("state");
            assert_eq!(state.terminal, terminal, "{status:?}");
        }
        rig.jobs.set(&training_job_name(TASK_ID), "Completed");
        rig.packs.add(TASK_ID, "arn:task/1", "RUNNING");
        rig.packs.add("another-task", "arn:task/2", "RUNNING");
        let state = compute_terminal(&rig.ctx, TASK_ID).await.expect("state");
        assert!(
            !state.terminal,
            "a live pack task of the run blocks the release"
        );
        assert_eq!(state.pack_task_arns, ["arn:task/1"]);
        rig.packs.set_status("arn:task/1", "STOPPED");
        assert!(
            compute_terminal(&rig.ctx, TASK_ID)
                .await
                .expect("state")
                .terminal
        );
    }

    #[tokio::test]
    async fn fin_awaiting_training() {
        let rig = ready(&passing()).await;
        rig.set_execution("ABORTED");
        rig.jobs.set(&training_job_name(TASK_ID), "InProgress");
        let report = rig.event("ABORTED").await.expect("awaiting");
        assert!(report.awaiting_compute);
        assert!(report.stopped_training_job);
        assert_eq!(
            rig.jobs.stops(),
            [training_job_name(TASK_ID)],
            "stopped once"
        );
        assert!(rig.s3_writes().is_empty(), "nothing written to S3");
        assert!(!rig.lock_released().await, "the lock is still held");
        assert_eq!(
            rig.record().await.task.status,
            TaskStatus::Cancelled,
            "readers see cancelled at once"
        );

        // The job is Stopping: no second stop, still awaiting.
        rig.jobs.set(&training_job_name(TASK_ID), "Stopping");
        let again = rig.sweep().await.expect("sweep");
        assert_eq!(again.action, "awaiting-compute");
        assert_eq!(rig.jobs.stops().len(), 1, "never stopped twice");
        assert!(!rig.lock_released().await);

        // The job is Stopped: the sweep finalizes.
        rig.jobs.set(&training_job_name(TASK_ID), "Stopped");
        let done = rig.sweep().await.expect("sweep");
        assert_eq!(done.action, "finalized");
        let rec = rig.record().await;
        assert_eq!(rec.task.status, TaskStatus::Cancelled);
        assert_eq!(phase_of(&rec), Phase::Cancelled);
        assert!(rig.objects.bytes(&Rig::key("verdict.json")).is_some());
        assert!(rig.objects.bytes(&Rig::key("manifest.json")).is_some());
        assert!(rig.lock_released().await);
    }

    #[tokio::test]
    async fn fin_cancel_packing() {
        let rig = ready(&passing()).await;
        let pack_arn = "arn:aws:ecs:us-east-1:123456789012:task/cluster/abc";
        rig.packs.add(TASK_ID, pack_arn, "RUNNING");
        rig.cancel_through_store().await;
        rig.executions
            .running
            .lock()
            .expect("lock")
            .push(Rig::arn());

        let first = rig.sweep().await.expect("sweep");
        assert_eq!(first.action, "cancel-retry");
        assert!(first.stopped_execution);
        assert_eq!(rig.executions.stops(), [Rig::arn()], "StopExecution once");
        assert_eq!(rig.packs.stops(), [pack_arn.to_string()], "StopTask once");
        assert!(rig.s3_writes().is_empty());
        assert!(!rig.lock_released().await);

        // A replacement `train` is refused single-flight while the lock is held.
        assert_eq!(
            lock::peek(rig.backend.as_ref()).await.as_deref(),
            Some(TASK_ID)
        );
        let refused = lock::acquire(rig.backend.as_ref(), "replacement", "carol", T0 + 60).await;
        assert!(
            matches!(refused, Err(lock::AcquireError::Held(ref h)) if h.holder == TASK_ID),
            "{refused:?}"
        );

        // The execution is ABORTED and the pack task STOPPED: finalized cancelled.
        rig.set_execution("ABORTED");
        rig.executions.running.lock().expect("lock").clear();
        rig.packs.set_status(pack_arn, "STOPPED");
        let second = rig.sweep().await.expect("sweep");
        assert_eq!(second.action, "finalized");
        assert_eq!(phase_of(&rig.record().await), Phase::Cancelled);
        assert!(rig.lock_released().await);
        lock::acquire(rig.backend.as_ref(), "replacement", "carol", T0 + 120)
            .await
            .expect("the lock is free after the release");
    }

    #[tokio::test]
    async fn fin_cancel_retry() {
        let rig = ready(&passing()).await;
        rig.cancel_through_store().await;
        rig.executions
            .running
            .lock()
            .expect("lock")
            .push(Rig::arn());
        for round in 1..=3 {
            let swept = rig.sweep().await.expect("sweep");
            assert_eq!(swept.action, "cancel-retry");
            assert_eq!(rig.executions.stops().len(), round, "re-issued every sweep");
            assert!(!rig.lock_released().await, "held throughout");
        }
        rig.set_execution("ABORTED");
        rig.executions.running.lock().expect("lock").clear();
        let done = rig.sweep().await.expect("sweep");
        assert_eq!(done.action, "finalized");
        assert_eq!(
            rig.executions.stops().len(),
            3,
            "no more stops once it is terminal"
        );
        assert!(rig.lock_released().await);
    }

    // ------------------------------------------------------------------- reconcile

    #[tokio::test]
    async fn fin_reconcile_lost_event() {
        let mut files = passing();
        files.outcome.as_mut().expect("outcome")["exit_class"] = json!("crash");
        let rig = ready(&files).await;
        rig.set_execution("FAILED");
        // No event was ever delivered; the record is still Working.
        assert_eq!(rig.record().await.task.status, TaskStatus::Working);
        let swept = rig.sweep().await.expect("sweep");
        assert_eq!(swept.action, "finalized");
        let finalized = swept.finalized.expect("finalized");
        assert_eq!(finalized.phase, Some(Phase::Failed));
        assert_eq!(rig.record().await.task.status, TaskStatus::Failed);
        assert!(rig.lock_released().await);
    }

    #[tokio::test]
    async fn fin_reconcile_free() {
        let rig = Rig::new();
        let swept = rig.sweep().await.expect("sweep");
        assert_eq!(swept.action, "lock-free");
        assert!(rig.s3_writes().is_empty());
        assert!(rig.executions.stops().is_empty());
        assert!(rig.jobs.stops().is_empty());
        assert!(rig.packs.stops().is_empty());
        assert!(rig.log().is_empty(), "no backend write either");

        // A released lock is free too.
        let held = Rig::new();
        held.start().await;
        assert!(lock::release(held.backend.as_ref(), TASK_ID).await);
        held.log.lock().expect("lock").clear();
        let swept = held.sweep().await.expect("sweep");
        assert_eq!(swept.action, "lock-free");
        assert!(held.log().is_empty());
        assert!(held.executions.stops().is_empty());
    }

    #[tokio::test]
    async fn fin_reconcile_running() {
        let rig = ready(&passing()).await;
        rig.executions
            .running
            .lock()
            .expect("lock")
            .push(Rig::arn());
        let swept = rig.sweep().await.expect("sweep");
        assert_eq!(swept.action, "running");
        assert!(rig.s3_writes().is_empty());
        assert!(rig.executions.stops().is_empty());
        assert!(rig.jobs.stops().is_empty());
        assert!(rig.packs.stops().is_empty());
        assert!(!rig.lock_released().await);
        assert_eq!(rig.record().await.task.status, TaskStatus::Working);
    }

    #[tokio::test]
    async fn fin_reconcile_orphan() {
        // 901 s after the lock was taken, no execution exists.
        let rig = Rig::new();
        rig.start().await;
        rig.clock.store(T0 + 901, Ordering::SeqCst);
        let swept = rig.sweep().await.expect("sweep");
        assert_eq!(swept.action, "dispatch-unconfirmed");
        let rec = rig.record().await;
        assert_eq!(rec.task.status, TaskStatus::Failed);
        assert!(rig
            .stored_payload()
            .await
            .error
            .expect("error")
            .contains("dispatch-unconfirmed"));
        assert!(rig.objects.bytes(&Rig::key("verdict.json")).is_some());
        assert!(rig.lock_released().await);

        // 899 s after: still inside the grace.
        let early = Rig::new();
        early.start().await;
        early.clock.store(T0 + 899, Ordering::SeqCst);
        let swept = early.sweep().await.expect("sweep");
        assert_eq!(swept.action, "dispatch-grace");
        assert!(early.s3_writes().is_empty());
        assert_eq!(early.record().await.task.status, TaskStatus::Working);
        assert!(!early.lock_released().await);
    }

    #[tokio::test]
    async fn fin_reconcile_rogue() {
        let rig = ready(&passing()).await;
        let rogue = "arn:aws:states:us-east-1:123456789012:execution:laya-train-dev:laya-X";
        {
            let mut running = rig.executions.running.lock().expect("lock");
            running.push(rogue.to_string());
            running.push(Rig::arn());
        }
        let swept = rig.sweep().await.expect("sweep");
        assert_eq!(swept.action, "running");
        assert_eq!(rig.executions.stops(), [rogue.to_string()], "stopped once");
        assert_eq!(swept.rogue_stopped, [rogue.to_string()]);
        assert!(!rig.lock_released().await, "the holder's run is untouched");
        assert_eq!(rig.record().await.task.status, TaskStatus::Working);
    }

    // ---------------------------------------------------------------- crash resume

    /// The state an uninterrupted run ends in.
    struct Reference {
        objects: BTreeMap<String, (Vec<u8>, Option<String>)>,
        result_text: String,
        status: TaskStatus,
        tagged: usize,
    }

    async fn reference() -> Reference {
        let rig = ready(&passing()).await;
        rig.set_execution("SUCCEEDED");
        let report = rig.payload(None).await.expect("uninterrupted");
        assert!(rig.lock_released().await);
        let rec = rig.record().await;
        Reference {
            objects: rig.objects.snapshot(),
            result_text: result_text(rec.result.as_ref().expect("result"))
                .expect("text")
                .to_string(),
            status: rec.task.status,
            tagged: report.tagged,
        }
    }

    /// Run a completed run's finalization with one injected failure, then let the
    /// sweep resume it, and compare the end state with an uninterrupted run's.
    async fn crash_then_resume(inject: impl FnOnce(&Rig, &Reference), reference: &Reference) {
        let rig = ready(&passing()).await;
        rig.set_execution("SUCCEEDED");
        inject(&rig, reference);
        let err = rig.payload(None).await.expect_err("the injected failure");
        assert!(matches!(err, FinalizerError::Store(_)), "{err:?}");
        assert!(!rig.lock_released().await, "the lock is held after a crash");

        let swept = rig.sweep().await.expect("the sweep resumes it");
        assert_eq!(swept.action, "finalized");
        assert!(rig.lock_released().await);
        assert_eq!(
            rig.objects.snapshot(),
            reference.objects,
            "objects and tags"
        );
        let rec = rig.record().await;
        assert_eq!(rec.task.status, reference.status);
        assert_eq!(
            result_text(rec.result.as_ref().expect("result")),
            Some(reference.result_text.as_str())
        );
    }

    #[tokio::test]
    async fn fin_crash_resume_after_record() {
        let reference = reference().await;
        crash_then_resume(|rig, _| rig.objects.fail_nth(Op::Put, 1), &reference).await;
    }

    #[tokio::test]
    async fn fin_crash_resume_after_verdict() {
        let reference = reference().await;
        crash_then_resume(|rig, _| rig.objects.fail_nth(Op::Put, 2), &reference).await;
    }

    #[tokio::test]
    async fn fin_crash_resume_after_manifest() {
        let reference = reference().await;
        crash_then_resume(|rig, _| rig.objects.fail_nth(Op::PutTag, 1), &reference).await;
    }

    #[tokio::test]
    async fn fin_crash_resume_after_tag() {
        let reference = reference().await;
        // The first read-back follows the pre-read pass over every target.
        crash_then_resume(
            |rig, r| rig.objects.fail_nth(Op::GetTag, r.tagged + 1),
            &reference,
        )
        .await;
    }

    #[tokio::test]
    async fn fin_crash_resume_after_readback() {
        let reference = reference().await;
        crash_then_resume(
            |rig, _| rig.backend.fail_release.store(true, Ordering::SeqCst),
            &reference,
        )
        .await;
    }

    // ============================================================ failure surface
    //
    // Plan 14-16. The finalizer and state-machine boundary is ONE class: at every
    // failure point the finalizer must reach one classified terminal phase decided
    // by the most specific record, release the lock (or hold it only for a cause the
    // next pass provably clears), and expose only caller-safe text. The section
    // drives the REAL entry point (`handle`) over the cross product of the surface's
    // axes and computes the invariants I1..I8 per cell with an oracle that never
    // calls the implementation's own rules. The enumeration it is bound to is
    // `fixtures/failure-surface-v1.json` and `14-FAILURE-SURFACE.md`.
    mod failure_surface {
        use super::*;

        // -------------------------------------------------- planted hostile text

        /// An account-id-shaped digit string. Planted, never a real account.
        const HOSTILE_ACCOUNT: &str = "987654321098";
        /// An ARN that embeds [`HOSTILE_ACCOUNT`].
        const HOSTILE_ARN: &str = "arn:aws:iam::987654321098:role/laya-train-execution-dev";
        /// An ECR host that embeds [`HOSTILE_ACCOUNT`].
        const HOSTILE_ECR: &str = "987654321098.dkr.ecr.us-east-1.amazonaws.com";
        /// Every planted constant I4 looks for in caller-readable bytes.
        const HOSTILE: [&str; 3] = [HOSTILE_ACCOUNT, HOSTILE_ARN, HOSTILE_ECR];

        /// The trainer's own refusal rule (the part of the line a caller must see).
        const REFUSED_OTHER_RULE: &str = "REFUSED shots-per-class";
        /// The dataset-changed refusal rule (D-08: not a pre-filter disagreement).
        const REFUSED_INPUTS_RULE: &str = "REFUSED inputs-changed";

        /// `$$.Execution.StartTime` of the rig's execution, as the CDK passes it.
        const T0_RFC3339: &str = "2027-01-15T08:00:00Z";
        /// A task id that is not the rig's.
        const FOREIGN_TASK_ID: &str = "00000000-0000-4000-8000-0000000000ff";
        /// Bytes that are no JSON value.
        const MALFORMED: &[u8] = b"{\"schema\": \"laya-train-";

        fn refused_other_line() -> String {
            format!(
                "{REFUSED_OTHER_RULE}: label `other` has 3 shots, the floor is 8 \
                 (image {HOSTILE_ECR}/aprender-laya-train)"
            )
        }

        fn refused_inputs_line() -> String {
            format!(
                "{REFUSED_INPUTS_RULE}: train.jsonl no longer hashes to the validated \
                 digest (read by {HOSTILE_ARN})"
            )
        }

        /// A `Cause` shaped like the SageMaker `DescribeTrainingJob` answer a
        /// `.sync` task failure carries: the ECR image, the role ARN and a
        /// `FailureReason` that names both. It contains no class word (crash,
        /// runtime cap, REFUSED), so only the records can supply the class.
        fn catch_cause() -> String {
            json!({
                "AlgorithmSpecification": {
                    "TrainingImage": format!("{HOSTILE_ECR}/aprender-laya-train@sha256:{}", sha("image")),
                    "TrainingInputMode": "File",
                },
                "RoleArn": HOSTILE_ARN,
                "TrainingJobName": format!("laya-{TASK_ID}"),
                "TrainingJobArn": format!(
                    "arn:aws:sagemaker:us-east-1:{HOSTILE_ACCOUNT}:training-job/laya-{TASK_ID}"
                ),
                "TrainingJobStatus": "Failed",
                "SecondaryStatus": "Failed",
                "FailureReason": format!(
                    "AlgorithmError: the job exited 1; see {HOSTILE_ARN} in account {HOSTILE_ACCOUNT}"
                ),
            })
            .to_string()
        }

        // ------------------------------------------------------------- the axes

        /// The surface's axes, one quoted value per entry. The sweep runs their full
        /// cross product and binds every enum below to this list; the checker
        /// `scripts/laya_deploy/laya_failure_surface.py --check` parses it and
        /// demands equality with `axes` in `fixtures/failure-surface-v1.json`.
        const SURFACE_AXES: &[(&str, &[&str])] = &[
            (
                "trigger",
                &[
                    "sm_clean",
                    "sm_caught",
                    "event_failed",
                    "event_timed_out",
                    "reconcile_failed",
                ],
            ),
            (
                "outcome",
                &[
                    "absent",
                    "malformed",
                    "foreign_task",
                    "pass",
                    "gate_fail",
                    "refused_other",
                    "refused_inputs_changed",
                    "crash",
                    "timeout",
                    "cpu_fallback",
                ],
            ),
            ("gate_report", &["ok", "absent"]),
            ("pack", &["absent", "accepted", "refused", "malformed"]),
            ("listing", &["complete", "partial_only", "none"]),
            (
                "absent_listed",
                &["none", "outcome", "pack_outcome", "gate_report"],
            ),
        ];

        /// The exit classes whose training job exits 1 (run_step.py `job_exit_code`),
        /// so the machine delivers them ONLY through `train.addCatch` with a populated
        /// stage error. The checker re-derives this by running `job_exit_code`.
        const CATCH_FED: &[&str] = &["refused", "crash", "timeout"];

        /// One axis of the surface: a closed enum whose wire names are the values
        /// `SURFACE_AXES` lists (the sweep binds the two).
        macro_rules! axis {
            ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
                #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
                enum $name { $($variant),+ }
                impl $name {
                    const ALL: &'static [Self] = &[$(Self::$variant),+];
                    fn name(self) -> &'static str {
                        match self { $(Self::$variant => $text),+ }
                    }
                }
            };
        }

        axis!(Via {
            SmClean => "sm_clean",
            SmCaught => "sm_caught",
            EventFailed => "event_failed",
            EventTimedOut => "event_timed_out",
            ReconcileFailed => "reconcile_failed",
        });
        axis!(OutcomeKind {
            Absent => "absent",
            Malformed => "malformed",
            ForeignTask => "foreign_task",
            Pass => "pass",
            GateFail => "gate_fail",
            RefusedOther => "refused_other",
            RefusedInputsChanged => "refused_inputs_changed",
            Crash => "crash",
            Timeout => "timeout",
            CpuFallback => "cpu_fallback",
        });
        axis!(GateKind {
            Ok => "ok",
            Absent => "absent",
        });
        axis!(PackKind {
            Absent => "absent",
            Accepted => "accepted",
            Refused => "refused",
            Malformed => "malformed",
        });
        axis!(ListingKind {
            Complete => "complete",
            PartialOnly => "partial_only",
            None => "none",
        });
        axis!(AbsentKind {
            None => "none",
            Outcome => "outcome",
            PackOutcome => "pack_outcome",
            GateReport => "gate_report",
        });

        /// One point of the surface: how the finalizer is reached, and what the jobs
        /// left under the run prefix.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        struct Cell {
            trigger: Via,
            outcome: OutcomeKind,
            gate_report: GateKind,
            pack: PackKind,
            listing: ListingKind,
            /// An object a writer manifest lists that S3 does not hold (WR-02).
            absent_listed: AbsentKind,
        }

        impl Default for Cell {
            /// The nominal passing run, delivered by the machine's own Finalize.
            fn default() -> Self {
                Self {
                    trigger: Via::SmClean,
                    outcome: OutcomeKind::Pass,
                    gate_report: GateKind::Ok,
                    pack: PackKind::Accepted,
                    listing: ListingKind::Complete,
                    absent_listed: AbsentKind::None,
                }
            }
        }

        impl Cell {
            fn label(&self) -> String {
                format!(
                    "trigger={} outcome={} gate_report={} pack={} listing={} absent_listed={}",
                    self.trigger.name(),
                    self.outcome.name(),
                    self.gate_report.name(),
                    self.pack.name(),
                    self.listing.name(),
                    self.absent_listed.name()
                )
            }
        }

        // ------------------------------------------------------------ the files

        fn outcome_json(kind: OutcomeKind) -> Option<Value> {
            let mut o = parse(OUTCOME);
            match kind {
                // Malformed bytes are planted raw by `setup`.
                OutcomeKind::Absent | OutcomeKind::Malformed => return None,
                OutcomeKind::ForeignTask => o["task_id"] = json!(FOREIGN_TASK_ID),
                OutcomeKind::Pass => {}
                OutcomeKind::GateFail => {
                    o["exit_class"] = json!("gate_fail");
                    o["train_rc"] = json!(3);
                }
                OutcomeKind::RefusedOther | OutcomeKind::RefusedInputsChanged => {
                    o["exit_class"] = json!("refused");
                    o["train_rc"] = json!(2);
                    o["device_used"] = json!("none");
                    o["device_ok"] = json!(false);
                    o["refused_line"] = json!(if kind == OutcomeKind::RefusedOther {
                        refused_other_line()
                    } else {
                        refused_inputs_line()
                    });
                }
                OutcomeKind::Crash => {
                    o["exit_class"] = json!("crash");
                    o["train_rc"] = json!(1);
                }
                OutcomeKind::Timeout => {
                    o["exit_class"] = json!("timeout");
                    o["train_rc"] = Value::Null;
                    o["device_ok"] = json!(false);
                }
                // run_step.py: a CPU fallback is classified crash with device_ok false.
                OutcomeKind::CpuFallback => {
                    o["exit_class"] = json!("crash");
                    o["train_rc"] = Value::Null;
                    o["device_used"] = json!("cpu");
                    o["device_ok"] = json!(false);
                }
            }
            Some(o)
        }

        fn pack_outcome_json(kind: PackKind) -> Option<Value> {
            let mut p = parse(PACK_OUTCOME);
            match kind {
                PackKind::Absent | PackKind::Malformed => return None,
                PackKind::Accepted => {
                    p["packed_line"] = json!(format!("PACKED waived=none image={HOSTILE_ECR}"));
                    p["verify_line"] = json!(format!(
                        "{{\"deploy_eligible\":true,\"waived\":null,\"role\":\"{HOSTILE_ARN}\"}}"
                    ));
                }
                PackKind::Refused => {
                    p["accepted"] = json!(false);
                    p["apr_sha256"] = Value::Null;
                    p["exit_code"] = json!(3);
                    p["deploy_eligible"] = json!(false);
                    p["shipped_seed"] = Value::Null;
                    p["packed_line"] =
                        json!(format!("REFUSED RescoreDrift: pack read {HOSTILE_ECR}"));
                    p["verify_line"] = Value::Null;
                }
            }
            Some(p)
        }

        /// The pack inventory: the `.apr` only for an accepted pack.
        fn pack_manifest_for(kind: PackKind) -> Option<Value> {
            match kind {
                PackKind::Absent => None,
                PackKind::Accepted => Some(pack_manifest_json()),
                PackKind::Refused | PackKind::Malformed => {
                    let mut m = pack_manifest_json();
                    if let Some(rows) = m.as_array_mut() {
                        rows.retain(|r| r["path"] != "fixture-model.apr");
                    }
                    Some(m)
                }
            }
        }

        /// The files a cell's jobs wrote. An `absent` record is neither written nor
        /// listed; a listed-but-absent object is listed here and removed by `setup`.
        fn files_for(cell: &Cell) -> Files {
            let mut manifest = parse(TRAINING_MANIFEST);
            if let Some(rows) = manifest["entries"].as_array_mut() {
                rows.retain(|r| {
                    let path = r["path"].as_str().unwrap_or_default();
                    !(path == layout::OUTCOME
                        && cell.outcome == OutcomeKind::Absent
                        && cell.absent_listed != AbsentKind::Outcome)
                        && !(path == "run/gate-report.json"
                            && cell.gate_report == GateKind::Absent
                            && cell.absent_listed != AbsentKind::GateReport)
                });
            }
            let (training_manifest, partial_manifest) = match cell.listing {
                ListingKind::Complete => (Some(manifest), None),
                ListingKind::PartialOnly => {
                    manifest["complete"] = json!(false);
                    (None, Some(manifest))
                }
                ListingKind::None => (None, None),
            };
            let gate_report = match cell.gate_report {
                GateKind::Absent => None,
                GateKind::Ok => {
                    let mut report = parse(GATE_REPORT);
                    if cell.outcome == OutcomeKind::GateFail {
                        report["pass"] = json!(false);
                    }
                    Some(report)
                }
            };
            Files {
                outcome: outcome_json(cell.outcome),
                gate_report,
                training_manifest,
                partial_manifest,
                pack_outcome: pack_outcome_json(cell.pack),
                pack_manifest: pack_manifest_for(cell.pack),
                extra_objects: Vec::new(),
            }
        }

        /// The exit class `outcome.json` records for this kind, when it parses.
        fn exit_class(kind: OutcomeKind) -> Option<&'static str> {
            match kind {
                OutcomeKind::Absent | OutcomeKind::Malformed | OutcomeKind::ForeignTask => None,
                OutcomeKind::Pass => Some("pass"),
                OutcomeKind::GateFail => Some("gate_fail"),
                OutcomeKind::RefusedOther | OutcomeKind::RefusedInputsChanged => Some("refused"),
                OutcomeKind::Crash | OutcomeKind::CpuFallback => Some("crash"),
                OutcomeKind::Timeout => Some("timeout"),
            }
        }

        fn job_status(outcome: OutcomeKind) -> &'static str {
            match outcome {
                OutcomeKind::Pass | OutcomeKind::GateFail => "Completed",
                OutcomeKind::Timeout => "Stopped",
                _ => "Failed",
            }
        }

        /// A rig whose run is started, whose objects are the cell's, and whose
        /// compute is over (the gate the cross product does not vary).
        async fn setup(cell: &Cell) -> Rig {
            let rig = Rig::new();
            rig.start().await;
            rig.seed(&files_for(cell));
            match cell.outcome {
                OutcomeKind::Absent => rig.objects.remove(&Rig::key(layout::OUTCOME)),
                OutcomeKind::Malformed => rig.objects.seed(&Rig::key(layout::OUTCOME), MALFORMED),
                _ => {}
            }
            if cell.gate_report == GateKind::Absent {
                rig.objects.remove(&Rig::key("run/gate-report.json"));
            }
            match cell.pack {
                PackKind::Absent => rig.objects.remove(&Rig::key(layout::PACK_OUTCOME)),
                PackKind::Malformed => {
                    rig.objects.seed(&Rig::key(layout::PACK_OUTCOME), MALFORMED);
                }
                PackKind::Accepted | PackKind::Refused => {}
            }
            match cell.absent_listed {
                AbsentKind::None => {}
                AbsentKind::Outcome => rig.objects.remove(&Rig::key(layout::OUTCOME)),
                AbsentKind::PackOutcome => rig.objects.remove(&Rig::key(layout::PACK_OUTCOME)),
                AbsentKind::GateReport => rig.objects.remove(&Rig::key("run/gate-report.json")),
            }
            rig.jobs
                .set(&training_job_name(TASK_ID), job_status(cell.outcome));
            rig.set_execution("RUNNING");
            rig
        }

        // ---------------------------------------------------------- the delivery

        /// The Finalize / CatchFinalize payload exactly as the CDK builds it
        /// (`executionRef`, plus `error: $.error` for the Catch).
        fn machine_payload(rig: &Rig, error: Option<Value>) -> Value {
            let mut value = json!({
                "source": "state-machine",
                "execution_arn": Rig::arn(),
                "start_time": T0_RFC3339,
                "input": rig.exec_input(),
            });
            if let Some(error) = error {
                value["error"] = error;
            }
            value
        }

        /// Drive the REAL entry point the way the trigger reaches it: payload parse,
        /// check_execution, finalize_task, read_run_objects, finalize_run,
        /// settle_record, publish, tags, lock release.
        async fn deliver(rig: &Rig, cell: &Cell) -> Result<FinalizeReport, FinalizerError> {
            let value = match cell.trigger {
                Via::SmClean => machine_payload(rig, None),
                Via::SmCaught => machine_payload(
                    rig,
                    Some(json!({"Error": "States.TaskFailed", "Cause": catch_cause()})),
                ),
                Via::EventFailed => {
                    rig.set_execution("FAILED");
                    event_json("FAILED", MACHINE)
                }
                Via::EventTimedOut => {
                    rig.set_execution("TIMED_OUT");
                    event_json("TIMED_OUT", MACHINE)
                }
                Via::ReconcileFailed => {
                    rig.set_execution("FAILED");
                    json!({"source": "reconcile"})
                }
            };
            handle(&rig.ctx, FinalizerInput::parse(&value)?).await
        }

        /// How the execution ends after the delivery (what a later sweep describes).
        fn end_status(trigger: Via) -> &'static str {
            match trigger {
                // Finalize and CatchFinalize both succeed, so the machine SUCCEEDS.
                Via::SmClean | Via::SmCaught => "SUCCEEDED",
                Via::EventFailed | Via::ReconcileFailed => "FAILED",
                Via::EventTimedOut => "TIMED_OUT",
            }
        }

        /// What one delivery left behind, read from the records (never a log line).
        struct Observed {
            first: Result<FinalizeReport, FinalizerError>,
            record: Option<StoredTask>,
            payload_text: Option<String>,
            payload: Option<StatusPayload>,
            verdict: Option<Vec<u8>>,
            manifest: Option<Vec<u8>>,
            /// Every object's retention tag after the first delivery.
            tags: BTreeMap<String, Option<String>>,
            /// The lock after the first delivery.
            released: bool,
            /// A reconcile sweep after the execution ended.
            second: Result<ReconcileReport, FinalizerError>,
        }

        async fn observe(
            rig: &Rig,
            cell: &Cell,
            first: Result<FinalizeReport, FinalizerError>,
        ) -> Observed {
            let record = rig.backend.get(&rig.owner, TASK_ID).await.expect("get");
            let payload_text = record
                .as_ref()
                .and_then(|r| r.result.as_ref())
                .and_then(result_text)
                .map(str::to_string);
            let payload = payload_text
                .as_deref()
                .and_then(|t| serde_json::from_str::<StatusPayload>(t).ok());
            let tags = rig
                .objects
                .snapshot()
                .into_iter()
                .map(|(key, (_, tag))| (key, tag))
                .collect();
            let released = rig.lock_released().await;
            rig.set_execution(end_status(cell.trigger));
            let second = rig.sweep().await;
            Observed {
                first,
                record,
                payload_text,
                payload,
                verdict: rig.objects.bytes(&Rig::key(layout::VERDICT)),
                manifest: rig.objects.bytes(&Rig::key(layout::MANIFEST)),
                tags,
                released,
                second,
            }
        }

        async fn run(cell: &Cell) -> Observed {
            let rig = setup(cell).await;
            let first = deliver(&rig, cell).await;
            observe(&rig, cell, first).await
        }

        // ------------------------------------------------------- the invariants

        /// The class invariant, as eight numbered clauses (14-FAILURE-SURFACE.md).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        enum Invariant {
            /// Terminal and classified.
            I1,
            /// Finalization is total and the lock is released.
            I2,
            /// The class is decided from the records.
            I3,
            /// Caller-safe text.
            I4,
            /// No `.apr` unless completed.
            I5,
            /// Aborted means cancelled.
            I6,
            /// Retention tags.
            I7,
            /// Delivery-path independence.
            I8,
        }

        impl Invariant {
            const ALL: [Self; 8] = [
                Self::I1,
                Self::I2,
                Self::I3,
                Self::I4,
                Self::I5,
                Self::I6,
                Self::I7,
                Self::I8,
            ];
        }

        /// The shrink-only ratchet: the invariants the sweep finds violated at HEAD
        /// 359788182. The sweep must detect EVERY entry at least once (a stale entry
        /// fails it) and nothing outside it. 14-17 deletes I2, I3, I7 and I8; 14-18
        /// deletes I4. Never add an entry to make the sweep pass.
        const KNOWN_DEFECTS: &[Invariant] = &[
            Invariant::I4, // WR-03: AWS text reaches the stored payload
        ];

        /// What the records alone decide, by the class table (not by `finalize_run`).
        struct Expected {
            phase: Phase,
            /// `Some` where D-08 fixes the disagreement flag.
            disagreement: Option<bool>,
            /// Text the error must carry (the class's own words).
            error_contains: Option<&'static str>,
            /// True when the error must be non-empty.
            error_nonempty: bool,
        }

        fn expected(cell: &Cell) -> Expected {
            let failed = |disagreement, error_contains| Expected {
                phase: Phase::Failed,
                disagreement,
                error_contains,
                error_nonempty: true,
            };
            let outcome_parses = !matches!(
                cell.outcome,
                OutcomeKind::Absent | OutcomeKind::Malformed | OutcomeKind::ForeignTask
            ) && cell.absent_listed != AbsentKind::Outcome;
            if !outcome_parses {
                return failed(None, None);
            }
            let gate_present =
                cell.gate_report == GateKind::Ok && cell.absent_listed != AbsentKind::GateReport;
            let pack_accepted =
                cell.pack == PackKind::Accepted && cell.absent_listed != AbsentKind::PackOutcome;
            match cell.outcome {
                OutcomeKind::RefusedOther => failed(Some(true), Some(REFUSED_OTHER_RULE)),
                OutcomeKind::RefusedInputsChanged => failed(Some(false), Some(REFUSED_INPUTS_RULE)),
                OutcomeKind::Crash => failed(None, Some("crash")),
                OutcomeKind::Timeout => failed(None, Some("runtime cap")),
                OutcomeKind::CpuFallback => failed(None, None),
                OutcomeKind::Pass if gate_present && pack_accepted => Expected {
                    phase: Phase::Completed,
                    disagreement: Some(false),
                    error_contains: None,
                    error_nonempty: false,
                },
                OutcomeKind::GateFail if gate_present => Expected {
                    phase: Phase::GateFailed,
                    disagreement: Some(false),
                    error_contains: None,
                    error_nonempty: false,
                },
                _ => failed(None, None),
            }
        }

        fn clip_debug(value: &impl std::fmt::Debug) -> String {
            format!("{value:?}").chars().take(240).collect()
        }

        /// Every invariant `obs` violates, with the reason. Computed from the
        /// records the finalizer left, never from its own rules or a log line.
        fn findings(cell: &Cell, obs: &Observed) -> Vec<(Invariant, String)> {
            let mut out = Vec::new();

            // I1: an Ok finalization left a terminal record with a classified result.
            if obs.first.is_ok() {
                let classified = obs.payload.as_ref().is_some_and(|p| {
                    matches!(
                        p.phase,
                        Phase::Completed | Phase::GateFailed | Phase::Failed | Phase::Cancelled
                    )
                });
                let terminal = obs
                    .record
                    .as_ref()
                    .is_some_and(|r| r.task.status.is_terminal());
                if !(classified && terminal) {
                    out.push((
                        Invariant::I1,
                        "Ok without a terminal classified record".into(),
                    ));
                }
            }

            // I2: with no injected fault the first pass is Ok and releases the lock,
            // and a later sweep is Ok too.
            if !(obs.first.is_ok() && obs.released && obs.second.is_ok()) {
                out.push((
                    Invariant::I2,
                    format!(
                        "first={} released={} sweep={}",
                        clip_debug(&obs.first.as_ref().err()),
                        obs.released,
                        clip_debug(&obs.second.as_ref().err())
                    ),
                ));
            }
            // I2, totality over the objects: an object the cell's manifests list but
            // S3 does not hold is listed by neither the payload nor manifest.json, and
            // the payload names it in exactly one listed-but-absent note (D-11, D-18);
            // with nothing removed there is no such note.
            if let Some(payload) = &obs.payload {
                if let Some(why) = absent_listing_mismatch(cell, obs, payload) {
                    out.push((Invariant::I2, why));
                }
            }

            // I3: the phase follows the records' class table, not the delivery path.
            if let Some(payload) = &obs.payload {
                let want = expected(cell);
                let error = payload.error.as_deref().unwrap_or_default();
                let mut wrong = Vec::new();
                if payload.phase != want.phase {
                    wrong.push(format!("phase {:?} not {:?}", payload.phase, want.phase));
                }
                if want
                    .disagreement
                    .is_some_and(|flag| payload.prefilter_disagreement != flag)
                {
                    wrong.push(format!(
                        "prefilter_disagreement {}",
                        payload.prefilter_disagreement
                    ));
                }
                if want
                    .error_contains
                    .is_some_and(|needle| !error.contains(needle))
                {
                    wrong.push(format!("error lacks {:?}", want.error_contains));
                }
                if want.error_nonempty && error.is_empty() {
                    wrong.push("error is empty".into());
                }
                // A non-error phase the records decided over the Catch's stage error
                // says so in the payload (D-18): the note on that delivery, and on no
                // other delivery of the same records. Read from the stored payload by
                // its fixed prefix, never through the implementation's constant.
                let want_note = cell.trigger == Via::SmCaught
                    && matches!(want.phase, Phase::Completed | Phase::GateFailed);
                let has_note = payload
                    .notes
                    .iter()
                    .any(|n| n.starts_with("stage-error-overridden"));
                if has_note != want_note {
                    wrong.push(format!(
                        "stage-error-overridden note {has_note}, want {want_note}"
                    ));
                }
                if !wrong.is_empty() {
                    out.push((Invariant::I3, wrong.join("; ")));
                }
            }

            // I4: no planted constant in anything a caller can read.
            if let Some(leaks) = hostile_leaks(obs, &HOSTILE) {
                out.push((Invariant::I4, leaks));
            }

            // I5: no `.apr` listed unless the phase is completed.
            if let Some(why) = apr_unless_completed(obs) {
                out.push((Invariant::I5, why));
            }

            // I7: every key the finalizer inventories or names carries the run's
            // retention tag, read back from the bucket state (D-19).
            if obs.first.is_ok() {
                if let Some(why) = retention_mismatch(obs) {
                    out.push((Invariant::I7, why));
                }
            }
            out
        }

        /// The relative path the cell removed although one of its manifests lists it,
        /// derived from the cell's own files (not from the implementation).
        fn removed_listed(cell: &Cell) -> Option<&'static str> {
            let path = match cell.absent_listed {
                AbsentKind::None => return None,
                AbsentKind::Outcome => layout::OUTCOME,
                AbsentKind::PackOutcome => layout::PACK_OUTCOME,
                AbsentKind::GateReport => "run/gate-report.json",
            };
            let files = files_for(cell);
            let listed = [
                &files.training_manifest,
                &files.partial_manifest,
                &files.pack_manifest,
            ]
            .into_iter()
            .flatten()
            .flat_map(entry_paths)
            .any(|listed| listed == path);
            listed.then_some(path)
        }

        fn absent_listing_mismatch(
            cell: &Cell,
            obs: &Observed,
            payload: &StatusPayload,
        ) -> Option<String> {
            let notes: Vec<&String> = payload
                .notes
                .iter()
                .filter(|n| n.starts_with("listed-but-absent"))
                .collect();
            let Some(path) = removed_listed(cell) else {
                return (!notes.is_empty())
                    .then(|| format!("nothing removed, but notes {notes:?}"));
            };
            let mut wrong = Vec::new();
            let named = |note: &&String| {
                note.split_once(" dropped: ")
                    .is_some_and(|(_, names)| names.split(", ").any(|n| n == path))
            };
            if notes.len() != 1 || !notes.iter().any(named) {
                wrong.push(format!("{path} removed, notes {:?}", payload.notes));
            }
            if payload.manifest.iter().any(|l| l.path == path) {
                wrong.push(format!("the stored payload lists the absent {path}"));
            }
            let in_manifest = obs
                .manifest
                .as_deref()
                .and_then(|b| serde_json::from_slice::<Manifest>(b).ok())
                .is_some_and(|m| m.entries.iter().any(|e| e.path == path));
            if in_manifest {
                wrong.push(format!("manifest.json lists the absent {path}"));
            }
            (!wrong.is_empty()).then(|| wrong.join("; "))
        }

        /// The caller-readable surfaces: the stored payload, verdict.json, manifest.json.
        fn caller_surfaces(obs: &Observed) -> [(&'static str, Option<&[u8]>); 3] {
            [
                (
                    "stored payload",
                    obs.payload_text.as_deref().map(str::as_bytes),
                ),
                ("verdict.json", obs.verdict.as_deref()),
                ("manifest.json", obs.manifest.as_deref()),
            ]
        }

        fn hostile_leaks(obs: &Observed, needles: &[&str]) -> Option<String> {
            let leaks: Vec<String> = caller_surfaces(obs)
                .into_iter()
                .filter_map(|(name, bytes)| {
                    let text = String::from_utf8_lossy(bytes?);
                    let found: Vec<&str> = needles
                        .iter()
                        .copied()
                        .filter(|n| text.contains(n))
                        .collect();
                    (!found.is_empty()).then(|| format!("{name} carries {found:?}"))
                })
                .collect();
            (!leaks.is_empty()).then(|| leaks.join("; "))
        }

        fn apr_unless_completed(obs: &Observed) -> Option<String> {
            let phase = obs.payload.as_ref()?.phase;
            if phase == Phase::Completed {
                return None;
            }
            let in_payload = obs
                .payload
                .as_ref()
                .is_some_and(|p| p.manifest.iter().any(|l| l.path.ends_with(".apr")));
            let in_manifest = obs
                .manifest
                .as_deref()
                .and_then(|b| serde_json::from_slice::<Manifest>(b).ok())
                .is_some_and(|m| m.entries.iter().any(|e| e.path.ends_with(".apr")));
            (in_payload || in_manifest).then(|| format!("{phase:?} lists an .apr"))
        }

        /// The keys D-19 retains together: every manifest.json entry, manifest.json
        /// itself, and every self-listing inventory that exists.
        fn retention_mismatch(obs: &Observed) -> Option<String> {
            let payload = obs.payload.as_ref()?;
            let keep = payload.phase == Phase::Completed
                && payload.verdict.as_ref().is_some_and(|v| v.pass);
            let want = if keep {
                RETENTION_KEEP
            } else {
                RETENTION_EXPIRE
            };
            let manifest: Manifest = serde_json::from_slice(obs.manifest.as_deref()?).ok()?;
            let mut keys: Vec<String> =
                manifest.entries.iter().map(|e| Rig::key(&e.path)).collect();
            keys.push(Rig::key(layout::MANIFEST));
            for name in [
                layout::TRAINING_MANIFEST,
                layout::PARTIAL_MANIFEST,
                layout::PACK_MANIFEST,
            ] {
                keys.push(Rig::key(name));
            }
            let wrong: Vec<String> = keys
                .iter()
                // A key the bucket does not hold cannot carry a tag (I2's concern).
                .filter_map(|key| obs.tags.get(key).map(|tag| (key, tag)))
                .filter(|(_, tag)| tag.as_deref() != Some(want))
                .map(|(key, tag)| format!("{key}={tag:?}"))
                .collect();
            (!wrong.is_empty()).then(|| format!("want {want}: {}", wrong.join(", ")))
        }

        /// What I8 compares across deliveries of the same records. `notes` is left
        /// out ON PURPOSE: only the state-machine Catch delivers a stage error, so the
        /// stage-error-overridden note legitimately differs by delivery (I3 pins
        /// exactly where it must and must not appear).
        #[derive(Debug, Clone, PartialEq, Eq)]
        struct Delivered {
            phase: Phase,
            is_error: bool,
            prefilter_disagreement: bool,
            keep: bool,
        }

        fn delivered(obs: &Observed) -> Option<Delivered> {
            let report = obs.first.as_ref().ok()?;
            let payload = obs.payload.as_ref()?;
            Some(Delivered {
                phase: payload.phase,
                is_error: obs.record.as_ref()?.result.as_ref()?.is_error,
                prefilter_disagreement: payload.prefilter_disagreement,
                keep: report.keep,
            })
        }

        /// The cell with its trigger erased: the records a group of deliveries share.
        fn records_of(cell: &Cell) -> Cell {
            Cell {
                trigger: Via::SmClean,
                ..*cell
            }
        }

        /// I8: every non-aborted trigger that returned Ok reaches the same phase,
        /// is_error, prefilter_disagreement and keep for the same records. Returns
        /// the first offending cell of the group and why.
        fn delivery_findings(group: &[(Cell, Option<Delivered>)]) -> Option<(Cell, String)> {
            let mut ok = group.iter().filter_map(|(c, d)| d.as_ref().map(|d| (c, d)));
            let (_, first) = ok.next()?;
            let (cell, _) = ok.find(|(_, d)| *d != first)?;
            let all: Vec<String> = group
                .iter()
                .map(|(c, d)| format!("{}={d:?}", c.trigger.name()))
                .collect();
            Some((*cell, all.join(" | ")))
        }

        /// I6 and I5 for an ABORTED execution: the Event path yields cancelled.
        async fn aborted_findings(cell: &Cell) -> Vec<(Invariant, String)> {
            let rig = setup(cell).await;
            rig.set_execution("ABORTED");
            rig.jobs.set(&training_job_name(TASK_ID), "Stopped");
            let first = rig.event("ABORTED").await;
            let rec = rig.backend.get(&rig.owner, TASK_ID).await.expect("get");
            let payload_text = rec
                .as_ref()
                .and_then(|r| r.result.as_ref())
                .and_then(result_text)
                .map(str::to_string);
            let payload = payload_text
                .as_deref()
                .and_then(|t| serde_json::from_str::<StatusPayload>(t).ok());
            let mut out = Vec::new();
            let cancelled = payload
                .as_ref()
                .is_some_and(|p| p.phase == Phase::Cancelled)
                && rec
                    .as_ref()
                    .is_some_and(|r| r.task.status == TaskStatus::Cancelled);
            if !cancelled {
                out.push((
                    Invariant::I6,
                    format!(
                        "phase {:?}, first {}",
                        payload.as_ref().map(|p| p.phase),
                        clip_debug(&first.as_ref().err())
                    ),
                ));
            }
            let obs = Observed {
                first,
                record: rec,
                payload_text,
                payload,
                verdict: rig.objects.bytes(&Rig::key(layout::VERDICT)),
                manifest: rig.objects.bytes(&Rig::key(layout::MANIFEST)),
                tags: BTreeMap::new(),
                released: rig.lock_released().await,
                second: Ok(ReconcileReport::default()),
            };
            if let Some(why) = apr_unless_completed(&obs) {
                out.push((Invariant::I5, why));
            }
            out
        }

        fn violations(cell: &Cell, obs: &Observed) -> Vec<Invariant> {
            findings(cell, obs)
                .into_iter()
                .map(|(inv, _)| inv)
                .collect()
        }

        // ------------------------------------------------------------ the tracer

        /// One caught-refused cell through every layer: the machine's Catch hands
        /// the finalizer a stage error for a trainer refusal the pre-filter
        /// accepted. Plan 14-16 proved the harness saw the WR-01 defect (I3) here;
        /// since 14-17 the records decide, so I1, I2 and I3 all hold.
        #[tokio::test]
        async fn failure_surface_tracer_wr01() {
            assert_eq!(parse_time_secs(&json!(T0_RFC3339)), Some(T0));
            assert_eq!(Invariant::ALL.len(), 8);
            let cell = Cell {
                trigger: Via::SmCaught,
                outcome: OutcomeKind::RefusedOther,
                ..Cell::default()
            };
            let obs = run(&cell).await;

            assert!(obs.first.is_ok(), "{:?}", obs.first);
            let record = obs.record.as_ref().expect("a record");
            assert_eq!(record.task.status, TaskStatus::Failed);
            assert!(record.result.is_some(), "a stored result");
            assert!(obs.released, "the lock is released");
            let payload = obs.payload.as_ref().expect("a stored payload");
            // WR-01 closed: the trainer's REFUSED line and the D-08 flag reach the
            // caller, not the Catch's text.
            assert!(
                payload.error.as_deref().is_some_and(
                    |e| e.contains(REFUSED_OTHER_RULE) && !e.starts_with("States.TaskFailed")
                ),
                "{:?}",
                payload.error
            );
            assert!(payload.prefilter_disagreement, "D-08");
            assert!(payload.notes.is_empty(), "{:?}", payload.notes);

            // The tracer models a delivery the machine really makes.
            assert!(exit_class(cell.outcome).is_some_and(|c| CATCH_FED.contains(&c)));

            // Its scope is WR-01's: no violation in scope. The planted Cause still
            // leaks (I4, WR-03, pinned for 14-18); whatever else the cell violates
            // must be a pinned defect, so the tracer cannot hide an unpinned one.
            let all = violations(&cell, &obs);
            let scoped: Vec<Invariant> = all
                .iter()
                .copied()
                .filter(|inv| TRACER_SCOPE.contains(inv))
                .collect();
            assert_eq!(
                scoped,
                Vec::<Invariant>::new(),
                "{} {:?}",
                cell.label(),
                findings(&cell, &obs)
            );
            assert!(!KNOWN_DEFECTS.contains(&Invariant::I3));
            assert!(
                all.iter().all(|inv| KNOWN_DEFECTS.contains(inv)),
                "{all:?} outside {KNOWN_DEFECTS:?}"
            );
        }

        /// The invariants the WR-01 tracer is about.
        const TRACER_SCOPE: [Invariant; 3] = [Invariant::I1, Invariant::I2, Invariant::I3];

        fn all_cells() -> Vec<Cell> {
            let mut cells = Vec::new();
            for &outcome in OutcomeKind::ALL {
                for &gate_report in GateKind::ALL {
                    for &pack in PackKind::ALL {
                        for &listing in ListingKind::ALL {
                            for &absent_listed in AbsentKind::ALL {
                                for &trigger in Via::ALL {
                                    cells.push(Cell {
                                        trigger,
                                        outcome,
                                        gate_report,
                                        pack,
                                        listing,
                                        absent_listed,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            cells
        }

        /// The enums ARE the axes: each one's wire names equal its SURFACE_AXES row.
        fn assert_axes_bound() {
            let names = |all: &[&'static str]| all.to_vec();
            let bound: [(&str, Vec<&str>); 6] = [
                ("trigger", Via::ALL.iter().map(|v| v.name()).collect()),
                (
                    "outcome",
                    OutcomeKind::ALL.iter().map(|v| v.name()).collect(),
                ),
                (
                    "gate_report",
                    GateKind::ALL.iter().map(|v| v.name()).collect(),
                ),
                ("pack", PackKind::ALL.iter().map(|v| v.name()).collect()),
                (
                    "listing",
                    ListingKind::ALL.iter().map(|v| v.name()).collect(),
                ),
                (
                    "absent_listed",
                    AbsentKind::ALL.iter().map(|v| v.name()).collect(),
                ),
            ];
            assert_eq!(SURFACE_AXES.len(), bound.len());
            for ((axis, values), (name, enum_values)) in SURFACE_AXES.iter().zip(bound) {
                assert_eq!(*axis, name);
                assert_eq!(names(values), enum_values, "axis {axis}");
            }
        }

        /// THE gate of the class: every cell of the cross product through the real
        /// entry point, I1..I8 per cell, and the violated set must equal the ratchet
        /// exactly (a pinned entry nothing violates fails it, as does a new one).
        #[tokio::test]
        async fn failure_surface_sweep_known_defects_only() {
            assert_axes_bound();
            let product: usize = SURFACE_AXES.iter().map(|(_, v)| v.len()).product();
            let started = std::time::Instant::now();

            let mut first_offender: BTreeMap<Invariant, (Cell, String)> = BTreeMap::new();
            let mut counts: BTreeMap<Invariant, BTreeMap<&'static str, usize>> = BTreeMap::new();
            let mut note = |inv: Invariant, cell: Cell, why: String| {
                *counts
                    .entry(inv)
                    .or_default()
                    .entry(cell.trigger.name())
                    .or_default() += 1;
                first_offender.entry(inv).or_insert((cell, why));
            };
            let mut groups: BTreeMap<Cell, Vec<(Cell, Option<Delivered>)>> = BTreeMap::new();
            let cells = all_cells();
            let mut ran = 0_usize;
            for cell in &cells {
                let obs = run(cell).await;
                ran += 1;
                for (inv, why) in findings(cell, &obs) {
                    note(inv, *cell, why);
                }
                groups
                    .entry(records_of(cell))
                    .or_default()
                    .push((*cell, delivered(&obs)));
            }
            assert_eq!(ran, product, "the cross product was truncated");
            assert_eq!(cells.len(), product);

            for group in groups.values() {
                if let Some((cell, why)) = delivery_findings(group) {
                    note(Invariant::I8, cell, why);
                }
            }

            // An ABORTED execution, over every outcome: the Event path's own table.
            let mut aborted = 0_usize;
            for &outcome in OutcomeKind::ALL {
                let cell = Cell {
                    outcome,
                    ..Cell::default()
                };
                aborted += 1;
                for (inv, why) in aborted_findings(&cell).await {
                    note(inv, cell, format!("ABORTED: {why}"));
                }
            }

            println!(
                "FAILURE SURFACE SWEEP: {ran} cells ({} axes) + {aborted} aborted cells in {:.1}s",
                SURFACE_AXES.len(),
                started.elapsed().as_secs_f64()
            );
            for inv in Invariant::ALL {
                match first_offender.get(&inv) {
                    Some((cell, why)) => println!(
                        "  {inv:?} violated in {} cells {:?}; first: {} -- {why}",
                        counts[&inv].values().sum::<usize>(),
                        counts[&inv],
                        cell.label()
                    ),
                    None => println!("  {inv:?} holds"),
                }
            }

            let violated: BTreeSet<Invariant> = first_offender.keys().copied().collect();
            let pinned: BTreeSet<Invariant> = KNOWN_DEFECTS.iter().copied().collect();
            assert_eq!(
                pinned.len(),
                KNOWN_DEFECTS.len(),
                "a ratchet entry is duplicated"
            );
            let stale: Vec<&Invariant> = pinned.difference(&violated).collect();
            let unpinned: Vec<&Invariant> = violated.difference(&pinned).collect();
            assert!(
                stale.is_empty(),
                "ratchet entries no cell violates (delete them): {stale:?}"
            );
            assert!(
                unpinned.is_empty(),
                "violations outside the ratchet: {:?}",
                unpinned
                    .iter()
                    .map(|inv| (inv, &first_offender[inv].1, first_offender[inv].0.label()))
                    .collect::<Vec<_>>()
            );
            assert!(
                started.elapsed() < std::time::Duration::from_secs(60),
                "the sweep must stay under 60 s"
            );
        }

        /// Every single S3 call of a passing run failing once (with planted hostile
        /// text) is healed by the next pass: Ok, the lock released, and the same
        /// objects, tags and record as an uninjected run.
        #[tokio::test]
        async fn failure_surface_transient_faults() {
            let nominal = Cell::default();
            let reference = setup(&nominal).await;
            let report = deliver(&reference, &nominal)
                .await
                .expect("the uninjected run");
            assert!(report.released);
            let want_objects = reference.objects.snapshot();
            let want_record = reference.record().await;
            let want_text = result_text(want_record.result.as_ref().expect("result"))
                .expect("text")
                .to_string();

            let mut failures: Vec<String> = Vec::new();
            let mut cases = 0_usize;
            for op in Op::ALL {
                let calls = reference.objects.calls(op);
                assert!(
                    calls > 0 || op == Op::List,
                    "{op:?}: a passing run makes no such call"
                );
                for n in 1..=calls {
                    cases += 1;
                    let marker = format!("injected-store-fault-{op:?}-{n}");
                    let message =
                        format!("{marker}: AccessDenied for {HOSTILE_ARN} in {HOSTILE_ACCOUNT}");
                    let rig = setup(&nominal).await;
                    rig.objects.fail_nth_with(op, n, &message);
                    let first = deliver(&rig, &nominal).await;
                    rig.set_execution(end_status(nominal.trigger));
                    let second = rig.sweep().await;
                    let released = rig.lock_released().await;
                    let record = rig.record().await;
                    let text = record
                        .result
                        .as_ref()
                        .and_then(result_text)
                        .map(str::to_string);
                    let case = format!("{op:?}#{n}");
                    if rig.objects.calls(op) < n {
                        failures.push(format!("{case}: the fault was never reached"));
                    }
                    if second.is_err() || !released {
                        failures.push(format!(
                            "{case}: not healed (first {}, second {}, released {released})",
                            clip_debug(&first.as_ref().err()),
                            clip_debug(&second.as_ref().err())
                        ));
                        continue;
                    }
                    if rig.objects.snapshot() != want_objects {
                        failures.push(format!("{case}: objects or tags differ"));
                    }
                    if record.task.status != want_record.task.status
                        || text.as_deref() != Some(want_text.as_str())
                    {
                        failures.push(format!("{case}: the record differs"));
                    }
                    let obs = Observed {
                        first,
                        record: Some(record),
                        payload_text: text,
                        payload: None,
                        verdict: rig.objects.bytes(&Rig::key(layout::VERDICT)),
                        manifest: rig.objects.bytes(&Rig::key(layout::MANIFEST)),
                        tags: BTreeMap::new(),
                        released,
                        second: second.map(|_| ReconcileReport::default()),
                    };
                    if let Some(leak) = hostile_leaks(&obs, &[marker.as_str()]) {
                        failures.push(format!("{case}: store error text reached a caller: {leak}"));
                    }
                }
            }
            println!("FAILURE SURFACE TRANSIENT: {cases} single-fault cases");
            assert!(cases >= 20, "the table is too small: {cases}");
            assert!(failures.is_empty(), "{failures:#?}");
        }
    }
}
