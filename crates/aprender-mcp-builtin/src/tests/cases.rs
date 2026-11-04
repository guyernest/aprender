//! Fixtures and the case table: the three §3.1 configurations as positives, then one-edit
//! negative controls for every §5.2 code. Ported from the platform's spike 036
//! (`sources/036-precheck-rules-offline/precheck/src/cases.rs`), which proved 45/45 offline.

use crate::precheck::Env;
use crate::store::MemStore;

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

/// The fork's fixture shape (`crates/aprender-mcp-laya-train/fixtures/pack-outcome-v1.json`).
pub fn pack_record(sha: &str) -> String {
    format!(
        r#"{{
  "schema": "laya-train-pack-outcome-v1",
  "task_id": "00000000-0000-4000-8000-000000000001",
  "pack_image_digest": "sha256:f59060360430b53654f4bd5d0734a13efe9bd0fbf952aa3fe460805f254815a9",
  "gate_contract_sha256": "bc5b89030053bc62e4a406091f6b1f4e41899107fd7271faf919be90e1838a8f",
  "parity_contract_sha256": "0d9e492d248c3df90f33a5ed4563357797f911fbdfd7bf8504593f3c2eea99c2",
  "ran": true, "skipped": null, "exit_code": 0, "accepted": true,
  "apr_sha256": "{sha}",
  "shipped_seed": 20260927, "waived": null,
  "packed_line": "PACKED waived=none",
  "verify_line": "{{\"deploy_eligible\":true,\"waived\":null}}",
  "deploy_eligible": true, "pack_seconds": 41.25, "verify_seconds": 87.5
}}"#
    )
}

pub const EVAL_JSONL: &str =
    "{\"text\":\"a\",\"label\":0}\n{\"text\":\"b\",\"label\":1}\n{\"text\":\"c\",\"label\":0}\n";
pub const EVAL_PROBS: &str =
    "{\"schema\":\"laya-eval-probs/v1\",\"rows\":[[0.9,0.1],[0.2,0.8],[0.7,0.3]]}";

pub fn comparison(candidate: &str, baseline: &str, verdict: &str) -> String {
    format!(
        r#"{{ "schema": "laya-compare/v1", "candidate_sha256": "{candidate}", "baseline_sha256": "{baseline}",
  "eval_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
  "positive_label": "antisemitic", "cost_rule": "false_flag_worse", "verdict": "{verdict}",
  "metric": "recall_at_precision_floor", "candidate": 0.620, "baseline": 0.551, "difference_interval_90": [0.012, 0.126] }}"#
    )
}

pub fn verification(sha: &str, pass: bool, environment: &str) -> String {
    format!(
        r#"{{ "schema": "aprender-verification/v1", "server": "antisemitism-staging", "environment": "{environment}",
  "artifact_sha256": "{sha}", "model_id": "antisemitism-v2", "rows_scored": 8, "max_abs_diff": 2.03e-6,
  "tolerance_abs": 5.0e-5, "pass": {pass}, "verified_at": "2026-11-20T10:05:40Z", "verified_by": "analyst@example.org",
  "build_id": "pmcp-aprender-builder:3f9a" }}"#
    )
}

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

/// Every object every positive case needs, so a negative control is one removal/edit.
pub fn base_store() -> MemStore {
    let s = MemStore::default();
    s.put(
        &format!("{STAGING_PREFIX}/antisemitism-v2.apr"),
        vec![0u8; 1024],
    );
    s.declare_size(
        &format!("{STAGING_PREFIX}/antisemitism-v2.apr"),
        846_220_868,
    );
    s.put(
        &format!("{STAGING_PREFIX}/pack-outcome.json"),
        pack_record(A),
    );
    s.put(&format!("{STAGING_PREFIX}/data/eval.jsonl"), EVAL_JSONL);
    s.put(&format!("{STAGING_PREFIX}/run/eval-probs.json"), EVAL_PROBS);
    for sha in [A, B] {
        s.put(&format!("{PROD_PREFIX}/{sha}.apr"), vec![0u8; 1024]);
        s.declare_size(&format!("{PROD_PREFIX}/{sha}.apr"), 846_220_868);
        s.put(
            &format!("{PROD_PREFIX}/{sha}/pack-outcome.json"),
            pack_record(sha),
        );
        s.put(&format!("{PROD_PREFIX}/{sha}/eval.jsonl"), EVAL_JSONL);
        s.put(&format!("{PROD_PREFIX}/{sha}/eval-probs.json"), EVAL_PROBS);
        s.put(
            &format!("{PROD_PREFIX}/{sha}/staging-verification.json"),
            verification(sha, true, "staging"),
        );
    }
    s.put(
        &format!("{PROD_PREFIX}/{A}/comparison.json"),
        comparison(A, B, "clearly_better"),
    );
    s
}

pub struct Case {
    pub name: &'static str,
    pub what: &'static str,
    pub expected_id: &'static str,
    pub config: String,
    pub store: MemStore,
    pub env: Env,
    pub expect_exit: i32,
    pub expect_code: Option<&'static str>,
}

fn env(previous: Option<&str>, history: Option<&[&str]>) -> Env {
    Env {
        previous_artifact_sha256: previous.map(str::to_string),
        published_sha256_history: history.map(|h| h.iter().map(|s| (*s).to_string()).collect()),
        build_id: Some("codebuild:spike-036".into()),
    }
}

fn with_store(edit: impl FnOnce(&MemStore)) -> MemStore {
    let s = base_store();
    edit(&s);
    s
}

#[allow(clippy::too_many_lines)]
pub fn all() -> Vec<Case> {
    let sp = STAGING_PREFIX;
    let pp = PROD_PREFIX;
    let mut v: Vec<Case> = Vec::new();
    let mut add = |name: &'static str,
                   what: &'static str,
                   expected_id: &'static str,
                   config: String,
                   store: MemStore,
                   env: Env,
                   exit: i32,
                   code: Option<&'static str>| {
        v.push(Case {
            name,
            what,
            expected_id,
            config,
            store,
            env,
            expect_exit: exit,
            expect_code: code,
        });
    };

    // ---- positives (the three §3.1 examples and the first-build posture) ----
    add(
        "P1",
        "staging, first build (no previous sha)",
        SERVER_ID,
        staging_config(),
        base_store(),
        env(None, None),
        0,
        None,
    );
    add(
        "P2",
        "prod first_model, first build",
        PROD_ID,
        prod_config("first_model", "promote", A),
        base_store(),
        env(None, None),
        0,
        None,
    );
    add(
        "P3",
        "prod promote A over B with comparison + staging verification",
        PROD_ID,
        prod_config(B, "promote", A),
        base_store(),
        env(Some(B), Some(&[B])),
        0,
        None,
    );
    add(
        "P4",
        "prod rollback to B from A, history known",
        PROD_ID,
        prod_config(A, "rollback", B),
        base_store(),
        env(Some(A), Some(&[B, A])),
        0,
        None,
    );
    add(
        "P5",
        "prod rollback without history → WARN, still eligible",
        PROD_ID,
        prod_config(A, "rollback", B),
        base_store(),
        env(Some(A), None),
        0,
        None,
    );
    add(
        "P6",
        "hold present, same sha republished (reason change only)",
        SERVER_ID,
        staging_config() + "\n[hold]\nreason = \"experiment running\"\n",
        base_store(),
        env(Some(A), None),
        0,
        None,
    );
    add(
        "P7",
        "hold present, new sha, clears_hold = true",
        SERVER_ID,
        staging_config().replace("[publish]\n", "[publish]\nclears_hold = true\n")
            + "\n[hold]\nreason = \"experiment running\"\n",
        base_store(),
        env(Some(B), None),
        0,
        None,
    );
    add(
        "P8",
        "comparison verdict 'worse' is shown, not enforced",
        PROD_ID,
        prod_config(B, "promote", A),
        with_store(|s| {
            s.put(
                &format!("{pp}/{A}/comparison.json"),
                comparison(A, B, "worse"),
            )
        }),
        env(Some(B), None),
        0,
        None,
    );

    // ---- config-invalid (exit 3) ----
    add(
        "N01",
        "unknown key",
        SERVER_ID,
        staging_config().replace("[model]\n", "[model]\nmemory_mb = 10240\n"),
        base_store(),
        env(None, None),
        3,
        Some("config-invalid"),
    );
    add(
        "N02",
        "unknown section",
        SERVER_ID,
        staging_config() + "\n[bounds]\nmax_texts = 4\n",
        base_store(),
        env(None, None),
        3,
        Some("config-invalid"),
    );
    add(
        "N03",
        "schema v2",
        SERVER_ID,
        staging_config().replace("aprender-server/v1", "aprender-server/v2"),
        base_store(),
        env(None, None),
        3,
        Some("config-invalid"),
    );
    add(
        "N04",
        "environment = test",
        SERVER_ID,
        staging_config().replace("environment = \"staging\"", "environment = \"test\""),
        base_store(),
        env(None, None),
        3,
        Some("config-invalid"),
    );
    add(
        "N05",
        "kind = setfit",
        SERVER_ID,
        staging_config().replace("kind = \"decide\"", "kind = \"setfit\""),
        base_store(),
        env(None, None),
        3,
        Some("config-invalid"),
    );
    add(
        "N06",
        "sha uppercase",
        SERVER_ID,
        staging_config().replace(
            &format!("artifact_sha256 = \"{A}\""),
            &format!("artifact_sha256 = \"{}\"", A.to_uppercase()),
        ),
        base_store(),
        env(None, None),
        3,
        Some("config-invalid"),
    );
    add(
        "N07",
        "id differs from SERVER_ID (hand edit)",
        "antisemitism-other",
        staging_config(),
        base_store(),
        env(None, None),
        3,
        Some("config-invalid"),
    );
    add(
        "N08",
        "prod without baseline_sha256",
        PROD_ID,
        prod_config("first_model", "promote", A).replace("baseline_sha256 = \"first_model\"\n", ""),
        base_store(),
        env(None, None),
        3,
        Some("config-invalid"),
    );
    add(
        "N09",
        "verify tolerance above the ceiling",
        SERVER_ID,
        staging_config() + "\n[verify]\ntolerance_abs = 2.0e-3\n",
        base_store(),
        env(None, None),
        3,
        Some("config-invalid"),
    );

    // ---- the records (exit 2) ----
    add(
        "N10",
        "artifact object absent",
        SERVER_ID,
        staging_config(),
        with_store(|s| s.remove(&format!("{sp}/antisemitism-v2.apr"))),
        env(None, None),
        2,
        Some("artifact-missing"),
    );
    add(
        "N11",
        "artifact over the cap",
        SERVER_ID,
        staging_config(),
        with_store(|s| s.declare_size(&format!("{sp}/antisemitism-v2.apr"), 1_342_177_281)),
        env(None, None),
        2,
        Some("artifact-too-large"),
    );
    add(
        "N12",
        "pack record absent",
        SERVER_ID,
        staging_config(),
        with_store(|s| s.remove(&format!("{sp}/pack-outcome.json"))),
        env(None, None),
        2,
        Some("pack-record-missing"),
    );
    add(
        "N13",
        "pack record over 1 MiB",
        SERVER_ID,
        staging_config(),
        with_store(|s| s.declare_size(&format!("{sp}/pack-outcome.json"), 1_048_577)),
        env(None, None),
        2,
        Some("pack-record-missing"),
    );
    add(
        "N14",
        "pack record not JSON",
        SERVER_ID,
        staging_config(),
        with_store(|s| s.put(&format!("{sp}/pack-outcome.json"), "PACKED waived=none")),
        env(None, None),
        2,
        Some("pack-record-schema"),
    );
    add(
        "N15",
        "pack record schema v0",
        SERVER_ID,
        staging_config(),
        with_store(|s| {
            s.put(
                &format!("{sp}/pack-outcome.json"),
                pack_record(A).replace("laya-train-pack-outcome-v1", "laya-train-pack-outcome-v0"),
            )
        }),
        env(None, None),
        2,
        Some("pack-record-schema"),
    );
    add(
        "N16",
        "pack accepted = false",
        SERVER_ID,
        staging_config(),
        with_store(|s| {
            s.put(
                &format!("{sp}/pack-outcome.json"),
                pack_record(A).replace("\"accepted\": true", "\"accepted\": false"),
            )
        }),
        env(None, None),
        2,
        Some("pack-record-not-eligible"),
    );
    add(
        "N17",
        "pack deploy_eligible = false",
        SERVER_ID,
        staging_config(),
        with_store(|s| {
            s.put(
                &format!("{sp}/pack-outcome.json"),
                pack_record(A).replace("\"deploy_eligible\": true", "\"deploy_eligible\": false"),
            )
        }),
        env(None, None),
        2,
        Some("pack-record-not-eligible"),
    );
    add(
        "N18",
        "pack apr_sha256 is another model",
        SERVER_ID,
        staging_config(),
        with_store(|s| s.put(&format!("{sp}/pack-outcome.json"), pack_record(B))),
        env(None, None),
        2,
        Some("pack-record-sha-mismatch"),
    );
    add(
        "N19",
        "eval.jsonl absent",
        SERVER_ID,
        staging_config(),
        with_store(|s| s.remove(&format!("{sp}/data/eval.jsonl"))),
        env(None, None),
        2,
        Some("eval-missing"),
    );
    add(
        "N20",
        "eval-probs over 32 MiB",
        SERVER_ID,
        staging_config(),
        with_store(|s| s.declare_size(&format!("{sp}/run/eval-probs.json"), 32 * 1024 * 1024 + 1)),
        env(None, None),
        2,
        Some("eval-missing"),
    );
    add(
        "N21",
        "reason empty",
        SERVER_ID,
        staging_config().replace(
            "reason = \"Phase 16 SageMaker control model; staging for the next experiment.\"",
            "reason = \"   \"",
        ),
        base_store(),
        env(None, None),
        2,
        Some("reason-missing"),
    );

    // ---- hold / baseline / comparison / verification / rollback ----
    add(
        "N22",
        "hold present, new sha, previous known, no clears_hold",
        SERVER_ID,
        staging_config() + "\n[hold]\nreason = \"experiment running\"\n",
        base_store(),
        env(Some(B), None),
        2,
        Some("hold-active"),
    );
    add(
        "N23",
        "prod baseline is not the previous sha",
        PROD_ID,
        prod_config(C, "promote", A),
        base_store(),
        env(Some(B), None),
        2,
        Some("baseline-mismatch"),
    );
    add(
        "N24",
        "prod first_model while a previous sha exists",
        PROD_ID,
        prod_config("first_model", "promote", A),
        base_store(),
        env(Some(B), None),
        2,
        Some("baseline-mismatch"),
    );
    add(
        "N25",
        "comparison_uri not set",
        PROD_ID,
        prod_config(B, "promote", A).replace(
            &format!("comparison_uri   = \"{pp}/{A}/comparison.json\"\n"),
            "",
        ),
        base_store(),
        env(Some(B), None),
        2,
        Some("comparison-missing"),
    );
    add(
        "N26",
        "comparison object absent",
        PROD_ID,
        prod_config(B, "promote", A),
        with_store(|s| s.remove(&format!("{pp}/{A}/comparison.json"))),
        env(Some(B), None),
        2,
        Some("comparison-missing"),
    );
    add(
        "N27",
        "comparison candidate is another sha",
        PROD_ID,
        prod_config(B, "promote", A),
        with_store(|s| {
            s.put(
                &format!("{pp}/{A}/comparison.json"),
                comparison(C, B, "clearly_better"),
            )
        }),
        env(Some(B), None),
        2,
        Some("comparison-sha-mismatch"),
    );
    add(
        "N28",
        "comparison baseline is another sha",
        PROD_ID,
        prod_config(B, "promote", A),
        with_store(|s| {
            s.put(
                &format!("{pp}/{A}/comparison.json"),
                comparison(A, C, "clearly_better"),
            )
        }),
        env(Some(B), None),
        2,
        Some("comparison-sha-mismatch"),
    );
    add(
        "N29",
        "staging_verification_uri not set",
        PROD_ID,
        prod_config(B, "promote", A).replace(
            &format!("staging_verification_uri = \"{pp}/{A}/staging-verification.json\"\n"),
            "",
        ),
        base_store(),
        env(Some(B), None),
        2,
        Some("staging-verification-missing"),
    );
    add(
        "N30",
        "staging verification object absent",
        PROD_ID,
        prod_config(B, "promote", A),
        with_store(|s| s.remove(&format!("{pp}/{A}/staging-verification.json"))),
        env(Some(B), None),
        2,
        Some("staging-verification-missing"),
    );
    add(
        "N31",
        "staging verification pass = false",
        PROD_ID,
        prod_config(B, "promote", A),
        with_store(|s| {
            s.put(
                &format!("{pp}/{A}/staging-verification.json"),
                verification(A, false, "staging"),
            )
        }),
        env(Some(B), None),
        2,
        Some("staging-verification-failed"),
    );
    add(
        "N32",
        "staging verification written by a prod server",
        PROD_ID,
        prod_config(B, "promote", A),
        with_store(|s| {
            s.put(
                &format!("{pp}/{A}/staging-verification.json"),
                verification(A, true, "prod"),
            )
        }),
        env(Some(B), None),
        2,
        Some("staging-verification-failed"),
    );
    add(
        "N33",
        "staging verification for another sha",
        PROD_ID,
        prod_config(B, "promote", A),
        with_store(|s| {
            s.put(
                &format!("{pp}/{A}/staging-verification.json"),
                verification(C, true, "staging"),
            )
        }),
        env(Some(B), None),
        2,
        Some("staging-verification-failed"),
    );
    add(
        "N34",
        "rollback to a sha never published (history known)",
        PROD_ID,
        prod_config(A, "rollback", B),
        base_store(),
        env(Some(A), Some(&[A])),
        2,
        Some("rollback-unknown-sha"),
    );
    add(
        "N35",
        "rollback as the first publish",
        PROD_ID,
        prod_config(A, "rollback", B),
        base_store(),
        env(None, None),
        2,
        Some("rollback-first-publish"),
    );

    // ---- store failure (exit 4) ----
    add(
        "N36",
        "S3 unreachable on the artifact HEAD",
        SERVER_ID,
        staging_config(),
        with_store(|s| s.fail(&format!("{sp}/antisemitism-v2.apr"))),
        env(None, None),
        4,
        Some("s3-unreachable"),
    );
    add(
        "N37",
        "S3 unreachable on the pack record GET",
        SERVER_ID,
        staging_config(),
        with_store(|s| s.fail(&format!("{sp}/pack-outcome.json"))),
        env(None, None),
        4,
        Some("s3-unreachable"),
    );
    v
}
