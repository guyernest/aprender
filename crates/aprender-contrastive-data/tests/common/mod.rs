//! Shared typed models and the canonical integrity verifier for the committed SetFit
//! pair-count reference fixtures (plan 02-04).
//!
//! WHY THESE LIVE HERE AND NOT IN AN INTEGRATION TEST
//! --------------------------------------------------
//! Every file directly under `tests/` is compiled as its OWN crate, so a `pub struct`
//! declared in `reference_fixtures.rs` is not importable from `pair_counts.rs`
//! (plan 02-07) or `negative_materializing.rs` (plan 02-08). `tests/common/mod.rs` is a
//! MODULE rather than a test target, and each consumer picks it up with `mod common;`.
//! Three consumers are planned, which is exactly why the definitions are not duplicated
//! into the first one.
//!
//! WORKING-DIRECTORY INDEPENDENCE IS THE WHOLE POINT OF `fixture_dir`
//! ------------------------------------------------------------------
//! `shasum -a 256 -c manifest.sha256` resolves the listed paths against the CALLER's
//! working directory, so it is correct only when run from the fixture directory:
//!
//! ```text
//! cd crates/aprender-contrastive-data/tests/setfit_reference \
//!     && shasum -a 256 -c manifest.sha256
//! ```
//!
//! That `cd` is part of the command, not a detail of it. It is retained as a
//! convenience only. The CANONICAL check is [`manifest_drift`], which resolves every
//! manifest entry against the manifest FILE's own directory — computed from
//! `CARGO_MANIFEST_DIR`, a compile-time constant — so `cargo test` gives the same
//! answer from the repository root and from the crate directory.
//!
//! THIS IS NOT A D-04 BOUNDARY VIOLATION
//! -------------------------------------
//! D-04 bans `std::fs` / `std::net` / path-shaped APIs under `src/`, and
//! `make contrastive-data-boundary` scans `src/` only. Integration tests are outside the
//! library boundary; reading committed fixtures off disk here is correct and intended.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

/// One clause of the three-clause deviation from SetFit
/// (`OBLIG-CPP-DEVIATION-DECLARED`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviationClause {
    pub clause_id: String,
    pub statement: String,
}

/// What the pinned `setfit==1.1.3` sampler MEASURABLY does.
///
/// `deny_unknown_fields` is deliberate: a field added by the generator and not mirrored
/// here fails the test rather than being silently ignored, so the fixture schema and the
/// consumers cannot drift apart.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredFixture {
    pub fixture_family: String,
    pub fixture_id: String,
    pub layout: Vec<u64>,
    pub n_examples: u64,
    pub n_classes: u64,
    pub sampling_strategy: String,
    pub multilabel: bool,
    /// `-1` means uncapped, matching the reference's own sentinel.
    pub max_pairs: i64,
    pub stored_pos: u64,
    pub stored_neg: u64,
    pub self_pair_count: u64,
    pub orientation_duplicate_count: u64,
    pub len_pos: u64,
    pub len_neg: u64,
    pub total: u64,
    /// Fields whose value depends on the reference's hardcoded `RandomState(42)`
    /// permutation rather than on the layout alone.
    pub rng_dependent_fields: Vec<String>,
    pub why_this_layout: String,
    pub derivation: String,
    pub reference_notes: Vec<String>,
    pub setfit_version: String,
    pub uv_lock_sha256: String,
    pub uv_version: String,
}

/// What Aprender's contract SAYS, for the same layout, with self-pairs excluded.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractedFixture {
    pub fixture_family: String,
    pub fixture_id: String,
    pub layout: Vec<u64>,
    pub n_examples: u64,
    pub n_classes: u64,
    pub positive_capacity: u64,
    pub negative_capacity: u64,
    pub closed_form_budget: u64,
    pub hard_cap: u64,
    pub clamp_engaged: bool,
    pub explicit_budget: Option<u64>,
    pub default_epoch_budget: u64,
    pub resolved_budget: u64,
    pub resolved_pos_count: u64,
    pub resolved_neg_count: u64,
    /// `None` for ordinary alternating layouts; `Some("negatives_only")` for the K ≈ N
    /// all-singleton layout, and so on.
    pub degenerate_case: Option<String>,
    pub self_pairs_excluded: bool,
    pub measured_counterpart: String,
    pub measured_total: u64,
    pub divergence_note: String,
    pub deviation_attribution: String,
    pub deviation_clauses: Vec<DeviationClause>,
    pub why_this_layout: String,
    pub derivation: String,
    pub setfit_version: String,
    pub uv_lock_sha256: String,
    pub uv_version: String,
}

/// The directory holding the committed fixtures and their manifest.
pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/setfit_reference")
}

/// Every problem found while checking `manifest.sha256`. Empty means clean.
pub fn manifest_drift() -> Vec<String> {
    vec!["stub: manifest verification is not implemented".to_string()]
}

/// Names of the fixture files present on disk, excluding the manifest itself.
pub fn fixture_files() -> Vec<String> {
    Vec::new()
}

/// Names listed in the manifest, in file order.
pub fn manifest_names() -> Vec<String> {
    Vec::new()
}

/// Every measured fixture, keyed by `fixture_id`.
pub fn load_measured() -> BTreeMap<String, MeasuredFixture> {
    BTreeMap::new()
}

/// Every contracted fixture, keyed by `fixture_id`.
pub fn load_contracted() -> BTreeMap<String, ContractedFixture> {
    BTreeMap::new()
}
