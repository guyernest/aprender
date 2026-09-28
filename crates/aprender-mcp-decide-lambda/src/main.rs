//! `bootstrap` — the AWS Lambda entry for the thin decide MCP server (Phase 8 D-15).
//!
//! The loopback-proxy pattern of `aprender-mcp-chronos-lambda`: the pmcp
//! streamable-HTTP server runs as an in-process background task bound to 127.0.0.1,
//! configured `stateless()`, and each Lambda invocation is proxied to it over loopback.
//!
//! The model loads LAZILY, on the first MCP request, behind a load lock so concurrent
//! first calls load once. It is not loaded in the init phase: default Lambda caps init
//! near 10 s, below the ~10.4 s download + build of the real artifact (RESEARCH A3). A
//! failed load (a hash mismatch, a download past its deadline, ...) leaves the lock
//! re-armed, so the next request retries.
//!
//! Cold-start evidence: the request that performs the load logs one
//! `decide.load performed_load=true probe_id=<id> load_ms=<n> ...` line, every other
//! request logs `performed_load=false`, and the proxied response carries
//! `x-decide-load: cold;load_ms=<n>` or `warm`. Request text is never logged.

#![allow(clippy::disallowed_methods)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use aprender_mcp_decide_lambda::{
    build_server, load_header_value, load_log_line, parse_probe_id, resolve_model, start_loopback,
    watch_loopback, LoadOnce, LoadTimeline, ModelSource, LOAD_HEADER, PROBE_ID_HEADER,
};
use lambda_http::{run, service_fn, Body, Error, Request, Response};
use once_cell::sync::OnceCell;
use reqwest::Client;
use tracing_subscriber::EnvFilter;

const DEFAULT_SERVER_NAME: &str = "aprender-decide";
const PACKAGE: &str = env!("CARGO_PKG_NAME");

/// What the first MCP request builds: the loopback server over the loaded model.
struct Loaded {
    base_url: String,
    timeline: LoadTimeline,
}

static LOADED: LoadOnce<Loaded> = LoadOnce::new();
static HTTP: OnceCell<Client> = OnceCell::new();

fn server_name() -> String {
    std::env::var("PMCP_SERVER_ID").unwrap_or_else(|_| DEFAULT_SERVER_NAME.to_string())
}

/// Why the first request could not bring the server up; logged in full, returned to the
/// caller only as its kind.
struct LoadFailure {
    kind: &'static str,
    detail: String,
}

/// Resolve the model (S3 into memory, or a local path), then start the loopback server.
async fn load() -> Result<Loaded, LoadFailure> {
    let source = ModelSource::from_env().map_err(|e| LoadFailure {
        kind: "config",
        detail: e.to_string(),
    })?;
    let (model, timeline) = resolve_model(&source).await.map_err(|e| LoadFailure {
        kind: e.kind(),
        detail: e.to_string(),
    })?;
    let name = server_name();
    let server = build_server(Arc::new(model), &name, env!("CARGO_PKG_VERSION")).map_err(|e| {
        LoadFailure {
            kind: "server",
            detail: e.to_string(),
        }
    })?;
    let port = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::new(std::net::IpAddr::from([127, 0, 0, 1]), port);
    let (bound, handle) = start_loopback(server, addr)
        .await
        .map_err(|e| LoadFailure {
            kind: "bind",
            detail: e.to_string(),
        })?;
    // A loopback that ends would leave `LOADED` pointing at a dead base_url for every later
    // request; the watcher ends the process instead, so Lambda replaces the environment.
    tokio::spawn(watch_loopback(handle, |code| std::process::exit(code)));
    tracing::info!(
        "{name}: MCP server on {bound}, serving artifact_sha256={}",
        timeline.artifact_sha256
    );
    Ok(Loaded {
        base_url: format!("http://{bound}"),
        timeline,
    })
}

fn http() -> Result<&'static Client, Error> {
    HTTP.get_or_try_init(|| {
        Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| Error::from(e.to_string()))
    })
}

fn json_response(status: u16, body: String) -> Result<Response<Body>, Error> {
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

    // Health check — also what pmcp.run's landing page probes. It names THIS package,
    // so a post-deploy GET tells this binary apart from the Chronos one.
    if method.as_str() == "GET" {
        let body = serde_json::json!({
            "ok": true,
            "server": server_name(),
            "package": PACKAGE,
            "loaded": LOADED.get().is_some(),
            "message": "Laya decide MCP server (aprender-mcp-decide via aprender-mcp-decide-lambda): one task-bound `classify` tool. POST JSON-RPC to '/' for MCP requests."
        })
        .to_string();
        return json_response(200, body);
    }

    // CORS preflight.
    if method.as_str() == "OPTIONS" {
        return Response::builder()
            .status(200)
            .header("access-control-allow-origin", "*")
            .header("access-control-allow-methods", "POST, OPTIONS, GET")
            .header(
                "access-control-allow-headers",
                "content-type, authorization, mcp-protocol-version, x-decide-probe-id",
            )
            .body(Body::Empty)
            .map_err(|e| Error::from(e.to_string()));
    }

    let probe_id = parse_probe_id(
        event
            .headers()
            .get(PROBE_ID_HEADER)
            .and_then(|v| v.to_str().ok()),
    )
    .map(str::to_string);

    let started = Instant::now();
    let (loaded, performed_load) = match LOADED.get_or_try_load(load).await {
        Ok(v) => v,
        Err(failure) => {
            tracing::error!(
                kind = failure.kind,
                "decide.load failed probe_id={}: {}",
                probe_id.as_deref().unwrap_or("none"),
                failure.detail
            );
            let body = serde_json::json!({
                "ok": false,
                "error": "model load failed; the next request retries",
                "kind": failure.kind,
            })
            .to_string();
            return json_response(503, body);
        }
    };
    let load_ms = started.elapsed().as_millis();
    tracing::info!(
        "{}",
        load_log_line(
            performed_load,
            probe_id.as_deref(),
            load_ms,
            performed_load.then_some(&loaded.timeline),
        )
    );

    let url = format!("{}{path_q}", loaded.base_url);
    let reqwest_method = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|e| Error::from(e.to_string()))?;

    let mut req = http()?.request(reqwest_method, &url);
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

    let mut builder = Response::builder()
        .status(status.as_u16())
        .header("access-control-allow-origin", "*")
        .header(LOAD_HEADER, load_header_value(performed_load, load_ms));
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

    run(service_fn(handler)).await
}
