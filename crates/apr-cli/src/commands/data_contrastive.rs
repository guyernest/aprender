//! `apr data select` and `apr data pairs` — the user surface of the deterministic
//! contrastive data protocol.
//!
//! # This file is a FILESYSTEM ADAPTER and nothing else (D-05)
//!
//! It reads bytes, hands them to `aprender-contrastive-data`, and writes the bytes the
//! crate hands back. No parsing, hashing, selection, sampling, budget resolution or
//! manifest composition happens here — all of it lives behind the crate's public API,
//! which is bytes-in / typed-values-out by construction (D-04, enforced by
//! `make contrastive-data-boundary`).
//!
//! # Neither command touches the network
//!
//! There is no `--offline` parameter on either entry point. The crate opens no socket and
//! this module only reads and writes local files, so an offline switch would advertise a
//! capability that does not exist.
//!
//! # Every artifact goes through ONE writer
//!
//! `atomic_write` is the only function here that creates a file: temp file in the
//! DESTINATION directory, `write_all`, `sync_all`, `rename`, no-clobber unless `--force`,
//! and temp cleanup on every error path. `apr data pairs --dump` calls the same helper, so
//! the three write-safety properties are proven once instead of once per artifact.
//!
//! # `run_pairs` is still Task 1's placeholder
//!
//! Plan 02-09 Task 3 fills it in without changing its signature. The two std placeholder
//! macros are deliberately NOT used — both abort the process, and panicking macros are
//! banned by repo policy — so the body returns a structured `CliError` instead. Naming
//! them literally here would also trip this plan's own acceptance grep, which is a
//! self-needle of the kind plan 02-08 had to fix twice.

use crate::commands::data_tweeteval;
use crate::error::{CliError, Result};
use crate::output;
use aprender_contrastive_data::attestation::DatasetAttestation;
use aprender_contrastive_data::dedup::ExclusionRecord;
use aprender_contrastive_data::error::ContrastiveDataError;
use aprender_contrastive_data::ledger::{AccessLedger, AccessRecord};
use aprender_contrastive_data::manifest::{SelectedExampleRecord, SelectionManifest};
use aprender_contrastive_data::prepared::{Canonical, PreparedDataset};
use aprender_contrastive_data::select::{FewShotSelector, SelectionConfig};
use aprender_contrastive_data::split::{CompatibilityTest, SplitRole};
use colored::Colorize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The file `apr data select` writes and `apr data pairs` reads.
pub(crate) const SELECTION_MANIFEST_FILE: &str = "selection-manifest.json";

/// Wire name for a seed drawn from the ten contracted benchmark seeds.
const SEED_MODE_CONTRACTED: &str = "contracted";

/// Wire name for a seed accepted only because `--any-seed` was passed.
const SEED_MODE_UNCONTRACTED: &str = "uncontracted";

/// What to do when a benchmark directory is missing or unreadable. An error that names a
/// missing file without naming the command that produces it is diagnosable but not
/// actionable.
const PREPARE_REMEDY: &str = "Prepare one with \
     `apr data tweet-eval-stance --output <DIR>` (canonical profile), then point --data at \
     that directory.";

// ==========================================================================================
// Test-only fault-injection seam
// ==========================================================================================

// When set, `atomic_write` fails AFTER the temp file is written and synced but BEFORE the
// rename — the one window in which a partial artifact could exist.
//
// A `thread_local` rather than a global: cargo runs each test on its own thread, so two
// tests cannot see each other's injection.
//
// Plain `//` rather than `///`: rustdoc generates nothing for a macro invocation, and
// `-D warnings` rejects the doc form (`unused_doc_comments`).
#[cfg(test)]
thread_local! {
    static FAIL_BEFORE_RENAME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether an induced pre-rename failure is armed on this thread.
#[cfg(test)]
fn induced_prerename_failure() -> bool {
    FAIL_BEFORE_RENAME.with(std::cell::Cell::get)
}

/// Production builds have no seam at all.
#[cfg(not(test))]
const fn induced_prerename_failure() -> bool {
    false
}

// ==========================================================================================
// The shared atomic writer — the only thing in this module that creates a file
// ==========================================================================================

/// A temp name inside the DESTINATION directory, unique per process and per call.
fn temp_path(target: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ordinal = COUNTER.fetch_add(1, Ordering::Relaxed);
    let stem = target.file_name().map_or_else(
        || "artifact".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    dir.join(format!(".{stem}.tmp.{}.{ordinal}", std::process::id()))
}

/// Fill the temp file and get it onto the platter, then offer the injection point.
///
/// `create_new` on the TEMP as well: two concurrent runs must not share one scratch file,
/// and a leftover scratch file from a crashed run is a diagnosable error rather than
/// silent reuse.
fn write_and_sync(temp: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    if induced_prerename_failure() {
        return Err(CliError::Io(std::io::Error::other(
            "induced pre-rename failure (test seam)",
        )));
    }
    Ok(())
}

/// Write `bytes` to `target` atomically, with no-clobber by default.
///
/// The temp file lives in the destination directory because `rename` is only atomic within
/// one filesystem; a temp in `/tmp` would silently degrade to a copy across a mount point,
/// which is exactly the partial-write window this exists to close.
///
/// Every failure path removes the temp, so an interrupted write leaves neither a partial
/// artifact nor a stray file for the next `--force`-less run to trip over.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] when `target` exists and `force` is false;
/// [`CliError::Io`] for any create, write, sync or rename failure.
fn atomic_write(target: &Path, bytes: &[u8], force: bool) -> Result<()> {
    if !force && target.exists() {
        return Err(CliError::ValidationFailed(format!(
            "Refusing to replace existing file {} (pass --force to replace it)",
            target.display()
        )));
    }
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;

    let temp = temp_path(target);
    let result =
        write_and_sync(&temp, bytes).and_then(|()| fs::rename(&temp, target).map_err(CliError::Io));
    if result.is_err() {
        // Best-effort: the write has already failed, and a cleanup failure must not
        // replace the reason it failed.
        let _ = fs::remove_file(&temp);
    }
    result
}

// ==========================================================================================
// Attested ingest — the ONLY door to a canonical dataset (D-16 / review finding F16)
// ==========================================================================================

/// Map a crate failure onto the CLI surface without losing detail. The crate's messages
/// already name the split, the field, and both the expected and observed value.
fn dataset_error(error: &ContrastiveDataError) -> CliError {
    CliError::ValidationFailed(format!("contrastive data: {error}"))
}

/// Read one required file, with a message that says what to do about it.
fn read_required(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::ValidationFailed(format!("{} not found. {PREPARE_REMEDY}", path.display()))
        } else {
            CliError::Io(error)
        }
    })
}

/// Split ROLE -> the file that holds it inside a prepared benchmark directory.
///
/// Canonical roles are `{role}.jsonl`; the compatibility profile writes its
/// `compatibility_test` role to `test.jsonl`, which is the filename the SetFit wrapper
/// expects. Handling that case here is what lets a compatibility directory be read far
/// enough to be refused by PROFILE — "no such file: compatibility_test.jsonl" would be a
/// true statement about the wrong problem.
fn role_file(role: &str) -> String {
    if role == CompatibilityTest::ROLE {
        "test.jsonl".to_string()
    } else {
        format!("{role}.jsonl")
    }
}

/// Open a prepared benchmark directory through the crate's attested boundary.
///
/// The CLI supplies bytes and nothing else. Which files to read comes from the
/// attestation's own split roles rather than from a hardcoded canonical triple, so a
/// compatibility, mixed, stale or forged directory reaches the boundary and is refused
/// there by profile, schema version, per-split digest, class counts, exclusion digest or
/// fingerprint — instead of dying earlier on a missing filename.
///
/// Only the CANONICAL constructor is ever called. A compatibility attestation is
/// `ProfileMismatch`, and the type system independently forbids a
/// `PreparedDataset<Compatibility>` from reaching selection at all (D-19).
///
/// # Errors
///
/// [`CliError::ValidationFailed`] for a missing/unreadable file, a manifest this build
/// cannot read, or any attested-boundary rejection.
fn read_attested_canonical(
    data_dir: &Path,
    ledger: &mut AccessLedger,
) -> Result<PreparedDataset<Canonical>> {
    let manifest_bytes = read_required(&data_dir.join(data_tweeteval::MANIFEST_FILE))?;
    // The schema-version gate and the attestation extraction belong to `data_tweeteval`,
    // so there is ONE reader of `benchmark-manifest.json` rather than two that can drift.
    let attestation_bytes = data_tweeteval::attestation_bytes_from_manifest(&manifest_bytes)?;
    let attestation =
        DatasetAttestation::from_bytes(&attestation_bytes).map_err(|e| dataset_error(&e))?;

    let mut buffers = BTreeMap::new();
    for role in attestation.splits.keys() {
        buffers.insert(
            role.clone(),
            read_required(&data_dir.join(role_file(role)))?,
        );
    }
    PreparedDataset::<Canonical>::from_attested_bytes(&attestation_bytes, &buffers, ledger)
        .map_err(|e| dataset_error(&e))
}

// ==========================================================================================
// Request pre-flight: fail on a bad REQUEST before blaming the data
// ==========================================================================================

/// The contracted few-shot sizes, rendered for an error message.
fn allowed_shots_text() -> String {
    let sizes: Vec<String> = data_tweeteval::FEW_SHOT_SIZES
        .iter()
        .map(usize::to_string)
        .collect();
    format!("{{{}}}", sizes.join(", "))
}

/// Reject a shot count outside the contracted set, BEFORE anything is read.
///
/// The list is `data_tweeteval::FEW_SHOT_SIZES` — the same array written into every
/// benchmark manifest's `few_shot` section — not a third copy of the four literals. The
/// crate re-validates independently inside `FewShotSelector::select`, so if the two ever
/// disagreed the result would be a typed `InvalidShots` error, never an off-protocol
/// selection.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] naming the offending value and the allowed set.
fn validate_shots(shots: u32) -> Result<()> {
    if data_tweeteval::FEW_SHOT_SIZES
        .iter()
        .any(|size| u64::try_from(*size).is_ok_and(|size| size == u64::from(shots)))
    {
        return Ok(());
    }
    Err(CliError::ValidationFailed(format!(
        "--shots {shots} is not a contracted few-shot size; expected one of {}",
        allowed_shots_text()
    )))
}

/// Resolve the seed mode, refusing an uncontracted seed unless `--any-seed` was passed.
///
/// The ten seeds are `data_tweeteval::BENCHMARK_SEEDS`
/// (`crates/apr-cli/src/commands/data_tweeteval.rs`), the same array written into every
/// benchmark manifest's `few_shot.seeds`. **42 is not among them**, which is why `--seed`
/// has no default.
///
/// The returned mode is REPORTED, not stored as a separate manifest field: the manifest
/// records `root_seed`, and the mode is a total function of it. A parallel `seed_mode`
/// field could disagree with the seed printed beside it, which is strictly worse than
/// deriving it.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] naming the seed, the ten contracted seeds and the
/// `--any-seed` escape hatch.
fn resolve_seed_mode(seed: u64, any_seed: bool) -> Result<&'static str> {
    if data_tweeteval::BENCHMARK_SEEDS.contains(&seed) {
        return Ok(SEED_MODE_CONTRACTED);
    }
    if any_seed {
        return Ok(SEED_MODE_UNCONTRACTED);
    }
    let seeds: Vec<String> = data_tweeteval::BENCHMARK_SEEDS
        .iter()
        .map(u64::to_string)
        .collect();
    Err(CliError::ValidationFailed(format!(
        "--seed {seed} is not one of the ten contracted benchmark seeds [{}]. Pass \
         --any-seed to draw an experimental selection anyway; the seed itself is recorded \
         in the manifest, so a reader can always tell which of the two you did",
        seeds.join(", ")
    )))
}

// ==========================================================================================
// Machine-readable reports
// ==========================================================================================

/// `apr data select --json`.
///
/// Every field is read off the manifest the crate produced; the CLI adds only the two
/// paths and the derived seed mode.
#[derive(Serialize)]
struct SelectReport<'a> {
    command: &'static str,
    data: String,
    manifest: String,
    profile: &'a str,
    root_seed: u64,
    /// Derived from `root_seed`, not stored beside it — see `seed_mode_of`.
    seed_mode: &'static str,
    shots_per_class: u32,
    selected: usize,
    semantic_hash: &'a str,
    ledger_hash: &'a str,
    ordered_examples: &'a [SelectedExampleRecord],
    exclusions: &'a ExclusionRecord,
    access_ledger: &'a [AccessRecord],
}

/// Render the `--json` report for a written selection manifest.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] if the report cannot be serialized.
fn select_report_json(
    data_dir: &Path,
    manifest_path: &Path,
    manifest: &SelectionManifest,
    seed_mode: &'static str,
) -> Result<String> {
    let payload = &manifest.payload;
    let report = SelectReport {
        command: "data-select",
        data: data_dir.display().to_string(),
        manifest: manifest_path.display().to_string(),
        profile: &payload.profile,
        root_seed: payload.root_seed,
        seed_mode,
        shots_per_class: payload.shots_per_class,
        selected: payload.ordered_examples.len(),
        semantic_hash: &manifest.semantic_hash,
        ledger_hash: &payload.ledger_hash,
        ordered_examples: &payload.ordered_examples,
        exclusions: &payload.exclusions,
        access_ledger: &payload.access_ledger,
    };
    serde_json::to_string_pretty(&report).map_err(|error| {
        CliError::ValidationFailed(format!("Failed to encode the selection report: {error}"))
    })
}

/// Human-readable `apr data select` output.
fn render_select_human(
    data_dir: &Path,
    manifest_path: &Path,
    manifest: &SelectionManifest,
    seed_mode: &str,
) {
    let payload = &manifest.payload;
    output::section("Few-Shot Selection");
    println!();
    output::kv("Data", data_dir.display());
    output::kv("Profile", &payload.profile);
    output::kv("Shots per class", payload.shots_per_class);
    output::kv("Root seed", format!("{} ({seed_mode})", payload.root_seed));
    output::kv(
        "Selected",
        format!(
            "{} example(s) across {} class(es)",
            payload.ordered_examples.len(),
            payload.label_names.len()
        ),
    );
    output::kv("Semantic hash", &manifest.semantic_hash);
    output::kv("Ledger hash", &payload.ledger_hash);
    output::kv(
        "Excluded",
        format!(
            "{} training row(s) removed from the pool before selection",
            payload.exclusions.excluded_train_ids().len()
        ),
    );
    output::kv("Manifest", manifest_path.display());
    println!();
    println!(
        "{} Selection manifest written. Replay it with:",
        "OK".green()
    );
    println!(
        "  apr data pairs --selection {} --data {}",
        manifest_path.display(),
        data_dir.display()
    );
}

// ==========================================================================================
// Commands
// ==========================================================================================

/// Select `shots` examples per class from an attested canonical benchmark directory and
/// write the replayable selection manifest.
///
/// The whole body is adapter work: pre-flight the REQUEST, read bytes, call the crate,
/// write the crate's bytes back verbatim, report. Nothing here decides which rows are
/// selected or what the manifest looks like.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] for an invalid shot count, an uncontracted seed without
/// `--any-seed`, any attested-ingest rejection, or an existing output file without
/// `--force`; [`CliError::Io`] for a read or write failure.
pub(crate) fn run_select(
    data: &Path,
    shots: u32,
    seed: u64,
    any_seed: bool,
    output: Option<&Path>,
    force: bool,
    json_output: bool,
) -> Result<()> {
    // Fail-closed on the REQUEST first. A bad shot count or an off-protocol seed is not a
    // problem with the data, and reporting it as one sends the user to the wrong place.
    validate_shots(shots)?;
    let seed_mode = resolve_seed_mode(seed, any_seed)?;

    let mut ledger = AccessLedger::new();
    let dataset = read_attested_canonical(data, &mut ledger)?;

    let cfg = SelectionConfig {
        root_seed: seed,
        shots_per_class: shots,
    };
    let selection =
        FewShotSelector::select(&dataset, &cfg, &mut ledger).map_err(|e| dataset_error(&e))?;
    // NOTHING may touch the ledger between these two calls: `from_selection` refuses a
    // ledger that has grown since `select` returned, because the payload's embedded
    // records would no longer describe it.
    let mut manifest =
        SelectionManifest::from_selection(&selection, &ledger).map_err(|e| dataset_error(&e))?;
    // The crate reads no clock for the same reason it opens no file. The field is outside
    // the hashed region, so filling it changes no digest.
    manifest.volatile.created_at = chrono::Utc::now().to_rfc3339();

    let manifest_path = output.unwrap_or(data).join(SELECTION_MANIFEST_FILE);
    let bytes = manifest.to_file_bytes().map_err(|e| dataset_error(&e))?;
    atomic_write(&manifest_path, &bytes, force)?;

    if json_output {
        println!(
            "{}",
            select_report_json(data, &manifest_path, &manifest, seed_mode)?
        );
    } else {
        render_select_human(data, &manifest_path, &manifest, seed_mode);
    }
    Ok(())
}

/// Replay a selection manifest against its dataset and report the bounded pair stream.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] for a manifest whose digest, provenance or ordered list
/// does not survive strict replay, for a budget/hard-cap combination the crate refuses, or
/// for an existing dump file without `--force`; [`CliError::Io`] for a read or write
/// failure.
pub(crate) fn run_pairs(
    selection: &Path,
    data: &Path,
    budget: Option<u64>,
    hard_cap: Option<u64>,
    dump: Option<&Path>,
    force: bool,
    json_output: bool,
) -> Result<()> {
    let _ = (selection, data, budget, hard_cap, dump, force, json_output);
    Err(CliError::ValidationFailed(
        "apr data pairs is not yet implemented (plan 02-09 Task 3)".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::data_tweeteval::{
        self, fixtures, CANONICAL_REVISION, LABEL_NAMES, MANIFEST_FILE,
    };
    use crate::TweetEvalStanceProfile;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    // --------------------------------------------------------------------------------
    // Fixtures. Every benchmark directory here is produced by RUNNING
    // `apr data tweet-eval-stance` over a synthetic source tree — nothing fabricates an
    // attestation by hand, because a hand-built one would only prove the reader accepts
    // what this test module thinks the writer emits.
    // --------------------------------------------------------------------------------

    fn prepare(root: &Path, name: &str, tag: &str, profile: TweetEvalStanceProfile) -> PathBuf {
        let source = root.join(format!("{name}-source"));
        fs::create_dir_all(&source).expect("fixture source directory is creatable");
        fixtures::write_canonical_fixture_tagged(&source, tag);
        let out = root.join(name);
        data_tweeteval::run(
            &out,
            profile,
            Some(&source),
            CANONICAL_REVISION,
            false,
            true,
            true,
        )
        .expect("the synthetic canonical fixture prepares cleanly");
        out
    }

    fn canonical_dir(root: &Path) -> PathBuf {
        prepare(
            root,
            "canonical",
            fixtures::DEFAULT_TAG,
            TweetEvalStanceProfile::Canonical,
        )
    }

    fn compatibility_dir(root: &Path) -> PathBuf {
        prepare(
            root,
            "setfit",
            fixtures::DEFAULT_TAG,
            TweetEvalStanceProfile::Setfit,
        )
    }

    /// A directory whose `train.jsonl` comes from one preparation and whose
    /// `validation.jsonl` comes from a DIFFERENT one. Both halves are individually
    /// well-formed; only the attested per-split digest catches it.
    fn mixed_dir(root: &Path) -> PathBuf {
        let first = canonical_dir(root);
        let second = prepare(
            root,
            "second",
            "second preparation",
            TweetEvalStanceProfile::Canonical,
        );
        let foreign = fs::read(second.join("validation.jsonl")).expect("second validation split");
        let mine = fs::read(first.join("validation.jsonl")).expect("first validation split");
        assert_ne!(
            foreign, mine,
            "the two preparations must differ, or the mixed fixture proves nothing"
        );
        fs::write(first.join("validation.jsonl"), foreign).expect("validation split is writable");
        first
    }

    fn edit_manifest(dir: &Path, mutate: impl FnOnce(&mut serde_json::Value)) {
        let path = dir.join(MANIFEST_FILE);
        let bytes = fs::read(&path).expect("benchmark manifest is readable");
        let mut value: serde_json::Value =
            serde_json::from_slice(&bytes).expect("benchmark manifest is JSON");
        mutate(&mut value);
        let mut out = serde_json::to_vec_pretty(&value).expect("manifest re-encodes");
        out.push(b'\n');
        fs::write(&path, out).expect("benchmark manifest is writable");
    }

    fn read_manifest(dir: &Path) -> SelectionManifest {
        let bytes = fs::read(dir.join(SELECTION_MANIFEST_FILE)).expect("selection manifest exists");
        SelectionManifest::from_bytes(&bytes).expect("selection manifest parses and verifies")
    }

    fn message(error: &CliError) -> String {
        error.to_string()
    }

    // --------------------------------------------------------------------------------
    // Happy paths
    // --------------------------------------------------------------------------------

    #[test]
    fn select_writes_a_manifest_whose_ordered_examples_carry_ids_and_labels() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());

        run_select(&data, 8, 13, false, None, false, false).expect("select succeeds");

        let manifest = read_manifest(&data);
        assert_eq!(manifest.payload.root_seed, 13);
        assert_eq!(manifest.payload.shots_per_class, 8);
        assert_eq!(manifest.payload.profile, "canonical");
        assert_eq!(
            manifest.payload.ordered_examples.len(),
            8 * LABEL_NAMES.len(),
            "8 shots for each of the three declared classes"
        );
        // Labels, not just ids: a manifest of bare ids would force every later consumer to
        // re-derive the class from the dataset.
        let labels: Vec<usize> = manifest
            .payload
            .ordered_examples
            .iter()
            .map(|row| row.label)
            .collect();
        assert_eq!(labels.first(), Some(&0));
        assert_eq!(labels.last(), Some(&2));
        assert!(
            manifest
                .payload
                .ordered_examples
                .iter()
                .all(|row| row.id.starts_with("train:")),
            "every selected row comes from the training split"
        );
        // The persisted access ledger is the D-19 evidence Phase 5's selection lock reads.
        assert!(
            manifest
                .payload
                .access_ledger
                .iter()
                .any(|record| record.purpose == "select"),
            "the selection's own access record is persisted in the manifest"
        );
        assert!(!manifest.payload.ledger_hash.is_empty());
    }

    #[test]
    fn two_runs_produce_identical_hashed_payloads() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());

        run_select(&data, 8, 13, false, None, false, false).expect("first select");
        let first = read_manifest(&data);
        run_select(&data, 8, 13, false, None, true, false).expect("second select with --force");
        let second = read_manifest(&data);

        assert_eq!(first.semantic_hash, second.semantic_hash);
        assert_eq!(
            first
                .payload
                .to_canonical_bytes()
                .expect("payload serializes"),
            second
                .payload
                .to_canonical_bytes()
                .expect("payload serializes"),
            "the hashed payload is byte-identical across runs"
        );
        assert!(
            !first.volatile.tool_version.is_empty(),
            "the volatile block is populated even though it is never hashed"
        );
    }

    #[test]
    fn the_written_manifest_is_the_verbatim_crate_serialization() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());
        run_select(&data, 8, 17, false, None, false, false).expect("select succeeds");

        let on_disk = fs::read(data.join(SELECTION_MANIFEST_FILE)).expect("manifest readable");
        let parsed = SelectionManifest::from_bytes(&on_disk).expect("round trip");
        assert_eq!(
            parsed.to_file_bytes().expect("re-serialize"),
            on_disk,
            "the CLI writes to_file_bytes() verbatim and composes no JSON around it"
        );
    }

    #[test]
    fn the_output_directory_can_differ_from_the_data_directory() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());
        let out = temp.path().join("elsewhere");

        run_select(&data, 8, 13, false, Some(&out), false, false).expect("select succeeds");

        assert!(out.join(SELECTION_MANIFEST_FILE).is_file());
        assert!(
            !data.join(SELECTION_MANIFEST_FILE).exists(),
            "--output redirects the write; it does not duplicate it"
        );
    }

    #[test]
    fn the_json_report_carries_ids_labels_hash_ledger_and_reduced_pools() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());
        run_select(&data, 8, 13, false, None, false, false).expect("select succeeds");
        let manifest = read_manifest(&data);
        let path = data.join(SELECTION_MANIFEST_FILE);

        let json = select_report_json(&data, &path, &manifest, SEED_MODE_CONTRACTED)
            .expect("report serializes");

        for needle in [
            "\"semantic_hash\"",
            "\"ordered_examples\"",
            "\"label\"",
            "\"access_ledger\"",
            "\"reduced_pools\"",
            "\"excluded_train_ids\"",
            "\"seed_mode\"",
            "\"contracted\"",
            "\"shots_per_class\"",
        ] {
            assert!(
                json.contains(needle),
                "the --json report must carry {needle}"
            );
        }
        assert!(json.contains(&manifest.semantic_hash));
        assert!(json.contains(&manifest.payload.ordered_examples[0].id));
    }

    // --------------------------------------------------------------------------------
    // Rejections
    // --------------------------------------------------------------------------------

    #[test]
    fn nine_shots_is_refused_before_the_filesystem_is_touched() {
        let temp = TempDir::new().expect("tempdir");
        // A directory that does NOT exist. If the shot count were validated after ingest
        // this test would report a missing-manifest error instead, so the assertion below
        // is about ORDER as well as about the message.
        let absent = temp.path().join("no-such-directory");

        let error = run_select(&absent, 9, 13, false, None, false, false)
            .expect_err("nine shots is not a contracted few-shot size");
        let text = message(&error);
        assert!(text.contains("8"), "{text}");
        assert!(text.contains("16"), "{text}");
        assert!(text.contains("32"), "{text}");
        assert!(text.contains("64"), "{text}");
        assert!(text.contains('9'), "the offending value is named: {text}");
        assert!(
            !text.contains("benchmark-manifest"),
            "the shot count is checked BEFORE anything is read: {text}"
        );
    }

    #[test]
    fn an_uncontracted_seed_names_the_ten_seeds_and_the_escape_hatch() {
        let temp = TempDir::new().expect("tempdir");
        let absent = temp.path().join("no-such-directory");

        let error = run_select(&absent, 8, 42, false, None, false, false)
            .expect_err("42 is not one of the ten contracted benchmark seeds");
        let text = message(&error);
        assert!(text.contains("42"), "{text}");
        assert!(text.contains("--any-seed"), "{text}");
        for seed in [13, 17, 23, 29, 31, 37, 41, 43, 47, 53] {
            assert!(
                text.contains(&seed.to_string()),
                "seed {seed} named in: {text}"
            );
        }
        assert!(
            !text.contains("benchmark-manifest"),
            "the seed policy is checked BEFORE anything is read: {text}"
        );
    }

    #[test]
    fn any_seed_accepts_an_uncontracted_seed_and_the_manifest_records_it() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());

        run_select(&data, 8, 42, true, None, false, false).expect("--any-seed accepts seed 42");

        let manifest = read_manifest(&data);
        assert_eq!(
            manifest.payload.root_seed, 42,
            "the seed itself is recorded, so the mode is a total function of the manifest"
        );
        let json = select_report_json(
            &data,
            &data.join(SELECTION_MANIFEST_FILE),
            &manifest,
            SEED_MODE_UNCONTRACTED,
        )
        .expect("report serializes");
        assert!(json.contains(SEED_MODE_UNCONTRACTED), "{json}");
    }

    #[test]
    fn a_compatibility_profile_directory_is_a_profile_mismatch() {
        let temp = TempDir::new().expect("tempdir");
        let data = compatibility_dir(temp.path());

        let error = run_select(&data, 8, 13, false, None, false, false)
            .expect_err("a setfit compatibility directory may never be selected from");
        let text = message(&error);
        assert!(text.contains("profile"), "{text}");
        // The attestation's profile tag is the CRATE's `Compatibility::PROFILE`
        // ("compatibility"), not the CLI's `--profile setfit` spelling. Asserting the
        // crate's word is the point: it is the value the boundary actually compared.
        assert!(text.contains("canonical"), "{text}");
        assert!(text.contains("compatibility"), "{text}");
    }

    #[test]
    fn a_mixed_directory_is_a_split_hash_mismatch_naming_the_split() {
        let temp = TempDir::new().expect("tempdir");
        let data = mixed_dir(temp.path());

        let error = run_select(&data, 8, 13, false, None, false, false)
            .expect_err("a directory assembled from two preparations is refused");
        let text = message(&error);
        assert!(text.contains("validation"), "the split is named: {text}");
        assert!(text.contains("hash") || text.contains("digest"), "{text}");
    }

    #[test]
    fn a_schema_version_one_directory_names_the_supported_set_and_the_remedy() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());
        edit_manifest(&data, |value| {
            // `Value::from` rather than the json! macro: this plan's acceptance grep
            // requires the file to contain no `json!`, and a needle that matches a test
            // helper is the self-needle problem 02-08 had to fix twice.
            value["schema_version"] = serde_json::Value::from(1_u64);
        });

        let error = run_select(&data, 8, 13, false, None, false, false)
            .expect_err("a version-1 benchmark directory has no migration");
        let text = message(&error);
        assert!(text.contains('1'), "{text}");
        assert!(text.contains('2'), "the supported set is named: {text}");
        assert!(
            text.contains("apr data tweet-eval-stance"),
            "the remedy is actionable: {text}"
        );
    }

    #[test]
    fn a_tampered_fingerprint_is_a_fingerprint_mismatch() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());
        edit_manifest(&data, |value| {
            value["dataset_attestation"]["dataset_fingerprint"] =
                serde_json::Value::String("0".repeat(64));
        });

        let error = run_select(&data, 8, 13, false, None, false, false)
            .expect_err("an edited fingerprint is refused");
        let text = message(&error);
        assert!(text.contains("fingerprint"), "{text}");
    }

    #[test]
    fn a_tampered_exclusion_hash_is_an_exclusion_record_mismatch() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());
        edit_manifest(&data, |value| {
            value["dataset_attestation"]["exclusion_hash"] =
                serde_json::Value::String("f".repeat(64));
        });

        let error = run_select(&data, 8, 13, false, None, false, false)
            .expect_err("an edited exclusion digest is refused");
        let text = message(&error);
        assert!(text.contains("exclusion"), "{text}");
    }

    #[test]
    fn a_missing_data_directory_names_the_file_it_could_not_find() {
        let temp = TempDir::new().expect("tempdir");
        let absent = temp.path().join("no-such-directory");

        let error = run_select(&absent, 8, 13, false, None, false, false)
            .expect_err("there is nothing to select from");
        let text = message(&error);
        assert!(text.contains("benchmark-manifest.json"), "{text}");
        assert!(text.contains("no-such-directory"), "{text}");
        assert!(
            text.contains("apr data tweet-eval-stance"),
            "an actionable message points at the command that creates one: {text}"
        );
    }

    // --------------------------------------------------------------------------------
    // Write safety — proven here once, for the helper every artifact goes through
    // --------------------------------------------------------------------------------

    #[test]
    fn an_existing_manifest_is_not_replaced_without_force_and_stays_byte_identical() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());
        run_select(&data, 8, 13, false, None, false, false).expect("first select");
        let before = fs::read(data.join(SELECTION_MANIFEST_FILE)).expect("manifest readable");

        let error = run_select(&data, 16, 17, false, None, false, false)
            .expect_err("no-clobber is the default");
        let text = message(&error);
        assert!(text.contains(SELECTION_MANIFEST_FILE), "{text}");
        assert!(text.contains("--force"), "{text}");

        let after = fs::read(data.join(SELECTION_MANIFEST_FILE)).expect("manifest readable");
        assert_eq!(before, after, "the refused write changed nothing");
    }

    #[test]
    fn force_replaces_the_manifest_completely_rather_than_overwriting_a_prefix() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());
        // Longer than any manifest, so a prefix-only write would leave trailing garbage
        // and the round trip below would fail.
        let filler = vec![b'x'; 4 * 1024 * 1024];
        fs::write(data.join(SELECTION_MANIFEST_FILE), &filler).expect("filler written");

        run_select(&data, 8, 13, false, None, true, false).expect("--force replaces it");

        let bytes = fs::read(data.join(SELECTION_MANIFEST_FILE)).expect("manifest readable");
        assert!(
            bytes.len() < filler.len(),
            "the file was replaced, not patched"
        );
        SelectionManifest::from_bytes(&bytes).expect("the replacement is a complete manifest");
    }

    #[test]
    fn an_induced_mid_write_failure_leaves_no_partial_artifact_and_no_temp_file() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());
        let before: Vec<PathBuf> = fs::read_dir(&data)
            .expect("data dir")
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .collect();

        FAIL_BEFORE_RENAME.with(|flag| flag.set(true));
        let error = run_select(&data, 8, 13, false, None, false, false)
            .expect_err("the induced failure must surface");
        FAIL_BEFORE_RENAME.with(|flag| flag.set(false));
        assert!(message(&error).contains("induced"), "{}", message(&error));

        assert!(
            !data.join(SELECTION_MANIFEST_FILE).exists(),
            "no partial manifest survives"
        );
        let after: Vec<PathBuf> = fs::read_dir(&data)
            .expect("data dir")
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .collect();
        assert_eq!(
            before.len(),
            after.len(),
            "no temp file was left behind: {after:?}"
        );
    }

    #[test]
    fn an_induced_mid_write_failure_leaves_a_pre_existing_manifest_byte_unchanged() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());
        run_select(&data, 8, 13, false, None, false, false).expect("first select");
        let before = fs::read(data.join(SELECTION_MANIFEST_FILE)).expect("manifest readable");
        let entries_before = fs::read_dir(&data).expect("data dir").count();

        FAIL_BEFORE_RENAME.with(|flag| flag.set(true));
        let error = run_select(&data, 16, 17, false, None, true, false)
            .expect_err("the induced failure must surface even under --force");
        FAIL_BEFORE_RENAME.with(|flag| flag.set(false));
        assert!(message(&error).contains("induced"), "{}", message(&error));

        let after = fs::read(data.join(SELECTION_MANIFEST_FILE)).expect("manifest readable");
        assert_eq!(before, after, "--force did not truncate the existing file");
        assert_eq!(
            entries_before,
            fs::read_dir(&data).expect("data dir").count(),
            "no temp file was left behind"
        );
    }

    // --------------------------------------------------------------------------------
    // Structural: the adapter must not reimplement the crate
    // --------------------------------------------------------------------------------

    #[test]
    fn the_selected_ids_are_the_crates_own_and_not_a_cli_reordering() {
        let temp = TempDir::new().expect("tempdir");
        let data = canonical_dir(temp.path());
        run_select(&data, 8, 13, false, None, false, false).expect("select succeeds");
        let manifest = read_manifest(&data);

        // The mirror: rebuild the selection through the crate directly and require the
        // written manifest to agree exactly. Without this, "a manifest was written" would
        // be satisfied by a CLI that selected rows itself.
        let attestation_bytes = data_tweeteval::attestation_bytes_from_manifest(
            &fs::read(data.join(MANIFEST_FILE)).expect("benchmark manifest"),
        )
        .expect("attestation bytes");
        let mut buffers = std::collections::BTreeMap::new();
        for role in ["train", "validation", "test"] {
            buffers.insert(
                role.to_string(),
                fs::read(data.join(format!("{role}.jsonl"))).expect("split readable"),
            );
        }
        let mut ledger = aprender_contrastive_data::ledger::AccessLedger::new();
        let dataset = aprender_contrastive_data::prepared::PreparedDataset::<
            aprender_contrastive_data::prepared::Canonical,
        >::from_attested_bytes(&attestation_bytes, &buffers, &mut ledger)
        .expect("the fixture directory is attested");
        let selection = aprender_contrastive_data::select::FewShotSelector::select(
            &dataset,
            &aprender_contrastive_data::select::SelectionConfig {
                root_seed: 13,
                shots_per_class: 8,
            },
            &mut ledger,
        )
        .expect("selection succeeds");

        let written: Vec<&str> = manifest
            .payload
            .ordered_examples
            .iter()
            .map(|row| row.id.as_str())
            .collect();
        assert_eq!(written, selection.ordered_ids());
    }
}
