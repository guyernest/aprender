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
//! # Task 1 placeholder state
//!
//! Both functions are declared here at their FINAL signatures with typed placeholder
//! bodies so that the clap variants and the dispatch arms landing in the same commit are
//! compilable. Plan 02-09 Task 2 fills in `run_select`, Task 3 fills in `run_pairs`;
//! neither changes a signature. The two std placeholder macros are deliberately NOT used
//! — both abort the process, and panicking macros are banned by repo policy — so each
//! body returns a structured `CliError` instead. Naming them literally here would also
//! trip this plan's own acceptance grep, which is a self-needle of the kind plan 02-08
//! had to fix twice.

use crate::error::{CliError, Result};
use std::path::Path;

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
