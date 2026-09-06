//! A THIN, stateless MCP `forecast` server over `aprender-forecast`.
//!
//! One tool, no model artifact: the request carries the series, the server fits and
//! forecasts inside the call (D-01). Same shape as `aprender-mcp-setfit` (D-06). Every
//! bound and every refusal lives in `aprender_forecast::forecast` — this crate re-checks
//! nothing (OPS-03).
//!
//! Ported from `sources/004-forecast-mcp-thin-server/src/lib.rs:278-307` (D-08).

// schemars' JsonSchema derive (re-exported through ForecastArgs) and serde_json::json!
// both expand to .unwrap() internally, at file scope where a narrower allow cannot
// reach. Same precedent as aprender-mcp-setfit/src/lib.rs:29-32.
#![allow(clippy::disallowed_methods)]

use aprender_forecast::{ForecastArgs, ForecastError};
use pmcp::types::capabilities::ServerCapabilities;
use pmcp::Server;

/// The single tool this server advertises.
pub const TOOL_NAME: &str = "forecast";

/// The tool description, lifted into `tools/list`.
///
/// The last sentence is the D-16 honesty requirement: the bands are reported at their
/// MEASURED empirical coverage, never at the nominal 80 %.
pub const TOOL_DESCRIPTION: &str = "Fit a time-series forecaster to the series you pass in and return a forecast — in one call, \
 no model to train or store first. Send parallel arrays `ds` (dates, YYYY-MM-DD) and `y` (numbers) plus \
 `horizon` (number of future periods) and optionally `freq` (D, W or MS — daily by default). Default model \
 is `prophet` (trend with changepoints + Fourier seasonality + optional holidays, with 80% uncertainty \
 bands and named components); `neuralprophet` adds an AR-Net over the last `n_lags` values for short-horizon \
 nowcasting. Returns future `ds`, `yhat`, `yhat_lower`, `yhat_upper`, `trend`, components and timing. \
 Bounded at 20,000 points and a 3,650-period horizon. \
 Band honesty: the nominal 80% interval covered 0.60 (Prophet) and 0.66 (NeuralProphet-lite) of held-out \
 points across 17 rolling-origin windows — treat `yhat_lower`/`yhat_upper` as a ~60-66% band, not an 80% one.";

/// Map the library's refusals onto MCP errors: the caller's fault stays the caller's fault.
#[must_use]
pub fn map_error(e: ForecastError) -> pmcp::Error {
    match e {
        ForecastError::Validation(s) => pmcp::Error::validation(s),
        ForecastError::Internal(s) => pmcp::Error::internal(s),
    }
}

/// One stateless tool. The fit runs on a blocking thread: seconds of CPU must not stall
/// the protocol loop.
///
/// # Errors
///
/// `pmcp::Error` if the server builder refuses the configuration.
pub fn build_server(name: &str, version: &str) -> pmcp::Result<Server> {
    Server::builder()
        .name(name)
        .version(version)
        .capabilities(ServerCapabilities::tools_only())
        .tool_typed_with_description::<ForecastArgs, _, _>(
            TOOL_NAME,
            TOOL_DESCRIPTION,
            move |args, _extra| async move {
                let response =
                    tokio::task::spawn_blocking(move || aprender_forecast::forecast(&args))
                        .await
                        .map_err(|e| pmcp::Error::internal(format!("forecast task join: {e}")))?
                        .map_err(map_error)?;
                serde_json::to_value(&response)
                    .map_err(|e| pmcp::Error::internal(format!("response serialization: {e}")))
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

    let server = std::sync::Arc::new(tokio::sync::Mutex::new(server));
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

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally
mod e2e {
    use super::{build_server, http_app};

    struct Client {
        http: reqwest::Client,
        url: String,
        next: u64,
    }

    impl Client {
        async fn call(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
            self.next += 1;
            let body = serde_json::json!({
                "jsonrpc": "2.0", "id": self.next, "method": method, "params": params
            });
            let r = self
                .http
                .post(&self.url)
                .header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream")
                .body(body.to_string())
                .send()
                .await
                .expect("send");
            let status = r.status();
            let text = r.text().await.expect("text");
            assert!(status.is_success(), "{method}: HTTP {status}\n{text}");
            let payload = text
                .lines()
                .find_map(|l| l.strip_prefix("data: "))
                .unwrap_or(&text);
            serde_json::from_str(payload)
                .unwrap_or_else(|e| panic!("{method}: non-JSON: {e}\n{text}"))
        }
    }

    fn tool_output(v: &serde_json::Value) -> serde_json::Value {
        if let Some(s) = v["result"].get("structuredContent") {
            return s.clone();
        }
        let text = v["result"]["content"][0]["text"]
            .as_str()
            .expect("text content");
        serde_json::from_str(text).expect("tool JSON")
    }

    fn csv(raw: &str) -> (Vec<String>, Vec<f64>) {
        let mut ds = Vec::new();
        let mut y = Vec::new();
        for line in raw.lines().skip(1) {
            let mut it = line.split(',');
            let Some(d) = it.next() else { continue };
            let Some(v) = it.next() else { continue };
            ds.push(d.trim().trim_matches('"').to_string());
            y.push(
                v.trim()
                    .trim_matches('"')
                    .parse()
                    .expect("second CSV column is a number"),
            );
        }
        (ds, y)
    }

    /// The tracer: ONE Prophet forecast crossing JSON-RPC -> tool schema -> validation
    /// door -> design -> L-BFGS -> predict -> response, over live streamable-HTTP,
    /// in-process.
    #[tokio::test]
    async fn forecast_prophet_happy_path_over_streamable_http() {
        let server = build_server("aprender-forecast-test", "0.0.0").expect("server");
        let app = http_app(server);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
        let mut c = Client {
            http: reqwest::Client::new(),
            url: format!("http://{addr}/mcp"),
            next: 0,
        };

        // The demo page is same-origin and is itself an MCP client.
        let page = reqwest::get(format!("http://{addr}/"))
            .await
            .expect("page")
            .text()
            .await
            .expect("html");
        assert!(
            page.contains("tools/call") && page.contains("forecast"),
            "demo page must be an MCP client"
        );

        let init = c
            .call(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2025-06-18", "capabilities": {},
                    "clientInfo": {"name": "e2e", "version": "0"}
                }),
            )
            .await;
        assert!(init.get("error").is_none(), "initialize: {init}");

        let tools = c.call("tools/list", serde_json::json!({})).await;
        let names: Vec<&str> = tools["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|t| t["name"].as_str().expect("name"))
            .collect();
        assert_eq!(names, vec!["forecast"], "exactly one tool");
        let schema = &tools["result"]["tools"][0]["inputSchema"];
        assert!(
            schema["properties"].get("horizon").is_some(),
            "schema advertises horizon: {schema}"
        );
        assert_eq!(
            schema["additionalProperties"],
            serde_json::json!(false),
            "deny_unknown_fields must surface in the advertised schema: {schema}"
        );

        // Peyton Manning, 365 days ahead — one real fit inside one call.
        let (ds, y) = csv(include_str!("../fixtures/peyton_manning.csv"));
        let r = c
            .call(
                "tools/call",
                serde_json::json!({
                    "name": "forecast",
                    "arguments": {"ds": ds, "y": y, "horizon": 365}
                }),
            )
            .await;
        assert!(
            r.get("error").is_none() && r["result"]["isError"] != true,
            "forecast failed: {r}"
        );
        let out = tool_output(&r);
        assert_eq!(out["ds"].as_array().expect("ds").len(), 365);
        assert_eq!(out["ds"][0], "2016-01-21");
        let yl = out["yhat_lower"].as_array().expect("yhat_lower");
        let yh = out["yhat"].as_array().expect("yhat");
        let yu = out["yhat_upper"].as_array().expect("yhat_upper");
        assert!(
            yl.iter()
                .zip(yh)
                .zip(yu)
                .all(|((l, h), u)| l.as_f64() <= h.as_f64() && h.as_f64() <= u.as_f64()),
            "band must bracket yhat"
        );
        assert!(
            out["components"].get("yearly").is_some() && out["components"].get("weekly").is_some(),
            "named components: {}",
            out["components"]
        );
        assert_eq!(out["trend"].as_array().expect("trend").len(), 365);
        assert!(
            out["fit_seconds"]
                .as_f64()
                .expect("fit_seconds")
                .is_finite()
                && out["predict_seconds"]
                    .as_f64()
                    .expect("predict_seconds")
                    .is_finite(),
            "timings are finite: {out}"
        );
        assert!(
            out["diagnostics"]["lbfgs"]["rounds"]
                .as_u64()
                .expect("lbfgs.rounds")
                >= 1,
            "the fit actually ran: {}",
            out["diagnostics"]
        );
    }
}
