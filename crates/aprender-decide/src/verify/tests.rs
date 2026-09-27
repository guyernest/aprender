//! verify.rs over private copies of the tiny Laya fixture (plan 08-02).
//!
//! Every negative is INDUCED in a copy's inputs — never by weakening a check — and every
//! file the report hashes is re-hashed after the edit, so only the rule under test can
//! refuse. The fixture's fine-tuned and zero-shot models are the same model (margin 0), so
//! the accept path runs under a TEST-ONLY permissive policy; the contract policy is what
//! `gate_failed_margin_only` and the real vectors run under.

use super::*;
use crate::test_support::{constant_f64, contract_yaml, fixture_dir, tolerance};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read json")).expect("parse json")
}

fn write_json(path: &Path, v: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(v).expect("serialize")).expect("write json");
}

fn sha(path: &Path) -> String {
    sha256_hex(&std::fs::read(path).expect("read for sha"))
}

/// The tiny fixture's own base sha256 (its checkpoint IS its base).
fn tiny_base_sha() -> String {
    sha(&fixture_dir().join("checkpoint/model.safetensors"))
}

/// The CONTRACT policy (laya-finetune-gate-v1 constants + laya-parity-v1 tolerance), with the
/// base swapped for the tiny one so the fixture can reach the gate.
fn contract_policy_tiny_base() -> VerifyPolicy {
    let gate = "laya-finetune-gate-v1.yaml";
    VerifyPolicy {
        min_macro_f1_margin: constant_f64(gate, "gate_min_macro_f1_margin"),
        max_ece: constant_f64(gate, "gate_max_ece"),
        ece_bins: constant_f64(gate, "ece_bins") as u64,
        metric_recompute_abs: constant_f64(gate, "gate_metric_recompute_abs"),
        rescore_probs_abs: tolerance("pack_rescore_probs_abs"),
        calibration_slice_min_per_class: constant_f64(gate, "calibration_slice_min_per_class")
            as u64,
        base_sha256: tiny_base_sha(),
    }
}

/// TEST-ONLY permissive policy: margin 0, max_ece 1, tiny base. The fixture's fine-tuned and
/// zero-shot models are one model, so only this policy lets the accept path run on it.
fn permissive_policy() -> VerifyPolicy {
    VerifyPolicy {
        min_macro_f1_margin: 0.0,
        max_ece: 1.0,
        ..contract_policy_tiny_base()
    }
}

/// A private run dir: the fixture copy, its data dir at `<run>/data`, its base at
/// `<run>/checkpoint`.
struct Run {
    _tmp: TempDir,
    dir: PathBuf,
}

impl Run {
    fn data(&self) -> PathBuf {
        self.dir.join("data")
    }

    fn base(&self) -> PathBuf {
        self.dir.join("checkpoint")
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }

    fn edit_json(&self, rel: &str, edit: impl FnOnce(&mut Value)) {
        let p = self.path(rel);
        let mut v = read_json(&p);
        edit(&mut v);
        write_json(&p, &v);
    }

    /// Recompute every hash the report records (recipe_id, inputs, probability files,
    /// probes), so an edit elsewhere reaches the rule under test.
    fn rehash(&self) {
        let recipe_id = sha(&self.path("recipe.json"));
        let task = sha(&self.data().join("task.json"));
        let train = sha(&self.data().join("train.jsonl"));
        let eval = sha(&self.data().join("eval.jsonl"));
        let ep = sha(&self.path("eval-probs.json"));
        let zp = sha(&self.path("zero-shot-probs.json"));
        let pr = sha(&self.path("probes.json"));
        self.edit_json("gate-report.json", |r| {
            r["recipe_id"] = recipe_id.into();
            r["inputs_sha256"]["task_json"] = task.into();
            r["inputs_sha256"]["train_jsonl"] = train.into();
            r["inputs_sha256"]["eval_jsonl"] = eval.into();
            r["eval_probs_sha256"] = ep.into();
            r["zero_shot_probs_sha256"] = zp.into();
            r["probes_sha256"] = pr.into();
        });
    }

    fn inputs(&self) -> PackInputs {
        PackInputs::from_run_dir(&self.dir, &self.data()).expect("run dir reads")
    }

    fn verify(&self, policy: &VerifyPolicy) -> Result<VerifyReport, VerifyError> {
        let inputs = PackInputs::from_run_dir(&self.dir, &self.data())?;
        let packed = artifact::write_decide_apr(&inputs)?;
        verify_run(&inputs, &packed, &self.data(), &self.base(), policy)
    }
}

/// The fixture exactly as committed (variant synthetic-fixture).
fn synthetic_copy() -> Run {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path().join("run");
    copy_dir(&fixture_dir(), &dir);
    Run { _tmp: tmp, dir }
}

/// A production-variant copy whose report thresholds are `policy`'s and whose reported pass
/// is `pass`, every hash recomputed.
fn production_copy(policy: &VerifyPolicy, pass: bool) -> Run {
    let run = synthetic_copy();
    run.edit_json("recipe.json", |r| r["variant"] = PRODUCTION_VARIANT.into());
    run.edit_json("gate-report.json", |r| {
        r["thresholds"]["min_macro_f1_margin"] = policy.min_macro_f1_margin.into();
        r["thresholds"]["max_ece"] = policy.max_ece.into();
        r["thresholds"]["ece_bins"] = policy.ece_bins.into();
        r["pass"] = pass.into();
    });
    run.rehash();
    run
}

/// The accept path, end to end, on the tiny fixture (the tracer).
#[test]
fn tiny_verify_roundtrip() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let report = run.verify(&policy).expect("the production copy verifies");
    assert!(report.deploy_eligible);
    assert_eq!(report.n, 9);
    assert_eq!(report.argmax_agree, 9, "argmax exact on every eval row");
    assert!(
        within(report.rescore_max_abs, policy.rescore_probs_abs),
        "{report:?}"
    );
    assert!(
        within(report.zs_rescore_max_abs, policy.rescore_probs_abs),
        "{report:?}"
    );
    let reported = read_json(&run.path("gate-report.json"));
    let ece = reported["fine_tuned"]["ece_post"]
        .as_f64()
        .expect("ece_post");
    assert!(within(
        (report.recomputed.ece_post - ece).abs(),
        policy.metric_recompute_abs
    ));
    assert!(
        within(report.recomputed.margin.abs(), 0.0),
        "one model on both sides"
    );
    let packed = artifact::write_decide_apr(&run.inputs()).expect("pack");
    assert_eq!(
        report.artifact_sha256,
        artifact::artifact_sha256_hex(&packed)
    );
    // The contract file's own base is the real en-root, never the tiny one.
    let real = contract_yaml("laya-finetune-gate-v1.yaml")["base"]["model_safetensors_sha256"]
        .as_str()
        .expect("contract base sha")
        .to_string();
    assert_ne!(real, policy.base_sha256);
}

// ===========================================================================
// Task 2: every refusal, induced in a copy's inputs (plan 08-09)
// ===========================================================================

fn append_line(path: &Path, line: &str) {
    let mut text = std::fs::read_to_string(path).expect("read jsonl");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line);
    text.push('\n');
    std::fs::write(path, text).expect("write jsonl");
}

fn row_json(text: &str, label: &str) -> String {
    let mut row = serde_json::Map::new();
    row.insert("text".into(), text.into());
    row.insert("label".into(), label.into());
    Value::Object(row).to_string()
}

/// Line `i` of a jsonl file as `(text, label)`.
fn jsonl_row(path: &Path, i: usize) -> (String, String) {
    let text = std::fs::read_to_string(path).expect("read jsonl");
    let v: Value = serde_json::from_str(text.lines().nth(i).expect("row")).expect("row json");
    (
        v["text"].as_str().expect("text").to_string(),
        v["label"].as_str().expect("label").to_string(),
    )
}

/// Shift probability `(row, col)` by `+d` and `(row, (col+1)%K)` by `-d`, so the row sum is kept.
fn shift_prob(run: &Run, file: &str, row: usize, col: usize, d: f64) {
    run.edit_json(file, |v| {
        let p = &mut v["rows"][row]["probabilities"];
        let k = p.as_array().expect("probs").len();
        let a = p[col].as_f64().expect("p");
        let b = p[(col + 1) % k].as_f64().expect("p");
        p[col] = (a + d).into();
        p[(col + 1) % k] = (b - d).into();
    });
}

fn expect_err(r: Result<VerifyReport, VerifyError>) -> VerifyError {
    match r {
        Ok(rep) => panic!("expected a refusal, got {rep:?}"),
        Err(e) => e,
    }
}

#[test]
fn synthetic_refused() {
    let run = synthetic_copy();
    let e = expect_err(run.verify(&permissive_policy()));
    assert!(
        matches!(&e, VerifyError::SyntheticNotDeployable { variant } if variant == SYNTHETIC_FIXTURE_VARIANT),
        "{e:?}"
    );
    assert_eq!(e.exit_code(), 2);
}

#[test]
fn base_mismatch_base_dir() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let other = run.path("other-base");
    copy_dir(&run.base(), &other);
    let st = other.join("model.safetensors");
    let mut bytes = std::fs::read(&st).expect("read base");
    bytes.push(0);
    std::fs::write(&st, bytes).expect("write base");
    let inputs = run.inputs();
    let packed = artifact::write_decide_apr(&inputs).expect("pack");
    let e = expect_err(verify_run(&inputs, &packed, &run.data(), &other, &policy));
    assert!(
        matches!(
            e,
            VerifyError::BaseMismatch {
                which: BaseWhich::BaseDir,
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn base_mismatch_contract() {
    // The policy carries the CONTRACT's real en-root base; the fixture declares the tiny one.
    let real = contract_yaml("laya-finetune-gate-v1.yaml")["base"]["model_safetensors_sha256"]
        .as_str()
        .expect("contract base sha")
        .to_string();
    let policy = VerifyPolicy {
        base_sha256: real,
        ..permissive_policy()
    };
    let run = production_copy(&policy, true);
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::BaseMismatch {
                which: BaseWhich::Contract,
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn input_hash_mismatch() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let p = run.data().join("eval.jsonl");
    let mut bytes = std::fs::read(&p).expect("read eval");
    let i = bytes
        .iter()
        .position(|b| *b == b'T')
        .expect("a 'T' in eval.jsonl");
    bytes[i] = b't';
    std::fs::write(&p, bytes).expect("write eval");
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::InputHashMismatch {
                file: "eval_jsonl",
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn split_overlap() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let (text, label) = jsonl_row(&run.data().join("eval.jsonl"), 1);
    append_line(&run.data().join("train.jsonl"), &row_json(&text, &label));
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::SplitOverlap {
                eval_row: 1,
                train_row: 12
            }
        ),
        "{e:?}"
    );
}

#[test]
fn conflicting_labels() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let (text, label) = jsonl_row(&run.data().join("train.jsonl"), 1);
    assert_eq!(label, "shipping");
    // Same normalized text (extra inner and outer whitespace), a different label.
    let spaced = format!("  {}  ", text.replacen(' ', "   ", 1));
    assert_ne!(spaced, text);
    append_line(
        &run.data().join("train.jsonl"),
        &row_json(&spaced, "billing"),
    );
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(&e, VerifyError::ConflictingLabels { rows } if rows == &vec![1, 12]),
        "{e:?}"
    );
}

#[test]
fn slice_splits_group() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    // Row 0 is in the slice; a same-label copy appended as row 12 lands in fit.
    let (text, label) = jsonl_row(&run.data().join("train.jsonl"), 0);
    let ids = read_json(&run.path("gate-report.json"))["calibration"]["slice_ids"].clone();
    assert_eq!(ids[0], 0, "row 0 is a slice row");
    append_line(
        &run.data().join("train.jsonl"),
        &row_json(&format!("{text} "), &label),
    );
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(&e, VerifyError::SliceInvalid { why } if why.contains("split between calibration and fit")),
        "{e:?}"
    );
}

#[test]
fn probs_missing_row() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    run.edit_json("eval-probs.json", |v| {
        v["rows"].as_array_mut().expect("rows").pop();
    });
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::ProbsRowCoverage {
                which: ProbsWhich::FineTuned,
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn probs_duplicate_index() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    run.edit_json("eval-probs.json", |v| v["rows"][1]["row"] = 0.into());
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(&e, VerifyError::ProbsRowCoverage { which: ProbsWhich::FineTuned, why } if why.contains("appears twice")),
        "{e:?}"
    );
}

#[test]
fn probs_row_sum() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    run.edit_json("eval-probs.json", |v| {
        let p = &mut v["rows"][2]["probabilities"][0];
        *p = (p.as_f64().expect("p") + 0.01).into();
    });
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(&e, VerifyError::ProbsInvalid { which: ProbsWhich::FineTuned, row: Some(2), why } if why.contains("sum")),
        "{e:?}"
    );
}

#[test]
fn probs_nan() {
    // Python's json.dump writes a bare `NaN` token; that file must be refused, never read.
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let p = run.path("eval-probs.json");
    let v = read_json(&p);
    let first = v["rows"][0]["probabilities"][0].to_string();
    let text = std::fs::read_to_string(&p).expect("read probs");
    assert!(text.contains(&first));
    std::fs::write(&p, text.replacen(&first, "NaN", 1)).expect("write probs");
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::ProbsInvalid {
                which: ProbsWhich::FineTuned,
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn rescore_drift_fine_tuned() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    shift_prob(&run, "eval-probs.json", 3, 0, 2e-5);
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::RescoreDrift {
                which: ProbsWhich::FineTuned,
                row: 3,
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn rescore_drift_zero_shot() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    shift_prob(&run, "zero-shot-probs.json", 5, 1, 2e-5);
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::RescoreDrift {
                which: ProbsWhich::ZeroShot,
                row: 5,
                ..
            }
        ),
        "{e:?}"
    );
}

/// The review's forgery: a FAILED report edited to pass (fine-tuned macro-F1 and margin
/// raised by 0.2, pass true), every hash recomputed, under the CONTRACT thresholds.
#[test]
fn forged_report() {
    let policy = contract_policy_tiny_base();
    let run = production_copy(&policy, true);
    run.edit_json("gate-report.json", |r| {
        let f1 = r["fine_tuned"]["macro_f1"].as_f64().expect("f1");
        let m = r["margin"].as_f64().expect("margin");
        r["fine_tuned"]["macro_f1"] = (f1 + 0.2).into();
        r["margin"] = (m + 0.2).into();
    });
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::ReportedMetricMismatch {
                field: "fine_tuned.macro_f1",
                ..
            }
        ),
        "{e:?}"
    );
}

/// A worse baseline inflates the margin: zero-shot-probs.json rewritten to predict the wrong
/// class with confidence, its hash (and the report's zero-shot F1) updated.
#[test]
fn forged_baseline() {
    let policy = contract_policy_tiny_base();
    let run = production_copy(&policy, true);
    let labels: Vec<usize> = std::fs::read_to_string(run.data().join("eval.jsonl"))
        .expect("eval")
        .lines()
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("row");
            ["shipping", "billing", "account"]
                .iter()
                .position(|x| *x == v["label"].as_str().expect("label"))
                .expect("label index")
        })
        .collect();
    run.edit_json("zero-shot-probs.json", |v| {
        for (i, row) in v["rows"]
            .as_array_mut()
            .expect("rows")
            .iter_mut()
            .enumerate()
        {
            let mut p = vec![0.1, 0.1, 0.1];
            p[(labels[i] + 1) % 3] = 0.8;
            row["probabilities"] = Value::from(p);
        }
    });
    run.edit_json("gate-report.json", |r| {
        r["zero_shot"]["macro_f1"] = 0.0.into();
        let ft = r["fine_tuned"]["macro_f1"].as_f64().expect("ft");
        r["margin"] = ft.into();
    });
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::RescoreDrift {
                which: ProbsWhich::ZeroShot,
                row: 0,
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn threshold_mismatch() {
    // The report carries the permissive thresholds; the verifier holds the contract's.
    let run = production_copy(&permissive_policy(), true);
    let e = expect_err(run.verify(&contract_policy_tiny_base()));
    assert!(
        matches!(
            e,
            VerifyError::ThresholdMismatch {
                field: "min_macro_f1_margin",
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn pass_disagrees() {
    let policy = contract_policy_tiny_base();
    let run = production_copy(&policy, true);
    let e = expect_err(run.verify(&policy));
    assert_eq!(
        e,
        VerifyError::PassDisagrees {
            reported: true,
            recomputed: false
        }
    );
}

/// The clause set discriminates: under the contract thresholds the tiny copy fails on the
/// margin ALONE (ft == zs, margin 0; its ece_post 0.008 passes).
#[test]
fn gate_failed_margin_only() {
    let policy = contract_policy_tiny_base();
    let run = production_copy(&policy, false);
    let e = expect_err(run.verify(&policy));
    assert_eq!(e.exit_code(), 3);
    let VerifyError::GateFailed(g) = e else {
        panic!("expected GateFailed, got {e:?}");
    };
    assert_eq!(g.clauses, vec![GateClause::Margin]);
    assert_eq!(g.argmax_agree, g.n);
    assert!(within(g.recomputed.ece_post, policy.max_ece));
    assert!(g.recomputed.margin < policy.min_macro_f1_margin);
    assert_eq!(g.artifact_sha256.len(), 64);
}

#[test]
fn pack_for_serving_writes_nothing() {
    let policy = contract_policy_tiny_base();
    let run = production_copy(&policy, false);
    let out_dir = TempDir::new().expect("out dir");
    let out = out_dir.path().join("v.apr");
    let e = expect_err(pack_for_serving(
        &run.dir,
        &run.data(),
        &run.base(),
        &out,
        &policy,
    ));
    assert!(
        matches!(&e, VerifyError::GateFailed(g) if g.clauses == vec![GateClause::Margin]),
        "{e:?}"
    );
    let left: Vec<_> = std::fs::read_dir(out_dir.path())
        .expect("read out dir")
        .collect();
    assert!(left.is_empty(), "nothing may be written: {left:?}");
}

/// FALSIFY-LAYA-GATE-003 (Rust half): the recomputed ECE IS the house top-label ECE, and it
/// matches every frozen reference value within gate_metric_recompute_abs.
#[test]
fn recompute_matches_house_ece_cases() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/setfit_fixtures/claims_stats/ece_top_label_cases.json");
    let cases = read_json(&path)["cases"].clone();
    let cases = cases.as_array().expect("cases");
    assert!(cases.len() >= 5, "frozen cases present");
    let tol = constant_f64("laya-finetune-gate-v1.yaml", "gate_metric_recompute_abs");
    for c in cases {
        let id = c["id"].as_str().expect("id");
        let k = c["n_classes"].as_u64().expect("k") as usize;
        let bins = c["n_bins"].as_u64().expect("bins") as usize;
        let probs: Vec<Vec<f32>> = c["probabilities"]
            .as_array()
            .expect("probs")
            .iter()
            .map(|r| {
                r.as_array()
                    .expect("row")
                    .iter()
                    .map(|p| p.as_f64().expect("p") as f32)
                    .collect()
            })
            .collect();
        let labels: Vec<usize> = c["labels"]
            .as_array()
            .expect("labels")
            .iter()
            .map(|l| l.as_u64().expect("label") as usize)
            .collect();
        let m = recompute_metrics(&probs, &labels, k, bins);
        let flat: Vec<f32> = probs.iter().flatten().copied().collect();
        let house = f64::from(expected_calibration_error_top_label(
            &flat, k, &labels, bins,
        ));
        assert_eq!(m.ece.to_bits(), house.to_bits(), "{id}: not the house ECE");
        let want = c["ece"].as_f64().expect("ece");
        assert!(
            within((m.ece - want).abs(), tol),
            "{id}: {} vs frozen {want}",
            m.ece
        );
    }
}

#[test]
fn fixture_bytes_refuses_production() {
    let run = production_copy(&permissive_policy(), true);
    let e = fixture_bytes(&run.dir, &run.data()).expect_err("production is not a fixture");
    assert!(
        matches!(&e, VerifyError::NotSyntheticFixture { variant } if variant == PRODUCTION_VARIANT),
        "{e:?}"
    );
}

#[test]
fn fixture_bytes_matches_golden() {
    let golden = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/laya_tiny.apr.sha256"),
    )
    .expect("golden sha");
    let golden = golden.split_whitespace().next().expect("golden hex");
    let bytes = fixture_bytes(&fixture_dir(), &fixture_dir().join("data")).expect("fixture packs");
    assert_eq!(artifact::artifact_sha256_hex(&bytes), golden);
}

// ===========================================================================
// Beyond the plan's list: the remaining plan-named variant and the exact-file path
// ===========================================================================

/// `ArgmaxDrift`: a near-tie row whose probabilities agree within tol but whose argmax flips.
#[test]
fn argmax_drift() {
    let texts = vec!["a".to_string(), "b".to_string()];
    let file = vec![vec![0.5f32, 0.499_995, 0.000_005], vec![0.2, 0.7, 0.1]];
    let served = vec![vec![0.499_995f32, 0.5, 0.000_005], vec![0.2, 0.7, 0.1]];
    let e = rescore(
        ProbsWhich::FineTuned,
        |_| {
            Ok(served
                .iter()
                .map(|p| Decision {
                    label_index: argmax(p),
                    probabilities: p.clone(),
                    tokens: 1,
                    truncated: false,
                })
                .collect())
        },
        &texts,
        &file,
        1e-5,
    )
    .expect_err("argmax flips");
    assert_eq!(
        e,
        VerifyError::ArgmaxDrift {
            which: ProbsWhich::FineTuned,
            row: 0
        }
    );
}

/// `verify_path` loads the EXACT file and accepts it under the permissive policy; the sha it
/// reports is the file's.
#[test]
fn verify_path_accepts_exact_file() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let bytes = artifact::write_decide_apr(&run.inputs()).expect("pack");
    let out = TempDir::new().expect("tempdir");
    let apr = out.path().join("v.apr");
    std::fs::write(&apr, &bytes).expect("write apr");
    let report =
        verify_path(&apr, &run.dir, &run.data(), &run.base(), &policy).expect("the file verifies");
    assert!(report.deploy_eligible);
    assert_eq!(
        report.artifact_sha256,
        artifact::artifact_sha256_hex(&bytes)
    );
}

/// `verify_path` refuses a file whose manifest describes another run.
#[test]
fn verify_path_manifest_mismatch() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let bytes = fixture_bytes(&fixture_dir(), &fixture_dir().join("data")).expect("fixture");
    let out = TempDir::new().expect("tempdir");
    let apr = out.path().join("fixture.apr");
    std::fs::write(&apr, &bytes).expect("write apr");
    let e = expect_err(verify_path(
        &apr,
        &run.dir,
        &run.data(),
        &run.base(),
        &policy,
    ));
    assert_eq!(e, VerifyError::ManifestMismatch { field: "recipe_id" });
}
