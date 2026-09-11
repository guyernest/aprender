//! Claims-gate tests (plan 05-10, EVAL-04).
//!
//! Every test name starts `bench_gate_`, and the module path itself contains `bench_gate`, so
//! `cargo test -p aprender-train --lib --features setfit bench_gate` selects exactly this file.
//! The COUNT matters as much as the status: a name-filtered `cargo test` that matches nothing
//! prints `test result: ok. 0 passed` and exits 0 (CR-02), so the Make floor reads the matched
//! count out of the log rather than trusting `ok`.
//!
//! # How every negative here is built, and the ONE rule that governs the list
//!
//! Each is built by MUTATING a programmatically-generated VALID run, so the mutation is the only
//! difference between a green run and a red one — a hand-written broken fixture proves only that
//! some bytes are refused, never that THIS defect is what refused them.
//!
//! **THE SCOPE A NEGATIVE IS MUTATED AT IS PART OF WHAT IT PROVES.** Since the claims contract's
//! 2.0.0 narrowing (D-19) the ACTIVE expectation set is 40 cells, one method. A proof taken at
//! one scope does NOT transfer to another (CLAUDE.md Verification Discipline rule 4), which is
//! why the `scope` column below exists and why two entries deliberately overlap in shape while
//! differing in scope. The Makefile banner at the `setfit-bench-gate` leg carries the same
//! account; they must agree.
//!
//! Three scopes appear:
//!
//! * **`verify_run` / ACTIVE 40** — through the PUBLIC two-argument door, over the shipped
//!   expectation set. This is what a user runs.
//! * **`verify_run_scoped` / DEFERRED 80** — the two-method scope, `#[cfg(test)]`-gated. The
//!   shapes that exist ONLY in a two-method design (an unpaired pair, a forged LoRA ledger)
//!   need rows production code cannot construct. `D-ITEM-05-15` restores this arm; these
//!   negatives are retained, contract-bound and unexercised by any shipped door.
//! * **resolver helper, directly** — `resolve_committed_evidence_path` called with an evidence
//!   kind and a declared string. This holds the resolver's two stages; it does NOT hold the
//!   wiring that makes the resolver reachable from a row at all.
//!
//! # The negative inventory
//!
//! | # | doctored shape | asserted variant | scope | added by |
//! |---|---|---|---|---|
//! | 1 | a cell left `pending` (selective omission) | `incomplete_cell` | `verify_run` / 40 | 05-10, re-mutated 05-11 |
//! | 2 | a required evidence block removed from a row | `row_schema_refused` | `verify_run` / 40 | 05-10, re-mutated 05-11 |
//! | 3 | one pair's two rows on different selection manifests | `unpaired_selection` | deferred 80 | 05-10 |
//! | 4 | a payload byte edited without resealing | `row_digest_mismatch` | `verify_run` / 40 | 05-10, re-mutated 05-11 |
//! | 5 | post-test selection (lock rule; epochs_completed) | `post_test_selection` | `verify_run` / 40 and deferred 80 | 05-10, re-mutated 05-11 |
//! | 6 | FORGED PROVENANCE (two-line ledger; edited lock file) | `provenance_mismatch` | deferred 80 | 05-10 |
//! | 7 | a manifest declaring a second method's cell | `expectation_set_mismatch` | `verify_run` / 40 | 05-11 |
//! | 8 | a second method's ROW in a declared slot | `row_slot_mismatch` | `verify_run` / 40 | 05-11 |
//! | 9 | an escaping `lock_record_path` — absolute, `..`, last-component symlink | `evidence_path_escape` | `verify_run` / 40 | 05-15 |
//! | 10 | thirteen path shapes x two evidence kinds | `evidence_path_escape`, `evidence_file_missing`, `evidence_read_failed`, and three ACCEPTANCE rows | resolver helper | 05-15 |
//! | 11 | a deleted committed lock record | `evidence_file_missing` | `verify_run` / 40 | 05-15 |
//! | 12 | a row declaring a foreign `contract_id` | `row_schema_refused` | `verify_run` / 40 | 05-15 |
//! | 13 | a MANIFEST declaring a foreign `contract_id` | `row_schema_refused` | `verify_run` / 40 | 05-15 |
//! | 14 | a contract-pinned constant a row may not choose (three fields) | `row_schema_refused` | `verify_run` / 40 | 05-15 |
//! | 15 | TWO cells carrying an escaping path | `evidence_path_escape` on the FIRST in contract order | `verify_run` / 40 | 05-15 |
//!
//! Entries 9 and 10 overlap by design: three of entry 10's rows are the same SHAPES as entry
//! 9's, proven at a different scope. Deleting either for duplicating the other is exactly the
//! scope transfer rule 4 forbids. 05-16 and 05-17 each extend this table when they add theirs.
//!
//! Every negative in this file runs in a default `cargo test -p aprender-train --lib --features
//! setfit` invocation: no `#[ignore]`, no extra feature, no network, no fixture file. The
//! deferred-scope entries run too — what is deferred is the PRODUCTION arm, not the test.
//!
//! # What these negatives do NOT prove
//!
//! They prove the gate detects INCONSISTENT evidence, and since 05-15 that a row cannot point
//! the gate at bytes outside the benchmark directory. They prove nothing about TRUTHFUL
//! provenance: a producer holding both the rows and the lock/ledger files can still emit a
//! mutually consistent forgery. That residual is stated in `bench_gate.rs`'s module doc and in
//! the contract's own `selection_safety_evidence.residual_risk`, and it is repeated here so a
//! reader of the test list does not conclude more from a green suite than it supports.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tempfile::TempDir;

use super::*;
use crate::train::setfit::bench_row::ExpectationScope;
use crate::train::setfit::bench_row::{
    BenchLockRef, BenchRowPayload, HostIdentity, LoraEvidence, QualityBlock, ResourceBlock,
    SetfitEvidence, BENCH_ROW_SCHEMA_VERSION, CALIBRATION_SPLIT, WARMUP_COUNT,
};

// ===========================================================================================
// The synthetic run builder
// ===========================================================================================

/// The knobs the doctored negatives and the zero-variance test turn.
#[derive(Debug, Clone, Copy, Default)]
struct RunSpec {
    /// When set, every seed at this shot level produces the IDENTICAL paired delta, which is
    /// what `paired_ci95_df9` refuses with `ZeroVarianceDifferences`.
    zero_variance_shots: Option<u32>,
}

/// Position of `value` in `list`, as an `f64` multiplier.
fn index_of<T: PartialEq + Copy>(list: &[T], value: T) -> usize {
    list.iter().position(|item| *item == value).unwrap_or(0)
}

/// The synthetic headline metric for one cell.
///
/// Small, exactly-representable values chosen so the non-degenerate deltas VARY across seeds
/// (otherwise every shot level would take the zero-variance branch and the interval arm of the
/// aggregate would never be exercised) and the degenerate ones are bit-identical.
fn synthetic_f_avg(spec: RunSpec, cell: CellKey) -> f64 {
    let shots_index = index_of(&BENCH_SHOTS, cell.shots);
    let seed_index = index_of(&BENCH_SEEDS, cell.seed);
    if spec.zero_variance_shots == Some(cell.shots) {
        // 0.5 and 0.25 are exact in binary64, so all ten differences are bit-identical 0.25.
        return match cell.method {
            Method::Setfit => 0.5,
            Method::Lora => 0.25,
        };
    }
    #[allow(clippy::cast_precision_loss)]
    let (base, seed_step) = match cell.method {
        Method::Setfit => (0.50 + shots_index as f64 * 0.01, 0.001),
        Method::Lora => (0.40 + shots_index as f64 * 0.01, 0.002),
    };
    #[allow(clippy::cast_precision_loss)]
    {
        base + seed_index as f64 * seed_step
    }
}

/// The pairing key both methods of a `(shots, seed)` cell consume.
fn synthetic_selection_hash(shots: u32, seed: u32) -> String {
    format!("selection-manifest-s{shots}-seed{seed}")
}

/// The committed lock record's bytes for a SetFit cell.
///
/// The gate never PARSES this file — the contract's rule is `sha256(bytes(lock_record_path)) ==
/// lock.lock_hash` — so synthetic bytes are the honest fixture here: using a real
/// `SelectionLock` would test the lock module, which has its own suite, and would hide the fact
/// that this gate's guarantee is a digest over bytes rather than a re-parse.
fn synthetic_lock_bytes(cell: CellKey) -> Vec<u8> {
    format!(
        "{{\"schema\":\"selection-lock-v1\",\"cell\":\"{}\",\"chosen_artifact_sha256\":\"{}\"}}",
        cell.render(),
        "0".repeat(64)
    )
    .into_bytes()
}

/// One append-only ledger line for a LoRA cell.
fn synthetic_ledger_line(cell: CellKey) -> String {
    format!(
        "{{\"timestamp\":\"1970-01-01T00:00:00+00:00\",\"selection_manifest_hash\":\"{}\",\
         \"epochs_requested\":3,\"seed\":{},\"config_hash\":\"{}\"}}",
        synthetic_selection_hash(cell.shots, cell.seed),
        cell.seed,
        "1".repeat(64)
    )
}

/// A synthetic `QualityBlock`, every headline carrying its bits sibling.
fn synthetic_quality(spec: RunSpec, cell: CellKey) -> QualityBlock {
    let f_avg = synthetic_f_avg(spec, cell);
    let macro_f1 = f_avg - 0.05;
    let mcc = f_avg - 0.10;
    QualityBlock {
        f_avg,
        f_avg_bits: f_avg.to_bits(),
        macro_f1,
        macro_f1_bits: macro_f1.to_bits(),
        per_class_precision: vec![0.6, 0.5, 0.4],
        per_class_recall: vec![0.6, 0.5, 0.4],
        per_class_f1: vec![0.6, 0.5, 0.4],
        mcc,
        mcc_bits: mcc.to_bits(),
        confusion_matrix: vec![vec![10, 1, 1], vec![1, 10, 1], vec![1, 1, 10]],
        n_test_rows: 35,
        ordered_labels: vec!["none".to_string(), "against".to_string(), "favor".to_string()],
        ece_top_label_validation: 0.05,
        ece_top_label_validation_bits: 0.05_f64.to_bits(),
        brier_multiclass_validation: 0.30,
        brier_multiclass_validation_bits: 0.30_f64.to_bits(),
        calibration_split: CALIBRATION_SPLIT.to_string(),
    }
}

/// A synthetic `ResourceBlock`.
///
/// The two methods carry DIFFERENT train mechanisms on purpose — `vm_hwm` for the CPU SetFit
/// host and `sysinfo_sampled_10hz` for the GPU LoRA host — because D-09 puts them on two hosts
/// and the mixed-mechanism case is the one the renderer has to label. A fixture where both sides
/// happened to agree would let the comparability machinery go untested.
fn synthetic_resource(cell: CellKey) -> ResourceBlock {
    let seed_index = index_of(&BENCH_SEEDS, cell.seed);
    #[allow(clippy::cast_precision_loss)]
    let jitter = seed_index as f64;
    match cell.method {
        Method::Setfit => ResourceBlock {
            train_wall_ms: 1_000 + seed_index as u64,
            cold_latency_ms: 12.0 + jitter,
            cold_measured_in_child_process: true,
            warm_latency_ms_median: 4.0 + jitter,
            throughput_rows_per_sec: 200.0 + jitter,
            throughput_batch_size: 32,
            warmup_count: WARMUP_COUNT,
            train_peak_rss_bytes: 500_000_000 + seed_index as u64,
            train_peak_rss_mechanism: "vm_hwm".to_string(),
            inference_peak_rss_bytes: 200_000_000 + seed_index as u64,
            inference_peak_rss_mechanism: MECHANISM_CHILD_MAX_RSS_VM_HWM.to_string(),
            peak_rss_sample_interval_hz: None,
            artifact_bytes: 90_000_000,
            deployable_total_bytes: 90_000_000,
        },
        Method::Lora => ResourceBlock {
            train_wall_ms: 90_000 + seed_index as u64,
            cold_latency_ms: 400.0 + jitter,
            cold_measured_in_child_process: true,
            warm_latency_ms_median: 60.0 + jitter,
            throughput_rows_per_sec: 20.0 + jitter,
            throughput_batch_size: 32,
            warmup_count: WARMUP_COUNT,
            train_peak_rss_bytes: 40_000_000_000 + seed_index as u64,
            train_peak_rss_mechanism: "sysinfo_sampled_10hz".to_string(),
            inference_peak_rss_bytes: 19_000_000_000 + seed_index as u64,
            inference_peak_rss_mechanism: MECHANISM_CHILD_MAX_RSS_VM_HWM.to_string(),
            peak_rss_sample_interval_hz: Some(10),
            artifact_bytes: 40_000_000,
            deployable_total_bytes: 18_040_000_000,
        },
    }
}

/// The host a cell ran on. Two hosts by design (D-09), never pooled.
fn synthetic_host(method: Method) -> HostIdentity {
    match method {
        Method::Setfit => HostIdentity {
            hostname: "local-cpu".to_string(),
            os: "macos".to_string(),
            arch: "aarch64".to_string(),
        },
        Method::Lora => HostIdentity {
            hostname: "lambda-vector".to_string(),
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
        },
    }
}

/// The full payload for one cell, with its evidence block already digest-consistent with the
/// lock or ledger bytes the builder is about to commit.
fn synthetic_payload(spec: RunSpec, cell: CellKey) -> BenchRowPayload {
    let evidence = match cell.method {
        Method::Setfit => MethodEvidence::Setfit(SetfitEvidence {
            evidence_table_hash: "a".repeat(64),
            apr_artifact_sha256: "b".repeat(64),
            lock: BenchLockRef {
                lock_hash: sha256_hex(&synthetic_lock_bytes(cell)),
                role: "written".to_string(),
                rule: SelectionRule::MaxMetricLowestIndexTieBreak.tag().to_string(),
                lock_record_path: format!(
                    "{LOCKS_DIR}/{}-s{}-seed{}.lock.json",
                    cell.method.tag(),
                    cell.shots,
                    cell.seed
                ),
            },
        }),
        Method::Lora => {
            let ledger = format!("{}\n", synthetic_ledger_line(cell));
            MethodEvidence::Lora(LoraEvidence {
                base_model_sha256: "c".repeat(64),
                base_model_bytes: 18_000_000_000,
                adapter_sha256: "d".repeat(64),
                epochs_requested: 3,
                epochs_completed: 3,
                early_stopping_disabled: true,
                val_split: 0.0,
                no_selection_attestation: true,
                candidate_ledger_sha256: sha256_hex(ledger.as_bytes()),
                candidates_trained: 1,
                candidate_ledger_path: format!(
                    "{LEDGER_DIR}/{}-s{}-seed{}.jsonl",
                    cell.method.tag(),
                    cell.shots,
                    cell.seed
                ),
            })
        }
    };

    BenchRowPayload {
        schema_version: BENCH_ROW_SCHEMA_VERSION,
        contract_id: CLAIMS_CONTRACT_ID.to_string(),
        method: cell.method,
        shots: cell.shots,
        seed: cell.seed,
        dataset_revision: "4fbd22cd78421f05b1ecdb4fc5725bc7a7bd8f66".to_string(),
        dataset_fingerprint: "e".repeat(64),
        model_revision: "f".repeat(40),
        selection_manifest_hash: synthetic_selection_hash(cell.shots, cell.seed),
        backend_identity: match cell.method {
            Method::Setfit => "cpu:trueno:simd".to_string(),
            Method::Lora => "gpu:cuda:cublas".to_string(),
        },
        host: synthetic_host(cell.method),
        quality: synthetic_quality(spec, cell),
        resource: synthetic_resource(cell),
        evidence,
    }
}

/// Which scope a synthetic run on disk was built for, READ FROM THE DISK.
///
/// Detected rather than threaded through thirty call sites: a second method's row file can
/// only exist if the directory was built for the deferred scope, so the disk already carries
/// the answer and a parameter would just be a second, drift-prone statement of it.
fn scope_of(root: &Path) -> ExpectationScope {
    let deferred_marker =
        root.join(ROWS_DIR).join(row_file_name(CellKey::new(Method::Lora, 8, 13)));
    if deferred_marker.exists() {
        ExpectationScope::DeferredTwoMethod
    } else {
        ExpectationScope::Active
    }
}

/// Write a complete, VALID 40-cell ACTIVE-scope benchmark directory.
///
/// Returns the temp dir; the manifest is derived from what is on disk by [`manifest_for`], so a
/// mutation helper can re-derive it after editing a row rather than fighting `record`'s
/// deliberate refusal to overwrite a differing digest.
fn write_valid_run(spec: RunSpec) -> TempDir {
    write_valid_run_scoped(spec, ExpectationScope::Active)
}

/// Write a complete, VALID benchmark directory for the DEFERRED two-method scope (80 cells).
///
/// The two doctored shapes that exist ONLY in a two-method design — an unpaired selection hash
/// and a forged LoRA provenance — need rows production code cannot construct, which is exactly
/// why the deferred scope is retained rather than deleted.
fn write_valid_run_deferred(spec: RunSpec) -> TempDir {
    write_valid_run_scoped(spec, ExpectationScope::DeferredTwoMethod)
}

fn write_valid_run_scoped(spec: RunSpec, scope: ExpectationScope) -> TempDir {
    let dir = TempDir::new().expect("a temp dir");
    let root = dir.path();
    fs::create_dir_all(root.join(ROWS_DIR)).expect("rows dir");
    fs::create_dir_all(root.join(LOCKS_DIR)).expect("locks dir");
    fs::create_dir_all(root.join(LEDGER_DIR)).expect("ledger dir");

    for cell in RunManifest::expectation_for(scope) {
        match cell.method {
            Method::Setfit => {
                let relative = format!(
                    "{LOCKS_DIR}/{}-s{}-seed{}.lock.json",
                    cell.method.tag(),
                    cell.shots,
                    cell.seed
                );
                fs::write(root.join(relative), synthetic_lock_bytes(cell)).expect("lock write");
            }
            Method::Lora => {
                let relative = format!(
                    "{LEDGER_DIR}/{}-s{}-seed{}.jsonl",
                    cell.method.tag(),
                    cell.shots,
                    cell.seed
                );
                fs::write(root.join(relative), format!("{}\n", synthetic_ledger_line(cell)))
                    .expect("ledger write");
            }
        }
        let row = BenchRow::new(synthetic_payload(spec, cell));
        fs::write(
            root.join(ROWS_DIR).join(row_file_name(cell)),
            row.to_file_bytes().expect("row serializes"),
        )
        .expect("row write");
    }
    dir
}

/// Declare a manifest and record whatever rows are actually on disk.
///
/// A missing row file leaves its cell `pending`, which is exactly how selective omission looks
/// to the gate — so negative 1 is produced by DELETING a file rather than by hand-editing a
/// status field.
fn manifest_for(root: &Path) -> RunManifest {
    let scope = scope_of(root);
    let mut manifest = RunManifest::declare_for(scope);
    for cell in RunManifest::expectation_for(scope) {
        let path = root.join(ROWS_DIR).join(row_file_name(cell));
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(row) = BenchRow::from_bytes(&bytes) else {
            // A doctored row that no longer parses cannot have its digest recorded honestly.
            // Recording the file's own digest keeps the manifest a truthful statement about
            // what is on disk and lets the ROW-level refusal be the thing under test.
            manifest.record(cell, &sha256_hex(&bytes)).expect("recording a fresh cell");
            continue;
        };
        manifest.record(cell, &row.semantic_hash).expect("recording a fresh cell");
    }
    manifest
}

/// Read a row file as untyped JSON, so a REQUIRED field can be removed (which the typed struct
/// cannot express).
fn read_row_value(root: &Path, cell: CellKey) -> serde_json::Value {
    let bytes = fs::read(root.join(ROWS_DIR).join(row_file_name(cell))).expect("row read");
    serde_json::from_slice(&bytes).expect("row is JSON")
}

/// Write an untyped row value back, pretty, with the trailing newline the writer uses.
fn write_row_value(root: &Path, cell: CellKey, value: &serde_json::Value) {
    let mut bytes = serde_json::to_vec_pretty(value).expect("row serializes");
    bytes.push(b'\n');
    fs::write(root.join(ROWS_DIR).join(row_file_name(cell)), bytes).expect("row write");
}

/// Mutate a row's typed payload and RESEAL it, so the envelope digest stays honest and the
/// defect under test is the one the mutation introduced rather than a digest mismatch.
fn reseal_row(root: &Path, cell: CellKey, mutate: impl FnOnce(&mut BenchRowPayload)) {
    let path = root.join(ROWS_DIR).join(row_file_name(cell));
    let bytes = fs::read(&path).expect("row read");
    let row = BenchRow::from_bytes(&bytes).expect("the valid fixture parses");
    let mut payload = row.payload;
    mutate(&mut payload);
    let resealed = BenchRow::new(payload);
    fs::write(&path, resealed.to_file_bytes().expect("row serializes")).expect("row write");
}

/// Verify, and require a refusal — reported in one line.
///
/// `expect_err` would print the `Ok` value's `Debug`, and a `VerifiedRunSet`'s `Debug` is eighty
/// rows of nested structs. A diagnostic nobody can read is a diagnostic that does not exist, so
/// the accepted case reports the COUNT and the doctored shape's name and stops there.
fn refuse(manifest: &RunManifest, root: &Path, doctored: &str) -> BenchGateError {
    match verify_scoped(manifest, root) {
        Ok(set) => panic!(
            "the doctored run `{doctored}` was ACCEPTED ({} rows verified). The gate did not \
             refuse it, so this negative is proving nothing",
            set.len()
        ),
        Err(error) => error,
    }
}

/// Verify against whichever scope the directory on disk was built for.
///
/// The ACTIVE-scope cases go through the PUBLIC two-argument `verify_run`, so what they prove
/// is proven about the shipped door and not about a test-only entry point.
fn verify_scoped(manifest: &RunManifest, root: &Path) -> Result<VerifiedRunSet, BenchGateError> {
    match scope_of(root) {
        ExpectationScope::Active => verify_run(manifest, root),
        scope => verify_run_scoped(manifest, root, scope),
    }
}

/// The cell every negative doctors, so the failures are directly comparable.
const TARGET_SETFIT: CellKey = CellKey::new(Method::Setfit, 16, 29);
/// Its LoRA partner.
const TARGET_LORA: CellKey = CellKey::new(Method::Lora, 16, 29);

// ===========================================================================================
// The positive control
// ===========================================================================================

#[test]
fn bench_gate_accepts_a_complete_valid_run() {
    let dir = write_valid_run(RunSpec::default());
    let manifest = manifest_for(dir.path());
    let verified = verify_run(&manifest, dir.path()).expect("the valid synthetic run verifies");
    assert_eq!(
        verified.len(),
        EXPECTED_CELLS,
        "a verified set is the whole contracted matrix or it is nothing"
    );
    assert!(!verified.is_empty());
    assert!(verified.row(TARGET_SETFIT).is_some());
}

#[test]
fn bench_gate_verified_rows_are_in_the_contract_order() {
    let dir = write_valid_run(RunSpec::default());
    let manifest = manifest_for(dir.path());
    let verified = verify_run(&manifest, dir.path()).expect("verifies");
    let keys: Vec<CellKey> = verified.rows().iter().map(|(cell, _)| *cell).collect();
    assert_eq!(keys, RunManifest::expectation());
}

// ===========================================================================================
// DOCTORED NEGATIVE 1 — a selectively omitted cell
// ===========================================================================================

#[test]
fn bench_gate_refuses_a_missing_cell_naming_it() {
    let dir = write_valid_run(RunSpec::default());
    fs::remove_file(dir.path().join(ROWS_DIR).join(row_file_name(TARGET_SETFIT)))
        .expect("remove the row");
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "an omitted cell is a refusal");
    assert_eq!(error.variant_tag(), "incomplete_cell");
    assert_eq!(error.cell(), Some(TARGET_SETFIT.render().as_str()));
    assert!(
        error.to_string().contains(&TARGET_SETFIT.render()),
        "the refusal must name the cell: {error}"
    );
}

// ===========================================================================================
// DOCTORED NEGATIVE 2 — a trimmed row (a required block removed)
// ===========================================================================================

#[test]
fn bench_gate_refuses_a_trimmed_row_whose_evidence_block_was_removed() {
    let dir = write_valid_run(RunSpec::default());
    let mut value = read_row_value(dir.path(), TARGET_SETFIT);
    // Remove the `lock` sub-block. A `setfit` row without it can still be READ as JSON and
    // still names a plausible cell — which is exactly why the refusal has to be structural.
    value
        .get_mut("payload")
        .and_then(|p| p.get_mut("evidence"))
        .and_then(|e| e.get_mut("setfit"))
        .and_then(serde_json::Value::as_object_mut)
        .expect("the fixture carries a setfit evidence block")
        .remove("lock");
    write_row_value(dir.path(), TARGET_SETFIT, &value);
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "a trimmed row is a refusal");
    assert_eq!(error.variant_tag(), "row_schema_refused");
    assert_eq!(error.cell(), Some(TARGET_SETFIT.render().as_str()));
}

// ===========================================================================================
// DOCTORED NEGATIVE 3 — a mismatched sampled-ID hash in one pair
// ===========================================================================================

#[test]
fn bench_gate_refuses_a_pair_measured_on_different_selection_manifests() {
    let dir = write_valid_run_deferred(RunSpec::default());
    // Reseal, so the row is internally perfect. The ONLY defect is that the two halves of one
    // pair consumed different sampled IDs — which is PF-007's incomparable comparison, and it
    // is invisible to every per-row check.
    reseal_row(dir.path(), TARGET_LORA, |payload| {
        payload.selection_manifest_hash = "a-different-draw-entirely".to_string();
    });
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "an unpaired pair is a refusal");
    assert_eq!(error.variant_tag(), "unpaired_selection");
    let rendered = error.to_string();
    assert!(rendered.contains("shots 16"), "{rendered}");
    assert!(rendered.contains("seed 29"), "{rendered}");
    assert!(rendered.contains("a-different-draw-entirely"), "{rendered}");
}

// ===========================================================================================
// DOCTORED NEGATIVE 4 — bit-flipped row bytes
// ===========================================================================================

#[test]
fn bench_gate_refuses_a_row_whose_payload_bytes_were_edited() {
    let dir = write_valid_run(RunSpec::default());
    let mut value = read_row_value(dir.path(), TARGET_SETFIT);
    // Edit a payload field and DO NOT reseal: the envelope still claims the old digest.
    *value
        .get_mut("payload")
        .and_then(|p| p.get_mut("dataset_revision"))
        .expect("the fixture carries a dataset revision") =
        serde_json::Value::String("0000000000000000000000000000000000000000".to_string());
    write_row_value(dir.path(), TARGET_SETFIT, &value);
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "edited bytes are a refusal");
    assert_eq!(error.variant_tag(), "row_digest_mismatch");
    assert_eq!(error.cell(), Some(TARGET_SETFIT.render().as_str()));
}

// ===========================================================================================
// DOCTORED NEGATIVE 5 — post-test selection, on both sides
// ===========================================================================================

#[test]
fn bench_gate_refuses_a_setfit_row_whose_lock_rule_is_not_the_committed_one() {
    let dir = write_valid_run(RunSpec::default());
    reseal_row(dir.path(), TARGET_SETFIT, |payload| {
        if let MethodEvidence::Setfit(evidence) = &mut payload.evidence {
            evidence.lock.rule = "best_observed_on_test".to_string();
        }
    });
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "an unknown rule is a refusal");
    assert_eq!(error.variant_tag(), "post_test_selection");
    assert!(
        error.to_string().contains("lock.rule"),
        "the refusal must name the failing conjunct: {error}"
    );
}

#[test]
fn bench_gate_refuses_a_lora_row_that_completed_fewer_epochs_than_it_requested() {
    let dir = write_valid_run_deferred(RunSpec::default());
    reseal_row(dir.path(), TARGET_LORA, |payload| {
        if let MethodEvidence::Lora(evidence) = &mut payload.evidence {
            evidence.epochs_completed = 1;
        }
    });
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "a short run is a refusal");
    assert_eq!(error.variant_tag(), "post_test_selection");
    let rendered = error.to_string();
    assert!(rendered.contains("epochs_completed"), "{rendered}");
    assert!(rendered.contains(&TARGET_LORA.render()), "{rendered}");
}

#[test]
fn bench_gate_refuses_every_conjunct_of_the_lora_attestation_separately() {
    // Each conjunct closes a DIFFERENT route to a post-hoc choice, and any one alone is
    // satisfiable while the claim is false — so each is exercised rather than one standing in
    // for the set.
    let mutations: Vec<(&str, fn(&mut LoraEvidence))> = vec![
        ("early_stopping_disabled", |e| {
            e.early_stopping_disabled = false;
        }),
        ("val_split", |e| e.val_split = 0.1),
        ("no_selection_attestation", |e| {
            e.no_selection_attestation = false;
        }),
    ];
    for (conjunct, mutate) in mutations {
        let dir = write_valid_run_deferred(RunSpec::default());
        reseal_row(dir.path(), TARGET_LORA, |payload| {
            if let MethodEvidence::Lora(evidence) = &mut payload.evidence {
                mutate(evidence);
            }
        });
        let manifest = manifest_for(dir.path());
        let Err(error) = verify_scoped(&manifest, dir.path()) else {
            panic!("the `{conjunct}` conjunct must be refused, and it was not");
        };
        assert_eq!(
            error.variant_tag(),
            "post_test_selection",
            "`{conjunct}` must be refused as post-test selection, got: {error}"
        );
        assert!(
            error.to_string().contains(conjunct),
            "the refusal must name the failing conjunct `{conjunct}`: {error}"
        );
    }
}

// ===========================================================================================
// DOCTORED NEGATIVE 6 — FORGED PROVENANCE, two sub-cases
// ===========================================================================================

#[test]
fn bench_gate_refuses_a_ledger_carrying_a_second_candidate_the_row_does_not_declare() {
    let dir = write_valid_run_deferred(RunSpec::default());
    let ledger_path = dir.path().join(format!(
        "{LEDGER_DIR}/{}-s{}-seed{}.jsonl",
        TARGET_LORA.method.tag(),
        TARGET_LORA.shots,
        TARGET_LORA.seed
    ));
    // TWO lines, and the row's `candidate_ledger_sha256` is UPDATED to match the new bytes —
    // so the digest check passes and only COUNTING catches it. That is the sharp form of this
    // forgery: a producer who edits both the file and the field it claims.
    let forged =
        format!("{}\n{}\n", synthetic_ledger_line(TARGET_LORA), synthetic_ledger_line(TARGET_LORA));
    fs::write(&ledger_path, forged.as_bytes()).expect("ledger write");
    reseal_row(dir.path(), TARGET_LORA, |payload| {
        if let MethodEvidence::Lora(evidence) = &mut payload.evidence {
            evidence.candidate_ledger_sha256 = sha256_hex(forged.as_bytes());
        }
    });
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "a two-line ledger is a refusal");
    assert_eq!(error.variant_tag(), "provenance_mismatch");
    let rendered = error.to_string();
    assert!(rendered.contains("candidates_trained = 1"), "{rendered}");
    assert!(rendered.contains("2 ledger line(s)"), "{rendered}");
    assert!(
        rendered.contains(&ledger_path.display().to_string()),
        "the refusal must name the FILE whose bytes disagreed: {rendered}"
    );
}

#[test]
fn bench_gate_refuses_a_setfit_row_whose_committed_lock_file_was_edited() {
    let dir = write_valid_run(RunSpec::default());
    let lock_path = dir.path().join(format!(
        "{LOCKS_DIR}/{}-s{}-seed{}.lock.json",
        TARGET_SETFIT.method.tag(),
        TARGET_SETFIT.shots,
        TARGET_SETFIT.seed
    ));
    let mut bytes = fs::read(&lock_path).expect("lock read");
    // One byte, inside the committed evidence. The row's `lock_hash` is untouched, so the row
    // and the file now disagree — and the row alone still looks perfect.
    let last = bytes.len() - 2;
    bytes[last] = b'9';
    fs::write(&lock_path, &bytes).expect("lock write");
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "an edited lock is a refusal");
    assert_eq!(error.variant_tag(), "provenance_mismatch");
    let rendered = error.to_string();
    assert!(rendered.contains("lock_hash"), "{rendered}");
    assert!(
        rendered.contains(&lock_path.display().to_string()),
        "the refusal must name the FILE whose bytes disagreed: {rendered}"
    );
}

#[test]
fn bench_gate_refuses_a_ledger_transplanted_from_another_cell() {
    let dir = write_valid_run_deferred(RunSpec::default());
    let ledger_path = dir.path().join(format!(
        "{LEDGER_DIR}/{}-s{}-seed{}.jsonl",
        TARGET_LORA.method.tag(),
        TARGET_LORA.shots,
        TARGET_LORA.seed
    ));
    let other = CellKey::new(Method::Lora, 16, 31);
    let transplanted = format!("{}\n", synthetic_ledger_line(other));
    fs::write(&ledger_path, transplanted.as_bytes()).expect("ledger write");
    reseal_row(dir.path(), TARGET_LORA, |payload| {
        if let MethodEvidence::Lora(evidence) = &mut payload.evidence {
            evidence.candidate_ledger_sha256 = sha256_hex(transplanted.as_bytes());
        }
    });
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "a transplanted ledger is a refusal");
    assert_eq!(error.variant_tag(), "provenance_mismatch");
    assert!(error.to_string().contains("different selection manifest"), "{error}");
}

// ===========================================================================================
// GAP 1 — THE ACTIVE 40-CELL SCOPE ESCAPE SWEEP (plan 05-15 task 2)
//
// Verifier gap 1's second `missing:` bullet names THREE bounds — an absolute path, a `..`
// traversal, and a symlink out of `bench_dir` — and requires a RED-turning negative for each,
// RE-MUTATED at the ACTIVE scope. The helper-level case table further down drives
// `resolve_committed_evidence_path` DIRECTLY, which is a different scope: it holds the
// resolver's two stages, not the wiring that makes the resolver reachable from a row at all.
// CLAUDE.md Verification Discipline rule 4 — extending a guard's SCOPE requires re-mutating in
// the new scope — is why both exist and why neither may be deleted for duplicating the other.
// ===========================================================================================

/// One escaping path SHAPE, as something the materializer builds rather than a hardcoded
/// literal — the two temp directories' real names are only known at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeShape {
    /// An ABSOLUTE declared path. `Path::join` DISCARDS the base when its argument is
    /// absolute, so the benchmark directory stops being part of the resolution at all.
    Absolute,
    /// A RELATIVE declared path that climbs out with `..`. `Path::join` never resolves `..`;
    /// the kernel does, at open time, after the gate has stopped looking.
    ParentTraversal,
    /// A relative path INSIDE the benchmark directory whose LAST component is a symlink to a
    /// file outside it. No syntactic check can see this one — only canonicalization can.
    LastComponentSymlink,
}

/// The three bounds gap 1 names, as a TABLE. A fourth bound is added as a row here, never as
/// another test function (CLAUDE.md Verification Discipline rule 7).
const ACTIVE_SCOPE_ESCAPE_SHAPES: [(&str, EscapeShape, &str); 3] = [
    (
        "absolute",
        EscapeShape::Absolute,
        "an absolute component makes `Path::join` discard the benchmark directory entirely",
    ),
    (
        "parent_traversal",
        EscapeShape::ParentTraversal,
        "`Path::join` never resolves `..`, so the climb happens at open time, unseen",
    ),
    (
        "last_component_symlink",
        EscapeShape::LastComponentSymlink,
        "the declared string is entirely well-formed; only the canonical form leaves the tree",
    ),
];

/// Build the declared string for one shape, creating whatever filesystem object it needs.
///
/// `outside_file` already holds the cell's OWN attested lock bytes, and the in-tree lock record
/// has already been deleted by the caller. That pairing is load-bearing: it reproduces
/// spot-check E exactly, so the PRE-FIX gate hashes the escape target, MATCHES, and returns
/// `Ok`. Aim any of these rows at `/etc/passwd` instead and the pre-fix run refuses with
/// `provenance_mismatch` — still red, but red for the wrong reason, proving the escape was
/// DETECTED rather than that it SUCCEEDED.
fn materialize(shape: EscapeShape, bench_dir: &Path, outside_file: &Path) -> String {
    match shape {
        EscapeShape::Absolute => outside_file.display().to_string(),
        EscapeShape::ParentTraversal => {
            // Built from the two temp dirs' REAL names at runtime. They are siblings under
            // `$TMPDIR`, so one `..` from the bench dir lands beside the scratch dir.
            let outside_dir = outside_file.parent().expect("the scratch file has a parent");
            format!(
                "../{}/{}",
                outside_dir.file_name().expect("the scratch dir has a name").to_string_lossy(),
                outside_file.file_name().expect("the scratch file has a name").to_string_lossy(),
            )
        }
        EscapeShape::LastComponentSymlink => {
            let relative = format!("{LOCKS_DIR}/escape-via-symlink.lock.json");
            let link = bench_dir.join(&relative);
            // `#[cfg(unix)]` gates the MATERIALIZER, never the table: a table that silently
            // shrinks on some host is a coverage loss no test-count floor can see.
            #[cfg(unix)]
            std::os::unix::fs::symlink(outside_file, &link).expect("symlink into the scratch dir");
            #[cfg(not(unix))]
            let _ = &link;
            relative
        }
    }
}

#[test]
fn bench_gate_refuses_every_escaping_lock_path_shape_at_the_active_scope() {
    let mut observations: Vec<(&str, String)> = Vec::new();

    for (label, shape, why) in ACTIVE_SCOPE_ESCAPE_SHAPES {
        let dir = write_valid_run(RunSpec::default());
        let outside = TempDir::new().expect("a scratch dir OUTSIDE the bench dir");
        let outside_file = outside.path().join("anywhere.json");

        // 1. The escape target holds the very bytes the row attests.
        fs::write(&outside_file, synthetic_lock_bytes(TARGET_SETFIT)).expect("scratch lock write");
        // 2. The IN-TREE lock record is deleted, so nothing legitimate can satisfy the row.
        fs::remove_file(dir.path().join(format!(
            "{LOCKS_DIR}/{}-s{}-seed{}.lock.json",
            TARGET_SETFIT.method.tag(),
            TARGET_SETFIT.shots,
            TARGET_SETFIT.seed
        )))
        .expect("delete the in-tree lock record");
        // 3. The row points at the materialized escape.
        let declared = materialize(shape, dir.path(), &outside_file);
        reseal_row(dir.path(), TARGET_SETFIT, |payload| {
            if let MethodEvidence::Setfit(evidence) = &mut payload.evidence {
                evidence.lock.lock_record_path = declared.clone();
            }
        });
        // 4. The manifest re-records the resealed row, so the run is otherwise self-consistent.
        let manifest = manifest_for(dir.path());

        let (observed, rendered) = match verify_run(&manifest, dir.path()) {
            Ok(set) => (format!("Ok({} rows verified)", set.len()), String::new()),
            Err(error) => (error.variant_tag().to_string(), error.to_string()),
        };
        // PRINTED PER SHAPE. The sweep is one test function but three bounds, and a single
        // combined verdict would hide a row that was never red.
        println!(
            "[bench_gate] ACTIVE_SCOPE_ESCAPE shape={label} declared={declared} \
             observed={observed} ({why})"
        );
        if observed == "evidence_path_escape" {
            assert!(
                rendered.contains(&declared),
                "the `{label}` refusal must quote the declared string verbatim: {rendered}"
            );
        }
        observations.push((label, observed));
    }

    let failures: Vec<&(&str, String)> =
        observations.iter().filter(|(_, observed)| observed != "evidence_path_escape").collect();
    assert!(
        failures.is_empty(),
        "every bound gap 1 names must be refused as `evidence_path_escape` through verify_run \
         at the ACTIVE 40-cell scope; these were not: {failures:?}"
    );
}

// ===========================================================================================
// WR-06 — a missing LOCK RECORD names its own kind, not the row's (plan 05-15 task 2)
// ===========================================================================================

#[test]
fn bench_gate_refuses_a_missing_lock_record_as_its_own_kind_not_as_a_missing_row() {
    let dir = write_valid_run(RunSpec::default());
    let lock_path = dir.path().join(format!(
        "{LOCKS_DIR}/{}-s{}-seed{}.lock.json",
        TARGET_SETFIT.method.tag(),
        TARGET_SETFIT.shots,
        TARGET_SETFIT.seed
    ));
    // The row is untouched and still names the committed spelling. ONLY the lock file is gone,
    // which is the honest shape of this defect: the row is fine, the evidence is not.
    fs::remove_file(&lock_path).expect("delete the in-tree lock record");
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "a missing lock record is a refusal");
    assert_eq!(error.variant_tag(), "evidence_file_missing");
    assert_eq!(error.cell(), Some(TARGET_SETFIT.render().as_str()));
    let rendered = error.to_string();
    assert!(rendered.contains("lock record"), "the refusal must name the KIND: {rendered}");
    assert!(
        rendered.contains(&lock_path.display().to_string()),
        "the refusal must name the FILE: {rendered}"
    );
    assert!(
        !rendered.contains("restore the row file"),
        "a missing lock record must not carry the ROW remedy — that is WR-06: {rendered}"
    );

    // And the ROW kind is UNCHANGED, message and all. This is a typing correction, not a
    // behaviour change for the row path: spot-check A's output must still read as it did.
    let dir = write_valid_run(RunSpec::default());
    let row_path = dir.path().join(ROWS_DIR).join(row_file_name(TARGET_SETFIT));
    let error = read_evidence(TARGET_SETFIT, EvidenceKind::Row, &row_path.with_extension("gone"))
        .expect_err("a missing row file is a refusal");
    assert_eq!(error.variant_tag(), "row_file_missing");
    assert!(
        error.to_string().contains("restore the row file"),
        "the ROW remedy is kept VERBATIM: {error}"
    );
}

// ===========================================================================================
// THE THIRD ENUMERATED FIELD — a foreign contract id, on both sides (plan 05-15 task 2)
// ===========================================================================================

#[test]
fn bench_gate_refuses_a_row_declaring_a_foreign_contract_id() {
    // Found by `05-15-gate-input-surface.md`, not by a probe: nothing anywhere compared a
    // row's declared contract against the constant, while `aggregate` stamped the published
    // payload with that constant regardless.
    let dir = write_valid_run(RunSpec::default());
    reseal_row(dir.path(), TARGET_SETFIT, |payload| {
        payload.contract_id = "setfit-benchmark-claims-v99".to_string();
    });
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "a foreign contract id is a refusal");
    assert_eq!(error.variant_tag(), "row_schema_refused");
    assert_eq!(error.cell(), Some(TARGET_SETFIT.render().as_str()));
    let rendered = error.to_string();
    assert!(rendered.contains("setfit-benchmark-claims-v99"), "names the DECLARED id: {rendered}");
    assert!(rendered.contains(CLAIMS_CONTRACT_ID), "names the EXPECTED id: {rendered}");
}

#[test]
fn bench_gate_refuses_a_manifest_declaring_a_foreign_contract_id() {
    let dir = write_valid_run(RunSpec::default());
    let mut manifest = manifest_for(dir.path());
    manifest.payload.contract_id = "setfit-benchmark-claims-v99".to_string();
    // Resealed by hand so the ENVELOPE digest does not fire first and mask the comparison.
    manifest.semantic_hash =
        sha256_hex(&manifest.payload.to_canonical_bytes().expect("payload serializes"));

    let error = refuse(&manifest, dir.path(), "a foreign manifest contract id is a refusal");
    assert_eq!(error.variant_tag(), "row_schema_refused");
    let rendered = error.to_string();
    assert!(rendered.contains("setfit-benchmark-claims-v99"), "names the DECLARED id: {rendered}");
    assert!(rendered.contains(CLAIMS_CONTRACT_ID), "names the EXPECTED id: {rendered}");
}

#[test]
fn bench_gate_refuses_each_contract_pinned_constant_a_row_may_not_choose() {
    // The other three (iii) entries the enumeration found: fields the contract pins to one
    // value each, checked only at EMISSION time and never when the gate READ a committed row.
    // Swept as a table rather than as three functions (CLAUDE.md Verification Discipline
    // rule 7). `throughput_batch_size` is deliberately absent — the contract pins no value for
    // it, so there is nothing contract-derived to compare against.
    let mutations: Vec<(&str, fn(&mut BenchRowPayload))> = vec![
        ("calibration_split", |p| p.quality.calibration_split = "test".to_string()),
        ("warmup_count", |p| p.resource.warmup_count = WARMUP_COUNT + 7),
        ("cold_measured_in_child_process", |p| {
            p.resource.cold_measured_in_child_process = false;
        }),
    ];
    for (field, mutate) in mutations {
        let dir = write_valid_run(RunSpec::default());
        reseal_row(dir.path(), TARGET_SETFIT, mutate);
        let manifest = manifest_for(dir.path());

        let error = refuse(&manifest, dir.path(), field);
        assert_eq!(
            error.variant_tag(),
            "row_schema_refused",
            "`{field}` must be refused as a schema-domain violation, got: {error}"
        );
        assert!(
            error.to_string().contains(field),
            "the refusal must name the offending field `{field}`: {error}"
        );
    }
}

// ===========================================================================================
// THE PATH-SHAPE CASE TABLE (plan 05-15 task 3)
//
// Scope: this table drives `resolve_committed_evidence_path` DIRECTLY, over BOTH evidence
// kinds. The ACTIVE-scope sweep above drives three of these same shapes through `verify_run`
// over 40 cells. NEITHER PROOF TRANSFERS TO THE OTHER (CLAUDE.md Verification Discipline rule
// 4): this one holds the resolver's two stages across every refusal shape, three acceptance
// rows and two behaviour-preserving rows; that one holds the wiring that makes the resolver
// reachable from a row at all. Deleting a row here because "the sweep covers it" is exactly
// the scope transfer rule 4 forbids.
//
// Ship the TABLE and re-run it, rather than re-reading the pattern (rule 7). A fourth shape is
// a new row, never a new test function.
// ===========================================================================================

/// How a case's declared string is built for a given [`EvidenceKind`].
///
/// A shape rather than a literal, because five of the thirteen cases need a filesystem object
/// created first and three need this kind's own directory spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Declared {
    /// Used verbatim. For shapes stage 1 refuses without touching the filesystem.
    Literal(&'static str),
    /// The real committed file for this kind, under this kind's own directory.
    CommittedForKind,
    /// The same file, prefixed `./` — [`std::path::Component::CurDir`], which names the SAME
    /// file and must be ACCEPTED. This row goes red if the syntactic stage over-refuses.
    CommittedForKindCurDir,
    /// A well-formed relative path under this kind's directory naming a file that is absent.
    AbsentUnderKindDir,
    /// A well-formed relative path naming this kind's DIRECTORY itself.
    KindDirItself,
    /// A path under this kind's directory whose LAST component is a symlink out of the tree.
    LastComponentSymlink,
    /// A path whose FIRST component is a symlinked DIRECTORY pointing out of the tree.
    FirstComponentSymlinkDir,
    /// A path resolving, via a symlink, into a SIBLING directory whose name has the bench
    /// directory's name as a STRING PREFIX. This row goes red if containment is implemented
    /// with `str::starts_with` instead of `Path::starts_with`, which is component-wise.
    PrefixSiblingSymlink,
}

/// What the resolver-then-read pair must do with a case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// Refused with this variant tag.
    Refused(&'static str),
    /// Resolved AND read. Without these rows the table would prove only that something is
    /// refused, which every broken implementation also achieves.
    Accepted,
}

/// One row of the table.
struct PathCase {
    label: &'static str,
    declared: Declared,
    expect: Expect,
    /// A substring the rendered refusal must carry. Empty means no additional check.
    detail_contains: &'static str,
    why: &'static str,
}

const EVIDENCE_PATH_CASES: [PathCase; 13] = [
    // ---- MUST NOT MATCH: eight refusal shapes ------------------------------------------
    PathCase {
        label: "absolute",
        declared: Declared::Literal("/tmp/outside/anywhere.json"),
        expect: Expect::Refused("evidence_path_escape"),
        detail_contains: "ABSOLUTE",
        why: "`Path::join` DISCARDS its base when the argument is absolute",
    },
    PathCase {
        label: "leading_parent_traversal",
        declared: Declared::Literal("../../../etc/passwd"),
        expect: Expect::Refused("evidence_path_escape"),
        detail_contains: "`..`",
        why: "`Path::join` never resolves `..`; the kernel does, at open time",
    },
    PathCase {
        label: "interior_parent_traversal",
        declared: Declared::Literal("locks/../../outside.json"),
        expect: Expect::Refused("evidence_path_escape"),
        detail_contains: "`..`",
        why: "a `..` that is not the FIRST component climbs out just as effectively",
    },
    PathCase {
        label: "empty",
        declared: Declared::Literal(""),
        expect: Expect::Refused("evidence_path_escape"),
        detail_contains: "empty",
        why: "an empty join yields the benchmark directory itself, which would be READ as a \
              directory and reported as an I/O accident rather than as the nonsense it is",
    },
    PathCase {
        label: "whitespace_only",
        declared: Declared::Literal("   "),
        expect: Expect::Refused("evidence_path_escape"),
        detail_contains: "empty",
        why: "same as the empty case, and a trim is the only thing that separates them",
    },
    PathCase {
        label: "last_component_symlink",
        declared: Declared::LastComponentSymlink,
        expect: Expect::Refused("evidence_path_escape"),
        detail_contains: "outside the benchmark directory",
        why: "the declared string is well-formed; only canonicalization can see this",
    },
    PathCase {
        label: "first_component_symlink_dir",
        declared: Declared::FirstComponentSymlinkDir,
        expect: Expect::Refused("evidence_path_escape"),
        detail_contains: "outside the benchmark directory",
        why: "the escape is in the FIRST component, so a last-component-only check misses it",
    },
    PathCase {
        label: "prefix_sibling_symlink",
        declared: Declared::PrefixSiblingSymlink,
        expect: Expect::Refused("evidence_path_escape"),
        detail_contains: "outside the benchmark directory",
        why: "THE ROW THAT GOES RED IF CONTAINMENT USES `str::starts_with`: a bench dir of \
              `<tmp>/bench` is a string prefix of `<tmp>/bench-evil/x.json` and a COMPONENT \
              prefix of nothing in it. `Path::starts_with` is component-wise, and is why this \
              row passes",
    },
    // ---- MUST MATCH: three acceptance rows ---------------------------------------------
    PathCase {
        label: "committed_spelling",
        declared: Declared::CommittedForKind,
        expect: Expect::Accepted,
        detail_contains: "",
        why: "the real spelling every committed row carries; a gate that refuses it cannot pass",
    },
    PathCase {
        label: "committed_spelling_cur_dir",
        declared: Declared::CommittedForKindCurDir,
        expect: Expect::Accepted,
        detail_contains: "",
        why: "`./x` names the same file as `x`; THE ROW THAT GOES RED IF STAGE 1 OVER-REFUSES",
    },
    PathCase {
        label: "committed_spelling_other_kind_dir",
        declared: Declared::CommittedForKind,
        expect: Expect::Accepted,
        detail_contains: "",
        why: "swept over BOTH kinds by the loop, so the LEDGER_DIR spelling is proven to be \
              accepted under `EvidenceKind::Ledger` and not only the LOCKS_DIR one",
    },
    // ---- BEHAVIOUR-PRESERVING: the new refusal must not swallow a distinct diagnosis ----
    PathCase {
        label: "absent_file_under_kind_dir",
        declared: Declared::AbsentUnderKindDir,
        expect: Expect::Refused("evidence_file_missing"),
        detail_contains: "does not exist",
        why: "AN ABSENT FILE IS NOT AN ESCAPE. Reporting it as one would tell an operator to \
              repoint a field that is already correct",
    },
    PathCase {
        label: "directory_in_a_files_position",
        declared: Declared::KindDirItself,
        expect: Expect::Refused("evidence_read_failed"),
        detail_contains: "not a regular file",
        why: "a contained, existing DIRECTORY passes containment and is refused by the bounded \
              read — the pre-existing diagnosis, unchanged",
    },
];

/// This kind's own directory under the benchmark directory.
fn kind_dir(kind: EvidenceKind) -> &'static str {
    match kind {
        EvidenceKind::Row => ROWS_DIR,
        EvidenceKind::Lock => LOCKS_DIR,
        EvidenceKind::Ledger => LEDGER_DIR,
    }
}

/// The committed filename this kind carries for the s8/seed13 cell.
fn kind_committed_file(kind: EvidenceKind) -> &'static str {
    match kind {
        EvidenceKind::Row => "setfit-s8-seed13.json",
        EvidenceKind::Lock => "setfit-s8-seed13.lock.json",
        EvidenceKind::Ledger => "setfit-s8-seed13.jsonl",
    }
}

/// A purpose-built tree whose BENCH DIRECTORY HAS A CHOSEN NAME.
///
/// `write_valid_run`'s temp dir has a random name, and the prefix-sibling row needs a sibling
/// whose name has the bench directory's name as a string prefix — which cannot be arranged
/// without naming the bench directory. Returns `(root, bench_dir)`; the root must outlive the
/// bench dir, so it is handed back rather than dropped.
fn path_case_fixture(kind: EvidenceKind) -> (TempDir, PathBuf) {
    let root = TempDir::new().expect("a temp root");
    let bench = root.path().join("bench");
    // `bench-evil` has `bench` as a STRING prefix and is not under it by any component.
    let evil = root.path().join("bench-evil");
    let outside = root.path().join("outside");
    fs::create_dir_all(bench.join(LOCKS_DIR)).expect("locks dir");
    fs::create_dir_all(bench.join(LEDGER_DIR)).expect("ledger dir");
    fs::create_dir_all(&evil).expect("the prefix-sibling dir");
    fs::create_dir_all(&outside).expect("the outside dir");
    fs::write(evil.join("x.json"), b"{}\n").expect("prefix-sibling file");
    fs::write(outside.join("anywhere.json"), b"{}\n").expect("outside file");
    fs::write(bench.join(kind_dir(kind)).join(kind_committed_file(kind)), b"{}\n")
        .expect("the committed evidence file");
    (root, bench)
}

/// Build one case's declared string against a fixture, creating any symlink it needs.
fn declared_string(declared: Declared, root: &Path, bench: &Path, kind: EvidenceKind) -> String {
    let dir = kind_dir(kind);
    match declared {
        Declared::Literal(literal) => literal.to_string(),
        Declared::CommittedForKind => format!("{dir}/{}", kind_committed_file(kind)),
        Declared::CommittedForKindCurDir => format!("./{dir}/{}", kind_committed_file(kind)),
        Declared::AbsentUnderKindDir => format!("{dir}/never-written.json"),
        Declared::KindDirItself => dir.to_string(),
        Declared::LastComponentSymlink => {
            let relative = format!("{dir}/via-last-link.json");
            symlink_for_test(&root.join("outside").join("anywhere.json"), &bench.join(&relative));
            relative
        }
        Declared::FirstComponentSymlinkDir => {
            symlink_for_test(&root.join("outside"), &bench.join("linked-dir"));
            "linked-dir/anywhere.json".to_string()
        }
        Declared::PrefixSiblingSymlink => {
            let relative = format!("{dir}/to-prefix-sibling.json");
            symlink_for_test(&root.join("bench-evil").join("x.json"), &bench.join(&relative));
            relative
        }
    }
}

/// `#[cfg(unix)]` gates the MATERIALIZER, never a table row: a table that silently shrinks on
/// some host is a coverage loss no test-count floor can see.
fn symlink_for_test(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).expect("symlink");
    #[cfg(not(unix))]
    {
        let _ = (target, link);
    }
}

#[test]
fn bench_gate_evidence_path_case_table_over_both_evidence_kinds() {
    let mut checked = 0_usize;
    for kind in [EvidenceKind::Lock, EvidenceKind::Ledger] {
        for case in &EVIDENCE_PATH_CASES {
            // A FRESH fixture per (case, kind), so no case can see another's symlinks.
            let (root, bench) = path_case_fixture(kind);
            let declared = declared_string(case.declared, root.path(), &bench, kind);
            let cell = CellKey::new(Method::Setfit, 8, 13);

            // The resolver, then the bounded read — exactly the pair `verify_provenance`
            // composes, so `evidence_read_failed` can still be reached for a contained
            // directory and is not swallowed by the new refusal.
            let outcome = resolve_committed_evidence_path(cell, &bench, kind, &declared)
                .and_then(|path| read_evidence(cell, kind, &path));

            match (case.expect, outcome) {
                (Expect::Accepted, Ok(_)) => {}
                (Expect::Accepted, Err(error)) => panic!(
                    "[{kind:?}/{}] `{declared}` must be ACCEPTED ({}), but was refused: {error}",
                    case.label, case.why
                ),
                (Expect::Refused(tag), Err(error)) => {
                    assert_eq!(
                        error.variant_tag(),
                        tag,
                        "[{kind:?}/{}] `{declared}` must be refused as `{tag}` ({}), got: {error}",
                        case.label,
                        case.why
                    );
                    if !case.detail_contains.is_empty() {
                        assert!(
                            error.to_string().contains(case.detail_contains),
                            "[{kind:?}/{}] the refusal must carry `{}`: {error}",
                            case.label,
                            case.detail_contains
                        );
                    }
                }
                (Expect::Refused(tag), Ok(_)) => panic!(
                    "[{kind:?}/{}] `{declared}` was ACCEPTED but must be refused as `{tag}`. {}",
                    case.label, case.why
                ),
            }
            checked += 1;
        }
    }
    // ROWS x KINDS, asserted, so neither field can silently lose coverage and a row deleted
    // to make something pass goes red here rather than quietly.
    assert_eq!(checked, EVIDENCE_PATH_CASES.len() * 2, "every row is swept over BOTH kinds");
    let refused = EVIDENCE_PATH_CASES
        .iter()
        .filter(|case| matches!(case.expect, Expect::Refused("evidence_path_escape")))
        .count();
    let accepted =
        EVIDENCE_PATH_CASES.iter().filter(|case| case.expect == Expect::Accepted).count();
    assert!(refused >= 8, "the table must carry at least eight escape shapes, has {refused}");
    assert!(accepted >= 3, "the table must carry at least three acceptance rows, has {accepted}");
    println!(
        "[bench_gate] EVIDENCE_PATH_CASES rows={} kinds=2 assertions={checked} escapes={refused} \
         accepted={accepted}",
        EVIDENCE_PATH_CASES.len()
    );
}

// ===========================================================================================
// DETERMINISTIC REFUSAL ORDER (plan 05-15 task 3)
// ===========================================================================================

#[test]
fn bench_gate_reports_the_first_offending_cell_in_contract_order_across_runs() {
    // TWO cells doctored, so there is a CHOICE to make. The contract order is (method, shots
    // ascending, seed ascending), so s8/seed17 precedes s32/seed41 and must be the one named
    // on every invocation. Repeated, because an assertion taken once is about a run rather
    // than about an order.
    let first = CellKey::new(Method::Setfit, 8, 17);
    let second = CellKey::new(Method::Setfit, 32, 41);
    let mut named: Vec<String> = Vec::new();

    for _ in 0..3 {
        let dir = write_valid_run(RunSpec::default());
        for cell in [second, first] {
            // Doctored in REVERSE contract order, so a gate that reported "whichever was
            // edited first" would name the wrong one.
            reseal_row(dir.path(), cell, |payload| {
                if let MethodEvidence::Setfit(evidence) = &mut payload.evidence {
                    evidence.lock.lock_record_path = "../escaped.json".to_string();
                }
            });
        }
        let manifest = manifest_for(dir.path());
        let error = refuse(&manifest, dir.path(), "two escaping cells are a refusal");
        assert_eq!(error.variant_tag(), "evidence_path_escape");
        named.push(error.cell().unwrap_or("<none>").to_string());
    }

    assert_eq!(
        named,
        vec![first.render(), first.render(), first.render()],
        "the refusal must name the FIRST offending cell in contract order, on every run"
    );
}

// ===========================================================================================
// The six negatives are SIX DISTINCT variants
// ===========================================================================================

/// Every doctored shape, each returning its own refusal, collected in one place.
///
/// The assertion is that six mutations produce six DISTINCT variant tags. Six refusals that all
/// came back as one catch-all variant would pass six individual `expect_err`s while telling a
/// reader nothing about WHICH dishonesty was found — and a missing cell, a tampered row and an
/// unpaired comparison are different defects with the same symptom.
#[test]
fn bench_gate_the_six_doctored_negatives_are_six_distinct_variants() {
    let mut tags: Vec<&'static str> = Vec::new();

    // 1. omission
    {
        let dir = write_valid_run_deferred(RunSpec::default());
        fs::remove_file(dir.path().join(ROWS_DIR).join(row_file_name(TARGET_SETFIT)))
            .expect("remove");
        let manifest = manifest_for(dir.path());
        tags.push(refuse(&manifest, dir.path(), "1").variant_tag());
    }
    // 2. trimmed block
    {
        let dir = write_valid_run_deferred(RunSpec::default());
        let mut value = read_row_value(dir.path(), TARGET_SETFIT);
        value
            .get_mut("payload")
            .and_then(|p| p.get_mut("evidence"))
            .and_then(|e| e.get_mut("setfit"))
            .and_then(serde_json::Value::as_object_mut)
            .expect("setfit block")
            .remove("lock");
        write_row_value(dir.path(), TARGET_SETFIT, &value);
        let manifest = manifest_for(dir.path());
        tags.push(refuse(&manifest, dir.path(), "2").variant_tag());
    }
    // 3. unpaired
    {
        let dir = write_valid_run_deferred(RunSpec::default());
        reseal_row(dir.path(), TARGET_LORA, |p| {
            p.selection_manifest_hash = "another-draw".to_string();
        });
        let manifest = manifest_for(dir.path());
        tags.push(refuse(&manifest, dir.path(), "3").variant_tag());
    }
    // 4. edited payload bytes
    {
        let dir = write_valid_run_deferred(RunSpec::default());
        let mut value = read_row_value(dir.path(), TARGET_SETFIT);
        *value
            .get_mut("payload")
            .and_then(|p| p.get_mut("dataset_fingerprint"))
            .expect("fingerprint") = serde_json::Value::String("0".repeat(64));
        write_row_value(dir.path(), TARGET_SETFIT, &value);
        let manifest = manifest_for(dir.path());
        tags.push(refuse(&manifest, dir.path(), "4").variant_tag());
    }
    // 5. post-test selection
    {
        let dir = write_valid_run_deferred(RunSpec::default());
        reseal_row(dir.path(), TARGET_LORA, |p| {
            if let MethodEvidence::Lora(e) = &mut p.evidence {
                e.epochs_completed = 1;
            }
        });
        let manifest = manifest_for(dir.path());
        tags.push(refuse(&manifest, dir.path(), "5").variant_tag());
    }
    // 6. forged provenance
    {
        let dir = write_valid_run_deferred(RunSpec::default());
        let lock_path = dir.path().join(format!(
            "{LOCKS_DIR}/{}-s{}-seed{}.lock.json",
            TARGET_SETFIT.method.tag(),
            TARGET_SETFIT.shots,
            TARGET_SETFIT.seed
        ));
        fs::write(&lock_path, b"{\"schema\":\"selection-lock-v1\",\"edited\":true}")
            .expect("lock write");
        let manifest = manifest_for(dir.path());
        tags.push(refuse(&manifest, dir.path(), "6").variant_tag());
    }

    assert_eq!(tags.len(), 6, "six doctored shapes");
    let distinct: BTreeSet<&&str> = tags.iter().collect();
    assert_eq!(
        distinct.len(),
        6,
        "six doctored shapes must produce SIX DISTINCT refusals, got {tags:?}"
    );

    // PRINTED SO A CLOSING AUDIT CAN READ THE COUNT OFF A LOG RATHER THAN QUOTE IT FROM A
    // PLAN. A number an auditor copies out of a document is a number nobody measured; under
    // `--nocapture` this line makes the tally an observation. Visible with
    // `cargo test -p aprender-train --lib --features setfit bench_gate -- --nocapture`.
    println!(
        "[bench_gate] DOCTORED_NEGATIVES={} DISTINCT_REFUSAL_VARIANTS={} TAGS={:?}",
        tags.len(),
        distinct.len(),
        tags
    );
}

// ===========================================================================================
// The two vacuity backstops, and the manifest's own digest
// ===========================================================================================

#[test]
fn bench_gate_refuses_a_zero_cell_manifest_before_reading_any_row() {
    let dir = write_valid_run(RunSpec::default());
    let mut manifest = manifest_for(dir.path());
    manifest.payload.cells.clear();
    // Reseal by hand so the digest check does not fire first and mask the backstop.
    manifest.semantic_hash =
        sha256_hex(&manifest.payload.to_canonical_bytes().expect("payload serializes"));

    let error = refuse(&manifest, dir.path(), "zero cells is a failure");
    assert_eq!(error.variant_tag(), "empty_expectation_set");
}

#[test]
fn bench_gate_refuses_a_manifest_whose_expectation_set_is_not_the_contracted_forty() {
    // THE CONTRACT'S OWN NAMED ADVERSARY: a producer declaring a twelve-cell expectation,
    // satisfying it completely, and publishing a "complete" run. RE-MUTATED at the ACTIVE
    // scope — the 80-cell proof does NOT transfer (CLAUDE.md Verification Discipline rule 4),
    // because the set this backstop compares against is the thing that changed.
    let dir = write_valid_run(RunSpec::default());
    let mut manifest = manifest_for(dir.path());
    manifest.payload.cells.truncate(12);
    manifest.semantic_hash =
        sha256_hex(&manifest.payload.to_canonical_bytes().expect("payload serializes"));

    let error = refuse(&manifest, dir.path(), "a 12-cell expectation is a failure");
    assert_eq!(error.variant_tag(), "expectation_set_mismatch");
    let rendered = error.to_string();
    assert!(rendered.contains("12"), "{rendered}");
    assert!(rendered.contains("40"), "{rendered}");
}

#[test]
fn bench_gate_refuses_a_manifest_declaring_a_second_methods_cell_before_reading_a_row() {
    // ACTIVE-SCOPE OUT-OF-SCOPE NEGATIVE #1, path one of two.
    //
    // A manifest that DECLARES a cell outside the active scope is refused at STEP 2 — before
    // any row byte is read — by the expectation-set backstop that already existed. No new
    // variant is minted: a third variant would be reachable only from a test-only constructor,
    // which is a guard over a path production cannot take.
    let dir = write_valid_run(RunSpec::default());
    let mut manifest = manifest_for(dir.path());
    manifest.payload.cells.push(crate::train::setfit::bench_row::CellEntry {
        method: Method::Lora,
        shots: 16,
        seed: 29,
        status: CellStatus::Complete,
        row_sha256: Some("c".repeat(64)),
    });
    manifest.semantic_hash =
        sha256_hex(&manifest.payload.to_canonical_bytes().expect("payload serializes"));

    let error = refuse(&manifest, dir.path(), "a second method's declared cell is a refusal");
    assert_eq!(
        error.variant_tag(),
        "expectation_set_mismatch",
        "the EXISTING step-2 variant, reused rather than replaced: {error}",
    );
    // And it fired BEFORE any row was read — there is no row file for that cell at all, so a
    // gate that got as far as the row loop would have reported a missing file instead.
    assert!(
        !dir.path().join(ROWS_DIR).join(row_file_name(TARGET_LORA)).exists(),
        "the negative only proves step 2 ran first if no such row exists to be read",
    );
}

#[test]
fn bench_gate_refuses_a_second_methods_row_placed_in_a_declared_setfit_slot() {
    // ACTIVE-SCOPE OUT-OF-SCOPE NEGATIVE #2, path two of two.
    //
    // The other route an out-of-scope cell can take through a production door: the manifest is
    // untouched and correct, but a second method's ROW is filed in a declared SetFit slot. That
    // is refused at STEP 4 by the slot-agreement check — again an EXISTING variant.
    //
    // Building the row at all is what BENCH_METHODS keeps possible: it is still the
    // ROW-VALIDITY domain and still carries both methods. Had the narrowing landed on it
    // instead of on ACTIVE_METHODS, this negative could not be constructed.
    let deferred = write_valid_run_deferred(RunSpec::default());
    let lora_bytes =
        fs::read(deferred.path().join(ROWS_DIR).join(row_file_name(TARGET_LORA))).expect("read");

    let dir = write_valid_run(RunSpec::default());
    fs::write(dir.path().join(ROWS_DIR).join(row_file_name(TARGET_SETFIT)), &lora_bytes)
        .expect("write");
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "a second method's row in a SetFit slot");
    assert_eq!(
        error.variant_tag(),
        "row_slot_mismatch",
        "the EXISTING step-4 variant, reused rather than replaced: {error}",
    );
    assert!(error.to_string().contains(&TARGET_LORA.render()), "{error}");
}

#[test]
fn bench_gate_refuses_a_manifest_whose_own_digest_disagrees_with_its_payload() {
    let dir = write_valid_run(RunSpec::default());
    let mut manifest = manifest_for(dir.path());
    manifest.semantic_hash = "0".repeat(64);

    let error = refuse(&manifest, dir.path(), "a doctored manifest is a refusal");
    assert_eq!(error.variant_tag(), "manifest_digest_mismatch");
}

#[test]
fn bench_gate_refuses_a_row_substituted_for_the_one_the_manifest_recorded() {
    let dir = write_valid_run(RunSpec::default());
    // Manifest FIRST, then a resealed replacement: the row is internally perfect and its cell
    // is right, but it is not the run that produced the recorded number.
    let manifest = manifest_for(dir.path());
    reseal_row(dir.path(), TARGET_SETFIT, |payload| {
        payload.quality.f_avg = 0.99;
        payload.quality.f_avg_bits = 0.99_f64.to_bits();
    });

    let error = refuse(&manifest, dir.path(), "a substitution is a refusal");
    assert_eq!(error.variant_tag(), "row_manifest_digest_mismatch");
}

#[test]
fn bench_gate_refuses_a_row_filed_under_the_wrong_slot() {
    let dir = write_valid_run(RunSpec::default());
    // Put cell (setfit, 16, 31)'s payload in (setfit, 16, 29)'s file, resealed so it is
    // internally consistent — a relabelling that only the slot comparison can see.
    let other = CellKey::new(Method::Setfit, 16, 31);
    let bytes = fs::read(dir.path().join(ROWS_DIR).join(row_file_name(other))).expect("read");
    fs::write(dir.path().join(ROWS_DIR).join(row_file_name(TARGET_SETFIT)), &bytes).expect("write");
    let manifest = manifest_for(dir.path());

    let error = refuse(&manifest, dir.path(), "a mis-slotted row is a refusal");
    assert_eq!(error.variant_tag(), "row_slot_mismatch");
    assert!(error.to_string().contains(&other.render()), "{error}");
}

// ===========================================================================================
// Aggregation: determinism, ordering, degeneracy
// ===========================================================================================

/// Every `f64` reachable in a serialized value, in document order.
fn collect_f64s(value: &serde_json::Value, out: &mut Vec<f64>) {
    match value {
        serde_json::Value::Number(number) => {
            if let Some(f) = number.as_f64() {
                out.push(f);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_f64s(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, item) in map {
                collect_f64s(item, out);
            }
        }
        _ => {}
    }
}

/// Whether any `null` occurs anywhere in a serialized value.
///
/// This is the CHECK for "no non-finite f64 was serialized", and it is a structural walk rather
/// than a substring grep. `serde_json` renders `NaN` and `±inf` as `null`, and a grep for the
/// strings "NaN"/"Infinity" would find neither of them while colliding with the `null_reason`
/// KEY this module deliberately emits.
fn contains_null(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::Array(items) => items.iter().any(contains_null),
        serde_json::Value::Object(map) => map.values().any(contains_null),
        _ => false,
    }
}

#[test]
fn bench_gate_two_aggregations_of_one_run_are_bit_identical() {
    let dir = write_valid_run(RunSpec::default());
    let manifest = manifest_for(dir.path());
    let verified = verify_run(&manifest, dir.path()).expect("verifies");

    let first = serde_json::to_value(aggregate(&verified)).expect("serializes");
    let second = serde_json::to_value(aggregate(&verified)).expect("serializes");

    let (mut a, mut b) = (Vec::new(), Vec::new());
    collect_f64s(&first, &mut a);
    collect_f64s(&second, &mut b);
    assert!(!a.is_empty(), "the determinism check must have f64s to compare");
    assert_eq!(a.len(), b.len());
    for (index, (left, right)) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(
            left.to_bits(),
            right.to_bits(),
            "f64 #{index} differs by BITS between two aggregations of the same rows"
        );
    }
    assert_eq!(first, second, "the whole document must be identical");
}

#[test]
fn bench_gate_aggregate_emits_the_pinned_key_sequence() {
    let dir = write_valid_run(RunSpec::default());
    let manifest = manifest_for(dir.path());
    let verified = verify_run(&manifest, dir.path()).expect("verifies");
    let report = aggregate(&verified);

    assert_eq!(report.key_sequence.len(), EXPECTED_CELLS);
    assert_eq!(
        &report.key_sequence[..5],
        &[
            "setfit/s8/seed13",
            "setfit/s8/seed17",
            "setfit/s8/seed23",
            "setfit/s8/seed29",
            "setfit/s8/seed31",
        ]
    );
    // THE TAIL MOVED WITH THE SCOPE. It read `lora/s64/*` under the 80-cell expectation; under
    // the ACTIVE 40-cell one the last group is SetFit's s64. A tail still reading `lora/...`
    // here after the narrowing would mean the aggregate had not followed the contract.
    assert_eq!(
        &report.key_sequence[EXPECTED_CELLS - 5..],
        &[
            "setfit/s64/seed37",
            "setfit/s64/seed41",
            "setfit/s64/seed43",
            "setfit/s64/seed47",
            "setfit/s64/seed53",
        ]
    );
    // Uniqueness, so the "deterministic order" claim cannot be satisfied by a sequence with a
    // repeated key that happens to sort stably.
    let distinct: BTreeSet<&String> = report.key_sequence.iter().collect();
    assert_eq!(distinct.len(), EXPECTED_CELLS);
}

#[test]
fn bench_gate_aggregate_recomputes_the_closed_form_summary_from_the_rows() {
    let dir = write_valid_run(RunSpec::default());
    let manifest = manifest_for(dir.path());
    let verified = verify_run(&manifest, dir.path()).expect("verifies");
    let report = aggregate(&verified);

    assert_eq!(report.quality.len(), 4, "ONE active method x four shot levels");
    assert_eq!(report.methods, vec!["setfit".to_string()], "the active scope measured one method");
    assert!(
        report.deltas.is_empty(),
        "a delta needs two arms; under the active scope there is nothing to difference, and an \
         EMPTY delta list is the point — a delta computed over an absent arm and reported with \
         a null reason would be a comparison section wearing a caveat",
    );
    assert_eq!(report.n_seeds, PAIRED_DESIGN_N);
    assert_eq!(report.degrees_of_freedom, 9);

    // UNCERTAINTY SURVIVES THE DESCOPE. Dropping the paired delta must not drop the second
    // half of EVAL-04: every active group carries a seed-dispersion interval on the frozen t.
    for group in &report.quality {
        assert!(
            group.f_avg_seed_ci95.is_present(),
            "group {:?}/s{} has no seed-dispersion interval; a single-method report that \
             quoted only a mean and a std would have silently dropped EVAL-04's uncertainty \
             clause under cover of a scope amendment",
            group.method,
            group.shots,
        );
    }

    let group = report
        .quality
        .iter()
        .find(|g| g.method == Method::Setfit && g.shots == 16)
        .expect("the group exists");
    assert_eq!(group.f_avg.n, PAIRED_DESIGN_N);
    assert_eq!(group.per_seed.len(), PAIRED_DESIGN_N);

    // RECOMPUTED BY HAND from the fixture's own generator, which is the property EVAL-04 asks
    // for: the published number must be derivable from the stored rows alone.
    let expected: Vec<f64> = BENCH_SEEDS
        .iter()
        .map(|seed| synthetic_f_avg(RunSpec::default(), CellKey::new(Method::Setfit, 16, *seed)))
        .collect();
    let mean = aprender::stats::hypothesis::mean_f64(&expected).expect("ten values");
    let std = aprender::stats::hypothesis::sample_std_f64(&expected).expect("ten values");
    assert_eq!(group.f_avg.mean.to_bits(), mean.to_bits());
    assert_eq!(group.f_avg.std.to_bits(), std.to_bits());
    assert_eq!(group.f_avg.min.to_bits(), expected[0].to_bits());
    assert_eq!(group.f_avg.max.to_bits(), expected[9].to_bits());

    // THE ACTIVE-SCOPE INTERVAL, recomputed by hand from the same ten stored values — so the
    // uncertainty a reader sees is derivable from the rows alone, exactly as the mean is.
    let ci = aprender::stats::hypothesis::ci95_one_sample_df9(&expected).expect("ten values");
    assert_eq!(group.f_avg_seed_ci95.low.expect("low").to_bits(), ci.low.to_bits());
    assert_eq!(group.f_avg_seed_ci95.high.expect("high").to_bits(), ci.high.to_bits());
    assert_eq!(
        group.f_avg_seed_ci95.half_width.expect("half width").to_bits(),
        (T_CRIT_975_DF9 * std / (PAIRED_DESIGN_N as f64).sqrt()).to_bits(),
        "the half width is the FROZEN t times the standard error, bit for bit",
    );
}

#[test]
fn bench_gate_deferred_scope_still_computes_the_paired_delta() {
    // THE PAIRED MACHINERY IS RETAINED AND UNEXERCISED, not deleted (D-19, D-ITEM-05-15).
    // These assertions were the ACTIVE-scope ones before the narrowing; they are RE-SITED here
    // rather than dropped, so the delta path keeps a running proof and D-ITEM-05-15 restores
    // an arm that still works instead of one nobody has executed since.
    let dir = write_valid_run_deferred(RunSpec::default());
    let manifest = manifest_for(dir.path());
    let verified = verify_scoped(&manifest, dir.path()).expect("verifies");
    let report = aggregate(&verified);

    assert_eq!(report.quality.len(), 8, "two methods x four shot levels");
    assert_eq!(report.deltas.len(), 4, "one paired comparison per shot level");
    assert_eq!(report.methods, vec!["setfit".to_string(), "lora".to_string()]);

    let delta = report.deltas.iter().find(|d| d.shots == 16).expect("the shot level exists");
    assert!(delta.ci95.is_present(), "a varying delta set has an interval");
    assert!(delta.ci95.null_reason.is_none());
    assert!(delta.p_value.is_some(), "p-values live in the detail (D-08)");
    assert_eq!(delta.per_seed_deltas.len(), PAIRED_DESIGN_N);
}

#[test]
fn bench_gate_a_zero_variance_delta_set_reports_a_point_estimate_and_no_interval() {
    let dir = write_valid_run_deferred(RunSpec { zero_variance_shots: Some(8) });
    let manifest = manifest_for(dir.path());
    let verified = verify_scoped(&manifest, dir.path()).expect("verifies");
    let report = aggregate(&verified);

    let degenerate = report.deltas.iter().find(|d| d.shots == 8).expect("the shot level exists");
    assert!(
        !degenerate.ci95.is_present(),
        "an interval over identical differences has no finite width"
    );
    assert_eq!(degenerate.ci95.null_reason.as_deref(), Some(ZERO_VARIANCE_NULL_REASON));
    assert!(degenerate.std_delta.is_none());
    // THE POINT ESTIMATE SURVIVES. It is well defined and it is what a reader wants; only the
    // interval is absent, and it is absent WITH A REASON.
    assert_eq!(degenerate.mean_delta.to_bits(), 0.25_f64.to_bits());
    // The non-degenerate levels are unaffected, so the branch is not a global switch.
    let ordinary = report.deltas.iter().find(|d| d.shots == 16).expect("the shot level exists");
    assert!(ordinary.ci95.is_present());

    // THE SERIALIZED SHAPE. Parsed, never grepped: a substring search for "NaN"/"Infinity"
    // would find neither (serde renders them as `null`) and would collide with the
    // `null_reason` key this module deliberately emits.
    let value = serde_json::to_value(&report).expect("serializes");
    assert!(
        !contains_null(&value),
        "no `null` may appear anywhere in the aggregate — a non-finite f64 serializes as null \
         and reads as a MISSING measurement rather than a degenerate one"
    );
    let mut floats = Vec::new();
    collect_f64s(&value, &mut floats);
    assert!(!floats.is_empty());
    for (index, float) in floats.iter().enumerate() {
        assert!(float.is_finite(), "f64 #{index} is not finite: {float}");
    }
    let rendered = serde_json::to_string(&value).expect("renders");
    assert!(rendered.contains("\"null_reason\":\"zero_variance\""), "{rendered}");
}

#[test]
fn bench_gate_resource_groups_carry_their_hosts_and_mechanism_classes() {
    let dir = write_valid_run_deferred(RunSpec::default());
    let manifest = manifest_for(dir.path());
    let verified = verify_scoped(&manifest, dir.path()).expect("verifies");
    let report = aggregate(&verified);

    let setfit = report
        .resource
        .iter()
        .find(|r| r.method == Method::Setfit && r.shots == 16)
        .expect("group");
    let lora =
        report.resource.iter().find(|r| r.method == Method::Lora && r.shots == 16).expect("group");

    assert_eq!(setfit.hosts, vec!["local-cpu (macos/aarch64)".to_string()]);
    assert_eq!(lora.hosts, vec!["lambda-vector (linux/x86_64)".to_string()]);
    assert_ne!(
        setfit.hosts, lora.hosts,
        "D-09 puts the two methods on two hosts; the aggregate must keep them apart"
    );

    // The mixed-mechanism case, which is what the renderer has to label.
    assert_eq!(
        setfit.train_peak_rss_mechanism_classes,
        vec![MechanismClass::ExactKernelHighWaterMark]
    );
    assert_eq!(lora.train_peak_rss_mechanism_classes, vec![MechanismClass::SampledLowerBound]);
    assert!(!mechanisms_are_comparable(
        &setfit.train_peak_rss_mechanisms[0],
        &lora.train_peak_rss_mechanisms[0]
    ));
    // ... and the SAME-mechanism control, so the predicate is not "always false".
    assert!(mechanisms_are_comparable(
        &setfit.inference_peak_rss_mechanisms[0],
        &lora.inference_peak_rss_mechanisms[0]
    ));

    // The size split: adapter-only bytes and deployable bytes differ by the base model.
    assert!(lora.deployable_total_bytes.mean > lora.artifact_bytes.mean);
    assert_eq!(
        setfit.deployable_total_bytes.mean.to_bits(),
        setfit.artifact_bytes.mean.to_bits(),
        "SetFit ships ONE standalone file, so the two figures are equal by construction"
    );
}

#[test]
fn bench_gate_mechanism_class_case_table() {
    // MUST-MATCH.
    assert_eq!(
        mechanism_class(MECHANISM_CHILD_MAX_RSS_TIME_L),
        MechanismClass::ExactKernelHighWaterMark
    );
    assert_eq!(
        mechanism_class(MECHANISM_CHILD_MAX_RSS_VM_HWM),
        MechanismClass::ExactKernelHighWaterMark
    );
    assert_eq!(mechanism_class("vm_hwm"), MechanismClass::ExactKernelHighWaterMark);
    assert_eq!(mechanism_class("sysinfo_sampled_10hz"), MechanismClass::SampledLowerBound);
    assert_eq!(mechanism_class("sysinfo_sampled_2hz"), MechanismClass::SampledLowerBound);
    // MUST-NOT-MATCH — the NEIGHBOURS of the wanted strings, which is where a loose pattern
    // actually goes wrong, rather than obviously unrelated text.
    assert_eq!(mechanism_class("vm_hwm_child"), MechanismClass::Unrecognised);
    assert_eq!(mechanism_class("sysinfo"), MechanismClass::Unrecognised);
    assert_eq!(mechanism_class("max_rss"), MechanismClass::Unrecognised);
    assert_eq!(mechanism_class(""), MechanismClass::Unrecognised);
    // An unrecognised mechanism is never comparable, INCLUDING to itself: an unknown boundary
    // is not evidence that two numbers mean the same thing.
    assert!(!mechanisms_are_comparable("mystery", "mystery"));
}

// ===========================================================================================
// Contract cross-pins and structural guards
// ===========================================================================================

/// Just enough of the claims contract to reach the frozen statistics constants.
#[derive(Debug, Deserialize)]
struct ContractFile {
    equations: ContractEquations,
}

#[derive(Debug, Deserialize)]
struct ContractEquations {
    claims_statistics: ContractClaimsStatistics,
}

#[derive(Debug, Deserialize)]
struct ContractClaimsStatistics {
    n_seeds: usize,
    degrees_of_freedom: usize,
    t_crit_975_df9: f64,
}

#[test]
fn bench_gate_frozen_t_constant_matches_the_contract_by_bits() {
    let parsed: ContractFile =
        serde_yaml::from_str(CLAIMS_CONTRACT_YAML).expect("the claims contract parses");
    let stats = parsed.equations.claims_statistics;

    // BIT equality, not a tolerance. A constant that can drift within a tolerance is not frozen,
    // and every published interval's width is this number times a standard error.
    assert_eq!(
        stats.t_crit_975_df9.to_bits(),
        T_CRIT_975_DF9.to_bits(),
        "the contract's frozen t literal and aprender_core::stats::hypothesis::T_CRIT_975_DF9 \
         are two pinned copies of one value and they have drifted"
    );
    assert_eq!(stats.n_seeds, PAIRED_DESIGN_N);
    assert_eq!(stats.degrees_of_freedom, PAIRED_DESIGN_N - 1);

    // NON-VACUITY: a parse that silently produced 0.0 would satisfy nothing useful, and the
    // assertion above would still hold if the constant were also 0.0.
    assert!(stats.t_crit_975_df9 > 2.0 && stats.t_crit_975_df9 < 3.0);
}

#[test]
fn bench_gate_aggregate_publishes_the_frozen_t_constant_it_used() {
    let dir = write_valid_run(RunSpec::default());
    let manifest = manifest_for(dir.path());
    let verified = verify_run(&manifest, dir.path()).expect("verifies");
    let report = aggregate(&verified);
    assert_eq!(report.t_crit_975_df9.to_bits(), T_CRIT_975_DF9.to_bits());
    assert_eq!(report.contract_id, CLAIMS_CONTRACT_ID);
}

/// Non-comment source lines only, so prose can neither satisfy nor trip a structural guard.
fn non_comment_source(src: &str) -> String {
    src.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn bench_gate_source_carries_no_rng_or_resampling_vocabulary() {
    let source = non_comment_source(BENCH_GATE_SOURCE);
    // NON-VACUITY FIRST: a guard whose haystack is empty passes for the wrong reason.
    assert!(
        source.len() > 5_000,
        "the no-RNG guard has nothing to scan; the comment filter has eaten the module"
    );
    for forbidden in [
        "rand::",
        "rand_chacha",
        "thread_rng",
        "StdRng",
        "bootstrap",
        "resample",
        "permutation_test",
        "shuffle",
    ] {
        assert!(
            !source.contains(forbidden),
            "`{forbidden}` appears in bench_gate's non-comment source. EVAL-04 requires the \
             published numbers to be EXACTLY recomputable from the rows; a resampling stream \
             downgrades `exactly` to `up to an RNG seed` (D-06)"
        );
    }
}

#[test]
fn bench_gate_verified_run_set_has_no_public_constructor() {
    let source = non_comment_source(BENCH_GATE_SOURCE);
    assert!(
        source.contains("pub struct VerifiedRunSet"),
        "the type must exist for this guard to mean anything"
    );
    for door in ["pub fn new(", "pub const fn new(", "pub fn from_rows", "pub rows:"] {
        assert!(
            !source.contains(door),
            "`{door}` would let a caller mint a VerifiedRunSet without verify_run, which is the \
             whole guarantee aggregate's signature rests on"
        );
    }
}

#[test]
fn bench_gate_verify_run_reads_the_lock_and_ledger_bytes_from_disk() {
    let source = non_comment_source(BENCH_GATE_SOURCE);
    // The recomputation must READ FILES. A gate that compared the row's `lock_hash` to itself
    // would pass every test above while proving nothing.
    assert!(source.contains("lock_record_path"), "the lock path is resolved");
    assert!(source.contains("candidate_ledger_path"), "the ledger path is resolved");
    assert!(
        source.matches("read_evidence(cell").count() >= 2,
        "both provenance branches must read bytes from disk"
    );
    assert!(
        source.contains("sha256_hex(&bytes)"),
        "the digests must be RECOMPUTED over the bytes that were read"
    );
}

#[test]
fn bench_gate_layout_constants_agree_with_the_shipped_row_filename_grammar() {
    // The gate resolves rows by NAME. Two spellings of a filename are two filenames, and a
    // drift would report `row_file_missing` for a path the writer never used.
    assert_eq!(row_file_name(CellKey::new(Method::Setfit, 16, 29)), "setfit-s16-seed29.json");
    assert_eq!(row_file_name(CellKey::new(Method::Lora, 64, 53)), "lora-s64-seed53.json");
    assert_eq!(ROWS_DIR, "rows");
    assert_eq!(LOCKS_DIR, "locks");
    assert_eq!(LEDGER_DIR, "ledger");
    assert_eq!(RUN_MANIFEST_FILE, "run-manifest.json");
    assert_eq!(CONTRACTED_CANDIDATES_TRAINED, 1);
}

#[test]
fn bench_gate_evidence_reads_are_bounded_from_the_declared_length() {
    let dir = TempDir::new().expect("temp dir");
    let path: PathBuf = dir.path().join("rows").join("setfit-s8-seed13.json");
    let error = read_evidence(CellKey::new(Method::Setfit, 8, 13), EvidenceKind::Row, &path)
        .expect_err("a missing file is a refusal");
    assert_eq!(error.variant_tag(), "row_file_missing");

    // A directory in a file's position is refused rather than read.
    let error = read_evidence(CellKey::new(Method::Setfit, 8, 13), EvidenceKind::Row, dir.path())
        .expect_err("a directory is not a row");
    assert_eq!(error.variant_tag(), "evidence_read_failed");

    // WR-06, option (a): the SAME absence under a non-ROW kind names its own artifact. One
    // unconditional mapping told the operator to restore the wrong thing.
    for kind in [EvidenceKind::Lock, EvidenceKind::Ledger] {
        let error = read_evidence(CellKey::new(Method::Setfit, 8, 13), kind, &path)
            .expect_err("a missing evidence file is a refusal");
        assert_eq!(error.variant_tag(), "evidence_file_missing");
        let rendered = error.to_string();
        assert!(rendered.contains(kind.tag()), "the refusal must name its own kind: {rendered}");
        assert!(
            !rendered.contains("restore the row file"),
            "a missing {} must not carry the ROW remedy: {rendered}",
            kind.tag()
        );
    }
}

// ===========================================================================================
// THE SINGLE-CELL VERIFICATION DOOR (plan 05-11 task 2)
//
// `verify_cell` is `verify_run`'s steps 1 + 4 + 6 over ONE declared cell. The two tests below
// carry it. The first is a BEHAVIOURAL equivalence table, not a structural restatement: a test
// asserting "the door calls verify_row_evidence and verify_provenance" would restate the
// door's own definition and could never go red, which is the exact defect class this phase
// exists to prevent.
// ===========================================================================================

/// One per-row defect: a name, the mutation that introduces it, and nothing else.
///
/// The mutation runs against a fresh directory for EACH entry point, so neither run can see
/// the other's side effects and the comparison is of two verdicts on the same defect rather
/// than of one verdict on a directory the other already touched.
struct RowDefect {
    name: &'static str,
    apply: fn(&Path),
}

/// Every per-row defect the door and `verify_run` both have to see, applied to `TARGET_SETFIT`.
fn per_row_defect_table() -> Vec<RowDefect> {
    vec![
        RowDefect {
            name: "the row file is absent",
            apply: |root| {
                fs::remove_file(root.join(ROWS_DIR).join(row_file_name(TARGET_SETFIT)))
                    .expect("remove");
            },
        },
        RowDefect {
            name: "the envelope digest no longer covers the payload",
            apply: |root| {
                let mut value = read_row_value(root, TARGET_SETFIT);
                *value
                    .get_mut("payload")
                    .and_then(|p| p.get_mut("quality"))
                    .and_then(|q| q.get_mut("f_avg"))
                    .expect("f_avg") = serde_json::json!(0.123_456_789);
                write_row_value(root, TARGET_SETFIT, &value);
            },
        },
        RowDefect {
            name: "the schema no longer parses (a required block was trimmed)",
            apply: |root| {
                let mut value = read_row_value(root, TARGET_SETFIT);
                value
                    .get_mut("payload")
                    .and_then(|p| p.get_mut("evidence"))
                    .and_then(|e| e.get_mut("setfit"))
                    .and_then(serde_json::Value::as_object_mut)
                    .expect("setfit block")
                    .remove("lock");
                write_row_value(root, TARGET_SETFIT, &value);
            },
        },
        RowDefect {
            name: "the row is filed under the wrong slot",
            apply: |root| {
                let other = CellKey::new(Method::Setfit, 16, 31);
                let bytes = fs::read(root.join(ROWS_DIR).join(row_file_name(other))).expect("read");
                fs::write(root.join(ROWS_DIR).join(row_file_name(TARGET_SETFIT)), &bytes)
                    .expect("write");
            },
        },
        RowDefect {
            name: "the committed lock bytes were tampered with",
            apply: |root| {
                let lock_path = root.join(format!(
                    "{LOCKS_DIR}/{}-s{}-seed{}.lock.json",
                    TARGET_SETFIT.method.tag(),
                    TARGET_SETFIT.shots,
                    TARGET_SETFIT.seed
                ));
                let mut bytes = fs::read(&lock_path).expect("lock read");
                let last = bytes.len() - 2;
                bytes[last] = b'9';
                fs::write(&lock_path, &bytes).expect("lock write");
            },
        },
        RowDefect {
            name: "the lock role is not one the vocabulary admits",
            apply: |root| {
                reseal_row(root, TARGET_SETFIT, |payload| {
                    if let MethodEvidence::Setfit(evidence) = &mut payload.evidence {
                        evidence.lock.role = "chosen_after_the_fact".to_string();
                    }
                });
            },
        },
        RowDefect {
            name: "the lock rule is not the committed one",
            apply: |root| {
                reseal_row(root, TARGET_SETFIT, |payload| {
                    if let MethodEvidence::Setfit(evidence) = &mut payload.evidence {
                        evidence.lock.rule = "best_observed_on_test".to_string();
                    }
                });
            },
        },
    ]
}

#[test]
fn bench_gate_the_single_cell_door_yields_the_same_variant_as_verify_run_for_every_row_defect() {
    // THE TEST THAT CARRIES THE WHOLE NO-DIVERGENCE CLAIM. Each defect is built ONCE per entry
    // point, fed to both, and the two variant TAGS are compared. If the door ever grows its own
    // copy of a check, or drops one, or reorders two, a row of this table goes red — which a
    // "the door calls the extracted function" assertion could never do.
    let mut compared = 0_usize;

    for defect in per_row_defect_table() {
        // verify_run's verdict.
        let run_dir = write_valid_run(RunSpec::default());
        let run_manifest =
            if defect.name.contains("wrong slot") || defect.name.contains("envelope digest") {
                // These two must be doctored AFTER the manifest records the honest digest,
                // otherwise the manifest would simply record the doctored bytes and the defect
                // would present as something else. Same ordering on both sides below.
                let m = manifest_for(run_dir.path());
                (defect.apply)(run_dir.path());
                m
            } else {
                (defect.apply)(run_dir.path());
                manifest_for(run_dir.path())
            };
        let run_tag = verify_run(&run_manifest, run_dir.path())
            .err()
            .unwrap_or_else(|| panic!("verify_run ACCEPTED `{}`", defect.name))
            .variant_tag();

        // The door's verdict, on the same defect in a fresh directory.
        let door_dir = write_valid_run(RunSpec::default());
        let door_manifest =
            if defect.name.contains("wrong slot") || defect.name.contains("envelope digest") {
                let m = manifest_for(door_dir.path());
                (defect.apply)(door_dir.path());
                m
            } else {
                (defect.apply)(door_dir.path());
                manifest_for(door_dir.path())
            };
        let door_tag = verify_cell(&door_manifest, door_dir.path(), TARGET_SETFIT)
            .err()
            .unwrap_or_else(|| panic!("verify_cell ACCEPTED `{}`", defect.name))
            .variant_tag();

        assert_eq!(
            door_tag, run_tag,
            "`{}`: the door said `{door_tag}` and verify_run said `{run_tag}`. The door is \
             steps 1 + 4 + 6 of verify_run and must not diagnose a row defect differently",
            defect.name,
        );
        compared += 1;
    }

    // NON-VACUITY. A table that silently shrank to zero rows would pass every assertion above.
    assert_eq!(compared, 7, "the per-row defect table must cover all seven shapes");
}

#[test]
fn bench_gate_the_single_cell_door_passes_on_one_complete_cell_among_thirty_nine_pending() {
    // THE PILOT STATE, WHICH IS THE DOOR'S REASON TO EXIST. The manifest declares 40 cells with
    // 39 still `pending` — precisely the state step 3's SWEEP refuses on. A door that inherited
    // the set-level checks could never pass on the cell it exists to check, and `bench report`
    // over a copy holding one row refuses at completeness BEFORE the row loop, so the pilot
    // row's own bytes are never read at all. This door reads them.
    let dir = write_valid_run(RunSpec::default());

    // Delete every row but the pilot, so the other 39 entries are genuinely pending rather
    // than hand-edited into looking that way.
    for cell in RunManifest::expectation() {
        if cell != TARGET_SETFIT {
            fs::remove_file(dir.path().join(ROWS_DIR).join(row_file_name(cell))).expect("remove");
        }
    }
    let manifest = manifest_for(dir.path());

    assert_eq!(manifest.completed(), 1, "exactly one cell is complete");
    assert_eq!(
        manifest.payload.cells.len(),
        EXPECTED_CELLS,
        "the expectation set is still fully DECLARED — that is what makes the other 39 \
         visible as pending rather than absent",
    );

    // The set-level door refuses, and must: 39 pending cells is not a publishable run.
    let run_error =
        verify_run(&manifest, dir.path()).expect_err("a 39-pending run is not complete");
    assert_eq!(run_error.variant_tag(), "incomplete_cell");

    // The single-cell door passes on the one complete cell. This is the whole point.
    verify_cell(&manifest, dir.path(), TARGET_SETFIT)
        .expect("the pilot cell's own evidence is valid and the door must say so");

    // ... and still refuses a cell that IS pending, so it is not simply permissive.
    let pending = CellKey::new(Method::Setfit, 16, 31);
    let pending_error = verify_cell(&manifest, dir.path(), pending)
        .expect_err("a pending cell has no evidence to verify");
    assert_eq!(pending_error.variant_tag(), "incomplete_cell");
}

#[test]
fn bench_gate_the_single_cell_door_refuses_a_cell_outside_the_active_scope() {
    // Asking the door for a second method's cell is an expectation-set disagreement, and it is
    // reported with the EXISTING variant — no new one is minted for the door either.
    let dir = write_valid_run(RunSpec::default());
    let manifest = manifest_for(dir.path());

    let error = verify_cell(&manifest, dir.path(), TARGET_LORA)
        .expect_err("a cell outside the active scope is not verifiable");
    assert_eq!(error.variant_tag(), "expectation_set_mismatch");
}

#[test]
fn bench_gate_the_variant_tag_table_gained_exactly_the_two_arms_this_round_authorised() {
    // Counted over the SHIPPED SOURCE rather than eyeballed in a diff, because a diff review is
    // exactly what missed this class of change before.
    //
    // 05-11 pinned this at 13 and its own two out-of-scope refusals reused EXISTING variants,
    // which is still the default answer. 05-15 mints exactly TWO, and both are reachable from
    // production input rather than from a test-only constructor — which is what 05-11's rule
    // actually forbade:
    //   * `evidence_path_escape`  — gap 1. A producer-written path leaving the benchmark tree
    //     had no refusal at all; reusing `evidence_read_failed` would have reported an I/O
    //     accident for a deliberate escape.
    //   * `evidence_file_missing` — WR-06. An absent LOCK or LEDGER was reported as a missing
    //     ROW file with a remedy naming the wrong artifact.
    //
    // The four contract-derived comparisons this round also added (contract_id on both the row
    // and the manifest, calibration_split, warmup_count, cold_measured_in_child_process) mint
    // NOTHING: they reuse `row_schema_refused`. That asymmetry is the point of this guard — a
    // new arm has to be argued for, one at a time.
    const GATE_SOURCE: &str = include_str!("bench_gate.rs");
    let table = GATE_SOURCE
        .split_once("pub const fn variant_tag(&self) -> &'static str {")
        .expect("the variant_tag table exists")
        .1
        .split_once("\n    }")
        .expect("the table ends")
        .0;
    let arms = table.matches("=> \"").count();
    assert_eq!(
        arms, 15,
        "BenchGateError::variant_tag has {arms} arms; it had 13 before 05-15 and that plan \
         authorised exactly two. A sixteenth would be a variant nobody argued for",
    );
    for minted in ["evidence_path_escape", "evidence_file_missing"] {
        assert!(table.contains(minted), "the `{minted}` arm must be the one that was minted");
    }
}
