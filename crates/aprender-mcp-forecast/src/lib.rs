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
    use aprender_forecast::dates::{days_from_civil, format_ymd};

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

    // ---------------------------------------------------------------------------------
    // The D-11 refusal set (SC1), driven through a LIVE streamable-HTTP server.
    //
    // One `#[tokio::test]` per case on purpose: a failing case names ONE cause. Each case
    // perturbs `valid_args()` by exactly one field, so nothing else can be the reason.
    // ---------------------------------------------------------------------------------

    /// The JSON-RPC error code pmcp 2.19.3 emits for a refusal, READ OFF a live reply
    /// (not from the SDK docs) and pinned here.
    ///
    /// MEASURED, AND NOT WHAT IT LOOKS LIKE. This is `-32603`, the JSON-RPC *internal
    /// error* code. `pmcp::Error::error_code()` returns `None` for BOTH `Validation` and
    /// `Internal` (only the `Protocol` variant carries an explicit code), so the transport
    /// falls back to `-32603` for either. The numeric code alone therefore cannot tell a
    /// caller's fault from a server fault. The discriminator pmcp actually ships is the
    /// message PREFIX `thiserror` renders from the variant — [`VALIDATION_PREFIX`] below
    /// versus `"Internal error: "` — so both are pinned and a pmcp upgrade that changes
    /// either turns this suite red instead of silently reclassifying every refusal.
    const REFUSAL_CODE: i64 = -32603;

    /// The class marker that separates "you sent something I cannot use" from "I broke".
    /// `map_error` routes `ForecastError::Validation` here and `Internal` to the other
    /// prefix; an `"Internal error: "` on any case below is a defect, not a wording choice.
    const VALIDATION_PREFIX: &str = "Validation error: ";

    /// Bring up a fresh in-process server and complete `initialize`.
    async fn serve() -> Client {
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
        c
    }

    /// The Peyton Manning daily series (2 905 points, ends 2016-01-20).
    fn peyton() -> (Vec<String>, Vec<f64>) {
        csv(include_str!("../fixtures/peyton_manning.csv"))
    }

    /// The Air Passengers MONTHLY series (144 points, 1949-01 .. 1960-12).
    fn air() -> (Vec<String>, Vec<f64>) {
        csv(include_str!("../fixtures/air_passengers.csv"))
    }

    /// A synthetic daily series: trend + a weekly term, strictly ascending from 2020-01-01.
    ///
    /// The refusal cases use this rather than Peyton so each request is small — every
    /// refusal fires at the door, before any design is built, so the series never matters
    /// beyond being valid in the dimension the case is not perturbing.
    fn synth(n: usize) -> (Vec<String>, Vec<f64>) {
        let t0 = days_from_civil(2020, 1, 1);
        let mut ds = Vec::with_capacity(n);
        let mut y = Vec::with_capacity(n);
        for i in 0..n {
            ds.push(format_ymd(t0 + i as i64));
            let t = i as f64;
            y.push(10.0 + 0.05 * t + (2.0 * std::f64::consts::PI * t / 7.0).sin());
        }
        (ds, y)
    }

    /// Arguments the door ACCEPTS. Every refusal case below perturbs exactly one field.
    fn valid_args() -> serde_json::Value {
        let (ds, y) = synth(60);
        serde_json::json!({"ds": ds, "y": y, "horizon": 7})
    }

    /// Assert the server REFUSED, that the refusal is validation-class, and that its
    /// message names the fix.
    async fn refused(c: &mut Client, args: serde_json::Value, needle: &str) {
        let reply = c
            .call(
                "tools/call",
                serde_json::json!({"name": "forecast", "arguments": args}),
            )
            .await;
        let is_error =
            reply.get("error").is_some() || reply["result"]["isError"] == serde_json::json!(true);
        assert!(is_error, "must be REFUSED, never defaulted; got: {reply}");
        assert!(
            reply.to_string().contains(needle),
            "the refusal must name the fix ({needle:?}); got: {reply}"
        );
        // Validation-class, not internal: the caller's fault stays the caller's fault.
        let message = if let Some(err) = reply.get("error") {
            assert_eq!(
                err["code"].as_i64(),
                Some(REFUSAL_CODE),
                "pinned refusal code; got: {reply}"
            );
            err["message"].as_str().expect("error.message").to_string()
        } else {
            reply["result"]["content"][0]["text"]
                .as_str()
                .expect("isError reply carries text content")
                .to_string()
        };
        assert!(
            message.starts_with(VALIDATION_PREFIX),
            "a caller-fixable input must reach the client as {VALIDATION_PREFIX:?}, \
             never as \"Internal error: \"; got: {message}"
        );
    }

    /// Merge `extra` into the valid argument object (one perturbation per case).
    fn with(extra: serde_json::Value) -> serde_json::Value {
        let mut args = valid_args();
        let obj = args.as_object_mut().expect("args object");
        for (k, v) in extra.as_object().expect("extra object") {
            obj.insert(k.clone(), v.clone());
        }
        args
    }

    #[tokio::test]
    async fn refuses_unknown_field() {
        // deny_unknown_fields is structural: the key is named back to the caller.
        refused(
            &mut serve().await,
            with(serde_json::json!({"bogus": 1})),
            "bogus",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_length_mismatch() {
        let (ds, y) = synth(60);
        refused(
            &mut serve().await,
            serde_json::json!({"ds": ds, "y": &y[..59], "horizon": 7}),
            "ds has",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_too_few_points() {
        let (ds, y) = synth(5);
        refused(
            &mut serve().await,
            serde_json::json!({"ds": ds, "y": y, "horizon": 7}),
            "at least",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_too_many_points() {
        // MAX_POINTS + 1. This is the T-06-02 bound: it is what actually caps the work
        // one request can buy, so it is asserted over the wire, not just in a unit test.
        let (ds, y) = synth(20_001);
        refused(
            &mut serve().await,
            serde_json::json!({"ds": ds, "y": y, "horizon": 7}),
            "exceeds max_points",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_unsorted_ds() {
        let (mut ds, y) = synth(60);
        ds.swap(10, 20);
        refused(
            &mut serve().await,
            serde_json::json!({"ds": ds, "y": y, "horizon": 7}),
            "strictly ascending",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_duplicate_ds() {
        let (mut ds, y) = synth(60);
        ds[1] = ds[0].clone();
        refused(
            &mut serve().await,
            serde_json::json!({"ds": ds, "y": y, "horizon": 7}),
            "strictly ascending",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_impossible_date() {
        // Shape-valid, calendar-invalid: caught by the civil-date ROUND TRIP, so the
        // message is `not a calendar date` rather than the YYYY-MM-DD shape refusal.
        let (mut ds, y) = synth(60);
        ds[3] = "2008-02-30".into();
        refused(
            &mut serve().await,
            serde_json::json!({"ds": ds, "y": y, "horizon": 7}),
            "calendar",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_constant_y() {
        let (ds, _) = synth(60);
        let y = vec![5.0_f64; ds.len()];
        refused(
            &mut serve().await,
            serde_json::json!({"ds": ds, "y": y, "horizon": 7}),
            "is constant",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_horizon_zero() {
        refused(
            &mut serve().await,
            with(serde_json::json!({"horizon": 0})),
            "horizon must be",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_horizon_above_max() {
        // MAX_HORIZON + 1.
        refused(
            &mut serve().await,
            with(serde_json::json!({"horizon": 3651})),
            "horizon must be",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_unknown_freq() {
        // The D-11 case that matters most: a silent daily fallback for "H" would be a
        // WRONG answer, not a degraded one.
        refused(
            &mut serve().await,
            with(serde_json::json!({"freq": "H"})),
            "use D, W or MS",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_interval_width_out_of_range() {
        // The interval is OPEN at both ends: a 100% band is not a number this model
        // produces.
        refused(
            &mut serve().await,
            with(serde_json::json!({"interval_width": 1.0})),
            "interval_width must be in (0, 1)",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_logistic_without_cap() {
        refused(
            &mut serve().await,
            with(serde_json::json!({"growth": "logistic"})),
            "needs cap",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_logistic_cap_below_max_y() {
        // synth(60) reaches ~13; a cap of 1.0 is below the data it is supposed to bound.
        refused(
            &mut serve().await,
            with(serde_json::json!({"growth": "logistic", "cap": 1.0})),
            "must exceed max(y)",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_unknown_model() {
        refused(
            &mut serve().await,
            with(serde_json::json!({"model": "arima"})),
            "prophet or neuralprophet",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_unknown_seasonality_mode() {
        refused(
            &mut serve().await,
            with(serde_json::json!({"seasonality_mode": "quadratic"})),
            "additive or multiplicative",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_unknown_growth() {
        refused(
            &mut serve().await,
            with(serde_json::json!({"growth": "exponential"})),
            "linear, logistic or flat",
        )
        .await;
    }

    // The four malformed-date SHAPES (REVIEW-06-U2). The spike's `s.get(..10)` prefix take
    // accepted the first of these silently; each is now its own case at the wire.

    #[tokio::test]
    async fn refuses_date_with_trailing_content() {
        let (mut ds, y) = synth(60);
        // The exact string the ported prefix-take used to accept as 2008-02-01.
        ds[0] = "2008-02-01garbage".into();
        refused(
            &mut serve().await,
            serde_json::json!({"ds": ds, "y": y, "horizon": 7}),
            "YYYY-MM-DD",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_empty_date() {
        let (mut ds, y) = synth(60);
        ds[0] = String::new();
        refused(
            &mut serve().await,
            serde_json::json!({"ds": ds, "y": y, "horizon": 7}),
            "YYYY-MM-DD",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_date_with_whitespace() {
        let (mut ds, y) = synth(60);
        ds[0] = " 2008-02-01".into();
        refused(
            &mut serve().await,
            serde_json::json!({"ds": ds, "y": y, "horizon": 7}),
            "YYYY-MM-DD",
        )
        .await;
    }

    #[tokio::test]
    async fn refuses_non_ascii_date() {
        let (mut ds, y) = synth(60);
        // Full-width digits: ten CHARS, not ten BYTES, and not ASCII either way.
        ds[0] = "２００８-02-01".into();
        refused(
            &mut serve().await,
            serde_json::json!({"ds": ds, "y": y, "horizon": 7}),
            "YYYY-MM-DD",
        )
        .await;
    }

    // ---------------------------------------------------------------------------------
    // Happy paths: the SHARED response shape (D-03) across both models and across the
    // option combinations the refusal cases only ever exercise negatively.
    // ---------------------------------------------------------------------------------

    /// Assert the shared shape: horizon-length parallel arrays with a bracketing band.
    fn assert_shared_shape(out: &serde_json::Value, horizon: usize) {
        for key in ["ds", "yhat", "yhat_lower", "yhat_upper", "trend"] {
            assert_eq!(
                out[key]
                    .as_array()
                    .unwrap_or_else(|| panic!("{key}: {out}"))
                    .len(),
                horizon,
                "{key} must have one row per horizon step: {out}"
            );
        }
        let l = out["yhat_lower"].as_array().expect("yhat_lower");
        let h = out["yhat"].as_array().expect("yhat");
        let u = out["yhat_upper"].as_array().expect("yhat_upper");
        for i in 0..horizon {
            let (lo, mid, hi) = (
                l[i].as_f64().expect("lower"),
                h[i].as_f64().expect("yhat"),
                u[i].as_f64().expect("upper"),
            );
            assert!(mid.is_finite(), "row {i}: yhat must be finite");
            assert!(
                lo <= mid && mid <= hi,
                "row {i}: band must bracket yhat ({lo} <= {mid} <= {hi})"
            );
        }
        assert!(
            out["predict_seconds"]
                .as_f64()
                .expect("predict_seconds")
                .is_finite(),
            "timings must be finite: {out}"
        );
    }

    #[tokio::test]
    async fn neuralprophet_lag_free_happy_path() {
        // The arm plan 06-04 filled in. 06-01 shipped it as a REFUSING stub, so this is
        // the first e2e proof that `model: neuralprophet` dispatches over the wire.
        let mut c = serve().await;
        let (ds, y) = peyton();
        let r = c
            .call(
                "tools/call",
                serde_json::json!({
                    "name": "forecast",
                    "arguments": {"ds": ds, "y": y, "horizon": 30, "model": "neuralprophet"}
                }),
            )
            .await;
        assert!(
            r.get("error").is_none() && r["result"]["isError"] != true,
            "neuralprophet forecast failed: {r}"
        );
        let out = tool_output(&r);
        assert_eq!(out["model"], "neuralprophet");
        assert_shared_shape(&out, 30);
        assert!(
            out["diagnostics"]["selected_lr"].as_f64().is_some(),
            "the lr sweep must be reported: {}",
            out["diagnostics"]
        );
        assert!(
            out["diagnostics"]["band"]
                .as_str()
                .expect("diagnostics.band")
                .contains("residual"),
            "the band must SAY it is residual-sd based, not quantile regression: {}",
            out["diagnostics"]
        );
    }

    #[tokio::test]
    async fn prophet_ms_multiplicative_happy_path() {
        // Monthly data, month-start future grid, multiplicative seasonality — the three
        // knobs the refusal cases only ever set WRONG.
        let mut c = serve().await;
        let (ds, y) = air();
        let r = c
            .call(
                "tools/call",
                serde_json::json!({
                    "name": "forecast",
                    "arguments": {
                        "ds": ds, "y": y, "horizon": 12,
                        "freq": "MS", "seasonality_mode": "multiplicative"
                    }
                }),
            )
            .await;
        assert!(
            r.get("error").is_none() && r["result"]["isError"] != true,
            "MS + multiplicative forecast failed: {r}"
        );
        let out = tool_output(&r);
        assert_eq!(out["freq"], "MS");
        assert_shared_shape(&out, 12);
        assert!(
            out["components"].get("multiplicative_terms").is_some(),
            "multiplicative seasonality must produce multiplicative_terms: {}",
            out["components"]
        );
        for d in out["ds"].as_array().expect("ds") {
            assert!(
                d.as_str().expect("ds entry").ends_with("-01"),
                "MS steps land on the first of a month: {d}"
            );
        }
    }

    #[tokio::test]
    async fn prophet_logistic_with_holidays_happy_path() {
        // Logistic growth WITH a valid cap, plus a named holiday — the positive twin of
        // refuses_logistic_without_cap / refuses_logistic_cap_below_max_y.
        let mut c = serve().await;
        let (ds, y) = peyton();
        let cap = y.iter().fold(f64::NEG_INFINITY, |a, v| a.max(*v)) + 2.0;
        let r = c
            .call(
                "tools/call",
                serde_json::json!({
                    "name": "forecast",
                    "arguments": {
                        "ds": ds, "y": y, "horizon": 30,
                        "growth": "logistic", "cap": cap,
                        "holidays": [{
                            "name": "superbowl",
                            "dates": [
                                "2010-02-07", "2011-02-06", "2012-02-05", "2013-02-03",
                                "2014-02-02", "2015-02-01", "2016-02-07"
                            ],
                            "lower_window": 0,
                            "upper_window": 1
                        }]
                    }
                }),
            )
            .await;
        assert!(
            r.get("error").is_none() && r["result"]["isError"] != true,
            "logistic + holidays forecast failed: {r}"
        );
        let out = tool_output(&r);
        assert_shared_shape(&out, 30);
        assert!(
            out["components"].get("superbowl").is_some(),
            "a named holiday must become a named component: {}",
            out["components"]
        );
        assert_eq!(
            out["diagnostics"]["growth"], "Logistic",
            "the diagnostics must report the growth actually fitted: {}",
            out["diagnostics"]
        );
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally
mod tests {
    use aprender_forecast::ForecastArgs;

    /// The strictness D-11 relies on must be ADVERTISED, not merely enforced: a client
    /// reads `tools/list` and has to be able to see that an unknown key will be refused
    /// and that exactly three fields are required.
    #[test]
    fn the_tool_schema_is_strict_and_requires_ds_y_horizon() {
        let schema =
            serde_json::to_value(schemars::schema_for!(ForecastArgs)).expect("schema serializes");
        assert_eq!(
            schema["additionalProperties"],
            serde_json::json!(false),
            "deny_unknown_fields must surface as additionalProperties: false: {schema}"
        );
        let required: Vec<&str> = schema["required"]
            .as_array()
            .expect("required array")
            .iter()
            .map(|v| v.as_str().expect("required entry is a string"))
            .collect();
        let mut sorted = required.clone();
        sorted.sort_unstable();
        assert_eq!(
            sorted,
            vec!["ds", "horizon", "y"],
            "required must be EXACTLY ds, y, horizon; got {required:?}"
        );
        let props = schema["properties"].as_object().expect("properties object");
        for optional in ["freq", "model", "n_lags", "seed", "holidays"] {
            assert!(
                props.contains_key(optional),
                "{optional} must be advertised as an optional property: {schema}"
            );
        }
    }
}
