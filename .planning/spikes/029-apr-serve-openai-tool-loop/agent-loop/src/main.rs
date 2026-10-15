//! Spike 029 — drive pmcp.run's SDK agent loop (`pmcp-agent`) against `apr serve`.
//!
//! This is the production `OpenAiCompatSource` + `AgentEngine`, composed by path. The only
//! spike-local parts are a `ToolInvoker` that advertises and executes two MCP-shaped tools,
//! and the seeding of the user task into the conversation store (the engine starts from the
//! store's history, exactly as a hosted run does).
//!
//!   cargo run --release -- --base http://127.0.0.1:8129/v1 --model Qwen3.5-0.8B-Q4_K_M \
//!       --task "What's the weather in Paris right now?" --out ../results/agent-run.json
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

/// Two MCP-shaped tools, executed locally so the loop's second turn is deterministic.
struct LocalTools;

#[async_trait]
impl ToolInvoker for LocalTools {
    async fn invoke(&self, call: ToolCall) -> ToolCallResult {
        eprintln!("[tool] {} {}", call.name, call.arguments);
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
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
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
            _ => panic!("unknown arg {k}"),
        }
    }
    a
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args();

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

    // The production OpenAI-compatible source. The SDK default timeout is 60 s — raised here,
    // because a CPU-served 4B model takes longer than that per turn (a finding in itself).
    let source = OpenAiCompatSource::with_options(
        args.base.clone(),
        args.model.clone(),
        SecretString::new("spike-029-not-a-secret"),
        HttpSourceOptions {
            timeout: Duration::from_secs(args.timeout_s),
            allow_insecure_http: true,
            ..HttpSourceOptions::default()
        },
    )
    .map_err(|e| anyhow::anyhow!("build source: {e}"))?;

    // Seed the user task into the store: the engine starts from the stored history.
    let store = InMemoryStore::new();
    let mut state = RunState::new();
    state.history.push(SamplingMessage::new(
        Role::User,
        SamplingMessageContent::Text { text: args.task.clone(), meta: None },
    ));
    store
        .save("spike-029", &state)
        .await
        .map_err(|e| anyhow::anyhow!("seed store: {e}"))?;

    let engine = AgentEngine::new(source, LocalTools, store, config);
    let mut trace = DecisionTrace::default();
    let t0 = Instant::now();
    let outcome = engine.run_traced("spike-029", &mut trace).await;
    let elapsed = t0.elapsed();

    let (tag, detail): (&str, Value) = match &outcome {
        RunOutcome::Completed { result } => ("Completed", json!(format!("{result:?}"))),
        RunOutcome::LimitReached => ("LimitReached", Value::Null),
        RunOutcome::RetryRequired { class } => ("RetryRequired", json!(format!("{class:?}"))),
        RunOutcome::Failed { error } => ("Failed", json!(error)),
        other => ("Other", json!(format!("{other:?}"))),
    };
    println!("outcome: {tag}");
    println!("elapsed: {:.2}s", elapsed.as_secs_f64());
    println!("trace: {trace:#?}");
    if let RunOutcome::Completed { result } = &outcome {
        println!("final: {result:#?}");
    }
    if let Some(out) = &args.out {
        let record = json!({
            "base": args.base, "model": args.model, "task": args.task,
            "outcome": tag, "detail": detail, "elapsed_s": elapsed.as_secs_f64(),
            "trace_debug": format!("{trace:?}"),
        });
        std::fs::write(out, serde_json::to_string_pretty(&record)?)?;
        eprintln!("wrote {out}");
    }
    Ok(())
}
