//! `bootstrap` — the AWS Lambda entry for the Laya TRAINING MCP server's request
//! function.
//!
//! The same loopback-proxy pattern the SetFit training deployment proved: the pmcp
//! streamable-HTTP server runs as an in-process background task bound to
//! 127.0.0.1, and each Lambda invocation is proxied to it over loopback. What
//! differs is everything behind the tools: the task store is DynamoDB, the
//! dispatcher starts one Step Functions execution, and datasets are read from S3.
//!
//! # This function trains nothing, and holds no AWS permission of its own
//!
//! Its whole job per `train` call is: validate the dataset, mint a task, take the
//! single-flight lock, start one execution and answer `working`. It carries no
//! `apr` binary and no dataset. Every AWS permission arrives later, by the grant
//! (D-10, plans 14-10 and 14-13); until then a call fails on AccessDenied, which
//! the library surfaces as a typed refusal rather than a crash.

// serde_json::json! expands to .unwrap() internally (health-check body).
#![allow(clippy::disallowed_methods)]

use std::net::SocketAddr;
use std::sync::Arc;

use aprender_mcp_laya_train::layout::SERVER_NAME;
use aprender_mcp_laya_train::{build_server, Dispatcher, ServerCtx, TaskBackend};
use aprender_mcp_laya_train_lambda::{
    require_env, server_config, Executions, S3DatasetSource, S3ObjectReader, S3Presign,
    SfnCancelSink, SfnDispatcher, SfnExecutions, ENV_BUCKET, ENV_STATE_MACHINE_ARN, ENV_TABLE,
};
use aprender_setfit_train_lambda::DynamoDbTaskBackend;
use lambda_http::{run, service_fn, Body, Error, Request, Response};
use once_cell::sync::OnceCell;
use reqwest::Client;
use tracing_subscriber::EnvFilter;

static BASE_URL: OnceCell<String> = OnceCell::new();
static HTTP: OnceCell<Client> = OnceCell::new();

/// The three names this deployment is configured by. Read ONCE at startup so an
/// unset one fails the function's init, not a client's first `train`.
struct Env {
    table: String,
    bucket: String,
    state_machine_arn: String,
}

fn read_env() -> Result<Env, String> {
    Ok(Env {
        table: require_env(ENV_TABLE)?,
        bucket: require_env(ENV_BUCKET)?,
        state_machine_arn: require_env(ENV_STATE_MACHINE_ARN)?,
    })
}

/// Build the MCP server over the DynamoDB store, the Step Functions dispatcher,
/// the S3 dataset source and the best-effort cancel sink.
///
/// ONE `Arc<dyn TaskBackend>` goes into [`ServerCtx::new`], which builds the task
/// store from it and keeps the same `Arc` for the single-flight lock, so the two
/// can never address different tables.
async fn build() -> Result<pmcp::Server, String> {
    let env = read_env()?;
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let backend: Arc<dyn TaskBackend> = Arc::new(DynamoDbTaskBackend::new(
        aws_sdk_dynamodb::Client::new(&config),
        env.table,
    ));
    let s3 = aws_sdk_s3::Client::new(&config);
    let executions: Arc<dyn Executions> = Arc::new(SfnExecutions::new(
        aws_sdk_sfn::Client::new(&config),
        env.state_machine_arn.clone(),
    ));
    let dispatcher: Arc<dyn Dispatcher> = Arc::new(SfnDispatcher::new(
        Arc::clone(&executions),
        Arc::new(S3Presign::new(s3.clone())),
        env.bucket.clone(),
        env.state_machine_arn.clone(),
    ));
    let source = Arc::new(S3DatasetSource::new(S3ObjectReader::new(s3), env.bucket));
    let cancel = Arc::new(SfnCancelSink::new(executions, env.state_machine_arn));
    let ctx = ServerCtx::new(backend, dispatcher, source, Some(cancel));
    build_server(ctx, env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())
}

/// Start the streamable-HTTP server in the background; return the bound addr.
async fn start_http_in_background(default_port: u16) -> Result<SocketAddr, String> {
    let server = build().await?;
    let server = Arc::new(tokio::sync::Mutex::new(server));

    let port = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(default_port);
    let host = std::env::var("MCP_HTTP_HOST")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| std::net::IpAddr::from([127, 0, 0, 1]));
    let addr = SocketAddr::new(host, port);

    let http_server = pmcp::server::streamable_http_server::StreamableHttpServer::with_config(
        addr,
        server,
        server_config(),
    );
    let (bound, handle) = http_server.start().await.map_err(|e| e.to_string())?;
    tracing::info!("{SERVER_NAME}: MCP server started on {bound}");

    tokio::spawn(async move {
        if let Err(e) = handle.await {
            tracing::error!("HTTP server error: {e}");
        }
    });

    Ok(bound)
}

/// Start the background server exactly once per container.
async fn ensure_server_started() -> Result<String, Error> {
    if let Some(url) = BASE_URL.get() {
        return Ok(url.clone());
    }
    let bound = start_http_in_background(8080).await.map_err(Error::from)?;
    let base = format!("http://{bound}");
    let _ = BASE_URL.set(base.clone());
    let client = Client::builder()
        .build()
        .map_err(|e| Error::from(e.to_string()))?;
    let _ = HTTP.set(client);
    Ok(base)
}

fn plain_response(status: u16, body: String) -> Result<Response<Body>, Error> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .header("access-control-allow-origin", "*")
        .body(Body::Text(body))
        .map_err(|e| Error::from(e.to_string()))
}

/// Proxy one Lambda invocation to the loopback MCP server.
async fn handler(event: Request) -> Result<Response<Body>, Error> {
    let method = event.method().clone();
    let path_q = event
        .uri()
        .path_and_query()
        .map_or_else(|| String::from("/"), |pq| pq.as_str().to_string());

    // Health check — also what pmcp.run's landing page probes.
    if method.as_str() == "GET" {
        let body = serde_json::json!({
            "ok": true,
            "server": SERVER_NAME,
            "message": "Laya fine-tuning TRAINING MCP server (aprender-mcp-laya-train). POST JSON-RPC \
                        to '/' for MCP requests; `train` returns an MCP task, poll tasks/get or train_status."
        })
        .to_string();
        return plain_response(200, body);
    }

    // CORS preflight.
    if method.as_str() == "OPTIONS" {
        return Response::builder()
            .status(200)
            .header("access-control-allow-origin", "*")
            .header("access-control-allow-methods", "POST, OPTIONS, GET")
            .header(
                "access-control-allow-headers",
                "content-type, authorization",
            )
            .body(Body::Empty)
            .map_err(|e| Error::from(e.to_string()));
    }

    let base = ensure_server_started().await?;
    let client = HTTP
        .get()
        .ok_or_else(|| Error::from("http client must exist after ensure_server_started"))?;

    let url = format!("{base}{path_q}");
    let reqwest_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|e| Error::from(e.to_string()))?;

    let mut req = client.request(reqwest_method, &url);
    for (name, value) in event.headers() {
        if let Ok(val) = value.to_str() {
            if name.as_str().eq_ignore_ascii_case("host") {
                continue;
            }
            req = req.header(name.as_str(), val);
        }
    }
    let body_bytes = match event.body() {
        Body::Empty => Vec::new(),
        Body::Text(s) => s.as_bytes().to_vec(),
        Body::Binary(b) => b.clone(),
    };
    req = req.body(body_bytes);

    let resp = req.send().await.map_err(|e| Error::from(e.to_string()))?;
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.bytes().await.map_err(|e| Error::from(e.to_string()))?;

    let mut builder = Response::builder().status(status.as_u16());
    builder = builder.header("access-control-allow-origin", "*");
    for (name, value) in &headers {
        if let Ok(val) = value.to_str() {
            if name.as_str().eq_ignore_ascii_case("transfer-encoding")
                || name.as_str().eq_ignore_ascii_case("content-length")
            {
                continue;
            }
            builder = builder.header(name.as_str(), val);
        }
    }
    builder
        .body(Body::Binary(bytes.to_vec()))
        .map_err(|e| Error::from(e.to_string()))
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_ansi(false)
        .try_init();

    // Fail closed at startup: a function that boots without its table, bucket or
    // state machine only fails once someone is waiting on it.
    read_env().map_err(Error::from)?;

    run(service_fn(handler)).await
}
