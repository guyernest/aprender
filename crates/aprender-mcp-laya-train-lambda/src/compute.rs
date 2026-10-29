//! The finalizer's view of the compute a run may still own: its Step Functions
//! execution, its SageMaker training job and its ECS pack task.
//!
//! Every call sits behind a small seam ([`ExecutionReader`], [`TrainingJobs`],
//! [`PackTasks`]) so each rule in the finalizer is proven offline against fakes.
//!
//! # Why the finalizer asks (D-11, D-12)
//!
//! The single-flight lock may be released only when nothing of the run is still
//! billing. AWS documents `.sync` cancellation of the underlying job as best effort
//! and Step Functions' status events as best effort, so the finalizer does not take
//! either's word for it: [`compute_terminal`] reads the job and the pack task
//! directly, and the finalizer stops what is still live and keeps the lock.

use aws_sdk_sfn::error::DisplayErrorContext;

use crate::finalizer::FinalizerCtx;

/// The prefix of the training job name: `laya-<task_id>` (the same string as the
/// execution name, which is how the state machine names the job).
pub const TRAINING_JOB_PREFIX: &str = "laya-";

/// The container-override variable that names the task a pack task belongs to
/// (set by the state machine's `EcsRunTask`, plan 14-08).
pub const PACK_TASK_ENV: &str = "LAYA_TASK_ID";

/// The training job's name for `task_id`.
#[must_use]
pub fn training_job_name(task_id: &str) -> String {
    format!("{TRAINING_JOB_PREFIX}{task_id}")
}

/// What `DescribeExecution` answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionInfo {
    /// `RUNNING`, `SUCCEEDED`, `FAILED`, `TIMED_OUT` or `ABORTED`.
    pub status: String,
    /// The execution input, as the JSON string it was started with.
    pub input: String,
    /// When the execution started, in epoch seconds.
    pub start_secs: u64,
}

impl ExecutionInfo {
    /// True for every status except `RUNNING` (and `PENDING_REDRIVE`).
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        !matches!(self.status.as_str(), "RUNNING" | "PENDING_REDRIVE")
    }
}

/// The Step Functions calls the finalizer makes.
#[pmcp::async_trait]
pub trait ExecutionReader: Send + Sync {
    /// `DescribeExecution`, or `None` when the execution does not exist.
    ///
    /// # Errors
    ///
    /// The SDK's message for any failure other than "does not exist".
    async fn describe(&self, execution_arn: &str) -> Result<Option<ExecutionInfo>, String>;

    /// `StopExecution`.
    ///
    /// # Errors
    ///
    /// The SDK's message.
    async fn stop(&self, execution_arn: &str) -> Result<(), String>;

    /// The ARNs of the state machine's `RUNNING` executions, across all pages.
    ///
    /// # Errors
    ///
    /// The SDK's message.
    async fn list_running(&self, state_machine_arn: &str) -> Result<Vec<String>, String>;
}

/// The SageMaker calls the finalizer makes.
#[pmcp::async_trait]
pub trait TrainingJobs: Send + Sync {
    /// The training job's status (`InProgress`, `Completed`, `Failed`, `Stopping`,
    /// `Stopped`), or `None` when no such job exists.
    ///
    /// # Errors
    ///
    /// The SDK's message for any failure other than "not found".
    async fn status(&self, job_name: &str) -> Result<Option<String>, String>;

    /// `StopTrainingJob`.
    ///
    /// # Errors
    ///
    /// The SDK's message.
    async fn stop(&self, job_name: &str) -> Result<(), String>;
}

/// One live ECS pack task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackTask {
    /// The task ARN.
    pub task_arn: String,
    /// ECS's `lastStatus` (never `STOPPED` for a task returned by [`PackTasks::live_for`]).
    pub last_status: String,
}

/// The ECS calls the finalizer makes.
#[pmcp::async_trait]
pub trait PackTasks: Send + Sync {
    /// The pack-family tasks whose `LAYA_TASK_ID` container override equals
    /// `task_id` and whose `lastStatus` is not `STOPPED`.
    ///
    /// # Errors
    ///
    /// The SDK's message.
    async fn live_for(&self, task_id: &str) -> Result<Vec<PackTask>, String>;

    /// `StopTask`.
    ///
    /// # Errors
    ///
    /// The SDK's message.
    async fn stop(&self, task_arn: &str) -> Result<(), String>;
}

// ------------------------------------------------------------- AWS: Step Functions

/// The aws-sdk-sfn [`ExecutionReader`].
#[derive(Debug, Clone)]
pub struct SfnExecutionReader {
    client: aws_sdk_sfn::Client,
}

impl SfnExecutionReader {
    /// Wrap a client.
    #[must_use]
    pub fn new(client: aws_sdk_sfn::Client) -> Self {
        Self { client }
    }
}

#[pmcp::async_trait]
impl ExecutionReader for SfnExecutionReader {
    async fn describe(&self, execution_arn: &str) -> Result<Option<ExecutionInfo>, String> {
        match self
            .client
            .describe_execution()
            .execution_arn(execution_arn)
            .send()
            .await
        {
            Ok(out) => Ok(Some(ExecutionInfo {
                status: out.status().as_str().to_string(),
                input: out.input().unwrap_or_default().to_string(),
                start_secs: u64::try_from(out.start_date().secs()).unwrap_or(0),
            })),
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

    async fn list_running(&self, state_machine_arn: &str) -> Result<Vec<String>, String> {
        let mut arns = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut request = self
                .client
                .list_executions()
                .state_machine_arn(state_machine_arn)
                .status_filter(aws_sdk_sfn::types::ExecutionStatus::Running);
            if let Some(t) = token.take() {
                request = request.next_token(t);
            }
            let page = request
                .send()
                .await
                .map_err(|e| DisplayErrorContext(&e).to_string())?;
            arns.extend(
                page.executions()
                    .iter()
                    .map(|e| e.execution_arn().to_string()),
            );
            match page.next_token() {
                Some(next) => token = Some(next.to_string()),
                None => break,
            }
        }
        Ok(arns)
    }
}

// ----------------------------------------------------------------- AWS: SageMaker

/// The aws-sdk-sagemaker [`TrainingJobs`].
#[derive(Debug, Clone)]
pub struct SageMakerJobs {
    client: aws_sdk_sagemaker::Client,
}

impl SageMakerJobs {
    /// Wrap a client.
    #[must_use]
    pub fn new(client: aws_sdk_sagemaker::Client) -> Self {
        Self { client }
    }
}

/// SageMaker answers a missing job with a `ValidationException` whose message is
/// "Requested resource not found" (the operation models no not-found error).
fn is_job_not_found<E, R>(err: &aws_sdk_sagemaker::error::SdkError<E, R>) -> bool
where
    E: aws_sdk_sagemaker::error::ProvideErrorMetadata,
{
    use aws_sdk_sagemaker::error::ProvideErrorMetadata;
    let by_code = matches!(err.code(), Some("ResourceNotFound" | "ValidationException"));
    by_code
        && err
            .message()
            .is_some_and(|m| m.to_ascii_lowercase().contains("not found"))
}

#[pmcp::async_trait]
impl TrainingJobs for SageMakerJobs {
    async fn status(&self, job_name: &str) -> Result<Option<String>, String> {
        match self
            .client
            .describe_training_job()
            .training_job_name(job_name)
            .send()
            .await
        {
            Ok(out) => Ok(out
                .training_job_status()
                .map(|s| s.as_str().to_string())
                .or_else(|| Some("Unknown".to_string()))),
            Err(e) if is_job_not_found(&e) => Ok(None),
            Err(e) => Err(aws_sdk_sagemaker::error::DisplayErrorContext(&e).to_string()),
        }
    }

    async fn stop(&self, job_name: &str) -> Result<(), String> {
        self.client
            .stop_training_job()
            .training_job_name(job_name)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| aws_sdk_sagemaker::error::DisplayErrorContext(&e).to_string())
    }
}

// ---------------------------------------------------------------------- AWS: ECS

/// The aws-sdk-ecs [`PackTasks`], bound to one cluster and one task-definition family.
#[derive(Debug, Clone)]
pub struct EcsPackTasks {
    client: aws_sdk_ecs::Client,
    cluster: String,
    family: String,
}

impl EcsPackTasks {
    /// Bind a client to a cluster and a family.
    #[must_use]
    pub fn new(
        client: aws_sdk_ecs::Client,
        cluster: impl Into<String>,
        family: impl Into<String>,
    ) -> Self {
        Self {
            client,
            cluster: cluster.into(),
            family: family.into(),
        }
    }

    /// `ListTasks` for one desired status, across all pages.
    async fn list(
        &self,
        desired: aws_sdk_ecs::types::DesiredStatus,
    ) -> Result<Vec<String>, String> {
        let mut arns = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut request = self
                .client
                .list_tasks()
                .cluster(&self.cluster)
                .family(&self.family)
                .desired_status(desired.clone());
            if let Some(t) = token.take() {
                request = request.next_token(t);
            }
            let page = request
                .send()
                .await
                .map_err(|e| aws_sdk_ecs::error::DisplayErrorContext(&e).to_string())?;
            arns.extend(page.task_arns().iter().cloned());
            match page.next_token() {
                Some(next) => token = Some(next.to_string()),
                None => break,
            }
        }
        Ok(arns)
    }
}

#[pmcp::async_trait]
impl PackTasks for EcsPackTasks {
    async fn live_for(&self, task_id: &str) -> Result<Vec<PackTask>, String> {
        // A task being stopped has desiredStatus STOPPED while its lastStatus is not
        // yet STOPPED, so both listings are needed.
        let mut arns = self
            .list(aws_sdk_ecs::types::DesiredStatus::Running)
            .await?;
        for arn in self
            .list(aws_sdk_ecs::types::DesiredStatus::Stopped)
            .await?
        {
            if !arns.contains(&arn) {
                arns.push(arn);
            }
        }
        let mut live = Vec::new();
        // DescribeTasks takes at most 100 ARNs.
        for chunk in arns.chunks(100) {
            let described = self
                .client
                .describe_tasks()
                .cluster(&self.cluster)
                .set_tasks(Some(chunk.to_vec()))
                .send()
                .await
                .map_err(|e| aws_sdk_ecs::error::DisplayErrorContext(&e).to_string())?;
            for task in described.tasks() {
                let last_status = task.last_status().unwrap_or("UNKNOWN");
                let ours = task.overrides().is_some_and(|o| {
                    o.container_overrides().iter().any(|c| {
                        c.environment().iter().any(|kv| {
                            kv.name() == Some(PACK_TASK_ENV) && kv.value() == Some(task_id)
                        })
                    })
                });
                if ours && last_status != "STOPPED" {
                    if let Some(arn) = task.task_arn() {
                        live.push(PackTask {
                            task_arn: arn.to_string(),
                            last_status: last_status.to_string(),
                        });
                    }
                }
            }
        }
        Ok(live)
    }

    async fn stop(&self, task_arn: &str) -> Result<(), String> {
        self.client
            .stop_task()
            .cluster(&self.cluster)
            .task(task_arn)
            .reason("laya-train finalizer: the run is over or cancelled")
            .send()
            .await
            .map(|_| ())
            .map_err(|e| aws_sdk_ecs::error::DisplayErrorContext(&e).to_string())
    }
}

// ------------------------------------------------------------------- the gate

/// What the compute gate found for one task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeState {
    /// True only when the training job is absent or `Completed | Failed | Stopped`
    /// AND no pack task of the run is in a status other than `STOPPED`.
    pub terminal: bool,
    /// The training job's status, `None` when absent.
    pub training: Option<String>,
    /// How many live pack tasks the run owns.
    pub pack_tasks: usize,
    /// Their ARNs.
    pub pack_task_arns: Vec<String>,
}

/// Whether `task_id`'s compute is finished. Read-only.
///
/// # Errors
///
/// The seam's message when either read fails: with no answer the gate cannot
/// call compute terminal, so the caller must not release anything.
pub async fn compute_terminal(ctx: &FinalizerCtx, task_id: &str) -> Result<ComputeState, String> {
    let training = ctx
        .training_jobs
        .status(&training_job_name(task_id))
        .await
        .map_err(|e| format!("DescribeTrainingJob: {e}"))?;
    let packs = ctx
        .pack_tasks
        .live_for(task_id)
        .await
        .map_err(|e| format!("ECS ListTasks/DescribeTasks: {e}"))?;
    let pack_task_arns: Vec<String> = packs
        .iter()
        .filter(|p| p.last_status != "STOPPED")
        .map(|p| p.task_arn.clone())
        .collect();
    let training_done = match training.as_deref() {
        None => true,
        Some("Completed" | "Failed" | "Stopped") => true,
        // InProgress, Stopping and anything unknown: not terminal (fail closed).
        Some(_) => false,
    };
    Ok(ComputeState {
        terminal: training_done && pack_task_arns.is_empty(),
        training,
        pack_tasks: pack_task_arns.len(),
        pack_task_arns,
    })
}

/// What [`stop_live_compute`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StopOutcome {
    /// True when `StopTrainingJob` was issued.
    pub training_job: bool,
    /// How many `StopTask` calls were issued.
    pub pack_tasks: usize,
    /// Failed stop calls (the next sweep re-issues them).
    pub errors: Vec<String>,
}

/// Stop what `state` says is still live: `StopTrainingJob` only for an `InProgress`
/// job (never `Stopping` or terminal), `StopTask` for every live pack task. A failed
/// call is recorded, not raised, so one failure does not hide the other stop.
pub async fn stop_live_compute(
    ctx: &FinalizerCtx,
    task_id: &str,
    state: &ComputeState,
) -> StopOutcome {
    let mut out = StopOutcome::default();
    if state.training.as_deref() == Some("InProgress") {
        match ctx.training_jobs.stop(&training_job_name(task_id)).await {
            Ok(()) => out.training_job = true,
            Err(e) => out.errors.push(format!("StopTrainingJob: {e}")),
        }
    }
    for arn in &state.pack_task_arns {
        match ctx.pack_tasks.stop(arn).await {
            Ok(()) => out.pack_tasks += 1,
            Err(e) => out.errors.push(format!("StopTask: {e}")),
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod fakes {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::{ExecutionInfo, ExecutionReader, PackTask, PackTasks, TrainingJobs};

    /// Scripted executions: the test sets what `describe` answers per ARN and what
    /// `list_running` returns; stops are recorded.
    #[derive(Default)]
    pub struct FakeExecutionReader {
        pub infos: Mutex<HashMap<String, ExecutionInfo>>,
        pub running: Mutex<Vec<String>>,
        pub stops: Mutex<Vec<String>>,
        pub describes: Mutex<usize>,
    }

    impl FakeExecutionReader {
        pub fn set(&self, arn: &str, status: &str, input: &str, start_secs: u64) {
            self.infos.lock().expect("lock").insert(
                arn.to_string(),
                ExecutionInfo {
                    status: status.to_string(),
                    input: input.to_string(),
                    start_secs,
                },
            );
        }

        pub fn stops(&self) -> Vec<String> {
            self.stops.lock().expect("lock").clone()
        }
    }

    #[pmcp::async_trait]
    impl ExecutionReader for FakeExecutionReader {
        async fn describe(&self, arn: &str) -> Result<Option<ExecutionInfo>, String> {
            *self.describes.lock().expect("lock") += 1;
            Ok(self.infos.lock().expect("lock").get(arn).cloned())
        }

        async fn stop(&self, arn: &str) -> Result<(), String> {
            self.stops.lock().expect("lock").push(arn.to_string());
            Ok(())
        }

        async fn list_running(&self, _machine: &str) -> Result<Vec<String>, String> {
            Ok(self.running.lock().expect("lock").clone())
        }
    }

    /// Scripted training jobs, keyed by job name.
    #[derive(Default)]
    pub struct FakeJobs {
        pub status: Mutex<HashMap<String, String>>,
        pub stops: Mutex<Vec<String>>,
    }

    impl FakeJobs {
        pub fn set(&self, name: &str, status: &str) {
            self.status
                .lock()
                .expect("lock")
                .insert(name.to_string(), status.to_string());
        }

        pub fn stops(&self) -> Vec<String> {
            self.stops.lock().expect("lock").clone()
        }
    }

    #[pmcp::async_trait]
    impl TrainingJobs for FakeJobs {
        async fn status(&self, name: &str) -> Result<Option<String>, String> {
            Ok(self.status.lock().expect("lock").get(name).cloned())
        }

        async fn stop(&self, name: &str) -> Result<(), String> {
            self.stops.lock().expect("lock").push(name.to_string());
            Ok(())
        }
    }

    /// Scripted pack tasks: `(task_id, arn, last_status)`.
    #[derive(Default)]
    pub struct FakePacks {
        pub tasks: Mutex<Vec<(String, String, String)>>,
        pub stops: Mutex<Vec<String>>,
    }

    impl FakePacks {
        pub fn add(&self, task_id: &str, arn: &str, last_status: &str) {
            self.tasks.lock().expect("lock").push((
                task_id.to_string(),
                arn.to_string(),
                last_status.to_string(),
            ));
        }

        pub fn set_status(&self, arn: &str, last_status: &str) {
            for t in self.tasks.lock().expect("lock").iter_mut() {
                if t.1 == arn {
                    t.2 = last_status.to_string();
                }
            }
        }

        pub fn stops(&self) -> Vec<String> {
            self.stops.lock().expect("lock").clone()
        }
    }

    #[pmcp::async_trait]
    impl PackTasks for FakePacks {
        async fn live_for(&self, task_id: &str) -> Result<Vec<PackTask>, String> {
            Ok(self
                .tasks
                .lock()
                .expect("lock")
                .iter()
                .filter(|(id, _, status)| id == task_id && status != "STOPPED")
                .map(|(_, arn, status)| PackTask {
                    task_arn: arn.clone(),
                    last_status: status.clone(),
                })
                .collect())
        }

        async fn stop(&self, arn: &str) -> Result<(), String> {
            self.stops.lock().expect("lock").push(arn.to_string());
            Ok(())
        }
    }
}
