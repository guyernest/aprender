//! `apr setfit train` — the user surface of the SetFit training lifecycle (D-05).
//!
//! # This file is a FILESYSTEM ADAPTER and nothing else
//!
//! It reads a configuration file, hands the bytes to `aprender-train`'s single
//! validating constructor, replays Phase 2's artifacts through the doors
//! `data_contrastive` already owns, drives the shipped lifecycle transitions, and
//! writes the bytes the library hands back. Nothing here decides a knob's legality,
//! samples a pair, tunes an encoder, fits a head or judges a verification — every
//! one of those lives behind `entrenar::train::setfit`'s public API.
//!
//! # Fail on the REQUEST before blaming the data
//!
//! The order of the first four steps is deliberate and is asserted by tests:
//! parse the config, merge the overrides, resolve the device, refuse an existing
//! output — all four BEFORE a single row of `--data` is read and long before the
//! encoder is loaded. A bad seed is not a problem with the dataset, and reporting it
//! as one sends the operator to the wrong place. Refusing the output path last of
//! the four is what stops a twenty-minute run from ending in "I will not overwrite
//! that file".
//!
//! # The override merge goes through the PUBLIC validated door
//!
//! `--seed` and `--device` are applied to the REQUEST that `SetFitTrainConfig::to_request`
//! hands back, and the result goes through `SetFitTrainConfig::new`. The merged
//! configuration is therefore validated AS A WHOLE by the same single implementation
//! that validated the file. Mutating an already-validated config would let an invalid
//! merge through; deserializing into the library's private wire struct is not possible
//! from this crate at all, which is the review finding plan 04-14 closed by adding
//! `to_request`.
//!
//! # Every artifact goes through ONE writer
//!
//! [`atomic_write`] is the only function here that creates a file: temp file in the
//! DESTINATION directory, `write_all`, `sync_all`, `rename`, no-clobber unless
//! `--force`, and temp cleanup on every error path. There is exactly one `fs::rename`
//! site and no `File::create` outside it, and both facts are asserted on this file's
//! own source.
//!
//! # Not the network
//!
//! No `--offline` parameter, for the reason `dispatch_setfit_command` records: this
//! command opens no socket. `--model-dir` names a checkout the operator obtained
//! beforehand.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use aprender::setfit::SetFitMiniLm;
use aprender_contrastive_data::hash::hex;
use aprender_contrastive_data::ledger::AccessLedger;
use aprender_contrastive_data::select::Selection;
use colored::Colorize;
use entrenar::train::device::{resolve_device, Device, DeviceError};
use entrenar::train::setfit::apr_codec::AprCodec;
use entrenar::train::setfit::config::SetFitTrainConfig;
use entrenar::train::setfit::{SetFitRun, SetFitTrainError};
use serde::Serialize;

use crate::commands::data_contrastive;
use crate::error::{CliError, Result};
use crate::output;

/// The two configuration encodings this command reads, for error messages.
const ACCEPTED_CONFIG_EXTENSIONS: &str = "`.toml` or `.json`";

/// What to do about a missing or unusable `--model-dir`.
///
/// The library cannot name a CLI flag, and it cannot know that this command never
/// downloads. Both facts belong in the adapter's message.
const MODEL_DIR_REMEDY: &str = "Point --model-dir at a pinned all-MiniLM-L6-v2 checkout \
     containing tokenizer.json and the encoder weights. This command NEVER downloads: \
     obtain the pinned revision beforehand (for example with `batuta hf pull`) and pass \
     the directory.";

// ==========================================================================================
// KNOWN GAP: the verified artifact's bytes are not reachable from outside aprender-train
// ==========================================================================================

/// Why this command cannot yet write its output, stated once, in full.
///
/// This is the only step of the plan's seven that the shipped library cannot serve, and
/// it is surfaced as a typed refusal rather than worked around. The two ways around it
/// are both worse than the gap:
///
/// * **Re-serialize here.** `SetFitBundle::from_run_parts` is `pub(crate)`, so an
///   adapter cannot obtain a bundle to hand to `AprCodec::serialize`. Assembling a
///   `SetFitArtifactView` from the run's rebuilt encoder and head instead would be a
///   SECOND implementation of the bundle -> artifact mapping that 04-05 proved field by
///   field — and the bytes it produced would not be the bytes that were verified, which
///   is precisely the "the served model is the evaluated model" claim this phase exists
///   to make true.
/// * **Widen the library here.** The door belongs in
///   `crates/aprender-train/src/train/setfit/`, which this plan does not own: the wave-5
///   ownership contract splits `apr-cli` (this plan) from `aprender-train` (04-12 and
///   04-16, both running concurrently).
///
/// 04-12's plan states the phase's rule for exactly this situation — "if a public-API gap
/// blocks the test, STOP and surface; that gap is itself an OPS-01 finding, not something
/// to patch around by editing core or train sources here."
const ARTIFACT_BYTES_GAP: &str = "the verified artifact's BYTES are not reachable through \
     aprender-train's public API. `SetFitRun::<HeadFitted>::verify_artifact` returns a \
     verified RUN, and the trusted policy behind it drops the artifact bytes internally \
     (keeping only their length, deliberately, so ~180 MB is not held alongside the rebuilt \
     model). Writing the file needs one new public door in \
     crates/aprender-train/src/train/setfit/ — for example \
     `SetFitRun::<ArtifactReloadedAndVerified>::into_artifact_bytes(self) -> Vec<u8>`. \
     Re-serializing in the CLI is NOT an acceptable substitute: it would be a second \
     implementation of the bundle-to-artifact mapping, and the bytes written would not be \
     the bytes that were verified.";

/// Take the verified artifact's bytes.
///
/// Every value this function needs already exists inside the verified run; the library
/// simply has no accessor that hands the bytes back. The signature is written the way it
/// will be written once the door lands, so closing the gap is a one-line change here.
///
/// # Errors
///
/// Always, today: [`CliError::Aprender`] carrying [`ARTIFACT_BYTES_GAP`].
fn verified_artifact_bytes(run: &SetFitRun<VerifiedState>) -> Result<Vec<u8>> {
    // The hash IS reachable, which is what makes the gap precise rather than vague: the
    // run can tell a caller what the artifact's digest is and cannot hand over the bytes
    // that digest was taken over.
    let _recorded_hash = run.artifact_hash();
    Err(CliError::Aprender(format!(
        "trained and verified successfully, but the artifact could not be written: \
         {ARTIFACT_BYTES_GAP}"
    )))
}

/// The lifecycle state `verify_artifact` mints, named once so the signature above reads.
type VerifiedState = entrenar::train::setfit::ArtifactReloadedAndVerified;

// ==========================================================================================
// Test-only fault-injection seam (the shape `data_contrastive` established)
// ==========================================================================================

// When set, `atomic_write` fails AFTER the temp file is written and synced but BEFORE the
// rename — the one window in which a partial artifact could exist.
//
// A `thread_local` rather than a global: cargo runs each test on its own thread, so two
// tests cannot see each other's injection.
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
// The single writer
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
fn fill_and_sync(temp: &Path, bytes: &[u8]) -> Result<()> {
    // `create_new` on the TEMP as well: two concurrent runs must not share one scratch
    // file, and a leftover scratch file from a crashed run is a diagnosable error rather
    // than silent reuse.
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

/// Produce `target` atomically, with no-clobber by default.
///
/// The temp file lives in the destination directory because `rename` is only atomic
/// within one filesystem; a temp in `/tmp` would silently degrade to a copy across a
/// mount point, which is exactly the partial-write window this exists to close.
///
/// Every failure path removes the temp, so an interrupted write leaves neither a partial
/// artifact nor a stray file for the next `--force`-less run to trip over — and the
/// cleanup's own failure never replaces the reason the write failed.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] when `target` exists and `force` is false;
/// [`CliError::Io`] for any create, write, sync or rename failure.
fn atomic_write(target: &Path, bytes: &[u8], force: bool) -> Result<()> {
    refuse_existing_output(target, force)?;
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;

    let temp = temp_path(target);
    let result =
        fill_and_sync(&temp, bytes).and_then(|()| fs::rename(&temp, target).map_err(CliError::Io));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// The no-clobber gate, callable on its own so it can run BEFORE training.
///
/// [`atomic_write`] calls it too, and that is not redundancy: the pre-flight closes the
/// window in which a long run ends by refusing to write, while the write-time check is
/// what makes the guarantee true for a file that appeared while the run was going.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] naming the path and `--force`.
fn refuse_existing_output(target: &Path, force: bool) -> Result<()> {
    if !force && target.exists() {
        return Err(CliError::ValidationFailed(format!(
            "Refusing to replace existing file {} (pass --force to replace it)",
            target.display()
        )));
    }
    Ok(())
}

// ==========================================================================================
// Configuration: file first, then a validated merge
// ==========================================================================================

/// Parse `--config` by extension, straight through the library's validating deserializer.
///
/// Deserialization IS validation here: `SetFitTrainConfig` carries
/// `#[serde(try_from = ...)]` onto its single constructor, so an unknown key is refused
/// by `deny_unknown_fields` and an invalid VALUE is refused by the knob table — both
/// before this function returns, and therefore before anything is read from `--data`.
///
/// # Errors
///
/// [`CliError::InvalidFormat`] for an extension this command does not read;
/// [`CliError::ValidationFailed`] for any parse or knob rejection, prefixed so the
/// message says which file the operator has to edit.
fn parse_config(path: &Path) -> Result<SetFitTrainConfig> {
    let bytes = fs::read_to_string(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::FileNotFound(path.to_path_buf())
        } else {
            CliError::Io(error)
        }
    })?;

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);

    match extension.as_deref() {
        Some("toml") => toml::from_str(&bytes).map_err(|error| config_error(path, &error)),
        Some("json") => serde_json::from_str(&bytes).map_err(|error| config_error(path, &error)),
        // Sniffing the content instead would make the same file mean different things on
        // different days. The extension is the declaration.
        other => Err(CliError::InvalidFormat(format!(
            "--config {}: unsupported extension {}; expected {ACCEPTED_CONFIG_EXTENSIONS}",
            path.display(),
            other.map_or_else(|| "<none>".to_string(), |ext| format!("`.{ext}`")),
        ))),
    }
}

/// One rendering for both deserializers, so a TOML and a JSON rejection read alike.
fn config_error(path: &Path, error: &impl std::fmt::Display) -> CliError {
    CliError::ValidationFailed(format!("--config {}: {error}", path.display()))
}

/// Apply `--seed` / `--device` through the PUBLIC validated merge door.
///
/// Read the twelve knobs back out as a REQUEST, override at most two of them, and send
/// the whole thing back through the single validating constructor. The order matters:
/// the merged result is validated AS A WHOLE, so an override cannot arrive at a value the
/// file itself would have been refused for. Overriding a field on the already-validated
/// config would be the opposite — the value would never meet the knob table, and the
/// type's promise that every value of it has been validated for its whole life would
/// become false.
///
/// Note that `SetFitTrainConfig::new` normalizes `pair_config.root_seed` to the top-level
/// seed, so `--seed` reseeds the pair stream too. That is the documented behaviour of the
/// door, not an accident of this call site.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] naming the flag whose value the merged configuration
/// was refused for.
fn merge_overrides(
    config: &SetFitTrainConfig,
    seed: Option<u64>,
    device: Option<&str>,
) -> Result<SetFitTrainConfig> {
    let mut request = config.to_request();
    if let Some(seed) = seed {
        request.root_seed = seed;
    }
    if let Some(device) = device {
        request.device = device.to_string();
    }
    SetFitTrainConfig::new(request).map_err(|error| {
        CliError::ValidationFailed(format!(
            "the merged configuration was refused: {error} — check --config together with \
             {}",
            overridden_flags(seed, device)
        ))
    })
}

/// Name the flags that participated in the merge, so the message points somewhere.
fn overridden_flags(seed: Option<u64>, device: Option<&str>) -> String {
    let mut flags = Vec::new();
    if seed.is_some() {
        flags.push("--seed");
    }
    if device.is_some() {
        flags.push("--device");
    }
    if flags.is_empty() {
        return "no overrides (the file alone was refused)".to_string();
    }
    flags.join(" and ")
}

/// Probe the host for the MERGED device request, failing closed (OPS-06).
///
/// This runs before any data is read and long before the encoder is loaded: an explicit
/// device this host cannot provide is a fact about the request, and discovering it after
/// twenty minutes of training would be useless. `resolve_device` is the single definition
/// of both the grammar and the no-silent-fallback rule; this only translates the refusal
/// and appends the flag the library cannot name.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] for an unavailable or unparseable device — nonzero exit
/// code 5, never a silent fall back to CPU.
fn resolve_requested_device(config: &SetFitTrainConfig) -> Result<Device> {
    let requested = config.device().as_str();
    resolve_device(requested).map_err(|error| {
        let remedy = match error {
            DeviceError::CudaNotAvailable { .. } => {
                " — this host has no usable CUDA runtime. Pass --device cpu to opt in to the \
                 CPU path explicitly; there is deliberately no silent fallback, because a \
                 benchmark number produced on a device nobody asked for is worse than no \
                 number"
            }
            DeviceError::InvalidSpec(_) => {
                " — set `device` in --config, or pass --device, to one of cpu, cuda, cuda:N \
                 (N in 0..=15) or auto"
            }
        };
        CliError::ValidationFailed(format!("device: {error}{remedy}"))
    })
}

// ==========================================================================================
// Phase 2 ingest — through the doors `apr data` already owns
// ==========================================================================================

/// What the selection says about where this run's data came from.
///
/// Captured BEFORE the selection is moved into the lifecycle, because the report needs it
/// on both the dry-run path (where no run exists) and the training path.
#[derive(Serialize)]
struct Provenance {
    dataset_fingerprint: String,
    validation_split_fingerprint: String,
    selection_semantic_hash: String,
    selection_ledger_hash: String,
    selection_root_seed: u64,
    shots_per_class: u32,
}

impl Provenance {
    /// Read every value off the replayed selection. Nothing here is recomputed.
    fn of(selection: &Selection) -> Self {
        Self {
            dataset_fingerprint: selection.dataset_fingerprint_hex().to_string(),
            validation_split_fingerprint: selection.validation_fingerprint_hex().to_string(),
            selection_semantic_hash: hex(&selection.semantic_hash()),
            selection_ledger_hash: hex(&selection.ledger_hash()),
            selection_root_seed: selection.root_seed(),
            shots_per_class: selection.shots_per_class(),
        }
    }
}

/// Everything the two Phase 2 artifacts produce, replayed against each other.
struct Phase2Inputs {
    dataset: aprender_contrastive_data::prepared::PreparedDataset<
        aprender_contrastive_data::prepared::Canonical,
    >,
    selection: Selection,
    provenance: Provenance,
}

/// Read `--data` and `--selection` through the attested doors, strictly.
///
/// Both doors belong to `data_contrastive`: one reader of `benchmark-manifest.json` and
/// one reader of `selection-manifest.json` for the whole CLI. `Selection::replay` is the
/// only route from manifest bytes back to a `Selection` — this command cannot construct
/// one, because the crate's constructor is private.
///
/// # Errors
///
/// [`CliError::ValidationFailed`] for any attested-ingest or replay rejection;
/// [`CliError::Io`] for a read failure.
fn read_phase2_inputs(data: &Path, selection_path: &Path) -> Result<Phase2Inputs> {
    let mut ledger = AccessLedger::new();
    let dataset = data_contrastive::read_attested_canonical(data, &mut ledger)?;
    let manifest = data_contrastive::read_selection_manifest(selection_path)?;
    let selection = Selection::replay(&manifest, &dataset, &mut ledger).map_err(|error| {
        CliError::ValidationFailed(format!(
            "selection {} does not replay against --data {}: {error}",
            selection_path.display(),
            data.display()
        ))
    })?;
    let provenance = Provenance::of(&selection);
    Ok(Phase2Inputs {
        dataset,
        selection,
        provenance,
    })
}

/// Map a lifecycle failure onto the CLI surface, adding the knob the library cannot name.
fn train_error(error: &SetFitTrainError) -> CliError {
    let remedy = match error {
        SetFitTrainError::UncalibratedRegime { .. } => {
            " — the thresholds this gate applies were MEASURED in another regime, and an \
             epsilon measured on one architecture is not evidence about another. Widening the \
             calibrated set is a deliberate contract edit (Phase 3 D-10(c)), never an inline \
             change to unblock a run"
        }
        SetFitTrainError::UnsupportedDeviceForPhase3 { .. } => {
            " — pass --device cpu, or set `device` in --config"
        }
        SetFitTrainError::SelectionRowMissing { .. }
        | SetFitTrainError::SelectionRowContentMismatch { .. } => {
            " — --selection and --data disagree about the rows. Re-run `apr data select` \
             against THIS dataset directory"
        }
        _ => "",
    };
    CliError::ValidationFailed(format!("setfit training: {error}{remedy}"))
}

// ==========================================================================================
// Reports
// ==========================================================================================

/// The MERGED configuration, exactly as validated, plus the probe's answer.
///
/// This is what makes a Phase 5 cell reproducible from the report alone (D-07): the twelve
/// knobs printed here are the merged ones, not the file's, so a reader never has to know
/// which flags were passed to reconstruct the run.
#[derive(Serialize)]
struct ResolvedConfigReport<'a> {
    requested: &'a SetFitTrainConfig,
    resolved_device: String,
}

/// `apr setfit train --json`.
#[derive(Serialize)]
struct TrainReport<'a> {
    command: &'static str,
    output: String,
    artifact_sha256: String,
    artifact_format_id: &'a str,
    evidence_table_hash: &'a str,
    provenance: &'a Provenance,
    resolved: ResolvedConfigReport<'a>,
}

/// `apr setfit train --dry-run [--json]`.
///
/// The `checks` list is explicit about what a dry run does NOT do. A pre-flight that lets
/// a reader assume it validated the encoder would be worse than no pre-flight.
#[derive(Serialize)]
struct DryRunReport<'a> {
    command: &'static str,
    dry_run: bool,
    output: String,
    model_dir: String,
    provenance: &'a Provenance,
    resolved: ResolvedConfigReport<'a>,
    checks_performed: [&'static str; 4],
    checks_skipped: [&'static str; 3],
}

/// The four things a dry run really does check.
const DRY_RUN_PERFORMED: [&str; 4] = [
    "config parsed and validated as a whole, including --seed/--device overrides",
    "device resolved on this host (no silent fallback)",
    "--output refused if it exists without --force",
    "--data and --selection replayed strictly against each other",
];

/// And the three it does not.
const DRY_RUN_SKIPPED: [&str; 3] = [
    "--model-dir was NOT opened (a multi-hundred-megabyte read is not a pre-flight)",
    "no encoder tuning, head fit or artifact verification ran",
    "nothing was written",
];

/// Human-readable dry-run output.
fn render_dry_run_human(report: &DryRunReport) {
    output::section("SetFit Training — Dry Run");
    println!();
    output::kv("Output", &report.output);
    output::kv("Model dir", &report.model_dir);
    output::kv("Device", &report.resolved.resolved_device);
    output::kv("Root seed", report.resolved.requested.root_seed());
    output::kv("Epochs", report.resolved.requested.epochs());
    output::kv("Batch size", report.resolved.requested.batch_size());
    output::kv("Encoder LR", report.resolved.requested.encoder_lr());
    output::kv(
        "Dataset fingerprint",
        &report.provenance.dataset_fingerprint,
    );
    output::kv(
        "Selection",
        format!(
            "{} ({} shot(s) per class, seed {})",
            report.provenance.selection_semantic_hash,
            report.provenance.shots_per_class,
            report.provenance.selection_root_seed
        ),
    );
    println!();
    println!("{} Checked:", "OK".green());
    for line in report.checks_performed {
        println!("  - {line}");
    }
    println!();
    println!("Not checked by a dry run:");
    for line in report.checks_skipped {
        println!("  - {line}");
    }
}

/// Human-readable completion output.
fn render_train_human(report: &TrainReport) {
    output::section("SetFit Training");
    println!();
    output::kv("Artifact", &report.output);
    output::kv("Format", report.artifact_format_id);
    output::kv("SHA-256", &report.artifact_sha256);
    output::kv("Evidence table", report.evidence_table_hash);
    output::kv("Device", &report.resolved.resolved_device);
    output::kv("Root seed", report.resolved.requested.root_seed());
    output::kv(
        "Dataset fingerprint",
        &report.provenance.dataset_fingerprint,
    );
    println!();
    println!(
        "{} The written artifact was reloaded from its own bytes and re-predicted before \
         this line printed.",
        "OK".green()
    );
}

/// Serialize a report, or say why it could not be serialized.
fn report_json(report: &impl Serialize) -> Result<String> {
    serde_json::to_string_pretty(report).map_err(|error| {
        CliError::ValidationFailed(format!("Failed to encode the training report: {error}"))
    })
}

// ==========================================================================================
// The command
// ==========================================================================================

/// Train a SetFit classifier from Phase 2 artifacts and write a verified APR.
///
/// See the module header for the ordering rules this body implements.
///
/// # Errors
///
/// [`CliError::FileNotFound`] for an absent `--config`; [`CliError::InvalidFormat`] for a
/// config extension this command does not read; [`CliError::ValidationFailed`] for any
/// knob, merge, device, ingest, replay or lifecycle rejection, and for an existing
/// `--output` without `--force`; [`CliError::ModelLoadFailed`] for an unusable
/// `--model-dir`; [`CliError::Io`] for a read or write failure.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    config_path: &Path,
    data: &Path,
    selection_path: &Path,
    model_dir: &Path,
    output_path: &Path,
    seed: Option<u64>,
    device: Option<&str>,
    force: bool,
    dry_run: bool,
    json_output: bool,
) -> Result<()> {
    // (1) The REQUEST, in full, before anything is read from --data.
    let file_config = parse_config(config_path)?;
    let merged = merge_overrides(&file_config, seed, device)?;
    let resolved_device = resolve_requested_device(&merged)?;
    // (2) And the one filesystem fact that is also a property of the request.
    refuse_existing_output(output_path, force)?;

    // (3) Phase 2's artifacts, replayed strictly against each other.
    let inputs = read_phase2_inputs(data, selection_path)?;

    let resolved = ResolvedConfigReport {
        requested: &merged,
        resolved_device: resolved_device.tag(),
    };

    if dry_run {
        let report = DryRunReport {
            command: "setfit-train",
            dry_run: true,
            output: output_path.display().to_string(),
            model_dir: model_dir.display().to_string(),
            provenance: &inputs.provenance,
            resolved,
            checks_performed: DRY_RUN_PERFORMED,
            checks_skipped: DRY_RUN_SKIPPED,
        };
        if json_output {
            println!("{}", report_json(&report)?);
        } else {
            render_dry_run_human(&report);
        }
        return Ok(());
    }

    // (4) The encoder. Deliberately after everything above: it is the expensive read.
    let encoder =
        SetFitMiniLm::from_pretrained_dir(model_dir, merged.root_seed()).map_err(|error| {
            CliError::ModelLoadFailed(format!(
                "--model-dir {}: {error}. {MODEL_DIR_REMEDY}",
                model_dir.display()
            ))
        })?;

    // (5) The shipped lifecycle, in the only order the typestate permits. Every gate —
    //     the device probe, the pair budget, the selection/dataset agreement, the
    //     calibration regime, the evidence thresholds, the head fit and the artifact
    //     round trip — is inside these four calls.
    let prepared = SetFitRun::prepare(encoder, inputs.dataset, inputs.selection, merged.clone())
        .map_err(|e| train_error(&e))?;
    let tuned = prepared.tune_encoder().map_err(|e| train_error(&e))?;
    let fitted = tuned.fit_head().map_err(|e| train_error(&e))?;
    let verified = fitted
        .verify_artifact(&AprCodec::new())
        .map_err(|e| train_error(&e))?;

    // (6) The write. One rename site, no-clobber, temp in the destination directory.
    let bytes = verified_artifact_bytes(&verified)?;
    atomic_write(output_path, &bytes, force)?;

    // (7) The report, from the run's own recorded values.
    let report = TrainReport {
        command: "setfit-train",
        output: output_path.display().to_string(),
        artifact_sha256: verified.artifact_hash(),
        artifact_format_id: verified.artifact_format_id(),
        evidence_table_hash: verified.evidence_table_hash(),
        provenance: &inputs.provenance,
        resolved,
    };
    if json_output {
        println!("{}", report_json(&report)?);
    } else {
        render_train_human(&report);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::data_tweeteval::{self, fixtures, CANONICAL_REVISION};
    use crate::TweetEvalStanceProfile;
    use aprender::setfit::MAX_SEQUENCE_LENGTH;
    use std::collections::BTreeSet;
    use tempfile::TempDir;

    /// This module's own source, for the source assertions.
    const SETFIT_TRAIN_SOURCE: &str = include_str!("setfit_train.rs");

    /// Assemble a search needle from fragments at RUNTIME.
    ///
    /// These scans read the file they live in, so a whole literal would appear IN the
    /// scanned source and inflate the count — the self-match hazard 04-14 committed once
    /// inside the very helper that warns about it. This doc comment therefore describes
    /// the needles without spelling any of them.
    fn needle(fragments: &[&str]) -> String {
        fragments.concat()
    }

    /// The contracted benchmark seed every fixture here uses.
    const FIXTURE_SEED: u64 = 13;

    /// A hand-authored TOML config carrying all twelve knobs.
    ///
    /// Hand-authored on purpose: this is the file shape an operator types, and the test
    /// below asserts it lands on exactly the value the library's own
    /// `SetFitTrainConfig::reference_defaults` produces. `budget` and `hard_cap` are
    /// omitted rather than written as nulls, which TOML cannot express — the library's
    /// wire form makes both optional, and the omission is the shape a real file has.
    fn reference_toml(seed: u64) -> String {
        format!(
            "encoder_lr = 2e-5\n\
             epochs = 1\n\
             batch_size = 16\n\
             warmup_ratio = 0.1\n\
             grad_clip_max_norm = 1.0\n\
             max_length = {max_length}\n\
             freeze_policy = []\n\
             root_seed = {seed}\n\
             device = \"cpu\"\n\
             lr_schedule = \"warmup_linear_decay\"\n\
             \n\
             [pair_config]\n\
             strategy = \"oversampling\"\n\
             singleton_policy = \"negatives_only\"\n\
             \n\
             [head_regularization]\n\
             kind = \"sklearn_equivalent_c\"\n\
             c = 1.0\n",
            max_length = MAX_SEQUENCE_LENGTH,
        )
    }

    fn write(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).expect("fixture file is writable");
        path
    }

    /// A config file this command accepts, at the given extension.
    fn config_file(dir: &Path, name: &str) -> PathBuf {
        write(dir, name, &reference_toml(FIXTURE_SEED))
    }

    /// A real Phase 2 benchmark directory plus its selection manifest.
    ///
    /// Both are produced by RUNNING the shipped commands over a synthetic source tree —
    /// `apr data tweet-eval-stance` then `apr data select` — exactly as
    /// `data_contrastive`'s own tests do. Nothing here fabricates an attestation or a
    /// manifest by hand: a hand-built one would only prove this reader accepts what this
    /// test module thinks the writer emits.
    fn phase2_artifacts(root: &Path) -> (PathBuf, PathBuf) {
        let source = root.join("source");
        fs::create_dir_all(&source).expect("fixture source directory is creatable");
        fixtures::write_canonical_fixture_tagged(&source, fixtures::DEFAULT_TAG);
        let data = root.join("benchmark");
        data_tweeteval::run(
            &data,
            TweetEvalStanceProfile::Canonical,
            Some(&source),
            CANONICAL_REVISION,
            false,
            true,
            true,
        )
        .expect("the synthetic canonical fixture prepares cleanly");
        data_contrastive::run_select(&data, 8, FIXTURE_SEED, false, None, false, true)
            .expect("selection succeeds against the prepared fixture");
        let selection = data.join("selection-manifest.json");
        assert!(selection.is_file(), "apr data select wrote its manifest");
        (data, selection)
    }

    /// Every path under `root`, so "nothing was written" is a set comparison rather than
    /// a spot check on the one filename the test happened to think of.
    fn listing(root: &Path) -> BTreeSet<PathBuf> {
        fn walk(dir: &Path, into: &mut BTreeSet<PathBuf>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, into);
                }
                into.insert(path);
            }
        }
        let mut out = BTreeSet::new();
        walk(root, &mut out);
        out
    }

    // --------------------------------------------------------------------------------
    // Configuration: both encodings, and every rejection BEFORE training
    // --------------------------------------------------------------------------------

    #[test]
    fn setfit_train_accepts_a_toml_config() {
        let temp = TempDir::new().expect("tempdir");
        let path = config_file(temp.path(), "train.toml");

        let parsed = parse_config(&path).expect("a well-formed TOML config is accepted");
        assert_eq!(
            parsed,
            SetFitTrainConfig::reference_defaults(FIXTURE_SEED),
            "a hand-authored TOML file must land on exactly the value the library's own \
             reference recipe produces — otherwise the documented file shape and the \
             documented defaults describe two different runs"
        );
    }

    #[test]
    fn setfit_train_accepts_a_json_config() {
        let temp = TempDir::new().expect("tempdir");
        // Derived from the library rather than hand-authored, so this leg proves the JSON
        // round trip through the SAME validating constructor.
        let encoded =
            serde_json::to_string_pretty(&SetFitTrainConfig::reference_defaults(FIXTURE_SEED))
                .expect("the reference config serializes");
        let path = write(temp.path(), "train.json", &encoded);

        let parsed = parse_config(&path).expect("a well-formed JSON config is accepted");
        assert_eq!(parsed, SetFitTrainConfig::reference_defaults(FIXTURE_SEED));
    }

    #[test]
    fn setfit_train_refuses_an_unknown_config_field() {
        let temp = TempDir::new().expect("tempdir");
        let mut contents = reference_toml(FIXTURE_SEED);
        contents.push_str("mystery_knob = 7\n");
        let path = write(temp.path(), "train.toml", &contents);

        let error = parse_config(&path).expect_err("an unknown key must be refused");
        assert!(
            matches!(error, CliError::ValidationFailed(_)),
            "an unknown key is a validation failure (exit 5); got: {error}"
        );
        assert!(
            error.to_string().contains("mystery_knob"),
            "the refusal must name the offending key; got: {error}"
        );
    }

    #[test]
    fn setfit_train_refuses_an_invalid_knob_value() {
        let temp = TempDir::new().expect("tempdir");
        // A structurally perfect file with one value the knob table forbids. This is the
        // case `deny_unknown_fields` cannot catch and the validating constructor must.
        let contents = reference_toml(FIXTURE_SEED).replace("epochs = 1", "epochs = 0");
        let path = write(temp.path(), "train.toml", &contents);

        let error = parse_config(&path).expect_err("a zero epoch count must be refused");
        assert!(
            matches!(error, CliError::ValidationFailed(_)),
            "got: {error}"
        );
        assert!(
            error.to_string().contains("epochs"),
            "the refusal must name the knob, not merely say the config was invalid; got: {error}"
        );
    }

    #[test]
    fn setfit_train_refuses_a_config_extension_it_does_not_read() {
        let temp = TempDir::new().expect("tempdir");
        let path = write(temp.path(), "train.yaml", &reference_toml(FIXTURE_SEED));

        let error = parse_config(&path).expect_err("an unread extension must be refused");
        assert!(
            matches!(error, CliError::InvalidFormat(_)),
            "a wrong extension is a format problem (exit 4), not a validation one; got: {error}"
        );
        assert!(
            error.to_string().contains(".toml") && error.to_string().contains(".json"),
            "the refusal must name what IS accepted; got: {error}"
        );
    }

    // --------------------------------------------------------------------------------
    // The merge, through the public door
    // --------------------------------------------------------------------------------

    #[test]
    fn setfit_train_seed_override_is_reflected_in_the_merged_config() {
        let base = SetFitTrainConfig::reference_defaults(FIXTURE_SEED);
        let merged = merge_overrides(&base, Some(29), None).expect("a contracted seed merges");

        assert_eq!(merged.root_seed(), 29, "the override must reach the config");
        assert_eq!(
            merged.pair_config().root_seed,
            29,
            "the merge door reseeds the pair stream too — a run whose pair sampler kept the \
             file's seed would replay a stream the reported seed does not describe"
        );
        assert_eq!(
            base.root_seed(),
            FIXTURE_SEED,
            "the merge must not mutate its input"
        );
    }

    #[test]
    fn setfit_train_refuses_an_invalid_device_override_at_the_merge() {
        let base = SetFitTrainConfig::reference_defaults(FIXTURE_SEED);
        let error = merge_overrides(&base, None, Some("gpu"))
            .expect_err("a device outside the grammar must be refused by the merge");

        assert!(
            matches!(error, CliError::ValidationFailed(_)),
            "got: {error}"
        );
        let rendered = error.to_string();
        assert!(
            rendered.contains("--device"),
            "the refusal must name the flag that carried the bad value; got: {rendered}"
        );
    }

    #[test]
    fn setfit_train_device_cuda_fails_closed_with_a_nonzero_exit_code() {
        // The grammar accepts `cuda`, so this passes the merge and is refused by the PROBE.
        // Splitting the two is what makes "no silent fallback" testable on a CPU host: the
        // request is well-formed and is still refused.
        let base = SetFitTrainConfig::reference_defaults(FIXTURE_SEED);
        let merged = merge_overrides(&base, None, Some("cuda")).expect("`cuda` parses");

        let error = resolve_requested_device(&merged)
            .expect_err("an explicit CUDA request on a CPU-only host must fail closed");
        let rendered = error.to_string();
        assert!(
            rendered.contains("CUDA"),
            "the refusal must say what was unavailable; got: {rendered}"
        );
        assert!(
            rendered.contains("--device cpu"),
            "and it must name the opt-in, because the absent behaviour is a silent fallback; \
             got: {rendered}"
        );
        assert_ne!(
            format!("{:?}", error.exit_code()),
            format!("{:?}", std::process::ExitCode::SUCCESS),
            "OPS-06 requires a NONZERO exit code, not merely a message on stderr"
        );
    }

    // --------------------------------------------------------------------------------
    // Output safety
    // --------------------------------------------------------------------------------

    #[test]
    fn setfit_train_refuses_an_existing_output_before_it_reads_the_data() {
        let temp = TempDir::new().expect("tempdir");
        let config = config_file(temp.path(), "train.toml");
        let output = write(temp.path(), "model.apr", "previous artifact");

        // --data does not exist. If the output check ran after ingest, THAT is the error
        // this would report, so the assertion below pins the ORDER and not just the check.
        let error = run(
            &config,
            &temp.path().join("absent-data"),
            &temp.path().join("absent-selection.json"),
            &temp.path().join("absent-model-dir"),
            &output,
            None,
            None,
            false,
            false,
            true,
        )
        .expect_err("an existing --output without --force must be refused");

        let rendered = error.to_string();
        assert!(
            rendered.contains("model.apr") && rendered.contains("--force"),
            "the refusal must name the file and the flag; got: {rendered}"
        );
        assert_eq!(
            fs::read_to_string(&output).expect("the existing file is still readable"),
            "previous artifact",
            "a refused run must not have touched the file it refused to replace"
        );
    }

    #[test]
    fn setfit_train_a_failed_write_leaves_no_partial_file() {
        let temp = TempDir::new().expect("tempdir");
        let target = temp.path().join("model.apr");
        let before = listing(temp.path());

        FAIL_BEFORE_RENAME.with(|cell| cell.set(true));
        let error = atomic_write(&target, b"bytes that never land", false)
            .expect_err("the injected pre-rename failure propagates");
        FAIL_BEFORE_RENAME.with(|cell| cell.set(false));

        assert!(matches!(error, CliError::Io(_)), "got: {error}");
        assert!(!target.exists(), "no artifact at the target path");
        assert_eq!(
            listing(temp.path()),
            before,
            "and no temp file left behind either — a stray scratch file would make the \
             next --force-less run fail for the wrong reason"
        );
    }

    // --------------------------------------------------------------------------------
    // The dry run, over REAL Phase 2 artifacts
    // --------------------------------------------------------------------------------

    #[test]
    fn setfit_train_dry_run_validates_the_real_inputs_and_writes_nothing() {
        let temp = TempDir::new().expect("tempdir");
        let (data, selection) = phase2_artifacts(temp.path());
        let config = config_file(temp.path(), "train.toml");
        let output = temp.path().join("model.apr");
        let before = listing(temp.path());

        run(
            &config,
            &data,
            &selection,
            // Never opened on this path, which is the point: a dry run must not cost a
            // multi-hundred-megabyte read. An absent directory proves it was not opened.
            &temp.path().join("absent-model-dir"),
            &output,
            Some(29),
            Some("cpu"),
            false,
            true,
            true,
        )
        .expect("a dry run over real Phase 2 artifacts succeeds");

        assert!(!output.exists(), "a dry run writes no artifact");
        assert_eq!(
            listing(temp.path()),
            before,
            "a dry run writes NOTHING — not the artifact, not a temp file, not a log"
        );
    }

    // --------------------------------------------------------------------------------
    // Source assertions: the shape the review findings require
    // --------------------------------------------------------------------------------

    #[test]
    fn setfit_train_merges_through_the_public_door_and_names_no_wire_type() {
        let door = needle(&["config.to_", "request()"]);
        assert!(
            SETFIT_TRAIN_SOURCE.contains(&door),
            "the override merge must go through the public read-back door; the private wire \
             struct this plan was originally written against cannot be named from apr-cli at \
             all, which is the finding 04-14 closed"
        );

        let wire = needle(&["Wire", " {"]);
        assert_eq!(
            SETFIT_TRAIN_SOURCE.matches(&wire).count(),
            0,
            "no wire type may be constructed here"
        );
        let wire_path = needle(&["Config", "Wire"]);
        assert_eq!(
            SETFIT_TRAIN_SOURCE.matches(&wire_path).count(),
            0,
            "and none may be named here either"
        );
    }

    #[test]
    fn setfit_train_has_one_rename_site_and_creates_files_nowhere_else() {
        let rename = needle(&["fs::", "rename("]);
        assert_eq!(
            SETFIT_TRAIN_SOURCE.matches(&rename).count(),
            1,
            "exactly ONE rename site, so the three write-safety properties are proven once \
             rather than once per caller (Phase 2 plan 02-09's discipline)"
        );

        let create = needle(&["File::", "create("]);
        assert_eq!(
            SETFIT_TRAIN_SOURCE.matches(&create).count(),
            0,
            "and no unconditional file creation outside the atomic helper, which opens with \
             create_new so a leftover scratch file is diagnosed rather than reused"
        );
    }
}
