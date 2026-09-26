//! A JSON-RPC probe for a live (or loopback) decide endpoint.
//!
//! It proves three things a deploy must show (D-11): the endpoint advertises exactly
//! one `classify` tool, that tool's description names the artifact's labels in task
//! order, and a call answers with the served file's sha256 — the identity pinned in
//! the deploy config. [`run_cold_first`] sends the `tools/call` as the FIRST and only
//! POST, the shape a cold container behind the gateway receives when the client's
//! `initialize` landed elsewhere; the server's `x-decide-load` header and its
//! `decide.load` log line (keyed by the probe id) say whether that request was cold.
//!
//! Every request carries `x-decide-probe-id`. A bearer token, when used, is sent and
//! never printed or stored in a report (T-08-07-04).

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};

use crate::{LOAD_HEADER, PROBE_ID_HEADER};

/// The protocol version the probe sends (header and `initialize`): one pmcp's
/// stateless server supports.
pub const PROBE_PROTOCOL_VERSION: &str = pmcp::DEFAULT_PROTOCOL_VERSION;

/// The text the identity probe classifies (synthetic; never dataset text).
pub const IDENTITY_PROBE_TEXT: &str = "Identity probe: please route this message.";

/// How long one probe request may take (a cold start inside it included).
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Why a probe could not complete.
#[derive(Debug)]
pub enum ProbeError {
    /// Transport failure (connect, timeout, body).
    Http(String),
    /// Non-2xx status.
    Status {
        /// The JSON-RPC method.
        method: &'static str,
        /// HTTP status.
        status: u16,
        /// The response body (the server's error; never contains the token).
        body: String,
    },
    /// A JSON-RPC error, a tool error, or a body that is not the expected shape.
    Protocol {
        /// The JSON-RPC method.
        method: &'static str,
        /// What was wrong.
        detail: String,
    },
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "http: {e}"),
            Self::Status {
                method,
                status,
                body,
            } => write!(f, "{method}: HTTP {status}: {body}"),
            Self::Protocol { method, detail } => write!(f, "{method}: {detail}"),
        }
    }
}

impl std::error::Error for ProbeError {}

/// One timed JSON-RPC call.
#[derive(Debug, Clone, Serialize)]
pub struct CallTiming {
    /// The JSON-RPC method.
    pub method: &'static str,
    /// Wall time of the POST, response body included.
    pub elapsed_ms: u128,
    /// The `x-decide-load` header on the response, if any.
    pub load_header: Option<String>,
}

/// The identity probe's result.
#[derive(Debug, Clone, Serialize)]
pub struct ProbeReport {
    /// The probe id sent on every request.
    pub probe_id: String,
    /// `tools/list` returned exactly one tool, named `classify`.
    pub one_tool_named_classify: bool,
    /// The tool description names every expected label, first occurrences in order.
    pub description_has_labels_in_order: bool,
    /// The call's `model.artifact_sha256` equals the expected sha256.
    pub identity_matches: bool,
    /// The identity the endpoint reported.
    pub artifact_sha256: String,
    /// The labels the call response carried.
    pub labels: Vec<String>,
    /// Built-row tokens the call was charged.
    pub tokens_total: usize,
    /// `initialize`, `tools/list`, `tools/call`, in order.
    pub calls: Vec<CallTiming>,
}

impl ProbeReport {
    /// All three identity checks passed.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.one_tool_named_classify
            && self.description_has_labels_in_order
            && self.identity_matches
    }
}

/// The cold-first sample: one `tools/call` with no `initialize` before it.
#[derive(Debug, Clone, Serialize)]
pub struct ColdSample {
    /// The probe id sent.
    pub probe_id: String,
    /// Wall time of the single POST.
    pub elapsed_ms: u128,
    /// Built-row tokens over the request.
    pub tokens_total: usize,
    /// Texts sent.
    pub texts: usize,
    /// Results with `truncated: true`.
    pub truncated: usize,
    /// The identity the endpoint reported.
    pub artifact_sha256: String,
    /// The `x-decide-load` header (`cold;load_ms=<n>` or `warm`).
    pub load_header: Option<String>,
}

/// A fresh probe id: `probe-<time hex>-<pid hex>`, inside the server's accepted
/// `[A-Za-z0-9-]{1,64}` alphabet.
#[must_use]
pub fn new_probe_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("probe-{nanos:x}-{:x}", std::process::id())
}

/// A JSON-RPC client over MCP streamable HTTP.
struct Rpc<'a> {
    client: reqwest::Client,
    url: &'a str,
    bearer: Option<&'a str>,
    probe_id: &'a str,
    next_id: u64,
}

impl<'a> Rpc<'a> {
    fn new(url: &'a str, bearer: Option<&'a str>, probe_id: &'a str) -> Result<Self, ProbeError> {
        let mut builder = reqwest::Client::builder().timeout(REQUEST_TIMEOUT);
        if is_loopback(url) {
            builder = builder.no_proxy();
        }
        let client = builder
            .build()
            .map_err(|e| ProbeError::Http(e.to_string()))?;
        Ok(Self {
            client,
            url,
            bearer,
            probe_id,
            next_id: 1,
        })
    }

    async fn call(
        &mut self,
        method: &'static str,
        params: Value,
    ) -> Result<(Value, CallTiming), ProbeError> {
        let id = self.next_id;
        self.next_id += 1;
        let body = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let mut req = self
            .client
            .post(self.url)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", PROBE_PROTOCOL_VERSION)
            .header(PROBE_ID_HEADER, self.probe_id)
            .body(body.to_string());
        if let Some(token) = self.bearer {
            req = req.bearer_auth(token);
        }
        let started = Instant::now();
        let resp = req
            .send()
            .await
            .map_err(|e| ProbeError::Http(without_url(&e)))?;
        let status = resp.status();
        let load_header = resp
            .headers()
            .get(LOAD_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let text = resp
            .text()
            .await
            .map_err(|e| ProbeError::Http(without_url(&e)))?;
        let elapsed_ms = started.elapsed().as_millis();
        if !status.is_success() {
            return Err(ProbeError::Status {
                method,
                status: status.as_u16(),
                body: text,
            });
        }
        let value = parse_rpc_body(&content_type, &text)
            .map_err(|detail| ProbeError::Protocol { method, detail })?;
        if let Some(err) = value.get("error") {
            return Err(ProbeError::Protocol {
                method,
                detail: format!("JSON-RPC error: {err}"),
            });
        }
        let result = value.get("result").cloned().ok_or(ProbeError::Protocol {
            method,
            detail: "response has no result".to_string(),
        })?;
        Ok((
            result,
            CallTiming {
                method,
                elapsed_ms,
                load_header,
            },
        ))
    }
}

/// Describe a transport error WITHOUT its URL: the URL is the caller's own, but a
/// query-string token must never be echoed into a report or a log.
fn without_url(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else if e.is_body() || e.is_decode() {
        "body"
    } else {
        "request"
    };
    match e.status() {
        Some(s) => format!("{kind} error (status {s})"),
        None => format!("{kind} error"),
    }
}

fn is_loopback(url: &str) -> bool {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    rest.starts_with("127.0.0.1") || rest.starts_with("localhost") || rest.starts_with("[::1]")
}

/// Parse a JSON-RPC response body, JSON or a single-event SSE stream.
///
/// # Errors
///
/// A description when neither shape parses.
pub fn parse_rpc_body(content_type: &str, body: &str) -> Result<Value, String> {
    if content_type.starts_with("text/event-stream") {
        let data: String = body
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        return serde_json::from_str(&data).map_err(|e| format!("SSE data is not JSON: {e}"));
    }
    serde_json::from_str(body).map_err(|e| format!("body is not JSON: {e}"))
}

/// The classify payload out of a `tools/call` result (JSON text content).
///
/// # Errors
///
/// A description when the result is a tool error or not the expected shape.
pub fn classify_payload(result: &Value) -> Result<Value, String> {
    if result.get("isError") == Some(&Value::Bool(true)) {
        let text = result["content"][0]["text"].as_str().unwrap_or("");
        return Err(format!("tool error: {text}"));
    }
    let text = result["content"][0]["text"]
        .as_str()
        .ok_or("content[0].text is not a string")?;
    serde_json::from_str(text).map_err(|e| format!("classify text is not JSON: {e}"))
}

/// Every label appears in `description`, first occurrences in `labels` order.
#[must_use]
pub fn labels_in_order(description: &str, labels: &[String]) -> bool {
    let mut from = 0usize;
    for label in labels {
        match description[from..].find(label.as_str()) {
            Some(at) => from += at + label.len(),
            None => return false,
        }
    }
    !labels.is_empty()
}

fn classify_params(texts: &[String]) -> Value {
    json!({ "name": aprender_mcp_decide::TOOL_NAME, "arguments": { "texts": texts } })
}

fn tokens_and_truncated(payload: &Value) -> (usize, usize) {
    let results = payload["results"].as_array().map_or(&[][..], Vec::as_slice);
    let tokens = results
        .iter()
        .filter_map(|r| r["tokens"].as_u64())
        .map(|t| usize::try_from(t).unwrap_or(usize::MAX))
        .fold(0usize, usize::saturating_add);
    let truncated = results
        .iter()
        .filter(|r| r["truncated"].as_bool() == Some(true))
        .count();
    (tokens, truncated)
}

/// `initialize` -> `tools/list` -> `tools/call classify`, checking identity and labels.
///
/// # Errors
///
/// [`ProbeError`] for a transport failure, a non-2xx status or a malformed response.
/// A WRONG identity is not an error: it is `identity_matches: false` in the report.
pub async fn run_identity_probe(
    url: &str,
    bearer: Option<&str>,
    expected_sha256: &str,
    expected_labels: &[String],
    probe_id: &str,
) -> Result<ProbeReport, ProbeError> {
    let mut rpc = Rpc::new(url, bearer, probe_id)?;
    let (_, init) = rpc
        .call(
            "initialize",
            json!({
                "protocolVersion": PROBE_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "aprender-decide-probe", "version": env!("CARGO_PKG_VERSION") }
            }),
        )
        .await?;
    let (listed, list) = rpc.call("tools/list", json!({})).await?;
    let tools = listed["tools"].as_array().cloned().unwrap_or_default();
    let one_tool_named_classify =
        tools.len() == 1 && tools[0]["name"] == aprender_mcp_decide::TOOL_NAME;
    let description = tools
        .first()
        .and_then(|t| t["description"].as_str())
        .unwrap_or("");
    let description_has_labels_in_order = labels_in_order(description, expected_labels);

    let (result, call) = rpc
        .call(
            "tools/call",
            classify_params(&[IDENTITY_PROBE_TEXT.to_string()]),
        )
        .await?;
    let payload = classify_payload(&result).map_err(|detail| ProbeError::Protocol {
        method: "tools/call",
        detail,
    })?;
    let artifact_sha256 = payload["model"]["artifact_sha256"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let labels: Vec<String> = payload["labels"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| l.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let (tokens_total, _) = tokens_and_truncated(&payload);
    Ok(ProbeReport {
        probe_id: probe_id.to_string(),
        one_tool_named_classify,
        description_has_labels_in_order,
        identity_matches: !artifact_sha256.is_empty() && artifact_sha256 == expected_sha256,
        artifact_sha256,
        labels,
        tokens_total,
        calls: vec![init, list, call],
    })
}

/// Send ONE `tools/call classify` as the first and only POST — no `initialize`, no
/// `tools/list` — and time it.
///
/// # Errors
///
/// [`ProbeError`] for a transport failure, a non-2xx status, a tool error or a
/// malformed response.
pub async fn run_cold_first(
    url: &str,
    bearer: Option<&str>,
    texts: &[String],
    probe_id: &str,
) -> Result<ColdSample, ProbeError> {
    let mut rpc = Rpc::new(url, bearer, probe_id)?;
    let (result, call) = rpc.call("tools/call", classify_params(texts)).await?;
    let payload = classify_payload(&result).map_err(|detail| ProbeError::Protocol {
        method: "tools/call",
        detail,
    })?;
    let (tokens_total, truncated) = tokens_and_truncated(&payload);
    Ok(ColdSample {
        probe_id: probe_id.to_string(),
        elapsed_ms: call.elapsed_ms,
        tokens_total,
        texts: texts.len(),
        truncated,
        artifact_sha256: payload["model"]["artifact_sha256"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        load_header: call.load_header,
    })
}

// ===========================================================================
// The maximal legal request (decide-tool-boundary-v1 `accepted_region_cold`)
// ===========================================================================

/// Which maximal legal request to build. Attention cost grows with the square of row
/// length, so two full rows cost more than eight short ones of the same total; the
/// accepted region is claimed for BOTH shapes and neither is inferred from the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MaximalShape {
    /// The fewest texts that reach the token budget, each `max_text_bytes` long and
    /// truncated to a full row: the largest attention cost and tokenizer input.
    Concentrated,
    /// `max_texts` texts whose built rows total the budget: the largest per-row overhead.
    Distributed,
}

impl MaximalShape {
    /// The shape's CLI / report name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Concentrated => "concentrated",
            Self::Distributed => "distributed",
        }
    }
}

impl std::str::FromStr for MaximalShape {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "concentrated" => Ok(Self::Concentrated),
            "distributed" => Ok(Self::Distributed),
            other => Err(format!(
                "unknown shape {other:?}: use concentrated or distributed"
            )),
        }
    }
}

/// Why a maximal request could not be built.
#[derive(Debug)]
pub enum MaximalError {
    /// The model refused to tokenize the synthetic text.
    Prepare(aprender_decide::DecideError),
    /// The limits admit no request at all (e.g. `max_texts == 0`).
    EmptyLimits,
}

impl std::fmt::Display for MaximalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prepare(e) => write!(f, "the model refused the synthetic text: {e}"),
            Self::EmptyLimits => write!(f, "the limits admit no request"),
        }
    }
}

impl std::error::Error for MaximalError {}

/// One synthetic unit: plain ASCII (so any byte cut is a char boundary), no dataset text.
const UNIT: &str = "probe ";

/// A synthetic text of `units` repetitions of [`UNIT`], at most `max_bytes` long.
fn synthetic(units: usize, max_bytes: usize) -> String {
    let mut text = UNIT.repeat(units);
    text.truncate(max_bytes);
    text
}

/// A synthetic text exactly `bytes` long (the largest tokenizer input the byte bound
/// admits).
fn synthetic_exact(bytes: usize) -> String {
    let mut text = UNIT.repeat(bytes / UNIT.len() + 1);
    text.truncate(bytes);
    text
}

fn row_tokens(model: &crate::Model, text: &str) -> Result<(usize, bool), MaximalError> {
    let rows = model
        .prepare(&[text.to_string()])
        .map_err(MaximalError::Prepare)?;
    Ok(rows
        .first()
        .map_or((0, false), |r| (r.tokens(), r.truncated())))
}

/// The largest unit count whose built row is at most `target` tokens, and that row's
/// length. Row length is non-decreasing in the unit count, so this is a binary search.
fn largest_within(
    model: &crate::Model,
    target: usize,
    max_bytes: usize,
) -> Result<(usize, usize), MaximalError> {
    let (mut lo, mut lo_tokens) = (0usize, row_tokens(model, "")?.0);
    let mut hi = max_bytes / UNIT.len();
    if lo_tokens > target {
        return Ok((0, lo_tokens));
    }
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        let (tokens, _) = row_tokens(model, &synthetic(mid, max_bytes))?;
        if tokens <= target {
            lo = mid;
            lo_tokens = tokens;
        } else {
            hi = mid - 1;
        }
    }
    Ok((lo, lo_tokens))
}

/// Build the maximal LEGAL request of `shape` under `limits`, sized with the model's
/// own tokenizer and row builder (`prepare`). Returns the texts and their built-row
/// token total. Synthetic text only.
///
/// CONCENTRATED is `min(ceil(max_total_tokens / max_len), max_texts)` texts: all but
/// the last are `max_text_bytes` long and truncated to a full row; the last is too when
/// the budget is a whole number of rows, and otherwise is sized to the remainder, so the
/// total is the largest value not over the budget. For Laya-en (max_len 512, budget
/// 1024) that is 2 full rows. DISTRIBUTED is `max_texts` texts whose built rows total
/// the largest value not over the budget.
///
/// # Errors
///
/// [`MaximalError`] if the model refuses the synthetic text or the limits admit nothing.
pub fn build_maximal_request(
    model: &crate::Model,
    limits: &aprender_mcp_decide::ClassifyLimits,
    shape: MaximalShape,
) -> Result<(Vec<String>, usize), MaximalError> {
    if limits.max_texts == 0 || limits.max_total_tokens == 0 {
        return Err(MaximalError::EmptyLimits);
    }
    let budget = limits.max_total_tokens;
    let max_bytes = limits.max_text_bytes;
    let texts = match shape {
        MaximalShape::Concentrated => {
            let max_len = model.manifest().agent.max_len.max(1);
            let count = budget.div_ceil(max_len).min(limits.max_texts);
            let full = synthetic_exact(max_bytes);
            let (full_tokens, _) = row_tokens(model, &full)?;
            let mut texts = vec![full; count];
            let over = (full_tokens * count).saturating_sub(budget);
            if over > 0 {
                // The budget is not a whole number of rows: the last text fills only
                // the remainder.
                let room = full_tokens.saturating_sub(over);
                let (units, _) = largest_within(model, room, max_bytes)?;
                if let Some(last) = texts.last_mut() {
                    *last = synthetic(units, max_bytes);
                }
            }
            texts
        }
        MaximalShape::Distributed => {
            let count = limits.max_texts;
            let mut units = Vec::with_capacity(count);
            let mut tokens = Vec::with_capacity(count);
            for i in 0..count {
                let share = budget / count + usize::from(i < budget % count);
                let (u, t) = largest_within(model, share, max_bytes)?;
                units.push(u);
                tokens.push(t);
            }
            // Second pass: hand any slack (rows land a token or two under their share)
            // to the texts in order, never exceeding the budget.
            for i in 0..count {
                let others: usize = tokens.iter().sum::<usize>() - tokens[i];
                let room = budget.saturating_sub(others);
                let (u, t) = largest_within(model, room, max_bytes)?;
                if t <= room && t >= tokens[i] {
                    units[i] = u;
                    tokens[i] = t;
                }
            }
            units.iter().map(|&u| synthetic(u, max_bytes)).collect()
        }
    };
    let total = model
        .prepare(&texts)
        .map_err(MaximalError::Prepare)?
        .iter()
        .map(aprender_decide::PreparedRow::tokens)
        .sum();
    Ok((texts, total))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aprender_mcp_decide::{check_token_budget, precheck, ClassifyArgs, ClassifyLimits};

    use super::*;
    use crate::tests::{serve, tiny_bytes, tiny_model};

    /// Limits the tiny fixture (rows of at most 64 tokens) can reach: two full rows.
    const SHRUNK: ClassifyLimits = ClassifyLimits {
        max_texts: 4,
        max_text_bytes: 2_048,
        max_total_tokens: 128,
        ..ClassifyLimits::CONTRACTED
    };

    fn rows(model: &crate::Model, texts: &[String]) -> Vec<aprender_decide::PreparedRow> {
        model.prepare(texts).expect("prepare")
    }

    fn assert_legal(limits: &ClassifyLimits, texts: &[String], total: usize) {
        let args = ClassifyArgs {
            texts: texts.to_vec(),
        };
        precheck(limits, &args).expect("the maximal request passes precheck");
        let per_text: Vec<usize> = rows(&tiny_model(), texts)
            .iter()
            .map(aprender_decide::PreparedRow::tokens)
            .collect();
        assert_eq!(per_text.iter().sum::<usize>(), total);
        check_token_budget(limits, &per_text).expect("the maximal request fits the budget");
    }

    #[test]
    fn maximal_concentrated_is_full_rows_of_max_byte_texts() {
        let model = tiny_model();
        assert_eq!(model.manifest().agent.max_len, 64);
        let (texts, total) =
            build_maximal_request(&model, &SHRUNK, MaximalShape::Concentrated).expect("build");
        assert_eq!(texts.len(), 2, "ceil(128 / 64) texts");
        assert!(texts.iter().all(|t| t.len() == SHRUNK.max_text_bytes));
        assert!(rows(&model, &texts).iter().all(|r| r.truncated()));
        assert!(total <= SHRUNK.max_total_tokens && total + 8 >= SHRUNK.max_total_tokens);
        assert_legal(&SHRUNK, &texts, total);
    }

    #[test]
    fn maximal_concentrated_sizes_the_last_text_to_an_uneven_budget() {
        let model = tiny_model();
        let limits = ClassifyLimits {
            max_total_tokens: 100,
            ..SHRUNK
        };
        let (texts, total) =
            build_maximal_request(&model, &limits, MaximalShape::Concentrated).expect("build");
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0].len(), limits.max_text_bytes);
        assert!(texts[1].len() < limits.max_text_bytes);
        assert!(total <= 100 && total + 8 >= 100, "total {total}");
        assert_legal(&limits, &texts, total);
    }

    #[test]
    fn maximal_distributed_fills_max_texts_to_the_budget() {
        let model = tiny_model();
        let (texts, total) =
            build_maximal_request(&model, &SHRUNK, MaximalShape::Distributed).expect("build");
        assert_eq!(texts.len(), SHRUNK.max_texts);
        assert!(total <= SHRUNK.max_total_tokens && total + 8 >= SHRUNK.max_total_tokens);
        assert_legal(&SHRUNK, &texts, total);
    }

    #[test]
    fn shape_names_round_trip() {
        for shape in [MaximalShape::Concentrated, MaximalShape::Distributed] {
            assert_eq!(shape.as_str().parse::<MaximalShape>(), Ok(shape));
        }
        assert!("both".parse::<MaximalShape>().is_err());
    }

    #[test]
    fn labels_in_order_requires_first_occurrences_in_order() {
        let labels = ["a1".to_string(), "b2".to_string()];
        assert!(labels_in_order("x a1 y b2", &labels));
        assert!(!labels_in_order("x b2 y a1", &labels));
        assert!(!labels_in_order("x a1 y", &labels));
        assert!(!labels_in_order("anything", &[]));
    }

    #[test]
    fn rpc_body_parses_json_and_sse() {
        let json = parse_rpc_body("application/json", r#"{"id":1}"#).expect("json");
        assert_eq!(json["id"], 1);
        let sse = parse_rpc_body("text/event-stream", "event: message\ndata: {\"id\":2}\n\n")
            .expect("sse");
        assert_eq!(sse["id"], 2);
        assert!(parse_rpc_body("application/json", "not json").is_err());
    }

    /// Both maximal shapes, built under the SERVED (contracted) limits, are served as the
    /// first and only POST to a fresh stateless loopback server — the request plan 08-11
    /// times on cold Lambda instances.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn loopback_maximal_requests_are_served_cold_first() {
        let model = tiny_model();
        for shape in [MaximalShape::Concentrated, MaximalShape::Distributed] {
            let (texts, total) =
                build_maximal_request(&model, &ClassifyLimits::CONTRACTED, shape).expect("build");
            let url = serve(Arc::clone(&model)).await;
            let sample = run_cold_first(&url, None, &texts, &new_probe_id())
                .await
                .unwrap_or_else(|e| panic!("{} request refused: {e}", shape.as_str()));
            assert_eq!(sample.tokens_total, total, "{}", shape.as_str());
            assert_eq!(sample.texts, texts.len());
            assert_eq!(sample.artifact_sha256, crate::sha256_hex(tiny_bytes()));
        }
    }
}
