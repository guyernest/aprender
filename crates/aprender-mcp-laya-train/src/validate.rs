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
//! # One rule set, two implementations, one table (D-08)
//!
//! The rules mirror `scripts/laya_train/data.py` `validate_data_dir`, the function
//! the trainer calls, in the same order and under the same rule names.
//! `scripts/laya_train/data_cases.json` is replayed by `data.py --selftest`
//! through that function and by [`tests::shared_case_table`] through
//! [`validate_for_submit`], so the two cannot drift silently.
//!
//! Two differences are DECLARED, bounded, and visible in the table:
//!
//! * [`DEFERRABLE_RULES`]: `train-class-too-small` depends on a seeded permutation
//!   when a class holds a text that appears in two or more rows. Rust refuses only
//!   the order-independent case (`count <= max(min_per_class, ceil(fraction *
//!   count))`); the undecidable case is ACCEPTED and reported in
//!   [`DatasetSummary::deferred_rules`], never silently dropped.
//! * [`STRICTER_RULES`]: a `task.json` with more than
//!   `aprender_decide::task::MAX_CRITERIA` criteria is refused `task-serving-limit`.
//!   The trainer accepts it, but `pack` reads task.json through `Task::from_slice`
//!   and refuses it too, so such a run could never yield an `.apr`.
//!
//! # No numerics
//!
//! Nothing here is a training number. The epoch values, the calibration fraction
//! and minimum per class, and the contract hash come from the embedded gate
//! contract. Detail strings name files, line numbers and counts only, never row
//! text (V7).

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::OnceLock;

use aprender_decide::digest::sha256_hex;
use aprender_decide::task::{Task, MAX_CRITERIA};
use aprender_decide::verify::normalized_sha256;
use serde::de::{Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use crate::args::{self, EpochRule};
use crate::layout::{MAX_FILE_BYTES, MAX_ROWS_PER_FILE, MAX_TOTAL_BYTES};

/// The gate contract, embedded so the request function judges a dataset by the
/// same numbers the trainer will read, and so every run can record which bytes
/// judged it (`ValidationPolicy::gate_contract_sha256`).
pub const GATE_CONTRACT_YAML: &str = include_str!("../../../contracts/laya-finetune-gate-v1.yaml");

/// The only rules the pre-filter may defer to the trainer (see the module doc).
/// Mirrored by the case table's header `deferrable_rules`.
pub const DEFERRABLE_RULES: [&str; 1] = ["train-class-too-small"];

/// The only rules where the pre-filter is stricter than the trainer (see the
/// module doc). Mirrored by the case table's header `stricter_rules`.
pub const STRICTER_RULES: [&str; 1] = ["task-serving-limit"];

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
    /// Rules the pre-filter could not decide in the request path and leaves to the
    /// trainer: a subset of [`DEFERRABLE_RULES`], empty when everything was decided.
    pub deferred_rules: Vec<String>,
}

/// What the pre-filter judges by.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationPolicy {
    /// The epoch rule, read from the contract's `recipe` block.
    pub epoch_rule: EpochRule,
    /// `constants.calibration_slice_fraction`: the share of each class the trainer
    /// holds out for calibration.
    pub calibration_fraction: f64,
    /// `constants.calibration_slice_min_per_class`.
    pub calibration_min_per_class: u64,
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
    constants: ConstantsView,
}

#[derive(Deserialize)]
struct ConstantsView {
    calibration_slice_fraction: f64,
    calibration_slice_min_per_class: u64,
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
            calibration_fraction: view.constants.calibration_slice_fraction,
            calibration_min_per_class: view.constants.calibration_slice_min_per_class,
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

/// How many members of one JSON object are kept. Members past it are counted and
/// skipped, so a hostile object costs at most this much memory (the cap is
/// `MAX_CRITERIA + 1`: one more than any accepted `criteria` object holds, so
/// "over the serving limit" is still observable).
const OBJECT_STORE_CAP: usize = MAX_CRITERIA + 1;

/// Just enough of a JSON value for the dataset rules.
///
/// Objects are read as a list of `(key, value)` pairs in document order, never
/// through `serde_json::Value`: that type keeps only the LAST of a repeated key
/// (and, with `preserve_order` linked in by pmcp, a different order than
/// `BTreeMap`), while `data.py` reads with `object_pairs_hook=list` and refuses a
/// repeated key, so the duplicate must stay visible here.
#[derive(Debug)]
enum Lite {
    Str(String),
    Null,
    Obj {
        pairs: Vec<(String, Lite)>,
        /// Every member the object had, stored or not.
        total: usize,
    },
    /// `[]`, which `data.py` reads as an object with no members (an
    /// `object_pairs_hook=list` quirk: `all()` over an empty list is true).
    EmptyArray,
    /// A number, a boolean, or a non-empty array.
    Other,
}

impl<'de> Deserialize<'de> for Lite {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct LiteVisitor;

        impl<'de> Visitor<'de> for LiteVisitor {
            type Value = Lite;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("any JSON value")
            }

            fn visit_str<E>(self, v: &str) -> Result<Lite, E> {
                Ok(Lite::Str(v.to_owned()))
            }

            fn visit_string<E>(self, v: String) -> Result<Lite, E> {
                Ok(Lite::Str(v))
            }

            fn visit_unit<E>(self) -> Result<Lite, E> {
                Ok(Lite::Null)
            }

            fn visit_bool<E>(self, _: bool) -> Result<Lite, E> {
                Ok(Lite::Other)
            }

            fn visit_i64<E>(self, _: i64) -> Result<Lite, E> {
                Ok(Lite::Other)
            }

            fn visit_u64<E>(self, _: u64) -> Result<Lite, E> {
                Ok(Lite::Other)
            }

            fn visit_f64<E>(self, _: f64) -> Result<Lite, E> {
                Ok(Lite::Other)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Lite, A::Error> {
                let mut any = false;
                while seq.next_element::<Lite>()?.is_some() {
                    any = true;
                }
                Ok(if any { Lite::Other } else { Lite::EmptyArray })
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Lite, A::Error> {
                let mut pairs = Vec::new();
                let mut total = 0usize;
                while let Some(key) = map.next_key::<String>()? {
                    let value: Lite = map.next_value()?;
                    total += 1;
                    if pairs.len() < OBJECT_STORE_CAP {
                        pairs.push((key, value));
                    }
                }
                Ok(Lite::Obj { pairs, total })
            }
        }

        deserializer.deserialize_any(LiteVisitor)
    }
}

/// Python's `str.isspace` set: Unicode `White_Space` plus U+001C..U+001F, which
/// `str.strip()` also strips (Rust's `char::is_whitespace` omits those four).
/// `data.py` uses `strip()` for the blank-line, empty-text, empty-instructions and
/// empty-criterion-name tests, so this is the set those tests need here.
fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn blank(s: &str) -> bool {
    s.chars().all(py_space)
}

/// A name for a refusal detail: at most 40 characters, so a hostile key cannot
/// make the answer large.
fn short(s: &str) -> String {
    let mut out: String = s.chars().take(40).collect();
    if s.chars().nth(40).is_some() {
        out.push('~');
    }
    out
}

fn first_ten(ids: &[usize]) -> String {
    let shown: Vec<String> = ids.iter().take(10).map(ToString::to_string).collect();
    format!("[{}]", shown.join(", "))
}

fn task_refusal(rule: &str, detail: impl Into<String>) -> Refusal {
    Refusal::new(rule, detail)
}

/// Read and judge `task.json` in `data.py` `load_task` order: parse, a repeated
/// top-level key, an unknown key, a missing key, `type`, `instructions`, the shape
/// of `criteria`, a repeated criterion name, fewer than 2 criteria, an empty
/// criterion name, a non-string description. Then the serving limit (more than
/// `MAX_CRITERIA` criteria, `task-serving-limit`, declared in [`STRICTER_RULES`]),
/// which is checked BEFORE `Task::from_slice` runs, so the one place Rust is
/// stricter is explicit and cannot be mistaken for a parse failure. Finally
/// `Task::from_slice` itself, the reader `pack` uses, supplies the labels.
fn check_task(bytes: &[u8]) -> Result<Task, Refusal> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| task_refusal("task-schema", "task.json is not UTF-8 JSON"))?;
    let doc: Lite = serde_json::from_str(text)
        .map_err(|_| task_refusal("task-schema", "task.json is not JSON"))?;
    let pairs = match doc {
        Lite::Obj { pairs, .. } => pairs,
        Lite::EmptyArray => Vec::new(),
        _ => {
            return Err(task_refusal(
                "task-schema",
                "task.json must be a JSON object",
            ))
        }
    };
    let mut seen: HashSet<&str> = HashSet::new();
    if !pairs.iter().all(|(k, _)| seen.insert(k.as_str())) {
        return Err(task_refusal("task-schema", "task.json repeats a key"));
    }
    let allowed = ["type", "instructions", "criteria"];
    let mut unknown: Vec<String> = pairs
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| !allowed.contains(k))
        .map(short)
        .collect();
    if !unknown.is_empty() {
        unknown.sort_unstable();
        return Err(task_refusal(
            "task-unknown-key",
            format!("task.json has unknown key(s) {unknown:?}; allowed {allowed:?}"),
        ));
    }
    let get = |name: &str| pairs.iter().find(|(k, _)| k == name).map(|(_, v)| v);
    let (Some(kind), Some(instructions), Some(criteria)) =
        (get("type"), get("instructions"), get("criteria"))
    else {
        return Err(task_refusal(
            "task-schema",
            "task.json is missing one of type, instructions, criteria",
        ));
    };
    if !matches!(kind, Lite::Str(t) if t == "choice") {
        return Err(task_refusal("task-type", "type must be \"choice\""));
    }
    if !matches!(instructions, Lite::Str(t) if !blank(t)) {
        return Err(task_refusal(
            "task-schema",
            "instructions must be a non-empty string",
        ));
    }
    let (criteria, total) = match criteria {
        Lite::Obj { pairs, total } => (pairs.as_slice(), *total),
        Lite::EmptyArray => (&[][..], 0),
        _ => {
            return Err(task_refusal(
                "task-schema",
                "criteria must be a JSON object",
            ))
        }
    };
    let mut names: HashSet<&str> = HashSet::new();
    if !criteria.iter().all(|(n, _)| names.insert(n.as_str())) {
        return Err(task_refusal(
            "task-duplicate-criterion",
            "a criterion name appears more than once",
        ));
    }
    if total < 2 {
        return Err(task_refusal(
            "task-too-few-criteria",
            format!("a choice task needs at least 2 criteria, got {total}"),
        ));
    }
    for (index, (name, description)) in criteria.iter().enumerate() {
        if blank(name) {
            return Err(task_refusal(
                "task-schema",
                format!("criterion {index} has an empty name"),
            ));
        }
        if !matches!(description, Lite::Null | Lite::Str(_)) {
            return Err(task_refusal(
                "task-schema",
                format!("criterion {index} description must be a string or null"),
            ));
        }
    }
    if total > MAX_CRITERIA {
        return Err(task_refusal(
            "task-serving-limit",
            format!(
                "task.json has {total} criteria; a servable model has at most {MAX_CRITERIA} \
                 (decide-apr-v1 max_criteria)"
            ),
        ));
    }
    Task::from_slice(bytes).map_err(|_| {
        task_refusal(
            "task-schema",
            "task.json is refused by the reader pack uses (Task::from_slice)",
        )
    })
}

/// `hex4` of `b[..4]`, or `None` unless four hex digits.
fn hex4(b: &[u8]) -> Option<u32> {
    if b.len() < 4 {
        return None;
    }
    let mut value = 0u32;
    for &c in &b[..4] {
        value = value * 16 + char::from(c).to_digit(16)?;
    }
    Some(value)
}

/// Byte offsets of every unpaired surrogate escape (`\uD800`..`\uDFFF` not forming
/// a high+low pair) in `line`. A backslash pair is skipped as a unit, so
/// `\\ud800` (an escaped backslash then text) is not an escape.
fn lone_surrogates(line: &str) -> Vec<usize> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' {
            i += 1;
            continue;
        }
        if b.get(i + 1) == Some(&b'u') {
            if let Some(unit) = b.get(i + 2..).and_then(hex4) {
                let low = |at: usize| -> Option<u32> {
                    (b.get(at) == Some(&b'\\') && b.get(at + 1) == Some(&b'u'))
                        .then(|| b.get(at + 2..).and_then(hex4))
                        .flatten()
                };
                if (0xD800..0xDC00).contains(&unit) {
                    if low(i + 6).is_some_and(|l| (0xDC00..0xE000).contains(&l)) {
                        i += 12;
                    } else {
                        out.push(i);
                        i += 6;
                    }
                    continue;
                }
                if (0xDC00..0xE000).contains(&unit) {
                    out.push(i);
                }
                i += 6;
                continue;
            }
        }
        i += 2;
    }
    out
}

/// A line with each lone-surrogate escape replaced by one marker character, plus
/// that character. serde_json refuses a lone surrogate at PARSE time while
/// `data.py` decodes it and refuses it later as `<role>-row-encoding`, AFTER the
/// key checks; marking it keeps the line parseable so the key checks run first and
/// the encoding refusal lands at `data.py`'s point of the check order. The marker
/// is a Unicode noncharacter that occurs in the line neither literally nor as an
/// escape; `None` marker means the line had no lone surrogate. `Err` only when all
/// 32 candidates occur in the line (a hostile line, refused as an encoding error).
fn mark_lone_surrogates(line: &str) -> Result<(Cow<'_, str>, Option<char>), ()> {
    if !line.contains("\\u") {
        return Ok((Cow::Borrowed(line), None));
    }
    let offsets = lone_surrogates(line);
    if offsets.is_empty() {
        return Ok((Cow::Borrowed(line), None));
    }
    let lower = line.to_ascii_lowercase();
    let marker = ('\u{FDD0}'..='\u{FDEF}')
        .find(|c| !line.contains(*c) && !lower.contains(&format!("\\u{:04x}", u32::from(*c))))
        .ok_or(())?;
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    for offset in offsets {
        out.push_str(&line[at..offset]);
        out.push(marker);
        at = offset + 6;
    }
    out.push_str(&line[at..]);
    Ok((Cow::Owned(out), Some(marker)))
}

/// One parsed row.
struct Row {
    text: String,
    label: usize,
}

/// The lines of a jsonl file as Rust's `str::lines` yields them, which is also what
/// `pack`'s reader (`aprender_decide::verify` `parse_rows`) uses and what
/// `data.py` `jsonl_lines` reproduces (WR-05: only `\n` ends a line, one `\r`
/// before it is stripped, U+0085 / U+2028 / U+2029 inside a text never split it).
fn jsonl_lines(text: &str) -> Vec<&str> {
    text.lines().collect()
}

/// Parse one jsonl file, refusing by `data.py` `load_rows` rule names in its order:
/// the file's UTF-8 first (`<role>-row-encoding`), then per line blank, JSON,
/// object, unknown key, exact keys (a missing OR repeated key is `row-schema`), the
/// lone-surrogate encoding of text then label, a non-empty string `text`, a string
/// `label`, a known label; after the loop `<role>-empty`, and for `eval` only
/// `eval-class-coverage`.
fn parse_rows(
    bytes: &[u8],
    role: &str,
    file: &str,
    labels: &[String],
    policy: &ValidationPolicy,
) -> Result<Vec<Row>, Refusal> {
    let row_rule = |suffix: &str| format!("{role}-row-{suffix}");
    let text = std::str::from_utf8(bytes).map_err(|e| {
        Refusal::new(
            &row_rule("encoding"),
            format!("{file} is not valid UTF-8 (byte {})", e.valid_up_to()),
        )
    })?;
    let lines = jsonl_lines(text);
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
    let mut rows = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let n = index + 1;
        let schema =
            |why: &str| Refusal::new(&row_rule("schema"), format!("{file} line {n} {why}"));
        if blank(line) {
            return Err(schema("is blank"));
        }
        let (parsed_line, marker) = mark_lone_surrogates(line).map_err(|()| {
            Refusal::new(
                &row_rule("encoding"),
                format!("{file} line {n} holds a lone surrogate escape"),
            )
        })?;
        let value: Lite = serde_json::from_str(&parsed_line).map_err(|_| schema("is not JSON"))?;
        let Lite::Obj { mut pairs, total } = value else {
            return Err(schema("is not a JSON object"));
        };
        let mut extra: Vec<String> = pairs
            .iter()
            .map(|(k, _)| k.as_str())
            .filter(|k| *k != "text" && *k != "label")
            .map(short)
            .collect();
        if !extra.is_empty() {
            extra.sort_unstable();
            extra.dedup();
            return Err(Refusal::new(
                &row_rule("unknown-key"),
                format!("{file} line {n} has unknown key(s) {extra:?}"),
            ));
        }
        let text_at = pairs.iter().position(|(k, _)| k == "text");
        let label_at = pairs.iter().position(|(k, _)| k == "label");
        let (Some(text_at), Some(label_at), 2) = (text_at, label_at, total) else {
            return Err(schema("must hold exactly [\"text\", \"label\"]"));
        };
        let (text_value, label_value) = if text_at < label_at {
            let label = pairs.swap_remove(label_at).1;
            (pairs.swap_remove(text_at).1, label)
        } else {
            let text = pairs.swap_remove(text_at).1;
            (text, pairs.swap_remove(label_at).1)
        };
        let marked = |v: &Lite| matches!((v, marker), (Lite::Str(s), Some(m)) if s.contains(m));
        for (field, value) in [("text", &text_value), ("label", &label_value)] {
            if marked(value) {
                return Err(Refusal::new(
                    &row_rule("encoding"),
                    format!(
                        "{file} line {n} {field} holds a lone surrogate; it is not UTF-8 and \
                         the row is refused"
                    ),
                ));
            }
        }
        let Lite::Str(row_text) = text_value else {
            return Err(schema("text must be a non-empty string"));
        };
        if blank(&row_text) {
            return Err(schema("text must be a non-empty string"));
        }
        let Lite::Str(label) = label_value else {
            return Err(Refusal::new(
                &row_rule("label"),
                format!("{file} line {n} label must be a criterion name (a string)"),
            ));
        };
        let Some(position) = labels.iter().position(|l| *l == label) else {
            return Err(Refusal::new(
                &row_rule("label"),
                format!("{file} line {n} label is not a criterion of task.json"),
            ));
        };
        rows.push(Row {
            text: row_text,
            label: position,
        });
    }
    if rows.is_empty() {
        return Err(Refusal::new(
            &format!("{role}-empty"),
            format!("{file} has no rows"),
        ));
    }
    if role == "eval" {
        let mut present = vec![false; labels.len()];
        for row in &rows {
            present[row.label] = true;
        }
        let missing = present.iter().filter(|p| !**p).count();
        if missing > 0 {
            return Err(Refusal::new(
                "eval-class-coverage",
                format!(
                    "{file} holds no row of {missing} criterion/criteria: every criterion needs \
                     at least one eval row for the gate's macro-F1 to compare the same labels"
                ),
            ));
        }
    }
    Ok(rows)
}

/// `refuse_overlap`: an `role` row (eval, or the shift probe) whose normalized text
/// hash is a train text's hash is refused `<role>-train-overlap`. Hashing is
/// `aprender_decide::verify::normalized_sha256` (nfc-trim-ws-v1) only.
fn refuse_overlap(train_hashes: &HashSet<String>, rows: &[Row], role: &str) -> Result<(), Refusal> {
    let hits: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| train_hashes.contains(&normalized_sha256(&r.text)))
        .map(|(i, _)| i)
        .collect();
    if hits.is_empty() {
        return Ok(());
    }
    Err(Refusal::new(
        &format!("{role}-train-overlap"),
        format!(
            "{} {role} row(s) equal a train text after NFC/trim/whitespace collapse ({role}.jsonl \
             rows {})",
            hits.len(),
            first_ten(&hits)
        ),
    ))
}

/// The calibration-split rules `data.py` `calibration_split` applies, as far as
/// they can be decided without the seeded permutation. Returns whether
/// `train-class-too-small` is DEFERRED to the trainer.
///
/// `group_train` first: one normalized text under two labels is refused
/// `train-conflicting-labels` (groups in first-seen order, as the trainer reports).
/// Then per class `need = max(min_per_class, ceil(fraction * count))`: a class with
/// `count <= need` can never leave a fit row, whatever the draw, so it is refused
/// `train-class-too-small`. A class with `count > need` is refused by the trainer
/// only when whole text groups of two or more rows overshoot the slice, which
/// depends on the permutation; that case is accepted here and reported as deferred
/// when the class has such a group.
fn check_calibration(
    train: &[Row],
    class_count: usize,
    policy: &ValidationPolicy,
) -> Result<bool, Refusal> {
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, row) in train.iter().enumerate() {
        let group = *index
            .entry(normalized_sha256(&row.text))
            .or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
        groups[group].push(i);
    }
    if let Some(group) = groups
        .iter()
        .find(|g| g.iter().any(|&i| train[i].label != train[g[0]].label))
    {
        return Err(Refusal::new(
            "train-conflicting-labels",
            format!(
                "train.jsonl rows {} carry the same normalized text under different labels",
                first_ten(group)
            ),
        ));
    }
    let mut counts = vec![0u64; class_count];
    let mut has_multi_row_group = vec![false; class_count];
    for group in &groups {
        let label = train[group[0]].label;
        counts[label] += group.len() as u64;
        has_multi_row_group[label] |= group.len() > 1;
    }
    let mut deferred = false;
    for (class, &count) in counts.iter().enumerate() {
        // `ceil` of an f64 product, exactly as `math.ceil(float(fraction) * n)`.
        let slice = (policy.calibration_fraction * count as f64).ceil() as u64;
        let need = policy.calibration_min_per_class.max(slice);
        if count <= need {
            return Err(Refusal::new(
                "train-class-too-small",
                format!(
                    "class {class} has {count} train row(s): a calibration slice of >= {need} \
                     row(s) must leave at least one fit row"
                ),
            ));
        }
        deferred |= has_multi_row_group[class];
    }
    Ok(deferred)
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
    let task = check_task(task_bytes)?;
    let labels = task.owned_labels();
    let train_bytes = files.train.as_deref().ok_or_else(|| {
        Refusal::new(
            "train-missing",
            "train.jsonl does not exist (it is required)",
        )
    })?;
    let train = parse_rows(train_bytes, "train", "train.jsonl", &labels, policy)?;
    let eval_bytes = files.eval.as_deref().ok_or_else(|| {
        Refusal::new("eval-missing", "eval.jsonl does not exist (it is required)")
    })?;
    let eval = parse_rows(eval_bytes, "eval", "eval.jsonl", &labels, policy)?;
    let train_hashes: HashSet<String> = train.iter().map(|r| normalized_sha256(&r.text)).collect();
    refuse_overlap(&train_hashes, &eval, "eval")?;
    let shift = match files.shift.as_deref() {
        Some(bytes) => {
            let rows = parse_rows(bytes, "shift", "shift.jsonl", &labels, policy)?;
            refuse_overlap(&train_hashes, &rows, "shift")?;
            Some(rows)
        }
        None => None,
    };
    let deferred = check_calibration(&train, labels.len(), policy)?;

    let mut counts = vec![0u64; labels.len()];
    for row in &train {
        counts[row.label] += 1;
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
        deferred_rules: if deferred {
            DEFERRABLE_RULES.iter().map(ToString::to_string).collect()
        } else {
            Vec::new()
        },
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

    // ---- the shared case table (D-08) ----

    const CASE_TABLE: &str = include_str!("../../../scripts/laya_train/data_cases.json");

    fn table() -> serde_json::Value {
        serde_json::from_str(CASE_TABLE).expect("data_cases.json parses")
    }

    /// Standard-alphabet base64 with padding (the generator's `base64.b64encode`).
    fn b64_decode(text: &str) -> Vec<u8> {
        let digit = |c: u8| -> u32 {
            match c {
                b'A'..=b'Z' => u32::from(c - b'A'),
                b'a'..=b'z' => u32::from(c - b'a') + 26,
                b'0'..=b'9' => u32::from(c - b'0') + 52,
                b'+' => 62,
                b'/' => 63,
                _ => panic!("not base64: {c}"),
            }
        };
        let raw: Vec<u8> = text.bytes().filter(|c| *c != b'=').collect();
        let mut out = Vec::new();
        for chunk in raw.chunks(4) {
            let mut acc = 0u32;
            for (i, c) in chunk.iter().enumerate() {
                acc |= digit(*c) << (18 - 6 * i);
            }
            for i in 0..chunk.len() - 1 {
                out.push(u8::try_from((acc >> (16 - 8 * i)) & 0xFF).expect("byte"));
            }
        }
        out
    }

    fn case_files(case: &serde_json::Value) -> DatasetFiles {
        let files = case["files"].as_object().expect("files");
        let get = |name: &str| {
            files.get(name).map(|spec| match spec.get("text") {
                Some(t) => t.as_str().expect("text").as_bytes().to_vec(),
                None => b64_decode(spec["b64"].as_str().expect("b64")),
            })
        };
        DatasetFiles {
            task_json: get("task.json"),
            train: get("train.jsonl"),
            eval: get("eval.jsonl"),
            shift: get("shift.jsonl"),
        }
    }

    fn case_epochs(case: &serde_json::Value) -> Option<u32> {
        case["epochs"]
            .as_u64()
            .map(|e| u32::try_from(e).expect("epochs"))
    }

    /// An accepted result in the shape the table writes `python.ok` in.
    fn summary_value(
        summary: &DatasetSummary,
        resolved: Option<u32>,
        policy: &ValidationPolicy,
    ) -> serde_json::Value {
        let counts: Vec<u64> = summary
            .labels
            .iter()
            .map(|l| summary.class_counts[l])
            .collect();
        #[derive(Serialize)]
        struct Shape<'a> {
            labels: &'a [String],
            class_counts: Vec<u64>,
            shots_per_class: u64,
            epochs: u32,
            shift_present: bool,
            inputs_sha256: &'a BTreeMap<String, String>,
        }
        serde_json::to_value(Shape {
            labels: &summary.labels,
            class_counts: counts,
            shots_per_class: summary.shots_per_class,
            epochs: resolved.unwrap_or(policy.epoch_rule.fixed),
            shift_present: summary.shift_present,
            inputs_sha256: &summary.inputs_sha256,
        })
        .expect("a summary serializes")
    }

    #[test]
    fn shared_case_table() {
        let p = policy();
        let t = table();
        assert_eq!(t["schema"], "laya-data-cases-v1");
        let stricter: Vec<&str> = t["stricter_rules"]
            .as_array()
            .expect("stricter_rules")
            .iter()
            .map(|r| r.as_str().expect("rule"))
            .collect();
        let cases = t["cases"].as_array().expect("cases");
        assert!(cases.len() >= 12, "{} cases", cases.len());
        let mut deferred = 0;
        for case in cases {
            let id = case["id"].as_str().expect("id");
            let py = &case["python"];
            let got = validate_for_submit(&case_files(case), case_epochs(case), &p);
            let rust = &case["rust"];
            if rust == "same" {
                match (py.get("refused"), py.get("ok"), &got) {
                    (Some(rule), None, Err(r)) => {
                        assert_eq!(rule.as_str(), Some(r.rule.as_str()), "{id}: {r}");
                    }
                    (None, Some(ok), Ok((summary, resolved))) => {
                        assert_eq!(&summary_value(summary, *resolved, &p), ok, "{id}");
                        assert!(summary.deferred_rules.is_empty(), "{id}");
                    }
                    (refused, accepted, got) => {
                        panic!("{id}: python {refused:?} / {accepted:?}, rust {got:?}")
                    }
                }
            } else if rust == "deferred" {
                let rule = py["refused"].as_str().expect("python refused");
                assert!(DEFERRABLE_RULES.contains(&rule), "{id}: {rule}");
                let (summary, _) = got.unwrap_or_else(|r| panic!("{id}: rust refused: {r}"));
                assert!(
                    summary.deferred_rules.iter().any(|d| d == rule),
                    "{id}: {:?}",
                    summary.deferred_rules
                );
                deferred += 1;
            } else {
                let rule = rust["stricter"].as_str().expect("stricter rule");
                assert!(stricter.contains(&rule), "{id}: {rule}");
                assert!(py.get("ok").is_some(), "{id}: python must accept");
                let err = got.expect_err(id);
                assert_eq!(err.rule, rule, "{id}: {err}");
            }
        }
        println!(
            "shared_case_table: {} cases, {deferred} deferred",
            cases.len()
        );
    }

    #[test]
    fn deferred_rules_reach_the_serialized_answer() {
        let p = policy();
        let t = table();
        let case = t["cases"]
            .as_array()
            .expect("cases")
            .iter()
            .find(|c| c["rust"] == "deferred")
            .expect("the table holds a deferred case");
        let (summary, _) = validate_for_submit(&case_files(case), case_epochs(case), &p)
            .expect("rust defers, it does not refuse");
        let answer = serde_json::to_value(&summary).expect("a summary serializes");
        assert_eq!(
            answer["deferred_rules"],
            serde_json::Value::from(vec!["train-class-too-small"]),
            "validate_dataset serializes the summary as is, so the caller sees the deferral"
        );
    }

    #[test]
    fn contract_mirror() {
        // Read through a second, untyped path so the typed view cannot agree with itself.
        let doc: serde_yaml::Value =
            serde_yaml::from_str(GATE_CONTRACT_YAML).expect("the gate contract parses");
        let p = policy();
        let constants = &doc["constants"];
        assert_eq!(
            Some(p.calibration_fraction),
            constants["calibration_slice_fraction"].as_f64()
        );
        assert_eq!(
            Some(p.calibration_min_per_class),
            constants["calibration_slice_min_per_class"].as_u64()
        );
        let recipe = &doc["recipe"];
        assert_eq!(
            Some(u64::from(p.epoch_rule.fixed)),
            recipe["epochs_at_most_16_per_class"].as_u64()
        );
        assert_eq!(
            Some(u64::from(p.epoch_rule.lo)),
            recipe["epochs_above_16_min"].as_u64()
        );
        assert_eq!(
            Some(u64::from(p.epoch_rule.hi)),
            recipe["epochs_above_16_max"].as_u64()
        );
        // The numbers are real, not zero defaults.
        assert!(p.calibration_fraction > 0.0 && p.calibration_fraction < 1.0);
        assert!(p.calibration_min_per_class >= 1);
    }

    #[test]
    fn deferrable_rules_mirror() {
        let t = table();
        let names = |key: &str| -> Vec<String> {
            t[key]
                .as_array()
                .expect(key)
                .iter()
                .map(|r| r.as_str().expect("rule").to_string())
                .collect()
        };
        assert_eq!(names("deferrable_rules"), DEFERRABLE_RULES);
        assert_eq!(names("stricter_rules"), STRICTER_RULES);
        // Every deferred case defers a rule the header declares, and nothing else is deferred.
        for case in t["cases"].as_array().expect("cases") {
            if case["rust"] == "deferred" {
                let rule = case["python"]["refused"].as_str().expect("refused");
                assert!(DEFERRABLE_RULES.contains(&rule), "{}", case["id"]);
            }
        }
    }

    #[test]
    fn no_false_refusal() {
        let p = policy();
        let t = table();
        let mut accepted_by_python = 0;
        for case in t["cases"].as_array().expect("cases") {
            let id = case["id"].as_str().expect("id");
            if case["python"].get("ok").is_none() {
                continue;
            }
            accepted_by_python += 1;
            match validate_for_submit(&case_files(case), case_epochs(case), &p) {
                Ok(_) => {}
                Err(r) => {
                    assert!(
                        STRICTER_RULES.contains(&r.rule.as_str()),
                        "{id}: the trainer accepts, rust refused {r}"
                    );
                    assert_eq!(
                        case["rust"]["stricter"].as_str(),
                        Some(r.rule.as_str()),
                        "{id}: a stricter refusal must be declared on the case"
                    );
                }
            }
        }
        assert!(
            accepted_by_python >= 8,
            "{accepted_by_python} accepted cases"
        );
        // The boundary is in the table: 510 criteria accepted, 511 declared stricter.
        let find = |id: &str| {
            t["cases"]
                .as_array()
                .expect("cases")
                .iter()
                .find(|c| c["id"] == id)
                .unwrap_or_else(|| panic!("{id} is not in the table"))
        };
        assert_eq!(find("criteria-510")["rust"], "same");
        assert_eq!(
            find("criteria-511")["rust"]["stricter"],
            "task-serving-limit"
        );
    }

    #[test]
    fn inspect_never_refuses_epochs() {
        let p = policy();
        let t = table();
        let mut epoch_cases = 0;
        for case in t["cases"].as_array().expect("cases") {
            let id = case["id"].as_str().expect("id");
            let inspected = validate_files(&case_files(case), &p);
            if let Err(r) = &inspected {
                assert_ne!(r.rule, "epochs", "{id}: inspection refused epochs");
            }
            if case["python"]["refused"] == "epochs" {
                epoch_cases += 1;
                let s = inspected.unwrap_or_else(|r| panic!("{id}: inspection refused {r}"));
                assert_eq!(
                    s.epochs_required,
                    s.shots_per_class > args::EPOCH_THRESHOLD,
                    "{id}"
                );
                if s.epochs_required {
                    assert_eq!(
                        s.epochs_range,
                        Some([p.epoch_rule.lo, p.epoch_rule.hi]),
                        "{id}"
                    );
                    assert_eq!(s.epochs_fixed, None, "{id}");
                } else {
                    assert_eq!(s.epochs_range, None, "{id}");
                    assert_eq!(s.epochs_fixed, Some(p.epoch_rule.fixed), "{id}");
                }
            }
        }
        assert!(epoch_cases >= 4, "{epoch_cases} epoch cases in the table");
    }

    #[test]
    fn refusal_details_never_echo_row_text() {
        let p = policy();
        let t = table();
        let mut secret_cases = 0;
        for case in t["cases"].as_array().expect("cases") {
            let id = case["id"].as_str().expect("id");
            if let Err(r) = validate_for_submit(&case_files(case), case_epochs(case), &p) {
                let shown = r.to_string();
                assert!(!shown.contains("SECRET"), "{id}: {shown}");
                if id.starts_with("secret-") {
                    secret_cases += 1;
                }
            }
        }
        assert_eq!(
            secret_cases, 4,
            "the four SECRET cases refuse and are checked"
        );
    }

    /// `validate_files` on `MAX_ROWS_PER_FILE` train rows: the request path's cost
    /// is bounded well inside the API Gateway's 30 s cap (the S3 fetch shares it).
    #[test]
    fn validate_bound_timing() {
        let p = policy();
        let rows = MAX_ROWS_PER_FILE;
        let mut train = String::with_capacity(rows * 48);
        for i in 0..rows {
            let label = ["a", "b", "c"][i % 3];
            train.push_str(&format!(
                "{{\"text\": \"synthetic train row number {i} for class {label}\", \"label\": \"{label}\"}}\n"
            ));
        }
        let mut eval = String::new();
        for i in 0..2_000 {
            let label = ["a", "b", "c"][i % 3];
            eval.push_str(&format!(
                "{{\"text\": \"synthetic eval row {i}\", \"label\": \"{label}\"}}\n"
            ));
        }
        let files = DatasetFiles {
            task_json: Some(
                br#"{"type":"choice","instructions":"Which?","criteria":{"a":null,"b":null,"c":null}}"#
                    .to_vec(),
            ),
            train: Some(train.into_bytes()),
            eval: Some(eval.into_bytes()),
            shift: None,
        };
        let started = std::time::Instant::now();
        let summary = validate_files(&files, &p).expect("the synthetic dataset is valid");
        let seconds = started.elapsed().as_secs_f64();
        println!("validate_bound_timing: {rows} train rows in {seconds:.3} s");
        assert_eq!(summary.rows.train, rows as u64);
        assert!(seconds < 20.0, "{rows} rows took {seconds:.1} s");
    }
}
