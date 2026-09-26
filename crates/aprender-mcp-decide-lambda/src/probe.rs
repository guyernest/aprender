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
