//! Probe a live (or local) decide endpoint against a local copy of its artifact.
//!
//! ```text
//! cargo run -p aprender-mcp-decide-lambda --example probe -- \
//!     --url https://<endpoint>/ --apr model.apr [--expect-sha256 <hex>] \
//!     [--maximal concentrated|distributed] [--cold-first] [--probe-id <id>] [--bearer <token>]
//! ```
//!
//! `--apr` is the source of the expected sha256 and labels (computed locally, through
//! the same decide-apr-v1 ladder the server runs) and of the maximal requests, which are
//! sized with the artifact's own tokenizer under the CONTRACTED limits.
//!
//! Without `--cold-first`: `initialize`, `tools/list`, `tools/call` (identity), then the
//! maximal request if `--maximal` is given. With `--cold-first`: ONE `tools/call` as the
//! first and only POST (the maximal request if `--maximal`, else the identity text) —
//! plan 08-11's accepted-region sample. A sample counts as cold only if the server's
//! `x-decide-load` header says `cold` and its `decide.load performed_load=true` log line
//! carries the printed `probe_id`.
//!
//! Prints one JSON line per probe and exits non-zero on any false check or HTTP error.
//! The bearer token (`--bearer` or `APRENDER_DECIDE_PROBE_TOKEN`) is never printed.

#![allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally

use std::path::PathBuf;
use std::process::ExitCode;

use aprender_mcp_decide::ClassifyLimits;
use aprender_mcp_decide_lambda::parse_probe_id;
use aprender_mcp_decide_lambda::probe::{
    build_maximal_request, new_probe_id, run_cold_first, run_identity_probe, MaximalShape,
    IDENTITY_PROBE_TEXT,
};

const TOKEN_ENV: &str = "APRENDER_DECIDE_PROBE_TOKEN";

const USAGE: &str = "usage: probe --url <mcp url> --apr <local.apr> [--expect-sha256 <hex>] \
[--maximal concentrated|distributed] [--cold-first] [--probe-id <id>] [--bearer <token>]

  --url            the MCP endpoint (JSON-RPC POST target)
  --apr            local copy of the served artifact: expected sha256, labels, maximal sizing
  --expect-sha256  expected identity (must agree with --apr when both are given)
  --maximal        send the maximal legal request of this shape (CONTRACTED limits)
  --cold-first     send ONE tools/call as the first and only POST (no initialize)
  --probe-id       id sent as x-decide-probe-id ([A-Za-z0-9-], <= 64); generated if absent
  --bearer         bearer token (else env APRENDER_DECIDE_PROBE_TOKEN); never printed";

struct Args {
    url: String,
    apr: PathBuf,
    expect_sha256: Option<String>,
    maximal: Option<MaximalShape>,
    cold_first: bool,
    probe_id: Option<String>,
    bearer: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut url = None;
    let mut apr = None;
    let mut expect_sha256 = None;
    let mut maximal = None;
    let mut cold_first = false;
    let mut probe_id = None;
    let mut bearer = None;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--url" => url = Some(value("--url")?),
            "--apr" => apr = Some(PathBuf::from(value("--apr")?)),
            "--expect-sha256" => expect_sha256 = Some(value("--expect-sha256")?),
            "--maximal" => maximal = Some(value("--maximal")?.parse::<MaximalShape>()?),
            "--cold-first" => cold_first = true,
            "--probe-id" => probe_id = Some(value("--probe-id")?),
            "--bearer" => bearer = Some(value("--bearer")?),
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Args {
        url: url.ok_or("--url is required")?,
        apr: apr.ok_or("--apr is required")?,
        expect_sha256,
        maximal,
        cold_first,
        probe_id,
        bearer: bearer.or_else(|| std::env::var(TOKEN_ENV).ok().filter(|t| !t.is_empty())),
    })
}

fn fail(message: &str) -> ExitCode {
    eprintln!("probe: {message}");
    ExitCode::FAILURE
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) if e.is_empty() => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("probe: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    let model = match aprender_mcp_decide::load_model_from_path(&args.apr) {
        Ok(m) => m,
        Err(e) => return fail(&format!("cannot load --apr: {e}")),
    };
    let local_sha = model.identity().artifact_sha256.clone();
    let expected = match &args.expect_sha256 {
        Some(given) if *given != local_sha => {
            return fail(&format!(
                "--expect-sha256 {given} disagrees with --apr, which hashes to {local_sha}"
            ))
        }
        Some(given) => given.clone(),
        None => local_sha,
    };
    let labels = model.task().owned_labels();
    let probe_id = match &args.probe_id {
        Some(id) => match parse_probe_id(Some(id)) {
            Some(ok) => ok.to_string(),
            None => return fail("--probe-id must be 1..=64 [A-Za-z0-9-] characters"),
        },
        None => new_probe_id(),
    };
    eprintln!("probe: probe_id={probe_id}");
    let bearer = args.bearer.as_deref();

    let (texts, planned_tokens, shape) = match args.maximal {
        Some(shape) => match build_maximal_request(&model, &ClassifyLimits::CONTRACTED, shape) {
            Ok((texts, total)) => (texts, Some(total), Some(shape)),
            Err(e) => return fail(&format!("cannot build the maximal request: {e}")),
        },
        None => (vec![IDENTITY_PROBE_TEXT.to_string()], None, None),
    };
    let shape_name = shape.map_or("identity", MaximalShape::as_str);

    let mut ok = true;
    if !args.cold_first {
        match run_identity_probe(&args.url, bearer, &expected, &labels, &probe_id).await {
            Ok(report) => {
                ok &= report.ok();
                let elapsed_ms: Vec<u128> = report.calls.iter().map(|c| c.elapsed_ms).collect();
                let load_header = report.calls.iter().find_map(|c| c.load_header.clone());
                println!(
                    "{}",
                    serde_json::json!({
                        "probe": "identity",
                        "probe_id": report.probe_id,
                        "one_tool_named_classify": report.one_tool_named_classify,
                        "description_has_labels_in_order": report.description_has_labels_in_order,
                        "identity_matches": report.identity_matches,
                        "artifact_sha256": report.artifact_sha256,
                        "elapsed_ms": elapsed_ms,
                        "tokens_total": report.tokens_total,
                        "shape": "identity",
                        "load_header": load_header,
                    })
                );
            }
            Err(e) => return fail(&format!("identity probe failed: {e}")),
        }
        if shape.is_none() {
            return if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
    }

    match run_cold_first(&args.url, bearer, &texts, &probe_id).await {
        Ok(sample) => {
            let identity_matches = sample.artifact_sha256 == expected;
            let tokens_match = planned_tokens.is_none_or(|t| t == sample.tokens_total);
            ok &= identity_matches && tokens_match;
            println!(
                "{}",
                serde_json::json!({
                    "probe": if args.cold_first { "cold_first" } else { "maximal" },
                    "probe_id": sample.probe_id,
                    "identity_matches": identity_matches,
                    "description_has_labels_in_order": serde_json::Value::Null,
                    "elapsed_ms": sample.elapsed_ms,
                    "tokens_total": sample.tokens_total,
                    "tokens_planned": planned_tokens,
                    "texts": sample.texts,
                    "truncated": sample.truncated,
                    "shape": shape_name,
                    "load_header": sample.load_header,
                })
            );
        }
        Err(e) => return fail(&format!("{shape_name} call failed: {e}")),
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
