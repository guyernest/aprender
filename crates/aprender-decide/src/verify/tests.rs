//! verify.rs over private copies of the tiny Laya fixture (plan 08-02).
//!
//! Every negative is INDUCED in a copy's inputs — never by weakening a check — and every
//! file the report hashes is re-hashed after the edit, so only the rule under test can
//! refuse. The fixture's fine-tuned and zero-shot models are the same model (margin 0), so
//! the accept path runs under a TEST-ONLY permissive policy; the contract policy is what
//! `gate_failed_margin_only` and the real vectors run under.

use super::*;
use crate::test_support::{constant_f64, contract_yaml, fixture_dir};
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

/// `contracts/<name>` parsed into its typed view — the SAME views `examples/pack_laya.rs` and
/// `tests/common` read.
fn view<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../contracts/{name}"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {name}: {e}"));
    serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("parse {name} into its view: {e}"))
}

/// The CONTRACT policy, through the library's one mapping (`from_contract_views`).
fn contract_policy() -> VerifyPolicy {
    VerifyPolicy::from_contract_views(
        &view::<GateContractView>("laya-finetune-gate-v1.yaml"),
        &view::<ParityContractView>("laya-parity-v1.yaml"),
    )
}

/// The tiny fixture's own base block (its checkpoint IS its base), read from its recipe.json,
/// with its base-dir file pins hashed from the fixture checkpoint.
fn tiny_base_pins() -> BasePins {
    let r = read_json(&fixture_dir().join("recipe.json"));
    let b = &r["base"];
    let s = |k: &str| {
        b[k].as_str()
            .unwrap_or_else(|| panic!("base.{k}"))
            .to_string()
    };
    BasePins {
        family: s("family"),
        checkpoint: s("checkpoint"),
        repo: s("repo"),
        revision: s("revision"),
        model_safetensors_sha256: s("sha256"),
        encoder_config_sha256: sha(&fixture_dir().join("checkpoint/encoder/config.json")),
        rl_agent_config_sha256: sha(&fixture_dir().join("checkpoint/rl_agent_config.json")),
        tokenizer_json_sha256: sha(&fixture_dir().join("checkpoint/tokenizer/tokenizer.json")),
    }
}

/// The CONTRACT policy with ONLY the base swapped for the tiny fixture's, so the fixture can
/// reach the gate.
fn contract_policy_tiny_base() -> VerifyPolicy {
    VerifyPolicy {
        base: tiny_base_pins(),
        ..contract_policy()
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
        let noise = self.path("rescore-noise.json");
        let noise = noise.exists().then(|| sha(&noise));
        self.edit_json("gate-report.json", |r| {
            r["recipe_id"] = recipe_id.into();
            r["inputs_sha256"]["task_json"] = task.into();
            r["inputs_sha256"]["train_jsonl"] = train.into();
            r["inputs_sha256"]["eval_jsonl"] = eval.into();
            r["eval_probs_sha256"] = ep.into();
            r["zero_shot_probs_sha256"] = zp.into();
            r["probes_sha256"] = pr.into();
            if let Some(h) = noise {
                r["rescore_noise_sha256"] = h.into();
            }
        });
        self.sync_shift();
        self.sync_seeds();
    }

    /// Re-derive the shift probe's hashes when the copy carries one (the trainer's job).
    fn sync_shift(&self) {
        if !self.path("shift-probs.json").exists() {
            return;
        }
        let (sp, sz) = (
            sha(&self.path("shift-probs.json")),
            sha(&self.path("shift-zero-shot-probs.json")),
        );
        let shift = self.data().join("shift.jsonl");
        let sj = shift.exists().then(|| sha(&shift));
        self.edit_json("gate-report.json", |r| {
            r["shift_probe"]["probs_sha256"] = sp.into();
            r["shift_probe"]["zero_shot_probs_sha256"] = sz.into();
            if let Some(h) = sj {
                r["inputs_sha256"]["shift_jsonl"] = h.into();
            }
        });
    }

    /// Re-derive `seeds.per_seed` the way the trainer writes it: the shipped seed's file IS
    /// eval-probs.json, and every row's metrics, margin (against the current zero-shot file),
    /// pass (the report's thresholds), rank key and hashes are recomputed from its file. A file
    /// the verifier would refuse is left for the verifier to refuse.
    fn sync_seeds(&self) {
        let report = read_json(&self.path("gate-report.json"));
        if report["seeds"]["per_seed"].is_null() {
            return;
        }
        let shipped = report["seeds"]["shipped"].as_i64().expect("shipped");
        std::fs::copy(
            self.path("eval-probs.json"),
            self.path(&format!("seeds/seed-{shipped}/eval-probs.json")),
        )
        .expect("copy the shipped seed's file");
        let Ok(data) = read_data_dir(&self.data()) else {
            return;
        };
        let labels = data.task.owned_labels();
        let y: Vec<usize> = data.eval.iter().map(|r| r.label).collect();
        let k = labels.len();
        let th = &report["thresholds"];
        let (min_margin, max_ece) = (
            th["min_macro_f1_margin"].as_f64().expect("margin"),
            th["max_ece"].as_f64().expect("max_ece"),
        );
        let bins = th["ece_bins"].as_u64().expect("bins") as usize;
        let scale = read_json(&self.path("recipe.json"))["seed_selection"]["rank_scale"]
            .as_f64()
            .expect("rank_scale");
        let Ok(zs) = validate_probs(
            ProbsWhich::ZeroShot,
            &std::fs::read(self.path("zero-shot-probs.json")).expect("zs"),
            &data.eval,
            &labels,
        ) else {
            return;
        };
        let zs_f1 = recompute_metrics(&zs, &y, k, bins).macro_f1;
        let ckpt = sha(&self.path("checkpoint/model.safetensors"));
        let t_applied = report["calibration"]["t_applied"].clone();
        self.edit_json("gate-report.json", |r| {
            for row in r["seeds"]["per_seed"].as_array_mut().expect("per_seed") {
                let seed = row["seed"].as_i64().expect("seed");
                let file = self.path(&format!("seeds/seed-{seed}/eval-probs.json"));
                let bytes = std::fs::read(&file).expect("seed file");
                row["eval_probs_sha256"] = sha256_hex(&bytes).into();
                if seed == shipped {
                    row["model_safetensors_sha256"] = ckpt.clone().into();
                    row["t_applied"] = t_applied.clone();
                }
                let Ok(p) = validate_probs(ProbsWhich::Seed(seed), &bytes, &data.eval, &labels)
                else {
                    continue;
                };
                let m = recompute_metrics(&p, &y, k, bins);
                let margin = m.macro_f1 - zs_f1;
                row["macro_f1"] = m.macro_f1.into();
                row["ece_post"] = m.ece.into();
                row["margin"] = margin.into();
                row["pass"] = (margin >= min_margin && m.ece <= max_ece).into();
                row["rank_key"] = ((m.ece * scale).floor() as i64).into();
            }
        });
    }

    /// Append a train.jsonl row the way a tenant's data would carry it, and re-derive the
    /// recipe's `shots_per_class` from the new train.jsonl as the trainer would (the epoch count
    /// stays legal: the fixture is far below 16 shots/class), so only the rule under test can
    /// refuse. The caller still reseals with `rehash`.
    fn append_train(&self, line: &str) {
        append_line(&self.data().join("train.jsonl"), line);
        let shots = train_shots(self);
        self.edit_json("recipe.json", |r| r["shots_per_class"] = shots.into());
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

/// A production copy under the 1.4.0 median rule (the default every accept path uses): seeds
/// 13 / 17 / 23 whose files give ECEs low / high / mid, seed 23 shipped (its file IS
/// eval-probs.json), so 23 is the median.
fn production_copy(policy: &VerifyPolicy, pass: bool) -> Run {
    seed_copy(
        policy,
        pass,
        [
            (13, SeedFile::Flatten),
            (17, SeedFile::Sharpen(0.05)),
            (23, SeedFile::Shipped),
        ],
        23,
    )
}

/// How a seed's `eval-probs.json` is derived from the shipped one.
#[derive(Debug, Clone, Copy)]
enum SeedFile {
    /// The shipped seed: a copy of `eval-probs.json`.
    Shipped,
    /// Halfway to uniform: argmax kept, confidence (and, here, ECE) lower.
    Flatten,
    /// `d` of mass moved onto each row's argmax: argmax kept, confidence (and ECE) higher.
    Sharpen(f64),
}

/// A three-seed production copy under `policy`'s seed selection: `files` says how each seed's
/// file is derived, `shipped` which seed the report ships. The per_seed rows are derived by
/// `rehash` exactly as the trainer derives them.
fn seed_copy(policy: &VerifyPolicy, pass: bool, files: [(i64, SeedFile); 3], shipped: i64) -> Run {
    let run = legacy_copy(policy, pass);
    let seeds = policy.seed_selection_seeds.clone();
    assert_eq!(
        files.iter().map(|f| f.0).collect::<Vec<_>>(),
        seeds,
        "the copy's seeds are the contract's"
    );
    run.edit_json("recipe.json", |r| {
        r["seed"] = seeds[0].into();
        r["seed_selection"] = obj(vec![
            ("policy", policy.seed_selection_policy.clone().into()),
            ("rank_scale", policy.seed_rank_scale.into()),
            ("seeds", Value::from(seeds.clone())),
            ("tie_break", policy.seed_tie_break.clone().into()),
        ]);
    });
    let base = read_json(&run.path("eval-probs.json"));
    for (seed, how) in files {
        let mut v = base.clone();
        for row in v["rows"].as_array_mut().expect("rows") {
            let p: Vec<f64> = row["probabilities"]
                .as_array()
                .expect("p")
                .iter()
                .map(|x| x.as_f64().expect("p"))
                .collect();
            let k = p.len() as f64;
            let a = (0..p.len()).fold(0, |m, i| if p[i] > p[m] { i } else { m });
            let q: Vec<f64> = match how {
                SeedFile::Shipped => p,
                SeedFile::Flatten => p.iter().map(|x| 0.5 * x + 0.5 / k).collect(),
                SeedFile::Sharpen(d) => p
                    .iter()
                    .enumerate()
                    .map(|(i, x)| if i == a { x + d } else { x - d / (k - 1.0) })
                    .collect(),
            };
            // Stored as exact f32 values, as the trainer writes them.
            row["probabilities"] =
                Value::from(q.iter().map(|&x| f64::from(x as f32)).collect::<Vec<f64>>());
        }
        let dir = run.path(&format!("seeds/seed-{seed}"));
        std::fs::create_dir_all(&dir).expect("seed dir");
        write_json(&dir.join("eval-probs.json"), &v);
    }
    let per_seed: Vec<Value> = seeds
        .iter()
        .map(|&s| {
            obj(vec![
                ("seed", s.into()),
                ("macro_f1", 0.0.into()),
                ("f_avg", Value::Null),
                ("ece_post", 0.0.into()),
                ("margin", 0.0.into()),
                ("pass", false.into()),
                ("t_applied", 1.0.into()),
                ("rank_key", 0.into()),
                ("eval_probs_sha256", "".into()),
                (
                    "model_safetensors_sha256",
                    sha256_hex(format!("seed-{s}").as_bytes()).into(),
                ),
            ])
        })
        .collect();
    run.edit_json("gate-report.json", |r| {
        r["seeds"] = obj(vec![
            ("declared", seeds[0].into()),
            ("n", seeds.len().into()),
            (
                "label",
                format!("median-ECE seed of {} seeds", seeds.len()).into(),
            ),
            ("policy", policy.seed_selection_policy.clone().into()),
            ("shipped", shipped.into()),
            ("per_seed", Value::Array(per_seed)),
        ]);
    });
    run.rehash();
    run
}

/// The contract's `early_stopping` block as recipe.json copies it (the trainer's default rule).
fn early_stopping_json(policy: &VerifyPolicy) -> Value {
    let es = &policy.recipe.early_stopping;
    obj(vec![
        ("eval_every_epochs", es.eval_every_epochs.into()),
        ("first_candidate_epoch", es.first_candidate_epoch.into()),
        ("min_delta", es.min_delta.into()),
        ("mode", es.mode.clone().into()),
        ("monitor", es.monitor.clone().into()),
        ("patience_epochs", es.patience_epochs.into()),
        ("restore", es.restore.clone().into()),
        ("tie_break", es.tie_break.clone().into()),
    ])
}

/// The largest class count of the copy's train.jsonl: what the trainer records as
/// `shots_per_class`.
fn train_shots(run: &Run) -> u64 {
    let data = read_data_dir(&run.data()).expect("data dir");
    let k = data.task.owned_labels().len();
    (0..k)
        .map(|c| data.train.iter().filter(|r| r.label == c).count() as u64)
        .max()
        .expect("classes")
}

/// A LEGACY (1.x) production copy: no seed_selection, single declared seed; report thresholds
/// are `policy`'s and the reported pass is `pass`, every hash recomputed. Its recipe is the one
/// a production trainer writes: the contract's recipe values, `seed` the declared seed, the
/// recipe's `shots_per_class` derived from train.jsonl with the epoch count the epoch rule
/// requires for it, and the default `early_stopping` rule — made consistent with the rule by
/// this helper, never by relaxing the rule.
fn legacy_copy(policy: &VerifyPolicy, pass: bool) -> Run {
    let run = synthetic_copy();
    let shots = train_shots(&run);
    let c = &policy.recipe;
    let epochs = if shots <= 16 {
        c.epochs_at_most_16_per_class
    } else {
        c.epochs_above_16_max
    };
    run.edit_json("recipe.json", |r| {
        r["variant"] = PRODUCTION_VARIANT.into();
        r["seed"] = c.declared_seed.into();
        r["shots_per_class"] = shots.into();
        r["epochs"] = epochs.into();
        r["early_stopping"] = early_stopping_json(policy);
    });
    run.edit_json("gate-report.json", |r| {
        r["seeds"]["declared"] = c.declared_seed.into();
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
    assert_eq!(report.shipped_seed, Some(23), "the median seed ships");
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
    assert_ne!(real, policy.base.model_safetensors_sha256);
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

/// The base's tokenizer is bound to the run's (`inputs_sha256.tokenizer_json`): a base dir
/// with the pinned weights but another tokenizer would score the zero-shot baseline on a
/// different tokenization than the fine-tune it is compared with.
#[test]
fn base_mismatch_tokenizer() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let other = run.path("other-base");
    copy_dir(&run.base(), &other);
    let tok = other.join("tokenizer/tokenizer.json");
    let mut bytes = std::fs::read(&tok).expect("read base tokenizer");
    bytes.push(b'\n'); // still valid JSON: only the binding can refuse it
    std::fs::write(&tok, bytes).expect("write base tokenizer");
    let inputs = run.inputs();
    let packed = artifact::write_decide_apr(&inputs).expect("pack");
    let e = expect_err(verify_run(&inputs, &packed, &run.data(), &other, &policy));
    assert!(
        matches!(
            &e,
            VerifyError::BaseMismatch {
                which: BaseWhich::BaseTokenizer,
                expected,
                ..
            } if *expected == policy.base.tokenizer_json_sha256
        ),
        "the contract pin refuses first: {e:?}"
    );
    // A policy whose pin IS the other tokenizer: the run's inputs_sha256.tokenizer_json still
    // refuses it (the tokenizer is bound to BOTH).
    let moved = VerifyPolicy {
        base: BasePins {
            tokenizer_json_sha256: sha(&tok),
            ..policy.base.clone()
        },
        ..policy.clone()
    };
    let e = expect_err(verify_run(&inputs, &packed, &run.data(), &other, &moved));
    assert!(
        matches!(
            &e,
            VerifyError::BaseMismatch {
                which: BaseWhich::BaseTokenizer,
                expected,
                ..
            } if *expected == inputs.gate_report.inputs_sha256.tokenizer_json
        ),
        "the run binding refuses: {e:?}"
    );
}

/// A base dir copy with `rel` changed by one appended newline (still valid JSON, so only the
/// pin can refuse it), verified against `run`'s packed bytes.
fn verify_with_edited_base_file(run: &Run, policy: &VerifyPolicy, rel: &str) -> VerifyError {
    let other = run.path("other-base");
    copy_dir(&run.base(), &other);
    let f = other.join(rel);
    let mut bytes = std::fs::read(&f).expect("read base file");
    bytes.push(b'\n');
    std::fs::write(&f, bytes).expect("write base file");
    let inputs = run.inputs();
    let packed = artifact::write_decide_apr(&inputs).expect("pack");
    expect_err(verify_run(&inputs, &packed, &run.data(), &other, policy))
}

/// V6-c: the base dir's encoder/config.json is pinned by the contract; one byte off is refused
/// naming it, and the zero-shot scorer is never built from it.
#[test]
fn base_encoder_config_unpinned_refused() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let e = verify_with_edited_base_file(&run, &policy, "encoder/config.json");
    assert!(
        matches!(
            &e,
            VerifyError::BaseMismatch {
                which: BaseWhich::EncoderConfig,
                ..
            }
        ),
        "{e:?}"
    );
    assert!(e.to_string().contains("which=encoder_config"), "{e}");
}

/// V6-c: the base dir's rl_agent_config.json is pinned by the contract.
#[test]
fn base_agent_config_unpinned_refused() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let e = verify_with_edited_base_file(&run, &policy, "rl_agent_config.json");
    assert!(
        matches!(
            &e,
            VerifyError::BaseMismatch {
                which: BaseWhich::AgentConfig,
                ..
            }
        ),
        "{e:?}"
    );
    assert!(e.to_string().contains("which=agent_config"), "{e}");
}

#[test]
fn base_mismatch_contract() {
    // The policy carries the CONTRACT's real en-root base; the fixture declares the tiny one.
    let real = contract_yaml("laya-finetune-gate-v1.yaml")["base"]["model_safetensors_sha256"]
        .as_str()
        .expect("contract base sha")
        .to_string();
    let policy = VerifyPolicy {
        base: BasePins {
            model_safetensors_sha256: real,
            ..tiny_base_pins()
        },
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

/// V6-b: a run that declares another base REVISION while keeping the pinned sha256 is refused
/// before any model is built — the `model.base` string it would serve is not the contract's.
#[test]
fn base_identity_mismatch_revision() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    run.edit_json("recipe.json", |r| {
        let rev = r["base"]["revision"]
            .as_str()
            .expect("revision")
            .to_string();
        r["base"]["revision"] = format!("{rev}-other").into();
    });
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            &e,
            VerifyError::BaseMismatch {
                which: BaseWhich::Revision,
                ..
            }
        ),
        "{e:?}"
    );
    assert!(e.to_string().contains("which=revision"), "{e}");
    assert_eq!(e.exit_code(), 2);
}

/// AL2 / V12-b: the typed view refuses a non-integer `ece_bins`, so no caller (the CLI, the
/// integration tests, these tests) can read `15.0` as 15 while another refuses it.
#[test]
fn policy_views_refuse_non_integer_ece_bins() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts/laya-finetune-gate-v1.yaml");
    let text = std::fs::read_to_string(&path).expect("read gate contract");
    let honest: GateContractView = serde_yaml::from_str(&text).expect("the contract parses");
    assert_eq!(honest.constants.ece_bins, 15);
    assert_eq!(
        text.matches("\n  ece_bins: 15\n").count(),
        1,
        "one ece_bins line"
    );
    let forged = text.replace("\n  ece_bins: 15\n", "\n  ece_bins: 15.0\n");
    let e = serde_yaml::from_str::<GateContractView>(&forged)
        .expect_err("a non-integer ece_bins must not deserialize");
    assert!(e.to_string().contains("ece_bins"), "{e}");
}

/// A production copy whose recipe.json got `edit` (then resealed), verified.
fn verify_recipe_edit(edit: impl FnOnce(&mut Value)) -> Result<VerifyReport, VerifyError> {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    run.edit_json("recipe.json", edit);
    run.rehash();
    run.verify(&policy)
}

/// V6-d: recipe.json must equal the contract's recipe block — one case per field class (a
/// float, the schedule literal, the seed, an early_stopping field, and shots_per_class against
/// the data dir) — and an ABSENT early_stopping (the declared fixed_epochs rule) is accepted.
#[test]
fn recipe_block_differs_from_contract_refused() {
    let cases: [(&str, fn(&mut Value)); 7] = [
        ("encoder_lr", |r| {
            r["encoder_lr"] = (r["encoder_lr"].as_f64().expect("lr") * 2.0).into();
        }),
        ("proper_reward_w_sph", |r| {
            r["proper_reward_w_sph"] = 0.5.into()
        }),
        ("schedule", |r| r["schedule"] = "linear".into()),
        ("seed", |r| r["seed"] = 17.into()),
        ("early_stopping.min_delta", |r| {
            r["early_stopping"]["min_delta"] = 0.002.into();
        }),
        ("early_stopping.patience_epochs", |r| {
            r["early_stopping"]["patience_epochs"] = 4.into();
        }),
        ("shots_per_class", |r| {
            let s = r["shots_per_class"].as_u64().expect("shots");
            r["shots_per_class"] = (s + 1).into();
        }),
    ];
    for (field, edit) in cases {
        let e = expect_err(verify_recipe_edit(edit));
        assert!(
            matches!(&e, VerifyError::RecipeMismatch { field: f, .. } if *f == field),
            "{field}: {e:?}"
        );
        assert!(
            e.to_string().contains(&format!("recipe.json {field} is")),
            "{e}"
        );
        assert_eq!(e.exit_code(), 2);
    }
    // Control: the fixed_epochs rule (no early_stopping key) is a declared rule, not a mismatch.
    let report = verify_recipe_edit(|r| {
        r.as_object_mut().expect("recipe").remove("early_stopping");
    })
    .expect("fixed_epochs verifies");
    assert!(report.deploy_eligible);
}

/// The epoch rule: <= 16 shots/class -> exactly epochs_at_most_16_per_class; above 16 -> an
/// epoch count in [epochs_above_16_min, epochs_above_16_max].
#[test]
fn recipe_epochs_break_the_rule_refused() {
    let policy = permissive_policy();
    let c = policy.recipe.clone();
    let e = expect_err(verify_recipe_edit(move |r| {
        r["epochs"] = (c.epochs_at_most_16_per_class - 1).into();
    }));
    assert!(
        matches!(
            &e,
            VerifyError::RecipeMismatch {
                field: "epochs",
                ..
            }
        ),
        "{e:?}"
    );
    let run = production_copy(&policy, true);
    let honest = run.inputs().recipe;
    check_recipe_block(&honest, &policy).expect("the honest copy's recipe");
    let c = &policy.recipe;
    let at = |shots: u64, epochs: u64| {
        let mut r = honest.clone();
        r.shots_per_class = shots;
        r.epochs = epochs;
        check_recipe_block(&r, &policy)
    };
    let fixed = c.epochs_at_most_16_per_class;
    let (lo, hi) = (c.epochs_above_16_min, c.epochs_above_16_max);
    for (shots, epochs, ok) in [
        (16, fixed, true),
        (16, fixed + 1, false),
        (1, fixed - 1, false),
        (17, lo, true),
        (17, hi, true),
        (64, lo - 1, false),
        (64, hi + 1, false),
    ] {
        let got = at(shots, epochs);
        assert_eq!(got.is_ok(), ok, "shots {shots} epochs {epochs}: {got:?}");
        if let Err(e) = got {
            assert!(
                matches!(
                    &e,
                    VerifyError::RecipeMismatch {
                        field: "epochs",
                        ..
                    }
                ),
                "{e:?}"
            );
        }
    }
}

/// `n` distinct train rows per class of `k`, a disjoint eval, and a slice of `per_class[c]` rows
/// of class `c` (its first rows), as check_split reads them.
fn fraction_case(
    k: usize,
    n: usize,
    per_class: &[usize],
) -> (Vec<DataRow>, Vec<DataRow>, GateCalibration) {
    let train: Vec<DataRow> = (0..k)
        .flat_map(|c| {
            (0..n).map(move |i| DataRow {
                text: format!("train text {c} {i}"),
                label: c,
            })
        })
        .collect();
    let eval: Vec<DataRow> = (0..k)
        .map(|c| DataRow {
            text: format!("eval text {c}"),
            label: c,
        })
        .collect();
    let ids: Vec<u64> = per_class
        .iter()
        .enumerate()
        .flat_map(|(c, &m)| (0..m).map(move |i| (c * n + i) as u64))
        .collect();
    let calib = GateCalibration {
        bucket: "choice:2".into(),
        t_fitted: 1.0,
        t_applied: 1.0,
        clamp_hit: false,
        slice_size: ids.len() as u64,
        slice_ids_sha256: sha256_hex(serde_json::to_string(&ids).expect("ids").as_bytes()),
        slice_ids: ids,
    };
    (train, eval, calib)
}

/// WR-08: at 16 rows per class the contract's fraction (0.25) needs 4 slice rows, above the
/// per-class minimum (2): a slice of 3 in one class — which the minimum alone accepted — is
/// refused naming the class and the need.
#[test]
fn slice_below_fraction_refused() {
    let p = contract_policy();
    let (n, k) = (16, 2);
    let need = slice_need(
        n,
        p.calibration_slice_fraction,
        p.calibration_slice_min_per_class,
    );
    assert!(
        need > p.calibration_slice_min_per_class,
        "the fraction decides here"
    );
    let short = usize::try_from(need).expect("need") - 1;
    let (train, eval, calib) = fraction_case(k, n, &[short + 1, short]);
    let e = check_split(
        &train,
        &eval,
        &calib,
        k,
        p.calibration_slice_fraction,
        p.calibration_slice_min_per_class,
    )
    .expect_err("one row below the fraction");
    assert!(
        matches!(&e, VerifyError::SliceInvalid { why }
            if why.contains("class 1") && why.contains(&format!("need >= {need}"))),
        "{e:?}"
    );
    // The same slice under the minimum alone (the pre-08-21 rule) was accepted.
    check_split(
        &train,
        &eval,
        &calib,
        k,
        0.0,
        p.calibration_slice_min_per_class,
    )
    .expect("the minimum alone accepts it");
}

/// Exactly the need in every class is accepted.
#[test]
fn slice_at_fraction_accepted() {
    let p = contract_policy();
    let (n, k) = (16, 3);
    let need = usize::try_from(slice_need(
        n,
        p.calibration_slice_fraction,
        p.calibration_slice_min_per_class,
    ))
    .expect("need");
    let (train, eval, calib) = fraction_case(k, n, &[need, need, need]);
    check_split(
        &train,
        &eval,
        &calib,
        k,
        p.calibration_slice_fraction,
        p.calibration_slice_min_per_class,
    )
    .expect("exactly the need");
}

/// V8-c: a symlink planted where the temp file would be (the pre-08-21 name
/// `.<out>.tmp-<pid>`, and every name the next calls could pick) is never followed: the victim
/// is unchanged and the planted links are left alone.
#[cfg(unix)]
#[test]
fn write_atomic_does_not_follow_a_planted_symlink() {
    let dir = TempDir::new().expect("tempdir");
    let victim = dir.path().join("victim.txt");
    std::fs::write(&victim, b"precious").expect("victim");
    let out = dir.path().join("v.apr");
    let pid = std::process::id();
    let next = WRITE_SEQ.load(AtomicOrdering::Relaxed);
    let mut planted = vec![dir.path().join(format!(".v.apr.tmp-{pid}"))];
    planted.extend((next..next + 64).map(|q| dir.path().join(format!(".v.apr.tmp-{pid}-{q}"))));
    for link in &planted {
        std::os::unix::fs::symlink(&victim, link).expect("plant symlink");
    }
    let result = write_atomic(&out, b"attacker-controlled bytes");
    assert_eq!(
        std::fs::read(&victim).expect("victim"),
        b"precious",
        "the victim must be unchanged ({result:?})"
    );
    for link in &planted {
        assert!(
            std::fs::symlink_metadata(link)
                .expect("the planted link is still there")
                .file_type()
                .is_symlink(),
            "{}",
            link.display()
        );
    }
    match result {
        Ok(()) => assert_eq!(
            std::fs::read(&out).expect("out"),
            b"attacker-controlled bytes"
        ),
        Err(e) => assert!(matches!(e, VerifyError::Read { .. }), "{e:?}"),
    }
}

/// V8-d: a ProbsRowCoverage refusal names WHICH probability file.
#[test]
fn probs_row_coverage_names_the_file() {
    let policy = permissive_policy();
    for (file, which, name) in [
        ("eval-probs.json", ProbsWhich::FineTuned, "fine_tuned"),
        ("zero-shot-probs.json", ProbsWhich::ZeroShot, "zero_shot"),
    ] {
        let run = production_copy(&policy, true);
        run.edit_json(file, |v| {
            v["rows"].as_array_mut().expect("rows").pop();
        });
        run.rehash();
        let e = expect_err(run.verify(&policy));
        assert!(
            matches!(&e, VerifyError::ProbsRowCoverage { which: w, .. } if *w == which),
            "{e:?}"
        );
        assert!(e.to_string().starts_with(&format!("{name}: ")), "{e}");
    }
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

/// laya-finetune-gate-v1 `eval_set.generic_rule`: eval.jsonl must hold every criterion.
/// With one absent, macro-F1 over `y ∪ pred` lets the margin be decided by which model
/// happens to predict the absent class, so the gate could certify a worse fine-tune.
#[test]
fn eval_missing_a_criterion_is_refused() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let p = run.data().join("eval.jsonl");
    let kept: String = std::fs::read_to_string(&p)
        .expect("read eval")
        .lines()
        .filter(|l| {
            serde_json::from_str::<Value>(l).expect("eval row")["label"].as_str() != Some("account")
        })
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(&p, kept).expect("write eval");
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            &e,
            VerifyError::DataInvalid { file: "eval_jsonl", why, .. } if why.contains("account")
        ),
        "{e:?}"
    );
    assert_eq!(e.exit_code(), 2);
}

#[test]
fn split_overlap() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let (text, label) = jsonl_row(&run.data().join("eval.jsonl"), 1);
    run.append_train(&row_json(&text, &label));
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
    run.append_train(&row_json(&spaced, "billing"));
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
    run.append_train(&row_json(&format!("{text} "), &label));
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

/// `verify_path` binds the WHOLE file to the run. The re-score only exercises the weights the
/// eval rows reach, so a file whose weights differ from the run's checkpoint where no eval row
/// or probe looks (here one value of the unused `act_head` family; in the field, embedding rows
/// of tokens no eval row contains) passes every rung and re-scores identically — and must
/// still be refused as not the run's bytes.
#[test]
fn verify_path_refuses_bytes_the_run_does_not_pack_to() {
    use aprender::format::v2::{AprV2ReaderRef, AprV2Writer};
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let honest = artifact::write_decide_apr(&run.inputs()).expect("pack");
    let tampered = {
        let r = AprV2ReaderRef::from_bytes(&honest).expect("open packed");
        let mut w = AprV2Writer::new(r.metadata().clone());
        let mut hit = false;
        for e in r.tensor_index() {
            let mut data = r.get_tensor_data(&e.name).expect("tensor data").to_vec();
            if e.name == "act_head.0.weight" {
                data[0] ^= 0x01; // lowest mantissa bit of one F16: still finite
                hit = true;
            }
            w.add_tensor(e.name.clone(), e.dtype, e.shape.clone(), data);
        }
        assert!(hit, "the fixture carries act_head.0.weight");
        w.write().expect("repack")
    };
    assert_ne!(tampered, honest);
    Decider::load_bytes(&tampered).expect("the ladder accepts the tampered file");
    let out = TempDir::new().expect("tempdir");
    let apr = out.path().join("tampered.apr");
    std::fs::write(&apr, &tampered).expect("write apr");
    let e = expect_err(verify_path(
        &apr,
        &run.dir,
        &run.data(),
        &run.base(),
        &policy,
    ));
    assert!(
        matches!(&e, VerifyError::ArtifactNotFromRun { file_sha256, .. }
            if *file_sha256 == artifact::artifact_sha256_hex(&tampered)),
        "{e:?}"
    );
    assert_eq!(e.exit_code(), 2);
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

// ===========================================================================
// Plan 08-15 Task 1: the noise-referenced re-score bound (laya-parity-v1 A1)
// ===========================================================================

/// A JSON object from ordered pairs (`json!`'s object form expands to `unwrap`).
fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// A probability file's rows exactly as the verifier reads them (JSON f64 -> f32).
fn file_probs(run: &Run, file: &str) -> Vec<Vec<f32>> {
    read_json(&run.path(file))["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| {
            r["probabilities"]
                .as_array()
                .expect("probabilities")
                .iter()
                .map(|p| p.as_f64().expect("p") as f32)
                .collect()
        })
        .collect()
}

/// A float64 reference equal to the file's CURRENT float32 rows widened (noise 0).
fn f64_rows(run: &Run, file: &str) -> Vec<Vec<f64>> {
    file_probs(run, file)
        .iter()
        .map(|r| r.iter().map(|&p| f64::from(p)).collect())
        .collect()
}

/// Move `d` of mass INTO row `row`'s argmax component from the next one: the sum is kept and
/// the argmax cannot flip.
fn nudge(rows: &mut [Vec<f64>], row: usize, d: f64) {
    let r = &mut rows[row];
    let a = (0..r.len()).fold(0, |m, i| if r[i] > r[m] { i } else { m });
    let b = (a + 1) % r.len();
    r[a] += d;
    r[b] -= d;
}

/// `max |f64(p32) - p64|`: what train.py records as `max_abs`, over the stored rows.
fn noise_of(p32: &[Vec<f32>], p64: &[Vec<f64>]) -> f64 {
    p32.iter()
        .zip(p64)
        .flat_map(|(a, b)| a.iter().zip(b).map(|(&x, &y)| (f64::from(x) - y).abs()))
        .fold(0.0, f64::max)
}

/// Write `rescore-noise.json` the way train.py writes it: the float64 rows `ft` / `zs` against
/// the copy's CURRENT probability files, `max_abs` and `bound = max(floor, k x max_abs)`
/// reported, k / floor from `policy`, control 0.0 on rows 0..min(5, n); then rehash so the
/// report names it. Returns the two noises.
fn write_noise_record(
    run: &Run,
    policy: &VerifyPolicy,
    ft: &[Vec<f64>],
    zs: &[Vec<f64>],
) -> [f64; 2] {
    let files = ["eval-probs.json", "zero-shot-probs.json"];
    let mut noises = [0.0; 2];
    let mut sets = Vec::new();
    for (i, (name, rows)) in ["fine_tuned", "zero_shot"].iter().zip([ft, zs]).enumerate() {
        let p32 = file_probs(run, files[i]);
        let noise = noise_of(&p32, rows);
        noises[i] = noise;
        let bound = policy.rescore_probs_abs.max(policy.rescore_noise_k * noise);
        let json_rows: Vec<Value> = rows
            .iter()
            .enumerate()
            .map(|(r, p)| {
                obj(vec![
                    ("row", r.into()),
                    ("probabilities_f64", Value::from(p.clone())),
                ])
            })
            .collect();
        sets.push(obj(vec![
            ("which", (*name).into()),
            ("scored", "eval".into()),
            ("t_applied", 1.0.into()),
            ("n", rows.len().into()),
            ("argmax_agree", rows.len().into()),
            ("max_abs", noise.into()),
            ("bound", bound.into()),
            ("rows", Value::Array(json_rows)),
        ]));
    }
    let n = ft.len();
    let record = obj(vec![
        ("schema", pack::RESCORE_NOISE_SCHEMA.into()),
        ("reference", "float64".into()),
        ("k", policy.rescore_noise_k.into()),
        ("floor_abs", policy.rescore_probs_abs.into()),
        ("control_max_abs", 0.0.into()),
        (
            "control_rows",
            Value::from((0..n.min(5)).collect::<Vec<usize>>()),
        ),
        ("sets", Value::Array(sets)),
    ]);
    write_json(&run.path("rescore-noise.json"), &record);
    run.rehash();
    noises
}

/// A production copy carrying a noise record whose float64 rows equal its own files (noise 0).
fn copy_with_record(policy: &VerifyPolicy) -> Run {
    let run = production_copy(policy, true);
    let (ft, zs) = (
        f64_rows(&run, "eval-probs.json"),
        f64_rows(&run, "zero-shot-probs.json"),
    );
    write_noise_record(&run, policy, &ft, &zs);
    run
}

/// Both validated probability files of a copy, and its PackInputs.
fn validated(run: &Run) -> (PackInputs, Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let inputs = run.inputs();
    let data = read_data_dir(&run.data()).expect("data dir");
    let labels = data.task.owned_labels();
    let ft = validate_probs(
        ProbsWhich::FineTuned,
        &inputs.eval_probs_json,
        &data.eval,
        &labels,
    )
    .expect("ft probs");
    let zs = validate_probs(
        ProbsWhich::ZeroShot,
        &inputs.zero_shot_probs_json,
        &data.eval,
        &labels,
    )
    .expect("zs probs");
    (inputs, ft, zs)
}

#[test]
fn noise_absent_uses_floor() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let (inputs, ft, zs) = validated(&run);
    assert!(inputs.gate_report.rescore_noise_sha256.is_none());
    assert!(inputs.rescore_noise_json.is_none());
    let floor = policy.rescore_probs_abs;
    let bounds = rescore_bounds(&inputs, &ft, &zs, &policy).expect("no record: the floor");
    for (b, which) in bounds
        .iter()
        .zip([ProbsWhich::FineTuned, ProbsWhich::ZeroShot])
    {
        assert_eq!(
            *b,
            RescoreBound {
                which,
                noise: None,
                bound: floor
            }
        );
    }
    let report = run.verify(&policy).expect("verifies at the floor");
    assert_eq!(report.rescore_bound.to_bits(), floor.to_bits());
    assert_eq!(report.zs_rescore_bound.to_bits(), floor.to_bits());
    assert_eq!((report.noise, report.zs_noise), (None, None));
}

/// The behavioural proof that the bar moved only where a record justifies it: torch's file is
/// 2e-5 from its own float64 answer on row 3 (the rest exact), so the recorded noise is 2e-5
/// and the bound 4 x 2e-5; the Rust re-score (which reproduces the float64 side here) drifts
/// 2e-5 from the file — ACCEPTED with the record, REFUSED at the 1e-5 floor without it.
#[test]
fn noise_bound_accepts_what_floor_refuses() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let (ft_ref, zs_ref) = (
        f64_rows(&run, "eval-probs.json"),
        f64_rows(&run, "zero-shot-probs.json"),
    );
    shift_prob(&run, "eval-probs.json", 3, 0, 2e-5);
    let noise = write_noise_record(&run, &policy, &ft_ref, &zs_ref);
    let floor = policy.rescore_probs_abs;
    assert!(noise[0] > floor && noise[0] < 3e-5, "{noise:?}");
    let want = floor.max(policy.rescore_noise_k * noise[0]);
    let report = run
        .verify(&policy)
        .expect("accepted: drift inside the recorded bound");
    assert!(
        report.rescore_max_abs > floor,
        "the drift is really beyond the floor: {report:?}"
    );
    assert!(within(report.rescore_max_abs, report.rescore_bound));
    assert_eq!(report.rescore_bound.to_bits(), want.to_bits());
    assert_eq!(report.noise.map(f64::to_bits), Some(noise[0].to_bits()));
    assert_eq!(
        report.zs_rescore_bound.to_bits(),
        floor.to_bits(),
        "zs noise 0"
    );

    // The same bytes with the record removed from the report: the floor, refused.
    std::fs::remove_file(run.path("rescore-noise.json")).expect("remove record");
    run.edit_json("gate-report.json", |r| {
        r.as_object_mut()
            .expect("report")
            .remove("rescore_noise_sha256");
    });
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(e, VerifyError::RescoreDrift { which: ProbsWhich::FineTuned, row: 3, bound, .. }
            if bound.to_bits() == floor.to_bits()),
        "{e:?}"
    );
    assert!(e.to_string().contains(&format!("bound={floor}")), "{e}");
}

/// A forger inflating the record: max_abs x10 with a consistent bound, or the bound alone.
#[test]
fn forged_noise_value() {
    let policy = permissive_policy();
    let run = copy_with_record(&policy);
    let floor = policy.rescore_probs_abs;
    run.edit_json("rescore-noise.json", |v| {
        let m = v["sets"][0]["max_abs"].as_f64().expect("max_abs").max(1e-6) * 10.0;
        v["sets"][0]["max_abs"] = m.into();
        v["sets"][0]["bound"] = floor.max(policy.rescore_noise_k * m).into();
    });
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::RescoreNoiseMismatch {
                which: ProbsWhich::FineTuned,
                field: "max_abs",
                ..
            }
        ),
        "{e:?}"
    );

    let run = copy_with_record(&policy);
    run.edit_json("rescore-noise.json", |v| {
        v["sets"][1]["bound"] = 5e-4.into()
    });
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::RescoreNoiseMismatch {
                which: ProbsWhich::ZeroShot,
                field: "bound",
                ..
            }
        ),
        "{e:?}"
    );
    assert_eq!(e.exit_code(), 2);
}

#[test]
fn noise_hash_mismatch() {
    let policy = permissive_policy();
    let run = copy_with_record(&policy);
    run.edit_json("rescore-noise.json", |v| {
        v["sets"][0]["t_applied"] = 2.0.into()
    });
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::InputHashMismatch {
                file: "rescore_noise_json",
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn noise_bound_over_ceiling() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let (mut ft_ref, zs_ref) = (
        f64_rows(&run, "eval-probs.json"),
        f64_rows(&run, "zero-shot-probs.json"),
    );
    nudge(&mut ft_ref, 2, 3e-4);
    let noise = write_noise_record(&run, &policy, &ft_ref, &zs_ref);
    assert!(policy.rescore_noise_k * noise[0] > policy.rescore_bound_max_abs);
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(e, VerifyError::RescoreBoundCeiling { which: ProbsWhich::FineTuned, ceiling, .. }
            if ceiling.to_bits() == policy.rescore_bound_max_abs.to_bits()),
        "{e:?}"
    );
}

#[test]
fn noise_argmax_flip() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let (mut ft_ref, zs_ref) = (
        f64_rows(&run, "eval-probs.json"),
        f64_rows(&run, "zero-shot-probs.json"),
    );
    // Swap row 2's top two components: the float64 row now names another label.
    let r = &mut ft_ref[2];
    let a = (0..r.len()).fold(0, |m, i| if r[i] > r[m] { i } else { m });
    let b = (0..r.len()).filter(|&i| i != a).fold(usize::MAX, |m, i| {
        if m == usize::MAX || r[i] > r[m] {
            i
        } else {
            m
        }
    });
    r.swap(a, b);
    write_noise_record(&run, &policy, &ft_ref, &zs_ref);
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(e, VerifyError::NoiseArgmaxFlip { which: ProbsWhich::FineTuned, row: 2, torch, reference }
            if torch == a && reference == b),
        "{e:?}"
    );
}

#[test]
fn noise_rows_incomplete() {
    let policy = permissive_policy();
    // A record missing its last row.
    let run = copy_with_record(&policy);
    run.edit_json("rescore-noise.json", |v| {
        v["sets"][0]["rows"].as_array_mut().expect("rows").pop();
    });
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            &e,
            VerifyError::RescoreNoiseInvalid {
                which: Some(ProbsWhich::FineTuned),
                field: "rows",
                ..
            }
        ),
        "{e:?}"
    );
    // A record repeating row 0 in place of row 1.
    let run = copy_with_record(&policy);
    run.edit_json("rescore-noise.json", |v| {
        v["sets"][1]["rows"][1]["row"] = 0.into();
    });
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            &e,
            VerifyError::RescoreNoiseInvalid {
                which: Some(ProbsWhich::ZeroShot),
                row: Some(1),
                field: "rows",
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn noise_k_mismatch() {
    let policy = permissive_policy();
    for (field, value) in [("k", 3.0), ("floor_abs", 2e-5)] {
        let run = copy_with_record(&policy);
        run.edit_json("rescore-noise.json", |v| v[field] = value.into());
        run.rehash();
        let e = expect_err(run.verify(&policy));
        assert!(
            matches!(&e, VerifyError::RescoreNoiseInvalid { which: None, field: f, .. } if *f == field),
            "{field}: {e:?}"
        );
    }
}

// ===========================================================================
// Plan 08-15 Task 2: the median-seed re-derivation (A3), the legacy refusal order, and the
// shift probe (A2)
// ===========================================================================

/// The copy's data dir, read.
fn data_of(run: &Run) -> DataDir {
    read_data_dir(&run.data()).expect("data dir")
}

#[test]
fn seed_median_ships() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    let rep = read_json(&run.path("gate-report.json"));
    let keys: Vec<i64> = rep["seeds"]["per_seed"]
        .as_array()
        .expect("per_seed")
        .iter()
        .map(|r| r["rank_key"].as_i64().expect("key"))
        .collect();
    assert!(
        keys[0] < keys[2] && keys[2] < keys[1],
        "13 low, 23 mid, 17 high: {keys:?}"
    );
    let got = check_seed_selection(&run.inputs(), &data_of(&run), &policy).expect("median");
    assert_eq!(got, Some(23));
    let report = run.verify(&policy).expect("the median copy verifies");
    assert_eq!(report.shipped_seed, Some(23));
}

#[test]
fn seed_shipped_not_median() {
    // Files consistent with shipping 13 (13's file IS eval-probs.json), but 13 has the LOWEST
    // ECE: the median is 17.
    let policy = permissive_policy();
    let run = seed_copy(
        &policy,
        true,
        [
            (13, SeedFile::Shipped),
            (17, SeedFile::Sharpen(0.05)),
            (23, SeedFile::Sharpen(0.1)),
        ],
        13,
    );
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(&e, VerifyError::SeedPolicyViolated { field: "shipped", why }
            if why.contains("13") && why.contains("17")),
        "{e:?}"
    );
    assert_eq!(e.exit_code(), 2);
}

#[test]
fn seed_tie_break_smaller_seed() {
    let scale = permissive_policy().seed_rank_scale;
    // 0.10009 and 0.10001 share rank key 1000: 13 ranks before 23, so the order is 17, 13, 23.
    let rows = [(13, 0.10009), (17, 0.05), (23, 0.10001)];
    assert_eq!(select_median_seed(&rows, scale).expect("median"), 13);
    // The tie decides the median: 13 and 23 share 1000 below 17's 2000 -> 13, 23, 17.
    let rows = [(23, 0.10001), (13, 0.10009), (17, 0.2)];
    assert_eq!(select_median_seed(&rows, scale).expect("median"), 23);
    // Distinct keys: the middle ECE ships regardless of seed order.
    let rows = [(13, 0.15), (17, 0.05), (23, 0.12)];
    assert_eq!(select_median_seed(&rows, scale).expect("median"), 23);
    // No median to ship: an even or empty N, a repeated seed, a non-finite ECE.
    for bad in [
        vec![(13, 0.1), (17, 0.2)],
        vec![],
        vec![(13, 0.1), (13, 0.2), (17, 0.3)],
        vec![(13, f64::NAN), (17, 0.2), (23, 0.3)],
    ] {
        let e = select_median_seed(&bad, scale).expect_err("refused");
        assert!(
            matches!(e, VerifyError::SeedPolicyViolated { .. }),
            "{bad:?}: {e:?}"
        );
    }
}

#[test]
fn seed_probs_hash_mismatch() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    shift_prob(&run, "seeds/seed-17/eval-probs.json", 1, 0, 1e-3);
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            &e,
            VerifyError::Pack(PackError::SeedProbsHashMismatch { seed: 17, .. })
        ),
        "{e:?}"
    );
    assert!(
        e.to_string().contains("seeds/seed-17/eval-probs.json"),
        "{e}"
    );
}

#[test]
fn seed_metric_forged() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    run.edit_json("gate-report.json", |r| {
        let row = &mut r["seeds"]["per_seed"][1];
        let e = row["ece_post"].as_f64().expect("ece_post");
        row["ece_post"] = (e + 1e-3).into();
    });
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::ReportedMetricMismatch {
                seed: Some(17),
                field: "per_seed.ece_post",
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn seed_checkpoint_not_shipped() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    run.edit_json("gate-report.json", |r| {
        r["seeds"]["per_seed"][2]["model_safetensors_sha256"] =
            sha256_hex(b"another checkpoint").into();
    });
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            &e,
            VerifyError::SeedPolicyViolated {
                field: "model_safetensors_sha256",
                ..
            }
        ),
        "{e:?}"
    );
}

#[test]
fn seed_policy_mismatch() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    run.edit_json("recipe.json", |r| {
        r["seed_selection"]["rank_scale"] = 1000.into();
    });
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            &e,
            VerifyError::SeedPolicyMismatch {
                field: "rank_scale",
                ..
            }
        ),
        "{e:?}"
    );
}

/// A LEGACY run whose gate passes is refused SeedPolicyMissing — and only after the re-scores:
/// the same copy with a drifted probability is refused RescoreDrift instead.
#[test]
fn legacy_policy_refused_after_gate_pass() {
    let policy = permissive_policy();
    let run = legacy_copy(&policy, true);
    assert_eq!(run.inputs().recipe.seed_selection, None);
    let e = expect_err(run.verify(&policy));
    assert_eq!(e, VerifyError::SeedPolicyMissing);
    assert_eq!(e.exit_code(), 2);
    shift_prob(&run, "eval-probs.json", 3, 0, 2e-5);
    run.rehash();
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            e,
            VerifyError::RescoreDrift {
                which: ProbsWhich::FineTuned,
                ..
            }
        ),
        "the re-scores come first: {e:?}"
    );
}

/// A LEGACY run whose gate FAILS keeps reporting GateFailed (exit 3), never SeedPolicyMissing.
#[test]
fn legacy_gate_fail_keeps_gate_failed() {
    let policy = contract_policy_tiny_base();
    let run = legacy_copy(&policy, false);
    let e = expect_err(run.verify(&policy));
    assert_eq!(e.exit_code(), 3, "{e:?}");
    assert!(
        matches!(&e, VerifyError::GateFailed(g) if g.clauses == vec![GateClause::Margin]),
        "{e:?}"
    );
}

/// Add a shift probe the way the trainer does: shift.jsonl (here the eval rows), both shift
/// probability files, and a shift_probe block whose metrics are recomputed from them.
fn add_shift_probe(run: &Run, policy: &VerifyPolicy) {
    std::fs::copy(
        run.data().join("eval.jsonl"),
        run.data().join("shift.jsonl"),
    )
    .expect("shift.jsonl");
    std::fs::copy(run.path("eval-probs.json"), run.path("shift-probs.json")).expect("shift ft");
    std::fs::copy(
        run.path("zero-shot-probs.json"),
        run.path("shift-zero-shot-probs.json"),
    )
    .expect("shift zs");
    let (_, ft, zs) = validated(run);
    let data = data_of(run);
    let y: Vec<usize> = data.eval.iter().map(|r| r.label).collect();
    let k = data.task.owned_labels().len();
    let bins = policy.ece_bins as usize;
    let (f, z) = (
        recompute_metrics(&ft, &y, k, bins),
        recompute_metrics(&zs, &y, k, bins),
    );
    run.edit_json("gate-report.json", |r| {
        r["shift_probe"] = obj(vec![
            ("gate_clause", false.into()),
            ("n", y.len().into()),
            (
                "zero_shot",
                obj(vec![
                    ("macro_f1", z.macro_f1.into()),
                    ("f_avg", Value::Null),
                    ("ece", z.ece.into()),
                ]),
            ),
            (
                "fine_tuned",
                obj(vec![
                    ("macro_f1", f.macro_f1.into()),
                    ("f_avg", Value::Null),
                    ("ece_post", f.ece.into()),
                ]),
            ),
            ("margin", (f.macro_f1 - z.macro_f1).into()),
            ("probs_sha256", "".into()),
            ("zero_shot_probs_sha256", "".into()),
        ]);
        r["inputs_sha256"]["shift_jsonl"] = "".into();
    });
    run.rehash();
}

#[test]
fn shift_probe_metric_forged() {
    let policy = permissive_policy();
    let plain = production_copy(&policy, true);
    let without = plain.verify(&policy).expect("verifies without a probe");
    let run = production_copy(&policy, true);
    add_shift_probe(&run, &policy);
    check_shift_probe(&run.inputs(), &data_of(&run), &policy).expect("the probe recomputes");
    let with = run.verify(&policy).expect("verifies with the probe");
    assert_eq!(
        with.recomputed, without.recomputed,
        "the probe never moves the gate"
    );
    assert_eq!(with.deploy_eligible, without.deploy_eligible);
    run.edit_json("gate-report.json", |r| {
        let e = r["shift_probe"]["fine_tuned"]["ece_post"]
            .as_f64()
            .expect("ece_post");
        r["shift_probe"]["fine_tuned"]["ece_post"] = (e + 0.01).into();
    });
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            &e,
            VerifyError::ShiftProbeMismatch {
                field: "shift_probe.fine_tuned.ece_post",
                ..
            }
        ),
        "{e:?}"
    );
    assert_eq!(e.exit_code(), 2);
}

/// The converse of `shift_file_missing`: a data dir that carries shift.jsonl while the report
/// names no probe is refused — a report may not silently drop the shift evidence.
#[test]
fn shift_probe_dropped_while_data_has_shift() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    std::fs::copy(
        run.data().join("eval.jsonl"),
        run.data().join("shift.jsonl"),
    )
    .expect("shift.jsonl");
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            &e,
            VerifyError::ShiftProbeMismatch {
                field: "shift_probe",
                ..
            }
        ),
        "{e:?}"
    );
    assert_eq!(e.exit_code(), 2);
}

#[test]
fn shift_file_missing() {
    let policy = permissive_policy();
    let run = production_copy(&policy, true);
    add_shift_probe(&run, &policy);
    std::fs::remove_file(run.data().join("shift.jsonl")).expect("remove shift.jsonl");
    let e = expect_err(run.verify(&policy));
    assert!(
        matches!(
            &e,
            VerifyError::ShiftProbeMismatch {
                field: "shift_jsonl",
                ..
            }
        ),
        "{e:?}"
    );
}
