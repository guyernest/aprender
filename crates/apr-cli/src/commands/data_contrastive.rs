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
//! # Task 2 RED state
//!
//! `run_select`'s tests are written; its body, the attested-ingest helper and the shared
//! atomic writer land in the GREEN commit. `run_pairs` is still Task 1's placeholder.
//! The two std placeholder macros are deliberately NOT used — both abort the process, and
//! panicking macros are banned by repo policy — so each body returns a structured
//! `CliError` instead. Naming them literally here would also trip this plan's own
//! acceptance grep, which is a self-needle of the kind plan 02-08 had to fix twice.

use crate::error::{CliError, Result};
use aprender_contrastive_data::dedup::ExclusionRecord;
use aprender_contrastive_data::ledger::AccessRecord;
use aprender_contrastive_data::manifest::{SelectedExampleRecord, SelectionManifest};
use serde::Serialize;
use std::path::Path;

/// The file `apr data select` writes and `apr data pairs` reads.
pub(crate) const SELECTION_MANIFEST_FILE: &str = "selection-manifest.json";

/// Wire name for a seed drawn from the ten contracted benchmark seeds.
const SEED_MODE_CONTRACTED: &str = "contracted";

/// Wire name for a seed accepted only because `--any-seed` was passed.
const SEED_MODE_UNCONTRACTED: &str = "uncontracted";

// ==========================================================================================
// Test-only fault-injection seam
// ==========================================================================================

/// When set, `atomic_write` fails AFTER the temp file is written and synced but BEFORE the
/// rename — the one window in which a partial artifact could exist.
///
/// A `thread_local` rather than a global: cargo runs each test on its own thread, so two
/// tests cannot see each other's injection.
#[cfg(test)]
thread_local! {
    static FAIL_BEFORE_RENAME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether an induced pre-rename failure is armed on this thread.
// TASK 2 RED: `atomic_write` does not exist yet, so nothing calls this. The allow is
// removed in the GREEN commit; if it survives, the writer is not using the seam.
#[allow(dead_code)]
#[cfg(test)]
fn induced_prerename_failure() -> bool {
    FAIL_BEFORE_RENAME.with(std::cell::Cell::get)
}

/// Production builds have no seam at all.
#[allow(dead_code)]
#[cfg(not(test))]
const fn induced_prerename_failure() -> bool {
    false
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
// TASK 2 RED: called only by tests until `run_select` lands in the GREEN commit.
#[allow(dead_code)]
fn select_report_json(
    data_dir: &Path,
    manifest_path: &Path,
    manifest: &SelectionManifest,
    seed_mode: &'static str,
) -> Result<String> {
    let _ = (data_dir, manifest_path, manifest, seed_mode);
    Err(CliError::ValidationFailed(
        "select_report_json is not yet implemented (plan 02-09 Task 2)".to_string(),
    ))
}

// ==========================================================================================
// Commands
// ==========================================================================================

/// Select `shots` examples per class from an attested canonical benchmark directory and
/// write the replayable selection manifest.
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
    let _ = (data, shots, seed, any_seed, output, force, json_output);
    Err(CliError::ValidationFailed(
        "apr data select is not yet implemented (plan 02-09 Task 2)".to_string(),
    ))
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
        assert!(text.contains("canonical"), "{text}");
        assert!(text.contains("setfit"), "{text}");
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
            value["schema_version"] = serde_json::json!(1);
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
            value["dataset_attestation"]["dataset_fingerprint"] = serde_json::json!("0".repeat(64));
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
            value["dataset_attestation"]["exclusion_hash"] = serde_json::json!("f".repeat(64));
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
