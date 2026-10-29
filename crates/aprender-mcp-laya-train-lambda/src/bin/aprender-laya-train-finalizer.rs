//! `aprender-laya-train-finalizer` — the Laya training server's finalizer.
//!
//! The one writer of a run's terminal task record, `verdict.json`,
//! `manifest.json`, retention tags and lock release (plan 14-05). It is invoked
//! three ways, all as the same Lambda: by the state machine's `Finalize` state, by
//! an EventBridge Step Functions status-change rule, and by a schedule that sends
//! `{"source":"reconcile"}` every ten minutes.
//!
//! This bin is NOT named `bootstrap`: the CDK asset build renames it to
//! `bootstrap` inside its own asset (a Lambda custom-runtime requirement), and
//! `cargo pmcp` finds a deployable by the name `bootstrap`, which only the request
//! function may carry here. It must never be mistaken for a second deployable.
//!
//! # Configuration
//!
//! `APRENDER_LAYA_TRAIN_TABLE`, `_BUCKET`, `_STATE_MACHINE_ARN`, `_CLUSTER_ARN` and
//! `_PACK_FAMILY`. An unset one fails the function's init (fail closed), not the
//! first invocation.

use std::sync::Arc;

use aprender_mcp_laya_train::TaskBackend;
use aprender_mcp_laya_train_lambda::{
    handle, require_env, EcsPackTasks, FinalizerCtx, FinalizerInput, S3ObjectStore, SageMakerJobs,
    SfnExecutionReader, ENV_BUCKET, ENV_CLUSTER_ARN, ENV_PACK_FAMILY, ENV_STATE_MACHINE_ARN,
    ENV_TABLE,
};
use aprender_setfit_train_lambda::DynamoDbTaskBackend;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use tracing_subscriber::EnvFilter;

/// The five names this function is configured by, read once at startup.
struct Env {
    table: String,
    bucket: String,
    state_machine_arn: String,
    cluster_arn: String,
    pack_family: String,
}

fn read_env() -> Result<Env, String> {
    Ok(Env {
        table: require_env(ENV_TABLE)?,
        bucket: require_env(ENV_BUCKET)?,
        state_machine_arn: require_env(ENV_STATE_MACHINE_ARN)?,
        cluster_arn: require_env(ENV_CLUSTER_ARN)?,
        pack_family: require_env(ENV_PACK_FAMILY)?,
    })
}

/// Build the context once per container: ONE backend `Arc` for the store and the lock.
async fn build(env: Env) -> FinalizerCtx {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let backend: Arc<dyn TaskBackend> = Arc::new(DynamoDbTaskBackend::new(
        aws_sdk_dynamodb::Client::new(&config),
        env.table,
    ));
    FinalizerCtx::new(
        backend,
        Arc::new(S3ObjectStore::new(
            aws_sdk_s3::Client::new(&config),
            env.bucket.clone(),
        )),
        Arc::new(SfnExecutionReader::new(aws_sdk_sfn::Client::new(&config))),
        Arc::new(SageMakerJobs::new(aws_sdk_sagemaker::Client::new(&config))),
        Arc::new(EcsPackTasks::new(
            aws_sdk_ecs::Client::new(&config),
            env.cluster_arn,
            env.pack_family,
        )),
        env.bucket,
        env.state_machine_arn,
    )
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_ansi(false)
        .try_init();

    // Fail the function's init, not every invocation separately.
    let env = read_env().map_err(Error::from)?;
    let ctx = build(env).await;

    lambda_runtime::run(service_fn(|event: LambdaEvent<serde_json::Value>| {
        let ctx = &ctx;
        async move {
            let input = FinalizerInput::parse(&event.payload).map_err(|e| {
                tracing::error!("laya finalizer refused an input: {e}");
                Error::from(e.to_string())
            })?;
            match handle(ctx, input).await {
                Ok(report) => {
                    // One line per invocation: ids, phases and counts, never a
                    // credential or an object's content.
                    tracing::info!(
                        "laya finalizer ok: task={} phase={:?} record={} keep={} tagged={} \
                         released={} awaiting_compute={} stopped_job={} stopped_packs={}",
                        report.task_id,
                        report.phase,
                        report.record,
                        report.keep,
                        report.tagged,
                        report.released,
                        report.awaiting_compute,
                        report.stopped_training_job,
                        report.stopped_pack_tasks,
                    );
                    if let Some(sweep) = &report.reconcile {
                        tracing::info!(
                            "laya finalizer sweep: holder={:?} action={} stopped_execution={} rogue_stopped={}",
                            sweep.holder,
                            sweep.action,
                            sweep.stopped_execution,
                            sweep.rogue_stopped.len(),
                        );
                    }
                    Ok::<(), Error>(())
                }
                Err(e) => {
                    // An Err keeps the lock held and trips the FinalizerErrors alarm.
                    tracing::error!("laya finalizer failed: {e}");
                    Err(Error::from(e.to_string()))
                }
            }
        }
    }))
    .await
}
