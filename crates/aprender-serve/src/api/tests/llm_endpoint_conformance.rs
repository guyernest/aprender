//! `contracts/llm-endpoint-openai-conformance-v1.yaml` — the model-free half of the claims gate.
//!
//! pmcp.run talks to `apr serve` through two OpenAI-compatible clients (durable-agent-lambda's
//! `openai_v1` and pmcp-agent's `OpenAiCompatSource`). This module holds
//!
//! 1. the PARSE tests: both clients' request bodies, read from source and from the spike-029 wire
//!    capture, deserialize into the server's own [`ChatCompletionRequest`];
//! 2. the READER: `check_record` / `check_results_dir`, which read every probe record
//!    (`conformance-*.json`) under `deploy/llm-endpoint/probe/results/` and refuse one whose
//!    verdicts contradict its declared profile. Nothing this phase claims about conformance is
//!    licensed except by those records and this reader.
//!
//! The record format is the "probe-record contract" stated in
//! `deploy/llm-endpoint/probe/README.md`; the reader implements it verbatim.

#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use super::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------
// The probe-record contract (stated once in the README; mirrored here as constants)
// ---------------------------------------------------------------------------------------------

/// `label` values a record may declare.
const LABELS: [&str; 5] = [
    "baseline",
    "release-local",
    "interim",
    "deployed",
    "deployed-https",
];

/// `profile` values a record may declare.
const PROFILES: [&str; 2] = ["baseline", "fixed"];

const OK: &str = "\u{2713}";
const CROSS: &str = "\u{2717}";
const INFO: &str = "\u{2139}";
const KNOWN_LIMITATION: &str = "KNOWN LIMITATION";

const P_TOOL_RESULT: &str = "tool result -> final answer";
const P_REQUIRED: &str = "tool_choice=required";
const P_MAX_COMPLETION: &str = "max_completion_tokens honoured";
const P_ENVELOPE: &str = "error envelope {error:{message,type}}";
const P_FOREIGN_MODEL: &str = "foreign model id";
const P_NONE: &str = "tool_choice=none";
const P_NO_AUTH: &str = "no Authorization header";
const P_CPU_PART: &str = "cpu_part in /health";

/// The four probes the spike-029 baseline reads ✗ on, and nothing else.
const BASELINE_CROSSES: [&str; 4] = [P_TOOL_RESULT, P_REQUIRED, P_MAX_COMPLETION, P_ENVELOPE];

/// The eight probes a `fixed`-profile record must carry.
const REQUIRED_FIXED: [&str; 8] = [
    P_TOOL_RESULT,
    P_REQUIRED,
    P_MAX_COMPLETION,
    P_ENVELOPE,
    P_FOREIGN_MODEL,
    P_NONE,
    P_NO_AUTH,
    P_CPU_PART,
];

/// The five D-05 gaps that stay open (limitations, never fixes); `cpu_part in /health` is one:
/// the entrypoint `[entry]` log line carries `cpu_part`, not `/health`.
const LIMITATIONS_ALLOWED: [&str; 5] =
    [P_REQUIRED, P_ENVELOPE, P_FOREIGN_MODEL, P_NONE, P_CPU_PART];

/// The two fixes that may be absent from the pinned release (D-03 content, D-05 alias). They may
/// read KNOWN LIMITATION only with a matching `unreleased_at_pin` entry.
const UNRELEASED_ELIGIBLE: [&str; 2] = [P_TOOL_RESULT, P_MAX_COMPLETION];

/// The words an `unreleased_at_pin` reason must contain.
const UNRELEASED_PHRASE: &str = "unreleased at pin";

fn results_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("deploy")
        .join("llm-endpoint")
        .join("probe")
        .join("results")
}

// ---------------------------------------------------------------------------------------------
// The reader
// ---------------------------------------------------------------------------------------------

fn str_field<'a>(rec: &'a Value, key: &str) -> Result<&'a str, String> {
    rec.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or non-string header key `{key}`"))
}

fn verdict_of<'a>(probes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    probes
        .iter()
        .find(|(p, _)| p == name)
        .map(|(_, v)| v.as_str())
}

/// `apr <digits>...` — a release version line, as `apr --version` prints it.
fn is_release_server_version(v: &str) -> bool {
    v.strip_prefix("apr ")
        .is_some_and(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
}

/// A server version that names a non-release build (a git sha, a `+` suffix, `interim`).
fn is_non_release_server_version(v: &str) -> bool {
    v.contains('+') || v.contains("-g") || v.contains("interim")
}

fn check_server_version(label: &str, server_version: &str) -> Result<(), String> {
    match label {
        "interim" if is_non_release_server_version(server_version) => Ok(()),
        "interim" => Err(format!(
            "interim record's server_version `{server_version}` is a bare release string; \
             an interim build carries a non-release git sha"
        )),
        "release-local" | "deployed" | "deployed-https" if is_release_server_version(server_version) => {
            Ok(())
        }
        "release-local" | "deployed" | "deployed-https" => Err(format!(
            "{label} record's server_version `{server_version}` does not start with `apr <tag digits>`"
        )),
        // `baseline`: as transcribed from the spike.
        _ => Ok(()),
    }
}

/// Read `probes[]` into `(name, verdict)` pairs, refusing a malformed or duplicate entry.
fn read_probes(rec: &Value) -> Result<Vec<(String, String)>, String> {
    let arr = rec
        .get("probes")
        .and_then(Value::as_array)
        .ok_or("missing or non-array header key `probes`")?;
    let mut out: Vec<(String, String)> = Vec::with_capacity(arr.len());
    for (i, p) in arr.iter().enumerate() {
        let name = p
            .get("probe")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("probes[{i}] has no string `probe`"))?;
        let verdict = p
            .get("verdict")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("probes[{i}] (`{name}`) has no string `verdict`"))?;
        if p.get("status").is_none() {
            return Err(format!("probes[{i}] (`{name}`) has no `status`"));
        }
        if out.iter().any(|(n, _)| n == name) {
            return Err(format!(
                "duplicate probe name `{name}` (a repeat could hide a {CROSS})"
            ));
        }
        out.push((name.to_string(), verdict.to_string()));
    }
    Ok(out)
}

fn read_limitations(rec: &Value) -> Result<Vec<String>, String> {
    rec.get("known_limitations")
        .and_then(Value::as_array)
        .ok_or("missing or non-array header key `known_limitations`")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| "known_limitations holds a non-string entry".to_string())
        })
        .collect()
}

/// `unreleased_at_pin` is OPTIONAL (absent = empty); read as `(probe, reason)` pairs.
fn read_unreleased(rec: &Value) -> Result<Vec<(String, String)>, String> {
    let Some(v) = rec.get("unreleased_at_pin") else {
        return Ok(Vec::new());
    };
    v.as_array()
        .ok_or("`unreleased_at_pin` is not an array")?
        .iter()
        .map(|e| {
            let probe = e
                .get("probe")
                .and_then(Value::as_str)
                .ok_or("an unreleased_at_pin entry has no string `probe`")?;
            let reason = e
                .get("reason")
                .and_then(Value::as_str)
                .ok_or("an unreleased_at_pin entry has no string `reason`")?;
            Ok((probe.to_string(), reason.to_string()))
        })
        .collect()
}

fn check_baseline(
    probes: &[(String, String)],
    unreleased: &[(String, String)],
) -> Result<(), String> {
    if !unreleased.is_empty() {
        return Err(
            "a baseline record carries unreleased_at_pin entries; the baseline is the \
                    immutable spike-029 transcription"
                .to_string(),
        );
    }
    let mut crosses: Vec<&str> = probes
        .iter()
        .filter(|(_, v)| v == CROSS)
        .map(|(p, _)| p.as_str())
        .collect();
    crosses.sort_unstable();
    let mut want: Vec<&str> = BASELINE_CROSSES.to_vec();
    want.sort_unstable();
    if crosses == want {
        Ok(())
    } else {
        Err(format!(
            "baseline profile: the {CROSS} set must equal the spike-029 gap set {want:?}, got {crosses:?}"
        ))
    }
}

fn check_fixed(
    probes: &[(String, String)],
    limitations: &[String],
    unreleased: &[(String, String)],
) -> Result<(), String> {
    if let Some((p, _)) = probes.iter().find(|(_, v)| v == CROSS) {
        return Err(format!("fixed profile: probe `{p}` reads {CROSS}"));
    }
    for required in REQUIRED_FIXED {
        if verdict_of(probes, required).is_none() {
            return Err(format!(
                "fixed profile: required probe `{required}` is absent"
            ));
        }
    }
    // D-04: the auth gate is the one fix that gates the ingress; it is never a limitation.
    match verdict_of(probes, P_NO_AUTH) {
        Some(OK) => {},
        other => {
            return Err(format!(
            "fixed profile: `{P_NO_AUTH}` must read {OK} (the D-04 gate is never a limitation), \
                 got {other:?}"
        ))
        },
    }
    // cpu_part: the server reports it, or the gap is recorded as a limitation. Never absent, never a cross.
    match verdict_of(probes, P_CPU_PART) {
        Some(OK) => {},
        Some(KNOWN_LIMITATION) => {
            if !limitations.iter().any(|l| l == P_CPU_PART) {
                return Err(format!(
                    "fixed profile: `{P_CPU_PART}` reads {KNOWN_LIMITATION} but is not named in known_limitations"
                ));
            }
        },
        other => {
            return Err(format!(
                "fixed profile: `{P_CPU_PART}` must read {OK} or {KNOWN_LIMITATION}, got {other:?}"
            ))
        },
    }
    // Every named limitation reads KNOWN LIMITATION and is an allowed name.
    for name in limitations {
        let eligible = UNRELEASED_ELIGIBLE.contains(&name.as_str());
        if !LIMITATIONS_ALLOWED.contains(&name.as_str()) && !eligible {
            return Err(format!(
                "fixed profile: `{name}` is not an allowed known limitation"
            ));
        }
        if verdict_of(probes, name) != Some(KNOWN_LIMITATION) {
            return Err(format!(
                "fixed profile: `{name}` is listed in known_limitations but its verdict is not {KNOWN_LIMITATION}"
            ));
        }
    }
    // Conversely: a KNOWN LIMITATION verdict is only legitimate for a listed, allowed name.
    for (name, verdict) in probes {
        if verdict != KNOWN_LIMITATION {
            continue;
        }
        if !limitations.iter().any(|l| l == name) {
            return Err(format!(
                "fixed profile: `{name}` reads {KNOWN_LIMITATION} but is not named in known_limitations"
            ));
        }
    }
    check_unreleased(probes, limitations, unreleased)
}

/// The `unreleased_at_pin` rules, both directions (plan-check iteration 1, D-05).
fn check_unreleased(
    probes: &[(String, String)],
    limitations: &[String],
    unreleased: &[(String, String)],
) -> Result<(), String> {
    for (probe, reason) in unreleased {
        if !UNRELEASED_ELIGIBLE.contains(&probe.as_str()) {
            return Err(format!(
                "unreleased_at_pin names `{probe}`, which is not one of {UNRELEASED_ELIGIBLE:?}"
            ));
        }
        if !reason.contains(UNRELEASED_PHRASE) {
            return Err(format!(
                "unreleased_at_pin reason for `{probe}` lacks the words `{UNRELEASED_PHRASE}`"
            ));
        }
        if verdict_of(probes, probe) != Some(KNOWN_LIMITATION) {
            return Err(format!(
                "unreleased_at_pin names `{probe}` but its verdict is not {KNOWN_LIMITATION} \
                 (an entry beside a passing probe is a contradiction)"
            ));
        }
        if !limitations.iter().any(|l| l == probe) {
            return Err(format!(
                "unreleased_at_pin names `{probe}` but known_limitations does not list it"
            ));
        }
    }
    for eligible in UNRELEASED_ELIGIBLE {
        let limited = verdict_of(probes, eligible) == Some(KNOWN_LIMITATION);
        let explained = unreleased.iter().any(|(p, _)| p == eligible);
        if limited && !explained {
            return Err(format!(
                "`{eligible}` reads {KNOWN_LIMITATION} with no unreleased_at_pin entry; an eligible \
                 fix may be a limitation only when the record says it is unreleased at the pin"
            ));
        }
    }
    Ok(())
}

/// Check one probe record against the probe-record contract.
fn check_record(rec: &Value) -> Result<(), String> {
    let label = str_field(rec, "label")?;
    let profile = str_field(rec, "profile")?;
    str_field(rec, "pin")?;
    let server_version = str_field(rec, "server_version")?;
    if !LABELS.contains(&label) {
        return Err(format!("label `{label}` is not one of {LABELS:?}"));
    }
    if !PROFILES.contains(&profile) {
        return Err(format!("profile `{profile}` is not one of {PROFILES:?}"));
    }
    check_server_version(label, server_version)?;
    let probes = read_probes(rec)?;
    let limitations = read_limitations(rec)?;
    let unreleased = read_unreleased(rec)?;
    match profile {
        "baseline" => check_baseline(&probes, &unreleased),
        _ => check_fixed(&probes, &limitations, &unreleased),
    }
}

/// Check every `conformance-*.json` directly in `dir` (non-recursive). A directory with no
/// match FAILS: a reader with nothing to read is theater. Everything else in the directory
/// (`auth-table-*.md`, `agent-run-*.json`, `platform-turn-evidence.json`, `UNEXPECTED-*.json`)
/// is outside the glob and is never opened. Returns the number of records checked.
fn check_results_dir(dir: &Path) -> Result<usize, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("read_dir entry: {e}"))?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_file() && name.starts_with("conformance-") && name.ends_with(".json") {
            files.push(path);
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(format!(
            "no conformance-*.json record under {} (a reader with nothing to read is theater)",
            dir.display()
        ));
    }
    for path in &files {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let rec: Value =
            serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))?;
        check_record(&rec).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(files.len())
}

// ---------------------------------------------------------------------------------------------
// In-memory record builders (mutated by the negative tests)
// ---------------------------------------------------------------------------------------------

fn probe_entry(name: &str, verdict: &str) -> Value {
    json!({"probe": name, "verdict": verdict, "status": 200})
}

/// A valid `fixed`/`release-local` record: the D-04 gate is up, the D-03 and D-05-alias fixes
/// are released (✓), the five D-05 gaps are KNOWN LIMITATION.
fn fixed_record() -> Value {
    json!({
        "label": "release-local",
        "profile": "fixed",
        "pin": "v0.71.4",
        "server_version": "apr 0.71.4 (abcdef123)",
        "probes": [
            probe_entry("GET /models", OK),
            probe_entry("chat plain", OK),
            probe_entry(P_TOOL_RESULT, OK),
            probe_entry(P_REQUIRED, KNOWN_LIMITATION),
            probe_entry(P_MAX_COMPLETION, OK),
            probe_entry(P_ENVELOPE, KNOWN_LIMITATION),
            probe_entry(P_FOREIGN_MODEL, KNOWN_LIMITATION),
            probe_entry(P_NONE, KNOWN_LIMITATION),
            probe_entry(P_NO_AUTH, OK),
            probe_entry(P_CPU_PART, KNOWN_LIMITATION),
        ],
        "known_limitations": [P_REQUIRED, P_ENVELOPE, P_FOREIGN_MODEL, P_NONE, P_CPU_PART],
    })
}

fn set_verdict(rec: &mut Value, probe: &str, verdict: &str) {
    let probes = rec["probes"].as_array_mut().expect("probes array");
    let entry = probes
        .iter_mut()
        .find(|p| p["probe"] == probe)
        .unwrap_or_else(|| panic!("probe `{probe}` not in the record"));
    entry["verdict"] = json!(verdict);
}

fn remove_probe(rec: &mut Value, probe: &str) {
    rec["probes"]
        .as_array_mut()
        .expect("probes array")
        .retain(|p| p["probe"] != probe);
}

fn set_limitations(rec: &mut Value, names: &[&str]) {
    rec["known_limitations"] = json!(names);
}

/// `fixed_record` with both eligible fixes unreleased at the pin, as a record on a release that
/// carries only the auth gate would read.
fn fixed_record_unreleased() -> Value {
    let mut rec = fixed_record();
    set_verdict(&mut rec, P_TOOL_RESULT, KNOWN_LIMITATION);
    set_verdict(&mut rec, P_MAX_COMPLETION, KNOWN_LIMITATION);
    set_limitations(
        &mut rec,
        &[
            P_REQUIRED,
            P_ENVELOPE,
            P_FOREIGN_MODEL,
            P_NONE,
            P_CPU_PART,
            P_TOOL_RESULT,
            P_MAX_COMPLETION,
        ],
    );
    rec["unreleased_at_pin"] = json!([
        {"probe": P_TOOL_RESULT, "reason": "unreleased at pin v0.71.4: merged upstream, not in a tag"},
        {"probe": P_MAX_COMPLETION, "reason": "unreleased at pin v0.71.4: PR #1 still open"},
    ]);
    rec
}

fn baseline_record() -> Value {
    let path = results_dir().join("conformance-v0.70.3-spike029-baseline.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("the committed baseline record is JSON")
}

// ---------------------------------------------------------------------------------------------
// The client request shapes (IR-1)
// ---------------------------------------------------------------------------------------------

/// durable-agent-lambda `openai_v1` (openai.rs:21-60): `model, messages, max_tokens, top_p,
/// stream, tools`; no `temperature`; an assistant tool-use turn is written with `content: ""`.
#[test]
fn platform_openai_v1_request_shape_parses() {
    let body = json!({
        "model": "Qwen3.5-0.8B-Q4_K_M",
        "messages": [
            {"role": "system", "content": "You are an operations assistant."},
            {"role": "user", "content": "What's the weather in Paris right now?"},
            {"role": "assistant", "content": "", "tool_calls": [{
                "id": "call_0", "type": "function",
                "function": {"name": "get_weather", "arguments": "{\"city\":\"Paris\"}"}
            }]},
            {"role": "tool", "tool_call_id": "call_0",
             "content": "{\"city\":\"Paris\",\"temperature_c\":17}"}
        ],
        "max_tokens": 384,
        "top_p": 1.0,
        "stream": false,
        "tools": [{"type": "function", "function": {
            "name": "get_weather",
            "description": "Get the current weather for a city.",
            "parameters": {"type": "object",
                           "properties": {"city": {"type": "string"}},
                           "required": ["city"]}
        }}]
    });
    let req: ChatCompletionRequest =
        serde_json::from_value(body).expect("the platform request shape parses");
    assert_eq!(req.messages.len(), 4);
    assert_eq!(
        req.messages[2].content, "",
        "the assistant tool turn keeps content \"\""
    );
    assert_eq!(req.messages[2].tool_calls.as_ref().map(Vec::len), Some(1));
    assert_eq!(req.messages[3].tool_call_id.as_deref(), Some("call_0"));
    assert_eq!(req.max_tokens, Some(384));
    assert!(
        req.temperature.is_none(),
        "the platform sends no temperature"
    );
    assert_eq!(req.tools.as_ref().map(Vec::len), Some(1));

    // Round trip: the empty content is serialized, not dropped.
    let back = serde_json::to_value(&req.messages[2]).expect("serialize");
    assert_eq!(back["content"], json!(""));
    assert_eq!(
        back["tool_calls"][0]["function"]["arguments"],
        json!("{\"city\":\"Paris\"}")
    );
}

/// pmcp-agent `OpenAiCompatSource` turn 1 — wire request #2 of `wire-0.8b-plain.jsonl` (the
/// `"n": 2` line): system + user only, every message carrying a `content` key; `model, messages,
/// max_tokens, tools`; no `tool_choice`, no `temperature`. Deliberately NOT the turn-2 body
/// (`"n": 3`, an assistant message with no `content` key), which cannot parse until 09-02 lands.
#[test]
fn pmcp_agent_request_shape_parses() {
    let body = json!({
        "model": "Qwen3.5-0.8B-Q4_K_M",
        "messages": [
            {"role": "system", "content": "You are a concise, helpful assistant. Use tools when helpful."},
            {"role": "user", "content": "What's the weather in Paris right now?"}
        ],
        "max_tokens": 384,
        "tools": [
            {"type": "function", "function": {
                "name": "get_weather",
                "parameters": {"type": "object",
                               "properties": {"city": {"type": "string", "description": "City name"},
                                              "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}},
                               "required": ["city"]},
                "description": "Get the current weather for a city."}},
            {"type": "function", "function": {
                "name": "add",
                "parameters": {"type": "object",
                               "properties": {"a": {"type": "number"}, "b": {"type": "number"}},
                               "required": ["a", "b"]},
                "description": "Add two numbers and return the sum."}}
        ]
    });
    let req: ChatCompletionRequest =
        serde_json::from_value(body).expect("the pmcp-agent turn-1 request shape parses");
    assert_eq!(req.max_tokens, Some(384));
    assert_eq!(req.messages.len(), 2);
    assert_eq!(req.tools.as_ref().map(Vec::len), Some(2));
    assert!(req.tool_choice.is_none(), "pmcp-agent sends no tool_choice");
    assert!(
        req.temperature.is_none(),
        "pmcp-agent sends no temperature on the real wire"
    );
}

/// `arguments` is a JSON STRING on the wire, and a tool call finishes with `tool_calls`.
#[test]
fn tool_call_arguments_serialize_as_string() {
    let call = ResponseToolCall {
        id: "call_0".to_string(),
        call_type: "function".to_string(),
        function: ResponseFunctionCall {
            name: "get_weather".to_string(),
            arguments: "{\"city\":\"Paris\"}".to_string(),
        },
    };
    let v = serde_json::to_value(&call).expect("serialize");
    assert!(
        v["function"]["arguments"].is_string(),
        "arguments must be a JSON string, got {v}"
    );
    assert_eq!(v["type"], json!("function"));
    let back: ResponseToolCall = serde_json::from_value(v).expect("round trip");
    assert_eq!(back, call);
}

// ---------------------------------------------------------------------------------------------
// The reader
// ---------------------------------------------------------------------------------------------

/// FALSIFY-LLM-CONF-007: every committed record matches its declared profile.
#[test]
fn conformance_record_matches_contract() {
    let n = check_results_dir(&results_dir()).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        n >= 1,
        "the spike-029 baseline record is committed, so at least one record is read"
    );
    // The committed baseline is itself a record the reader accepts.
    check_record(&baseline_record()).unwrap_or_else(|e| panic!("{e}"));
    // Positive controls for the in-memory builders the negative tests mutate.
    check_record(&fixed_record()).unwrap_or_else(|e| panic!("{e}"));
    check_record(&fixed_record_unreleased()).unwrap_or_else(|e| panic!("{e}"));
}

/// The gate can fail (Verification Discipline 4): a `fixed` record with one cross is refused.
#[test]
fn reader_refuses_a_fixed_record_with_a_cross() {
    let mut rec = fixed_record();
    set_verdict(&mut rec, "chat plain", CROSS);
    let err = check_record(&rec).expect_err("a fixed record with a cross must be refused");
    assert!(err.contains("chat plain"), "{err}");
    // A baseline whose cross set drifted is refused too.
    let mut base = baseline_record();
    set_verdict(&mut base, "chat plain", CROSS);
    assert!(check_record(&base).is_err());
    // And a baseline that lost one of the four crosses.
    let mut base = baseline_record();
    set_verdict(&mut base, P_REQUIRED, OK);
    assert!(check_record(&base).is_err());
    // Info rows never count as a cross.
    let mut rec = fixed_record();
    set_verdict(&mut rec, "chat plain", INFO);
    check_record(&rec).expect("an info row is not a cross");
}

/// The input is the contract glob `conformance-*.json`, not `*.json`: a malformed sibling is
/// never opened, and zero matches fails rather than skips.
#[test]
fn reader_glob_skips_non_conformance_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path();
    std::fs::write(dir.join("agent-run-matrix.json"), "{\"cells\": []}").expect("write sibling");
    std::fs::write(dir.join("auth-table-x.md"), "| not | json |").expect("write sibling");
    std::fs::write(dir.join("UNEXPECTED-x.json"), "not json at all").expect("write sibling");
    std::fs::write(dir.join("platform-turn-evidence.json"), "[]").expect("write sibling");
    let record =
        std::fs::read_to_string(results_dir().join("conformance-v0.70.3-spike029-baseline.json"))
            .expect("read the baseline record");
    let target = dir.join("conformance-x-baseline.json");
    std::fs::write(&target, record).expect("write the conformance copy");
    assert_eq!(
        check_results_dir(dir),
        Ok(1),
        "the malformed siblings were never opened — the glob is the contract"
    );
    std::fs::remove_file(&target).expect("remove the conformance file");
    let err = check_results_dir(dir).expect_err("zero matches must fail, never skip");
    assert!(err.contains("no conformance-*.json"), "{err}");
}

/// Probe names are unique in every profile: a duplicate could hide a cross.
#[test]
fn reader_refuses_duplicate_probe_names() {
    let good = baseline_record();
    check_record(&good).expect("positive control: the baseline is valid");
    let mut dup = good.clone();
    dup["probes"]
        .as_array_mut()
        .expect("probes array")
        .push(probe_entry("chat plain", CROSS));
    let err = check_record(&dup).expect_err("a repeated probe name must be refused");
    assert!(
        err.contains("duplicate") && err.contains("chat plain"),
        "{err}"
    );

    let mut dup_fixed = fixed_record();
    dup_fixed["probes"]
        .as_array_mut()
        .expect("probes array")
        .push(probe_entry(P_NO_AUTH, CROSS));
    assert!(check_record(&dup_fixed)
        .expect_err("a repeated probe in a fixed record")
        .contains("duplicate"));
}

/// Contract row 012: the fixed profile requires `cpu_part in /health` to read ✓ or
/// KNOWN LIMITATION (named in `known_limitations`), never ✗ or absent.
#[test]
fn fixed_profile_cpu_part_limitation_is_enforced() {
    // Ok: KNOWN LIMITATION with the name listed.
    let rec = fixed_record();
    check_record(&rec).expect("cpu_part as a listed KNOWN LIMITATION is valid");
    // Ok again: the server reports cpu_part (✓); the name leaves the limitation list.
    let mut ok = fixed_record();
    set_verdict(&mut ok, P_CPU_PART, OK);
    set_limitations(&mut ok, &[P_REQUIRED, P_ENVELOPE, P_FOREIGN_MODEL, P_NONE]);
    check_record(&ok).expect("cpu_part reading ✓ is valid");

    // Refused: the probe is absent.
    let mut absent = fixed_record();
    remove_probe(&mut absent, P_CPU_PART);
    set_limitations(
        &mut absent,
        &[P_REQUIRED, P_ENVELOPE, P_FOREIGN_MODEL, P_NONE],
    );
    let err = check_record(&absent).expect_err("an absent cpu_part probe must be refused");
    assert!(err.contains(P_CPU_PART), "{err}");

    // Refused: the verdict is a cross.
    let mut cross = fixed_record();
    set_verdict(&mut cross, P_CPU_PART, CROSS);
    set_limitations(
        &mut cross,
        &[P_REQUIRED, P_ENVELOPE, P_FOREIGN_MODEL, P_NONE],
    );
    assert!(
        check_record(&cross).is_err(),
        "a cross on cpu_part must be refused"
    );

    // Refused: KNOWN LIMITATION, but the name is missing from known_limitations.
    let mut unnamed = fixed_record();
    set_limitations(
        &mut unnamed,
        &[P_REQUIRED, P_ENVELOPE, P_FOREIGN_MODEL, P_NONE],
    );
    let err = check_record(&unnamed).expect_err("an unlisted cpu_part limitation must be refused");
    assert!(err.contains(P_CPU_PART), "{err}");
}

/// D-05: none of the remaining 029 fixes may gate the end-to-end turn. The D-04 auth gate must
/// read ✓; the D-03 content fix and the D-05 alias may be KNOWN LIMITATION only with an explicit
/// `unreleased_at_pin` reason.
#[test]
fn fixed_profile_unreleased_fix_requires_explicit_reason() {
    // Ok: both eligible fixes unreleased, each with its entry.
    check_record(&fixed_record_unreleased()).expect("a fixed record with both fixes unreleased");

    // Refused: the same record without the entries.
    let mut no_entries = fixed_record_unreleased();
    no_entries
        .as_object_mut()
        .expect("object")
        .remove("unreleased_at_pin");
    let err = check_record(&no_entries).expect_err("a limitation with no entry must be refused");
    assert!(err.contains("unreleased_at_pin"), "{err}");

    // Refused: an entry whose reason lacks the words.
    let mut bad_reason = fixed_record_unreleased();
    bad_reason["unreleased_at_pin"][0]["reason"] = json!("not released yet");
    let err = check_record(&bad_reason).expect_err("a reason without the phrase must be refused");
    assert!(err.contains(UNRELEASED_PHRASE), "{err}");

    // Refused: an entry beside a ✓ verdict.
    let mut beside_ok = fixed_record_unreleased();
    set_verdict(&mut beside_ok, P_TOOL_RESULT, OK);
    set_limitations(
        &mut beside_ok,
        &[
            P_REQUIRED,
            P_ENVELOPE,
            P_FOREIGN_MODEL,
            P_NONE,
            P_CPU_PART,
            P_MAX_COMPLETION,
        ],
    );
    assert!(
        check_record(&beside_ok).is_err(),
        "an entry beside a passing probe is a contradiction"
    );

    // Refused: an entry naming a probe outside the eligible pair.
    let mut outside = fixed_record_unreleased();
    outside["unreleased_at_pin"]
        .as_array_mut()
        .expect("array")
        .push(json!({"probe": P_REQUIRED, "reason": "unreleased at pin v0.71.4: nope"}));
    let err =
        check_record(&outside).expect_err("an entry outside the eligible pair must be refused");
    assert!(err.contains(P_REQUIRED), "{err}");

    // Refused: the D-04 gate recorded as a limitation, and as a cross. It can never be waved through.
    let mut auth_limited = fixed_record_unreleased();
    set_verdict(&mut auth_limited, P_NO_AUTH, KNOWN_LIMITATION);
    let mut limits = auth_limited["known_limitations"].clone();
    limits.as_array_mut().expect("array").push(json!(P_NO_AUTH));
    auth_limited["known_limitations"] = limits;
    assert!(
        check_record(&auth_limited).is_err(),
        "the auth gate is never a limitation"
    );
    let mut auth_cross = fixed_record_unreleased();
    set_verdict(&mut auth_cross, P_NO_AUTH, CROSS);
    assert!(
        check_record(&auth_cross).is_err(),
        "an open server cannot be recorded as fixed"
    );
    let mut auth_info = fixed_record_unreleased();
    set_verdict(&mut auth_info, P_NO_AUTH, INFO);
    assert!(
        check_record(&auth_info).is_err(),
        "informational is the baseline reading, not a fixed one"
    );
}

/// The label and `server_version` rules of the probe-record contract.
#[test]
fn reader_label_and_server_version_rules() {
    // interim: a bare release string is refused; a non-release build is accepted.
    let mut interim = fixed_record();
    interim["label"] = json!("interim");
    interim["pin"] = json!("upstream-main+PRs");
    interim["server_version"] = json!("apr 0.71.4");
    assert!(
        check_record(&interim).is_err(),
        "interim with a bare release version"
    );
    interim["server_version"] = json!("apr 0.71.5-g1a2b3c4 (upstream-main+PRs)");
    check_record(&interim).expect("interim with a non-release build");

    // release-local / deployed / deployed-https: must start `apr <digits>`.
    for label in ["release-local", "deployed", "deployed-https"] {
        let mut rec = fixed_record();
        rec["label"] = json!(label);
        check_record(&rec).unwrap_or_else(|e| panic!("{label}: {e}"));
        rec["server_version"] = json!("some other server");
        assert!(
            check_record(&rec).is_err(),
            "{label} with a foreign server_version"
        );
    }

    // An unknown label or profile, or a missing header key, is refused.
    let mut bad_label = fixed_record();
    bad_label["label"] = json!("nightly");
    assert!(check_record(&bad_label).is_err());
    let mut bad_profile = fixed_record();
    bad_profile["profile"] = json!("strict");
    assert!(check_record(&bad_profile).is_err());
    let mut missing = fixed_record();
    missing
        .as_object_mut()
        .expect("object")
        .remove("known_limitations");
    assert!(check_record(&missing).is_err());
}
