//! [`SfnDispatcher`]: the Laya training server's `Dispatcher` over Step Functions
//! and S3 presigning.
//!
//! One `train` is one Step Functions execution named `laya-<task_id>`. The name is
//! deterministic, so after an AMBIGUOUS start error (a timeout, a lost response)
//! [`SfnDispatcher::confirm`] can ask `DescribeExecution` whether it exists before
//! the library releases the single-flight lock.
//!
//! The AWS calls sit behind two small seams, [`Executions`] and [`Presign`], so
//! every rule here is proven offline against fakes.

use std::sync::Arc;

use aprender_mcp_laya_train::layout::{inputs_string, DOWNLOAD_URL_TTL_SECS};
use aprender_mcp_laya_train::{CancelSink, Confirm, DispatchError, Dispatcher, RunSpec};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_sfn::error::{DisplayErrorContext, SdkError};
use aws_sdk_sfn::operation::start_execution::StartExecutionError;

/// The prefix of every execution name: `laya-<task_id>`.
pub const EXECUTION_NAME_PREFIX: &str = "laya-";

/// Step Functions' bound on an execution name.
const MAX_EXECUTION_NAME_LEN: usize = 80;

/// What a failed `StartExecution` proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartErrorKind {
    /// Nothing started: safe to release the lock and compensate.
    Rejected,
    /// An execution may exist: ask [`Executions::describe`] before releasing.
    Ambiguous,
}

/// A failed start, classified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartError {
    /// What the failure proves.
    pub kind: StartErrorKind,
    /// The SDK's error text (never the execution input).
    pub message: String,
}

/// The three Step Functions calls the request function makes.
#[pmcp::async_trait]
pub trait Executions: Send + Sync {
    /// `StartExecution` on this server's state machine with a closed JSON input.
    ///
    /// # Errors
    ///
    /// A [`StartError`] whose kind says whether anything could have started.
    async fn start(&self, name: &str, input_json: &str) -> Result<(), StartError>;

    /// `DescribeExecution`: the execution's status, or `None` when it does not exist.
    ///
    /// # Errors
    ///
    /// The SDK's message for any failure other than "does not exist".
    async fn describe(&self, execution_arn: &str) -> Result<Option<String>, String>;

    /// `StopExecution`.
    ///
    /// # Errors
    ///
    /// The SDK's message.
    async fn stop(&self, execution_arn: &str) -> Result<(), String>;
}

/// Mint a time-limited GET link.
#[pmcp::async_trait]
pub trait Presign: Send + Sync {
    /// A presigned GET for `bucket`/`key` valid for `ttl_secs`, or `None`.
    async fn get_url(&self, bucket: &str, key: &str, ttl_secs: u64) -> Option<String>;
}

/// Classify a failed `StartExecution` (D-11, T-14-62).
///
/// [`StartErrorKind::Rejected`] ONLY for an answer that proves nothing started: a
/// modeled refusal Step Functions gives before an execution exists
/// (`ExecutionLimitExceeded`, `InvalidArn`, `InvalidExecutionInput`, `InvalidName`,
/// `StateMachineDoesNotExist`, `StateMachineDeleting`, `ValidationException`) or an
/// access denial. Everything else is [`StartErrorKind::Ambiguous`]: a timeout, a
/// dispatch or connection failure, a malformed response, throttling, a 5xx, an
/// unknown error code, and `ExecutionAlreadyExists` (an execution with this name
/// exists, so the run DID start). The library calls `confirm` on an ambiguous
/// error and releases the lock only on a confirmed non-start, so the safe default
/// costs one `DescribeExecution`, never a double run.
///
/// Generic over the raw-response type so the table test needs no HTTP types.
#[must_use]
pub fn classify_start_error<R>(err: &SdkError<StartExecutionError, R>) -> StartErrorKind {
    use aws_sdk_sfn::error::ProvideErrorMetadata;
    let SdkError::ServiceError(service) = err else {
        return StartErrorKind::Ambiguous;
    };
    match service.err() {
        StartExecutionError::ExecutionLimitExceeded(_)
        | StartExecutionError::InvalidArn(_)
        | StartExecutionError::InvalidExecutionInput(_)
        | StartExecutionError::InvalidName(_)
        | StartExecutionError::StateMachineDoesNotExist(_)
        | StartExecutionError::StateMachineDeleting(_)
        | StartExecutionError::ValidationException(_) => StartErrorKind::Rejected,
        StartExecutionError::ExecutionAlreadyExists(_) => StartErrorKind::Ambiguous,
        other => match other.code() {
            Some("AccessDenied" | "AccessDeniedException") => StartErrorKind::Rejected,
            _ => StartErrorKind::Ambiguous,
        },
    }
}

/// The aws-sdk-sfn [`Executions`], bound to one state machine.
#[derive(Debug, Clone)]
pub struct SfnExecutions {
    client: aws_sdk_sfn::Client,
    state_machine_arn: String,
}

impl SfnExecutions {
    /// Bind a client to a state machine.
    #[must_use]
    pub fn new(client: aws_sdk_sfn::Client, state_machine_arn: impl Into<String>) -> Self {
        Self {
            client,
            state_machine_arn: state_machine_arn.into(),
        }
    }
}

#[pmcp::async_trait]
impl Executions for SfnExecutions {
    async fn start(&self, name: &str, input_json: &str) -> Result<(), StartError> {
        self.client
            .start_execution()
            .state_machine_arn(&self.state_machine_arn)
            .name(name)
            .input(input_json)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| StartError {
                kind: classify_start_error(&e),
                message: DisplayErrorContext(&e).to_string(),
            })
    }

    async fn describe(&self, execution_arn: &str) -> Result<Option<String>, String> {
        match self
            .client
            .describe_execution()
            .execution_arn(execution_arn)
            .send()
            .await
        {
            Ok(out) => Ok(Some(out.status().as_str().to_string())),
            Err(e)
                if e.as_service_error()
                    .is_some_and(|se| se.is_execution_does_not_exist()) =>
            {
                Ok(None)
            }
            Err(e) => Err(DisplayErrorContext(&e).to_string()),
        }
    }

    async fn stop(&self, execution_arn: &str) -> Result<(), String> {
        self.client
            .stop_execution()
            .execution_arn(execution_arn)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| DisplayErrorContext(&e).to_string())
    }
}

/// The aws-sdk-s3 [`Presign`].
#[derive(Debug, Clone)]
pub struct S3Presign {
    client: aws_sdk_s3::Client,
}

impl S3Presign {
    /// Wrap a client.
    #[must_use]
    pub fn new(client: aws_sdk_s3::Client) -> Self {
        Self { client }
    }
}

#[pmcp::async_trait]
impl Presign for S3Presign {
    async fn get_url(&self, bucket: &str, key: &str, ttl_secs: u64) -> Option<String> {
        let cfg = PresigningConfig::expires_in(std::time::Duration::from_secs(ttl_secs)).ok()?;
        self.client
            .get_object()
            .bucket(bucket)
            .key(key)
            .presigned(cfg)
            .await
            .ok()
            .map(|signed| signed.uri().to_string())
    }
}

/// The execution ARN for `task_id` on `state_machine_arn`:
/// `arn:<p>:states:<region>:<acct>:stateMachine:<name>` becomes
/// `arn:<p>:states:<region>:<acct>:execution:<name>:laya-<task_id>`.
/// `None` for a malformed state machine ARN.
#[must_use]
pub fn execution_arn(state_machine_arn: &str, task_id: &str) -> Option<String> {
    let parts: Vec<&str> = state_machine_arn.split(':').collect();
    let [arn, partition, service, region, account, kind, name] = parts.as_slice() else {
        return None;
    };
    let well_formed = *arn == "arn"
        && *service == "states"
        && *kind == "stateMachine"
        && [partition, region, account, name]
            .iter()
            .all(|p| !p.is_empty());
    well_formed.then(|| {
        format!("arn:{partition}:states:{region}:{account}:execution:{name}:{EXECUTION_NAME_PREFIX}{task_id}")
    })
}

/// One path segment that cannot climb, address another key or smuggle an escape:
/// non-empty, not `.` or `..`, and free of `%` (an encoded dot), `\\`, `?`, `#`
/// and control characters.
fn link_segment_ok(seg: &str) -> bool {
    !seg.is_empty()
        && seg != "."
        && seg != ".."
        && !seg
            .chars()
            .any(|c| c.is_control() || matches!(c, '%' | '\\' | '?' | '#'))
}

/// The key a link may be minted for: `<run_prefix><path>`, only when it resolves
/// inside the run's own prefix (D-18, T-14-10). `run_prefix` must be the key form
/// `runs/<name>/<task_id>/`; `path` must be relative with no `..`, no leading `/`,
/// no empty segment and no `%`. The bucket is never a parameter, it is the
/// dispatcher's own. `None` otherwise.
#[must_use]
pub fn guard_link_key(run_prefix: &str, path: &str) -> Option<String> {
    // Exactly `runs/<name>/<task_id>/`: three safe segments and the trailing slash.
    let prefix_ok = run_prefix.strip_suffix('/').is_some_and(|bare| {
        let segments: Vec<&str> = bare.split('/').collect();
        segments.len() == 3 && segments[0] == "runs" && segments.into_iter().all(link_segment_ok)
    });
    let path_ok = path.split('/').all(link_segment_ok);
    (prefix_ok && path_ok).then(|| format!("{run_prefix}{path}"))
}

/// Dispatches a run as one Step Functions execution and signs links for its files.
pub struct SfnDispatcher {
    executions: Arc<dyn Executions>,
    presign: Arc<dyn Presign>,
    bucket: String,
    state_machine_arn: String,
}

impl std::fmt::Debug for SfnDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SfnDispatcher")
            .field("bucket", &self.bucket)
            .field("state_machine_arn", &self.state_machine_arn)
            .finish_non_exhaustive()
    }
}

impl SfnDispatcher {
    /// A dispatcher over the two seams.
    #[must_use]
    pub fn new(
        executions: Arc<dyn Executions>,
        presign: Arc<dyn Presign>,
        bucket: impl Into<String>,
        state_machine_arn: impl Into<String>,
    ) -> Self {
        Self {
            executions,
            presign,
            bucket: bucket.into(),
            state_machine_arn: state_machine_arn.into(),
        }
    }
}

/// `laya-<task_id>`, or `None` when it would not be a valid execution name.
fn execution_name(task_id: &str) -> Option<String> {
    let name = format!("{EXECUTION_NAME_PREFIX}{task_id}");
    let ok = name.len() <= MAX_EXECUTION_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then_some(name)
}

#[pmcp::async_trait]
impl Dispatcher for SfnDispatcher {
    fn bucket(&self) -> &str {
        &self.bucket
    }

    async fn dispatch(&self, spec: &RunSpec) -> Result<(), DispatchError> {
        let name = execution_name(&spec.task_id).ok_or_else(|| {
            DispatchError::Rejected("the task id is not a valid execution name".to_string())
        })?;
        debug_assert_eq!(
            inputs_string(&spec.inputs_sha256),
            spec.execution_input().inputs_sha256
        );
        let input = serde_json::to_string(&spec.execution_input()).map_err(|e| {
            DispatchError::Rejected(format!("cannot encode the execution input: {e}"))
        })?;
        self.executions
            .start(&name, &input)
            .await
            .map_err(|e| match e.kind {
                StartErrorKind::Rejected => DispatchError::Rejected(e.message),
                StartErrorKind::Ambiguous => DispatchError::Ambiguous(e.message),
            })
    }

    async fn confirm(&self, task_id: &str) -> Result<Confirm, String> {
        let arn = execution_arn(&self.state_machine_arn, task_id)
            .ok_or_else(|| "the state machine ARN is malformed".to_string())?;
        match self.executions.describe(&arn).await? {
            Some(_status) => Ok(Confirm::Started),
            None => Ok(Confirm::NotStarted),
        }
    }

    async fn link(&self, run_prefix: &str, path: &str) -> Option<String> {
        let key = guard_link_key(run_prefix, path)?;
        self.presign
            .get_url(&self.bucket, &key, DOWNLOAD_URL_TTL_SECS)
            .await
    }
}

// JOIN BOUND: the cancel sink waits at most this long for StopExecution. The sink is
// called from inside `tasks/cancel`, which is served behind API Gateway's 30 s request
// cap; 10 s leaves more than 15 s for the rest of that request (the store's durable
// Cancelled write, the response). The two numbers move together: raise this and the
// cap is no longer safe, lower the cap and this is wasted.
pub const CANCEL_JOIN_BOUND: std::time::Duration = std::time::Duration::from_secs(10);

/// Stops the Step Functions execution behind a cancelled task. BEST EFFORT, by design.
///
/// The durable cancel intent is the `Cancelled` record the store writes BEFORE it
/// fires this sink; the single-flight lock stays held until compute is terminal;
/// and the finalizer's reconcile sweep (plan 14-05) re-issues StopExecution,
/// StopTrainingJob and the ECS StopTask every sweep until it is. So a lost,
/// timed-out or failed StopExecution here delays the stop, it never forgets it.
///
/// [`CancelSink::cancel`] is synchronous, and may be called on an async runtime's
/// worker thread, so the call runs on its OWN thread with its OWN current-thread
/// runtime (`new_current_thread().enable_all()`: the SDK's timeouts need the time
/// driver and its sockets need IO) and is waited on with a bounded join. That keeps the SDK client off the
/// caller's runtime handle and makes the wait finite. It never panics the request:
/// every outcome is one log line, without credentials.
pub struct SfnCancelSink {
    executions: Arc<dyn Executions>,
    state_machine_arn: String,
    join_bound: std::time::Duration,
}

impl std::fmt::Debug for SfnCancelSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SfnCancelSink")
            .field("state_machine_arn", &self.state_machine_arn)
            .field("join_bound", &self.join_bound)
            .finish_non_exhaustive()
    }
}

impl SfnCancelSink {
    /// A sink with the production [`CANCEL_JOIN_BOUND`].
    #[must_use]
    pub fn new(executions: Arc<dyn Executions>, state_machine_arn: impl Into<String>) -> Self {
        Self {
            executions,
            state_machine_arn: state_machine_arn.into(),
            join_bound: CANCEL_JOIN_BOUND,
        }
    }

    /// Replace the join bound (tests only need a different one; production keeps 10 s).
    #[must_use]
    pub fn with_join_bound(mut self, bound: std::time::Duration) -> Self {
        self.join_bound = bound;
        self
    }
}

/// Run `wait` where blocking is allowed: handing the worker's other tasks to a
/// sibling thread on a multi-thread runtime, plainly on anything else.
fn blocking<T>(wait: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(wait)
        }
        _ => wait(),
    }
}

impl CancelSink for SfnCancelSink {
    fn cancel(&self, task_id: &str) {
        let Some(arn) = execution_arn(&self.state_machine_arn, task_id) else {
            tracing::error!(
                "laya-train cancel: StopExecution not sent for task {task_id}: the state machine ARN is malformed"
            );
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let executions = Arc::clone(&self.executions);
        let spawned = std::thread::Builder::new()
            .name("laya-train-cancel".to_string())
            .spawn(move || {
                let outcome = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(executions.stop(&arn)),
                    Err(e) => Err(format!("cannot build a runtime: {e}")),
                };
                let _ = tx.send(outcome);
            });
        let handle = match spawned {
            Ok(handle) => handle,
            Err(e) => {
                tracing::error!(
                    "laya-train cancel: StopExecution not sent for task {task_id}: cannot spawn a thread: {e}"
                );
                return;
            }
        };
        let bound = self.join_bound;
        match blocking(|| rx.recv_timeout(bound)) {
            Ok(Ok(())) => {
                let _ = handle.join();
                tracing::info!("laya-train cancel: StopExecution sent for task {task_id}");
            }
            Ok(Err(message)) => {
                let _ = handle.join();
                tracing::error!(
                    "laya-train cancel: StopExecution failed for task {task_id}: {message}"
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // The thread is left detached; the reconcile sweep re-issues the stop.
                tracing::error!(
                    "laya-train cancel: StopExecution timed out for task {task_id} after {}s",
                    bound.as_secs()
                );
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                let _ = handle.join();
                tracing::error!(
                    "laya-train cancel: StopExecution thread ended without an answer for task {task_id}"
                );
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod fakes {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::{Executions, Presign, StartError};

    /// Records starts; answers describe/stop from scripts.
    #[derive(Default)]
    pub struct FakeExecutions {
        pub starts: Mutex<Vec<(String, String)>>,
        pub start_script: Mutex<VecDeque<Result<(), StartError>>>,
        pub describe_script: Mutex<VecDeque<Result<Option<String>, String>>>,
        pub stops: Mutex<Vec<String>>,
    }

    #[pmcp::async_trait]
    impl Executions for FakeExecutions {
        async fn start(&self, name: &str, input_json: &str) -> Result<(), StartError> {
            self.starts
                .lock()
                .expect("lock")
                .push((name.to_string(), input_json.to_string()));
            self.start_script
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or(Ok(()))
        }

        async fn describe(&self, _execution_arn: &str) -> Result<Option<String>, String> {
            self.describe_script
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or(Err("no scripted answer".to_string()))
        }

        async fn stop(&self, execution_arn: &str) -> Result<(), String> {
            self.stops
                .lock()
                .expect("lock")
                .push(execution_arn.to_string());
            Ok(())
        }
    }

    /// `https://presigned.example/<bucket>/<key>?ttl=<secs>`.
    pub struct FakePresign;

    #[pmcp::async_trait]
    impl Presign for FakePresign {
        async fn get_url(&self, bucket: &str, key: &str, ttl_secs: u64) -> Option<String> {
            Some(format!(
                "https://presigned.example/{bucket}/{key}?ttl={ttl_secs}"
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use aprender_mcp_laya_train::layout::{ExecutionInput, MAX_REQUEST_BYTES};
    use aws_sdk_sfn::error::{ErrorMetadata, SdkError};
    use aws_sdk_sfn::types::error as sfn_err;

    use super::fakes::{FakeExecutions, FakePresign};
    use super::*;

    const MACHINE: &str = "arn:aws:states:us-east-1:123456789012:stateMachine:laya-train-dev";
    const TASK: &str = "0f8b3c1e-4d2a-4b6e-9c71-2a5d8e9f1b34";

    fn hex(c: char) -> String {
        std::iter::repeat_n(c, 64).collect()
    }

    fn spec(epochs: Option<u32>, with_shift: bool) -> RunSpec {
        let mut inputs: BTreeMap<String, String> = BTreeMap::new();
        inputs.insert("task.json".to_string(), hex('a'));
        inputs.insert("train.jsonl".to_string(), hex('b'));
        inputs.insert("eval.jsonl".to_string(), hex('c'));
        if with_shift {
            inputs.insert("shift.jsonl".to_string(), hex('d'));
        }
        RunSpec {
            task_id: TASK.to_string(),
            owner: "alice".to_string(),
            name: "model-one".to_string(),
            dataset_prefix: "datasets/d/".to_string(),
            run_prefix: format!("runs/model-one/{TASK}/"),
            epochs,
            inputs_sha256: inputs,
            gate_contract_sha256: hex('e'),
        }
    }

    fn dispatcher(executions: &Arc<FakeExecutions>) -> SfnDispatcher {
        SfnDispatcher::new(
            Arc::clone(executions) as Arc<dyn Executions>,
            Arc::new(FakePresign),
            "fixture-bucket",
            MACHINE,
        )
    }

    // ---- execution_input ----

    #[tokio::test]
    async fn execution_input_is_closed_and_carries_handles_only() {
        for (epochs, want_epochs, with_shift, entries) in
            [(None, "", false, 3), (Some(12), "12", true, 4)]
        {
            let ex = Arc::new(FakeExecutions::default());
            dispatcher(&ex)
                .dispatch(&spec(epochs, with_shift))
                .await
                .expect("started");
            let starts = ex.starts.lock().expect("lock").clone();
            assert_eq!(starts.len(), 1);
            let (name, input) = &starts[0];
            assert_eq!(name, &format!("laya-{TASK}"));
            assert!(name.len() <= 80);
            let raw: serde_json::Value = serde_json::from_str(input).expect("json");
            let mut keys: Vec<&str> = raw
                .as_object()
                .expect("object")
                .keys()
                .map(String::as_str)
                .collect();
            keys.sort_unstable();
            assert_eq!(
                keys,
                [
                    "dataset_prefix",
                    "epochs",
                    "gate_contract_sha256",
                    "inputs_sha256",
                    "name",
                    "owner",
                    "run_prefix",
                    "schema",
                    "task_id"
                ],
                "exactly the nine keys, no content, token or URL"
            );
            let parsed: ExecutionInput = serde_json::from_str(input).expect("closed input");
            assert_eq!(parsed.epochs, want_epochs);
            assert_eq!(parsed.inputs_sha256.split(';').count(), entries);
            assert_eq!(parsed.gate_contract_sha256.len(), 64);
            assert_eq!(parsed.schema, "laya-train-execution-input-v1");
            assert!(!input.contains("http"), "no URL travels in the input");
        }
    }

    #[tokio::test]
    async fn execution_input_refuses_a_task_id_that_is_not_an_execution_name() {
        let ex = Arc::new(FakeExecutions::default());
        let mut bad = spec(None, false);
        bad.task_id = "x".repeat(80);
        let err = dispatcher(&ex).dispatch(&bad).await.expect_err("too long");
        assert!(matches!(err, DispatchError::Rejected(_)), "{err:?}");
        bad.task_id = "has space".to_string();
        assert!(matches!(
            dispatcher(&ex).dispatch(&bad).await,
            Err(DispatchError::Rejected(_))
        ));
        assert!(
            ex.starts.lock().expect("lock").is_empty(),
            "nothing started"
        );
    }

    // ---- execution_dispatch_classify ----

    fn service(e: StartExecutionError) -> SdkError<StartExecutionError, ()> {
        SdkError::service_error(e, ())
    }

    fn coded(code: &str) -> SdkError<StartExecutionError, ()> {
        service(StartExecutionError::generic(
            ErrorMetadata::builder().code(code).message("m").build(),
        ))
    }

    #[test]
    fn execution_dispatch_classify_table() {
        use StartErrorKind::{Ambiguous, Rejected};
        macro_rules! modeled {
            ($variant:ident, $ty:ident) => {
                service(StartExecutionError::$variant(
                    sfn_err::$ty::builder().build(),
                ))
            };
        }
        let rejected: Vec<(&str, SdkError<StartExecutionError, ()>)> = vec![
            (
                "ExecutionLimitExceeded",
                modeled!(ExecutionLimitExceeded, ExecutionLimitExceeded),
            ),
            ("InvalidArn", modeled!(InvalidArn, InvalidArn)),
            (
                "InvalidExecutionInput",
                modeled!(InvalidExecutionInput, InvalidExecutionInput),
            ),
            ("InvalidName", modeled!(InvalidName, InvalidName)),
            (
                "StateMachineDoesNotExist",
                modeled!(StateMachineDoesNotExist, StateMachineDoesNotExist),
            ),
            (
                "StateMachineDeleting",
                modeled!(StateMachineDeleting, StateMachineDeleting),
            ),
            (
                "ValidationException",
                modeled!(ValidationException, ValidationException),
            ),
            ("AccessDeniedException", coded("AccessDeniedException")),
            ("AccessDenied", coded("AccessDenied")),
        ];
        for (name, err) in &rejected {
            assert_eq!(
                classify_start_error(err),
                Rejected,
                "{name} proves nothing started"
            );
        }
        let ambiguous: Vec<(&str, SdkError<StartExecutionError, ()>)> = vec![
            (
                "ExecutionAlreadyExists",
                modeled!(ExecutionAlreadyExists, ExecutionAlreadyExists),
            ),
            ("timeout", SdkError::timeout_error("timed out")),
            (
                "dispatch failure",
                SdkError::dispatch_failure(aws_sdk_sfn::error::ConnectorError::io(
                    "connection refused".into(),
                )),
            ),
            ("response error", SdkError::response_error("garbled", ())),
            ("construction failure", SdkError::construction_failure("no")),
            ("ThrottlingException", coded("ThrottlingException")),
            ("InternalServerError (5xx)", coded("InternalServerError")),
            ("ServiceUnavailable (5xx)", coded("ServiceUnavailable")),
            ("an unknown code", coded("SomethingNew")),
        ];
        for (name, err) in &ambiguous {
            assert_eq!(
                classify_start_error(err),
                Ambiguous,
                "{name} may have started"
            );
        }
    }

    #[tokio::test]
    async fn execution_dispatch_maps_kinds_to_dispatch_errors() {
        let ex = Arc::new(FakeExecutions::default());
        {
            let mut script = ex.start_script.lock().expect("lock");
            script.push_back(Err(StartError {
                kind: StartErrorKind::Rejected,
                message: "bad".to_string(),
            }));
            script.push_back(Err(StartError {
                kind: StartErrorKind::Ambiguous,
                message: "maybe".to_string(),
            }));
        }
        let d = dispatcher(&ex);
        assert_eq!(
            d.dispatch(&spec(None, false)).await,
            Err(DispatchError::Rejected("bad".to_string()))
        );
        assert_eq!(
            d.dispatch(&spec(None, false)).await,
            Err(DispatchError::Ambiguous("maybe".to_string()))
        );
    }

    // ---- execution_confirm / execution_arn ----

    #[tokio::test]
    async fn execution_confirm_answers_from_describe() {
        let ex = Arc::new(FakeExecutions::default());
        {
            let mut script = ex.describe_script.lock().expect("lock");
            script.push_back(Ok(Some("RUNNING".to_string())));
            script.push_back(Ok(None));
            script.push_back(Err("throttled".to_string()));
        }
        let d = dispatcher(&ex);
        assert_eq!(d.confirm(TASK).await, Ok(Confirm::Started));
        assert_eq!(d.confirm(TASK).await, Ok(Confirm::NotStarted));
        assert_eq!(d.confirm(TASK).await, Err("throttled".to_string()));
    }

    #[test]
    fn execution_arn_is_derived_from_the_state_machine_arn() {
        assert_eq!(
            execution_arn(MACHINE, "t").as_deref(),
            Some("arn:aws:states:us-east-1:123456789012:execution:laya-train-dev:laya-t")
        );
        for bad in [
            "",
            "laya-train-dev",
            "arn:aws:states:us-east-1:123456789012:execution:laya-train-dev:x",
            "arn:aws:lambda:us-east-1:123456789012:stateMachine:laya-train-dev",
            "arn:aws:states:us-east-1:123456789012:stateMachine:",
            "arn:aws:states::123456789012:stateMachine:laya-train-dev",
            "arn:aws:states:us-east-1:123456789012:stateMachine:laya-train-dev:extra",
        ] {
            assert_eq!(execution_arn(bad, "t"), None, "{bad:?}");
        }
    }

    // ---- link_guard ----

    #[test]
    fn link_guard_confines_keys_to_the_run_prefix() {
        let prefix = "runs/model-one/t-1/";
        assert_eq!(
            guard_link_key(prefix, "run/gate-report.json").as_deref(),
            Some("runs/model-one/t-1/run/gate-report.json")
        );
        assert_eq!(
            guard_link_key(prefix, "motivation-v1.apr").as_deref(),
            Some("runs/model-one/t-1/motivation-v1.apr")
        );
        for bad in [
            "../x",
            "/x",
            "run/../../y",
            "a//b",
            "",
            "a/%2e%2e/b",
            "a/%2E/b",
            "./x",
            "a/",
            "a\\b",
            "a?b",
            "a#b",
            "a\nb",
        ] {
            assert_eq!(guard_link_key(prefix, bad), None, "{bad:?}");
        }
        for bad_prefix in [
            "",
            "runs/",
            "datasets/x/",
            "runs/a/../b/",
            "/runs/a/b/",
            "runs/a/b",
            "runs//b/",
        ] {
            assert_eq!(guard_link_key(bad_prefix, "x.json"), None, "{bad_prefix:?}");
        }
    }

    #[tokio::test]
    async fn link_guard_mints_with_the_read_time_ttl_in_the_dispatchers_bucket() {
        let ex = Arc::new(FakeExecutions::default());
        let url = dispatcher(&ex)
            .link("runs/model-one/t-1/", "run/gate-report.json")
            .await
            .expect("inside the prefix");
        assert_eq!(
            url,
            "https://presigned.example/fixture-bucket/runs/model-one/t-1/run/gate-report.json?ttl=3600"
        );
        assert_eq!(
            dispatcher(&ex).link("runs/model-one/t-1/", "../x").await,
            None
        );
    }

    // ---- cancel_sink ----

    /// A stopper that never answers within a test's patience.
    struct SleepyStopper;

    #[pmcp::async_trait]
    impl Executions for SleepyStopper {
        async fn start(&self, _: &str, _: &str) -> Result<(), StartError> {
            Ok(())
        }
        async fn describe(&self, _: &str) -> Result<Option<String>, String> {
            Ok(None)
        }
        async fn stop(&self, _: &str) -> Result<(), String> {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(())
        }
    }

    struct FailingStopper;

    #[pmcp::async_trait]
    impl Executions for FailingStopper {
        async fn start(&self, _: &str, _: &str) -> Result<(), StartError> {
            Ok(())
        }
        async fn describe(&self, _: &str) -> Result<Option<String>, String> {
            Ok(None)
        }
        async fn stop(&self, _: &str) -> Result<(), String> {
            Err("AccessDenied: not allowed".to_string())
        }
    }

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("lock").extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Run `f` with a thread-local subscriber and return what it logged.
    fn logged(f: impl FnOnce()) -> String {
        let sink = Captured::default();
        let writer = sink.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .with_ansi(false)
            .finish();
        tracing::subscriber::with_default(subscriber, f);
        let bytes = sink.0.lock().expect("lock").clone();
        String::from_utf8(bytes).expect("utf8")
    }

    #[test]
    fn cancel_sink_stops_the_derived_execution() {
        let ex = Arc::new(FakeExecutions::default());
        let sink = SfnCancelSink::new(Arc::clone(&ex) as Arc<dyn Executions>, MACHINE);
        let log = logged(|| sink.cancel("t"));
        assert_eq!(
            ex.stops.lock().expect("lock").clone(),
            ["arn:aws:states:us-east-1:123456789012:execution:laya-train-dev:laya-t"]
        );
        assert!(log.contains("StopExecution sent"), "{log}");
    }

    #[test]
    fn cancel_sink_returns_within_the_join_bound_when_the_stop_hangs() {
        let sink = SfnCancelSink::new(Arc::new(SleepyStopper), MACHINE);
        let started = Instant::now();
        let log = logged(|| sink.cancel("t"));
        let took = started.elapsed();
        assert!(took < Duration::from_secs(11), "cancel took {took:?}");
        assert!(
            took >= Duration::from_secs(9),
            "the bound is the wait: {took:?}"
        );
        assert!(log.contains("StopExecution timed out"), "{log}");
    }

    #[test]
    fn cancel_sink_logs_a_failing_stop_and_returns() {
        let sink = SfnCancelSink::new(Arc::new(FailingStopper), MACHINE);
        let log = logged(|| sink.cancel("t"));
        assert!(log.contains("StopExecution failed"), "{log}");
        assert!(log.contains("AccessDenied"), "{log}");
    }

    #[test]
    fn cancel_sink_with_a_malformed_arn_logs_and_never_panics() {
        let sink = SfnCancelSink::new(Arc::new(FailingStopper), "not-an-arn");
        let log = logged(|| sink.cancel("t"));
        assert!(log.contains("StopExecution not sent"), "{log}");
    }

    #[tokio::test]
    async fn cancel_sink_works_from_a_current_thread_runtime() {
        let ex = Arc::new(FakeExecutions::default());
        let sink = SfnCancelSink::new(Arc::clone(&ex) as Arc<dyn Executions>, MACHINE);
        sink.cancel("t");
        assert_eq!(ex.stops.lock().expect("lock").len(), 1);
    }

    /// The real SDK client shape, not a fake: static credentials, a closed loopback
    /// port, a 2 s operation timeout, called from inside a multi-thread runtime.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_sink_real_client() {
        use aws_sdk_sfn::config::timeout::TimeoutConfig;
        use aws_sdk_sfn::config::{retry::RetryConfig, BehaviorVersion, Credentials, Region};

        let config = aws_sdk_sfn::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .endpoint_url("http://127.0.0.1:9")
            .credentials_provider(Credentials::new(
                "AKIATESTTESTTESTTEST",
                "secret",
                None,
                None,
                "test",
            ))
            .retry_config(RetryConfig::disabled())
            .timeout_config(
                TimeoutConfig::builder()
                    .operation_timeout(Duration::from_secs(2))
                    .build(),
            )
            .build();
        let executions = SfnExecutions::new(aws_sdk_sfn::Client::from_conf(config), MACHINE);
        let sink = SfnCancelSink::new(Arc::new(executions), MACHINE);

        let started = Instant::now();
        let log = logged(|| sink.cancel("t"));
        let took = started.elapsed();
        assert!(took < Duration::from_secs(11), "cancel took {took:?}");
        let errors: Vec<&str> = log
            .lines()
            .filter(|l| l.contains("ERROR") && l.contains("StopExecution"))
            .collect();
        assert_eq!(
            errors.len(),
            1,
            "one error line naming StopExecution: {log}"
        );
        assert!(!log.contains("secret"), "no credential in the log: {log}");
        assert!(MAX_REQUEST_BYTES > 0);
    }
}
