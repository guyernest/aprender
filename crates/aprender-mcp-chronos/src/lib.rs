//! A THIN, stateless MCP `forecast` server over `aprender_forecast::chronos` — the
//! ZERO-SHOT forecaster (D-03).
//!
//! One immutable Chronos-Bolt model behind one tool: no fit, no knobs, no RNG, nothing to
//! store. The weights are compiled into the binary (`CHRONOS_EMBED_DIR` at build time,
//! D-13) or read once at startup from `CHRONOS_MODEL_DIR`; they are NEVER downloaded at
//! cold start. Every bound and every refusal lives in `aprender_forecast::chronos` — this
//! crate re-checks nothing (OPS-03).
//!
//! Same shape as `aprender-mcp-forecast` (D-06/D-08), minus the router pool: a Chronos
//! call is ~18 ms against a shared `Arc<Model>`, so pmcp's per-router server mutex is not
//! the bottleneck it is for a seconds-long Prophet fit (D-12).
//!
//! Ported from `sources/007-chronos-mcp-thin-server/src/lib.rs`.

// schemars' JsonSchema derive (reached through ChronosArgs) and serde_json::json! both
// expand to .unwrap() internally, at file scope where a narrower allow cannot reach. Same
// precedent as aprender-mcp-forecast/src/lib.rs:13.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use aprender_forecast::chronos::{
    load_model_from_bytes, load_model_from_dir, ChronosArgs, Model, ModelLoadError,
};
use aprender_forecast::ForecastError;
use pmcp::types::capabilities::ServerCapabilities;
use pmcp::Server;

/// The Chronos-Bolt weights staged by `build.rs`.
///
/// Non-empty when the build set `CHRONOS_EMBED_DIR`; EMPTY in a plain build, where
/// [`resolve_model`] falls back to `CHRONOS_MODEL_DIR` read as a runtime path.
pub static EMBEDDED_WEIGHTS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/model.safetensors"));

/// The checkpoint `config.json` staged alongside [`EMBEDDED_WEIGHTS`], and empty exactly
/// when it is (`embedded_markers_are_consistent`).
pub static EMBEDDED_CONFIG: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/config.json"));

/// The single tool this server advertises.
pub const TOOL_NAME: &str = "forecast";

/// The tool description, lifted into `tools/list`.
///
/// The coverage sentence is the D-16 honesty requirement: the band is reported at its
/// MEASURED empirical coverage on rolling origins, never at the nominal 80 %.
pub const TOOL_DESCRIPTION: &str = "Zero-shot time-series forecast with Chronos-Bolt: no training, no model to store — send the series, \
 get quantile forecasts back in one call. Pass parallel arrays `ds` (dates, YYYY-MM-DD) and `y` (numbers; `null` for a \
 missing value), `horizon` (future periods) and optionally `freq` (D, W or MS — daily by default). Returns future `ds`, \
 `yhat` (median), `yhat_lower`/`yhat_upper` (10th/90th percentiles) and all nine quantiles (0.1 … 0.9). The model looks at \
 the last 2,048 points. Accepts 4 to 20,000 points. Horizons 1 to 64 are one direct forecast; longer horizons need \
 `allow_long_horizon: true` because the model then rolls its own forecast forward and accuracy degrades past step 64 \
 (max 1,024), and the response then carries a warning naming the rollout. \
 Band honesty: the nominal q10–q90 band covered 0.65 of held-out points across 17 rolling-origin windows (spike 006) — \
 treat `yhat_lower`/`yhat_upper` as a ~65% band, not an 80% one.";

/// Load the model this process serves: embedded bytes if the build staged them, else
/// `CHRONOS_MODEL_DIR` read as a runtime path.
///
/// Read ONCE, at startup, never from a request (T-06-06 / V12).
///
/// # Errors
///
/// [`ModelLoadError`] when no model is available either way, or when the bytes at hand
/// are not a decodable Chronos-Bolt checkpoint.
pub fn resolve_model() -> Result<Model, ModelLoadError> {
    if !EMBEDDED_WEIGHTS.is_empty() {
        return load_model_from_bytes(EMBEDDED_WEIGHTS, EMBEDDED_CONFIG, "embedded");
    }
    let dir = std::env::var_os("CHRONOS_MODEL_DIR").ok_or_else(|| {
        ModelLoadError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no embedded model in this build and CHRONOS_MODEL_DIR is unset",
        ))
    })?;
    load_model_from_dir(std::path::Path::new(&dir))
}

/// Map the library's refusals onto MCP errors: the caller's fault stays the caller's fault.
#[must_use]
pub fn map_error(e: ForecastError) -> pmcp::Error {
    match e {
        ForecastError::Validation(s) => pmcp::Error::validation(s),
        ForecastError::Internal(s) => pmcp::Error::internal(s),
    }
}

/// One stateless tool over one immutable model.
///
/// The `Arc` is shared, never cloned-into-a-pool: nothing in the forward mutates the
/// model, so concurrent calls simply read it. The forward runs on a blocking thread so
/// the protocol loop is never stalled by an 18 ms — or, at horizon 365, a 0.85 s — CPU
/// burst.
///
/// # Errors
///
/// `pmcp::Error` if the server builder refuses the configuration.
pub fn build_server(model: Arc<Model>, name: &str, version: &str) -> pmcp::Result<Server> {
    Server::builder()
        .name(name)
        .version(version)
        .capabilities(ServerCapabilities::tools_only())
        .tool_typed_with_description::<ChronosArgs, _, _>(
            TOOL_NAME,
            TOOL_DESCRIPTION,
            move |args, _extra| {
                let model = Arc::clone(&model);
                async move {
                    let response = tokio::task::spawn_blocking(move || {
                        aprender_forecast::chronos::forecast(&model, &args)
                    })
                    .await
                    .map_err(|e| pmcp::Error::internal(format!("forecast task join: {e}")))?
                    .map_err(map_error)?;
                    serde_json::to_value(&response)
                        .map_err(|e| pmcp::Error::internal(format!("response serialization: {e}")))
                }
            },
        )
        .build()
}

/// The HTTP app: pmcp's streamable-http MCP router at `/mcp` (stateless, JSON responses,
/// localhost-locked CORS) plus a same-origin demo page and two sample datasets.
///
/// The demo page is itself an MCP client — it speaks `initialize` -> `tools/list` ->
/// `tools/call` against `/mcp`, so serving it proves the same door the tests drive.
pub fn http_app(server: Server) -> axum::Router {
    use axum::response::{Html, IntoResponse};
    use axum::routing::get;
    use pmcp::server::streamable_http_server::StreamableHttpServerConfig;

    let server = Arc::new(tokio::sync::Mutex::new(server));
    let config = pmcp::axum::RouterConfig {
        server_config: StreamableHttpServerConfig::stateless(),
        allowed_origins: Some(pmcp::axum::AllowedOrigins::localhost()),
        ..Default::default()
    };
    let mcp = pmcp::axum::router_with_config(server, config);
    axum::Router::new()
        .route(
            "/",
            get(|| async { Html(include_str!("../static/index.html")) }),
        )
        .route(
            "/sample/peyton",
            get(|| async {
                (
                    [("content-type", "text/csv")],
                    include_str!("../fixtures/peyton_manning.csv"),
                )
                    .into_response()
            }),
        )
        .route(
            "/sample/air",
            get(|| async {
                (
                    [("content-type", "text/csv")],
                    include_str!("../fixtures/air_passengers.csv"),
                )
                    .into_response()
            }),
        )
        .nest("/mcp", mcp)
}
