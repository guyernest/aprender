//! Adapt a Laya `{text, label}` data directory into an attested canonical SetFit directory.
//!
//! Phase 10 plan 10-05 (D-11). The Laya gate trains on `train.jsonl` and is judged on
//! `eval.jsonl`; the one SetFit comparison cell must be drawn from the same train split and
//! judged on the same eval split. The TweetEval bench row cannot carry it (three-class
//! schema, 40-cell matrix), so this adapter reshapes the Laya rows into the crate's own
//! canonical three-split profile and lets `apr data select` / `apr setfit train` /
//! `apr eval` take it from there.
//!
//! The adapter maps rows; the CRATE computes every hash. It never hashes a text itself:
//! `PreparedDataset::<Canonical>::from_labeled_rows` runs the ingest ladder,
//! `encode_jsonl` produces the bytes that are written, and
//! `DatasetAttestation::from_prepared` produces the attestation the manifest carries.
//!
//! * Label order is `task.json`'s `criteria` in DOCUMENT order (the decide-apr-v1 label
//!   index), never lexical order: `not_antisemitic` is label 0 and `antisemitic` label 1
//!   although their lexical order is the reverse. The map is therefore read through an
//!   ordered visitor and never through `serde_json::Map` or `Value`.
//! * The validation slice is carved from TRAIN only (stratified, seeded Philox, forward
//!   Fisher-Yates). The test split is the Laya eval rows, in file order, untouched.
//! * Every row's `source_split` is its role string (`train`, `validation`, `test`), because
//!   the ingest ladder refuses any other value with `SplitRoleMismatch`.
//!
//! Output is counts and fingerprints only. No text is ever printed.
//!
//! ```text
//! cargo run --release -p aprender-contrastive-data --example laya_dir_to_canonical -- \
//!     --laya-dir data/decide/goldstandard-2024 --output data/decide/goldstandard-2024-setfit
//! ```
//!
//! Exit 0 and one `ADAPTED ...` line on success; exit 2 and one `REFUSED ...` line when the
//! crate refuses the rows (a result, never something to bypass by dropping rows); exit 1 and
//! one `ERROR ...` line for usage, IO and malformed-input failures.

use std::fmt;
use std::fs;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use aprender_contrastive_data::attestation::{
    DatasetAttestation, DATASET_ATTESTATION_SCHEMA_VERSION,
};
use aprender_contrastive_data::error::ContrastiveDataError;
use aprender_contrastive_data::ledger::AccessLedger;
use aprender_contrastive_data::prepared::{
    Canonical, CanonicalDeclarations, PreparedDataset, PreparedJsonl,
};
use aprender_contrastive_data::rng::{bounded, derive_key};
use aprender_contrastive_data::schema::LabeledExample;
use aprender_contrastive_data::split::SplitDeclaration;
use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The Philox domain of the validation carve. A fixed string, so the carve is a pure
/// function of `(seed, rows)`.
const CARVE_DOMAIN: &str = "laya-goldstandard-validation-carve";
const DEFAULT_LAYA_DIR: &str = "data/decide/goldstandard-2024";
const DEFAULT_OUTPUT_DIR: &str = "data/decide/goldstandard-2024-setfit";
const MANIFEST_FILE: &str = "benchmark-manifest.json";
const OUTPUT_FILES: [&str; 4] = [
    "train.jsonl",
    "validation.jsonl",
    "test.jsonl",
    MANIFEST_FILE,
];

// ---------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------

#[derive(Debug)]
enum AdaptError {
    /// A bad flag or value. Exit 1.
    Usage(String),
    /// A file could not be read or written. Exit 1.
    Io(String),
    /// The Laya files are not what this adapter reads. Exit 1.
    Data(String),
    /// The crate refused the rows. A result, not a failure of the adapter. Exit 2.
    Refused(ContrastiveDataError),
}

impl fmt::Display for AdaptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "usage: {message}"),
            Self::Io(message) => write!(f, "io: {message}"),
            Self::Data(message) => write!(f, "data: {message}"),
            Self::Refused(error) => write!(f, "{error} ({error:?})"),
        }
    }
}

impl From<ContrastiveDataError> for AdaptError {
    fn from(error: ContrastiveDataError) -> Self {
        Self::Refused(error)
    }
}

// ---------------------------------------------------------------------------------------
// task.json and the Laya rows
// ---------------------------------------------------------------------------------------

/// Criteria names in DOCUMENT order.
///
/// Mirrors `OrderedCriteria` in `crates/aprender-decide/src/task.rs`: a map visitor that
/// pushes entries into a `Vec` as they appear. Collecting into `serde_json::Map` or `Value`
/// would iterate key-sorted unless a `preserve_order` feature happened to be on, and sorted,
/// `antisemitic` would become label 0.
struct OrderedCriteria(Vec<String>);

impl<'de> Deserialize<'de> for OrderedCriteria {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedVisitor;

        impl<'de> Visitor<'de> for OrderedVisitor {
            type Value = OrderedCriteria;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a criteria object mapping names to descriptions")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut names = Vec::new();
                while let Some((name, _description)) = map.next_entry::<String, IgnoredAny>()? {
                    names.push(name);
                }
                Ok(OrderedCriteria(names))
            }
        }

        deserializer.deserialize_map(OrderedVisitor)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskDoc {
    #[serde(rename = "type")]
    kind: String,
    // Read by the decide parser, not by this adapter; named so `deny_unknown_fields` accepts
    // the file exactly as `aprender-decide` does.
    #[allow(dead_code)]
    instructions: String,
    criteria: OrderedCriteria,
}

/// The label map of a `task.json`, in document order.
fn label_names_from_task(bytes: &[u8]) -> Result<Vec<String>, AdaptError> {
    let doc: TaskDoc = serde_json::from_slice(bytes)
        .map_err(|error| AdaptError::Data(format!("task.json does not parse: {error}")))?;
    if doc.kind != "choice" {
        return Err(AdaptError::Data(format!(
            "task.json type is {:?}, expected \"choice\"",
            doc.kind
        )));
    }
    let names = doc.criteria.0;
    if names.len() != 2 {
        return Err(AdaptError::Data(format!(
            "task.json has {} criteria; this adapter writes the two-label cell only",
            names.len()
        )));
    }
    for (index, name) in names.iter().enumerate() {
        if name.is_empty() {
            return Err(AdaptError::Data(format!(
                "criterion {index} has an empty name"
            )));
        }
        if names.iter().take(index).any(|earlier| earlier == name) {
            return Err(AdaptError::Data(format!(
                "criterion {name:?} appears more than once"
            )));
        }
    }
    Ok(names)
}

/// One Laya line. Any other key is an error.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayaRow {
    text: String,
    label: String,
}

/// `(text, label index)` per line, in file order.
fn parse_laya_rows(
    bytes: &[u8],
    file: &str,
    label_names: &[String],
) -> Result<Vec<(String, usize)>, AdaptError> {
    let body = std::str::from_utf8(bytes)
        .map_err(|_| AdaptError::Data(format!("{file} is not valid UTF-8")))?;
    let mut rows = Vec::new();
    for (index, line) in body.lines().enumerate() {
        let row: LayaRow = serde_json::from_str(line).map_err(|error| {
            AdaptError::Data(format!(
                "{file} line {} is not a {{text, label}} row: {error}",
                index + 1
            ))
        })?;
        let label = label_names
            .iter()
            .position(|name| *name == row.label)
            .ok_or_else(|| {
                AdaptError::Data(format!(
                    "{file} line {} carries a label outside the criteria",
                    index + 1
                ))
            })?;
        rows.push((row.text, label));
    }
    if rows.is_empty() {
        return Err(AdaptError::Data(format!("{file} has no rows")));
    }
    Ok(rows)
}

// ---------------------------------------------------------------------------------------
// The validation carve
// ---------------------------------------------------------------------------------------

/// Per-class record of the carve, for the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct CarveClass {
    label: String,
    class_rows: usize,
    validation_rows: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Carve {
    /// Row indices (into the Laya train file) that stay in train, ascending.
    train: Vec<usize>,
    /// Row indices that become validation, ascending.
    validation: Vec<usize>,
    per_class: Vec<CarveClass>,
}

/// `ceil(fraction * n)`, tolerant of float noise like `0.1 * 30 = 3.0000000000000004`.
fn ceil_share(fraction: f64, n: usize) -> usize {
    ((fraction * n as f64) - 1e-9).ceil().max(0.0) as usize
}

/// Stratified, seeded carve of a validation slice out of the TRAIN rows.
///
/// Per class a forward Fisher-Yates permutation (the same shape as `select::select`), each
/// swap drawn with `bounded(key, class, i, n - i)` from
/// `derive_key(seed, CARVE_DOMAIN)`; the first `ceil(fraction * n_class)` rows of the
/// permuted class become validation. Ascending index order is restored afterwards so the
/// output files keep the Laya file order.
fn carve_validation(
    labels: &[usize],
    label_names: &[String],
    fraction: f64,
    seed: u64,
) -> Result<Carve, AdaptError> {
    let key = derive_key(seed, CARVE_DOMAIN);
    let mut train = Vec::new();
    let mut validation = Vec::new();
    let mut per_class = Vec::new();
    for (class, name) in label_names.iter().enumerate() {
        let mut pool: Vec<usize> = labels
            .iter()
            .enumerate()
            .filter(|(_, label)| **label == class)
            .map(|(index, _)| index)
            .collect();
        let class_rows = pool.len();
        let take = ceil_share(fraction, class_rows).min(class_rows);
        for i in 0..class_rows {
            let Some(remaining) = NonZeroU64::new((class_rows - i) as u64) else {
                break;
            };
            let j = i + bounded(&key, class as u32, i as u64, remaining) as usize;
            pool.swap(i, j);
        }
        validation.extend_from_slice(&pool[..take]);
        train.extend_from_slice(&pool[take..]);
        per_class.push(CarveClass {
            label: name.clone(),
            class_rows,
            validation_rows: take,
        });
    }
    train.sort_unstable();
    validation.sort_unstable();
    Ok(Carve {
        train,
        validation,
        per_class,
    })
}

// ---------------------------------------------------------------------------------------
// Assembly through the crate's public doors
// ---------------------------------------------------------------------------------------

/// Rows of one role. `source_split` is the role string itself (ingest Gate 2).
fn labeled_rows(
    role: &str,
    items: &[(&str, usize)],
    label_names: &[String],
) -> Vec<LabeledExample> {
    items
        .iter()
        .enumerate()
        .map(|(index, (text, label))| LabeledExample {
            id: format!("{role}:{index}"),
            input: (*text).to_string(),
            label: *label,
            label_text: label_names.get(*label).cloned().unwrap_or_default(),
            source_split: role.to_string(),
        })
        .collect()
}

/// The declaration is the assertion: counts computed from the rows the adapter is about to
/// hand over, which the ladder then re-counts independently.
fn declaration(rows: &[LabeledExample], label_names: &[String]) -> SplitDeclaration {
    let mut expected_class_counts = vec![0usize; label_names.len()];
    for row in rows {
        if let Some(slot) = expected_class_counts.get_mut(row.label) {
            *slot += 1;
        }
    }
    SplitDeclaration {
        expected_class_counts,
        label_names: label_names.to_vec(),
    }
}

fn assemble(
    train: Vec<LabeledExample>,
    validation: Vec<LabeledExample>,
    test: Vec<LabeledExample>,
    label_names: &[String],
) -> Result<PreparedDataset<Canonical>, ContrastiveDataError> {
    let decls = CanonicalDeclarations {
        train: declaration(&train, label_names),
        validation: declaration(&validation, label_names),
        test: declaration(&test, label_names),
        label_names: label_names.to_vec(),
    };
    PreparedDataset::<Canonical>::from_labeled_rows(
        train,
        validation,
        test,
        &decls,
        &mut AccessLedger::new(),
    )
}

// ---------------------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------------------

#[derive(Serialize)]
struct CarveManifest {
    fraction: f64,
    seed: u64,
    domain: &'static str,
    per_class: Vec<CarveClass>,
}

#[derive(Serialize)]
struct SourceManifest {
    laya_data_dir: String,
    task_json_sha256: String,
    train_jsonl_sha256: String,
    eval_jsonl_sha256: String,
    validation_carve: CarveManifest,
}

#[derive(Serialize)]
struct Manifest {
    schema_version: u32,
    dataset: &'static str,
    target: &'static str,
    task: &'static str,
    profile: &'static str,
    labels: Vec<String>,
    source: SourceManifest,
    dataset_attestation: DatasetAttestation,
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

// ---------------------------------------------------------------------------------------
// The pure adapter: bytes in, bytes out
// ---------------------------------------------------------------------------------------

struct Inputs<'a> {
    /// The directory as the user gave it (recorded verbatim in the manifest).
    laya_dir_label: String,
    task: &'a [u8],
    train: &'a [u8],
    eval: &'a [u8],
    fraction: f64,
    seed: u64,
}

struct Adapted {
    jsonl: PreparedJsonl,
    manifest_bytes: Vec<u8>,
    label_names: Vec<String>,
    train_counts: Vec<u64>,
    validation_counts: Vec<u64>,
    test_counts: Vec<u64>,
    dataset_fingerprint: String,
}

fn adapt(inputs: &Inputs<'_>) -> Result<Adapted, AdaptError> {
    let label_names = label_names_from_task(inputs.task)?;
    let laya_train = parse_laya_rows(inputs.train, "train.jsonl", &label_names)?;
    let laya_eval = parse_laya_rows(inputs.eval, "eval.jsonl", &label_names)?;

    let labels: Vec<usize> = laya_train.iter().map(|(_, label)| *label).collect();
    let carve = carve_validation(&labels, &label_names, inputs.fraction, inputs.seed)?;

    let pick = |indices: &[usize]| -> Vec<(&str, usize)> {
        indices
            .iter()
            .map(|index| (laya_train[*index].0.as_str(), laya_train[*index].1))
            .collect()
    };
    let train_items = pick(&carve.train);
    let validation_items = pick(&carve.validation);
    let test_items: Vec<(&str, usize)> = laya_eval
        .iter()
        .map(|(text, label)| (text.as_str(), *label))
        .collect();

    let dataset = assemble(
        labeled_rows("train", &train_items, &label_names),
        labeled_rows("validation", &validation_items, &label_names),
        labeled_rows("test", &test_items, &label_names),
        &label_names,
    )?;
    let jsonl = dataset.encode_jsonl()?;
    let attestation = DatasetAttestation::from_prepared(&dataset);
    let dataset_fingerprint = attestation.dataset_fingerprint.clone();

    let manifest = Manifest {
        schema_version: DATASET_ATTESTATION_SCHEMA_VERSION,
        dataset: "goldstandard-2024",
        target: "antisemitism",
        task: "single-label two-class antisemitism classification",
        profile: "canonical",
        labels: label_names.clone(),
        source: SourceManifest {
            laya_data_dir: inputs.laya_dir_label.clone(),
            task_json_sha256: sha256_hex(inputs.task),
            train_jsonl_sha256: sha256_hex(inputs.train),
            eval_jsonl_sha256: sha256_hex(inputs.eval),
            validation_carve: CarveManifest {
                fraction: inputs.fraction,
                seed: inputs.seed,
                domain: CARVE_DOMAIN,
                per_class: carve.per_class,
            },
        },
        dataset_attestation: attestation,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| AdaptError::Data(format!("manifest does not serialize: {error}")))?;

    Ok(Adapted {
        jsonl,
        manifest_bytes,
        label_names,
        train_counts: dataset.train().class_counts().to_vec(),
        validation_counts: dataset.validation().class_counts().to_vec(),
        test_counts: dataset.test().class_counts().to_vec(),
        dataset_fingerprint,
    })
}

// ---------------------------------------------------------------------------------------
// IO
// ---------------------------------------------------------------------------------------

fn read_file(path: &Path) -> Result<Vec<u8>, AdaptError> {
    fs::read(path)
        .map_err(|error| AdaptError::Io(format!("cannot read {}: {error}", path.display())))
}

/// Write the four outputs, refusing to replace any of them without `force`.
fn write_outputs(output: &Path, adapted: &Adapted, force: bool) -> Result<(), AdaptError> {
    if !force {
        if let Some(existing) = OUTPUT_FILES
            .iter()
            .map(|name| output.join(name))
            .find(|path| path.exists())
        {
            return Err(AdaptError::Io(format!(
                "refusing to replace existing {} (pass --force to replace known outputs)",
                existing.display()
            )));
        }
    }
    fs::create_dir_all(output)
        .map_err(|error| AdaptError::Io(format!("cannot create {}: {error}", output.display())))?;
    for role in ["train", "validation", "test"] {
        let bytes = adapted
            .jsonl
            .get(role)
            .ok_or_else(|| AdaptError::Data(format!("the crate emitted no {role} bytes")))?;
        let path = output.join(format!("{role}.jsonl"));
        fs::write(&path, bytes)
            .map_err(|error| AdaptError::Io(format!("cannot write {}: {error}", path.display())))?;
    }
    let path = output.join(MANIFEST_FILE);
    fs::write(&path, &adapted.manifest_bytes)
        .map_err(|error| AdaptError::Io(format!("cannot write {}: {error}", path.display())))?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------------------

struct Args {
    laya_dir: PathBuf,
    output: PathBuf,
    fraction: f64,
    seed: u64,
    force: bool,
}

fn parse_args(raw: &[String]) -> Result<Args, AdaptError> {
    let mut args = Args {
        laya_dir: PathBuf::from(DEFAULT_LAYA_DIR),
        output: PathBuf::from(DEFAULT_OUTPUT_DIR),
        fraction: 0.1,
        seed: 13,
        force: false,
    };
    let mut iter = raw.iter();
    while let Some(flag) = iter.next() {
        let mut value = |name: &str| {
            iter.next()
                .cloned()
                .ok_or_else(|| AdaptError::Usage(format!("{name} needs a value")))
        };
        match flag.as_str() {
            "--laya-dir" => args.laya_dir = PathBuf::from(value("--laya-dir")?),
            "--output" => args.output = PathBuf::from(value("--output")?),
            "--validation-fraction" => {
                args.fraction = value("--validation-fraction")?.parse().map_err(|_| {
                    AdaptError::Usage("--validation-fraction is not a number".to_string())
                })?;
            }
            "--seed" => {
                args.seed = value("--seed")?.parse().map_err(|_| {
                    AdaptError::Usage("--seed is not an unsigned integer".to_string())
                })?;
            }
            "--force" => args.force = true,
            other => return Err(AdaptError::Usage(format!("unknown argument {other:?}"))),
        }
    }
    if !(args.fraction > 0.0 && args.fraction < 1.0) {
        return Err(AdaptError::Usage(
            "--validation-fraction must be strictly between 0 and 1".to_string(),
        ));
    }
    Ok(args)
}

fn counts_text(counts: &[u64]) -> String {
    let parts: Vec<String> = counts.iter().map(u64::to_string).collect();
    format!("[{}]", parts.join(","))
}

fn run(raw: &[String]) -> Result<String, AdaptError> {
    let args = parse_args(raw)?;
    let task = read_file(&args.laya_dir.join("task.json"))?;
    let train = read_file(&args.laya_dir.join("train.jsonl"))?;
    let eval = read_file(&args.laya_dir.join("eval.jsonl"))?;
    let adapted = adapt(&Inputs {
        laya_dir_label: args.laya_dir.display().to_string(),
        task: &task,
        train: &train,
        eval: &eval,
        fraction: args.fraction,
        seed: args.seed,
    })?;
    write_outputs(&args.output, &adapted, args.force)?;
    Ok(format!(
        "ADAPTED {} labels={} train={} validation={} test={} dataset_fingerprint={}",
        args.output.display(),
        adapted.label_names.join(","),
        counts_text(&adapted.train_counts),
        counts_text(&adapted.validation_counts),
        counts_text(&adapted.test_counts),
        adapted.dataset_fingerprint,
    ))
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    match run(&raw) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(AdaptError::Refused(error)) => {
            println!("REFUSED {error} ({error:?})");
            ExitCode::from(2)
        }
        Err(error) => {
            eprintln!("ERROR {error}");
            ExitCode::from(1)
        }
    }
}

// ---------------------------------------------------------------------------------------
// Tests (synthetic, made-up sentences only)
// ---------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aprender_contrastive_data::schema::parse_jsonl_bytes;

    const TASK_DOC_ORDER: &str = r#"{
  "type": "choice",
  "instructions": "Is this sentence about cats?",
  "criteria": {
    "not_antisemitic": "first described option",
    "antisemitic": "second described option"
  }
}"#;

    const TASK_REVERSED: &str = r#"{
  "type": "choice",
  "instructions": "Is this sentence about cats?",
  "criteria": {
    "antisemitic": "first described option",
    "not_antisemitic": "second described option"
  }
}"#;

    /// `n0` rows of the first label then `n1` of the second, interleaved by index parity so
    /// file order is not already sorted by class. Texts are made up and all distinct.
    fn synthetic_jsonl(prefix: &str, n0: usize, n1: usize) -> Vec<u8> {
        let mut lines = Vec::new();
        for i in 0..n0.max(n1) {
            if i < n0 {
                lines.push(format!(
                    r#"{{"text":"{prefix} plain sentence {i} about gardens","label":"not_antisemitic"}}"#
                ));
            }
            if i < n1 {
                lines.push(format!(
                    r#"{{"text":"{prefix} marked sentence {i} about rivers","label":"antisemitic"}}"#
                ));
            }
        }
        let mut body = lines.join("\n");
        body.push('\n');
        body.into_bytes()
    }

    fn names() -> Vec<String> {
        vec!["not_antisemitic".to_string(), "antisemitic".to_string()]
    }

    fn adapt_synthetic(seed: u64) -> Adapted {
        let train = synthetic_jsonl("tr", 41, 17);
        let eval = synthetic_jsonl("ev", 12, 5);
        adapt(&Inputs {
            laya_dir_label: "synthetic".to_string(),
            task: TASK_DOC_ORDER.as_bytes(),
            train: &train,
            eval: &eval,
            fraction: 0.1,
            seed,
        })
        .expect("the synthetic Laya dir must adapt")
    }

    fn class_labels(rows: &[(String, usize)]) -> Vec<usize> {
        rows.iter().map(|(_, label)| *label).collect()
    }

    #[test]
    fn carve_is_deterministic_for_a_seed_and_changes_with_it() {
        let rows = parse_laya_rows(&synthetic_jsonl("tr", 41, 17), "train.jsonl", &names())
            .expect("parse");
        let labels = class_labels(&rows);
        let a = carve_validation(&labels, &names(), 0.1, 13).expect("carve");
        let b = carve_validation(&labels, &names(), 0.1, 13).expect("carve");
        let c = carve_validation(&labels, &names(), 0.1, 14).expect("carve");
        assert_eq!(a, b, "the same seed must carve the same rows");
        assert_ne!(
            a.validation, c.validation,
            "another seed must carve other rows"
        );
    }

    #[test]
    fn carve_is_stratified_with_ceil_per_class() {
        let rows = parse_laya_rows(&synthetic_jsonl("tr", 41, 17), "train.jsonl", &names())
            .expect("parse");
        let carve = carve_validation(&class_labels(&rows), &names(), 0.1, 13).expect("carve");
        // ceil(0.1 * 41) = 5, ceil(0.1 * 17) = 2
        assert_eq!(carve.per_class[0].validation_rows, 5);
        assert_eq!(carve.per_class[1].validation_rows, 2);
        let in_validation = |class: usize| {
            carve
                .validation
                .iter()
                .filter(|index| rows[**index].1 == class)
                .count()
        };
        assert_eq!(in_validation(0), 5);
        assert_eq!(in_validation(1), 2);
    }

    #[test]
    fn ceil_share_ignores_float_noise() {
        // 0.1 * 30 is 3.0000000000000004 in f64; the share is 3, not 4.
        assert_eq!(ceil_share(0.1, 30), 3);
        assert_eq!(ceil_share(0.1, 41), 5);
        assert_eq!(ceil_share(0.1, 1494), 150);
        assert_eq!(ceil_share(0.1, 7016), 702);
    }

    #[test]
    fn carve_partitions_train_and_validation_is_text_disjoint_from_the_rest() {
        let rows = parse_laya_rows(&synthetic_jsonl("tr", 41, 17), "train.jsonl", &names())
            .expect("parse");
        let carve = carve_validation(&class_labels(&rows), &names(), 0.1, 13).expect("carve");
        assert_eq!(carve.train.len() + carve.validation.len(), rows.len());
        let validation: std::collections::BTreeSet<&str> = carve
            .validation
            .iter()
            .map(|index| rows[*index].0.as_str())
            .collect();
        let train: std::collections::BTreeSet<&str> = carve
            .train
            .iter()
            .map(|index| rows[*index].0.as_str())
            .collect();
        assert!(
            validation.is_disjoint(&train),
            "validation texts must not appear in train"
        );
        assert_eq!(validation.len() + train.len(), rows.len());
        assert!(
            carve.train.windows(2).all(|w| w[0] < w[1]),
            "train keeps file order"
        );
        assert!(
            carve.validation.windows(2).all(|w| w[0] < w[1]),
            "validation keeps file order"
        );
    }

    #[test]
    fn test_split_is_the_eval_rows_in_file_order_and_validation_never_touches_eval() {
        let adapted = adapt_synthetic(13);
        let eval =
            parse_laya_rows(&synthetic_jsonl("ev", 12, 5), "eval.jsonl", &names()).expect("parse");
        let test = parse_jsonl_bytes(adapted.jsonl.get("test").expect("test bytes"), "test")
            .expect("parse test");
        let validation = parse_jsonl_bytes(
            adapted.jsonl.get("validation").expect("validation bytes"),
            "validation",
        )
        .expect("parse validation");
        assert_eq!(test.len(), eval.len());
        for (row, (text, label)) in test.iter().zip(&eval) {
            assert_eq!(
                &row.input, text,
                "test rows must be the eval rows, row for row"
            );
            assert_eq!(row.label, *label);
        }
        assert!(
            validation.iter().all(|row| row.input.starts_with("tr ")),
            "every validation row is drawn from train, never from eval"
        );
    }

    #[test]
    fn round_trip_through_the_crate_keeps_ids_and_labels_per_role() {
        let adapted = adapt_synthetic(13);
        for (role, counts) in [
            ("train", &adapted.train_counts),
            ("validation", &adapted.validation_counts),
            ("test", &adapted.test_counts),
        ] {
            let bytes = adapted.jsonl.get(role).expect("role bytes");
            let rows = parse_jsonl_bytes(bytes, role).expect("the crate parses what it encoded");
            let total: u64 = counts.iter().sum();
            assert_eq!(rows.len() as u64, total, "{role}: row count");
            for (index, row) in rows.iter().enumerate() {
                assert_eq!(row.id, format!("{role}:{index}"));
                assert_eq!(row.source_split, role);
                assert_eq!(row.label_text, names()[row.label]);
            }
            let again = aprender_contrastive_data::schema::encode_jsonl(&rows).expect("encode");
            assert_eq!(again, bytes, "{role}: encode(parse(b)) == b");
        }
    }

    #[test]
    fn manifest_has_numeric_schema_version_2_and_a_dataset_attestation_object() {
        let adapted = adapt_synthetic(13);
        let value: serde_json::Value =
            serde_json::from_slice(&adapted.manifest_bytes).expect("manifest is JSON");
        assert_eq!(value["schema_version"].as_u64(), Some(2));
        assert!(value["dataset_attestation"].is_object());
        assert_eq!(
            value["dataset_attestation"]["schema_version"].as_u64(),
            Some(2)
        );
        assert_eq!(
            value["labels"],
            serde_json::json!(["not_antisemitic", "antisemitic"])
        );
        assert_eq!(value["profile"], "canonical");
        assert_eq!(
            value["dataset_attestation"]["dataset_fingerprint"].as_str(),
            Some(adapted.dataset_fingerprint.as_str())
        );
    }

    #[test]
    fn label_order_is_document_order_not_lexical_order() {
        let in_order = label_names_from_task(TASK_DOC_ORDER.as_bytes()).expect("task");
        assert_eq!(in_order, vec!["not_antisemitic", "antisemitic"]);
        let reversed = label_names_from_task(TASK_REVERSED.as_bytes()).expect("task");
        assert_eq!(reversed, vec!["antisemitic", "not_antisemitic"]);
        // Lexical order would put `antisemitic` first for BOTH documents.
        let mut lexical = in_order.clone();
        lexical.sort();
        assert_ne!(
            in_order, lexical,
            "the document order is not the lexical order"
        );
        assert_ne!(in_order, reversed, "no sort is in the path");

        // And the labels written to the rows follow it.
        let adapted = adapt_synthetic(13);
        assert_eq!(adapted.label_names, vec!["not_antisemitic", "antisemitic"]);
        let first =
            parse_jsonl_bytes(adapted.jsonl.get("test").expect("test"), "test").expect("parse");
        let not_antisemitic = first
            .iter()
            .find(|row| row.label_text == "not_antisemitic")
            .expect("a not_antisemitic row");
        assert_eq!(not_antisemitic.label, 0);
        let antisemitic = first
            .iter()
            .find(|row| row.label_text == "antisemitic")
            .expect("an antisemitic row");
        assert_eq!(antisemitic.label, 1);
    }

    #[test]
    fn source_split_equals_the_role() {
        let adapted = adapt_synthetic(13);
        for role in ["train", "validation", "test"] {
            let rows =
                parse_jsonl_bytes(adapted.jsonl.get(role).expect("bytes"), role).expect("parse");
            assert!(!rows.is_empty());
            assert!(rows.iter().all(|row| row.source_split == role));
        }

        // A row carrying another role's string is refused by the crate's ladder.
        let items = [
            ("made-up sentence one", 0usize),
            ("made-up sentence two", 1usize),
        ];
        let train = labeled_rows("train", &items, &names());
        let validation = labeled_rows(
            "train",
            &[("made-up sentence three", 0), ("made-up sentence four", 1)],
            &names(),
        );
        let test = labeled_rows(
            "test",
            &[("made-up sentence five", 0), ("made-up sentence six", 1)],
            &names(),
        );
        let refused = assemble(train, validation, test, &names())
            .expect_err("a validation row claiming `train` must be refused");
        assert!(
            matches!(refused, ContrastiveDataError::SplitRoleMismatch { .. }),
            "expected SplitRoleMismatch, got {refused:?}"
        );
    }

    #[test]
    fn input_defects_are_errors_not_silent_drops() {
        let eval = synthetic_jsonl("ev", 3, 2);
        let train = synthetic_jsonl("tr", 10, 4);
        // A label outside the criteria.
        let bad_label = br#"{"text":"made-up","label":"neutral"}
"#;
        assert!(parse_laya_rows(bad_label, "train.jsonl", &names()).is_err());
        // An extra key.
        let extra = br#"{"text":"made-up","label":"antisemitic","id":3}
"#;
        assert!(parse_laya_rows(extra, "train.jsonl", &names()).is_err());
        // A criteria object that repeats a name.
        let duplicate = br#"{"type":"choice","instructions":"x","criteria":{"a":"1","a":"2"}}"#;
        assert!(label_names_from_task(duplicate).is_err());
        // Three labels are not the two-label cell.
        let three = br#"{"type":"choice","instructions":"x","criteria":{"a":"1","b":"2","c":"3"}}"#;
        assert!(label_names_from_task(three).is_err());
        // A good dir still adapts after all that.
        assert!(adapt(&Inputs {
            laya_dir_label: "synthetic".to_string(),
            task: TASK_DOC_ORDER.as_bytes(),
            train: &train,
            eval: &eval,
            fraction: 0.1,
            seed: 13,
        })
        .is_ok());
    }

    #[test]
    fn a_cross_split_leak_is_the_crates_result_not_the_adapters() {
        // The same made-up sentence in train and eval: the crate decides what that means
        // (exclusion record or refusal); the adapter does not drop the row first.
        let train = br#"{"text":"identical made-up sentence","label":"antisemitic"}
{"text":"another made-up train sentence","label":"not_antisemitic"}
{"text":"a third made-up train sentence","label":"not_antisemitic"}
{"text":"a fourth made-up train sentence","label":"antisemitic"}
"#;
        let eval = br#"{"text":"identical made-up sentence","label":"antisemitic"}
{"text":"a made-up eval sentence","label":"not_antisemitic"}
"#;
        let outcome = adapt(&Inputs {
            laya_dir_label: "synthetic".to_string(),
            task: TASK_DOC_ORDER.as_bytes(),
            train,
            eval,
            fraction: 0.5,
            seed: 13,
        });
        match outcome {
            Ok(adapted) => {
                // Accepted: the leak is recorded by the crate in its attestation, and the
                // test split is still the eval rows, row for row.
                let test = parse_jsonl_bytes(adapted.jsonl.get("test").expect("test"), "test")
                    .expect("parse");
                assert_eq!(test.len(), 2);
            }
            Err(AdaptError::Refused(_)) => {}
            Err(other) => panic!("a leak must be the crate's call, got {other}"),
        }
    }

    #[test]
    fn write_refuses_existing_outputs_without_force() {
        let adapted = adapt_synthetic(13);
        let dir = std::env::temp_dir().join(format!("laya-adapter-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_outputs(&dir, &adapted, false).expect("first write into an empty dir");
        for name in OUTPUT_FILES {
            assert!(dir.join(name).is_file(), "{name} written");
        }
        let second = write_outputs(&dir, &adapted, false);
        assert!(
            matches!(second, Err(AdaptError::Io(_))),
            "second write must be refused"
        );
        write_outputs(&dir, &adapted, true).expect("--force replaces known outputs");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn arguments_are_validated() {
        let ok = parse_args(&[]).expect("defaults");
        assert_eq!(ok.seed, 13);
        assert!((ok.fraction - 0.1).abs() < f64::EPSILON);
        assert!(parse_args(&["--validation-fraction".into(), "1.5".into()]).is_err());
        assert!(parse_args(&["--seed".into()]).is_err());
        assert!(parse_args(&["--nope".into()]).is_err());
    }
}
