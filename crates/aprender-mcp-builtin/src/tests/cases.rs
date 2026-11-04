//! Fixtures: the design's §3.1 configurations as text, ported from the platform's spike 036
//! (`sources/036-precheck-rules-offline/precheck/src/cases.rs`). Task 3 adds the records,
//! the in-memory store and the 45-case table.
#![allow(dead_code)] // the record fixtures and the case table arrive with the precheck module

pub const SERVER_ID: &str = "antisemitism-staging";
pub const PROD_ID: &str = "antisemitism-prod";
/// The candidate.
pub const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
/// The baseline.
pub const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
/// Never published.
pub const C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

pub const STAGING_PREFIX: &str =
    "s3://aprender-laya-train-dev-123456789012/runs/antisemitism-v2/task-01jbx";
pub const PROD_PREFIX: &str = "s3://aprender-decide-weights-210987654321-prod/decide/antisemitism";

/// The staging example of §3.1: a server reading the trainer's run as it lies.
pub fn staging_config() -> String {
    format!(
        r#"id = "{SERVER_ID}"
schema = "aprender-server/v1"
environment = "staging"

[model]
kind = "decide"
model_id = "antisemitism-v2"
artifact_uri = "{STAGING_PREFIX}/antisemitism-v2.apr"
artifact_sha256 = "{A}"
labels = ["not_antisemitic", "antisemitic"]
description = "Antisemitism decider, GoldStandard2024 plus reviewer feedback through 2026-11."

[records]
pack_outcome_uri = "{STAGING_PREFIX}/pack-outcome.json"
eval_jsonl_uri   = "{STAGING_PREFIX}/data/eval.jsonl"
eval_probs_uri   = "{STAGING_PREFIX}/run/eval-probs.json"
verification_prefix = "{STAGING_PREFIX}/verifications/"

[publish]
reason = "Phase 16 SageMaker control model; staging for the next experiment."
"#
    )
}

/// The prod example of §3.1 (promote) and its rollback variant: a server reading copies.
pub fn prod_config(baseline: &str, kind: &str, artifact: &str) -> String {
    let kind_line = if kind == "promote" {
        String::new()
    } else {
        format!("kind = \"{kind}\"\n")
    };
    format!(
        r#"id = "{PROD_ID}"
schema = "aprender-server/v1"
environment = "prod"

[model]
kind = "decide"
model_id = "antisemitism-v2"
artifact_uri = "{PROD_PREFIX}/{artifact}.apr"
artifact_sha256 = "{artifact}"
labels = ["not_antisemitic", "antisemitic"]

[records]
pack_outcome_uri = "{PROD_PREFIX}/{artifact}/pack-outcome.json"
eval_jsonl_uri   = "{PROD_PREFIX}/{artifact}/eval.jsonl"
eval_probs_uri   = "{PROD_PREFIX}/{artifact}/eval-probs.json"
staging_verification_uri = "{PROD_PREFIX}/{artifact}/staging-verification.json"
comparison_uri   = "{PROD_PREFIX}/{artifact}/comparison.json"
verification_prefix = "{PROD_PREFIX}/{artifact}/verifications/"

[publish]
{kind_line}reason = "Experiment 3 ended 2026-11-20; verdict clearly_better on recall at the 0.95 precision floor."
baseline_sha256 = "{baseline}"

[tools]
verify_required_role = "admin"
verify_allowed_principals = ["3k9f2m2mclientid"]
"#
    )
}
