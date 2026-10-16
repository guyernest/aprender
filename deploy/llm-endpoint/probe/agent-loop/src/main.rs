//! Agent-loop matrix driver (moved in-repo from spike 029): pmcp.run's SDK agent loop
//! (`pmcp-agent`) against an OpenAI-compatible endpoint such as `apr serve`.
//!
//! This is the production `OpenAiCompatSource` + `AgentEngine`, composed by path. The only
//! probe-local parts are a `ToolInvoker` that advertises and executes two MCP-shaped tools, and the
//! seeding of the user task into the conversation store (the engine starts from the store's
//! history, exactly as a hosted run does).
//!
//! The bearer key is read from `APR_PROBE_KEY` and the driver FAILS CLOSED without it. The spike's
//! predictable literal is usable only behind `--allow-test-key` AND a loopback `--base`
//! (127.0.0.1, localhost, [::1]); it can therefore never reach a real host.
//!
//!   APR_PROBE_KEY=... cargo run --release -- --base http://127.0.0.1:8030/v1 \
//!       --model Qwen3.5-0.8B-Q4_K_M --task "What's the weather in Paris right now?" \
//!       --package "$PWD/spike029/agent.package.json" --out /tmp/raw.json
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use async_trait::async_trait;
use pmcp::types::sampling::{SamplingMessage, SamplingMessageContent};
use pmcp::types::tools::ToolInfo;
use pmcp::types::Role;
use pmcp_agent::sources::{HttpSourceOptions, OpenAiCompatSource, SecretString};
use pmcp_agent::{
    resolve_agent, AgentEngine, ConversationStore, DecisionTrace, EnvVarResolver, InMemoryStore,
    RunOutcome, RunState, ToolCall, ToolCallResult, ToolInvoker,
};
use pmcp_package::AgentPackage;
use serde_json::{json, Value};

/// The spike's predictable literal. Never sent unless `--allow-test-key` AND a loopback `--base`.
const TEST_KEY_LITERAL: &str = "spike-029-not-a-secret";

/// Two MCP-shaped tools, executed locally so the loop's second turn is deterministic. The names of
/// the calls the loop actually made are recorded for the matrix record.
struct LocalTools {
    calls: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl ToolInvoker for LocalTools {
    async fn invoke(&self, call: ToolCall) -> ToolCallResult {
        eprintln!("[tool] {} {}", call.name, call.arguments);
        if let Ok(mut g) = self.calls.lock() {
            g.push(call.name.clone());
        }
        match call.name.as_str() {
            "get_weather" => {
                let city = call.arguments["city"].as_str().unwrap_or("?").to_string();
                ToolCallResult::ok(
                    call.id,
                    json!({"city": city, "temperature_c": 17, "condition": "light rain"}),
                )
            },
            "add" => {
                let a = call.arguments["a"].as_f64().unwrap_or(f64::NAN);
                let b = call.arguments["b"].as_f64().unwrap_or(f64::NAN);
                ToolCallResult::ok(call.id, json!({"sum": a + b}))
            },
            other => ToolCallResult::error(call.id, format!("unknown tool {other}")),
        }
    }

    async fn list_tools(&self) -> Vec<ToolInfo> {
        vec![
            ToolInfo::new(
                "get_weather",
                Some("Get the current weather for a city.".to_string()),
                json!({"type": "object",
                       "properties": {"city": {"type": "string", "description": "City name"},
                                      "unit": {"type": "string", "enum": ["celsius", "fahrenheit"]}},
                       "required": ["city"]}),
            ),
            ToolInfo::new(
                "add",
                Some("Add two numbers and return the sum.".to_string()),
                json!({"type": "object",
                       "properties": {"a": {"type": "number"}, "b": {"type": "number"}},
                       "required": ["a", "b"]}),
            ),
        ]
    }
}

struct Args {
    base: String,
    model: String,
    task: String,
    out: Option<String>,
    timeout_s: u64,
    max_tokens: u32,
    max_iterations: u32,
    package: String,
    /// `None` means "default": true for a loopback base, false otherwise.
    insecure_http: Option<bool>,
    allow_test_key: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        base: "http://127.0.0.1:8029/v1".into(),
        model: "Qwen3.5-0.8B-Q4_K_M".into(),
        task: "What's the weather in Paris right now?".into(),
        out: None,
        timeout_s: 900,
        max_tokens: 384,
        max_iterations: 6,
        package: "spike029/agent.package.json".into(),
        insecure_http: None,
        allow_test_key: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        // A bare flag takes no value; every other flag consumes the next argument.
        if k == "--allow-test-key" {
            a.allow_test_key = true;
            continue;
        }
        let v = it.next().unwrap_or_default();
        match k.as_str() {
            "--base" => a.base = v,
            "--model" => a.model = v,
            "--task" => a.task = v,
            "--out" => a.out = Some(v),
            "--timeout-s" => a.timeout_s = v.parse().expect("--timeout-s u64"),
            "--max-tokens" => a.max_tokens = v.parse().expect("--max-tokens u32"),
            "--max-iterations" => a.max_iterations = v.parse().expect("--max-iterations u32"),
            "--package" => a.package = v,
            "--insecure-http" => {
                a.insecure_http = Some(match v.as_str() {
                    "true" => true,
                    "false" => false,
                    other => {
                        eprintln!("--insecure-http takes true|false, got {other:?}");
                        std::process::exit(2);
                    },
                });
            },
            _ => panic!("unknown arg {k}"),
        }
    }
    a
}

/// The host part of a base URL, lower-cased, with brackets kept for IPv6. A URL carrying userinfo
/// (`user@host`) has no trustworthy host here, so it yields `None` and is treated as non-loopback.
fn base_host(base: &str) -> Option<String> {
    let rest = base.split_once("://").map_or(base, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        let end = stripped.find(']')?;
        format!("[{}]", &stripped[..end])
    } else {
        authority.split(':').next().unwrap_or("").to_string()
    };
    Some(host.to_ascii_lowercase())
}

/// Exact match only: `127.0.0.1.evil.example` and `localhost.evil.example` are not loopback.
fn is_loopback_base(base: &str) -> bool {
    matches!(
        base_host(base).as_deref(),
        Some("127.0.0.1") | Some("localhost") | Some("[::1]")
    )
}

/// Resolve the bearer key, failing closed. Exits 2 with a `REFUSED key` line on every refusal.
fn resolve_key(args: &Args) -> String {
    let loopback = is_loopback_base(&args.base);
    if args.allow_test_key && !loopback {
        eprintln!("REFUSED key: --allow-test-key is restricted to a loopback --base");
        std::process::exit(2);
    }
    match std::env::var("APR_PROBE_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ if args.allow_test_key => TEST_KEY_LITERAL.to_string(),
        _ => {
            eprintln!(
                "REFUSED key: APR_PROBE_KEY is not set (--allow-test-key is only for a loopback --base)"
            );
            std::process::exit(2);
        },
    }
}

/// Concatenate the text blocks of the final assistant turn.
fn final_text(result: &pmcp_agent::IterationResult) -> String {
    result
        .assistant_message
        .content
        .iter()
        .filter_map(|c| match c {
            SamplingMessageContent::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args();
    // The key gate runs first: nothing else (package read, network) happens without it.
    let key = resolve_key(&args);
    let insecure_http = args
        .insecure_http
        .unwrap_or_else(|| is_loopback_base(&args.base));

    // The scaffolded AgentPackage, resolved the way `cargo pmcp agent dev` resolves it.
    let pkg: AgentPackage = serde_json::from_str(
        &std::fs::read_to_string(&args.package).with_context(|| format!("read {}", args.package))?,
    )
    .context("parse AgentPackage")?;
    let mut config = resolve_agent(&pkg, &EnvVarResolver::new())
        .await
        .context("resolve agent package")?;
    config.max_tokens = args.max_tokens; // per-turn output cap; the scaffold's 100000 is a CPU footgun
    config.max_iterations = args.max_iterations;

    // The production OpenAI-compatible source. The SDK default timeout is 60 s: raised here,
    // because a CPU-served model takes longer than that per turn.
    let source = OpenAiCompatSource::with_options(
        args.base.clone(),
        args.model.clone(),
        SecretString::new(key),
        HttpSourceOptions {
            timeout: Duration::from_secs(args.timeout_s),
            allow_insecure_http: insecure_http,
            ..HttpSourceOptions::default()
        },
    )
    .map_err(|e| anyhow::anyhow!("build source: {e}"))?;

    // Seed the user task into the store: the engine starts from the stored history.
    let store = InMemoryStore::new();
    let mut state = RunState::new();
    state.history.push(SamplingMessage::new(
        Role::User,
        SamplingMessageContent::Text {
            text: args.task.clone(),
            meta: None,
        },
    ));
    store
        .save("agent-loop", &state)
        .await
        .map_err(|e| anyhow::anyhow!("seed store: {e}"))?;

    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let engine = AgentEngine::new(
        source,
        LocalTools {
            calls: Arc::clone(&calls),
        },
        store,
        config,
    );
    let mut trace = DecisionTrace::default();
    let t0 = Instant::now();
    let outcome = engine.run_traced("agent-loop", &mut trace).await;
    let elapsed = t0.elapsed();

    let (tag, detail, text): (&str, Value, String) = match &outcome {
        RunOutcome::Completed { result } => (
            "Completed",
            json!(format!("{result:?}")),
            final_text(result),
        ),
        RunOutcome::LimitReached => ("LimitReached", Value::Null, String::new()),
        RunOutcome::RetryRequired { class } => {
            ("RetryRequired", json!(format!("{class:?}")), String::new())
        },
        RunOutcome::Failed { error } => ("Failed", json!(error), String::new()),
        other => ("Other", json!(format!("{other:?}")), String::new()),
    };
    println!("outcome: {tag}");
    println!("elapsed: {:.2}s", elapsed.as_secs_f64());
    println!("trace: {trace:#?}");
    if let RunOutcome::Completed { result } = &outcome {
        println!("final: {result:#?}");
    }
    if let Some(out) = &args.out {
        let tool_calls: Vec<String> = calls.lock().map(|g| g.clone()).unwrap_or_default();
        // The key is never written: only the base, model and task identify the run.
        let record = json!({
            "base": args.base, "model": args.model, "task": args.task,
            "outcome": tag, "detail": detail, "elapsed_s": elapsed.as_secs_f64(),
            "iterations": trace.steps.len(),
            "tool_calls": tool_calls,
            "final_text": text,
            "trace_debug": format!("{trace:?}"),
        });
        std::fs::write(out, serde_json::to_string_pretty(&record)?)?;
        eprintln!("wrote {out}");
    }
    Ok(())
}
