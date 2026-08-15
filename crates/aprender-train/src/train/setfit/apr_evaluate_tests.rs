//! Fresh-process evaluation tests (plan 04-07, TRN-07).
//!
//! Every test name starts `apr_evaluate_`.
//!
//! The fixture estate is 04-16's: the ONLY run in this crate that can produce real
//! `setfit-apr-v1` bytes, plus inputs an independent process would rebuild from Phase 2's
//! ingest ladder and a persisted selection manifest. It is REUSED, never copied — a second
//! APR-capable fixture would be free to drift in exactly the dimensions the load ladder's
//! probes are sensitive to.

use aprender_contrastive_data::ledger::AccessLedger;
use aprender_contrastive_data::manifest::SelectionManifest;
use aprender_contrastive_data::select::{FewShotSelector, Selection, SelectionConfig};

use super::super::apr_codec::fixture::{apr_capable_run, artifact_bytes_of};
use super::super::apr_reload::reload_verified_run_from_apr;
use super::super::evaluate::evaluate_validation;
use super::super::lock::{create_selection_lock, SelectionCandidate, SelectionRule};
use super::super::test_fixtures as fx;
use super::*;

/// This module's subject, for the source assertions.
const APR_EVALUATE_SOURCE: &str = include_str!("apr_evaluate.rs");

/// The artifact under test, as bytes.
fn apr_artifact_bytes() -> Vec<u8> {
    artifact_bytes_of(&apr_capable_run())
}

/// The dataset and selection a SEPARATE process would hold, rebuilt from scratch.
///
/// Identical in shape to `apr_reload_tests::fresh_process_inputs` and for the same reason: a
/// test that handed the door back the objects the artifact was written from would prove the
/// gate passes by IDENTITY, which is not the property claimed. Routing the selection through
/// a persisted manifest and `Selection::replay` is what makes the ledger hash comparable.
fn fresh_process_inputs() -> (PreparedDataset<Canonical>, Selection) {
    let variant = fx::calibrated_variant();

    let mut ledger = AccessLedger::new();
    let dataset = fx::synthetic_dataset(&mut ledger);
    let selection = FewShotSelector::select(
        &dataset,
        &SelectionConfig { root_seed: variant.root_seed, shots_per_class: variant.shots_per_class },
        &mut ledger,
    )
    .expect("the synthetic corpus must support the calibrated selection");
    let manifest = SelectionManifest::from_selection(&selection, &ledger)
        .expect("the live ledger is the one the selection was taken under");
    let bytes = manifest.to_file_bytes().expect("the manifest must serialize");

    let mut fresh_ledger = AccessLedger::new();
    let fresh_dataset = fx::synthetic_dataset(&mut fresh_ledger);
    let restored = SelectionManifest::from_bytes(&bytes).expect("the manifest must parse back");
    let replayed = Selection::replay(&restored, &fresh_dataset, &mut fresh_ledger)
        .expect("the manifest must replay against the rebuilt corpus");
    (fresh_dataset, replayed)
}

/// A credential minted the way `apr eval` mints one, plus the dataset it was gated against.
fn fresh_credential() -> (ReloadedSetFitCredential, PreparedDataset<Canonical>) {
    let bytes = apr_artifact_bytes();
    let (dataset, selection) = fresh_process_inputs();
    let credential = reload_verified_run_from_apr(&bytes, &dataset, &selection)
        .expect("the artifact and the independently-rebuilt inputs must agree");
    (credential, dataset)
}

// ===========================================================================================
// The claim: a process that trained nothing can measure, and therefore can be a candidate
// ===========================================================================================

/// The gap 04-16 recorded open is closed: a `.apr` file becomes a `SelectionCandidate`.
#[test]
fn apr_evaluate_lets_a_fresh_process_build_the_candidate_set_it_locks_over() {
    let (credential, dataset) = fresh_credential();

    let evaluation =
        evaluate_validation_from_artifact(&credential, &dataset, ValidationMetricKind::Accuracy)
            .expect("a reloaded artifact must be measurable on the corpus it was gated against");

    // The measurement is REAL: a value in range, over a non-empty split, bound to this
    // artifact. A test that only asserted `is_ok` would pass for an evaluator that returned a
    // constant.
    assert!(
        (0.0..=1.0).contains(&evaluation.value()),
        "accuracy must be a fraction, got {}",
        evaluation.value()
    );
    assert!(evaluation.n_rows() > 0, "the split measured must be non-empty");
    assert_eq!(
        evaluation.artifact_hash(),
        credential.artifact_hash(),
        "the evaluation must be bound to the artifact it was computed with, read off the \
         credential rather than supplied",
    );

    // And it is what a candidate is made of — the whole point of the door.
    let candidate = SelectionCandidate::from_evaluation("cfg-hash-for-this-test", evaluation);
    let lock = create_selection_lock(
        &credential,
        vec![candidate],
        SelectionRule::MaxMetricLowestIndexTieBreak,
    )
    .expect("the credential's own artifact is in the candidate set");
    assert_eq!(
        lock.chosen_artifact_hash(),
        credential.artifact_hash(),
        "the lock must record THIS artifact as the chosen one",
    );
    assert_eq!(lock.lock_hash().len(), 64, "a lock hash is a SHA-256");
}

/// The value is DETERMINISTIC: two evaluations of one artifact agree down to the bits.
#[test]
fn apr_evaluate_is_bitwise_deterministic_for_one_artifact_and_corpus() {
    let (credential, dataset) = fresh_credential();
    let first =
        evaluate_validation_from_artifact(&credential, &dataset, ValidationMetricKind::MacroF1)
            .expect("measurable");
    let second =
        evaluate_validation_from_artifact(&credential, &dataset, ValidationMetricKind::MacroF1)
            .expect("measurable");
    assert_eq!(
        first.value_bits(),
        second.value_bits(),
        "two runs must agree BY BITS; a decimal comparison would accept two values differing \
         in the last place, and the selection lock hashes the bits",
    );
    assert_eq!(first.metric_kind(), ValidationMetricKind::MacroF1);
}

/// Both metric kinds are reachable and the closed enum is honoured.
#[test]
fn apr_evaluate_computes_both_contracted_metric_kinds() {
    let (credential, dataset) = fresh_credential();
    for kind in [ValidationMetricKind::Accuracy, ValidationMetricKind::MacroF1] {
        let evaluation =
            evaluate_validation_from_artifact(&credential, &dataset, kind).expect("measurable");
        assert_eq!(evaluation.metric_kind(), kind);
        assert!((0.0..=1.0).contains(&evaluation.value()));
    }
}

/// The committed facts are read off the CORPUS's witness, not off the artifact's copy.
#[test]
fn apr_evaluate_commits_both_fingerprints_from_the_supplied_datasets_witness() {
    let (credential, dataset) = fresh_credential();
    let evaluation =
        evaluate_validation_from_artifact(&credential, &dataset, ValidationMetricKind::Accuracy)
            .expect("measurable");

    let witness = dataset.validation_witness();
    assert_eq!(evaluation.dataset_fingerprint(), witness.dataset_fingerprint_hex());
    assert_eq!(evaluation.validation_split_fingerprint(), witness.fingerprint_hex());
    assert_ne!(
        evaluation.dataset_fingerprint(),
        evaluation.validation_split_fingerprint(),
        "the two fingerprints digest different things; equal values would mean one path was \
         read twice",
    );
}

// ===========================================================================================
// The refusals
// ===========================================================================================

/// A different corpus is refused, naming both fingerprints.
#[test]
fn apr_evaluate_refuses_a_dataset_that_is_not_the_artifacts_corpus() {
    let (credential, own) = fresh_credential();

    // ONE TEST ROW ALTERED, and nothing else. That is the sharpest available probe: the
    // VALIDATION split is byte-identical, so an evaluator that only compared the split's
    // fingerprint would happily measure this corpus and report a number about a dataset the
    // artifact was never trained on.
    let other = fx::dataset_with_altered_test_row();
    assert_ne!(
        other.validation_witness().dataset_fingerprint_hex(),
        own.validation_witness().dataset_fingerprint_hex(),
        "non-vacuity: the probe corpus must actually differ, or this test proves nothing",
    );
    assert_eq!(
        other.validation_witness().fingerprint_hex(),
        own.validation_witness().fingerprint_hex(),
        "the probe must hold the VALIDATION split constant, so only the corpus check can fire",
    );

    let error =
        evaluate_validation_from_artifact(&credential, &other, ValidationMetricKind::Accuracy)
            .expect_err("a metric on another corpus is not evidence about this artifact");
    let rendered = error.to_string();
    assert!(
        matches!(
            error,
            SetFitTrainError::AprEvaluate(AprEvaluateError::DatasetFingerprintMismatch { .. })
        ),
        "the corpus disagreement must be its own typed refusal; got: {rendered}",
    );
    assert!(
        rendered.contains(&other.validation_witness().dataset_fingerprint_hex()),
        "the refusal must name the supplied fingerprint; got: {rendered}",
    );
}

/// The refusals are distinct values, and each rendering names both compared values.
#[test]
fn apr_evaluate_refusals_are_distinct_and_name_what_they_compared() {
    let a = AprEvaluateError::DatasetFingerprintMismatch {
        recorded: "aaaa".to_string(),
        supplied: "bbbb".to_string(),
    };
    let b = AprEvaluateError::ValidationSplitFingerprintMismatch {
        recorded: "cccc".to_string(),
        supplied: "dddd".to_string(),
    };
    let c = AprEvaluateError::LabelMapMismatch {
        artifact: vec!["x".to_string()],
        dataset: vec!["y".to_string()],
    };
    let d = AprEvaluateError::ProvenanceUnreadable { field: "dataset_fingerprint" };
    let e = AprEvaluateError::ValidationSplitEmpty;

    let rendered: Vec<String> = [&a, &b, &c, &d, &e].iter().map(|x| x.to_string()).collect();
    for (i, left) in rendered.iter().enumerate() {
        for (j, right) in rendered.iter().enumerate() {
            if i != j {
                assert_ne!(left, right, "two refusals must not read the same");
            }
        }
    }
    assert!(rendered[0].contains("aaaa") && rendered[0].contains("bbbb"));
    assert!(rendered[1].contains("cccc") && rendered[1].contains("dddd"));
    assert!(rendered[2].contains('x') && rendered[2].contains('y'));
    assert!(rendered[3].contains("dataset_fingerprint"));
}

// ===========================================================================================
// The structural claims
// ===========================================================================================

/// This module computes NO metric and constructs NO evaluation of its own.
#[test]
fn apr_evaluate_reduction_is_the_trainers_own() {
    // CODE LINES only: the module header explains the sharing at length using the very names
    // scanned for, and a guard that fails on its own documentation is the F-05 defect.
    let code: String = APR_EVALUATE_SOURCE
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            !t.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        code.contains("fn evaluate_validation_from_artifact"),
        "non-vacuity: the comment filter must not have eaten the module",
    );

    assert!(
        code.contains("evaluate::evaluation_from_predictions("),
        "the bounds check, the metric dispatch and the record construction must be the \
         trainer's shared tail, called — not restated",
    );
    for banned in [
        "fn accuracy(",
        "fn macro_f1(",
        "ValidationEvaluation {",
        "mean_in_index_order",
        "true_positive",
    ] {
        assert!(
            !code.contains(banned),
            "`{banned}` here would be a SECOND reduction implementation; OPS-03 says one per \
             operation, and two float pipelines that must agree are two that eventually will \
             not",
        );
    }
}

/// The prediction path is core's ONE classify door, not a second encode.
#[test]
fn apr_evaluate_predicts_through_cores_single_classification_path() {
    let code: String = APR_EVALUATE_SOURCE
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        code.contains("model.classify(&request)"),
        "prediction must go through `VerifiedSetFitModel::classify` — the path the artifact's \
         own probe replay is evidence about",
    );
    assert!(
        code.contains("MAX_BATCH_TEXTS"),
        "the split must be chunked by core's own bound; a single oversized call would be \
         REFUSED rather than measured",
    );
    for banned in ["encode_eval_rows", "predict_indices", "encode_batch_traced"] {
        assert!(
            !code.contains(banned),
            "`{banned}` would be a second encode path, measuring something the artifact carries \
             no probes for",
        );
    }
}

/// No lifecycle state is minted and no evidence is fabricated here.
#[test]
fn apr_evaluate_mints_no_state_and_fabricates_no_evidence() {
    let code: String = APR_EVALUATE_SOURCE
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for banned in [
        "HeadFittedEvidence",
        "PassedEvidence",
        "UpdateEvidence",
        "validate_evidence",
        "verify_artifact",
        "SetFitRun {",
        "unimplemented!",
        "todo!",
    ] {
        assert!(!code.contains(banned), "this module must not construct or fabricate `{banned}`",);
    }
}
