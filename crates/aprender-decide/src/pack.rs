//! The packer: a Laya run dir (plus its data dir) -> ONE `decide-apr-v1` `.apr` (D-17).
//!
//! [`PackInputs::from_run_dir`] reads the `run_dir_layout` of
//! `contracts/laya-finetune-gate-v1.yaml` and refuses a run dir that contradicts itself
//! (a task copy that differs from the data dir's, a report whose hashes do not match the
//! files beside it) rather than laundering it into an artifact. [`pack_run_dir`] then
//! hands the inputs to [`crate::artifact::write_decide_apr`].
//!
//! # What this module deliberately does NOT do
//!
//! It applies no gate or variant POLICY: a failing gate report and a `synthetic-fixture`
//! recipe both pack, because the tiny test fixture is exactly that. Refusing a failing
//! report, and requiring the report thresholds to equal the contract, belong to plan
//! 08-09's pack CLI and verifier. Deployability is decide-apr-v1 `deploy_eligibility`,
//! never "it packed".
//!
//! # SafeTensors scope
//!
//! This is the only code in the crate that reads SafeTensors, and it is a PACK-time
//! library path. The served path reads only `.apr` (servers never call `pack`), so the
//! SafeTensors carve-out in CLAUDE.md is not widened.

use crate::artifact::{self, ArtifactError, BaseDecl, InputsSha256, ProbeRecord};
use aprender::format::v2::TensorDType;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::{Path, PathBuf};

/// Lowercase-hex sha256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// One checkpoint tensor exactly as stored: raw little-endian bytes, never re-rounded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointTensor {
    /// The verbatim Laya tensor name.
    pub name: String,
    /// `F16` for every weight, `F32` for the `temperature` buffer.
    pub dtype: TensorDType,
    /// Row-major shape.
    pub shape: Vec<usize>,
    /// Raw little-endian bytes copied from `model.safetensors`.
    pub bytes: Vec<u8>,
}

/// `recipe.json` (laya-finetune-gate-v1 `recipe_json_schema`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    /// `production` or `synthetic-fixture`.
    pub variant: String,
    /// `adamw`.
    pub optimizer: String,
    /// Encoder learning rate.
    pub encoder_lr: f64,
    /// Head learning rate.
    pub head_lr: f64,
    /// Cosine floor.
    pub eta_min: f64,
    /// AdamW weight decay.
    pub weight_decay: f64,
    /// Gradient clip norm.
    pub grad_clip: f64,
    /// Batch size.
    pub batch_size: u64,
    /// Spherical-score reward weight.
    pub proper_reward_w_sph: f64,
    /// RPS reward weight.
    pub proper_reward_w_rps: f64,
    /// `cosine`.
    pub schedule: String,
    /// Shots per class.
    pub shots_per_class: u64,
    /// Epochs.
    pub epochs: u64,
    /// Training seed.
    pub seed: i64,
    /// The declared base (D-04).
    pub base: BaseDecl,
}

/// `gate-report.json` `thresholds`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateThresholds {
    /// Required macro-F1 margin over zero-shot.
    pub min_macro_f1_margin: f64,
    /// ECE ceiling after calibration.
    pub max_ece: f64,
    /// ECE bins.
    pub ece_bins: u64,
}

/// `gate-report.json` `zero_shot`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZeroShotMetrics {
    /// Macro-F1.
    pub macro_f1: f64,
    /// Stance F_avg; `null` for non-stance tasks.
    pub f_avg: Option<f64>,
    /// Top-label ECE.
    pub ece: f64,
    /// Eval rows.
    pub n: u64,
}

/// `gate-report.json` `fine_tuned`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FineTunedMetrics {
    /// Macro-F1.
    pub macro_f1: f64,
    /// Stance F_avg; `null` for non-stance tasks.
    pub f_avg: Option<f64>,
    /// ECE before calibration.
    pub ece_pre: f64,
    /// ECE after calibration.
    pub ece_post: f64,
    /// Eval NLL.
    pub nll: f64,
    /// Eval rows.
    pub n: u64,
}

/// `gate-report.json` `calibration`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateCalibration {
    /// Temperature bucket key (`choice:3-5`).
    pub bucket: String,
    /// Fitted temperature.
    pub t_fitted: f64,
    /// Applied (clamped) temperature.
    pub t_applied: f64,
    /// Whether the fit hit a bound.
    pub clamp_hit: bool,
    /// Calibration slice size.
    pub slice_size: u64,
    /// Sorted 0-based `train.jsonl` row indices of the slice.
    pub slice_ids: Vec<u64>,
    /// sha256 of the compact JSON of `slice_ids`.
    pub slice_ids_sha256: String,
}

/// `gate-report.json` `seeds`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateSeeds {
    /// The declared seed.
    pub declared: i64,
    /// Seeds run.
    pub n: u64,
    /// `single seed` or `mean ± sd over N seeds`.
    pub label: String,
}

/// `gate-report.json` (laya-finetune-gate-v1 `gate_report_schema`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateReport {
    /// `laya-gate-report-v1`.
    pub schema: String,
    /// The trainer's verdict (08-09 recomputes it; this module does not judge it).
    pub pass: bool,
    /// Thresholds the trainer used.
    pub thresholds: GateThresholds,
    /// Zero-shot metrics on the eval rows.
    pub zero_shot: ZeroShotMetrics,
    /// Fine-tuned metrics on the eval rows.
    pub fine_tuned: FineTunedMetrics,
    /// `fine_tuned.macro_f1 - zero_shot.macro_f1`.
    pub margin: f64,
    /// The temperature fit.
    pub calibration: GateCalibration,
    /// Seed record.
    pub seeds: GateSeeds,
    /// Device read back from the parameters.
    pub device_used: String,
    /// Whether that device is the CPU.
    pub device_is_cpu: bool,
    /// torch version.
    pub torch_version: String,
    /// sha256 of `recipe.json`.
    pub recipe_id: String,
    /// Input hashes.
    pub inputs_sha256: InputsSha256,
    /// sha256 of `eval-probs.json`.
    pub eval_probs_sha256: String,
    /// sha256 of `zero-shot-probs.json`.
    pub zero_shot_probs_sha256: String,
    /// sha256 of `probes.json`.
    pub probes_sha256: String,
}

/// The gate report `schema` value this packer reads.
pub const GATE_REPORT_SCHEMA: &str = "laya-gate-report-v1";

/// `probes.json` (laya-finetune-gate-v1 `probes_json_schema`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbesFile {
    /// One record per decide-apr-v1 probe input, in order.
    pub probes: Vec<ProbeRecord>,
}

/// Everything one artifact is packed from, already read and cross-checked.
#[derive(Debug, Clone, PartialEq)]
pub struct PackInputs {
    /// Every checkpoint tensor, sorted by name.
    pub tensors: Vec<CheckpointTensor>,
    /// `checkpoint/encoder/config.json`, byte-exact.
    pub encoder_config: Vec<u8>,
    /// `checkpoint/rl_agent_config.json`, byte-exact.
    pub agent_config: Vec<u8>,
    /// `checkpoint/tokenizer/tokenizer.json`, byte-exact.
    pub tokenizer: Vec<u8>,
    /// `task.json`, byte-exact (equal to the data dir's).
    pub task_json: Vec<u8>,
    /// `recipe.json`, byte-exact (its sha256 is the recipe_id).
    pub recipe_json: Vec<u8>,
    /// `gate-report.json`, byte-exact.
    pub gate_report_json: Vec<u8>,
    /// Parsed `recipe.json`.
    pub recipe: Recipe,
    /// Parsed `gate-report.json`.
    pub gate_report: GateReport,
    /// Parsed `probes.json`: Laya's OWN probe probabilities (Python values).
    pub probes: Vec<ProbeRecord>,
    /// Input hashes recomputed from the data dir, tokenizer and declared base.
    pub inputs_sha256: InputsSha256,
}

/// Why a run dir could not be packed.
#[derive(Debug, Clone, PartialEq)]
pub enum PackError {
    /// A run-dir or data-dir file could not be read.
    Read {
        /// The path.
        path: PathBuf,
        /// The I/O error.
        reason: String,
    },
    /// `model.safetensors` could not be parsed.
    SafeTensors(String),
    /// A checkpoint tensor has a dtype the artifact does not carry.
    UnsupportedDtype {
        /// Tensor name.
        name: String,
        /// The safetensors dtype.
        dtype: String,
    },
    /// A run-dir JSON file did not parse under its contract schema.
    Schema {
        /// File name.
        file: &'static str,
        /// The serde error.
        reason: String,
    },
    /// The gate report names a schema this packer does not read.
    GateReportSchema {
        /// The `schema` value found.
        observed: String,
    },
    /// The run dir's `task.json` is not a byte copy of the data dir's.
    TaskCopyDiffers,
    /// A hash the gate report records disagrees with the file beside it.
    ReportHashMismatch {
        /// Which recorded hash.
        what: &'static str,
        /// Recorded in the report.
        recorded: String,
        /// Recomputed from the file.
        observed: String,
    },
    /// The artifact writer refused.
    Artifact(ArtifactError),
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, reason } => write!(f, "pack: read {}: {reason}", path.display()),
            Self::SafeTensors(e) => write!(f, "pack: model.safetensors: {e}"),
            Self::UnsupportedDtype { name, dtype } => {
                write!(f, "pack: tensor {name} has dtype {dtype}; only F16 (and F32 temperature) are carried")
            }
            Self::Schema { file, reason } => write!(f, "pack: {file}: {reason}"),
            Self::GateReportSchema { observed } => write!(
                f,
                "pack: gate-report.json schema is {observed:?}, expected {GATE_REPORT_SCHEMA:?}"
            ),
            Self::TaskCopyDiffers => write!(
                f,
                "pack: the run dir's task.json is not a byte copy of the data dir's task.json"
            ),
            Self::ReportHashMismatch {
                what,
                recorded,
                observed,
            } => write!(
                f,
                "pack: gate report records {what} = {recorded}, the file hashes to {observed}"
            ),
            Self::Artifact(e) => write!(f, "pack: {e}"),
        }
    }
}

impl std::error::Error for PackError {}

impl From<ArtifactError> for PackError {
    fn from(e: ArtifactError) -> Self {
        Self::Artifact(e)
    }
}

fn read_file(dir: &Path, rel: &str) -> Result<Vec<u8>, PackError> {
    let path = dir.join(rel);
    std::fs::read(&path).map_err(|e| PackError::Read {
        path,
        reason: e.to_string(),
    })
}

fn parse<T: serde::de::DeserializeOwned>(file: &'static str, bytes: &[u8]) -> Result<T, PackError> {
    serde_json::from_slice(bytes).map_err(|e| PackError::Schema {
        file,
        reason: e.to_string(),
    })
}

fn check_hash(what: &'static str, recorded: &str, bytes: &[u8]) -> Result<(), PackError> {
    let observed = sha256_hex(bytes);
    if observed == recorded {
        Ok(())
    } else {
        Err(PackError::ReportHashMismatch {
            what,
            recorded: recorded.to_string(),
            observed,
        })
    }
}

/// Every tensor of `model.safetensors` as raw bytes, sorted by name. F16 stays F16 and
/// F32 stays F32; any other dtype is a typed refusal.
fn read_checkpoint_tensors(bytes: &[u8]) -> Result<Vec<CheckpointTensor>, PackError> {
    let st = safetensors::SafeTensors::deserialize(bytes)
        .map_err(|e| PackError::SafeTensors(e.to_string()))?;
    let mut out = st
        .tensors()
        .into_iter()
        .map(|(name, view)| {
            let dtype = match view.dtype() {
                safetensors::Dtype::F16 => TensorDType::F16,
                safetensors::Dtype::F32 => TensorDType::F32,
                other => {
                    return Err(PackError::UnsupportedDtype {
                        name,
                        dtype: format!("{other:?}"),
                    })
                }
            };
            Ok(CheckpointTensor {
                name,
                dtype,
                shape: view.shape().to_vec(),
                bytes: view.data().to_vec(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

impl PackInputs {
    /// Read and cross-check a run dir (laya-finetune-gate-v1 `run_dir_layout`) and its
    /// data dir.
    ///
    /// Refuses when the run-dir `task.json` differs from the data dir's, when the gate
    /// report's `recipe_id`, `inputs_sha256`, `eval_probs_sha256`,
    /// `zero_shot_probs_sha256` or `probes_sha256` disagree with the files, or when any
    /// JSON file carries a field its contract schema does not declare.
    ///
    /// # Errors
    ///
    /// A [`PackError`] naming the file or hash that failed.
    pub fn from_run_dir(run_dir: &Path, data_dir: &Path) -> Result<Self, PackError> {
        let task_json = read_file(run_dir, "task.json")?;
        let data_task = read_file(data_dir, "task.json")?;
        if task_json != data_task {
            return Err(PackError::TaskCopyDiffers);
        }
        let train = read_file(data_dir, "train.jsonl")?;
        let eval = read_file(data_dir, "eval.jsonl")?;
        let tokenizer = read_file(run_dir, "checkpoint/tokenizer/tokenizer.json")?;
        let encoder_config = read_file(run_dir, "checkpoint/encoder/config.json")?;
        let agent_config = read_file(run_dir, "checkpoint/rl_agent_config.json")?;
        let recipe_json = read_file(run_dir, "recipe.json")?;
        let gate_report_json = read_file(run_dir, "gate-report.json")?;
        let probes_json = read_file(run_dir, "probes.json")?;
        let eval_probs = read_file(run_dir, "eval-probs.json")?;
        let zero_shot_probs = read_file(run_dir, "zero-shot-probs.json")?;

        let recipe: Recipe = parse("recipe.json", &recipe_json)?;
        let gate_report: GateReport = parse("gate-report.json", &gate_report_json)?;
        let probes: ProbesFile = parse("probes.json", &probes_json)?;
        if gate_report.schema != GATE_REPORT_SCHEMA {
            return Err(PackError::GateReportSchema {
                observed: gate_report.schema,
            });
        }

        check_hash("recipe_id", &gate_report.recipe_id, &recipe_json)?;
        check_hash("probes_sha256", &gate_report.probes_sha256, &probes_json)?;
        check_hash(
            "eval_probs_sha256",
            &gate_report.eval_probs_sha256,
            &eval_probs,
        )?;
        check_hash(
            "zero_shot_probs_sha256",
            &gate_report.zero_shot_probs_sha256,
            &zero_shot_probs,
        )?;
        let recorded = &gate_report.inputs_sha256;
        check_hash("inputs_sha256.task_json", &recorded.task_json, &data_task)?;
        check_hash("inputs_sha256.train_jsonl", &recorded.train_jsonl, &train)?;
        check_hash("inputs_sha256.eval_jsonl", &recorded.eval_jsonl, &eval)?;
        check_hash(
            "inputs_sha256.tokenizer_json",
            &recorded.tokenizer_json,
            &tokenizer,
        )?;
        if recorded.base_model != recipe.base.sha256 {
            return Err(PackError::ReportHashMismatch {
                what: "inputs_sha256.base_model",
                recorded: recorded.base_model.clone(),
                observed: recipe.base.sha256.clone(),
            });
        }
        let inputs_sha256 = InputsSha256 {
            task_json: sha256_hex(&data_task),
            train_jsonl: sha256_hex(&train),
            eval_jsonl: sha256_hex(&eval),
            base_model: recipe.base.sha256.clone(),
            tokenizer_json: sha256_hex(&tokenizer),
        };

        let tensors =
            read_checkpoint_tensors(&read_file(run_dir, "checkpoint/model.safetensors")?)?;
        Ok(Self {
            tensors,
            encoder_config,
            agent_config,
            tokenizer,
            task_json,
            recipe_json,
            gate_report_json,
            recipe,
            gate_report,
            probes: probes.probes,
            inputs_sha256,
        })
    }
}

/// Pack a run dir into `decide-apr-v1` bytes: [`PackInputs::from_run_dir`] then
/// [`artifact::write_decide_apr`]. No gate or variant policy (see the module docs).
///
/// # Errors
///
/// A [`PackError`] from reading, cross-checking or writing.
#[provable_contracts_macros::contract("decide-apr-v1", equation = "determinism")]
pub fn pack_run_dir(run_dir: &Path, data_dir: &Path) -> Result<Vec<u8>, PackError> {
    let inputs = PackInputs::from_run_dir(run_dir, data_dir)?;
    Ok(artifact::write_decide_apr(&inputs)?)
}

#[cfg(test)]
mod tests {
    use super::{PackError, PackInputs};
    use crate::test_support::fixture_dir;
    use std::path::{Path, PathBuf};

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

    /// A private copy of the tiny run dir, so a test can corrupt one file.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("decide-0805-{name}-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("clear scratch");
        }
        copy_dir(&fixture_dir(), &dir);
        dir
    }

    fn append(path: &Path, extra: &[u8]) {
        let mut bytes = std::fs::read(path).expect("read");
        bytes.extend_from_slice(extra);
        std::fs::write(path, bytes).expect("write");
    }

    #[test]
    fn task_copy_must_match() {
        let dir = scratch("task-copy");
        append(&dir.join("task.json"), b" ");
        let e = PackInputs::from_run_dir(&dir, &dir.join("data")).expect_err("copy differs");
        assert_eq!(e, PackError::TaskCopyDiffers);
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn report_hash_must_match() {
        let dir = scratch("probes-hash");
        append(&dir.join("probes.json"), b"\n");
        let e = PackInputs::from_run_dir(&dir, &dir.join("data")).expect_err("hash differs");
        assert!(
            matches!(
                &e,
                PackError::ReportHashMismatch {
                    what: "probes_sha256",
                    ..
                }
            ),
            "{e}"
        );
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn unknown_recipe_key_is_refused() {
        let dir = scratch("recipe-key");
        let path = dir.join("recipe.json");
        let text = std::fs::read_to_string(&path).expect("recipe");
        let text = text.replacen('{', "{\n  \"extra\": 1,", 1);
        std::fs::write(&path, text).expect("write recipe");
        let e = PackInputs::from_run_dir(&dir, &dir.join("data")).expect_err("unknown key");
        assert!(
            matches!(&e, PackError::Schema { file: "recipe.json", reason } if reason.contains("extra")),
            "{e}"
        );
        std::fs::remove_dir_all(&dir).expect("clean up");
    }
}
