//! Verify-by-reload tests (plan 03-08, TRN-01, TRN-06).
//!
//! Every test name starts `verify_`, which is the filter this plan's Task 2
//! verification runs.
//!
//! # What the `EchoCodec` negative proves, and what it does not
//!
//! [`EchoCodec`] is an in-band CHEATING CODEC. Its `deserialize` ignores its input
//! bytes entirely and returns a bundle held in a cache that is SET ONCE AT
//! CONSTRUCTION; its `serialize` is honest and deliberately does NOT refresh that
//! cache. It is the shape of implementor the codec/policy split exists to defeat:
//! one that hands back an object it already had instead of one it read.
//!
//! **It proves** that the trusted policy forces the reloaded value to be a
//! FUNCTION OF THE HASHED BYTES. The round-trip closure check re-serializes what
//! the codec returned and requires the result to equal the bytes that were hashed;
//! `EchoCodec`'s cache holds a different bundle, so re-serializing it cannot
//! reproduce them, and the transition returns
//! `SetFitTrainError::ReloadNotFromBytes`. A codec therefore cannot substitute an
//! arbitrary object for the artifact's contents.
//!
//! **It does not prove** that the artifact was durable. An in-process codec that
//! round-trips faithfully through a `Vec<u8>` it never writes anywhere satisfies
//! every check here, and is indistinguishable from one that wrote a file — because
//! nothing in this crate observes the filesystem. Durability is a claim about I/O
//! and it is out of this seam's scope; phase 4's format is where it acquires one.
//!
//! # The negative was MEASURED red, not assumed red
//!
//! Before the closure check existed, `verify_artifact(&EchoCodec)` returned `Ok`
//! and minted `ArtifactReloadedAndVerified` — because `EchoCodec`'s cached bundle
//! is internally consistent, so the rebuilt model reproduced the cached bundle's
//! own answers. The check is what turned it red. An in-band negative that has only
//! ever been observed passing is not a negative (Ph1 D-24 / Ph2 D-25).

use super::super::test_fixtures as fx;
use super::*;
use crate::train::setfit::bundle::{BundleError, SetFitBundle};
use crate::train::setfit::tune::TuningProbes;
use crate::train::setfit::{
    digest_of_ordered, ArtifactReloadedAndVerified, HeadFitted, Prepared, SetFitRun,
    SetFitTrainError,
};

// ===========================================================================================
// Fixtures
// ===========================================================================================

/// A complete calibrated pipeline up to `HeadFitted`.
fn head_fitted_run() -> SetFitRun<HeadFitted> {
    fx::head_fitted_run(fx::calibrated_variant())
}

/// The same pipeline, with the intra-batch pull order reversed.
///
/// The probe changes only the ORDER pairs are drawn inside a batch. Batch
/// structure, batch count, start ordinals and lengths are untouched, so a digest
/// reconstructed from configuration would be identical for both runs.
fn perturbed_run() -> Result<SetFitRun<HeadFitted>, SetFitTrainError> {
    SetFitRun::<Prepared>::tune_encoder_with_probes(
        fx::prepared_run(fx::calibrated_variant(), None),
        TuningProbes::REVERSE_INTRA_BATCH_PULL,
    )?
    .fit_head()
}

/// A verified run through the shipped codec.
fn verified_run() -> SetFitRun<ArtifactReloadedAndVerified> {
    fx::verified_run(fx::calibrated_variant())
}

// ===========================================================================================
// The happy path
// ===========================================================================================

/// The full pipeline reaches the final state and carries the artifact's identity.
#[test]
fn verify_full_pipeline_reaches_the_final_state() {
    let run = verified_run();
    assert_eq!(run.state_name(), "artifact_reloaded_and_verified");
    assert_eq!(run.artifact_format_id(), SERDE_JSON_FORMAT_ID);

    let hash = run.artifact_hash();
    assert_eq!(hash.len(), 64, "a SHA-256 renders as 64 hex characters");
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "the artifact hash must be lowercase hex, got `{hash}`",
    );

    let report = run.evidence().verify_report();
    assert!(report.probe_rows() > 0, "the verification must have compared rows");
    assert!(report.embedding_dim() > 0);
    assert_eq!(report.class_count(), 3, "the fixture declares three classes");
    assert!(report.artifact_bytes() > 0);
}

/// The reloaded model's answers EXACTLY equal the pre-close ones.
///
/// Exact, not "within a tolerance": this codec's tolerance is zero, so the
/// verification passing already means the maxima are zero. Asserting the recorded
/// maxima as well is what distinguishes "matched exactly" from "matched inside a
/// tolerance somebody widened".
#[test]
fn verify_reloaded_answers_are_exactly_the_pre_close_ones() {
    let run = verified_run();
    let report = run.evidence().verify_report();

    assert_eq!(
        report.tolerance_embedding_abs(),
        0.0,
        "the serde codec verifies at exact-zero tolerance",
    );
    assert_eq!(report.tolerance_probability_abs(), 0.0);
    assert_eq!(
        report.max_embedding_abs_diff(),
        0.0,
        "an embedding element differed across the persistence boundary",
    );
    assert_eq!(
        report.max_probability_abs_diff(),
        0.0,
        "a class probability differed across the persistence boundary",
    );

    let probe = run.probe_predictions();
    assert_eq!(probe.ids().len(), probe.embeddings().len());
    assert_eq!(probe.ids().len(), probe.probabilities().len());
    assert_eq!(probe.ids().len(), probe.labels().len());
    assert_eq!(
        probe.ids(),
        run.evidence().encode_ledger(),
        "the probe must have walked the same rows in the same order the head was fitted on",
    );
    for (row, p) in probe.probabilities().iter().enumerate() {
        let total: f64 = p.iter().sum();
        assert!((total - 1.0).abs() < 1e-6, "row {row}: reloaded probabilities sum to {total}",);
    }
}

/// The round-trip closure check ran and PASSED for the faithful codec.
///
/// The control for the `EchoCodec` negative below. Without it, that negative would
/// be satisfied by a check that refuses every codec.
#[test]
fn verify_round_trip_closure_holds_for_the_faithful_codec() {
    let run = verified_run();
    assert!(
        run.evidence().verify_report().round_trip_closed(),
        "re-serializing the reloaded bundle must have reproduced the hashed bytes",
    );
}

// ===========================================================================================
// The cheating codec
// ===========================================================================================

/// A codec whose `deserialize` ignores its input entirely.
///
/// See this module's docs for what the negative it drives proves and does not.
struct EchoCodec {
    /// SET ONCE at construction. `serialize` must NOT refresh it: if it did, the
    /// codec would echo the genuine bundle back and the negative would be
    /// vacuously green.
    cached: SetFitBundle,
}

impl EchoCodec {
    fn new(cached: SetFitBundle) -> Self {
        Self { cached }
    }
}

impl super::sealed::Sealed for EchoCodec {}

impl SetFitCodec for EchoCodec {
    fn format_id(&self) -> &'static str {
        "setfit-echo-test-v1"
    }

    /// Honest. The dishonesty is entirely in `deserialize`.
    fn serialize(&self, bundle: &SetFitBundle) -> Result<Vec<u8>, CodecError> {
        bundle.to_canonical_bytes().map_err(|source| CodecError::Bundle {
            format_id: "setfit-echo-test-v1".to_string(),
            source,
        })
    }

    /// Ignores `bytes` completely and returns the construction-time cache.
    fn deserialize(&self, _bytes: &[u8]) -> Result<SetFitBundle, CodecError> {
        Ok(self.cached.clone())
    }
}

/// A cache that is not the run's bundle, and that NOTHING ELSE would reject.
///
/// # Choosing the perturbation is most of this test
///
/// The first attempt appended a label. That was refused — by the head's
/// coefficient-arity check, because a label without a weight row is a malformed
/// bundle. The negative would then have been green while proving nothing about the
/// closure check: remove the check and it stays green, for the wrong reason.
///
/// A perturbed TENSOR has the same problem one step later: the rebuilt model
/// answers differently, so the probe comparison refuses it.
///
/// `source_revision` is the perturbation with no second catcher. It is pure
/// PROVENANCE: no rebuild step reads it, no comparison touches it, and a model
/// rebuilt from this cache reproduces every pre-close answer exactly. So the
/// cached bundle describes a model that behaves identically and is NOT the
/// artifact that was hashed — which is the subtlest form of the attack, and the
/// only one that isolates the round-trip closure check as the thing doing the work.
fn echo_cache(run: &SetFitRun<HeadFitted>) -> SetFitBundle {
    let mut bundle = SetFitBundle::from_run_parts(
        "setfit-echo-test-v1",
        run.encoder(),
        run.evidence().head(),
        run.evidence().ordered_labels(),
        run.config(),
        run.evidence().passed().summary(),
    )
    .expect("the cache must be a well-formed bundle");
    assert_ne!(
        bundle.architecture.source_revision, "0000000",
        "the perturbation must actually change the field",
    );
    bundle.architecture.source_revision = "0000000".to_string();
    bundle
}

/// A codec that returns an object it had lying around is REFUSED.
#[test]
fn verify_echo_codec_cannot_mint_the_final_state() {
    let run = head_fitted_run();
    let codec = EchoCodec::new(echo_cache(&run));

    match run.verify_artifact(&codec) {
        Err(SetFitTrainError::ReloadNotFromBytes {
            hashed_len,
            reserialized_len,
            first_diff_offset,
        }) => {
            assert!(hashed_len > 0, "the artifact must have been serialized");
            assert!(reserialized_len > 0);
            assert!(
                hashed_len != reserialized_len || first_diff_offset.is_some(),
                "a refusal must name a real difference: either the lengths differ or an \
                 offset does",
            );
        }
        Err(other) => panic!(
            "the cheating codec must be refused by the round-trip closure check, not by \
             something else: {other}"
        ),
        Ok(_) => panic!(
            "a codec that ignores its input bytes MINTED the final state. The reloaded value \
             is not a function of the artifact, so no persistence boundary was crossed."
        ),
    }
}

/// A codec handed deliberately corrupted bytes fails typed, and no state is minted.
///
/// The corruption is injected on the CODEC, not on the policy: the policy is the
/// trusted half, and injecting there would be testing a mutation of the thing
/// under test rather than the thing.
struct CorruptingCodec {
    inner: SerdeJsonCodec,
}

impl super::sealed::Sealed for CorruptingCodec {}

impl SetFitCodec for CorruptingCodec {
    fn format_id(&self) -> &'static str {
        SERDE_JSON_FORMAT_ID
    }

    fn serialize(&self, bundle: &SetFitBundle) -> Result<Vec<u8>, CodecError> {
        self.inner.serialize(bundle)
    }

    /// Flip one structural byte before parsing, exactly as a damaged medium would.
    fn deserialize(&self, bytes: &[u8]) -> Result<SetFitBundle, CodecError> {
        let mut damaged = bytes.to_vec();
        if let Some(first) = damaged.first_mut() {
            *first = b'~';
        }
        self.inner.deserialize(&damaged)
    }
}

/// A codec that is honest about its bytes but decodes a payload belonging to ANOTHER format.
///
/// It declares a format id of its own while handing back a bundle stamped
/// `SERDE_JSON_FORMAT_ID`. Nothing in its own `deserialize` checks that — which is the whole
/// point: it stands in for a future implementor (phase 4's `AprCodec`) that simply forgot to
/// write the guard. The check must come from the module, not from the implementor's care.
struct ForeignFormatCodec {
    inner: SerdeJsonCodec,
}

impl super::sealed::Sealed for ForeignFormatCodec {}

impl SetFitCodec for ForeignFormatCodec {
    fn format_id(&self) -> &'static str {
        "setfit-bundle-someone-elses-format-v1"
    }

    fn serialize(&self, bundle: &SetFitBundle) -> Result<Vec<u8>, CodecError> {
        self.inner.serialize(bundle)
    }

    /// Faithful to its bytes, and silent about whose format they are.
    fn deserialize(&self, bytes: &[u8]) -> Result<SetFitBundle, CodecError> {
        self.inner.deserialize(bytes)
    }
}

/// A codec that omits the format-id guard is still refused, because `decode` makes it.
///
/// # Why this exercises `decode` and not `verify_artifact`
///
/// It CANNOT be reached through `verify_artifact`: `close` stamps the payload with
/// `codec.format_id()`, so within one policy run the written id and the reading codec always
/// agree by construction. (That was measured — an earlier draft of this test drove it through
/// `verify_artifact` and the run returned `Ok`, which is the correct answer for that path.)
/// The mismatch only arises where bytes cross BETWEEN codecs, which is the codec surface
/// plan 03-10 exposes and phase 4 adds a second implementor to.
///
/// So the falsification is aimed at `decode` directly: `ForeignFormatCodec` checks nothing of
/// its own, is handed bytes written by `SerdeJsonCodec`, and the refusal must still arrive
/// naming both ids.
#[test]
fn verify_a_codec_that_omits_the_format_check_is_still_refused() {
    let run = head_fitted_run();
    let honest = SerdeJsonCodec::new();
    let bundle = SetFitBundle::from_run_parts(
        honest.format_id(),
        run.encoder(),
        run.evidence().head(),
        run.evidence().ordered_labels(),
        run.config(),
        run.evidence().passed().summary(),
    )
    .expect("a well-formed bundle in the honest codec's format");
    let bytes = honest.serialize(&bundle).expect("the honest codec serializes");

    let forgetful = ForeignFormatCodec { inner: SerdeJsonCodec::new() };
    // Its own `deserialize` is happy — it never looks at the id.
    forgetful.deserialize(&bytes).expect("the forgetful codec itself raises nothing");

    // Trusted `decode` refuses on its behalf.
    match super::decode(&forgetful, &bytes) {
        Err(CodecError::ForeignFormat { expected, got }) => {
            assert_eq!(expected, "setfit-bundle-someone-elses-format-v1");
            assert_eq!(got, SERDE_JSON_FORMAT_ID, "the id the payload actually declares");
        }
        other => panic!("expected a typed ForeignFormat refusal, got {other:?}"),
    }
}

#[test]
fn verify_corrupted_bytes_fail_typed_and_mint_nothing() {
    let run = head_fitted_run();
    let codec = CorruptingCodec { inner: SerdeJsonCodec::new() };

    match run.verify_artifact(&codec) {
        Err(SetFitTrainError::Codec(CodecError::Bundle { format_id, source })) => {
            assert_eq!(format_id, SERDE_JSON_FORMAT_ID);
            match source {
                BundleError::Serialization { context, detail } => {
                    assert_eq!(context, "parse", "the failure site must be named");
                    assert!(
                        detail.contains("line") || detail.contains("column"),
                        "the parse failure should carry its position, got `{detail}`",
                    );
                }
                other => panic!("expected a typed parse failure, got {other:?}"),
            }
        }
        other => panic!("corrupted bytes must fail typed at the codec, got {other:?}"),
    }
}

// ===========================================================================================
// The seam's shape — source assertions
// ===========================================================================================

/// The codec trait has EXACTLY three methods, and none of them decides anything.
#[test]
fn verify_codec_trait_is_a_pure_sealed_codec() {
    let src = include_str!("verify.rs");
    let at = src
        .find("pub trait SetFitCodec: sealed::Sealed {")
        .expect("the trait must be declared sealed");
    let end = at + src[at..].find("\n}\n").expect("the trait must close");
    let body = &src[at..end];

    let methods: Vec<&str> = body.lines().filter(|l| l.trim_start().starts_with("fn ")).collect();
    assert_eq!(methods.len(), 3, "the codec must have exactly three methods, found: {methods:?}",);
    assert!(body.contains("fn format_id(&self) -> &'static str;"));
    assert!(
        body.contains("fn serialize(&self, bundle: &SetFitBundle) -> Result<Vec<u8>, CodecError>;")
    );
    assert!(
        body.contains("fn deserialize(&self, bytes: &[u8]) -> Result<SetFitBundle, CodecError>;")
    );

    for forbidden in ["hash", "Tolerance", "tolerance", "compare", "verify", "SetFitRun"] {
        assert!(
            !body.contains(forbidden),
            "the codec must not be able to {forbidden}; a trait method mentioning it would \
             hand an implementor a power the policy split exists to remove. Body:\n{body}",
        );
    }
}

/// `artifact_hash` is a FREE FUNCTION, not a trait method.
#[test]
fn verify_artifact_hash_is_a_free_function() {
    let src = include_str!("verify.rs");
    assert!(
        src.contains("pub(crate) fn artifact_hash(bytes: &[u8]) -> [u8; 32] {"),
        "artifact_hash must be a free function in this module",
    );
    let at = src.find("pub trait SetFitCodec").expect("the trait must exist");
    let end = at + src[at..].find("\n}\n").expect("the trait must close");
    assert!(
        !src[at..end].contains("artifact_hash"),
        "a codec that hashed its own output could report any digest for any bytes",
    );
}

/// `SerdeJsonCodec` is `pub` — plan 03-10's out-of-crate test constructs one.
#[test]
fn verify_serde_codec_is_publicly_constructible() {
    let src = include_str!("verify.rs");
    assert!(src.contains("pub struct SerdeJsonCodec;"));
    assert!(src.contains("pub const fn new() -> Self {"));
    // And it really constructs, through the public path, with no crate-internal help.
    let codec = SerdeJsonCodec::new();
    assert_eq!(codec.format_id(), SERDE_JSON_FORMAT_ID);
    assert!(
        !SERDE_JSON_FORMAT_ID.to_ascii_lowercase().contains("apr"),
        "the interim serde format must not claim the shipped container's name",
    );
}

/// The comparison object traces to the reloaded bytes, not to the pre-close model.
///
/// A SOURCE assertion, because the property is about what the code CAN reach. The
/// policy takes the encoder and head BY VALUE into `close`, so after that call
/// there is no binding to the live model at all — the borrow checker enforces it,
/// and this test states the shape a reader should expect to find.
#[test]
fn verify_policy_closes_the_live_model_before_it_reloads() {
    let src = include_str!("verify.rs");
    assert!(
        src.contains("    encoder: SetFitMiniLm,\n    head: MultinomialLogisticRegression,"),
        "close/run_verify_policy must take the live model BY VALUE",
    );

    let at = src.find("pub(crate) fn run_verify_policy").expect("the policy must exist");
    let body = &src[at..];
    let close_at = body.find("close(").expect("the policy must close the artifact");
    let deserialize_at = body.find("decode(codec,").expect("the policy must reload from bytes");
    let closure_at = body
        .find("close_round_trip(codec, &reloaded, &bytes)")
        .expect("the policy must re-serialize the reloaded bundle and compare the bytes");
    let rebuild_at = body
        .find("rebuild_from(&reloaded)")
        .expect("the policy must rebuild from the RELOADED bundle");
    let probe_at = body
        .find("probe_model(&mut rebuilt_encoder")
        .expect("the policy must re-probe the REBUILT model");
    let compare_at = body.find("compare_probes(").expect("the policy must compare");
    assert!(
        close_at < deserialize_at && deserialize_at < closure_at && closure_at < rebuild_at,
        "the order must be close -> deserialize -> closure check -> rebuild; the closure \
         check has to precede the rebuild, or a codec whose cache described a behaviourally \
         identical model would pass the comparison and never be seen",
    );
    assert!(rebuild_at < probe_at, "the re-probe must follow the rebuild");
    assert!(probe_at < compare_at, "the comparison must come last");

    // The closure check compares the RE-SERIALIZED bytes against the HASHED ones.
    let closure_fn =
        src.find("fn close_round_trip<C: SetFitCodec>(").expect("close_round_trip must exist");
    let closure_body = &src[closure_fn..closure_fn + 900];
    assert!(
        closure_body.contains("codec.serialize(reloaded)"),
        "the closure check must call the codec's serialize a SECOND time",
    );
    assert!(
        closure_body.contains("if reserialized == hashed"),
        "the closure check must compare the two byte streams",
    );

    // `rebuild_from` reads the reloaded bundle and nothing else.
    let rebuild_fn = src.find("fn rebuild_from(").expect("rebuild_from must exist");
    let rebuild_body = &src[rebuild_fn..rebuild_fn + 1400];
    assert!(
        rebuild_body.contains("SetFitMiniLm::from_bundle_parts"),
        "the rebuild must go through the bytes-only door",
    );
    assert!(
        rebuild_body.contains("from_stored_coefficients"),
        "the head must be rebuilt from the artifact's stored coefficients",
    );
}

/// The final state's evidence type is not an `Option`.
#[test]
fn verify_final_state_has_a_non_option_evidence_type() {
    let src = include_str!("mod.rs");
    assert!(
        src.contains("impl LifecycleState for ArtifactReloadedAndVerified {"),
        "the final state must implement LifecycleState",
    );
    assert!(
        src.contains("type Evidence = ArtifactVerifiedEvidence;"),
        "its evidence must be the named struct, not an Option",
    );
    assert!(
        !src.contains("type Evidence = Option<"),
        "the absent-field pattern forbids an Option evidence type",
    );
}

// ===========================================================================================
// The reproducibility surface
// ===========================================================================================

/// Every reproducibility accessor exists, is `pub`, and is read-only — proved by COMPILING.
///
/// # Why this is not a source-text scan any more
///
/// It used to `include_str!("mod.rs")` and assert that ten `pub fn` signatures appeared and
/// that the block contained no `&mut self` / `(self)`. That was the wrong altitude twice
/// over. It asserted `expected.len() == 10` against a ten-element literal — a tautology the
/// compiler already knows — and, being a substring search, it was blind to the very drift it
/// existed to catch: an ELEVENTH accessor, `artifact_format_id`, was added and the test
/// stayed green because nothing counted what was actually there. It also went red on a
/// rustfmt reflow that changed no behaviour.
///
/// Binding a SHARED reference and calling through it moves all of that to compile time: a
/// `&mut self` or by-value receiver is not callable through `&T`, an absent or non-`pub`
/// accessor is a name error, and a changed return type is a type error. None of it can pass
/// by accident.
#[test]
fn verify_reproducibility_accessors_are_read_only_and_complete() {
    let run = verified_run();

    // The whole proof: `r` is a SHARED reference, so every call below is a compile-time
    // assertion that the accessor takes `&self` and hands back a read-only view.
    let r: &SetFitRun<ArtifactReloadedAndVerified> = &run;

    let _: String = r.selection_semantic_hash();
    let _: &str = r.pair_order_digest();
    let _: &[(u32, u64, u32)] = r.batch_boundaries();
    let _: u64 = r.step_count();
    let _: &str = r.loss_trace_hash();
    let _: &str = r.evidence_table_hash();
    let _: &str = r.parameter_registry_hash();
    let _: String = r.encode_ledger_hash();
    let _: String = r.artifact_hash();
    let _: &VerifyProbe = r.probe_predictions();
    let _: &str = r.artifact_format_id();

    // EXHAUSTIVENESS is the one property the calls above cannot carry: they prove each
    // listed accessor exists, not that the list is the WHOLE surface. So count the block's
    // declarations and require the number to match. This is the assertion the old form was
    // trying and failing to make — keep it in step when the surface changes on purpose.
    const ACCESSORS_CALLED_ABOVE: usize = 11;
    let src = include_str!("mod.rs");
    let at = src
        .find("impl SetFitRun<ArtifactReloadedAndVerified> {")
        .expect("the accessor block must exist");
    let block = &src[at..];
    let end = block.find("\n}\n").expect("the block must close");
    let declared = block[..end].matches("\n    pub fn ").count();
    assert_eq!(
        declared, ACCESSORS_CALLED_ABOVE,
        "the accessor block declares {declared} `pub fn`s but this test exercises \
         {ACCESSORS_CALLED_ABOVE}; add the new accessor to the calls above (which is what \
         proves it read-only) and bump the count",
    );
}

/// `pair_order_digest()` returns the digest 03-05's loop RECORDED.
#[test]
fn verify_pair_order_digest_is_the_recorded_one() {
    let run = verified_run();
    let recorded = run.evidence().passed().table().consumed_pair_digest.clone();
    assert!(!recorded.is_empty(), "the loop must have recorded a digest");
    assert_eq!(
        run.pair_order_digest(),
        recorded,
        "the accessor must MOVE the recorded digest through, not restate it",
    );

    // The other recorded values travel the same way.
    assert_eq!(
        run.batch_boundaries(),
        run.evidence().passed().table().batch_boundary_list.as_slice(),
    );
    assert_eq!(run.step_count(), run.evidence().passed().table().step_count);
    assert_eq!(run.loss_trace_hash(), run.evidence().passed().table().loss_trace_hash,);
    assert_eq!(
        run.parameter_registry_hash(),
        run.evidence().passed().table().parameter_registry_hash,
    );
    assert_eq!(run.evidence_table_hash(), run.evidence().passed().summary().table_hash,);
}

/// A run whose CONSUMPTION ORDER was perturbed reports a DIFFERENT digest.
///
/// This is the test a recomputed accessor cannot pass. The probe reverses only the
/// intra-batch pull order: batch structure, batch count, start ordinals and lengths
/// are identical, and so is every configuration knob. An accessor that rebuilt the
/// digest from configuration would therefore report the SAME value for both runs
/// and this test would be the one that noticed.
#[test]
fn verify_pair_order_digest_changes_when_consumption_order_changes() {
    let forward = verified_run();
    let reversed = perturbed_run()
        .expect("the perturbed pipeline must still reach HeadFitted")
        .verify_artifact(&SerdeJsonCodec::new())
        .expect("the perturbed run must still verify");

    // Non-vacuity: the two runs agree on everything the probe did NOT touch.
    assert_eq!(
        forward.batch_boundaries(),
        reversed.batch_boundaries(),
        "the probe must leave the batch STRUCTURE alone, or this test is too coarse",
    );
    assert_eq!(forward.step_count(), reversed.step_count());
    assert_eq!(
        forward.selection_semantic_hash(),
        reversed.selection_semantic_hash(),
        "both runs draw the same rows",
    );

    assert_ne!(
        forward.pair_order_digest(),
        reversed.pair_order_digest(),
        "a pair digest that survives a real intra-batch reordering is a RECOMPUTED digest, \
         and two runs that consumed different orders would reproduce each other perfectly",
    );
}

/// `encode_ledger_hash` digests the RECORDED ledger, and is order-sensitive.
#[test]
fn verify_encode_ledger_hash_digests_the_recorded_ledger() {
    let run = verified_run();
    let ledger = run.evidence().encode_ledger().to_vec();
    assert!(!ledger.is_empty(), "the encode-once path must have recorded a ledger");
    assert_eq!(run.encode_ledger_hash(), digest_of_ordered(&ledger));

    let mut reordered = ledger.clone();
    reordered.reverse();
    assert_ne!(
        digest_of_ordered(&ledger),
        digest_of_ordered(&reordered),
        "an encode-ledger digest that survives a reordering is not evidence of order",
    );
}

/// Two identical pipelines agree on every reproducibility value, including the artifact hash.
#[test]
fn verify_two_identical_pipelines_agree_on_the_whole_surface() {
    let first = verified_run();
    let second = verified_run();

    assert_eq!(first.selection_semantic_hash(), second.selection_semantic_hash());
    assert_eq!(first.pair_order_digest(), second.pair_order_digest());
    assert_eq!(first.batch_boundaries(), second.batch_boundaries());
    assert_eq!(first.step_count(), second.step_count());
    assert_eq!(first.loss_trace_hash(), second.loss_trace_hash());
    assert_eq!(first.evidence_table_hash(), second.evidence_table_hash());
    assert_eq!(first.parameter_registry_hash(), second.parameter_registry_hash());
    assert_eq!(first.encode_ledger_hash(), second.encode_ledger_hash());
    assert_eq!(
        first.artifact_hash(),
        second.artifact_hash(),
        "the whole path is deterministic, so two runs must produce the same artifact",
    );
    assert_eq!(first.probe_predictions().labels(), second.probe_predictions().labels(),);
}
