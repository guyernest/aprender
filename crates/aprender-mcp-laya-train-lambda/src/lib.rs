//! The AWS ends of the Laya training server's seams: the request function's
//! dataset source, Step Functions dispatcher, cancel sink and read-time presigner.
//!
//! `aprender-mcp-laya-train` owns the tools, the single-flight lease and the
//! finalize decision, and leaves three seams open. This crate fills them with S3
//! and Step Functions and adds nothing else; if something here starts to look like
//! a second copy of a rule that lives upstream, it is wrong.
//!
//! The request function holds no AWS permission of its own (D-10): every grant
//! arrives later, and the three variables below are all it needs to find its
//! resources.

mod compute;
mod dataset;
mod finalizer;
mod objects;
mod sfn;

pub use compute::{
    compute_terminal, stop_live_compute, training_job_name, ComputeState, EcsPackTasks,
    ExecutionInfo, ExecutionReader, PackTask, PackTasks, SageMakerJobs, SfnExecutionReader,
    StopOutcome, TrainingJobs, PACK_TASK_ENV, TRAINING_JOB_PREFIX,
};
pub use dataset::{ObjectReader, ReadError, S3DatasetSource, S3ObjectReader};
pub use finalizer::{
    handle, reconcile, FinalizeReport, FinalizerCtx, FinalizerError, FinalizerInput,
    ReconcileReport, StateMachinePayload, StatusEvent, DISPATCH_CONFIRM_GRACE_SECS,
    EVENT_DETAIL_TYPE,
};
pub use objects::{ObjectError, ObjectStore, S3ObjectStore};
pub use sfn::{
    classify_start_error, execution_arn, guard_link_key, Executions, Presign, S3Presign,
    SfnCancelSink, SfnDispatcher, SfnExecutions, StartError, StartErrorKind, CANCEL_JOIN_BOUND,
    EXECUTION_NAME_PREFIX,
};

/// The DynamoDB table holding task records and the single-flight lock.
pub const ENV_TABLE: &str = "APRENDER_LAYA_TRAIN_TABLE";

/// The server bucket: `datasets/` in, `runs/` out.
pub const ENV_BUCKET: &str = "APRENDER_LAYA_TRAIN_BUCKET";

/// The Step Functions state machine one `train` starts an execution of.
pub const ENV_STATE_MACHINE_ARN: &str = "APRENDER_LAYA_TRAIN_STATE_MACHINE_ARN";

/// The ECS cluster the pack task runs on (the finalizer lists its tasks).
pub const ENV_CLUSTER_ARN: &str = "APRENDER_LAYA_TRAIN_CLUSTER_ARN";

/// The pack task-definition family (the finalizer lists tasks of this family).
pub const ENV_PACK_FAMILY: &str = "APRENDER_LAYA_TRAIN_PACK_FAMILY";

/// The streamable-HTTP config for the deployed Lambda and the tracer test.
///
/// `stateless()` is not a preference: sessions and SSE do not survive serverless
/// (successive requests reach different containers, each with a fresh in-memory
/// session map). MCP Tasks still work across containers because `tasks/get` is
/// served from the DynamoDB store. `max_request_bytes` is the server crate's own
/// bound, so an oversized body is refused with HTTP 413 before any JSON parsing.
#[must_use]
pub fn server_config() -> pmcp::server::streamable_http_server::StreamableHttpServerConfig {
    pmcp::server::streamable_http_server::StreamableHttpServerConfig {
        max_request_bytes: aprender_mcp_laya_train::layout::MAX_REQUEST_BYTES,
        ..pmcp::server::streamable_http_server::StreamableHttpServerConfig::stateless()
    }
}

/// Read a required variable at startup, or say which one is missing.
///
/// # Errors
///
/// A message naming the variable.
pub fn require_env(name: &str) -> Result<String, String> {
    aprender_setfit_train_lambda::require_env(name)
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use aprender_mcp_laya_train::layout::{
        parse_inputs_string, ExecutionInput, DOWNLOAD_URL_TTL_SECS, SCHEMA_EXECUTION_INPUT,
    };
    use aprender_mcp_laya_train::{
        build_server, Dispatcher, InMemoryTaskBackend, ServerCtx, TaskBackend,
    };
    use sha2::{Digest, Sha256};

    use crate::dataset::fakes::FakeReader;
    use crate::sfn::fakes::{FakeExecutions, FakePresign};
    use crate::{S3DatasetSource, SfnDispatcher};

    const BUCKET: &str = "fixture-bucket";
    const MACHINE: &str = "arn:aws:states:us-east-1:123456789012:stateMachine:laya-train-dev";

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// A 3-class data dir, `per_class` train rows and 2 eval rows per class.
    pub(crate) fn dataset(per_class: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let task = br#"{"type":"choice","instructions":"Which team?","criteria":{"shipping":"parcels","billing":"charges","account":"login"}}"#.to_vec();
        let (mut train, mut eval) = (String::new(), String::new());
        for label in ["shipping", "billing", "account"] {
            for i in 0..per_class {
                train.push_str(&format!(
                    "{{\"text\": \"train {label} message number {i}\", \"label\": \"{label}\"}}\n"
                ));
            }
            for i in 0..2 {
                eval.push_str(&format!(
                    "{{\"text\": \"eval {label} message number {i}\", \"label\": \"{label}\"}}\n"
                ));
            }
        }
        (task, train.into_bytes(), eval.into_bytes())
    }

    /// One MCP request over loopback HTTP; the JSON-RPC response, parsed.
    async fn rpc(
        http: &reqwest::Client,
        url: &str,
        id: u64,
        method: &str,
        params: serde_json::Value,
    ) -> serde_json::Value {
        let body =
            serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
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
            .unwrap_or(&text);
        serde_json::from_str(payload).unwrap_or_else(|e| panic!("non-JSON body: {e}\n{text}"))
    }

    #[tokio::test]
    async fn tracer_request() {
        let (task, train, eval) = dataset(20);
        let mut reader = FakeReader::default();
        reader.put(BUCKET, "datasets/tracer/task.json", task.clone());
        reader.put(BUCKET, "datasets/tracer/train.jsonl", train.clone());
        reader.put(BUCKET, "datasets/tracer/eval.jsonl", eval.clone());

        let executions = Arc::new(FakeExecutions::default());
        let dispatcher = Arc::new(SfnDispatcher::new(
            Arc::clone(&executions) as Arc<dyn crate::Executions>,
            Arc::new(FakePresign),
            BUCKET,
            MACHINE,
        ));
        let backend: Arc<dyn TaskBackend> = Arc::new(InMemoryTaskBackend::new());
        // The same constructors the bootstrap uses.
        let ctx = ServerCtx::new(
            backend,
            Arc::clone(&dispatcher) as Arc<dyn Dispatcher>,
            Arc::new(S3DatasetSource::new(reader, BUCKET)),
            None,
        );
        let server = build_server(ctx, "0.0.0").expect("the server builds");
        let http_server = pmcp::server::streamable_http_server::StreamableHttpServer::with_config(
            "127.0.0.1:0".parse().expect("addr"),
            Arc::new(tokio::sync::Mutex::new(server)),
            crate::server_config(),
        );
        let (bound, _handle) = http_server.start().await.expect("the server binds");
        let url = format!("http://{bound}");
        let http = reqwest::Client::new();

        let init = rpc(
            &http,
            &url,
            1,
            "initialize",
            serde_json::json!({"protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "tracer", "version": "0"}}),
        )
        .await;
        assert!(init["error"].is_null(), "{init}");

        let created = rpc(
            &http,
            &url,
            2,
            "tools/call",
            serde_json::json!({"name": "train", "task": {}, "arguments": {
                "dataset_uri": format!("s3://{BUCKET}/datasets/tracer/"),
                "name": "tracer-model", "epochs": 4}}),
        )
        .await;
        let id = created["result"]["task"]["taskId"]
            .as_str()
            .unwrap_or_else(|| panic!("a task-shaped answer: {created}"))
            .to_string();
        assert_eq!(created["result"]["task"]["status"], "working", "{created}");

        let starts = executions.starts.lock().expect("lock").clone();
        assert_eq!(starts.len(), 1, "exactly one start: {starts:?}");
        assert_eq!(starts[0].0, format!("laya-{id}"));
        let raw: serde_json::Value = serde_json::from_str(&starts[0].1).expect("json input");
        assert_eq!(raw.as_object().expect("object").len(), 9, "{raw}");
        let input: ExecutionInput = serde_json::from_str(&starts[0].1).expect("closed input");
        assert_eq!(input.schema, SCHEMA_EXECUTION_INPUT);
        assert_eq!(input.epochs, "4");
        assert_eq!(input.run_prefix, format!("runs/tracer-model/{id}/"));
        assert_eq!(input.dataset_prefix, "datasets/tracer/");
        let want: BTreeMap<String, String> = [
            ("task.json", &task),
            ("train.jsonl", &train),
            ("eval.jsonl", &eval),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), sha(v)))
        .collect();
        assert_eq!(
            parse_inputs_string(&input.inputs_sha256).expect("parses"),
            want
        );

        let url = dispatcher
            .link(&input.run_prefix, "run/gate-report.json")
            .await
            .expect("a link inside the run prefix");
        assert_eq!(
            url,
            format!(
                "https://presigned.example/{BUCKET}/runs/tracer-model/{id}/run/gate-report.json?ttl={DOWNLOAD_URL_TTL_SECS}"
            )
        );
        assert_eq!(dispatcher.link(&input.run_prefix, "../x").await, None);
    }
}
