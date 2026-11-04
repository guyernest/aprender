//! The three tools (design §4.2–§4.4): `classify` loads the model lazily on its first call,
//! `status` never loads, `verify` re-scores frozen evaluation rows behind a door. Ported from
//! the platform's spike 033 harness handlers, which were measured live behind the proxy.

use std::future::Future;
use std::pin::Pin;
use std::time::Instant;

use aprender_mcp_decide::{parse_args, ClassifyArgs, ClassifyLimits, TOOL_NAME};
use aprender_mcp_decide_lambda::load_log_line;
use pmcp::{RequestHandlerExtra, SimpleTool, SimpleToolExt, ToolHandler};

use crate::config::ModelSection;
use crate::server::Runtime;

/// The boxed future a [`pmcp::SimpleTool`] handler returns.
pub type ToolFuture = Pin<Box<dyn Future<Output = pmcp::Result<serde_json::Value>> + Send>>;

/// The `classify` description, from the configuration (never from the model, which is not
/// loaded when `tools/list` is answered): `[model].description`, then the bounds and labels.
#[must_use]
pub fn classify_description(model: &ModelSection) -> String {
    let l = ClassifyLimits::CONTRACTED;
    let mut s = String::new();
    if let Some(d) = &model.description {
        s.push_str(d);
        s.push(' ');
    }
    s.push_str(&format!(
        "Classify 1..{} texts (each at most {} bytes, {} built tokens per call) with the \
         {} decider; labels: {}. The model loads on the first call of a fresh container.",
        l.max_texts,
        l.max_text_bytes,
        l.max_total_tokens,
        model.model_id,
        model
            .labels
            .as_ref()
            .map_or_else(|| "(from the artifact)".to_string(), |v| v.join(", ")),
    ));
    s
}

/// The load failure as the caller sees it: the kind and the detail, and that the next call
/// retries (the cell stays empty).
fn load_error(f: &crate::load::LoadFailure) -> pmcp::Error {
    pmcp::Error::internal(format!(
        "model load failed ({}): {}; the next call retries",
        f.kind, f.detail
    ))
}

/// `classify`: bounds first (an oversized or malformed request is refused before it can
/// trigger a cold load), then the once-per-process load, then the contracted service.
pub fn classify_tool(rt: Runtime) -> impl ToolHandler + 'static {
    let description = classify_description(&rt.config.model);
    SimpleTool::new(
        TOOL_NAME,
        move |args: serde_json::Value, _extra: RequestHandlerExtra| -> ToolFuture {
            let rt = rt.clone();
            Box::pin(async move {
                let parsed = parse_args(args)?;
                let started = Instant::now();
                let (loaded, performed) = rt
                    .cell
                    .get_or_load(&rt.source, rt.config.model.labels.as_deref())
                    .await
                    .map_err(|f| load_error(&f))?;
                let load_ms = started.elapsed().as_millis();
                tracing::info!(
                    "{}",
                    load_log_line(
                        performed,
                        None,
                        load_ms,
                        performed.then_some(&loaded.timeline)
                    )
                );
                let response = loaded.service.call(parsed).await?;
                serde_json::to_value(&response)
                    .map_err(|e| pmcp::Error::internal(format!("response serialization: {e}")))
            })
        },
    )
    .with_description(description)
    .with_schema_from::<ClassifyArgs>()
}

pub use args::StatusArgs;

/// The argument types and their schema derives: the only non-test place in this crate where
/// `clippy::disallowed_methods` is allowed. schemars' `JsonSchema` derive expands to
/// `.unwrap()` and its impl lands at module scope, where a struct-level allow cannot reach
/// it (the `aprender-mcp-decide` precedent, IN-05).
mod args {
    #![allow(clippy::disallowed_methods)]

    use serde::Deserialize;

    /// `status` takes no arguments.
    #[derive(Debug, Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct StatusArgs {}

    /// `verify`'s arguments (design §4.4). Defaults: `[verify].default_rows`, seed 0, write.
    #[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct VerifyArgs {
        /// How many frozen evaluation rows to re-score, 1..=8.
        #[serde(default)]
        pub rows: Option<u32>,
        /// The seed of the row selection; the same seed selects the same rows.
        #[serde(default)]
        pub seed: Option<u64>,
        /// Write the verification record under `[records].verification_prefix` (default true).
        #[serde(default)]
        pub write_record: Option<bool>,
    }
}

/// The non-secret part of the verified caller (`extra.auth_context`, which pmcp fills from
/// the proxy's `x-pmcp-user-*` headers; never a token), or `null` without an identity.
#[must_use]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap()
pub fn identity_json(extra: &RequestHandlerExtra) -> serde_json::Value {
    extra
        .auth_context
        .as_ref()
        .map_or(serde_json::Value::Null, |c| {
            serde_json::json!({
                "subject": c.subject,
                "email": c.email(),
                "groups": c.groups(),
            })
        })
}

/// `status`: the configuration, the load state and the build identity. Never loads.
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap()
pub fn status_tool(rt: Runtime) -> impl ToolHandler + 'static {
    SimpleTool::new(
        "status",
        move |_args: serde_json::Value, extra: RequestHandlerExtra| -> ToolFuture {
            let rt = rt.clone();
            Box::pin(async move {
                let cfg = &rt.config;
                if let Some(role) = cfg.tools.status_required_role.as_deref() {
                    if !verify_allowed(&extra, role, &cfg.tools.verify_allowed_principals) {
                        return Err(denied("status", role));
                    }
                }
                let l = ClassifyLimits::CONTRACTED;
                let load = rt.cell.get().map(|x| {
                    serde_json::json!({
                        "performed_at_unix": x.performed_at_unix,
                        "download_ms": x.timeline.fetch_ms,
                        "sha_ms": x.timeline.sha_ms,
                        "build_ms": x.timeline.build_ms,
                        "bytes": x.timeline.bytes,
                        "rss_mb": x.timeline.rss_mb,
                        "peak_rss_mb": x.timeline.peak_rss_mb,
                        "graviton": x.timeline.graviton,
                        "source": x.timeline.source,
                        "artifact_sha256": x.timeline.artifact_sha256,
                    })
                });
                Ok(serde_json::json!({
                    "schema": "aprender-status/v1",
                    "server": cfg.id,
                    "environment": cfg.environment.as_str(),
                    "model": {
                        "kind": cfg.model.kind,
                        "model_id": cfg.model.model_id,
                        "artifact_sha256": cfg.model.artifact_sha256,
                        "labels": cfg.model.labels,
                    },
                    "loaded": load.is_some(),
                    "load": load,
                    "bounds": {
                        "max_texts": l.max_texts,
                        "max_text_bytes": l.max_text_bytes,
                        "max_total_tokens": l.max_total_tokens,
                        "max_in_flight": l.max_in_flight,
                        "max_pending": l.max_pending,
                    },
                    "publish": {
                        "kind": cfg.publish.kind.as_str(),
                        "reason": cfg.publish.reason,
                        "baseline_sha256": cfg.publish.baseline_sha256,
                    },
                    "hold": cfg.hold.as_ref().map(|h| serde_json::json!({ "reason": h.reason })),
                    "records": {
                        "pack_outcome_uri": cfg.records.pack_outcome_uri,
                        "eval_jsonl_uri": cfg.records.eval_jsonl_uri,
                        "eval_probs_uri": cfg.records.eval_probs_uri,
                        "verification_prefix": cfg.records.verification_prefix,
                    },
                    "identity": identity_json(&extra),
                    "build": {
                        "build_id": rt.build_id,
                        "config_sha256": rt.config_sha256,
                        "crate_version": env!("CARGO_PKG_VERSION"),
                    },
                }))
            })
        },
    )
    .with_description(
        "Configuration, load state and build identity of this server. Never loads the model.",
    )
    .with_schema_from::<StatusArgs>()
}

// ---------------------------------------------------------------------------
// verify (design §4.4) and its door (§4.6)
// ---------------------------------------------------------------------------

pub use args::VerifyArgs;

/// The door, in this order (design §4.6, corrected by the platform's spike 035): no verified
/// identity → deny; any group equals `role` → allow; `subject` equals an allow-listed
/// principal (a Cognito app client id, exact equality) → allow; else deny. The allow-list is
/// tested after the groups only because a machine token carries no groups at all.
#[must_use]
pub fn verify_allowed(
    extra: &RequestHandlerExtra,
    role: &str,
    allowed_principals: &[String],
) -> bool {
    let Some(ctx) = extra.auth_context.as_ref() else {
        return false;
    };
    if ctx.groups().iter().any(|g| g == role) {
        return true;
    }
    allowed_principals.iter().any(|p| p == &ctx.subject)
}

/// The refusal the platform's checks match on: a JSON-RPC error, the GraphRAG wording.
fn denied(tool: &str, role: &str) -> pmcp::Error {
    pmcp::Error::internal(format!("Permission denied: {tool} requires '{role}' role"))
}

/// `Permission denied: verify requires '<role>' role`.
#[must_use]
pub fn permission_denied(role: &str) -> pmcp::Error {
    denied("verify", role)
}

/// `rows` row indices out of `0..n_eval`: a ChaCha8 shuffle seeded by `seed`, the first
/// `rows`, sorted ascending, so a sample is reproducible from its seed.
#[must_use]
pub fn select_rows(n_eval: usize, rows: usize, seed: u64) -> Vec<usize> {
    use rand::seq::SliceRandom;
    use rand::SeedableRng;
    let mut indices: Vec<usize> = (0..n_eval).collect();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    indices.shuffle(&mut rng);
    indices.truncate(rows);
    indices.sort_unstable();
    indices
}

/// One `eval.jsonl` row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct EvalRow {
    pub text: String,
    pub label: String,
}

/// Parse `eval.jsonl`: one JSON object per line, `\r` stripped, blank lines skipped.
///
/// # Errors
///
/// A message naming the 1-based line that is not a row.
pub fn read_eval_rows(bytes: &[u8]) -> Result<Vec<EvalRow>, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| format!("not UTF-8: {e}"))?;
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let row: EvalRow = serde_json::from_str(line)
            .map_err(|e| format!("line {}: not an eval row ({e})", i + 1))?;
        rows.push(row);
    }
    Ok(rows)
}

/// The rows with their label resolved to its index in `labels`, for the fork's probs reader.
///
/// # Errors
///
/// A message naming the first unknown label.
pub fn eval_to_data_rows(
    rows: &[EvalRow],
    labels: &[String],
) -> Result<Vec<aprender_decide::verify::DataRow>, String> {
    rows.iter()
        .enumerate()
        .map(|(i, r)| {
            let label = labels
                .iter()
                .position(|l| l == &r.label)
                .ok_or_else(|| format!("row {i}: label {:?} is not one of {labels:?}", r.label))?;
            Ok(aprender_decide::verify::DataRow {
                text: r.text.clone(),
                label,
            })
        })
        .collect()
}

/// The largest absolute component difference, NaN-visible: a NaN on either side, or a
/// length mismatch, is `NaN`, which fails every `<= tol`.
#[must_use]
pub fn max_abs_diff(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() {
        return f64::NAN;
    }
    a.iter().zip(b).fold(0.0_f64, |acc, (x, y)| {
        let d = (f64::from(*x) - f64::from(*y)).abs();
        if acc.is_nan() || d.is_nan() {
            f64::NAN
        } else {
            acc.max(d)
        }
    })
}

fn nan_max(acc: f64, d: f64) -> f64 {
    if acc.is_nan() || d.is_nan() {
        f64::NAN
    } else {
        acc.max(d)
    }
}

/// `verify`: re-score `rows` frozen evaluation rows through the same service as `classify`
/// and compare with the training run's stored probabilities; write the record create-only.
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap()
#[allow(clippy::too_many_lines)]
pub fn verify_tool(rt: Runtime) -> impl ToolHandler + 'static {
    let description = format!(
        "Re-score frozen evaluation rows against the training run's stored probabilities. \
         Requires the '{}' role.",
        rt.config.verify_role()
    );
    SimpleTool::new(
        "verify",
        move |args: serde_json::Value, extra: RequestHandlerExtra| -> ToolFuture {
            let rt = rt.clone();
            Box::pin(async move {
                let cfg = &rt.config;
                let role = cfg.verify_role();
                if !verify_allowed(&extra, role, &cfg.tools.verify_allowed_principals) {
                    return Err(permission_denied(role));
                }
                let parsed: VerifyArgs = serde_json::from_value(args)
                    .map_err(|e| pmcp::Error::validation(format!("verify: {e}")))?;
                let max_rows = ClassifyLimits::CONTRACTED.max_texts;
                let rows = parsed.rows.map_or_else(
                    || cfg.default_rows() as usize,
                    |r| usize::try_from(r).unwrap_or(usize::MAX),
                );
                if rows < 1 || rows > max_rows {
                    return Err(pmcp::Error::tool_rejected(
                        format!("verify rows must be 1..={max_rows} (got {rows})"),
                        None,
                    ));
                }
                let seed = parsed.seed.unwrap_or(0);
                let write_record = parsed.write_record.unwrap_or(true);

                // The records first: a missing or oversized file never costs a cold load.
                let eval_cap = crate::precheck::EVAL_CAP;
                let mut files = Vec::with_capacity(2);
                for uri in [&cfg.records.eval_jsonl_uri, &cfg.records.eval_probs_uri] {
                    let bytes = rt
                        .store
                        .get(uri, eval_cap)
                        .await
                        .map_err(|e| pmcp::Error::internal(format!("verify: {e}")))?
                        .ok_or_else(|| {
                            pmcp::Error::internal(format!(
                                "verify: {uri} absent or over {eval_cap} bytes"
                            ))
                        })?;
                    files.push(bytes);
                }
                let probs_bytes = files.pop().unwrap_or_default();
                let eval_bytes = files.pop().unwrap_or_default();
                let eval = read_eval_rows(&eval_bytes)
                    .map_err(|e| pmcp::Error::internal(format!("verify: eval.jsonl: {e}")))?;
                if rows > eval.len() {
                    return Err(pmcp::Error::tool_rejected(
                        format!(
                            "verify rows {rows} exceeds the eval set's {} rows",
                            eval.len()
                        ),
                        None,
                    ));
                }

                let started = Instant::now();
                let (loaded, performed) = rt
                    .cell
                    .get_or_load(&rt.source, cfg.model.labels.as_deref())
                    .await
                    .map_err(|f| load_error(&f))?;
                tracing::info!(
                    "{}",
                    load_log_line(
                        performed,
                        None,
                        started.elapsed().as_millis(),
                        performed.then_some(&loaded.timeline)
                    )
                );

                let data_rows = eval_to_data_rows(&eval, &loaded.labels)
                    .map_err(|e| pmcp::Error::internal(format!("verify: eval.jsonl: {e}")))?;
                let probs = aprender_decide::verify::validate_probs(
                    aprender_decide::verify::ProbsWhich::FineTuned,
                    &probs_bytes,
                    &data_rows,
                    &loaded.labels,
                )
                .map_err(|e| pmcp::Error::internal(format!("verify: eval-probs.json: {e}")))?;

                let selected = select_rows(eval.len(), rows, seed);
                let texts: Vec<&str> = selected.iter().map(|&i| eval[i].text.as_str()).collect();
                // The classify door's bounds apply to the selected rows; its refusal is
                // re-worded for a verify caller, who sent no texts, and stays in-band.
                let for_verify = |e: pmcp::Error| match e {
                    pmcp::Error::ToolRejected { message, .. } => pmcp::Error::tool_rejected(
                        format!(
                            "verify: a selected eval row cannot be scored through the classify \
                             door ({message}); choose another seed or fewer rows"
                        ),
                        None,
                    ),
                    other => other,
                };
                let classify_args =
                    parse_args(serde_json::json!({ "texts": texts })).map_err(for_verify)?;
                let response = loaded
                    .service
                    .call(classify_args)
                    .await
                    .map_err(for_verify)?;
                let max_abs = selected
                    .iter()
                    .zip(&response.results)
                    .map(|(&row, r)| max_abs_diff(&r.probabilities, &probs[row]))
                    .fold(0.0_f64, nan_max);
                let tol = cfg.tolerance_abs();
                let pass = max_abs <= tol;

                let now = chrono::Utc::now();
                let verified_by = extra.auth_context.as_ref().map_or_else(String::new, |c| {
                    c.email().map_or_else(|| c.subject.clone(), str::to_string)
                });
                let record = serde_json::json!({
                    "schema": "aprender-verification/v1",
                    "server": cfg.id,
                    "environment": cfg.environment.as_str(),
                    "artifact_sha256": cfg.model.artifact_sha256,
                    "model_id": cfg.model.model_id,
                    "eval_jsonl_sha256": aprender_mcp_decide_lambda::sha256_hex(&eval_bytes),
                    "eval_probs_sha256": aprender_mcp_decide_lambda::sha256_hex(&probs_bytes),
                    "rows_scored": selected.len(),
                    "row_indices": selected,
                    "max_abs_diff": max_abs,
                    "tolerance_abs": tol,
                    "pass": pass,
                    "verified_at": now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    "verified_by": verified_by,
                    "build_id": rt.build_id,
                });

                let (written, record_uri, reason) =
                    match (&cfg.records.verification_prefix, write_record) {
                        (None, _) => (
                            false,
                            None,
                            Some("no verification_prefix configured".to_string()),
                        ),
                        (Some(_), false) => (false, None, Some("write_record = false".to_string())),
                        (Some(prefix), true) => {
                            let sha12 = cfg.model.artifact_sha256.get(..12).unwrap_or_default();
                            let uri =
                                format!("{prefix}{}-{sha12}.json", now.format("%Y%m%dT%H%M%SZ"));
                            let bytes = serde_json::to_vec_pretty(&record).map_err(|e| {
                                pmcp::Error::internal(format!("verify: record serialization: {e}"))
                            })?;
                            match rt.store.put_if_absent(&uri, bytes).await {
                                Ok(crate::store::PutOutcome::Written) => (true, Some(uri), None),
                                Ok(crate::store::PutOutcome::AlreadyExists) => {
                                    (false, Some(uri), Some("record already exists".to_string()))
                                }
                                Err(e) => (false, Some(uri), Some(e.0)),
                            }
                        }
                    };
                Ok(serde_json::json!({
                    "record": record,
                    "written": written,
                    "record_uri": record_uri,
                    "reason": reason,
                }))
            })
        },
    )
    .with_description(description)
    .with_schema_from::<VerifyArgs>()
}
