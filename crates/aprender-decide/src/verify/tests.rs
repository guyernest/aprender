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
