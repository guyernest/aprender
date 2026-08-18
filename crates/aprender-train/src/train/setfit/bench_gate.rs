//! The claims gate: fail-closed verification over a benchmark directory, then closed-form
//! aggregation (plan 05-10, EVAL-04).
//!
//! Contract: `setfit-benchmark-claims-v1` — equations `completeness_rule`, `pairing_rule`,
//! `selection_safety_evidence`, `no_selection_attestation` and `claims_statistics`. This module
//! implements that file field for field, in the rule order the file declares.
//!
//! # The one property this module exists for
//!
//! EVAL-04: *missing, selectively omitted, unmatched, or post-test-selected cells invalidate the
//! report*. That is a STRUCTURAL claim, so it is enforced structurally: [`aggregate`] takes a
//! [`VerifiedRunSet`], which has no public constructor and is reachable only out of
//! [`verify_run`]. There is no partial-data mode and no "aggregate what we have" path, because
//! neither can be expressed.
//!
//! # Verification order IS the property
//!
//! [`verify_run`] walks the contract's rules in the contract's order, and returns on the FIRST
//! failure:
//!
//! 1. the manifest's own digest, recomputed from its payload's canonical bytes;
//! 2. the manifest's expectation set equals the contract-derived 80, in contract order;
//! 3. every declared cell is `complete` with a recorded digest;
//! 4. per cell: the row file exists, [`BenchRow::from_bytes`] accepts it (schema first, then its
//!    own envelope digest — that ORDER belongs to `bench_row` and is documented there), the file
//!    bytes hash to the digest the MANIFEST recorded, and the payload agrees with the slot;
//! 5. per `(shots, seed)`: the two rows carry an identical `selection_manifest_hash`;
//! 6. per row: provenance RECOMPUTED from committed bytes (below);
//! 7. per LoRA row: the no-selection attestation's conjuncts.
//!
//! Refusing early is not an optimisation. Aggregating over partially-verified rows would produce
//! a number, and a number is what a reader takes away.
//!
//! # Provenance is recomputed, never trusted
//!
//! A row's `lock_hash` and `candidate_ledger_sha256` are CLAIMS ABOUT FILES. This gate reads the
//! bytes at `lock.lock_record_path` and at `candidate_ledger_path` and recomputes their SHA-256,
//! and it counts the ledger's lines rather than reading `candidates_trained`. A row field that
//! disagrees with the file it names is a refusal naming BOTH the cell and the file.
//!
//! # THE RESIDUAL, STATED RATHER THAN HIDDEN
//!
//! Recomputing the lock and ledger digests raises selection safety from a SELF-ASSERTED BOOLEAN
//! to APPEND-ONLY, RECOMPUTABLE EVIDENCE. It does not reach a cryptographic train-then-seal
//! credential, and it is not claimed to. **A producer that controls the rows AND the lock/ledger
//! files can still emit a mutually consistent forgery.** The six doctored negatives in this
//! module's test file prove detection of INCONSISTENT evidence; they prove nothing about
//! TRUTHFUL provenance. This is the same wording `setfit-benchmark-claims-v1`'s own
//! `selection_safety_evidence.residual_risk` uses, and it is repeated here because a gate whose
//! module doc overstates what it proves is the exact failure this phase exists to prevent — the
//! phase's own bar is "a gate could pass while the claim is false".
//!
//! # No RNG, and no second definition of the arithmetic
//!
//! Every statistic comes from the closed-form f64 surface plan 05-04 shipped in
//! `aprender::stats::hypothesis` — [`mean_f64`], [`sample_std_f64`], [`min_max_f64`],
//! [`paired_ci95_df9`] and [`ttest_rel_f64`], all built on the frozen [`T_CRIT_975_DF9`]. Nothing
//! is reimplemented here: a second mean is a second definition, and two definitions of one number
//! disagree eventually and invisibly. There is no `rand` import anywhere in this file, and a test
//! scans the non-comment source to keep it that way (D-06).
//!
//! # A degenerate interval is VISIBLE, never `null`
//!
//! Ten seeds that all move by the same amount is not exotic. `paired_ci95_df9` returns
//! [`AprenderError::ZeroVarianceDifferences`] for that input, and this module renders it as
//! [`Ci95`] carrying [`ZERO_VARIANCE_NULL_REASON`] and NO bounds. A non-finite `f64` would be
//! serialized by `serde_json` as `null`, which a reader parses as a MISSING measurement rather
//! than a degenerate one (Ph3 CR-03). Nothing in [`RunAggregate`] serializes to `null`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use aprender::error::AprenderError;
use aprender::stats::hypothesis::{
    mean_f64, min_max_f64, paired_ci95_df9, sample_std_f64, ttest_rel_f64, PAIRED_DESIGN_N,
    T_CRIT_975_DF9,
};
use serde::{Deserialize, Serialize};

use super::bench_row::{
    sha256_hex, BenchRow, CellKey, CellStatus, Method, MethodEvidence, RunManifest, BENCH_METHODS,
    BENCH_SEEDS, BENCH_SHOTS, CLAIMS_CONTRACT_ID, EXPECTED_CELLS, MECHANISM_CHILD_MAX_RSS_TIME_L,
    MECHANISM_CHILD_MAX_RSS_VM_HWM,
};
use super::lock::SelectionRule;

// ===========================================================================================
// Directory layout — ONE spelling, shared with the adapters
// ===========================================================================================

/// Row files live here, relative to the benchmark directory.
pub const ROWS_DIR: &str = "rows";

/// Committed SetFit selection-lock records live here.
pub const LOCKS_DIR: &str = "locks";

/// Append-only LoRA candidate ledgers live here.
pub const LEDGER_DIR: &str = "ledger";

/// The pre-declared expectation set lives here.
pub const RUN_MANIFEST_FILE: &str = "run-manifest.json";

/// The house cap for every evidence file this gate reads.
///
/// The gate never reads a model artifact; everything it opens is a row, a manifest, a lock
/// record or a ledger, all of which are small JSON. The cap is checked from the file's DECLARED
/// length before a byte is read, and again against the stream, so a length that lied is detected
/// rather than silently truncated into a payload that happens to parse.
pub const MAX_EVIDENCE_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// The two `role` values a committed lock record may carry, matching `apr eval`'s `LockRow`.
pub const LOCK_ROLES: [&str; 2] = ["written", "consumed"];

/// The ledger's contracted line count. More than one line IS the model selection the LoRA
/// attestation says did not happen.
pub const CONTRACTED_CANDIDATES_TRAINED: u32 = 1;

/// The value [`Ci95::null_reason`] carries when the paired differences have no variance.
pub const ZERO_VARIANCE_NULL_REASON: &str = "zero_variance";

/// The claims contract, embedded at compile time.
///
/// `include_str!` rather than a runtime read, on `bench_row`'s precedent: a cross-pin test that
/// silently skips when the contract is absent proves nothing.
#[cfg(test)]
pub(crate) const CLAIMS_CONTRACT_YAML: &str =
    include_str!("../../../../../contracts/setfit-benchmark-claims-v1.yaml");

/// This module's own source, for the no-RNG structural guard.
#[cfg(test)]
const BENCH_GATE_SOURCE: &str = include_str!("bench_gate.rs");

/// The row filename grammar, produced by ONE function.
///
/// `{method}-s{shots}-seed{seed}.json`. Two spellings of a filename are two filenames, and this
/// gate compares a manifest-recorded digest against the file this name resolves to — so a drift
/// between the writer's spelling and the reader's would make every cell look un-run while
/// reporting a missing-file error that names a path the writer never used. `apr-cli`'s
/// `setfit_bench::row_file_name` delegates here rather than restating it.
#[must_use]
pub fn row_file_name(cell: CellKey) -> String {
    format!("{}-s{}-seed{}.json", cell.method.tag(), cell.shots, cell.seed)
}

// ===========================================================================================
// Resource mechanism classes (review consensus item 2)
// ===========================================================================================

/// What KIND of number a peak-RSS mechanism produces.
///
/// The renderer needs this at the point of comparison, not in a methods paragraph: a
/// `sysinfo_sampled_*` figure is a LOWER BOUND whose bias is one-directional and whose magnitude
/// depends on the allocation pattern, so it cannot be corrected for and is not comparable to an
/// exact kernel high-water mark. A table that puts one beside the other is arithmetically true
/// and factually misleading, which is the class of defect PF-008 names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MechanismClass {
    /// The kernel's own high-water mark: `child_max_rss_*`, or `vm_hwm` for the training process.
    ExactKernelHighWaterMark,
    /// A poll at a finite rate. Can only UNDERSTATE, never overstate.
    SampledLowerBound,
    /// A mechanism string this build does not know. Never silently treated as comparable.
    Unrecognised,
}

impl MechanismClass {
    /// The stable tag, for rendering.
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::ExactKernelHighWaterMark => "exact_kernel_high_water_mark",
            Self::SampledLowerBound => "sampled_lower_bound",
            Self::Unrecognised => "unrecognised",
        }
    }
}

/// Classify a mechanism string.
///
/// `vm_hwm` is the TRAINING process's own `/proc/self/status` high-water mark — an exact kernel
/// figure, taken in-process rather than off a child, which is why it is named separately from
/// the two `child_max_rss_*` strings the contract enumerates.
#[must_use]
pub fn mechanism_class(mechanism: &str) -> MechanismClass {
    if mechanism == MECHANISM_CHILD_MAX_RSS_TIME_L
        || mechanism == MECHANISM_CHILD_MAX_RSS_VM_HWM
        || mechanism == "vm_hwm"
    {
        MechanismClass::ExactKernelHighWaterMark
    } else if mechanism.starts_with("sysinfo_sampled") {
        MechanismClass::SampledLowerBound
    } else {
        MechanismClass::Unrecognised
    }
}

/// Whether two figures measured by these mechanisms may be placed side by side unqualified.
///
/// `Unrecognised` is never comparable to anything, including itself: an unknown mechanism is not
/// evidence that two numbers mean the same thing.
#[must_use]
pub fn mechanisms_are_comparable(left: &str, right: &str) -> bool {
    let (a, b) = (mechanism_class(left), mechanism_class(right));
    a == b && a != MechanismClass::Unrecognised
}

// ===========================================================================================
// Refusals
// ===========================================================================================

/// Why a benchmark directory is not a publishable run.
///
/// Every variant names the CELL, and every provenance variant also names the FILE whose bytes
/// disagreed — because "which cell" and "which file" are what a reader needs to re-run, and a
/// gate that says only "verification failed" gets its output ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BenchGateError {
    /// The manifest's envelope digest disagrees with its payload's own bytes.
    ManifestDigestMismatch {
        /// The digest the envelope claims.
        expected: String,
        /// The digest the payload's bytes produce.
        got: String,
    },
    /// The manifest declares no cells. Zero cells is a failure, not a clean run.
    EmptyExpectationSet,
    /// The manifest's cell sequence is not exactly the contract-derived 80 in contract order.
    ExpectationSetMismatch {
        /// How many cells the manifest declares.
        declared: usize,
        /// How many the contract derives.
        expected: usize,
    },
    /// A declared cell is still `pending`, or is `complete` with no recorded digest.
    IncompleteCell {
        /// The offending cell, rendered.
        cell: String,
        /// What the manifest actually records for it.
        detail: String,
    },
    /// The row file for a complete cell is not on disk.
    RowFileMissing {
        /// The offending cell, rendered.
        cell: String,
        /// The path that was expected to hold it.
        path: String,
    },
    /// An evidence file could not be read (permissions, over the cap, a directory).
    EvidenceReadFailed {
        /// The offending cell, rendered.
        cell: String,
        /// The file.
        path: String,
        /// The underlying diagnostic.
        detail: String,
    },
    /// The row's OWN envelope digest disagrees with its payload — the bit-flip signature.
    RowDigestMismatch {
        /// The offending cell, rendered.
        cell: String,
        /// The row file.
        path: String,
        /// The digest the envelope claims.
        expected: String,
        /// The digest the payload's bytes produce.
        got: String,
    },
    /// The row's bytes are not a row this schema can accept — a trimmed block, an unknown
    /// field, a foreign schema version, an uncontracted cell.
    RowSchemaRefused {
        /// The offending cell, rendered.
        cell: String,
        /// The row file.
        path: String,
        /// The library's own typed diagnostic.
        detail: String,
    },
    /// The row file's bytes do not hash to the digest the MANIFEST recorded for that cell.
    ///
    /// Distinct from [`Self::RowDigestMismatch`]: that one says the row contradicts ITSELF, this
    /// one says a self-consistent row was substituted for the one the run recorded.
    RowManifestDigestMismatch {
        /// The offending cell, rendered.
        cell: String,
        /// The row file.
        path: String,
        /// The digest the manifest recorded.
        recorded: String,
        /// The digest the file's bytes produce.
        actual: String,
    },
    /// A row filed under one cell whose payload names another.
    RowSlotMismatch {
        /// The slot it was filed under.
        cell: String,
        /// The row file.
        path: String,
        /// The cell its payload names.
        payload_cell: String,
    },
    /// A `(shots, seed)` pair whose two rows consumed DIFFERENT selection manifests.
    ///
    /// PF-007's incomparable comparison: two methods evaluated on different sampled few-shot
    /// subsets are not two measurements of the same thing, and at 8 examples per class the
    /// subset is a larger source of variation than the method.
    UnpairedSelection {
        /// Examples per class.
        shots: u32,
        /// The seed.
        seed: u32,
        /// The SetFit row's pairing key.
        setfit_hash: String,
        /// The LoRA row's pairing key.
        lora_hash: String,
    },
    /// A row field disagrees with the committed file it names — FORGED PROVENANCE.
    ProvenanceMismatch {
        /// The offending cell, rendered.
        cell: String,
        /// The file whose bytes disagreed.
        file: String,
        /// What the row claims.
        claimed: String,
        /// What the file's bytes produce.
        recomputed: String,
        /// Which claim it was.
        detail: String,
    },
    /// A conjunct that forecloses post-test selection does not hold.
    PostTestSelection {
        /// The offending cell, rendered.
        cell: String,
        /// Which conjunct failed.
        conjunct: String,
        /// The observed state.
        detail: String,
    },
}

impl BenchGateError {
    /// A stable discriminant string, so a test can assert that N refusals are N DISTINCT
    /// variants without matching on rendered prose.
    #[must_use]
    pub const fn variant_tag(&self) -> &'static str {
        match self {
            Self::ManifestDigestMismatch { .. } => "manifest_digest_mismatch",
            Self::EmptyExpectationSet => "empty_expectation_set",
            Self::ExpectationSetMismatch { .. } => "expectation_set_mismatch",
            Self::IncompleteCell { .. } => "incomplete_cell",
            Self::RowFileMissing { .. } => "row_file_missing",
            Self::EvidenceReadFailed { .. } => "evidence_read_failed",
            Self::RowDigestMismatch { .. } => "row_digest_mismatch",
            Self::RowSchemaRefused { .. } => "row_schema_refused",
            Self::RowManifestDigestMismatch { .. } => "row_manifest_digest_mismatch",
            Self::RowSlotMismatch { .. } => "row_slot_mismatch",
            Self::UnpairedSelection { .. } => "unpaired_selection",
            Self::ProvenanceMismatch { .. } => "provenance_mismatch",
            Self::PostTestSelection { .. } => "post_test_selection",
        }
    }

    /// The cell this refusal is about, when it is about one.
    #[must_use]
    pub fn cell(&self) -> Option<&str> {
        match self {
            Self::IncompleteCell { cell, .. }
            | Self::RowFileMissing { cell, .. }
            | Self::EvidenceReadFailed { cell, .. }
            | Self::RowDigestMismatch { cell, .. }
            | Self::RowSchemaRefused { cell, .. }
            | Self::RowManifestDigestMismatch { cell, .. }
            | Self::RowSlotMismatch { cell, .. }
            | Self::ProvenanceMismatch { cell, .. }
            | Self::PostTestSelection { cell, .. } => Some(cell),
            Self::ManifestDigestMismatch { .. }
            | Self::EmptyExpectationSet
            | Self::ExpectationSetMismatch { .. }
            | Self::UnpairedSelection { .. } => None,
        }
    }
}

impl core::fmt::Display for BenchGateError {
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ManifestDigestMismatch { expected, got } => write!(
                f,
                "the run manifest's envelope claims digest {expected} but its payload's own \
                 bytes hash to {got}. These are not the bytes that were attested; re-run the \
                 affected cells rather than editing the manifest",
            ),
            Self::EmptyExpectationSet => write!(
                f,
                "the run manifest declares zero cells. A gate whose input is empty finds no \
                 failures and would report success; zero cells is a failure, not a clean run. \
                 Re-declare the manifest with `apr setfit bench run --bench-dir <DIR>`",
            ),
            Self::ExpectationSetMismatch { declared, expected } => write!(
                f,
                "the run manifest declares {declared} cells, but {CLAIMS_CONTRACT_ID} derives \
                 {expected} in the order (method, shots ascending, seed ascending). \
                 Completeness is defined by the contract-derived set, never by whatever the \
                 manifest happens to list — otherwise a 12-cell expectation could be satisfied \
                 completely and published as a complete run",
            ),
            Self::IncompleteCell { cell, detail } => write!(
                f,
                "cell {cell} is not complete: {detail}. The report has no partial-data mode — \
                 run it with `apr setfit bench run --method {} --shots ... --seed ...`, or, if \
                 it ran on the other host, ingest its row with `--record <ROW_FILE>`",
                cell.split('/').next().unwrap_or("<method>"),
            ),
            Self::RowFileMissing { cell, path } => write!(
                f,
                "cell {cell} is recorded complete in the manifest, but {path} does not exist. \
                 A recorded digest with no bytes behind it is an omission the manifest cannot \
                 see; re-run the cell or restore the row file",
            ),
            Self::EvidenceReadFailed { cell, path, detail } => {
                write!(f, "cell {cell}: {path} could not be read: {detail}",)
            }
            Self::RowDigestMismatch { cell, path, expected, got } => write!(
                f,
                "cell {cell}: {path} claims digest {expected} but its payload's own bytes hash \
                 to {got}. These are not the bytes that were attested; re-run the cell rather \
                 than editing the row",
            ),
            Self::RowSchemaRefused { cell, path, detail } => {
                write!(f, "cell {cell}: {path} is not a row this schema accepts: {detail}",)
            }
            Self::RowManifestDigestMismatch { cell, path, recorded, actual } => write!(
                f,
                "cell {cell}: the manifest recorded row digest {recorded}, but {path} hashes to \
                 {actual}. The row is internally consistent, so this is a SUBSTITUTION rather \
                 than a corruption: the run that produced the published number is no longer the \
                 run on disk",
            ),
            Self::RowSlotMismatch { cell, path, payload_cell } => write!(
                f,
                "cell {cell}: {path} carries a payload for {payload_cell}. The manifest slot \
                 and the row payload are two independent statements of the same fact; a \
                 disagreement is a refusal, not a relabelling",
            ),
            Self::UnpairedSelection { shots, seed, setfit_hash, lora_hash } => write!(
                f,
                "the (shots {shots}, seed {seed}) pair is NOT comparable: setfit consumed \
                 selection manifest {setfit_hash} and lora consumed {lora_hash}. Two methods \
                 evaluated on different sampled few-shot subsets are not two measurements of \
                 the same thing — at 8 examples per class the subset is a larger source of \
                 variation than the method. Re-run both cells against ONE selection manifest",
            ),
            Self::ProvenanceMismatch { cell, file, claimed, recomputed, detail } => write!(
                f,
                "cell {cell}: {detail}. The row claims {claimed}; {file} produces {recomputed}. \
                 Selection safety is RECOMPUTED from committed bytes, never read from a row \
                 field — a field that disagrees with the file it names is evidence of nothing",
            ),
            Self::PostTestSelection { cell, conjunct, detail } => write!(
                f,
                "cell {cell}: the `{conjunct}` conjunct does not hold ({detail}). Each conjunct \
                 closes a different route to a post-hoc choice, and any one alone is satisfiable \
                 while the claim is false. Re-run the cell under the frozen published defaults",
            ),
        }
    }
}

impl std::error::Error for BenchGateError {}

// ===========================================================================================
// The verified set — no public constructor, on purpose
// ===========================================================================================

/// Eighty rows that passed every rule of `setfit-benchmark-claims-v1`, in contract order.
///
/// **There is no public constructor and no public field.** The only way to hold one is to have
/// called [`verify_run`] and had it return `Ok`, which is what makes [`aggregate`]'s signature a
/// proof rather than a convention: the statistics cannot be run over an unverified directory
/// even by a future caller inside this crate who has forgotten why. This is the same typestate
/// argument Phase 3 used for `SetFitRun` and Phase 4 for `ReloadedSetFitCredential` — proving a
/// runtime flag is always set needs whole-program reasoning; proving a value cannot be built
/// needs one look at the type.
#[derive(Debug, Clone)]
pub struct VerifiedRunSet {
    /// Cell-keyed rows in the contract order, exactly [`EXPECTED_CELLS`] of them.
    rows: Vec<(CellKey, BenchRow)>,
}

impl VerifiedRunSet {
    /// The verified rows, in the deterministic contract order.
    #[must_use]
    pub fn rows(&self) -> &[(CellKey, BenchRow)] {
        &self.rows
    }

    /// How many rows were verified. Always [`EXPECTED_CELLS`] for a value that exists.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Never true for a value that exists; present so `len` does not trip clippy's pairing lint.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The row for a cell, if the set holds one.
    #[must_use]
    pub fn row(&self, cell: CellKey) -> Option<&BenchRow> {
        self.rows.iter().find(|(key, _)| *key == cell).map(|(_, row)| row)
    }
}

// ===========================================================================================
// Bounded evidence reads
// ===========================================================================================

/// Read a small evidence file, refused from its DECLARED length before a byte is read.
fn read_evidence(cell: CellKey, path: &Path) -> Result<Vec<u8>, BenchGateError> {
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            BenchGateError::RowFileMissing { cell: cell.render(), path: path.display().to_string() }
        } else {
            BenchGateError::EvidenceReadFailed {
                cell: cell.render(),
                path: path.display().to_string(),
                detail: error.to_string(),
            }
        }
    })?;
    if !metadata.is_file() {
        return Err(BenchGateError::EvidenceReadFailed {
            cell: cell.render(),
            path: path.display().to_string(),
            detail: "not a regular file".to_string(),
        });
    }
    if metadata.len() > MAX_EVIDENCE_FILE_BYTES {
        return Err(BenchGateError::EvidenceReadFailed {
            cell: cell.render(),
            path: path.display().to_string(),
            detail: format!(
                "declares {} bytes against the evidence cap of {MAX_EVIDENCE_FILE_BYTES}; \
                 refused from its declared length, before a byte is read",
                metadata.len()
            ),
        });
    }
    let file = fs::File::open(path).map_err(|error| BenchGateError::EvidenceReadFailed {
        cell: cell.render(),
        path: path.display().to_string(),
        detail: error.to_string(),
    })?;
    let mut bytes = Vec::new();
    // `+ 1` so a length that LIED is detected rather than silently truncated into a payload
    // that happens to parse.
    file.take(MAX_EVIDENCE_FILE_BYTES + 1).read_to_end(&mut bytes).map_err(|error| {
        BenchGateError::EvidenceReadFailed {
            cell: cell.render(),
            path: path.display().to_string(),
            detail: error.to_string(),
        }
    })?;
    if bytes.len() as u64 > MAX_EVIDENCE_FILE_BYTES {
        return Err(BenchGateError::EvidenceReadFailed {
            cell: cell.render(),
            path: path.display().to_string(),
            detail: "the stream exceeded the cap, so its declared length lied".to_string(),
        });
    }
    Ok(bytes)
}

// ===========================================================================================
// verify_run — the boundary guard
// ===========================================================================================

/// Verify a benchmark directory against `setfit-benchmark-claims-v1`, in the contract's order.
///
/// `bench_dir` is the directory holding [`RUN_MANIFEST_FILE`], [`ROWS_DIR`], [`LOCKS_DIR`] and
/// [`LEDGER_DIR`]. It is the whole directory rather than just the rows directory because a row's
/// `lock_record_path` and `candidate_ledger_path` are RELATIVE TO THE BENCHMARK DIRECTORY —
/// a rows-only parameter could not resolve the very files this gate recomputes from.
///
/// # Errors
///
/// One [`BenchGateError`] naming the FIRST rule that failed and the cell (and, for provenance,
/// the file) it failed on. The walk stops there: aggregating over partially-verified rows would
/// produce a number, and a number is what a reader takes away.
pub fn verify_run(
    manifest: &RunManifest,
    bench_dir: &Path,
) -> Result<VerifiedRunSet, BenchGateError> {
    // ---- 1. THE MANIFEST'S OWN DIGEST -----------------------------------------------------
    // Recomputed here even though `RunManifest::from_bytes` already checked it, because a
    // manifest can also be built in memory (`declare()` + `record()`), and this gate must not
    // depend on which door its argument came through.
    let recomputed = manifest
        .payload
        .to_canonical_bytes()
        .map_or_else(|_| String::new(), |bytes| sha256_hex(&bytes));
    if recomputed != manifest.semantic_hash {
        return Err(BenchGateError::ManifestDigestMismatch {
            expected: manifest.semantic_hash.clone(),
            got: recomputed,
        });
    }

    // ---- 2. THE EXPECTATION SET, BEFORE ANY ROW BYTE IS READ -------------------------------
    // The two vacuity backstops. Without them a producer can declare almost nothing, satisfy it
    // completely, and publish a "complete" run — and a gate with no input reports success.
    if manifest.payload.cells.is_empty() {
        return Err(BenchGateError::EmptyExpectationSet);
    }
    let expectation = RunManifest::expectation();
    let declared: Vec<CellKey> = manifest.payload.cells.iter().map(|e| e.cell()).collect();
    if declared != expectation {
        return Err(BenchGateError::ExpectationSetMismatch {
            declared: declared.len(),
            expected: EXPECTED_CELLS,
        });
    }

    // ---- 3. EVERY EXPECTED CELL IS COMPLETE -----------------------------------------------
    // Also before any row byte is read: a `pending` entry in a pre-declared table is what makes
    // a selectively omitted cell VISIBLE, and reading rows first would report a file-level
    // symptom for a run-level omission.
    for entry in &manifest.payload.cells {
        match (entry.status, entry.row_sha256.as_deref()) {
            (CellStatus::Complete, Some(digest)) if !digest.is_empty() => {}
            (CellStatus::Complete, _) => {
                return Err(BenchGateError::IncompleteCell {
                    cell: entry.cell().render(),
                    detail: "the manifest marks it complete but records no row digest".to_string(),
                });
            }
            (CellStatus::Pending, _) => {
                return Err(BenchGateError::IncompleteCell {
                    cell: entry.cell().render(),
                    detail: "the manifest still lists it as `pending`, so this run is not \
                             complete and no aggregate over it is publishable"
                        .to_string(),
                });
            }
        }
    }

    // ---- 4. PER CELL: FILE, SCHEMA + ENVELOPE DIGEST, MANIFEST DIGEST, SLOT ----------------
    let rows_dir = bench_dir.join(ROWS_DIR);
    let mut rows: Vec<(CellKey, BenchRow)> = Vec::with_capacity(EXPECTED_CELLS);
    for entry in &manifest.payload.cells {
        let cell = entry.cell();
        let path = rows_dir.join(row_file_name(cell));
        let bytes = read_evidence(cell, &path)?;

        // `from_bytes` is the ONE door: it parses (schema) and then recomputes the envelope
        // digest, and it returns nothing on either failure. The two outcomes are separated here
        // because they are different defects with the same symptom — a trimmed block is an
        // omission, a digest mismatch is tampering.
        // Match the variant directly rather than dispatching on `variant_tag()` and
        // then re-matching: the string compare discarded the type information the
        // second match needed back, which forced an unreachable arm to stay total.
        // Matching once makes that arm impossible to write, and moves `expected`/
        // `got` out of the owned error instead of cloning them.
        let row = BenchRow::from_bytes(&bytes).map_err(|error| match error {
            super::bench_row::BenchRowError::SemanticHashMismatch { expected, got } => {
                BenchGateError::RowDigestMismatch {
                    cell: cell.render(),
                    path: path.display().to_string(),
                    expected,
                    got,
                }
            }
            other => BenchGateError::RowSchemaRefused {
                cell: cell.render(),
                path: path.display().to_string(),
                detail: other.to_string(),
            }
        })?;

        let recorded = entry.row_sha256.clone().unwrap_or_default();
        // WHICH DIGEST THE MANIFEST HOLDS, stated once. `emit_row` records `row.semantic_hash`
        // — the digest over the payload's CANONICAL COMPACT bytes — not a digest over the file.
        // That is deliberate and it is `bench_row_schema`'s own invariant: the file is PRETTY so
        // rows can be reviewed in diffs, and the digest is canonical so whitespace cannot change
        // a row's identity. Comparing the file's bytes here would refuse a byte-identical row
        // that had been reformatted, and would call it tampering.
        //
        // The check is not weakened by that: `from_bytes` above has already proven
        // `semantic_hash == sha256(canonical(payload))`, so a row whose payload differs at all
        // carries a different `semantic_hash` and is caught here.
        if row.semantic_hash != recorded {
            return Err(BenchGateError::RowManifestDigestMismatch {
                cell: cell.render(),
                path: path.display().to_string(),
                recorded,
                actual: row.semantic_hash.clone(),
            });
        }

        if row.payload.cell() != cell {
            return Err(BenchGateError::RowSlotMismatch {
                cell: cell.render(),
                path: path.display().to_string(),
                payload_cell: row.payload.cell().render(),
            });
        }

        rows.push((cell, row));
    }

    // ---- 5. PAIRING ------------------------------------------------------------------------
    verify_pairing(&rows)?;

    // ---- 6. PROVENANCE, RECOMPUTED FROM COMMITTED BYTES ------------------------------------
    for (cell, row) in &rows {
        verify_provenance(*cell, row, bench_dir)?;
    }

    // ---- 7. THE LoRA NO-SELECTION ATTESTATION ----------------------------------------------
    for (cell, row) in &rows {
        if let MethodEvidence::Lora(evidence) = &row.payload.evidence {
            verify_lora_attestation(*cell, evidence)?;
        }
    }

    Ok(VerifiedRunSet { rows })
}

/// Every `(shots, seed)` pair's two rows must carry an identical `selection_manifest_hash`.
fn verify_pairing(rows: &[(CellKey, BenchRow)]) -> Result<(), BenchGateError> {
    let mut by_cell: BTreeMap<CellKey, &str> = BTreeMap::new();
    for (cell, row) in rows {
        by_cell.insert(*cell, row.payload.selection_manifest_hash.as_str());
    }
    // Iterated in the contract order rather than over the map, so the FIRST mismatch reported
    // is deterministic across runs.
    for shots in BENCH_SHOTS {
        for seed in BENCH_SEEDS {
            let setfit = by_cell.get(&CellKey::new(Method::Setfit, shots, seed));
            let lora = by_cell.get(&CellKey::new(Method::Lora, shots, seed));
            if let (Some(setfit_hash), Some(lora_hash)) = (setfit, lora) {
                if setfit_hash != lora_hash {
                    return Err(BenchGateError::UnpairedSelection {
                        shots,
                        seed,
                        setfit_hash: (*setfit_hash).to_string(),
                        lora_hash: (*lora_hash).to_string(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// One JSONL line of the LoRA candidate ledger, as much of it as this gate reads.
#[derive(Debug, Deserialize)]
struct LedgerLine {
    /// The selection manifest the invocation consumed. Must equal the row's pairing key.
    selection_manifest_hash: String,
}

/// Recompute a row's provenance from the committed bytes it names.
fn verify_provenance(
    cell: CellKey,
    row: &BenchRow,
    bench_dir: &Path,
) -> Result<(), BenchGateError> {
    match &row.payload.evidence {
        MethodEvidence::Setfit(evidence) => {
            let relative = PathBuf::from(&evidence.lock.lock_record_path);
            let path = bench_dir.join(&relative);
            let bytes = read_evidence(cell, &path)?;
            let recomputed = sha256_hex(&bytes);
            if recomputed != evidence.lock.lock_hash {
                return Err(BenchGateError::ProvenanceMismatch {
                    cell: cell.render(),
                    file: path.display().to_string(),
                    claimed: evidence.lock.lock_hash.clone(),
                    recomputed,
                    detail: "the committed lock record's bytes do not hash to the row's \
                             `lock.lock_hash`"
                        .to_string(),
                });
            }
            // The lock's ROLE and RULE are what make it a selection lock rather than a file.
            if !LOCK_ROLES.contains(&evidence.lock.role.as_str()) {
                return Err(BenchGateError::PostTestSelection {
                    cell: cell.render(),
                    conjunct: "lock.role".to_string(),
                    detail: format!(
                        "the row records role `{}`; the vocabulary is {LOCK_ROLES:?}",
                        evidence.lock.role
                    ),
                });
            }
            if evidence.lock.rule != SelectionRule::MaxMetricLowestIndexTieBreak.tag() {
                return Err(BenchGateError::PostTestSelection {
                    cell: cell.render(),
                    conjunct: "lock.rule".to_string(),
                    detail: format!(
                        "the row records rule `{}`; this build commits under `{}`, and a lock \
                         written under an unrecognised rule is a selection nothing can replay",
                        evidence.lock.rule,
                        SelectionRule::MaxMetricLowestIndexTieBreak.tag()
                    ),
                });
            }
            Ok(())
        }
        MethodEvidence::Lora(evidence) => {
            let relative = PathBuf::from(&evidence.candidate_ledger_path);
            let path = bench_dir.join(&relative);
            let bytes = read_evidence(cell, &path)?;
            let recomputed = sha256_hex(&bytes);
            if recomputed != evidence.candidate_ledger_sha256 {
                return Err(BenchGateError::ProvenanceMismatch {
                    cell: cell.render(),
                    file: path.display().to_string(),
                    claimed: evidence.candidate_ledger_sha256.clone(),
                    recomputed,
                    detail: "the committed candidate ledger's bytes do not hash to the row's \
                             `candidate_ledger_sha256`"
                        .to_string(),
                });
            }

            // THE LINE COUNT IS COUNTED, NOT READ. A ledger carrying a second candidate while
            // the row still claims one is the forgery this recomputation exists to catch.
            let lines: Vec<&[u8]> =
                bytes.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()).collect();
            let counted = u32::try_from(lines.len()).unwrap_or(u32::MAX);
            if counted != evidence.candidates_trained {
                return Err(BenchGateError::ProvenanceMismatch {
                    cell: cell.render(),
                    file: path.display().to_string(),
                    claimed: format!("candidates_trained = {}", evidence.candidates_trained),
                    recomputed: format!("{counted} ledger line(s)"),
                    detail: "the committed candidate ledger's line count disagrees with the \
                             row's `candidates_trained`"
                        .to_string(),
                });
            }

            // Every line must name the SAME selection manifest the row does, so a ledger
            // transplanted from another cell cannot stand in for this one's.
            for (index, line) in lines.iter().enumerate() {
                let parsed: LedgerLine = serde_json::from_slice(line).map_err(|error| {
                    BenchGateError::ProvenanceMismatch {
                        cell: cell.render(),
                        file: path.display().to_string(),
                        claimed: "a JSONL candidate line".to_string(),
                        recomputed: format!("line {} did not parse: {error}", index + 1),
                        detail: "the committed candidate ledger is not the append-only JSONL the \
                                 contract requires"
                            .to_string(),
                    }
                })?;
                if parsed.selection_manifest_hash != row.payload.selection_manifest_hash {
                    return Err(BenchGateError::ProvenanceMismatch {
                        cell: cell.render(),
                        file: path.display().to_string(),
                        claimed: row.payload.selection_manifest_hash.clone(),
                        recomputed: parsed.selection_manifest_hash,
                        detail: format!(
                            "ledger line {} names a different selection manifest than the row",
                            index + 1
                        ),
                    });
                }
            }
            Ok(())
        }
    }
}

/// The six conjuncts of `no_selection_attestation`, each closing a different route to a
/// post-hoc choice.
///
/// The digest and the line count are checked in [`verify_provenance`]; the four self-reported
/// conjuncts are checked here, AFTER them, because the recomputed pair is what makes the
/// self-reported four more than a declaration.
fn verify_lora_attestation(
    cell: CellKey,
    evidence: &super::bench_row::LoraEvidence,
) -> Result<(), BenchGateError> {
    if evidence.epochs_completed != evidence.epochs_requested {
        return Err(BenchGateError::PostTestSelection {
            cell: cell.render(),
            conjunct: "epochs_completed == epochs_requested".to_string(),
            detail: format!(
                "requested {}, completed {} — a run that ended somewhere other than where it \
                 was told to ended at a checkpoint somebody chose",
                evidence.epochs_requested, evidence.epochs_completed
            ),
        });
    }
    if !evidence.early_stopping_disabled {
        return Err(BenchGateError::PostTestSelection {
            cell: cell.render(),
            conjunct: "early_stopping_disabled".to_string(),
            detail: "early stopping was enabled, so the run picked a checkpoint".to_string(),
        });
    }
    if evidence.val_split != 0.0 {
        return Err(BenchGateError::PostTestSelection {
            cell: cell.render(),
            conjunct: "val_split == 0.0".to_string(),
            detail: format!(
                "a validation split of {} was carved, which created something to select on",
                evidence.val_split
            ),
        });
    }
    if !evidence.no_selection_attestation {
        return Err(BenchGateError::PostTestSelection {
            cell: cell.render(),
            conjunct: "no_selection_attestation".to_string(),
            detail: "the row does not attest that no selection happened".to_string(),
        });
    }
    if evidence.candidates_trained != CONTRACTED_CANDIDATES_TRAINED {
        return Err(BenchGateError::PostTestSelection {
            cell: cell.render(),
            conjunct: "candidates_trained == 1".to_string(),
            detail: format!(
                "{} candidates were trained for this cell; a second candidate IS the \
                 uncontracted model selection the attestation says did not happen",
                evidence.candidates_trained
            ),
        });
    }
    Ok(())
}

// ===========================================================================================
// The aggregate
// ===========================================================================================

/// Mean, `(n-1)` std, min and max over one series.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SeriesSummary {
    /// How many observations. Always [`PAIRED_DESIGN_N`] here.
    pub n: usize,
    /// Arithmetic mean.
    pub mean: f64,
    /// SAMPLE standard deviation, `(n-1)` denominator — the SetFit paper's convention, so the
    /// numbers are directly comparable to published results.
    pub std: f64,
    /// The smallest observation.
    pub min: f64,
    /// The largest observation.
    pub max: f64,
}

/// One seed's headline numbers, so a reader can recompute the summary by hand.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SeedValue {
    /// The contracted seed.
    pub seed: u32,
    /// Official `F_avg`.
    pub f_avg: f64,
    /// `f_avg`'s IEEE-754 bits, carried so a decimal rendering can never become the compared
    /// value.
    pub f_avg_bits: u64,
    /// Three-class macro F1.
    pub macro_f1: f64,
    /// Matthews correlation coefficient.
    pub mcc: f64,
}

/// The quality summary for one `(method, shots)` group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MethodShotQuality {
    /// Which method.
    pub method: Method,
    /// Examples per class.
    pub shots: u32,
    /// The HEADLINE metric's summary.
    pub f_avg: SeriesSummary,
    /// Macro F1's summary, published beside `f_avg` and never instead of it.
    pub macro_f1: SeriesSummary,
    /// MCC's summary.
    pub mcc: SeriesSummary,
    /// Every seed's own numbers, seed ascending.
    pub per_seed: Vec<SeedValue>,
}

/// One seed's paired delta.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SeedDelta {
    /// The contracted seed.
    pub seed: u32,
    /// `F_avg(setfit) - F_avg(lora)` on the SAME selection manifest.
    pub delta: f64,
    /// The delta's IEEE-754 bits.
    pub delta_bits: u64,
}

/// A paired 95% interval, or an explicit statement that there is none.
///
/// Every numeric field is `skip_serializing_if = "Option::is_none"`, so the degenerate case
/// serializes as `{"null_reason": "zero_variance"}` and NOT as a set of `null` bounds. That
/// distinction is the whole point: `serde_json` renders a non-finite `f64` as `null`, and a
/// reader parses `null` as a MISSING measurement rather than a degenerate one (Ph3 CR-03).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ci95 {
    /// `d̄ − t·s_d/√n`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<f64>,
    /// `d̄ + t·s_d/√n`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<f64>,
    /// `t·s_d/√n`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub half_width: Option<f64>,
    /// `s_d/√n`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub std_err: Option<f64>,
    /// Present EXACTLY when there is no interval, stating why in one machine-readable word.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub null_reason: Option<String>,
}

impl Ci95 {
    /// Whether an interval exists.
    #[must_use]
    pub const fn is_present(&self) -> bool {
        self.low.is_some() && self.high.is_some()
    }
}

/// The paired comparison at one shot level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShotDelta {
    /// Examples per class.
    pub shots: u32,
    /// Ten per-seed deltas, seed ascending.
    pub per_seed_deltas: Vec<SeedDelta>,
    /// `d̄`, the mean paired difference. ALWAYS present, even when the interval is not: a point
    /// estimate over identical differences is well defined and is the honest thing to report.
    pub mean_delta: f64,
    /// `s_d`. Absent exactly when the differences have no variance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub std_delta: Option<f64>,
    /// The interval, or the stated reason there is none.
    pub ci95: Ci95,
    /// The paired t-statistic. DETAIL ONLY — never claim language (D-08).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t_statistic: Option<f64>,
    /// The two-tailed p-value. DETAIL ONLY — it may sit in the machine-readable payload and it
    /// may NOT appear in a verdict. A binary verdict is precisely where few-shot seed
    /// sensitivity hides: rankings that reverse across seeds become one word.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p_value: Option<f64>,
}

/// The resource summary for one `(method, shots)` group, every figure carrying its boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MethodShotResource {
    /// Which method.
    pub method: Method,
    /// Examples per class.
    pub shots: u32,
    /// The distinct hosts these cells ran on, sorted. Resource figures are NEVER pooled across
    /// hosts (D-09); this field is what lets the renderer say so at the point of comparison.
    pub hosts: Vec<String>,
    /// The distinct `<device>:<implementation>:<kernel>` identities, sorted.
    pub backends: Vec<String>,
    /// Mean training wall clock, milliseconds.
    pub train_wall_ms: SeriesSummary,
    /// Cold latency: ONE classify in a dedicated fresh child.
    pub cold_latency_ms: SeriesSummary,
    /// Warm latency: median of ten after three warmups, against the reloaded model.
    pub warm_latency_ms_median: SeriesSummary,
    /// Rows per second over one full test-split batch pass.
    pub throughput_rows_per_sec: SeriesSummary,
    /// The batch size that pass used. Throughput without a batch size is not comparable.
    pub throughput_batch_sizes: Vec<u32>,
    /// Peak RSS of the TRAINING process.
    pub train_peak_rss_bytes: SeriesSummary,
    /// The distinct mechanisms `train_peak_rss_bytes` was measured by, sorted.
    pub train_peak_rss_mechanisms: Vec<String>,
    /// The class of those mechanisms, for the renderer's comparability note.
    pub train_peak_rss_mechanism_classes: Vec<MechanismClass>,
    /// Peak RSS of the dedicated COLD-MEASUREMENT CHILD — a different process, a different
    /// number. A kernel high-water mark is process-cumulative, so one pooled figure would
    /// report the training peak while claiming to report the inference peak.
    pub inference_peak_rss_bytes: SeriesSummary,
    /// The distinct mechanisms `inference_peak_rss_bytes` was measured by, sorted.
    pub inference_peak_rss_mechanisms: Vec<String>,
    /// The class of those mechanisms.
    pub inference_peak_rss_mechanism_classes: Vec<MechanismClass>,
    /// Bytes of the artifact THIS method wrote. For LoRA this is the ADAPTER ALONE and it is
    /// NOT a deployable size — see [`Self::deployable_total_bytes`].
    pub artifact_bytes: SeriesSummary,
    /// Bytes a user must ship to serve this model. The ONLY field a cross-method size claim may
    /// be built on (review consensus item 8).
    pub deployable_total_bytes: SeriesSummary,
}

/// Everything recomputed from a [`VerifiedRunSet`], and nothing else.
///
/// No number in here comes from anywhere but the stored rows plus closed-form arithmetic, which
/// is what EVAL-04's "exactly recompute" asks for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunAggregate {
    /// The contract these numbers are computed under.
    pub contract_id: String,
    /// Seeds per `(method, shots)` group.
    pub n_seeds: usize,
    /// Degrees of freedom of the paired design.
    pub degrees_of_freedom: usize,
    /// The frozen two-tailed 95% critical value the intervals were built with.
    pub t_crit_975_df9: f64,
    /// The cell keys, in the exact order this aggregate iterated them. Pinned by test.
    pub key_sequence: Vec<String>,
    /// Quality per `(method, shots)`, in contract order.
    pub quality: Vec<MethodShotQuality>,
    /// The paired deltas per shot level, shots ascending.
    pub deltas: Vec<ShotDelta>,
    /// Resource per `(method, shots)`, in contract order.
    pub resource: Vec<MethodShotResource>,
}

/// The invariant message used wherever a helper's `Option` cannot be `None`.
///
/// [`VerifiedRunSet`] holds exactly [`EXPECTED_CELLS`] rows covering every `(method, shots)`
/// group at all ten seeds — [`verify_run`] proved it before the value existed. `expect` rather
/// than a fallible signature on purpose: if the type cannot carry that guarantee, the typestate
/// is decoration.
const SERIES_INVARIANT: &str =
    "VerifiedRunSet holds exactly ten seeds per (method, shots) — verify_run proved it";

/// Summarise one series with the 05-04 closed-form helpers. No local mean or std.
fn summarise(values: &[f64]) -> SeriesSummary {
    let mean = mean_f64(values).expect(SERIES_INVARIANT);
    let std = sample_std_f64(values).expect(SERIES_INVARIANT);
    let (min, max) = min_max_f64(values).expect(SERIES_INVARIANT);
    SeriesSummary { n: values.len(), mean, std, min, max }
}

/// Distinct values of a string field across a group, sorted — so two runs emit the same order.
fn distinct_sorted(values: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = values.collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Recompute every published number from a verified run set.
///
/// Deterministic by construction: the iteration order is the contract order (method in declared
/// order, then shots ascending, then seed ascending), every collection is a `Vec` or a
/// `BTreeMap`, and no `HashMap` is iterated anywhere. Two invocations over the same rows produce
/// bit-identical output, which a test asserts by comparing serialized `f64` bits rather than
/// rendered decimals.
///
/// The signature is the proof that the arithmetic cannot run on unverified data: [`VerifiedRunSet`]
/// has no public constructor.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn aggregate(verified: &VerifiedRunSet) -> RunAggregate {
    let mut key_sequence = Vec::with_capacity(verified.len());
    let mut quality = Vec::with_capacity(BENCH_METHODS.len() * BENCH_SHOTS.len());
    let mut resource = Vec::with_capacity(BENCH_METHODS.len() * BENCH_SHOTS.len());

    // Index once, in a BTreeMap keyed by the derived-Ord CellKey, so every lookup below is
    // deterministic and no hash iteration order can leak into a published number.
    let mut by_cell: BTreeMap<CellKey, &BenchRow> = BTreeMap::new();
    for (cell, row) in verified.rows() {
        by_cell.insert(*cell, row);
    }

    for method in BENCH_METHODS {
        for shots in BENCH_SHOTS {
            let mut f_avg = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut macro_f1 = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut mcc = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut per_seed = Vec::with_capacity(PAIRED_DESIGN_N);

            let mut train_wall = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut cold = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut warm = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut throughput = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut train_peak = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut inference_peak = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut artifact = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut deployable = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut hosts = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut backends = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut train_mechanisms = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut inference_mechanisms = Vec::with_capacity(PAIRED_DESIGN_N);
            let mut batch_sizes = Vec::with_capacity(PAIRED_DESIGN_N);

            for seed in BENCH_SEEDS {
                let cell = CellKey::new(method, shots, seed);
                key_sequence.push(cell.render());
                let Some(row) = by_cell.get(&cell) else {
                    // Unreachable through verify_run, which proved all 80 cells present. Skipped
                    // rather than panicked so a future in-crate caller gets a short series and a
                    // loud `expect` below rather than an abort here.
                    continue;
                };
                let q = &row.payload.quality;
                f_avg.push(q.f_avg);
                macro_f1.push(q.macro_f1);
                mcc.push(q.mcc);
                per_seed.push(SeedValue {
                    seed,
                    f_avg: q.f_avg,
                    f_avg_bits: q.f_avg_bits,
                    macro_f1: q.macro_f1,
                    mcc: q.mcc,
                });

                let r = &row.payload.resource;
                #[allow(clippy::cast_precision_loss)]
                {
                    train_wall.push(r.train_wall_ms as f64);
                    train_peak.push(r.train_peak_rss_bytes as f64);
                    inference_peak.push(r.inference_peak_rss_bytes as f64);
                    artifact.push(r.artifact_bytes as f64);
                    deployable.push(r.deployable_total_bytes as f64);
                }
                cold.push(r.cold_latency_ms);
                warm.push(r.warm_latency_ms_median);
                throughput.push(r.throughput_rows_per_sec);
                batch_sizes.push(r.throughput_batch_size);
                train_mechanisms.push(r.train_peak_rss_mechanism.clone());
                inference_mechanisms.push(r.inference_peak_rss_mechanism.clone());
                hosts.push(format!(
                    "{} ({}/{})",
                    row.payload.host.hostname, row.payload.host.os, row.payload.host.arch
                ));
                backends.push(row.payload.backend_identity.clone());
            }

            quality.push(MethodShotQuality {
                method,
                shots,
                f_avg: summarise(&f_avg),
                macro_f1: summarise(&macro_f1),
                mcc: summarise(&mcc),
                per_seed,
            });

            let train_mechanisms = distinct_sorted(train_mechanisms.into_iter());
            let inference_mechanisms = distinct_sorted(inference_mechanisms.into_iter());
            let mut batch_sizes_sorted = batch_sizes;
            batch_sizes_sorted.sort_unstable();
            batch_sizes_sorted.dedup();

            resource.push(MethodShotResource {
                method,
                shots,
                hosts: distinct_sorted(hosts.into_iter()),
                backends: distinct_sorted(backends.into_iter()),
                train_wall_ms: summarise(&train_wall),
                cold_latency_ms: summarise(&cold),
                warm_latency_ms_median: summarise(&warm),
                throughput_rows_per_sec: summarise(&throughput),
                throughput_batch_sizes: batch_sizes_sorted,
                train_peak_rss_bytes: summarise(&train_peak),
                train_peak_rss_mechanism_classes: mechanism_classes(&train_mechanisms),
                train_peak_rss_mechanisms: train_mechanisms,
                inference_peak_rss_bytes: summarise(&inference_peak),
                inference_peak_rss_mechanism_classes: mechanism_classes(&inference_mechanisms),
                inference_peak_rss_mechanisms: inference_mechanisms,
                artifact_bytes: summarise(&artifact),
                deployable_total_bytes: summarise(&deployable),
            });
        }
    }

    let deltas = BENCH_SHOTS.iter().map(|shots| shot_delta(*shots, &by_cell)).collect();

    RunAggregate {
        contract_id: CLAIMS_CONTRACT_ID.to_string(),
        n_seeds: PAIRED_DESIGN_N,
        degrees_of_freedom: PAIRED_DESIGN_N - 1,
        t_crit_975_df9: T_CRIT_975_DF9,
        key_sequence,
        quality,
        deltas,
        resource,
    }
}

/// The distinct classes of a sorted mechanism list, deduplicated but order-preserving.
fn mechanism_classes(mechanisms: &[String]) -> Vec<MechanismClass> {
    let mut out: Vec<MechanismClass> =
        mechanisms.iter().map(|m| mechanism_class(m.as_str())).collect();
    out.dedup();
    out
}

/// The paired comparison at one shot level, computed with the 05-04 closed-form surface only.
fn shot_delta(shots: u32, by_cell: &BTreeMap<CellKey, &BenchRow>) -> ShotDelta {
    let mut setfit = Vec::with_capacity(PAIRED_DESIGN_N);
    let mut lora = Vec::with_capacity(PAIRED_DESIGN_N);
    let mut per_seed_deltas = Vec::with_capacity(PAIRED_DESIGN_N);

    for seed in BENCH_SEEDS {
        let s = by_cell.get(&CellKey::new(Method::Setfit, shots, seed));
        let l = by_cell.get(&CellKey::new(Method::Lora, shots, seed));
        if let (Some(s), Some(l)) = (s, l) {
            let sv = s.payload.quality.f_avg;
            let lv = l.payload.quality.f_avg;
            setfit.push(sv);
            lora.push(lv);
            let delta = sv - lv;
            per_seed_deltas.push(SeedDelta { seed, delta, delta_bits: delta.to_bits() });
        }
    }

    // ONE definition of the differences. `paired_ci95_df9` and `ttest_rel_f64` both take the two
    // series and derive `d̄`/`s_d` through the same private helper in `hypothesis.rs` (OPS-03),
    // so the interval and the statistic can never disagree about the moments.
    let ci = paired_ci95_df9(&setfit, &lora);
    let test = ttest_rel_f64(&setfit, &lora);

    match ci {
        Ok(ci) => ShotDelta {
            shots,
            per_seed_deltas,
            mean_delta: ci.mean_diff,
            std_delta: Some(ci.std_diff),
            ci95: Ci95 {
                low: Some(ci.low),
                high: Some(ci.high),
                half_width: Some(ci.half_width),
                std_err: Some(ci.std_err),
                null_reason: None,
            },
            t_statistic: test.as_ref().ok().map(|t| t.statistic),
            p_value: test.ok().map(|t| t.pvalue),
        },
        // THE DEGENERATE CASE, MADE VISIBLE. The point estimate is still reported — it is well
        // defined and it is what a reader wants — and the interval is STRUCTURALLY ABSENT with a
        // stated reason rather than serialized as a non-finite number that renders as `null`.
        Err(AprenderError::ZeroVarianceDifferences { constant_value, .. }) => ShotDelta {
            shots,
            per_seed_deltas,
            mean_delta: constant_value,
            std_delta: None,
            ci95: Ci95 {
                low: None,
                high: None,
                half_width: None,
                std_err: None,
                null_reason: Some(ZERO_VARIANCE_NULL_REASON.to_string()),
            },
            t_statistic: None,
            p_value: None,
        },
        // Unreachable through a VerifiedRunSet, which guarantees ten pairs at every shot level.
        // Reported as a degenerate interval with its own reason rather than panicking: a gate
        // that aborts tells a reader less than one that says which number is missing and why.
        Err(other) => ShotDelta {
            shots,
            per_seed_deltas,
            mean_delta: mean_f64(
                &setfit.iter().zip(lora.iter()).map(|(s, l)| s - l).collect::<Vec<f64>>(),
            )
            .unwrap_or_default(),
            std_delta: None,
            ci95: Ci95 {
                low: None,
                high: None,
                half_width: None,
                std_err: None,
                null_reason: Some(other.to_string()),
            },
            t_statistic: None,
            p_value: None,
        },
    }
}

#[cfg(test)]
#[path = "bench_gate_tests.rs"]
mod bench_gate_tests;
