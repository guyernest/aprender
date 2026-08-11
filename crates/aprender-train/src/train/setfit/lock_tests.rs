//! Selection-lock and canonical-test-token tests (plan 03-09, TRN-07).
//!
//! Every test name starts `lock_`, which is the filter this plan's Task 2 verification runs.
//!
//! # The headline test is the SEQUENCE, not the happy path
//!
//! `lock_then_tune_then_test_invalidates_and_a_travelled_token_cannot_be_repaired` walks the
//! attack the requirement exists to stop: take a lock on run A, keep tuning to produce run B,
//! then try to reach the test split. Both doors — minting and granting — refuse, and the
//! refusals name both hashes. A lock whose only test is "matching hashes mint a token" would be
//! green for a lock that never compared anything.

use super::super::test_fixtures as fx;
use super::*;

// The two source scanners the non-existence assertions read with. Shared with
// `evaluate_tests.rs` through the fixture module rather than restated here — see
// `fx::source_block_after` for why one copy is the point.
use fx::{source_block_after as block_after, source_signature_after as signature_after};

use crate::train::setfit::evaluate::{evaluate_validation, evaluation_for_tests};
use crate::train::setfit::tune::TuningProbes;
use crate::train::setfit::verify::SerdeJsonCodec;
use crate::train::setfit::Prepared;

use aprender_contrastive_data::prepared::{Canonical, PreparedDataset};

/// This module's implementation source, for the non-existence assertions.
const LOCK_SOURCE: &str = include_str!("lock.rs");

/// A validation-split fingerprint the synthetic candidates share.
const SPLIT_FP: &str = "5f1d9c0aa1e24b7f8c3d6e5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c";
/// A dataset fingerprint the synthetic candidates share.
const DATASET_FP: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

// ===========================================================================================
// Fixtures
// ===========================================================================================

/// A synthetic candidate at a controlled value, sharing the fixture fingerprints.
fn candidate(config: &str, artifact: &str, value: f64) -> SelectionCandidate {
    SelectionCandidate::from_evaluation(
        config,
        evaluation_for_tests(
            ValidationMetricKind::MacroF1,
            value,
            artifact,
            SPLIT_FP,
            DATASET_FP,
            3,
        ),
    )
}

/// A synthetic candidate whose evaluation disagrees with the fixture on one field.
fn candidate_with(
    config: &str,
    artifact: &str,
    value: f64,
    metric: ValidationMetricKind,
    split_fp: &str,
    dataset_fp: &str,
) -> SelectionCandidate {
    SelectionCandidate::from_evaluation(
        config,
        evaluation_for_tests(metric, value, artifact, split_fp, dataset_fp, 3),
    )
}

/// The lock over `candidates`, at the fixture's provenance.
fn lock_of(candidates: Vec<SelectionCandidate>) -> Result<SelectionLock, LockError> {
    SelectionLock::from_candidates(
        candidates,
        SelectionRule::MaxMetricLowestIndexTieBreak,
        "selection-semantic-hash",
        "ledger-hash",
    )
}

/// A complete calibrated pipeline through the shipped codec.
fn verified_run() -> SetFitRun<ArtifactReloadedAndVerified> {
    fx::verified_run(fx::calibrated_variant())
}

/// The SAME configuration, tuned with the intra-batch pull order reversed.
///
/// This is "keep tuning after the lock was taken" in its purest form: the configuration, the
/// selection and the seed are identical, so a lock keyed on the config would still match. Only
/// the EXECUTION differs, and therefore only the artifact does.
fn retuned_run() -> SetFitRun<ArtifactReloadedAndVerified> {
    SetFitRun::<Prepared>::tune_encoder_with_probes(
        fx::prepared_run(fx::calibrated_variant(), None),
        TuningProbes::REVERSE_INTRA_BATCH_PULL,
    )
    .expect("the perturbed run must still pass the evidence gate")
    .fit_head()
    .expect("the head must fit on the fixture's encode-once rows")
    .verify_artifact(&SerdeJsonCodec::new())
    .expect("a faithful codec must complete the round trip")
}

/// The fixture dataset, rebuilt independently of any run's copy.
fn fixture_dataset() -> PreparedDataset<Canonical> {
    fx::fixture_dataset()
}

/// A lock over the supplied run plus two synthetic competitors that TIE with it.
///
/// The run's candidate is placed FIRST and the competitors carry its measured value, so the
/// lowest-index tie-break selects the real run whatever the fixture model happens to score.
/// Fabricating a strictly lower competitor would need a value the test does not know.
fn lock_over(run: &SetFitRun<ArtifactReloadedAndVerified>) -> SelectionLock {
    let dataset = fixture_dataset();
    let real = evaluate_validation(run, &dataset, ValidationMetricKind::MacroF1)
        .expect("the fixture dataset is the one the run was prepared from");
    let value = real.value();
    let split_fp = real.validation_split_fingerprint().to_string();
    let dataset_fp = real.dataset_fingerprint().to_string();

    let candidates = vec![
        SelectionCandidate::from_evaluation("config-a", real),
        candidate_with(
            "config-b",
            "1111111111111111111111111111111111111111111111111111111111111111",
            value,
            ValidationMetricKind::MacroF1,
            &split_fp,
            &dataset_fp,
        ),
        candidate_with(
            "config-c",
            "2222222222222222222222222222222222222222222222222222222222222222",
            value,
            ValidationMetricKind::MacroF1,
            &split_fp,
            &dataset_fp,
        ),
    ];
    run.create_selection_lock(candidates, SelectionRule::MaxMetricLowestIndexTieBreak)
        .expect("the creating run is candidate 0 and every candidate agrees")
}

// ===========================================================================================
// The four candidate-consistency rejections
// ===========================================================================================

#[test]
fn lock_rejects_an_empty_candidate_list() {
    let error = lock_of(Vec::new()).expect_err("an empty list records no decision");
    assert_eq!(error, LockError::NoCandidates);
    assert!(error.to_string().contains("at least one candidate"), "{error}");
}

#[test]
fn lock_rejects_a_candidate_whose_metric_kind_differs() {
    let error = lock_of(vec![
        candidate("a", "aa", 0.5),
        candidate("b", "bb", 0.6),
        candidate_with("c", "cc", 0.7, ValidationMetricKind::Accuracy, SPLIT_FP, DATASET_FP),
    ])
    .expect_err("a selection across two different quantities is not a selection");

    assert_eq!(
        error,
        LockError::MetricKindMismatch {
            index: 2,
            expected: ValidationMetricKind::MacroF1,
            observed: ValidationMetricKind::Accuracy,
        },
    );
    assert!(error.to_string().contains("candidate 2"), "the error must name the index: {error}");
}

#[test]
fn lock_rejects_a_candidate_whose_validation_split_fingerprint_differs() {
    let other = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
    let error = lock_of(vec![
        candidate("a", "aa", 0.5),
        candidate_with("b", "bb", 0.6, ValidationMetricKind::MacroF1, other, DATASET_FP),
    ])
    .expect_err("metrics computed on different rows rank the splits, not the models");

    assert_eq!(
        error,
        LockError::ValidationSplitFingerprintMismatch {
            index: 1,
            expected: SPLIT_FP.to_string(),
            observed: other.to_string(),
        },
    );
    let rendered = error.to_string();
    assert!(rendered.contains("candidate 1"), "{rendered}");
    assert!(rendered.contains(SPLIT_FP) && rendered.contains(other), "{rendered}");
}

#[test]
fn lock_rejects_a_candidate_whose_dataset_fingerprint_differs() {
    let other = "cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe";
    let error = lock_of(vec![
        candidate("a", "aa", 0.5),
        candidate("b", "bb", 0.6),
        candidate("c", "cc", 0.7),
        candidate_with("d", "dd", 0.8, ValidationMetricKind::MacroF1, SPLIT_FP, other),
    ])
    .expect_err("a candidate measured on another corpus is not comparable");

    assert_eq!(
        error,
        LockError::DatasetFingerprintMismatch {
            index: 3,
            expected: DATASET_FP.to_string(),
            observed: other.to_string(),
        },
    );
    assert!(error.to_string().contains("candidate 3"), "{error}");
}

#[test]
fn lock_rejects_duplicate_artifact_hashes() {
    let error = lock_of(vec![
        candidate("a", "aa", 0.5),
        candidate("b", "bb", 0.6),
        candidate("c", "aa", 0.9),
    ])
    .expect_err("one artifact cannot occupy two places in the ranking");

    assert_eq!(
        error,
        LockError::DuplicateArtifactHash {
            index: 2,
            first_index: 0,
            artifact_hash: "aa".to_string(),
        },
    );
    let rendered = error.to_string();
    assert!(rendered.contains("candidate 2"), "{rendered}");
    assert!(rendered.contains("candidate 0"), "{rendered}");
}

/// A non-finite metric is refused at admission, naming the index.
///
/// All three non-finite bit patterns are checked, not just NaN: `total_cmp` places `+infinity`
/// above every finite value too, so a candidate carrying one wins for the same reason.
#[test]
fn lock_rejects_a_candidate_whose_metric_value_is_not_finite() {
    for (position, value) in [(1_usize, f64::NAN), (2, f64::INFINITY), (0, f64::NEG_INFINITY)] {
        let mut candidates =
            vec![candidate("a", "aa", 0.5), candidate("b", "bb", 0.6), candidate("c", "cc", 0.7)];
        candidates[position] = candidate("x", "xx", value);

        let error = lock_of(candidates)
            .expect_err("a non-finite metric is not a measurement the rule can order");
        assert_eq!(
            error,
            LockError::NonFiniteMetric { index: position, value_bits: value.to_bits() },
            "the rejection must name the offending index and carry the exact bits",
        );
        let rendered = error.to_string();
        assert!(rendered.contains(&format!("candidate {position}")), "{rendered}");
    }
}

/// The non-finite refusal BITES: admitted, a NaN would be SELECTED, not ignored.
///
/// This is the two-sided half of the guard and the reason the check exists at all. A test that
/// only asserted the rejection would be exactly as green if `total_cmp` had ignored NaN — which
/// is what the superseded rationale in `evaluate.rs` claimed, reasoning from `partial_cmp`
/// semantics that this rule does not use. So the rule is applied DIRECTLY to a candidate set
/// containing a NaN, bypassing the admission check, and the NaN is shown to win.
#[test]
fn lock_a_nan_would_win_the_rule_which_is_why_admission_refuses_it() {
    let with_nan = vec![
        candidate("a", "aa", 0.5),
        candidate("nan", "nn", f64::NAN),
        candidate("c", "cc", 0.9),
    ];

    // The rule itself, not the guarded door: `apply` is what would see the NaN if admission
    // let it through.
    let chosen = SelectionRule::MaxMetricLowestIndexTieBreak.apply(&with_nan);
    assert_eq!(
        chosen, 1,
        "f64::total_cmp orders +NaN above +infinity, so the NaN candidate WINS -- the failure \
         mode is deterministic selection of a degenerate candidate, not an unusable ordering",
    );

    // And with the same list handed to the real door, it never reaches the rule.
    assert_eq!(
        lock_of(with_nan).expect_err("admission must refuse before the rule ever runs"),
        LockError::NonFiniteMetric { index: 1, value_bits: f64::NAN.to_bits() },
    );
}

// ===========================================================================================
// The rule is APPLIED, not asserted
// ===========================================================================================

#[test]
fn lock_applies_the_rule_and_selects_the_highest_metric() {
    let lock = lock_of(vec![
        candidate("a", "aa", 0.10),
        candidate("b", "bb", 0.90),
        candidate("c", "cc", 0.55),
    ])
    .expect("a consistent candidate set must lock");

    assert_eq!(lock.chosen_index(), 1);
    assert_eq!(lock.chosen_artifact_hash(), "bb");
    assert_eq!(lock.rule(), SelectionRule::MaxMetricLowestIndexTieBreak);
    assert_eq!(lock.candidates().len(), 3, "the WHOLE candidate history is committed");
    assert_eq!(lock.schema_version(), 1);
}

#[test]
fn lock_breaks_an_exact_tie_to_the_lowest_index() {
    let lock = lock_of(vec![
        candidate("a", "aa", 0.30),
        candidate("b", "bb", 0.75),
        candidate("c", "cc", 0.75),
    ])
    .expect("a consistent candidate set must lock");

    assert_eq!(lock.chosen_index(), 1, "an exact tie must keep the EARLIER candidate");
    assert_eq!(lock.chosen_artifact_hash(), "bb");
}

// ===========================================================================================
// The hash binds every committed field
// ===========================================================================================

#[test]
fn lock_hash_binds_every_committed_field() {
    let base = lock_of(vec![candidate("a", "aa", 0.10), candidate("b", "bb", 0.90)])
        .expect("the base lock must build");
    let baseline = base.lock_hash().to_string();
    assert_eq!(baseline.len(), 64, "a SHA-256 renders as 64 hex characters");
    assert_eq!(base.verify_integrity(), Ok(()));

    // Each variation changes exactly ONE committed fact.
    let variations: Vec<(&str, SelectionLock)> = vec![
        (
            "config_hash",
            lock_of(vec![candidate("A", "aa", 0.10), candidate("b", "bb", 0.90)])
                .expect("variation must build"),
        ),
        (
            "artifact_hash",
            lock_of(vec![candidate("a", "aa", 0.10), candidate("b", "bZ", 0.90)])
                .expect("variation must build"),
        ),
        (
            "metric value",
            lock_of(vec![candidate("a", "aa", 0.11), candidate("b", "bb", 0.90)])
                .expect("variation must build"),
        ),
        (
            "candidate order",
            lock_of(vec![candidate("b", "bb", 0.90), candidate("a", "aa", 0.10)])
                .expect("variation must build"),
        ),
        (
            "selection_semantic_hash",
            SelectionLock::from_candidates(
                vec![candidate("a", "aa", 0.10), candidate("b", "bb", 0.90)],
                SelectionRule::MaxMetricLowestIndexTieBreak,
                "a-different-semantic-hash",
                "ledger-hash",
            )
            .expect("variation must build"),
        ),
        (
            "ledger_hash",
            SelectionLock::from_candidates(
                vec![candidate("a", "aa", 0.10), candidate("b", "bb", 0.90)],
                SelectionRule::MaxMetricLowestIndexTieBreak,
                "selection-semantic-hash",
                "a-different-ledger-hash",
            )
            .expect("variation must build"),
        ),
    ];

    for (field, variation) in variations {
        assert_ne!(
            variation.lock_hash(),
            baseline,
            "changing `{field}` must change the lock hash, or that field is not committed",
        );
    }
}

#[test]
fn lock_forged_record_fails_its_hash_check_and_cannot_mint() {
    let run = verified_run();
    let mut lock = lock_over(&run);
    assert_eq!(lock.verify_integrity(), Ok(()));
    assert_eq!(lock.chosen_index(), 0);

    // Point the lock at a DIFFERENT candidate without re-deriving the hash — the shape a
    // hand-edited record takes.
    lock.forge_chosen_index_for_tests(1);

    let error = lock.verify_integrity().expect_err("a rewritten record must not verify");
    let LockError::LockHashMismatch { recorded, recomputed } = error.clone() else {
        panic!("expected a hash mismatch, got {error:?}");
    };
    assert_ne!(recorded, recomputed);
    let rendered = error.to_string();
    assert!(rendered.contains(&recorded) && rendered.contains(&recomputed), "{rendered}");

    // And the check is where it matters: minting runs it FIRST, so a forged lock cannot open
    // the door even when its forged choice happens to name a real artifact.
    let mint = lock.mint_test_token(&run).expect_err("a forged lock must not mint");
    assert!(matches!(mint, LockError::LockHashMismatch { .. }), "got {mint:?}");
}

// ===========================================================================================
// Minting, granting, and the sequence the requirement exists for
// ===========================================================================================

#[test]
fn lock_mints_a_token_for_the_locked_model_and_grants_canonical_test_access() {
    let run = verified_run();
    let lock = lock_over(&run);
    assert_eq!(lock.chosen_artifact_hash(), run.artifact_hash());
    assert_eq!(lock.selection_semantic_hash(), run.selection_semantic_hash());
    assert_eq!(lock.ledger_hash(), hex::encode(run.selection().ledger_hash()));

    let token = lock.mint_test_token(&run).expect("a matching artifact must mint");
    assert_eq!(token.artifact_hash(), run.artifact_hash());
    assert_eq!(token.lock_hash(), lock.lock_hash());
    assert_eq!(token.dataset_fingerprint(), lock.dataset_fingerprint());
    assert_eq!(token.validation_split_fingerprint(), lock.validation_split_fingerprint());

    let expected_rows = run.dataset().test().rows().len();
    let grant = CanonicalTestAccess::grant(token, &run, run.dataset())
        .expect("the token was minted for this very model over this very dataset");
    assert_eq!(grant.artifact_hash(), run.artifact_hash());
    assert_eq!(grant.lock_hash(), lock.lock_hash());
    assert_eq!(grant.test().rows().len(), expected_rows);
    assert!(!grant.test().rows().is_empty(), "the grant must admit real rows");
}

/// A token valid for its own model is still refused against a DIFFERENT canonical corpus.
///
/// This is the half the earlier `&Split<Test>` signature left open: the artifact check passed,
/// nothing looked at the data, and the grant went on reporting the locked corpus's `lock_hash`
/// while handing out another corpus's rows. The model is deliberately the RIGHT one here, so the
/// only thing that can produce a refusal is the dataset identity.
#[test]
fn lock_grant_refuses_a_token_presented_with_a_different_canonical_dataset() {
    let run = verified_run();
    let lock = lock_over(&run);
    let token = lock.mint_test_token(&run).expect("the locked model must mint");

    let other = fx::dataset_with_altered_test_row();
    let locked_fingerprint = token.dataset_fingerprint().to_string();
    let other_fingerprint = other.validation_witness().dataset_fingerprint_hex();
    assert_ne!(
        locked_fingerprint, other_fingerprint,
        "the two corpora must actually differ, or the refusal below would hold vacuously",
    );

    let error = CanonicalTestAccess::grant(token, &run, &other)
        .expect_err("the right model over the wrong corpus is still the wrong access");
    assert_eq!(
        error,
        LockError::TokenDatasetMismatch {
            token_dataset_fingerprint: locked_fingerprint.clone(),
            observed_dataset_fingerprint: other_fingerprint.clone(),
        },
    );
    let rendered = error.to_string();
    assert!(rendered.contains(&locked_fingerprint), "both digests must be named: {rendered}");
    assert!(rendered.contains(&other_fingerprint), "both digests must be named: {rendered}");
}

/// The granted rows come OUT of the dataset that was checked, not from one supplied beside it.
///
/// The load-bearing half of the repair. A `grant` that verified the dataset's fingerprint and
/// then admitted a separately-passed `&Split<Test>` would satisfy every assertion in the refusal
/// test above while still handing out rows nobody checked, so the identity of the returned split
/// is asserted against the checked dataset directly.
#[test]
fn lock_grant_admits_the_checked_datasets_own_test_rows() {
    let run = verified_run();
    let lock = lock_over(&run);
    let token = lock.mint_test_token(&run).expect("the locked model must mint");

    let dataset = fx::fixture_dataset();
    let expected: Vec<&str> = dataset.test().rows().iter().map(|row| row.id.as_str()).collect();
    assert!(!expected.is_empty(), "the fixture must have test rows, or this proves nothing");

    let grant = CanonicalTestAccess::grant(token, &run, &dataset)
        .expect("an independently rebuilt copy of the SAME corpus must be accepted");
    let admitted: Vec<&str> = grant.test().rows().iter().map(|row| row.id.as_str()).collect();
    assert_eq!(
        admitted, expected,
        "the grant must hand out the test split of the dataset whose fingerprint it checked",
    );
}

/// The whole attack, in order: lock, keep tuning, then try to reach the test split.
#[test]
fn lock_then_tune_then_test_invalidates_and_a_travelled_token_cannot_be_repaired() {
    let run_a = verified_run();
    let run_b = retuned_run();
    assert_ne!(
        run_a.artifact_hash(),
        run_b.artifact_hash(),
        "the fixture must actually re-tune, or every assertion below would hold vacuously",
    );

    let lock = lock_over(&run_a);
    let token = lock.mint_test_token(&run_a).expect("the locked model must mint");

    // (1) Minting against the RE-TUNED run is refused, and the refusal names both hashes.
    let stale = lock.mint_test_token(&run_b).expect_err("a re-tuned model must not mint");
    assert_eq!(
        stale,
        LockError::StaleLock { locked: run_a.artifact_hash(), observed: run_b.artifact_hash() },
    );
    let rendered = stale.to_string();
    assert!(rendered.contains(&run_a.artifact_hash()), "{rendered}");
    assert!(rendered.contains(&run_b.artifact_hash()), "{rendered}");

    // (2) The token minted for run A cannot be carried to run B either. Minting checked the
    //     identity at one instant; the grant re-checks it at the point of access.
    let carried = CanonicalTestAccess::grant(token, &run_b, run_b.dataset())
        .expect_err("a token that travelled must not be re-paired");
    assert_eq!(
        carried,
        LockError::TokenModelMismatch {
            token_artifact_hash: run_a.artifact_hash(),
            model_artifact_hash: run_b.artifact_hash(),
        },
    );
    let rendered = carried.to_string();
    assert!(rendered.contains(&run_a.artifact_hash()), "{rendered}");
    assert!(rendered.contains(&run_b.artifact_hash()), "{rendered}");
}

#[test]
fn lock_requires_the_creating_run_to_be_among_the_candidates() {
    let run = verified_run();
    let error = run
        .create_selection_lock(
            vec![candidate("a", "aa", 0.5), candidate("b", "bb", 0.6)],
            SelectionRule::MaxMetricLowestIndexTieBreak,
        )
        .expect_err("a lock that omits the model that created it records someone else's run");

    assert_eq!(
        error,
        LockError::ChosenModelNotACandidate { artifact_hash: run.artifact_hash(), candidates: 2 },
    );
    assert!(error.to_string().contains(&run.artifact_hash()), "{error}");
}

/// The evaluation the lock commits is the REAL one, computed by the evaluator.
#[test]
fn lock_commits_the_evaluation_the_evaluator_produced() {
    let run = verified_run();
    let dataset = fixture_dataset();
    let real = evaluate_validation(&run, &dataset, ValidationMetricKind::MacroF1)
        .expect("the fixture dataset is the one the run was prepared from");
    let lock = lock_over(&run);

    let chosen = lock.chosen();
    assert_eq!(chosen.artifact_hash(), run.artifact_hash());
    assert_eq!(chosen.evaluation().value_bits(), real.value_bits());
    assert_eq!(chosen.evaluation().metric_kind(), ValidationMetricKind::MacroF1);
    assert_eq!(lock.validation_split_fingerprint(), real.validation_split_fingerprint());
    assert_eq!(lock.dataset_fingerprint(), real.dataset_fingerprint());
}

/// A candidate's artifact hash is READ OUT of its evaluation and cannot be supplied beside it.
#[test]
fn lock_candidate_artifact_hash_comes_from_its_evaluation() {
    let evaluation = evaluation_for_tests(
        ValidationMetricKind::MacroF1,
        0.75,
        "9999999999999999999999999999999999999999999999999999999999999999",
        SPLIT_FP,
        DATASET_FP,
        3,
    );
    let expected = evaluation.artifact_hash().to_string();
    let built = SelectionCandidate::from_evaluation("config", evaluation);
    assert_eq!(built.artifact_hash(), expected);
    assert_eq!(built.evaluation().artifact_hash(), expected);

    let signature = signature_after(LOCK_SOURCE, "pub fn from_evaluation(");
    assert!(
        !signature.contains("artifact_hash"),
        "an artifact hash supplied beside the evaluation could disagree with it: `{signature}`",
    );
}

// ===========================================================================================
// The non-existence assertions — this is where the two review fixes are pinned
// ===========================================================================================

/// `mint_test_token` takes the run OBJECT and no hash bytes. This is review fix 2.
#[test]
fn lock_mint_test_token_takes_the_run_object_and_no_hash_bytes() {
    let signature = signature_after(LOCK_SOURCE, "pub fn mint_test_token(");
    assert!(
        signature.contains("&SetFitRun<ArtifactReloadedAndVerified>"),
        "minting must read the hash off the run: `{signature}`",
    );
    assert!(
        !signature.contains("[u8; 32]") && !signature.contains("[u8;32]"),
        "a byte-array parameter lets a caller submit the locked hash and evaluate a different \
         artifact: `{signature}`",
    );
    assert!(!signature.contains("artifact_hash:"), "`{signature}`");
}

/// `from_candidates` has no `chosen` parameter. This is review fix 3.
#[test]
fn lock_from_candidates_has_no_chosen_parameter() {
    let signature = signature_after(LOCK_SOURCE, "fn from_candidates(");
    assert!(
        !signature.contains("chosen"),
        "a lock that records only a caller's winner is consistent with a test peek: `{signature}`",
    );
    assert!(signature.contains("rule: SelectionRule"), "`{signature}`");
    assert!(signature.contains("candidates: Vec<SelectionCandidate>"), "`{signature}`");
}

/// The token has no public constructor and no public field.
#[test]
fn lock_token_has_no_public_constructor_and_no_public_field() {
    let block = block_after(LOCK_SOURCE, "impl CanonicalTestToken {");
    assert!(!block.contains("pub fn new"), "reading a token must not be able to mint one");
    assert!(!block.contains("pub const fn new"));

    let declaration = block_after(LOCK_SOURCE, "pub struct CanonicalTestToken {");
    for field in declaration.lines().skip(1) {
        assert!(
            !field.trim_start().starts_with("pub "),
            "every token field must be private; found `{field}`",
        );
    }
}

/// The grant takes the token, the model and the canonical DATASET — never a bare split.
///
/// The negative half is the one the cleanup review added. A bare `&Split<Test>` carries no
/// provenance, so a grant accepting one cannot bind the corpus no matter what it asserts
/// internally; pinning its absence is what stops the parameter regressing to the shape that
/// admitted another corpus's rows under the locked corpus's `lock_hash`.
#[test]
fn lock_grant_signature_takes_the_token_the_model_and_the_canonical_dataset() {
    let signature = signature_after(LOCK_SOURCE, "pub fn grant<'a>(");
    assert!(signature.contains("token: CanonicalTestToken"), "`{signature}`");
    assert!(signature.contains("&SetFitRun<ArtifactReloadedAndVerified>"), "`{signature}`");
    assert!(signature.contains("&'a PreparedDataset<Canonical>"), "`{signature}`");
    assert!(
        !signature.contains("Split<Test>"),
        "a bare split carries no provenance, so the corpus check could not be sound: `{signature}`",
    );
    assert!(
        !signature.contains("CompatibilityTest"),
        "`Split<CompatibilityTest>` is a different type and is unexpressible here",
    );
}

/// The lock is append-only: no API removes or edits a committed candidate.
#[test]
fn lock_has_no_removal_or_edit_api() {
    let block = block_after(LOCK_SOURCE, "impl SelectionLock {");
    for forbidden in ["fn remove", "fn pop", "fn clear", "fn push", "fn set_", "&mut Vec<"] {
        // The one `&mut self` door is the `#[cfg(test)]` forgery, which is what proves the
        // integrity check bites; it is named here rather than excluded by accident.
        assert!(
            !block.contains(forbidden),
            "`{forbidden}` would make the committed candidate list rewritable",
        );
    }
    assert_eq!(
        block.matches("&mut self").count(),
        1,
        "exactly one mutating door, and it is the test-only forgery",
    );
    assert!(
        LOCK_SOURCE.contains("#[cfg(test)]\n    pub(super) fn forge_chosen_index_for_tests("),
        "the only mutating door must be `#[cfg(test)]`-gated",
    );
}

/// The lock's door on the run is exactly ONE method, and it takes a shared borrow.
///
/// This is the other half of `verify_reproducibility_accessors_are_read_only_and_complete`.
/// `create_selection_lock` lives in `lock.rs` rather than in `mod.rs`'s reproducibility block
/// (see the doc on that impl for why), and moving a method out of a counted block is only
/// honest if the new home counts it too. This counts it.
#[test]
fn lock_run_side_door_is_a_single_read_only_method() {
    let block = block_after(LOCK_SOURCE, "impl SetFitRun<ArtifactReloadedAndVerified> {");
    assert_eq!(
        block.matches("\n    pub fn ").count(),
        1,
        "the lock's door on the run must stay a single method, or this guard stops describing it",
    );
    assert!(block.contains("pub fn create_selection_lock("), "{block}");
    assert!(
        block.contains("&self,") && !block.contains("&mut self"),
        "the door must take a SHARED borrow: creating a lock cannot alter the run it records",
    );

    // And a compile-time half, exactly as the reproducibility guard does it: called through a
    // shared reference, so a `&mut self` or by-value receiver would not type-check.
    let run = verified_run();
    let r: &SetFitRun<ArtifactReloadedAndVerified> = &run;
    let _: Result<SelectionLock, LockError> =
        r.create_selection_lock(Vec::new(), SelectionRule::MaxMetricLowestIndexTieBreak);
}

/// No wall-clock type appears in the lock, so its canonical bytes cannot drift.
#[test]
fn lock_source_carries_no_wall_clock_type() {
    for forbidden in ["Dur\u{61}tion", "Inst\u{61}nt", "SystemTim\u{65}"] {
        assert!(
            !LOCK_SOURCE.contains(forbidden),
            "a wall-clock value in the lock would make two identical selections hash differently",
        );
    }
}

/// The canonical bytes carry the schema version, the rule and every candidate.
#[test]
fn lock_canonical_bytes_commit_the_whole_record() {
    let lock = lock_of(vec![candidate("config-a", "aa", 0.10), candidate("config-b", "bb", 0.90)])
        .expect("the lock must build");
    let bytes = lock.to_canonical_bytes();
    let json = String::from_utf8(bytes).expect("the canonical form is UTF-8 JSON");

    assert!(json.contains("\"schema_version\":1"), "{json}");
    assert!(json.contains("\"rule\":\"max_metric_lowest_index_tie_break\""), "{json}");
    assert!(json.contains("\"chosen_index\":1"), "{json}");
    assert!(json.contains("\"config_hash\":\"config-a\""), "{json}");
    assert!(json.contains("\"config_hash\":\"config-b\""), "{json}");
    assert!(json.contains("\"selection_semantic_hash\":\"selection-semantic-hash\""), "{json}");
    assert!(json.contains("\"ledger_hash\":\"ledger-hash\""), "{json}");
    assert!(json.contains("\"value_bits\":"), "the metric travels as bits: {json}");
    assert!(
        !json.contains("\"lock_hash\""),
        "the digest must not be inside the bytes it digests: {json}",
    );
    assert_eq!(lock.recompute_lock_hash(), lock.lock_hash());
}
