
/// Gate 1: Metadata Plausibility Validation (Bug 210, GH-222)
///
/// Validates that model hyperparameters (rope_theta, max_position_embeddings, rms_norm_eps)
/// fall within known plausible ranges for the detected architecture family.
///
/// This gate catches the root cause of GH-222: importing SafeTensors without config.json
/// silently stored rope_theta=10000.0 for Qwen2, which should be 1000000.0.
fn run_metadata_plausibility_gate(path: &Path, config: &QaConfig) -> Result<GateResult> {
    let start = Instant::now();

    if !config.json && config.verbose {
        println!(
            "{}",
            "Running metadata plausibility validation (Bug 210)...".yellow()
        );
    }

    // Extract metadata from the model file
    let data = std::fs::read(path)
        .map_err(|e| CliError::ValidationFailed(format!("Failed to read model: {e}")))?;

    if data.len() < 4 {
        let duration = start.elapsed();
        return Ok(GateResult::failed(
            "metadata_plausibility",
            "File too small for metadata extraction",
            None,
            None,
            duration,
        ));
    }

    let (architecture, rope_theta, max_pos, rms_norm_eps) = extract_model_metadata(&data, path)?;

    let mut violations: Vec<String> = Vec::new();
    let mut checks_passed = 0usize;

    check_rope_theta(
        architecture.as_deref(),
        rope_theta,
        &data,
        &mut violations,
        &mut checks_passed,
    );
    check_max_position_embeddings(max_pos, &mut violations, &mut checks_passed);
    check_rms_norm_eps(rms_norm_eps, &mut violations, &mut checks_passed);
    check_arch_theta_cross_validation(
        architecture.as_ref(),
        rope_theta,
        &mut violations,
        &mut checks_passed,
    );

    let duration = start.elapsed();

    if violations.is_empty() {
        Ok(GateResult::passed(
            "metadata_plausibility",
            &format!(
                "{checks_passed} metadata checks passed (arch={}, rope_theta={}, max_pos={})",
                architecture.as_deref().unwrap_or("unknown"),
                rope_theta.map_or("none".to_string(), |t| format!("{t}")),
                max_pos.map_or("none".to_string(), |p| format!("{p}")),
            ),
            Some(checks_passed as f64),
            Some(0.0),
            duration,
        ))
    } else {
        Ok(GateResult::failed(
            "metadata_plausibility",
            &format!(
                "{} metadata violation(s): {}",
                violations.len(),
                violations.join("; ")
            ),
            Some(violations.len() as f64),
            Some(0.0),
            duration,
        ))
    }
}

/// Return plausible rope_theta range for an architecture family.
fn rope_theta_range(arch: Option<&str>) -> (f64, f64, &'static str) {
    match arch {
        Some("qwen2" | "qwen2.5" | "qwen") => (100_000.0, f64::MAX, "expected ~1000000.0 (100x too low, will produce garbage)"),
        Some("llama" | "llama2" | "llama3") => (1000.0, 10_000_000.0, "expected 10000-500000"),
        _ => (100.0, 100_000_000.0, "outside plausible range [100, 100M]"),
    }
}

/// Check rope_theta plausibility per architecture family.
fn check_rope_theta(
    arch: Option<&str>,
    rope_theta: Option<f32>,
    data: &[u8],
    violations: &mut Vec<String>,
    checks_passed: &mut usize,
) {
    let Some(theta) = rope_theta else {
        if data.len() >= 4 && &data[0..4] == b"GGUF" {
            *checks_passed += 1;
        } else {
            violations.push("rope_theta missing from APR metadata".to_string());
        }
        return;
    };
    let theta_f64 = f64::from(theta);
    let (min, max, msg) = rope_theta_range(arch);
    if theta_f64 >= min && theta_f64 <= max {
        *checks_passed += 1;
    } else {
        violations.push(format!("rope_theta={theta} for {} — {msg}", arch.unwrap_or("unknown")));
    }
}

/// Check max_position_embeddings is within plausible range.
fn check_max_position_embeddings(
    max_pos: Option<usize>,
    violations: &mut Vec<String>,
    checks_passed: &mut usize,
) {
    if let Some(val) = max_pos {
        if (128..=1_048_576).contains(&val) {
            *checks_passed += 1;
        } else {
            violations.push(format!(
                "max_position_embeddings={val} outside plausible range [128, 1M]"
            ));
        }
    } else {
        *checks_passed += 1;
    }
}

/// Check rms_norm_eps is within plausible range.
fn check_rms_norm_eps(
    rms_norm_eps: Option<f32>,
    violations: &mut Vec<String>,
    checks_passed: &mut usize,
) {
    if let Some(eps) = rms_norm_eps {
        let eps_f64 = f64::from(eps);
        if eps_f64 <= 0.0 || eps_f64 > 0.01 {
            violations.push(format!(
                "rms_norm_eps={eps} outside plausible range (0, 0.01]"
            ));
        } else {
            *checks_passed += 1;
        }
    } else {
        *checks_passed += 1;
    }
}

/// Cross-validate architecture against rope_theta (Bug 210 signature detection).
fn check_arch_theta_cross_validation(
    architecture: Option<&String>,
    rope_theta: Option<f32>,
    violations: &mut Vec<String>,
    checks_passed: &mut usize,
) {
    if let (Some(arch), Some(theta)) = (architecture, rope_theta) {
        let theta_f64 = f64::from(theta);
        let suspicious = matches!(arch.as_str(), "qwen2" | "qwen2.5" | "qwen")
            && (theta_f64 - 10000.0).abs() < 1.0;
        if suspicious {
            violations.push(format!(
                "CRITICAL: {arch} with rope_theta=10000.0 — \
                 likely missing config.json (Bug 210)"
            ));
        } else {
            *checks_passed += 1;
        }
    } else {
        *checks_passed += 1;
    }
}

/// Metadata extracted from model file for plausibility validation.
type ModelMetadata = (Option<String>, Option<f32>, Option<usize>, Option<f32>);

/// Extract model metadata from file bytes (GGUF, APR, or SafeTensors format).
fn extract_model_metadata(data: &[u8], path: &Path) -> Result<ModelMetadata> {
    let magic = &data[0..4];

    if magic == b"GGUF" {
        // GGUF format: use GgufReader
        let reader = aprender::format::gguf::reader::GgufReader::from_bytes(data.to_vec())
            .map_err(|e| CliError::ValidationFailed(format!("GGUF parse failed: {e}")))?;
        let arch = reader.architecture();
        let rope_theta = reader.rope_theta();
        let max_pos = reader.context_length();
        let rms_norm_eps = reader.rms_norm_eps();
        Ok((arch, rope_theta, max_pos, rms_norm_eps))
    } else if &magic[0..3] == b"APR" || magic == b"APRN" {
        // APR format: parse v2 header + JSON metadata
        use aprender::format::v2::AprV2Reader;
        let reader = AprV2Reader::from_bytes(data)
            .map_err(|e| CliError::ValidationFailed(format!("APR parse failed: {e}")))?;
        let meta = reader.metadata();
        let _ = path;
        Ok((
            meta.architecture.clone(),
            meta.rope_theta,
            meta.max_position_embeddings,
            meta.rms_norm_eps,
        ))
    } else {
        // SafeTensors or unknown format: try to load config.json from sibling (GAP-UX-002)
        let config_path = {
            #[cfg(feature = "inference")]
            {
                realizar::safetensors::find_sibling_file(path, "config.json")
            }
            #[cfg(not(feature = "inference"))]
            {
                // Without realizar, manually look for sibling config.json
                path.parent()
                    .map(|dir| dir.join("config.json"))
                    .filter(|p| p.exists())
            }
        };
        if let Some(config_path) = config_path {
            // Read architecture and rope_theta from HF config.json
            let config_str = std::fs::read_to_string(&config_path)
                .map_err(|e| CliError::ValidationFailed(format!("config.json read failed: {e}")))?;
            let arch = extract_json_string(&config_str, "model_type");
            let rope_theta = extract_json_f32(&config_str, "rope_theta");
            let max_pos = extract_json_usize(&config_str, "max_position_embeddings");
            let rms_norm_eps = extract_json_f32(&config_str, "rms_norm_eps");
            Ok((arch, rope_theta, max_pos, rms_norm_eps))
        } else {
            // No config.json — return None for all fields (gate will note the gap)
            Ok((None, None, None, None))
        }
    }
}

/// Extract a string value from JSON by key (simple parser, no serde dependency).
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let idx = json.find(&pattern)?;
    let after_key = &json[idx + pattern.len()..];
    // Skip whitespace and colon
    let after_colon = after_key.find(':').map(|i| &after_key[i + 1..])?;
    let trimmed = after_colon.trim_start();
    if trimmed.starts_with('"') {
        let start = 1;
        let end = trimmed[start..].find('"')?;
        Some(trimmed[start..start + end].to_string())
    } else {
        None
    }
}

/// Extract an f32 value from JSON by key.
fn extract_json_f32(json: &str, key: &str) -> Option<f32> {
    let pattern = format!("\"{key}\"");
    let idx = json.find(&pattern)?;
    let after_key = &json[idx + pattern.len()..];
    let after_colon = after_key.find(':').map(|i| &after_key[i + 1..])?;
    let trimmed = after_colon.trim_start();
    // Parse until next comma, brace, or whitespace
    let end = trimmed.find([',', '}', '\n'])?;
    trimmed[..end].trim().parse::<f32>().ok()
}

/// Extract a usize value from JSON by key.
fn extract_json_usize(json: &str, key: &str) -> Option<usize> {
    let pattern = format!("\"{key}\"");
    let idx = json.find(&pattern)?;
    let after_key = &json[idx + pattern.len()..];
    let after_colon = after_key.find(':').map(|i| &after_key[i + 1..])?;
    let trimmed = after_colon.trim_start();
    let end = trimmed.find([',', '}', '\n'])?;
    trimmed[..end].trim().parse::<usize>().ok()
}

/// Output verification result (PMAT-QA-PROTOCOL-001 §7.4)
#[derive(Debug, Clone)]
pub enum OutputVerification {
    /// Output passed all checks
    Pass,
    /// Output failed verification
    Fail {
        /// Reason for failure
        reason: String,
    },
}

/// Statistical gibberish detection — returns Some(reason) if output is junk.
///
/// Three signals; any one triggers rejection:
/// 1. **Non-ASCII saturation**: For ASCII-prompt completions, > 60% non-ASCII
///    bytes is a strong gibberish indicator. English+code answers are ASCII-heavy.
/// 2. **Repeated-fragment detection**: A 4+ byte substring appearing 3+ times
///    consecutively (e.g. "udaÅĤo udaÅĤo udaÅĤo") flags BPE/loop pathologies.
/// 3. **Replacement-character density**: > 1 U+FFFD per 32 chars indicates
///    repeated UTF-8 decode failures.
///
/// All thresholds are conservative — coherent English/code outputs pass cleanly,
/// while the Qwen2-0.5B observed gibberish ("ëĸ» Ãĥ pÃ³Åº zwiÄħzku") is rejected.
fn detect_gibberish(output: &str, test_id: &str) -> Option<String> {
    // Signal 1: non-ASCII saturation
    let total = output.chars().count();
    if total >= 16 {
        let non_ascii = output.chars().filter(|c| !c.is_ascii()).count();
        let ratio = non_ascii as f64 / total as f64;
        if ratio > 0.6 {
            return Some(format!(
                "{test_id}: gibberish (non-ASCII ratio {:.1}% > 60%)",
                ratio * 100.0
            ));
        }
    }

    // Signal 2: 4+ byte fragment repeated 3+ times in a row
    let bytes = output.as_bytes();
    if bytes.len() >= 12 {
        let max_frag = 16.min(bytes.len() / 3);
        for frag_len in 4..=max_frag {
            let mut i = 0;
            while i + frag_len * 3 <= bytes.len() {
                let frag = &bytes[i..i + frag_len];
                if &bytes[i + frag_len..i + 2 * frag_len] == frag
                    && &bytes[i + 2 * frag_len..i + 3 * frag_len] == frag
                {
                    let preview = String::from_utf8_lossy(frag).into_owned();
                    return Some(format!(
                        "{test_id}: gibberish (fragment {preview:?} repeats 3+ times)"
                    ));
                }
                i += 1;
            }
        }
    }

    // Signal 3: replacement-character density
    let fffd_count = output.matches('\u{FFFD}').count();
    if fffd_count > 0 && total >= 32 && fffd_count * 32 > total {
        return Some(format!(
            "{test_id}: gibberish (U+FFFD density {fffd_count}/{total} > 1/32)"
        ));
    }

    None
}

/// Verify output is correct: not empty, no garbage, contains expected answer
/// (PMAT-QA-PROTOCOL-001 §7.4)
///
/// Order of checks is CRITICAL (fail fast on garbage):
/// 1. Not empty
/// 2. No garbage patterns (BEFORE checking answer)
/// 3. No BPE artifacts
/// 4. Contains expected answer
pub fn verify_output(
    output: &str,
    test_id: &str,
    expected_patterns: &[&str],
) -> OutputVerification {
    // Check 1: Not empty
    if output.trim().is_empty() {
        return OutputVerification::Fail {
            reason: format!("{test_id}: Empty output"),
        };
    }

    // Check 2: Garbage patterns (fail fast BEFORE checking answer)
    let garbage_patterns = ["\u{FFFD}", "[UNK]", "akunji", "olumbia"];
    for pattern in &garbage_patterns {
        if output.contains(pattern) {
            return OutputVerification::Fail {
                reason: format!("{test_id}: Garbage detected: '{pattern}'"),
            };
        }
    }

    // Check 2.5: Statistical gibberish detection (Toyota Way root-cause fix).
    // The fixed garbage-pattern list above misses new defect classes — e.g.
    // Qwen2-0.5B-Instruct emits CJK/Polish/diacritic byte-fragments like
    // "udaÅĤo", "ëĸ»", "zwiÄħzku" that no prior pattern catches. We add three
    // statistical signals; ANY positive trip rejects the output.
    if let Some(reason) = detect_gibberish(output, test_id) {
        return OutputVerification::Fail { reason };
    }

    // Check 3: BPE artifacts (null bytes, excessive control chars)
    let null_count = output.bytes().filter(|&b| b == 0).count();
    if null_count > 0 {
        return OutputVerification::Fail {
            reason: format!("{test_id}: {null_count} null bytes detected (BPE artifact)"),
        };
    }

    // Check 4: Contains expected answer
    if !expected_patterns.is_empty() {
        let found = expected_patterns
            .iter()
            .any(|p| output.to_lowercase().contains(&p.to_lowercase()));
        if !found {
            return OutputVerification::Fail {
                reason: format!(
                    "{test_id}: Expected one of {:?}, got: '{}'",
                    expected_patterns,
                    output.chars().take(100).collect::<String>()
                ),
            };
        }
    }

    OutputVerification::Pass
}

/// GH-279-4: Strip `<think>...</think>` blocks from model output.
///
/// Qwen3 thinking mode generates chain-of-thought reasoning inside `<think>` tags
/// before the actual answer. The golden output gate validates the ANSWER, not the
/// reasoning. This function strips thinking blocks so `verify_output()` sees only
/// the final answer.
///
/// Behavior:
/// - No `<think>` tags → passthrough (no-op for non-thinking models)
/// - Complete `<think>...</think>` → stripped, answer preserved
/// - Unclosed `<think>` (tokens exhausted during reasoning) → truncated at `<think>`
/// - Multiple blocks → all stripped
pub fn strip_thinking_blocks(output: &str) -> String {
    let mut result = output.to_string();
    // Strip all complete <think>...</think> blocks
    while let (Some(start), Some(end)) = (result.find("<think>"), result.find("</think>")) {
        if end > start {
            result = format!("{}{}", &result[..start], &result[end + "</think>".len()..]);
        } else {
            break;
        }
    }
    // Handle unclosed <think> (model ran out of tokens during reasoning)
    if let Some(start) = result.find("<think>") {
        result.truncate(start);
    }
    result.trim().to_string()
}

/// JIDOKA: Validate GPU golden output matches expected patterns (PMAT-232 lesson).
///
/// Without this, GPU correctness was NEVER tested — `apr qa` golden output only ran CPU.
/// Returns `Some(failure_reason)` if GPU output fails, `None` if pass or skipped.
#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(all(feature = "inference", feature = "cuda"))]
fn validate_gpu_golden_output(
    mapped: &realizar::gguf::MappedGGUFModel,
    prompt_tokens: &[u32],
    gen_config: &realizar::gguf::QuantizedGenerateConfig,
    gguf: &realizar::gguf::GGUFModel,
    expected_patterns: &[&str],
    config: &QaConfig,
) -> Result<Option<String>> {
    use realizar::gguf::{OwnedQuantizedModel, OwnedQuantizedModelCuda};
    let model = OwnedQuantizedModel::from_mapped(mapped)
        .map_err(|e| CliError::ValidationFailed(format!("Model failed: {e}")))?;
    match OwnedQuantizedModelCuda::new(model, 0) {
        Ok(mut cuda_model) => match cuda_model.generate_gpu_resident(prompt_tokens, gen_config) {
            Ok(gpu_tokens) => {
                let gpu_text = gguf.decode(&gpu_tokens);
                let gpu_answer = strip_thinking_blocks(&gpu_text); // GH-279-4
                if let OutputVerification::Fail { reason } =
                    verify_output(&gpu_answer, "golden_output_gpu", expected_patterns)
                {
                    return Ok(Some(format!("GPU output failed (CPU passed): {reason}")));
                }
            }
            Err(e) => {
                if !config.json && config.verbose {
                    println!("{}", format!("GPU golden output skipped: {e}").yellow());
                }
            }
        },
        Err(e) => {
            if !config.json && config.verbose {
                println!("{}", format!("CUDA init skipped: {e}").yellow());
            }
        }
    }
    Ok(None)
}

/// Run golden output test for APR format models.
///
/// PMAT-CODE-SHIP-006-FIX (2026-05-10): routed through `realizar::run_inference`
/// + `OwnedQuantizedModel::from_apr` (the same working path that SHIP-002 +
/// SHIP-008 LIVE-discharged) instead of the legacy `AprTransformer::from_apr_file`
/// + `generate_with_cache` path. The legacy path produced "\\ns\\ns" degenerate
/// output on canonical 7B teacher (recorded as §61.8 Branch A); the
/// run_inference path produces clean ChatML responses.
///
/// Caller passes a pre-formatted ChatML prompt (e.g.,
/// `"<|im_start|>user\\nWhat is 2+2?<|im_end|>\\n<|im_start|>assistant\\n"`)
/// per `golden_test_cases()` — we tokenize it directly and pass via
/// `InferenceConfig::with_input_tokens` to BYPASS the chat-template auto-wrap
/// in `prepare_tokens_apr` (which would double-wrap a pre-formatted prompt).
#[cfg(feature = "inference")]
fn golden_output_apr(path: &Path, prompt: &str, max_tokens: usize) -> Result<(Vec<u32>, String)> {
    use realizar::apr::AprV2Model;
    use realizar::{run_inference, InferenceConfig};

    // Tokenize the (already-ChatML-formatted) prompt with the embedded BPE
    // tokenizer. This produces the exact prompt token sequence the qa gate
    // intends; passing it via with_input_tokens bypasses prepare_tokens'
    // ChatML auto-wrap (which would otherwise double-wrap pre-formatted prompts).
    let apr_model = AprV2Model::load(path)
        .map_err(|e| CliError::ValidationFailed(format!("Failed to load APR: {e}")))?;
    let tokenizer = apr_model
        .load_embedded_bpe_tokenizer()
        .ok_or_else(|| CliError::ValidationFailed(ENCODER_ONLY_QA_REFUSAL.to_string()))?;
    let prompt_tokens = tokenizer.encode(prompt);

    let config = InferenceConfig::new(path)
        .with_input_tokens(prompt_tokens)
        .with_max_tokens(max_tokens)
        .with_temperature(0.0)
        .with_top_k(1);

    let result = run_inference(&config)
        .map_err(|e| CliError::ValidationFailed(format!("Generation failed: {e}")))?;

    Ok((result.tokens, result.text))
}

/// Why `apr qa` refuses an APR with no embedded tokenizer, and which doors DO cover it.
///
/// # This is a statement about the GATE's scope, never about the model (D-11)
///
/// `apr qa`'s first check loads an embedded BPE tokenizer and then generates with a token
/// budget and a top-k. It is a GENERATIVE-model gate, so an encoder or classifier APR has no
/// generative path for it to gate. Phase 4's closing audit item 7 established this with a
/// CONTROL rather than an anecdote: the SetFit-shaped container and a committed plain-BERT
/// `slice_model.apr` — no SetFit tag, no `setfit.head.*` entries, no U8 blob — receive the
/// IDENTICAL exit code and the IDENTICAL message. The cause is therefore the gate's scope and
/// not anything about the SetFit entries.
///
/// # Why the wording is constrained
///
/// The refusal must not suggest the artifact is unfit in any way. A well-formed classifier
/// meeting a gate that does not apply to it is an out-of-scope result, and an operator who
/// reads it as a verdict on their model will go looking for a defect that is not there. So the
/// message names the doors that DO cover the model — the quality-validation door, the
/// evaluation door and this phase's benchmark door — and says plainly that qa's own coverage
/// of encoder-only APRs is deferred.
///
/// # It EXTENDS rather than replaces
///
/// The leading sentence keeps the exact `APR missing embedded tokenizer` phrase the Phase 4
/// lifecycle control asserts on both fixtures, so this change cannot silently retire that
/// control. The exit code and the detection logic are untouched: no capability path is added
/// here, and D-11's capability half stays a deferred ticket.
pub(crate) const ENCODER_ONLY_QA_REFUSAL: &str = "APR missing embedded tokenizer. \
     `apr qa` is a GENERATIVE-model gate: its first check loads an embedded BPE tokenizer and \
     then generates with a token budget, so it has nothing to gate on an encoder or classifier \
     APR. This is a statement about the GATE's scope, not about the model. Encoder and \
     classifier APRs are covered by `apr validate --quality` for format and quality \
     validation, by `apr eval` for task metrics, and by `apr setfit bench run` / \
     `apr setfit bench report` for the benchmark door. Extending qa itself to cover \
     encoder-only APRs is deferred to its own ticket (D-11).";

#[cfg(test)]
mod encoder_only_qa_refusal_tests {
    use super::ENCODER_ONLY_QA_REFUSAL;

    /// The message says what a reader needs, in a MUST-MATCH / MUST-NOT-MATCH table.
    ///
    /// The must-not-match half is the half that matters. Every row is wording that would tell
    /// an operator their ARTIFACT is at fault, which is the one thing this refusal does not
    /// mean — the model is well formed and the gate simply does not apply to it. A refusal
    /// that reads as a verdict on the file sends someone hunting a defect that is not there.
    #[test]
    fn qa_encoder_only_refusal_names_the_doors_and_never_impugns_the_artifact() {
        for (label, needle) in [
            ("the phrase the Phase 4 control asserts", "APR missing embedded tokenizer"),
            ("the gate's own scope", "GENERATIVE-model gate"),
            ("the quality-validation door", "apr validate --quality"),
            ("the evaluation door", "apr eval"),
            ("the benchmark door", "apr setfit bench report"),
            ("the scope-not-model statement", "not about the model"),
            ("the deferred ticket", "D-11"),
        ] {
            assert!(
                ENCODER_ONLY_QA_REFUSAL.contains(needle),
                "{label} is MISSING from the encoder-only qa refusal: {ENCODER_ONLY_QA_REFUSAL}"
            );
        }

        let lowered = ENCODER_ONLY_QA_REFUSAL.to_lowercase();
        for (label, needle) in [
            ("invalidity", "invalid"),
            ("corruption", "corrupt"),
            ("malformation", "malformed"),
            ("unsupportedness", "unsupported"),
            ("brokenness", "broken"),
            ("damage", "damaged"),
            ("badness", "bad "),
            ("failure of the model", "the model failed"),
            ("a verdict on the file", "not a valid"),
        ] {
            assert!(
                !lowered.contains(needle),
                "the refusal used `{label}` wording ({needle:?}); it is out-of-scope, not a \
                 verdict on the artifact: {ENCODER_ONLY_QA_REFUSAL}"
            );
        }
    }

    /// NON-VACUITY. Every must-not-match row above must be capable of matching SOMETHING, or
    /// the loop proves nothing — a list of words no message would ever carry passes forever.
    #[test]
    fn qa_encoder_only_refusal_must_not_match_rows_can_actually_fire() {
        let doctored = format!("{ENCODER_ONLY_QA_REFUSAL} The APR is invalid and corrupt: \
             a malformed, unsupported, broken, damaged file, and a bad one. \
             the model failed and is not a valid APR.");
        let lowered = doctored.to_lowercase();
        for needle in [
            "invalid",
            "corrupt",
            "malformed",
            "unsupported",
            "broken",
            "damaged",
            "bad ",
            "the model failed",
            "not a valid",
        ] {
            assert!(
                lowered.contains(needle),
                "must-not-match row {needle:?} cannot match any string, so asserting its \
                 absence from the refusal is vacuous"
            );
        }
    }
}
