//! The build-time check (design §5): the §5.2 rule table, in order, over the records the
//! configuration names. Ported from the platform's spike 036 prototype, which proved the table
//! offline (45/45) and the output grammar against the platform's real build-error extractor.
//!
//! Output grammar, stdout only: zero or more `WARN <rule> skipped: <why>` lines, then exactly
//! one verdict line: `ERROR: REFUSED <code>: <detail>` (exit 2, or 3 for `config-invalid`),
//! `ERROR: REFUSED s3-unreachable: <op> <uri>: <cause>` (exit 4), or `ELIGIBLE {json}`
//! (exit 0). No line ever carries a record's contents.

use serde_json::{json, Value};

use crate::config::{self, config_sha256, is_sha256_hex, Config, PublishKind, FIRST_MODEL};
use crate::store::{ObjectStore, StoreError};

/// Eligible: the model may deploy.
pub const EXIT_ELIGIBLE: i32 = 0;
/// A rule refused (a record is missing, wrong or ineligible).
pub const EXIT_REFUSED: i32 = 2;
/// The configuration did not parse or validate.
pub const EXIT_CONFIG_INVALID: i32 = 3;
/// The object store could not be read.
pub const EXIT_STORE_UNREACHABLE: i32 = 4;

/// `decide-apr-v1`'s artifact cap.
pub const ARTIFACT_CAP: u64 = 1_342_177_280;
/// `laya_eligibility MAX_RECORD_BYTES`: the pack, comparison and verification records.
pub const RECORD_CAP: u64 = 1_048_576;
/// The trainer's per-file cap on `eval.jsonl` and `eval-probs.json`.
pub const EVAL_CAP: u64 = 32 * 1024 * 1024;
/// The pack record's schema.
pub const PACK_SCHEMA: &str = "laya-train-pack-outcome-v1";

/// What the platform passes in (design §5.3). All optional; an empty string is absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    pub previous_artifact_sha256: Option<String>,
    pub published_sha256_history: Option<Vec<String>>,
    pub build_id: Option<String>,
}

impl Env {
    /// Read the three variables through `lookup` (the testable form of [`Self::from_env`]).
    /// A value that is empty after trimming is absent: the buildspec exports empty strings
    /// on a first build.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let non_empty = |name: &str| {
            lookup(name)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        Self {
            previous_artifact_sha256: non_empty("APRENDER_PREVIOUS_ARTIFACT_SHA256"),
            published_sha256_history: non_empty("APRENDER_PUBLISHED_SHA256_HISTORY").map(|h| {
                h.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            }),
            build_id: non_empty("APRENDER_BUILD_ID"),
        }
    }

    /// Read the three variables from the process environment.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }
}

/// What the binary prints and exits with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// 0 eligible, 2 refused, 3 config invalid, 4 store unreachable.
    pub exit: i32,
    /// The refusal code (`config-invalid`, `s3-unreachable` or a rule), `None` when eligible.
    pub code: Option<String>,
    /// Printed before the verdict, one per skipped rule.
    pub warns: Vec<String>,
    /// The one verdict line.
    pub last_line: String,
}

impl Outcome {
    fn refused(code: &str, detail: impl AsRef<str>, warns: Vec<String>) -> Self {
        Self {
            exit: EXIT_REFUSED,
            code: Some(code.to_string()),
            warns,
            last_line: format!("ERROR: REFUSED {code}: {}", detail.as_ref()),
        }
    }

    fn unreachable(e: &StoreError, warns: Vec<String>) -> Self {
        Self {
            exit: EXIT_STORE_UNREACHABLE,
            code: Some("s3-unreachable".into()),
            warns,
            last_line: format!("ERROR: REFUSED s3-unreachable: {e}"),
        }
    }

    /// The WARN lines, then the verdict line, each `\n`-terminated.
    #[must_use]
    pub fn render(&self) -> String {
        let mut s = String::new();
        for w in &self.warns {
            s.push_str("WARN ");
            s.push_str(w);
            s.push('\n');
        }
        s.push_str(&self.last_line);
        s.push('\n');
        s
    }
}

/// A JSON record under `cap`: `Ok(None)` absent or over the cap; `Ok(Some(Err))` not JSON.
async fn read_json(
    store: &dyn ObjectStore,
    uri: &str,
    cap: u64,
) -> Result<Option<Result<Value, String>>, StoreError> {
    match store.get(uri, cap).await? {
        None => Ok(None),
        Some(b) => Ok(Some(
            serde_json::from_slice::<Value>(&b).map_err(|e| e.to_string()),
        )),
    }
}

/// §5.2, rule by rule. Returns the outcome the binary must print and exit with.
#[allow(clippy::too_many_lines)]
#[allow(clippy::disallowed_methods)] // serde_json::json! expands to .unwrap()
pub async fn run(
    toml_text: &str,
    expected_id: &str,
    env: &Env,
    store: &dyn ObjectStore,
) -> Outcome {
    let cfg: Config = match config::parse(toml_text, expected_id) {
        Ok(c) => c,
        Err(e) => {
            return Outcome {
                exit: EXIT_CONFIG_INVALID,
                code: Some("config-invalid".into()),
                warns: vec![],
                last_line: format!("ERROR: REFUSED {e}"),
            }
        }
    };
    let mut warns: Vec<String> = Vec::new();
    let mut read = json!({
        "config_sha256": config_sha256(toml_text),
        "server": cfg.id, "environment": cfg.environment.as_str(), "kind": cfg.model.kind,
        "model_id": cfg.model.model_id, "artifact_sha256": cfg.model.artifact_sha256,
        "publish_kind": cfg.publish.kind.as_str(), "baseline_sha256": cfg.publish.baseline_sha256,
        "previous_artifact_sha256": env.previous_artifact_sha256, "build_id": env.build_id,
    });
    let art = cfg.model.artifact_sha256.as_str();

    // artifact-missing / artifact-too-large
    match store.head(&cfg.model.artifact_uri).await {
        Err(e) => return Outcome::unreachable(&e, warns),
        Ok(None) => {
            return Outcome::refused(
                "artifact-missing",
                format!("HeadObject {}: no such object", cfg.model.artifact_uri),
                warns,
            )
        }
        Ok(Some(n)) if n > ARTIFACT_CAP => {
            return Outcome::refused(
                "artifact-too-large",
                format!("ContentLength {n} > {ARTIFACT_CAP} (decide-apr-v1 cap)"),
                warns,
            )
        }
        Ok(Some(n)) => read["artifact_bytes"] = json!(n),
    }

    // pack-record-* (laya_eligibility.check_record, in Rust)
    let pack = match read_json(store, &cfg.records.pack_outcome_uri, RECORD_CAP).await {
        Err(e) => return Outcome::unreachable(&e, warns),
        Ok(None) => {
            return Outcome::refused(
                "pack-record-missing",
                format!(
                    "{} absent or over {RECORD_CAP} bytes",
                    cfg.records.pack_outcome_uri
                ),
                warns,
            )
        }
        Ok(Some(Err(e))) => {
            return Outcome::refused(
                "pack-record-schema",
                format!("not readable JSON ({e})"),
                warns,
            )
        }
        Ok(Some(Ok(v))) => v,
    };
    if pack.get("schema").and_then(Value::as_str) != Some(PACK_SCHEMA) {
        return Outcome::refused(
            "pack-record-schema",
            format!("schema is not {PACK_SCHEMA}"),
            warns,
        );
    }
    if pack.get("accepted") != Some(&Value::Bool(true)) {
        return Outcome::refused("pack-record-not-eligible", "accepted is not true", warns);
    }
    if pack.get("deploy_eligible") != Some(&Value::Bool(true)) {
        return Outcome::refused(
            "pack-record-not-eligible",
            "deploy_eligible is not true",
            warns,
        );
    }
    match pack.get("apr_sha256").and_then(Value::as_str) {
        Some(s) if is_sha256_hex(s) && s == art => {}
        Some(s) if is_sha256_hex(s) => {
            return Outcome::refused(
                "pack-record-sha-mismatch",
                format!("apr_sha256 {s} differs from [model].artifact_sha256 {art}"),
                warns,
            )
        }
        _ => {
            return Outcome::refused(
                "pack-record-schema",
                "apr_sha256 is not 64 lowercase hex",
                warns,
            )
        }
    }
    read["pack"] = json!({
        "task_id": pack.get("task_id"), "pack_image_digest": pack.get("pack_image_digest"),
        "parity_contract_sha256": pack.get("parity_contract_sha256"), "verify_seconds": pack.get("verify_seconds"),
    });

    // eval-missing
    for (name, uri) in [
        ("eval_jsonl", &cfg.records.eval_jsonl_uri),
        ("eval_probs", &cfg.records.eval_probs_uri),
    ] {
        match store.get(uri, EVAL_CAP).await {
            Err(e) => return Outcome::unreachable(&e, warns),
            Ok(None) => {
                return Outcome::refused(
                    "eval-missing",
                    format!("{uri} absent or over {EVAL_CAP} bytes"),
                    warns,
                )
            }
            Ok(Some(b)) => {
                read[format!("{name}_bytes")] = json!(b.len());
                read[format!("{name}_sha256")] = json!(aprender_mcp_decide_lambda::sha256_hex(&b));
            }
        }
    }

    // reason-missing
    if cfg.publish.reason.trim().is_empty() {
        return Outcome::refused("reason-missing", "[publish].reason is empty", warns);
    }

    let previous = env.previous_artifact_sha256.as_deref();

    // hold-active
    if let Some(hold) = &cfg.hold {
        match previous {
            None => warns.push("hold-active skipped: no previous published sha (first build)".into()),
            Some(p) if p != art && !cfg.publish.clears_hold => {
                return Outcome::refused(
                    "hold-active",
                    format!(
                        "[hold] is present ({:?}) and this publish changes the artifact from {p} to {art}; set [publish].clears_hold = true to override",
                        hold.reason
                    ),
                    warns,
                )
            },
            Some(_) => {},
        }
        read["hold"] = json!({ "reason": hold.reason, "cleared": cfg.publish.clears_hold });
    }

    if cfg.is_prod() {
        let baseline = cfg
            .publish
            .baseline_sha256
            .as_deref()
            .unwrap_or(FIRST_MODEL);
        // baseline-mismatch
        match previous {
            None => {
                warns.push("baseline-mismatch skipped: no previous published sha (first build)".into());
            },
            Some(p) if baseline == FIRST_MODEL => {
                return Outcome::refused(
                    "baseline-mismatch",
                    format!("[publish].baseline_sha256 is first_model but a previous published sha exists ({p})"),
                    warns,
                )
            },
            Some(p) if p != baseline => {
                return Outcome::refused(
                    "baseline-mismatch",
                    format!("[publish].baseline_sha256 {baseline} is not the previous published sha {p}"),
                    warns,
                )
            },
            Some(_) => {},
        }

        match cfg.publish.kind {
            PublishKind::Promote => {
                // comparison-* (unless first_model)
                if baseline != FIRST_MODEL {
                    let Some(uri) = &cfg.records.comparison_uri else {
                        return Outcome::refused(
                            "comparison-missing",
                            "[records].comparison_uri is not set",
                            warns,
                        );
                    };
                    let cmp = match read_json(store, uri, RECORD_CAP).await {
                        Err(e) => return Outcome::unreachable(&e, warns),
                        Ok(None) => {
                            return Outcome::refused(
                                "comparison-missing",
                                format!("{uri} absent or over {RECORD_CAP} bytes"),
                                warns,
                            )
                        }
                        Ok(Some(Err(e))) => {
                            return Outcome::refused(
                                "comparison-missing",
                                format!("{uri} is not readable JSON ({e})"),
                                warns,
                            )
                        }
                        Ok(Some(Ok(v))) => v,
                    };
                    let cand = cmp
                        .get("candidate_sha256")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let base = cmp
                        .get("baseline_sha256")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if cand != art {
                        return Outcome::refused(
                            "comparison-sha-mismatch",
                            format!("comparison candidate_sha256 {cand} is not the artifact {art}"),
                            warns,
                        );
                    }
                    if base != baseline {
                        return Outcome::refused(
                            "comparison-sha-mismatch",
                            format!("comparison baseline_sha256 {base} is not [publish].baseline_sha256 {baseline}"),
                            warns,
                        );
                    }
                    // The verdict is shown, not enforced (umbrella §5.6).
                    read["comparison"] = json!({
                        "verdict": cmp.get("verdict"), "metric": cmp.get("metric"),
                        "candidate": cmp.get("candidate"), "baseline": cmp.get("baseline"),
                    });
                }
                // staging-verification-*
                let Some(uri) = &cfg.records.staging_verification_uri else {
                    return Outcome::refused(
                        "staging-verification-missing",
                        "[records].staging_verification_uri is not set",
                        warns,
                    );
                };
                let ver = match read_json(store, uri, RECORD_CAP).await {
                    Err(e) => return Outcome::unreachable(&e, warns),
                    Ok(None) => {
                        return Outcome::refused(
                            "staging-verification-missing",
                            format!("{uri} absent or over {RECORD_CAP} bytes"),
                            warns,
                        )
                    }
                    Ok(Some(Err(e))) => {
                        return Outcome::refused(
                            "staging-verification-missing",
                            format!("{uri} is not readable JSON ({e})"),
                            warns,
                        )
                    }
                    Ok(Some(Ok(v))) => v,
                };
                let vsha = ver
                    .get("artifact_sha256")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if vsha != art {
                    return Outcome::refused(
                        "staging-verification-failed",
                        format!("record artifact_sha256 {vsha} is not the artifact {art}"),
                        warns,
                    );
                }
                if ver.get("pass") != Some(&Value::Bool(true)) {
                    return Outcome::refused(
                        "staging-verification-failed",
                        "record pass is not true",
                        warns,
                    );
                }
                if ver.get("environment").and_then(Value::as_str) != Some("staging") {
                    return Outcome::refused(
                        "staging-verification-failed",
                        "record environment is not staging",
                        warns,
                    );
                }
                read["staging_verification"] = json!({
                    "verified_at": ver.get("verified_at"), "verified_by": ver.get("verified_by"),
                    "max_abs_diff": ver.get("max_abs_diff"),
                });
            }
            PublishKind::Rollback => {
                if previous.is_none() {
                    return Outcome::refused(
                        "rollback-first-publish",
                        "a rollback needs a previous published sha and this server has none",
                        warns,
                    );
                }
                match &env.published_sha256_history {
                    None => warns.push(
                        "rollback-unknown-sha skipped: no published-sha history was passed".into(),
                    ),
                    Some(h) if !h.iter().any(|s| s == art) => {
                        return Outcome::refused(
                            "rollback-unknown-sha",
                            format!(
                                "{art} was never published by this server (history has {} entries)",
                                h.len()
                            ),
                            warns,
                        )
                    }
                    Some(_) => {}
                }
            }
        }
    }

    read["warns"] = json!(warns);
    let last_line = match serde_json::to_string(&read) {
        Ok(s) => format!("ELIGIBLE {s}"),
        Err(e) => {
            return Outcome {
                exit: EXIT_CONFIG_INVALID,
                code: Some("config-invalid".into()),
                warns,
                last_line: format!(
                "ERROR: REFUSED config-invalid: the ELIGIBLE record could not be serialised ({e})"
            ),
            }
        }
    };
    Outcome {
        exit: EXIT_ELIGIBLE,
        code: None,
        warns,
        last_line,
    }
}
