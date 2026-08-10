//! The codec seam and the trusted verify-by-reload policy (plan 03-08, D-07, TRN-01).
//!
//! # The seam is implementable and powerless
//!
//! [`SetFitCodec`] is a PURE CODEC: a format identifier and a bytes <-> bundle
//! pair, and nothing else. It has no hashing method, no tolerance parameter, no
//! comparison method and no way to construct a lifecycle state. Everything that
//! DECIDES anything — the artifact hash, the close/reload/re-encode/re-predict
//! sequence, the comparison, and the minting of
//! [`ArtifactReloadedAndVerified`](super::ArtifactReloadedAndVerified) — lives in
//! trusted crate-internal code below and cannot be overridden.
//!
//! The split exists because the earlier shape did not have it. A single
//! `ReloadVerify` trait owning `reload()` lets an implementor return the
//! pre-close bundle unchanged: the comparison then passes and no persistence
//! boundary ever existed. Reducing the trait to a codec removes the comparison
//! and the minting from an implementor's reach, which is the substantive half of
//! that fix; the seal below is the conservative other half.
//!
//! # The trait is SEALED, and what that costs phase 4
//!
//! `sealed::Sealed` is a private-module supertrait, so no out-of-crate type can
//! implement [`SetFitCodec`]. Sealing later would be a breaking change and
//! un-sealing later is not, so phase 3 seals. The consequence is concrete: phase
//! 4's APR codec lands as a thin ADAPTER module inside this crate — an
//! `impl Sealed for AprCodec` plus a `SetFitCodec` impl that calls
//! `aprender-core`'s APR format code. The APR FORMAT itself stays in
//! `aprender-core`; only the adapter moves. If an openly implementable codec is
//! wanted later, deleting the supertrait is a non-breaking change.
//!
//! # CANONICAL-SERIALIZATION OBLIGATION
//!
//! Implementors MUST be byte-canonical: re-serializing a bundle obtained from
//! [`SetFitCodec::deserialize`] must reproduce the input bytes exactly. This is a
//! real constraint, not a free property. It holds for [`SerdeJsonCodec`] because
//! the bundle uses `BTreeMap` ordering, declaration-order fields and bit-pattern
//! floats — and it took a Cargo feature to make even that true (`float_roundtrip`;
//! without it serde_json's float parser lands one ULP away and the check fails on
//! every honest bundle). A phase-4 writer with padding, unordered metadata or a
//! checksum placed after the payload would fail [`SetFitTrainError::ReloadNotFromBytes`]
//! and be blocked until someone edited the contract.
//!
//! It is stated twice on purpose — here and as an explicit clause of the
//! `reload_verify_roundtrip` equation — so that a phase-4 author MEETS it or
//! amends it deliberately, rather than discovering it as a mysterious
//! verification failure.

use std::collections::BTreeMap;

use aprender::classification::MultinomialLogisticRegression;
use aprender::setfit::SetFitMiniLm;
use aprender_contrastive_data::prepared::{Canonical, PreparedDataset};
use aprender_contrastive_data::select::Selection;
use sha2::{Digest, Sha256};

use super::bundle::{BundleError, SetFitBundle};
use super::config::ResolvedSetFitConfig;
use super::evidence::EvidenceSummary;
use super::{head_input, ArtifactVerifiedEvidence, SetFitTrainError, VerifyProbe, VerifyReport};

/// The seal. Private module, public-in-private trait — the standard Rust idiom.
mod sealed {
    /// Implemented only by codecs declared inside this crate.
    pub trait Sealed {}
}

/// The identifier of the phase-3 serde codec.
///
/// It never claims to be the project's shipped model container: an interim debug
/// format wearing that name would make a phase-4 reader believe it had found the
/// real thing.
pub const SERDE_JSON_FORMAT_ID: &str = "setfit-serde-json-v1";

// ===========================================================================================
// The codec seam
// ===========================================================================================

/// A bytes <-> [`SetFitBundle`] codec, and nothing else.
///
/// Three methods. None hashes, none compares, none carries a tolerance, and none
/// can construct a lifecycle state. See the module docs for the sealing decision
/// and the canonical-serialization obligation every implementor must satisfy.
pub trait SetFitCodec: sealed::Sealed {
    /// The identifier this codec stamps into, and expects to find in, a payload.
    fn format_id(&self) -> &'static str;

    /// Encode a bundle.
    ///
    /// # Errors
    ///
    /// [`CodecError`] naming this codec's format and the underlying failure.
    fn serialize(&self, bundle: &SetFitBundle) -> Result<Vec<u8>, CodecError>;

    /// Decode a bundle.
    ///
    /// # Errors
    ///
    /// [`CodecError`] naming this codec's format and the underlying failure.
    fn deserialize(&self, bytes: &[u8]) -> Result<SetFitBundle, CodecError>;
}

/// A codec-level failure, carrying the typed bundle error underneath it.
///
/// The inner [`BundleError`] is preserved rather than rendered to a string: a
/// caller must be able to tell a contracted limit from a parse failure from a
/// schema-version refusal without matching on message text.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CodecError {
    /// The bundle layer refused the payload.
    Bundle {
        /// The codec that was reading or writing.
        format_id: String,
        /// What the bundle layer said.
        source: BundleError,
    },
    /// The payload was written by a different codec.
    ForeignFormat {
        /// The identifier this codec owns.
        expected: String,
        /// The identifier the payload declares.
        got: String,
    },
}

impl core::fmt::Display for CodecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Bundle { format_id, source } => {
                write!(f, "codec `{format_id}`: {source}")
            }
            Self::ForeignFormat { expected, got } => write!(
                f,
                "codec `{expected}` was handed a payload written by `{got}`; a codec reads \
                 only its own format",
            ),
        }
    }
}

impl std::error::Error for CodecError {}

/// The phase-3 codec: the bundle's own canonical JSON.
///
/// `pub`, and reachable from outside the crate on purpose — plan 03-10's
/// out-of-crate integration test constructs one directly, and a `pub(crate)` type
/// would make that test unwritable without a backdoor. It is still SEALED against
/// out-of-crate *implementations* of the trait, which is the property that matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SerdeJsonCodec;

impl SerdeJsonCodec {
    /// The codec. It has no state and no configuration.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl sealed::Sealed for SerdeJsonCodec {}

impl SetFitCodec for SerdeJsonCodec {
    fn format_id(&self) -> &'static str {
        SERDE_JSON_FORMAT_ID
    }

    fn serialize(&self, bundle: &SetFitBundle) -> Result<Vec<u8>, CodecError> {
        bundle.to_canonical_bytes().map_err(|source| CodecError::Bundle {
            format_id: SERDE_JSON_FORMAT_ID.to_string(),
            source,
        })
    }

    fn deserialize(&self, bytes: &[u8]) -> Result<SetFitBundle, CodecError> {
        let bundle = SetFitBundle::from_canonical_bytes(bytes).map_err(|source| {
            CodecError::Bundle { format_id: SERDE_JSON_FORMAT_ID.to_string(), source }
        })?;
        if bundle.format_id() != SERDE_JSON_FORMAT_ID {
            return Err(CodecError::ForeignFormat {
                expected: SERDE_JSON_FORMAT_ID.to_string(),
                got: bundle.format_id().to_string(),
            });
        }
        Ok(bundle)
    }
}

// ===========================================================================================
// Trusted policy — crate-internal, not overridable
// ===========================================================================================

/// SHA-256 over an artifact's canonical bytes.
///
/// A FREE FUNCTION and deliberately not a trait method: a codec that hashed its
/// own output could report any digest it liked for any bytes, and the digest is
/// the only thing tying a verified run to a specific artifact.
pub(crate) fn artifact_hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// The comparison tolerance. Crate-internal; no codec can set one.
///
/// [`SerdeJsonCodec`]'s verification runs at [`Self::EXACT`]. The type exists
/// because phase 4's format may need a contracted non-zero tolerance, and a
/// tolerance that arrives at that point with nowhere to live tends to arrive as a
/// parameter on the trait — which is exactly the power this split removed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tolerance {
    /// Maximum accepted absolute difference between two embedding elements.
    pub(crate) embedding_abs: f32,
    /// Maximum accepted absolute difference between two class probabilities.
    pub(crate) probability_abs: f64,
}

impl Tolerance {
    /// No difference at all is accepted.
    pub(crate) const EXACT: Self = Self { embedding_abs: 0.0, probability_abs: 0.0 };
}

/// What the pre-close model answered, recorded before it was dropped.
///
/// Built by [`close`], which CONSUMES the live model, so this is the only thing
/// that survives it.
pub(crate) struct ClosedArtifact {
    /// The serialized artifact.
    pub(crate) bytes: Vec<u8>,
    /// SHA-256 of exactly those bytes.
    pub(crate) hash: [u8; 32],
    /// The pre-close probe.
    pub(crate) probe: VerifyProbe,
}

/// Serialize the run's state, hash it, and DROP the live model.
///
/// # The close is structural, not a discipline
///
/// `encoder` and `head` are taken BY VALUE and never returned. They go out of
/// scope when this function does, so the caller has no binding to the live model
/// afterwards and cannot accidentally compare against it — the borrow checker
/// enforces what a `drop(...)` call would only document.
fn close<C: SetFitCodec>(
    codec: &C,
    mut encoder: SetFitMiniLm,
    head: MultinomialLogisticRegression,
    ordered_labels: &[String],
    dataset: &PreparedDataset<Canonical>,
    selection: &Selection,
    config: &ResolvedSetFitConfig,
    summary: &EvidenceSummary,
) -> Result<ClosedArtifact, SetFitTrainError> {
    // The probe is taken FIRST, while the live model still exists: it is the
    // baseline the reloaded model is judged against.
    let probe = probe_model(&mut encoder, &head, dataset, selection, config)?;

    let bundle = SetFitBundle::from_run_parts(
        codec.format_id(),
        &encoder,
        &head,
        ordered_labels,
        config,
        summary,
    )
    .map_err(SetFitTrainError::Bundle)?;
    let bytes = codec.serialize(&bundle).map_err(SetFitTrainError::Codec)?;
    let hash = artifact_hash(&bytes);

    Ok(ClosedArtifact { bytes, hash, probe })
}

/// Encode the selected rows through THIS model and predict with THIS head.
///
/// The encode goes through `head_input::head_dataset`, which is the encode-once
/// path stage two itself uses — eval mode, inside `no_grad`, every result
/// detached. Using a second encode path here would compare the reloaded model
/// against something the trainer never ran.
fn probe_model(
    encoder: &mut SetFitMiniLm,
    head: &MultinomialLogisticRegression,
    dataset: &PreparedDataset<Canonical>,
    selection: &Selection,
    config: &ResolvedSetFitConfig,
) -> Result<VerifyProbe, SetFitTrainError> {
    let input =
        head_input::head_dataset(encoder, dataset, selection, config.requested().batch_size())?;
    let embeddings = input.embeddings().to_vec();
    let probabilities = head.predict_proba(&embeddings).map_err(SetFitTrainError::HeadFit)?;
    let labels = head.predict(&embeddings).map_err(SetFitTrainError::HeadFit)?;
    Ok(VerifyProbe { ids: input.encode_ledger().to_vec(), embeddings, probabilities, labels })
}

/// Compare two probes at `tolerance`, naming the first divergence.
fn compare_probes(
    before: &VerifyProbe,
    after: &VerifyProbe,
    tolerance: Tolerance,
) -> Result<(f32, f64), SetFitTrainError> {
    let diverged = |field: &'static str,
                    row: usize,
                    index: usize,
                    expected: String,
                    observed: String,
                    bound: f64| {
        SetFitTrainError::ReloadDiverged { field, row, index, expected, observed, tolerance: bound }
    };

    if before.ids.len() != after.ids.len() {
        return Err(diverged(
            "probe_row_count",
            0,
            0,
            before.ids.len().to_string(),
            after.ids.len().to_string(),
            0.0,
        ));
    }
    for (row, (a, b)) in before.ids.iter().zip(after.ids.iter()).enumerate() {
        if a != b {
            return Err(diverged("probe_id", row, 0, a.clone(), b.clone(), 0.0));
        }
    }

    let mut max_embedding: f32 = 0.0;
    for (row, (a, b)) in before.embeddings.iter().zip(after.embeddings.iter()).enumerate() {
        if a.len() != b.len() {
            return Err(diverged(
                "embedding_width",
                row,
                0,
                a.len().to_string(),
                b.len().to_string(),
                0.0,
            ));
        }
        for (index, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            let delta = (x - y).abs();
            if delta > max_embedding {
                max_embedding = delta;
            }
            if !(delta <= tolerance.embedding_abs) {
                return Err(diverged(
                    "embedding",
                    row,
                    index,
                    format!("{x:e}"),
                    format!("{y:e}"),
                    f64::from(tolerance.embedding_abs),
                ));
            }
        }
    }

    let mut max_probability: f64 = 0.0;
    for (row, (a, b)) in before.probabilities.iter().zip(after.probabilities.iter()).enumerate() {
        if a.len() != b.len() {
            return Err(diverged(
                "class_count",
                row,
                0,
                a.len().to_string(),
                b.len().to_string(),
                0.0,
            ));
        }
        for (index, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            let delta = (x - y).abs();
            if delta > max_probability {
                max_probability = delta;
            }
            if !(delta <= tolerance.probability_abs) {
                return Err(diverged(
                    "probability",
                    row,
                    index,
                    format!("{x:e}"),
                    format!("{y:e}"),
                    tolerance.probability_abs,
                ));
            }
        }
    }

    for (row, (a, b)) in before.labels.iter().zip(after.labels.iter()).enumerate() {
        if a != b {
            return Err(diverged("label", row, 0, a.clone(), b.clone(), 0.0));
        }
    }

    Ok((max_embedding, max_probability))
}

/// Rebuild a model and head from a reloaded bundle, and from nothing else.
fn rebuild_from(
    bundle: &SetFitBundle,
) -> Result<(SetFitMiniLm, MultinomialLogisticRegression), SetFitTrainError> {
    let tokenizer_bytes = bundle.tokenizer_bytes().map_err(SetFitTrainError::Bundle)?;
    let tensors: BTreeMap<String, (Vec<usize>, Vec<f32>)> =
        bundle.named_tensors().map_err(SetFitTrainError::Bundle)?;
    let encoder = SetFitMiniLm::from_bundle_parts(
        &tokenizer_bytes,
        bundle.architecture(),
        &tensors,
        bundle.root_seed(),
    )
    .map_err(|e| SetFitTrainError::Encoder { reason: e.to_string() })?;
    let (labels, n_features, weights, intercepts) =
        bundle.head_parts().map_err(SetFitTrainError::Bundle)?;
    let head = MultinomialLogisticRegression::from_stored_coefficients(
        labels, n_features, weights, intercepts,
    )
    .map_err(SetFitTrainError::HeadFit)?;
    Ok((encoder, head))
}

/// Everything the trusted policy produced. Every field traces to the reloaded bytes.
pub(crate) struct VerifiedOutcome {
    /// The encoder REBUILT from the artifact.
    pub(crate) encoder: SetFitMiniLm,
    /// The head REBUILT from the artifact.
    pub(crate) head: MultinomialLogisticRegression,
    /// The measured outcome.
    pub(crate) report: VerifyReport,
    /// What the REBUILT model answered.
    pub(crate) probe: VerifyProbe,
    /// SHA-256 of the artifact's bytes.
    pub(crate) artifact_hash: [u8; 32],
}

/// The whole trusted sequence, driven from [`super::SetFitRun::verify_artifact`].
///
/// The caller has no binding to the pre-close model at any point after [`close`]
/// returns, because [`close`] consumed it.
pub(crate) fn run_verify_policy<C: SetFitCodec>(
    codec: &C,
    encoder: SetFitMiniLm,
    head: MultinomialLogisticRegression,
    ordered_labels: &[String],
    dataset: &PreparedDataset<Canonical>,
    selection: &Selection,
    config: &ResolvedSetFitConfig,
    summary: &EvidenceSummary,
    tolerance: Tolerance,
) -> Result<VerifiedOutcome, SetFitTrainError> {
    // (1) Probe, serialize, hash, and DROP the live model. `close` consumes it.
    let ClosedArtifact { bytes, hash, probe } =
        close(codec, encoder, head, ordered_labels, dataset, selection, config, summary)?;

    // (2) Reload FROM BYTES.
    let reloaded = codec.deserialize(&bytes).map_err(SetFitTrainError::Codec)?;

    // (3) Rebuild from the reloaded bundle and from nothing else.
    let (mut rebuilt_encoder, rebuilt_head) = rebuild_from(&reloaded)?;

    // (4) Re-encode and re-predict FROM THE REBUILT MODEL.
    let after = probe_model(&mut rebuilt_encoder, &rebuilt_head, dataset, selection, config)?;

    // (5) Compare at the trusted tolerance.
    let (max_embedding_abs_diff, max_probability_abs_diff) =
        compare_probes(&probe, &after, tolerance)?;

    let report = VerifyReport {
        artifact_bytes: bytes.len(),
        probe_rows: probe.ids.len(),
        embedding_dim: probe.embeddings.first().map_or(0, Vec::len),
        class_count: probe.probabilities.first().map_or(0, Vec::len),
        max_embedding_abs_diff,
        max_probability_abs_diff,
        tolerance_embedding_abs: tolerance.embedding_abs,
        tolerance_probability_abs: tolerance.probability_abs,
        round_trip_closed: false,
    };
    Ok(VerifiedOutcome {
        encoder: rebuilt_encoder,
        head: rebuilt_head,
        report,
        probe: after,
        artifact_hash: hash,
    })
}

/// Assemble the final state's evidence.
pub(crate) fn verified_evidence(
    parts: super::HeadFittedParts,
    artifact_hash: [u8; 32],
    format_id: &str,
    report: VerifyReport,
    probe: VerifyProbe,
    head: MultinomialLogisticRegression,
) -> ArtifactVerifiedEvidence {
    ArtifactVerifiedEvidence {
        passed: parts.passed,
        head,
        report: parts.report,
        effective_lambda: parts.effective_lambda,
        ordered_labels: parts.ordered_labels,
        encode_ledger: parts.encode_ledger,
        encode_call_count: parts.encode_call_count,
        artifact_hash,
        format_id: format_id.to_string(),
        verify: report,
        probe,
    }
}

#[cfg(test)]
#[path = "verify_tests.rs"]
mod verify_tests;
