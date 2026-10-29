//! Dataset validation: the pre-filter the request function runs before it mints
//! anything (D-08).
//!
//! # Inspection versus submission (D-21)
//!
//! [`validate_files`] INSPECTS. It runs every data rule and REPORTS what `train`
//! would need (`epochs_required`, `epochs_range`, `epochs_fixed`), but it never
//! applies the epochs refusal: `validate_dataset` has no `epochs` argument, so
//! refusing for its absence would make an above-16-shots dataset impossible to
//! validate. Only [`validate_for_submit`], which `train` calls, enforces the
//! epoch rule, through `args::resolve_epochs`.
//!
//! # No numerics
//!
//! Nothing here is a training number. The three epoch values and the contract
//! hash come from the embedded gate contract; the data rules mirror
//! `scripts/laya_train/data.py` and refuse with the same rule names. Detail
//! strings name files, line numbers and counts only, never row text (V7).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use aprender_decide::digest::sha256_hex;
use aprender_decide::task::Task;
use serde::{Deserialize, Serialize};

use crate::args::{self, EpochRule};
use crate::layout::{MAX_FILE_BYTES, MAX_ROWS_PER_FILE, MAX_TOTAL_BYTES};

/// The gate contract, embedded so the request function judges a dataset by the
/// same numbers the trainer will read, and so every run can record which bytes
/// judged it (`ValidationPolicy::gate_contract_sha256`).
pub const GATE_CONTRACT_YAML: &str = include_str!("../../../contracts/laya-finetune-gate-v1.yaml");

/// A refused request. Displays as `REFUSED <rule>: <detail>`, the trainer's own voice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// The rule that refused.
    pub rule: String,
    /// What is wrong: files, line numbers and counts, never row text.
    pub detail: String,
}

impl Refusal {
    /// A refusal under `rule`.
    #[must_use]
    pub fn new(rule: &str, detail: impl Into<String>) -> Self {
        Self {
            rule: rule.to_string(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "REFUSED {}: {}", self.rule, self.detail)
    }
}

impl std::error::Error for Refusal {}

/// The four files of a data dir, as raw bytes.
#[derive(Debug, Clone, Default)]
pub struct DatasetFiles {
    /// `task.json`.
    pub task_json: Option<Vec<u8>>,
    /// `train.jsonl`.
    pub train: Option<Vec<u8>>,
    /// `eval.jsonl`.
    pub eval: Option<Vec<u8>>,
    /// `shift.jsonl` (the optional shift probe).
    pub shift: Option<Vec<u8>>,
}

/// Row counts per file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowCounts {
    /// `train.jsonl` rows.
    pub train: u64,
    /// `eval.jsonl` rows.
    pub eval: u64,
    /// `shift.jsonl` rows, when present.
    pub shift: Option<u64>,
}

/// What `validate_dataset` answers for a dataset every data rule accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetSummary {
    /// The criterion names in label-index order.
    pub labels: Vec<String>,
    /// Train rows per label (zero-filled).
    pub class_counts: BTreeMap<String, u64>,
    /// The largest train class count (the trainer's own definition).
    pub shots_per_class: u64,
    /// Row counts per file.
    pub rows: RowCounts,
    /// Whether `shift.jsonl` is present.
    pub shift_present: bool,
    /// True when `train` will need `epochs` (above 16 shots per class).
    pub epochs_required: bool,
    /// The accepted `epochs` range when it is required.
    pub epochs_range: Option<[u32; 2]>,
    /// The fixed epoch count when `epochs` is not accepted (16 or fewer shots).
    pub epochs_fixed: Option<u32>,
    /// File name to lowercase hex sha256 of the validated bytes.
    pub inputs_sha256: BTreeMap<String, String>,
    /// Rules the pre-filter has not applied yet (empty until plan 14-02).
    pub deferred_rules: Vec<String>,
}

/// What the pre-filter judges by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationPolicy {
    /// The epoch rule, read from the contract's `recipe` block.
    pub epoch_rule: EpochRule,
    /// sha256 (hex) of [`GATE_CONTRACT_YAML`]'s bytes.
    pub gate_contract_sha256: String,
    /// Largest single file.
    pub max_file_bytes: usize,
    /// Largest dataset in total.
    pub max_total_bytes: usize,
    /// Most rows in one file.
    pub max_rows_per_file: usize,
}

#[derive(Deserialize)]
struct ContractView {
    recipe: RecipeView,
}

#[derive(Deserialize)]
struct RecipeView {
    epochs_at_most_16_per_class: u32,
    epochs_above_16_min: u32,
    epochs_above_16_max: u32,
}

impl ValidationPolicy {
    /// Read the policy from the embedded gate contract.
    ///
    /// # Errors
    ///
    /// A message when the embedded contract does not parse (a build defect: a
    /// test covers it).
    pub fn from_contract() -> Result<Self, String> {
        let view: ContractView = serde_yaml::from_str(GATE_CONTRACT_YAML)
            .map_err(|e| format!("embedded gate contract does not parse: {e}"))?;
        Ok(Self {
            epoch_rule: EpochRule {
                fixed: view.recipe.epochs_at_most_16_per_class,
                lo: view.recipe.epochs_above_16_min,
                hi: view.recipe.epochs_above_16_max,
            },
            gate_contract_sha256: sha256_hex(GATE_CONTRACT_YAML.as_bytes()),
            max_file_bytes: MAX_FILE_BYTES,
            max_total_bytes: MAX_TOTAL_BYTES,
            max_rows_per_file: MAX_ROWS_PER_FILE,
        })
    }

    /// [`Self::from_contract`], parsed once per process.
    ///
    /// # Errors
    ///
    /// As [`Self::from_contract`].
    pub fn shared() -> Result<&'static Self, String> {
        static POLICY: OnceLock<Result<ValidationPolicy, String>> = OnceLock::new();
        POLICY
            .get_or_init(Self::from_contract)
            .as_ref()
            .map_err(Clone::clone)
    }
}

fn bound_check(files: &DatasetFiles, policy: &ValidationPolicy) -> Result<(), Refusal> {
    let named = [
        ("task.json", &files.task_json),
        ("train.jsonl", &files.train),
        ("eval.jsonl", &files.eval),
        ("shift.jsonl", &files.shift),
    ];
    let mut total = 0usize;
    for (name, bytes) in named {
        let len = bytes.as_ref().map_or(0, Vec::len);
        if len > policy.max_file_bytes {
            return Err(Refusal::new(
                "bound-file",
                format!(
                    "{name} is {len} bytes; at most {} bytes per file",
                    policy.max_file_bytes
                ),
            ));
        }
        total += len;
    }
    if total > policy.max_total_bytes {
        return Err(Refusal::new(
            "bound-total",
            format!(
                "the dataset is {total} bytes in total; at most {} bytes",
                policy.max_total_bytes
            ),
        ));
    }
    Ok(())
}

/// Parse one jsonl file into label indices, refusing by `data.py`'s rule names.
fn parse_rows(
    bytes: &[u8],
    role: &str,
    file: &str,
    labels: &[String],
    policy: &ValidationPolicy,
) -> Result<Vec<usize>, Refusal> {
    let mut lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if lines.len() > policy.max_rows_per_file {
        return Err(Refusal::new(
            "bound-rows",
            format!(
                "{file} has {} rows; at most {} rows per file",
                lines.len(),
                policy.max_rows_per_file
            ),
        ));
    }
    if lines.is_empty() {
        return Err(Refusal::new(
            &format!("{role}-empty"),
            format!("{file} has no rows"),
        ));
    }
    let row_rule = |suffix: &str| format!("{role}-row-{suffix}");
    let mut out = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let n = index + 1;
        if std::str::from_utf8(line).is_err() {
            return Err(Refusal::new(
                &row_rule("schema"),
                format!("{file} line {n} is not valid UTF-8"),
            ));
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            return Err(Refusal::new(
                &row_rule("schema"),
                format!("{file} line {n} is blank"),
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(line).map_err(|_| {
            Refusal::new(&row_rule("schema"), format!("{file} line {n} is not JSON"))
        })?;
        let Some(object) = value.as_object() else {
            return Err(Refusal::new(
                &row_rule("schema"),
                format!("{file} line {n} is not a JSON object"),
            ));
        };
        let mut extra: Vec<&str> = object
            .keys()
            .map(String::as_str)
            .filter(|k| *k != "text" && *k != "label")
            .collect();
        if !extra.is_empty() {
            extra.sort_unstable();
            return Err(Refusal::new(
                &row_rule("unknown-key"),
                format!("{file} line {n} has unknown key(s) {extra:?}"),
            ));
        }
        let (Some(text), Some(label)) = (object.get("text"), object.get("label")) else {
            return Err(Refusal::new(
                &row_rule("schema"),
                format!("{file} line {n} must hold exactly [\"text\", \"label\"]"),
            ));
        };
        if !text.as_str().is_some_and(|t| !t.trim().is_empty()) {
            return Err(Refusal::new(
                &row_rule("schema"),
                format!("{file} line {n} text must be a non-empty string"),
            ));
        }
        let Some(label) = label.as_str() else {
            return Err(Refusal::new(
                &row_rule("label"),
                format!("{file} line {n} label must be a criterion name (a string)"),
            ));
        };
        let Some(position) = labels.iter().position(|l| l == label) else {
            return Err(Refusal::new(
                &row_rule("label"),
                format!("{file} line {n} label is not a criterion of task.json"),
            ));
        };
        out.push(position);
    }
    Ok(out)
}

/// INSPECT a dataset: every data rule, and a report of what `train` will need.
/// Never refuses `epochs` (see the module doc): `validate_dataset` calls this.
///
/// # Errors
///
/// The first [`Refusal`] a data rule raises.
pub fn validate_files(
    files: &DatasetFiles,
    policy: &ValidationPolicy,
) -> Result<DatasetSummary, Refusal> {
    bound_check(files, policy)?;
    let task_bytes = files
        .task_json
        .as_deref()
        .ok_or_else(|| Refusal::new("task-missing", "task.json does not exist (it is required)"))?;
    let task = Task::from_slice(task_bytes)
        .map_err(|e| Refusal::new("task-schema", format!("task.json is refused: {e:?}")))?;
    let train_bytes = files.train.as_deref().ok_or_else(|| {
        Refusal::new(
            "train-missing",
            "train.jsonl does not exist (it is required)",
        )
    })?;
    let eval_bytes = files.eval.as_deref().ok_or_else(|| {
        Refusal::new("eval-missing", "eval.jsonl does not exist (it is required)")
    })?;
    let labels = task.owned_labels();
    let train = parse_rows(train_bytes, "train", "train.jsonl", &labels, policy)?;
    let eval = parse_rows(eval_bytes, "eval", "eval.jsonl", &labels, policy)?;
    let shift = match files.shift.as_deref() {
        Some(bytes) => Some(parse_rows(bytes, "shift", "shift.jsonl", &labels, policy)?),
        None => None,
    };

    let mut counts = vec![0u64; labels.len()];
    for label in &train {
        counts[*label] += 1;
    }
    let shots_per_class = counts.iter().copied().max().unwrap_or(0);
    let class_counts = labels.iter().cloned().zip(counts).collect();

    let rule = &policy.epoch_rule;
    let epochs_required = shots_per_class > args::EPOCH_THRESHOLD;
    let mut inputs_sha256 = BTreeMap::new();
    inputs_sha256.insert("task.json".to_string(), sha256_hex(task_bytes));
    inputs_sha256.insert("train.jsonl".to_string(), sha256_hex(train_bytes));
    inputs_sha256.insert("eval.jsonl".to_string(), sha256_hex(eval_bytes));
    if let Some(bytes) = files.shift.as_deref() {
        inputs_sha256.insert("shift.jsonl".to_string(), sha256_hex(bytes));
    }
    Ok(DatasetSummary {
        labels,
        class_counts,
        shots_per_class,
        rows: RowCounts {
            train: train.len() as u64,
            eval: eval.len() as u64,
            shift: shift.as_ref().map(|s| s.len() as u64),
        },
        shift_present: shift.is_some(),
        epochs_required,
        epochs_range: epochs_required.then_some([rule.lo, rule.hi]),
        epochs_fixed: (!epochs_required).then_some(rule.fixed),
        inputs_sha256,
        deferred_rules: Vec::new(),
    })
}

/// SUBMIT a dataset: [`validate_files`], then the epoch rule on its
/// `shots_per_class`. The only place the epoch rule refuses; `train` and the
/// shared-table replay (plan 14-02) call this.
///
/// Returns the summary and the epoch count to pass to the trainer (`None` at 16
/// shots per class or fewer, where the recipe fixes it).
///
/// # Errors
///
/// A data-rule [`Refusal`], else rule `epochs`.
pub fn validate_for_submit(
    files: &DatasetFiles,
    epochs: Option<u32>,
    policy: &ValidationPolicy,
) -> Result<(DatasetSummary, Option<u32>), Refusal> {
    let summary = validate_files(files, policy)?;
    let resolved = args::resolve_epochs(summary.shots_per_class, epochs, &policy.epoch_rule)?;
    Ok((summary, resolved))
}

#[cfg(test)]
pub(crate) mod test_data {
    use super::DatasetFiles;

    /// A 3-class dataset with `per_class` train rows per class, 2 eval rows per class.
    pub fn dataset(per_class: usize) -> DatasetFiles {
        let task = br#"{"type":"choice","instructions":"Which team?","criteria":{"shipping":"parcels","billing":"charges","account":"login"}}"#;
        let mut train = String::new();
        let mut eval = String::new();
        for label in ["shipping", "billing", "account"] {
            for i in 0..per_class {
                train.push_str(&format!(
                    "{{\"text\": \"train {label} message number {i}\", \"label\": \"{label}\"}}\n"
                ));
            }
            for i in 0..2 {
                eval.push_str(&format!(
                    "{{\"text\": \"eval {label} message number {i}\", \"label\": \"{label}\"}}\n"
                ));
            }
        }
        DatasetFiles {
            task_json: Some(task.to_vec()),
            train: Some(train.into_bytes()),
            eval: Some(eval.into_bytes()),
            shift: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_data::dataset;
    use super::*;

    fn policy() -> ValidationPolicy {
        ValidationPolicy::from_contract().expect("the embedded contract parses")
    }

    #[test]
    fn policy_reads_the_contract_and_hashes_its_bytes() {
        let p = policy();
        assert_eq!(
            p.epoch_rule,
            EpochRule {
                fixed: 12,
                lo: 4,
                hi: 12
            }
        );
        assert_eq!(
            p.gate_contract_sha256,
            sha256_hex(GATE_CONTRACT_YAML.as_bytes())
        );
        assert_eq!(p.gate_contract_sha256.len(), 64);
        assert!(ValidationPolicy::shared().is_ok());
    }

    #[test]
    fn validate_reports_epochs_above_16() {
        let p = policy();
        let above = validate_files(&dataset(20), &p).expect("valid");
        assert_eq!(above.shots_per_class, 20);
        assert!(above.epochs_required);
        assert_eq!(above.epochs_range, Some([4, 12]));
        assert_eq!(above.epochs_fixed, None);
        let at = validate_files(&dataset(16), &p).expect("valid");
        assert_eq!(at.shots_per_class, 16);
        assert!(!at.epochs_required);
        assert_eq!(at.epochs_range, None);
        assert_eq!(at.epochs_fixed, Some(12));
        assert_eq!(at.class_counts["billing"], 16);
        assert_eq!(
            at.rows,
            RowCounts {
                train: 48,
                eval: 6,
                shift: None
            }
        );
        assert!(at.deferred_rules.is_empty());
    }

    #[test]
    fn validate_files_never_refuses_epochs() {
        let p = policy();
        for n in [1, 4, 16, 17, 20, 40] {
            if let Err(r) = validate_files(&dataset(n), &p) {
                assert_ne!(r.rule, "epochs", "{r}");
            }
        }
    }

    #[test]
    fn validate_for_submit_enforces_epochs() {
        let p = policy();
        let err = validate_for_submit(&dataset(20), None, &p).expect_err("needs epochs");
        assert_eq!(err.rule, "epochs");
        assert!(err.detail.contains("[4, 12]"), "{err}");
        let (_, e) = validate_for_submit(&dataset(20), Some(4), &p).expect("4 ok");
        assert_eq!(e, Some(4));
        let (_, e) = validate_for_submit(&dataset(16), None, &p).expect("fixed");
        assert_eq!(e, None);
        let err = validate_for_submit(&dataset(16), Some(12), &p).expect_err("fixed, not ignored");
        assert_eq!(err.rule, "epochs");
    }

    #[test]
    fn inputs_hashes_are_the_file_bytes() {
        let p = policy();
        let files = dataset(5);
        let s = validate_files(&files, &p).expect("valid");
        assert_eq!(
            s.inputs_sha256["train.jsonl"],
            sha256_hex(files.train.as_deref().expect("train"))
        );
        assert!(!s.inputs_sha256.contains_key("shift.jsonl"));
    }

    #[test]
    fn missing_and_malformed_inputs_are_refused_by_rule() {
        let p = policy();
        let mut f = dataset(5);
        f.task_json = None;
        assert_eq!(validate_files(&f, &p).expect_err("x").rule, "task-missing");
        let mut f = dataset(5);
        f.task_json = Some(b"{}".to_vec());
        assert_eq!(validate_files(&f, &p).expect_err("x").rule, "task-schema");
        let mut f = dataset(5);
        f.train = None;
        assert_eq!(validate_files(&f, &p).expect_err("x").rule, "train-missing");
        let mut f = dataset(5);
        f.eval = None;
        assert_eq!(validate_files(&f, &p).expect_err("x").rule, "eval-missing");
        let mut f = dataset(5);
        f.train = Some(b"{\"text\": \"a b\", \"label\": \"nope\"}\n".to_vec());
        assert_eq!(
            validate_files(&f, &p).expect_err("x").rule,
            "train-row-label"
        );
        let mut f = dataset(5);
        f.eval = Some(b"{\"text\": \"a b\", \"label\": \"nope\"}\n".to_vec());
        assert_eq!(
            validate_files(&f, &p).expect_err("x").rule,
            "eval-row-label"
        );
    }

    #[test]
    fn detail_strings_never_echo_row_text() {
        let p = policy();
        let mut f = dataset(5);
        f.train = Some(b"{\"text\": \"SECRET-ROW-TEXT\", \"label\": \"SECRET-LABEL\"}\n".to_vec());
        let err = validate_files(&f, &p).expect_err("bad label");
        let shown = err.to_string();
        assert!(!shown.contains("SECRET"), "{shown}");
        assert!(shown.contains("line 1"), "{shown}");
    }

    #[test]
    fn transport_bounds_are_refused() {
        let mut p = policy();
        p.max_file_bytes = 100;
        assert_eq!(
            validate_files(&dataset(5), &p).expect_err("x").rule,
            "bound-file"
        );
        let mut p = policy();
        p.max_total_bytes = 100;
        assert_eq!(
            validate_files(&dataset(5), &p).expect_err("x").rule,
            "bound-total"
        );
        let mut p = policy();
        p.max_rows_per_file = 3;
        assert_eq!(
            validate_files(&dataset(5), &p).expect_err("x").rule,
            "bound-rows"
        );
    }
}
