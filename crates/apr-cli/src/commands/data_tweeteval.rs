//! TweetEval abortion stance benchmark preparation.
//!
//! The canonical TweetEval source stores one text file and one label file per
//! split. This command converts those files to aprender's classification JSONL
//! schema without vendoring or redistributing the original tweets.

use crate::error::{CliError, Result};
use crate::output;
use crate::TweetEvalStanceProfile;
use colored::Colorize;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) const DATASET_ID: &str = "tweet_eval_stance_abortion";
const TARGET: &str = "Legalization of Abortion";
/// Pinned canonical TweetEval revision. Single source of truth for the clap
/// default, the recorded provenance, and the tests.
pub(crate) const CANONICAL_REVISION: &str = "4fbd22cd78421f05b1ecdb4fc5725bc7a7bd8f66";
const UPSTREAM_REPOSITORY: &str = "https://github.com/cardiffnlp/tweeteval";
const UPSTREAM_DATA_PATH: &str = "datasets/stance/abortion";
/// Canonical stance label mapping. `apr eval --dataset tweet-eval-stance`
/// reuses this so the CLI cannot drift from the generated manifest.
pub(crate) const LABEL_NAMES: [&str; 3] = ["none", "against", "favor"];
/// Class indices contributing to the official TweetEval stance score.
pub(crate) const F_AVG_CLASSES: [usize; 2] = [1, 2];
/// Official TweetEval stance score, stated once for every consumer.
pub(crate) const F_AVG_FORMULA: &str = "(F1_against + F1_favor) / 2";
const SOURCE_FILES: [&str; 6] = [
    "train_text.txt",
    "train_labels.txt",
    "val_text.txt",
    "val_labels.txt",
    "test_text.txt",
    "test_labels.txt",
];
const TRAIN_COUNTS: [usize; 3] = [159, 319, 109];
const VALIDATION_COUNTS: [usize; 3] = [18, 36, 12];
const TEST_COUNTS: [usize; 3] = [45, 189, 46];
const FEW_SHOT_SIZES: [usize; 4] = [8, 16, 32, 64];
const BENCHMARK_SEEDS: [u64; 10] = [13, 17, 23, 29, 31, 37, 41, 43, 47, 53];

#[derive(Debug, Clone, Serialize)]
struct StanceSample {
    id: String,
    input: String,
    label: usize,
    label_text: &'static str,
    source_split: String,
}

#[derive(Debug)]
struct CanonicalDataset {
    train: Vec<StanceSample>,
    validation: Vec<StanceSample>,
    test: Vec<StanceSample>,
    source_sha256: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct SourceManifest {
    repository: &'static str,
    data_path: &'static str,
    revision: String,
    /// True only when this run fetched the files from the pinned revision.
    /// With `--source` the revision is user-asserted and unverified, so
    /// consumers must not treat it as provenance.
    revision_verified: bool,
    files_sha256: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct SplitManifest {
    file: String,
    source_splits: Vec<String>,
    samples: usize,
    class_counts: BTreeMap<String, usize>,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct EvaluationManifest {
    primary_metric: &'static str,
    formula: &'static str,
    included_labels: [&'static str; 2],
    secondary_metrics: [&'static str; 5],
}

#[derive(Debug, Serialize)]
struct FewShotManifest {
    sampling: &'static str,
    shots_per_class: [usize; 4],
    seeds: [u64; 10],
}

#[derive(Debug, Serialize)]
struct BenchmarkManifest {
    schema_version: u32,
    dataset: &'static str,
    target: &'static str,
    task: &'static str,
    profile: &'static str,
    labels: BTreeMap<usize, &'static str>,
    source: SourceManifest,
    splits: BTreeMap<String, SplitManifest>,
    evaluation: EvaluationManifest,
    few_shot: FewShotManifest,
    license_notice: &'static str,
}

/// Prepare the benchmark from a local canonical source directory or a pinned download.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    output_dir: &Path,
    profile: TweetEvalStanceProfile,
    source_dir: Option<&Path>,
    revision: &str,
    force: bool,
    offline: bool,
    json_output: bool,
) -> Result<()> {
    validate_revision(revision)?;

    let downloaded = match source_dir {
        Some(_) => None,
        None => {
            if offline {
                return Err(CliError::ValidationFailed(
                    "--offline requires --source <canonical-tweeteval-directory>".to_string(),
                ));
            }
            let temp = tempfile::tempdir().map_err(CliError::Io)?;
            download_source(temp.path(), revision)?;
            Some(temp)
        }
    };
    let source_dir = source_dir.unwrap_or_else(|| {
        downloaded
            .as_ref()
            .expect("downloaded source exists when --source is omitted")
            .path()
    });

    let revision_verified = downloaded.is_some();
    let dataset = load_canonical_dataset(source_dir)?;
    let (split_bytes, manifest) = build_outputs(dataset, profile, revision, revision_verified)?;
    write_outputs(output_dir, profile, &split_bytes, &manifest, force)?;

    if json_output {
        let value = serde_json::json!({
            "dataset": DATASET_ID,
            "profile": profile_name(profile),
            "output": output_dir.display().to_string(),
            "splits": manifest.splits,
            "manifest": output_dir.join("benchmark-manifest.json").display().to_string(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
    } else {
        output::section("TweetEval Stance Benchmark");
        println!();
        output::kv("Dataset", DATASET_ID);
        output::kv("Profile", profile_name(profile));
        output::kv(
            "Revision",
            if revision_verified {
                format!("{revision} (downloaded)")
            } else {
                format!("{revision} (asserted via --source, not verified)")
            },
        );
        output::kv("Output", output_dir.display());
        println!();
        for (name, split) in &manifest.splits {
            output::kv(
                &title_case(name),
                format!("{} ({} samples)", split.file, split.samples),
            );
        }
        println!();
        match profile {
            TweetEvalStanceProfile::Canonical => println!(
                "{} Benchmark prepared; canonical test data remains isolated from validation.",
                "OK".green()
            ),
            TweetEvalStanceProfile::Setfit => println!(
                "{} Compatibility benchmark prepared; validation and test are merged.",
                "OK".green()
            ),
        }
    }

    Ok(())
}

fn validate_revision(revision: &str) -> Result<()> {
    if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CliError::ValidationFailed(
            "TweetEval --revision must be a full 40-character git commit SHA".to_string(),
        ));
    }
    Ok(())
}

fn download_source(destination: &Path, revision: &str) -> Result<()> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .build();

    for filename in SOURCE_FILES {
        let url = format!(
            "https://raw.githubusercontent.com/cardiffnlp/tweeteval/{revision}/{UPSTREAM_DATA_PATH}/{filename}"
        );
        let response = agent.get(&url).call().map_err(|error| match error {
            ureq::Error::Status(404, _) => CliError::HttpNotFound(format!(
                "TweetEval source file not found at revision {revision}: {filename}"
            )),
            other => CliError::NetworkError(format!(
                "Failed to download TweetEval source file {filename}: {other}"
            )),
        })?;
        let path = destination.join(filename);
        let mut file = fs::File::create(&path)?;
        io::copy(&mut response.into_reader(), &mut file)?;
    }
    Ok(())
}

fn load_canonical_dataset(source_dir: &Path) -> Result<CanonicalDataset> {
    if !source_dir.is_dir() {
        return Err(CliError::ValidationFailed(format!(
            "TweetEval source directory not found: {}",
            source_dir.display()
        )));
    }

    // Read every source file exactly once, then both hash and parse *those*
    // bytes. Reading twice would let the recorded SHA-256 describe content
    // that never passed the class-count contract.
    let mut raw: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for filename in SOURCE_FILES {
        raw.insert(
            filename.to_string(),
            read_required(&source_dir.join(filename))?,
        );
    }
    let source_sha256: BTreeMap<String, String> = raw
        .iter()
        .map(|(name, bytes)| (name.clone(), sha256(bytes)))
        .collect();

    let train = load_split(&raw, "train", TRAIN_COUNTS)?;
    let validation = load_split(&raw, "val", VALIDATION_COUNTS)?;
    let test = load_split(&raw, "test", TEST_COUNTS)?;

    Ok(CanonicalDataset {
        train,
        validation,
        test,
        source_sha256,
    })
}

fn decode_utf8<'a>(raw: &'a BTreeMap<String, Vec<u8>>, name: &str) -> Result<&'a str> {
    let bytes = raw.get(name).ok_or_else(|| {
        CliError::ValidationFailed(format!("Missing TweetEval source file: {name}"))
    })?;
    std::str::from_utf8(bytes)
        .map_err(|error| CliError::ValidationFailed(format!("{name} is not UTF-8: {error}")))
}

fn load_split(
    raw: &BTreeMap<String, Vec<u8>>,
    source_name: &str,
    expected_counts: [usize; 3],
) -> Result<Vec<StanceSample>> {
    let text = decode_utf8(raw, &format!("{source_name}_text.txt"))?;
    let labels = decode_utf8(raw, &format!("{source_name}_labels.txt"))?;

    let texts: Vec<&str> = text.lines().collect();
    let raw_labels: Vec<&str> = labels.lines().collect();
    if texts.len() != raw_labels.len() {
        return Err(CliError::ValidationFailed(format!(
            "TweetEval {source_name} text/label length mismatch: {} texts vs {} labels",
            texts.len(),
            raw_labels.len()
        )));
    }

    let canonical_name = if source_name == "val" {
        "validation"
    } else {
        source_name
    };
    let mut counts = [0usize; 3];
    let mut samples = Vec::with_capacity(texts.len());
    for (index, (input, raw_label)) in texts.iter().zip(raw_labels.iter()).enumerate() {
        if input.trim().is_empty() {
            return Err(CliError::ValidationFailed(format!(
                "TweetEval {canonical_name} sample {index} has empty text"
            )));
        }
        let label = raw_label.trim().parse::<usize>().map_err(|error| {
            CliError::ValidationFailed(format!(
                "Invalid TweetEval label '{}' in {canonical_name} sample {index}: {error}",
                raw_label.trim()
            ))
        })?;
        let label_text = LABEL_NAMES.get(label).copied().ok_or_else(|| {
            CliError::ValidationFailed(format!(
                "TweetEval label {label} in {canonical_name} sample {index} is outside 0..3"
            ))
        })?;
        counts[label] += 1;
        samples.push(StanceSample {
            id: format!("{canonical_name}:{index}"),
            input: (*input).to_string(),
            label,
            label_text,
            source_split: canonical_name.to_string(),
        });
    }

    if counts != expected_counts {
        return Err(CliError::ValidationFailed(format!(
            "TweetEval {canonical_name} class-count contract failed: expected {expected_counts:?}, got {counts:?}"
        )));
    }
    Ok(samples)
}

fn read_required(path: &Path) -> Result<Vec<u8>> {
    if !path.is_file() {
        return Err(CliError::FileNotFound(path.to_path_buf()));
    }
    fs::read(path).map_err(CliError::Io)
}

fn build_outputs(
    dataset: CanonicalDataset,
    profile: TweetEvalStanceProfile,
    revision: &str,
    revision_verified: bool,
) -> Result<(BTreeMap<String, Vec<u8>>, BenchmarkManifest)> {
    let CanonicalDataset {
        train,
        validation,
        test,
        source_sha256,
    } = dataset;
    let mut output_samples = BTreeMap::new();
    output_samples.insert("train".to_string(), (train, vec!["train".to_string()]));
    match profile {
        TweetEvalStanceProfile::Canonical => {
            output_samples.insert(
                "validation".to_string(),
                (validation, vec!["validation".to_string()]),
            );
            output_samples.insert("test".to_string(), (test, vec!["test".to_string()]));
        }
        TweetEvalStanceProfile::Setfit => {
            let mut merged = validation;
            merged.extend(test);
            output_samples.insert(
                "test".to_string(),
                (merged, vec!["validation".to_string(), "test".to_string()]),
            );
        }
    }

    let mut split_bytes = BTreeMap::new();
    let mut splits = BTreeMap::new();
    for (name, (samples, source_splits)) in output_samples {
        let bytes = encode_jsonl(&samples)?;
        let filename = format!("{name}.jsonl");
        splits.insert(
            name.clone(),
            SplitManifest {
                file: filename,
                source_splits,
                samples: samples.len(),
                class_counts: class_counts(&samples),
                sha256: sha256(&bytes),
            },
        );
        split_bytes.insert(name, bytes);
    }

    let labels = LABEL_NAMES
        .iter()
        .copied()
        .enumerate()
        .collect::<BTreeMap<usize, &'static str>>();
    let manifest = BenchmarkManifest {
        schema_version: 1,
        dataset: DATASET_ID,
        target: TARGET,
        task: "single-label three-class stance classification",
        profile: profile_name(profile),
        labels,
        source: SourceManifest {
            repository: UPSTREAM_REPOSITORY,
            data_path: UPSTREAM_DATA_PATH,
            revision: revision.to_string(),
            revision_verified,
            files_sha256: source_sha256,
        },
        splits,
        evaluation: EvaluationManifest {
            primary_metric: "f_avg",
            formula: F_AVG_FORMULA,
            included_labels: [
                LABEL_NAMES[F_AVG_CLASSES[0]],
                LABEL_NAMES[F_AVG_CLASSES[1]],
            ],
            secondary_metrics: ["macro_f1", "mcc", "per_class_f1", "ece", "confusion_matrix"],
        },
        few_shot: FewShotManifest {
            sampling: "balanced without replacement from the canonical training split",
            shots_per_class: FEW_SHOT_SIZES,
            seeds: BENCHMARK_SEEDS,
        },
        license_notice: "TweetEval refers users to the original task licenses and Twitter/X regulations; this command downloads upstream data on demand and aprender does not vendor tweet text.",
    };

    Ok((split_bytes, manifest))
}

fn encode_jsonl(samples: &[StanceSample]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for sample in samples {
        serde_json::to_writer(&mut bytes, sample).map_err(|error| {
            CliError::ValidationFailed(format!("Failed to encode TweetEval JSONL: {error}"))
        })?;
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn class_counts(samples: &[StanceSample]) -> BTreeMap<String, usize> {
    let mut counts = LABEL_NAMES
        .iter()
        .map(|name| ((*name).to_string(), 0usize))
        .collect::<BTreeMap<_, _>>();
    for sample in samples {
        *counts
            .get_mut(sample.label_text)
            .expect("label text comes from LABEL_NAMES") += 1;
    }
    counts
}

fn write_outputs(
    output_dir: &Path,
    profile: TweetEvalStanceProfile,
    split_bytes: &BTreeMap<String, Vec<u8>>,
    manifest: &BenchmarkManifest,
    force: bool,
) -> Result<()> {
    let known_files = [
        "train.jsonl",
        "validation.jsonl",
        "test.jsonl",
        "benchmark-manifest.json",
    ];
    if !force {
        if let Some(existing) = known_files
            .iter()
            .map(|name| output_dir.join(name))
            .find(|path| path.exists())
        {
            return Err(CliError::ValidationFailed(format!(
                "Refusing to replace existing benchmark file {} (pass --force to replace known outputs)",
                existing.display()
            )));
        }
    }
    fs::create_dir_all(output_dir)?;

    // A benchmark directory is only meaningful as a whole: splits plus the
    // manifest that describes them. Roll back anything already written so a
    // failure part-way through does not leave a half-written dataset that the
    // next non-`--force` run then refuses to replace.
    let mut written: Vec<PathBuf> = Vec::new();
    for (name, bytes) in split_bytes {
        let path = output_dir.join(format!("{name}.jsonl"));
        if let Err(error) = write_file(&path, bytes, force) {
            remove_all(&written);
            return Err(error);
        }
        written.push(path);
    }
    if force && profile == TweetEvalStanceProfile::Setfit {
        let stale_validation = output_dir.join("validation.jsonl");
        if stale_validation.exists() {
            if let Err(error) = fs::remove_file(stale_validation) {
                remove_all(&written);
                return Err(CliError::Io(error));
            }
        }
    }

    let mut manifest_bytes = serde_json::to_vec_pretty(manifest).map_err(|error| {
        CliError::ValidationFailed(format!("Failed to encode benchmark manifest: {error}"))
    })?;
    manifest_bytes.push(b'\n');
    let manifest_path = output_dir.join("benchmark-manifest.json");
    if let Err(error) = write_file(&manifest_path, &manifest_bytes, force) {
        remove_all(&written);
        return Err(error);
    }
    Ok(())
}

/// Best-effort removal of files written by a run that later failed.
fn remove_all(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

fn write_file(path: &Path, bytes: &[u8], force: bool) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn profile_name(profile: TweetEvalStanceProfile) -> &'static str {
    match profile {
        TweetEvalStanceProfile::Canonical => "canonical",
        TweetEvalStanceProfile::Setfit => "setfit",
    }
}

fn title_case(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_fixture_split(root: &Path, split: &str, counts: [usize; 3]) {
        let mut texts = String::new();
        let mut labels = String::new();
        let mut index = 0usize;
        for (label, count) in counts.into_iter().enumerate() {
            for _ in 0..count {
                texts.push_str(&format!("authored fixture {split} sample {index}\n"));
                labels.push_str(&format!("{label}\n"));
                index += 1;
            }
        }
        fs::write(root.join(format!("{split}_text.txt")), texts).unwrap();
        fs::write(root.join(format!("{split}_labels.txt")), labels).unwrap();
    }

    fn write_canonical_fixture(root: &Path) {
        write_fixture_split(root, "train", TRAIN_COUNTS);
        write_fixture_split(root, "val", VALIDATION_COUNTS);
        write_fixture_split(root, "test", TEST_COUNTS);
    }

    fn line_count(path: &Path) -> usize {
        fs::read_to_string(path).unwrap().lines().count()
    }

    #[test]
    fn canonical_profile_preserves_fixed_splits_and_labels() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let output = temp.path().join("output");
        fs::create_dir(&source).unwrap();
        write_canonical_fixture(&source);

        run(
            &output,
            TweetEvalStanceProfile::Canonical,
            Some(&source),
            CANONICAL_REVISION,
            false,
            true,
            true,
        )
        .unwrap();

        assert_eq!(line_count(&output.join("train.jsonl")), 587);
        assert_eq!(line_count(&output.join("validation.jsonl")), 66);
        assert_eq!(line_count(&output.join("test.jsonl")), 280);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("benchmark-manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["profile"], "canonical");
        assert_eq!(manifest["labels"]["0"], "none");
        assert_eq!(manifest["labels"]["1"], "against");
        assert_eq!(manifest["labels"]["2"], "favor");
        assert_eq!(manifest["evaluation"]["primary_metric"], "f_avg");
    }

    #[test]
    fn setfit_profile_merges_validation_and_test_only() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let output = temp.path().join("output");
        fs::create_dir(&source).unwrap();
        write_canonical_fixture(&source);

        run(
            &output,
            TweetEvalStanceProfile::Setfit,
            Some(&source),
            CANONICAL_REVISION,
            false,
            true,
            false,
        )
        .unwrap();

        assert_eq!(line_count(&output.join("train.jsonl")), 587);
        assert_eq!(line_count(&output.join("test.jsonl")), 346);
        assert!(!output.join("validation.jsonl").exists());
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("benchmark-manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["profile"], "setfit");
        assert_eq!(manifest["splits"]["test"]["class_counts"]["against"], 225);
    }

    #[test]
    fn length_mismatch_rejects_truncated_source() {
        let temp = tempfile::tempdir().unwrap();
        write_canonical_fixture(temp.path());
        fs::write(temp.path().join("train_labels.txt"), "0\n").unwrap();

        let error = load_canonical_dataset(temp.path()).unwrap_err();
        assert!(error.to_string().contains("length mismatch"));
    }

    #[test]
    fn class_count_contract_rejects_modified_source() {
        let temp = tempfile::tempdir().unwrap();
        write_canonical_fixture(temp.path());

        // Keep the row count identical so the length check passes and the
        // class-count contract is the branch actually under test: relabel one
        // `none` row as `favor`.
        let labels = fs::read_to_string(temp.path().join("train_labels.txt")).unwrap();
        let mut lines: Vec<&str> = labels.lines().collect();
        assert_eq!(lines[0], "0");
        lines[0] = "2";
        let mutated = lines.join("\n") + "\n";
        fs::write(temp.path().join("train_labels.txt"), mutated).unwrap();

        let error = load_canonical_dataset(temp.path()).unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("class-count contract failed"),
            "expected the class-count contract to reject the relabelled source, got: {message}"
        );
        assert!(message.contains("[159, 319, 109]"), "got: {message}");
        assert!(message.contains("[158, 319, 110]"), "got: {message}");
    }

    #[test]
    fn force_replaces_outputs_and_clears_stale_validation() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let output = temp.path().join("output");
        fs::create_dir(&source).unwrap();
        write_canonical_fixture(&source);

        run(
            &output,
            TweetEvalStanceProfile::Canonical,
            Some(&source),
            CANONICAL_REVISION,
            false,
            true,
            false,
        )
        .unwrap();
        assert!(output.join("validation.jsonl").exists());

        // Re-preparing the same directory as `setfit` with --force must drop
        // the canonical validation split, otherwise a 66-row file survives
        // that the regenerated manifest no longer describes.
        run(
            &output,
            TweetEvalStanceProfile::Setfit,
            Some(&source),
            CANONICAL_REVISION,
            true,
            true,
            false,
        )
        .unwrap();

        assert!(!output.join("validation.jsonl").exists());
        assert_eq!(line_count(&output.join("test.jsonl")), 346);
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("benchmark-manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["profile"], "setfit");
        assert!(manifest["splits"].get("validation").is_none());
    }

    #[test]
    fn local_source_records_the_revision_as_unverified() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let output = temp.path().join("output");
        fs::create_dir(&source).unwrap();
        write_canonical_fixture(&source);

        run(
            &output,
            TweetEvalStanceProfile::Canonical,
            Some(&source),
            CANONICAL_REVISION,
            false,
            true,
            false,
        )
        .unwrap();

        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(output.join("benchmark-manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["source"]["revision"], CANONICAL_REVISION);
        assert_eq!(manifest["source"]["revision_verified"], false);
    }

    #[test]
    fn existing_outputs_require_force() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let output = temp.path().join("output");
        fs::create_dir(&source).unwrap();
        write_canonical_fixture(&source);
        run(
            &output,
            TweetEvalStanceProfile::Canonical,
            Some(&source),
            CANONICAL_REVISION,
            false,
            true,
            false,
        )
        .unwrap();

        let error = run(
            &output,
            TweetEvalStanceProfile::Canonical,
            Some(&source),
            CANONICAL_REVISION,
            false,
            true,
            false,
        )
        .unwrap_err();
        assert!(error.to_string().contains("Refusing to replace"));
    }

    #[test]
    fn download_is_forbidden_in_offline_mode() {
        let temp = tempfile::tempdir().unwrap();
        let error = run(
            temp.path(),
            TweetEvalStanceProfile::Canonical,
            None,
            CANONICAL_REVISION,
            false,
            true,
            false,
        )
        .unwrap_err();
        assert!(error.to_string().contains("--source"));
    }

    #[test]
    #[ignore = "opt-in network test against the pinned canonical TweetEval revision"]
    fn pinned_upstream_satisfies_the_dataset_contract() {
        let temp = tempfile::tempdir().unwrap();
        download_source(temp.path(), CANONICAL_REVISION).unwrap();
        let dataset = load_canonical_dataset(temp.path()).unwrap();
        assert_eq!(dataset.train.len(), 587);
        assert_eq!(dataset.validation.len(), 66);
        assert_eq!(dataset.test.len(), 280);
    }

    /// Build a one-split raw source map from an explicit list of label strings.
    ///
    /// Text is synthetic and never empty, so the only validation branch this can
    /// exercise is the label one — which is the point.
    fn raw_split_with_labels(split: &str, labels: &[String]) -> BTreeMap<String, Vec<u8>> {
        let mut text = String::new();
        let mut label_lines = String::new();
        for (index, label) in labels.iter().enumerate() {
            text.push_str(&format!("authored fixture {split} sample {index}\n"));
            label_lines.push_str(label);
            label_lines.push('\n');
        }
        let mut raw = BTreeMap::new();
        raw.insert(format!("{split}_text.txt"), text.into_bytes());
        raw.insert(format!("{split}_labels.txt"), label_lines.into_bytes());
        raw
    }

    proptest::proptest! {
        /// OBLIG-TWEET-EVAL-LABEL-BOUNDS, and the runnable evidence standing in for
        /// the DECLARED-but-never-executed KANI-TWEET-EVAL-001 (cargo-kani is not
        /// installed in this repository and no `#[kani::proof]` harness exists here).
        ///
        /// Bounded identically to that harness — bound 4, i.e. label values drawn
        /// from `0..4`, which covers the three valid indices plus the first
        /// out-of-range one. Randomized and bounded, not exhaustive: closing that
        /// gap is exactly what a real Kani run would add.
        ///
        /// The property is an ORDERING claim, not just a range claim. `load_split`
        /// does `counts[label] += 1` immediately after resolving `label_text`, so if
        /// the bound check did not strictly precede the increment, an out-of-range
        /// label would panic with an index-out-of-bounds instead of returning a
        /// typed error. proptest treats a panic as a failure, so this test
        /// distinguishes "rejected properly" from "crashed".
        #[test]
        fn label_index_is_in_bounds_or_a_typed_error(
            labels in proptest::collection::vec(0usize..4, 1usize..=8)
        ) {
            let label_strings: Vec<String> =
                labels.iter().map(|label| label.to_string()).collect();
            let raw = raw_split_with_labels("train", &label_strings);

            let mut expected_counts = [0usize; 3];
            for label in &labels {
                if let Some(slot) = expected_counts.get_mut(*label) {
                    *slot += 1;
                }
            }
            let any_out_of_range = labels.iter().any(|label| *label >= LABEL_NAMES.len());

            let result = load_split(&raw, "train", expected_counts);

            if any_out_of_range {
                let error = result.expect_err(
                    "a label at or above LABEL_NAMES.len() must be REJECTED; accepting it \
                     means the bound check does not gate the label map lookup",
                );
                let message = error.to_string();
                // The message must NAME the offending value, because that is what
                // makes a real corrupt-source failure diagnosable rather than red.
                proptest::prop_assert!(
                    message.contains("is outside 0..3"),
                    "expected the out-of-range label diagnosis, got: {message}"
                );
            } else {
                let samples = result.expect(
                    "every label below LABEL_NAMES.len() is in bounds, and the expected \
                     counts were computed from these very labels",
                );
                proptest::prop_assert_eq!(samples.len(), labels.len());
                for sample in &samples {
                    proptest::prop_assert!(sample.label < LABEL_NAMES.len());
                    proptest::prop_assert_eq!(sample.label_text, LABEL_NAMES[sample.label]);
                }
            }
        }
    }
}
