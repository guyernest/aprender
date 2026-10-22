//! Probe a live (or local) decide endpoint against a local copy of its artifact.
//!
//! ```text
//! cargo run -p aprender-mcp-decide-lambda --example probe -- \
//!     --url https://<endpoint>/ --apr model.apr [--expect-sha256 <hex>] \
//!     [--maximal concentrated|distributed] [--cold-first] [--probe-id <id>] [--bearer <token>]
//! cargo run -p aprender-mcp-decide-lambda --example probe -- \
//!     --plan-only --apr model.apr --maximal concentrated|distributed
//! cargo run -p aprender-mcp-decide-lambda --example probe -- \
//!     --url https://<endpoint>/ --apr model.apr --replay N [--replay-from K]
//! cargo run -p aprender-mcp-decide-lambda --example probe -- \
//!     --url https://<endpoint>/ --apr model.apr --replay-over-limit
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
//! `--plan-only` builds the maximal request from `--apr` and prints its size (texts, planned
//! tokens, identity) WITHOUT any network call — the offline half of `just laya-deploy-verify`'s
//! DRY_RUN, which proves both shapes can be built for the artifact before anything is deployed.
//! It also prints the compiled door (`max_texts`, `max_total_tokens`) and `min_row_tokens`, the
//! shortest built row, measured by the server's own build-time check, and exits non-zero when
//! the served task does not fit the door.
//!
//! `--replay N` is the acceptance replay (Phase 10 D-16): ONE bare `tools/call` of N committed
//! synthetic texts (N from 1 to the compiled `classify_max_texts`), checked for one result per
//! text, probabilities that index the labels in task order and sum to one, and the served sha256.
//! Timings are reported, never gated. A server refusal of a legal-count replay (the token budget
//! during a step-down) is a typed `refused` line and exit 3, not a transport failure.
//! `--replay-over-limit` sends the compiled maximum plus one text and passes only when the
//! server refuses it, naming the bound.
//!
//! Prints one JSON line per probe and exits non-zero on any false check or HTTP error (1), a
//! usage error (2) or a typed server refusal of a replay (3).
//! The bearer token (`--bearer` or `APRENDER_DECIDE_PROBE_TOKEN`) is never printed.

#![allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap() internally

use std::path::PathBuf;
use std::process::ExitCode;

use aprender_mcp_decide::ClassifyLimits;
use aprender_mcp_decide_lambda::parse_probe_id;
use aprender_mcp_decide_lambda::probe::{
    build_maximal_request, new_probe_id, refusal_bound, replay_texts_from, run_cold_first,
    run_count_refusal, run_identity_probe, run_replay, served_min_row_tokens, MaximalShape,
    ProbeError, IDENTITY_PROBE_TEXT, REPLAY_TEXTS,
};

const TOKEN_ENV: &str = "APRENDER_DECIDE_PROBE_TOKEN";

const USAGE: &str = "usage: probe --url <mcp url> --apr <local.apr> [--expect-sha256 <hex>] \
[--maximal concentrated|distributed] [--cold-first] [--probe-id <id>] [--bearer <token>]
       probe --url <mcp url> --apr <local.apr> --replay <N> [--replay-from <K>]
       probe --url <mcp url> --apr <local.apr> --replay-over-limit
       probe --plan-only --apr <local.apr> --maximal concentrated|distributed

  --url           the MCP endpoint (JSON-RPC POST target)
  --apr            local copy of the served artifact: expected sha256, labels, maximal sizing
  --expect-sha256  expected identity (must agree with --apr when both are given)
  --maximal        send the maximal legal request of this shape (CONTRACTED limits)
  --cold-first     send ONE tools/call as the first and only POST (no initialize)
  --probe-id       id sent as x-decide-probe-id ([A-Za-z0-9-], <= 64); generated if absent
  --bearer         bearer token (else env APRENDER_DECIDE_PROBE_TOKEN); never printed
  --plan-only      build the --maximal request and print its size, min_row_tokens and the compiled
                   door; no network, no --url; exits 1 when the task does not fit the door
  --replay         the acceptance replay: ONE call of N synthetic texts (N in 1..=the compiled
                   classify_max_texts); needs --url; a server refusal prints a `refused` line, exit 3
  --replay-from    start index into the cycled replay texts (default 0; needs --replay)
  --replay-over-limit  one call of classify_max_texts + 1 texts; passes only if the server refuses
                   it, naming the bound; needs --url";

struct Args {
    url: Option<String>,
    apr: PathBuf,
    expect_sha256: Option<String>,
    maximal: Option<MaximalShape>,
    cold_first: bool,
    probe_id: Option<String>,
    bearer: Option<String>,
    plan_only: bool,
    replay: Option<usize>,
    replay_from: usize,
    replay_over_limit: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut url = None;
    let mut apr = None;
    let mut expect_sha256 = None;
    let mut maximal = None;
    let mut cold_first = false;
    let mut probe_id = None;
    let mut bearer = None;
    let mut plan_only = false;
    let mut replay = None;
    let mut replay_from = None;
    let mut replay_over_limit = false;
    // `args_os`, not `args`: `std::env::args` PANICS (exit 101) on an argument that is not
    // valid UTF-8. Such an argument is a usage error (exit 2), reported by position only,
    // never quoted, because it may be the bearer token.
    let mut it = std::env::args_os().skip(1).enumerate().map(|(i, a)| {
        a.into_string()
            .map_err(|_| format!("argument {} is not valid UTF-8", i + 1))
    });
    while let Some(arg) = it.next() {
        let arg = arg?;
        let mut value = |name: &str| {
            it.next()
                .ok_or_else(|| format!("{name} needs a value"))
                .and_then(std::convert::identity)
        };
        match arg.as_str() {
            "--url" => url = Some(value("--url")?),
            "--apr" => apr = Some(PathBuf::from(value("--apr")?)),
            "--expect-sha256" => expect_sha256 = Some(value("--expect-sha256")?),
            "--maximal" => maximal = Some(value("--maximal")?.parse::<MaximalShape>()?),
            "--cold-first" => cold_first = true,
            "--probe-id" => probe_id = Some(value("--probe-id")?),
            "--bearer" => bearer = Some(value("--bearer")?),
            "--plan-only" => plan_only = true,
            "--replay" => {
                // The range is the COMPILED door, read from the constant: the only place it
                // is decided, so a lowered `classify_max_texts` moves it with no edit here.
                let max = ClassifyLimits::CONTRACTED.max_texts;
                let n = value("--replay")?
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=max).contains(n))
                    .ok_or_else(|| {
                        format!("--replay must be in 1..={max} (the compiled classify_max_texts)")
                    })?;
                replay = Some(n);
            }
            "--replay-from" => {
                let last = REPLAY_TEXTS.len() - 1;
                let k = value("--replay-from")?
                    .parse::<usize>()
                    .ok()
                    .filter(|k| *k <= last)
                    .ok_or_else(|| format!("--replay-from must be in 0..={last}"))?;
                replay_from = Some(k);
            }
            "--replay-over-limit" => replay_over_limit = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    if plan_only && maximal.is_none() {
        return Err("--plan-only needs --maximal".to_string());
    }
    if !plan_only && url.is_none() {
        return Err("--url is required".to_string());
    }
    if replay.is_some() && replay_over_limit {
        return Err("--replay and --replay-over-limit are separate legs".to_string());
    }
    if (replay.is_some() || replay_over_limit) && (cold_first || plan_only) {
        return Err(
            "--replay and --replay-over-limit conflict with --cold-first and --plan-only"
                .to_string(),
        );
    }
    if replay_from.is_some() && replay.is_none() {
        return Err("--replay-from needs --replay".to_string());
    }
    Ok(Args {
        url,
        apr: apr.ok_or("--apr is required")?,
        expect_sha256,
        maximal,
        cold_first,
        probe_id,
        bearer: bearer.or_else(|| std::env::var(TOKEN_ENV).ok().filter(|t| !t.is_empty())),
        plan_only,
        replay,
        replay_from: replay_from.unwrap_or(0),
        replay_over_limit,
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

    // The compiled door every mode reports and the replay range follows.
    let door = ClassifyLimits::CONTRACTED;

    // `tokens_planned` is the request TOTAL. The distributed shape fills each text to its
    // share of the budget, so it approaches `max_total_tokens`; it is NOT `max_texts x
    // min_row_tokens` (that product is only the floor). `min_row_tokens` is the shortest
    // built row, measured by the server's own build-time check (`served_min_row_tokens`
    // wraps `aprender_mcp_decide::check_served_task_fits`): a task that does not fit the door
    // is refused here, as the Lambda would refuse to start.
    if args.plan_only {
        let min_row_tokens = match served_min_row_tokens(&model, &door) {
            Ok(n) => n,
            Err(refusal) => {
                return fail(&format!("the served task does not fit the door: {refusal}"))
            }
        };
        println!(
            "{}",
            serde_json::json!({
                "probe": "plan",
                "shape": shape_name,
                "texts": texts.len(),
                "max_text_bytes": texts.iter().map(String::len).max(),
                "tokens_planned": planned_tokens,
                "min_row_tokens": min_row_tokens,
                "max_texts": door.max_texts,
                "max_total_tokens": door.max_total_tokens,
                "artifact_sha256": expected,
                "labels": labels,
            })
        );
        return ExitCode::SUCCESS;
    }
    let Some(url) = args.url.as_deref() else {
        return fail("--url is required");
    };

    let mut ok = true;
    if !args.cold_first {
        match run_identity_probe(url, bearer, &expected, &labels, &probe_id).await {
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
                        "response_labels_match": report.response_labels_match,
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
        // The replay legs run BEFORE the early return below: without `--maximal` that return
        // ends the run, and a replay placed after it would be unreachable.
        if let Some(n) = args.replay {
            let texts = replay_texts_from(args.replay_from, n);
            match run_replay(url, bearer, &expected, &labels, &texts, &probe_id).await {
                Ok(report) => {
                    ok &= report.ok();
                    println!(
                        "{}",
                        serde_json::json!({
                            "probe": "replay",
                            "probe_id": report.probe_id,
                            "n_texts": report.n_texts,
                            "n_results": report.n_results,
                            "one_result_per_text": report.one_result_per_text,
                            "probabilities_len_match": report.probabilities_len_match,
                            "probabilities_in_unit_range": report.probabilities_in_unit_range,
                            "sums_within_tolerance": report.sums_within_tolerance,
                            "max_abs_sum_deviation": report.max_abs_sum_deviation,
                            "label_is_argmax": report.label_is_argmax,
                            "labels_in_task_order": report.labels_in_task_order,
                            "identity_matches": report.identity_matches,
                            "artifact_sha256": report.artifact_sha256,
                            "labels": report.labels,
                            "tokens_total": report.tokens_total,
                            "truncated": report.truncated,
                            "elapsed_ms": report.call.elapsed_ms,
                            "door_max_texts": door.max_texts,
                            "shape": "replay",
                        })
                    );
                }
                // A server REFUSAL of a legal-count replay (the token budget, during a
                // step-down) is a typed outcome, not a transport failure.
                Err(ProbeError::Protocol { detail, .. }) if detail.starts_with("tool error: ") => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "probe": "replay",
                            "refused": true,
                            "n_texts": n,
                            "refusal_bound": refusal_bound(&detail).as_str(),
                            "refusal": detail.trim_start_matches("tool error: "),
                            "door_max_texts": door.max_texts,
                        })
                    );
                    return ExitCode::from(3);
                }
                Err(e) => return fail(&format!("replay failed: {e}")),
            }
        }
        if args.replay_over_limit {
            match run_count_refusal(url, bearer, door.max_texts, &probe_id).await {
                Ok(report) => {
                    ok &= report.ok();
                    println!(
                        "{}",
                        serde_json::json!({
                            "probe": "replay_over_limit",
                            "probe_id": report.probe_id,
                            "n_texts": report.n_texts,
                            "is_error": report.is_error,
                            "names_count_bound": report.names_count_bound,
                            "names_limit_value": report.names_limit_value,
                            "names_contract": report.names_contract,
                            "elapsed_ms": report.call.elapsed_ms,
                            "door_max_texts": door.max_texts,
                        })
                    );
                }
                Err(e) => return fail(&format!("over-limit replay failed: {e}")),
            }
        }
        if shape.is_none() {
            return if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
    }

    match run_cold_first(url, bearer, &texts, &probe_id).await {
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
