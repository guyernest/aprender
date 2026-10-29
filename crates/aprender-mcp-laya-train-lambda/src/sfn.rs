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
use aprender_mcp_laya_train::{Confirm, DispatchError, Dispatcher, RunSpec};
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

/// Classify a failed `StartExecution`. Anything not proven harmless is ambiguous.
#[must_use]
pub fn classify_start_error<R>(_err: &SdkError<StartExecutionError, R>) -> StartErrorKind {
    StartErrorKind::Ambiguous
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

/// The key a link may be minted for: `<run_prefix><path>`, only when it resolves
/// inside the run's own prefix. `None` otherwise.
#[must_use]
pub fn guard_link_key(run_prefix: &str, path: &str) -> Option<String> {
    (!path.is_empty() && path.split('/').all(|seg| !seg.is_empty() && seg != ".."))
        .then(|| format!("{run_prefix}{path}"))
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
