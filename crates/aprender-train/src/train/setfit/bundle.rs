//! The complete deterministic state of a finished SetFit run, and its canonical
//! wire form (plan 03-08, TRN-01, D-07).
//!
//! # Completeness is the point
//!
//! A bundle that lists tensors, a head and a label map is enough to *describe* a
//! model and not enough to *rebuild* one. This one carries, in a fixed field
//! order: the exact tokenizer bytes and the digest that identifies them, the
//! encoder architecture record including the vocabulary remap, the
//! pooling/normalization/truncation policy and the run's max length, the root
//! seed, every named encoder tensor, the head's coefficients and ordered labels,
//! both the requested and the resolved configuration, and the evidence summary
//! with its table hash. `bundle_rebuilds_a_bit_identical_encoder` is what makes
//! that list normative rather than aspirational: it rebuilds a model from these
//! bytes alone and asserts the embeddings are bit-for-bit the originals.
//!
//! # The wire form is the storage form
//!
//! There is no separate in-memory representation, so there is no place for the
//! two to disagree. Tensor data and tokenizer bytes are carried as lowercase hex
//! of their little-endian bytes, which has three consequences worth stating:
//!
//! 1. Round-tripping an `f32` is EXACT for every value including subnormals,
//!    negative zero and quiet NaN, because the bit pattern is what travels. The
//!    decimal alternative round-trips finite values exactly through ryu but turns
//!    every non-finite value into `null`, which then fails to parse back.
//! 2. The element count of a tensor is `hex_len / 8` and is therefore known
//!    BEFORE any `Vec<f32>` is allocated. That is what lets the four contracted
//!    limits below be enforced ahead of allocation rather than after it.
//! 3. It doubles the size of the tensor payload, on top of JSON's own overhead.
//!    That amplification is measured in `bundle_size_is_recorded` and is exactly
//!    why phase 4 replaces the FORMAT behind the codec seam while keeping this
//!    boundary.
//!
//! A third-party base64 encoder would be denser than hex; it is deliberately not
//! introduced, because adding a package to shrink an interim format is a
//! dependency taken for a format that is scheduled to be replaced.
//!
//! # This format never claims to be the shipped container format
//!
//! The format identifier written here is the serde one. Naming it after the
//! project's model container would make a phase-4 reader believe an interim
//! debug format is the real thing.

use std::collections::BTreeMap;

use aprender::classification::MultinomialLogisticRegression;
use aprender::setfit::{
    EncoderArchitecture, SetFitMiniLm, L2_EPS, MAX_SEQUENCE_LENGTH, NORMALIZATION_POLICY,
    PADDING_MODE, POOLING_POLICY,
};
use serde::{Deserialize, Serialize};

use super::config::{ResolvedSetFitConfig, SetFitTrainConfig};
use super::evidence::EvidenceSummary;

// ===========================================================================================
// Schema and limits
// ===========================================================================================

/// Wire schema version of [`SetFitBundle`].
///
/// Bumped whenever a field is added, removed or re-meant. A reader that meets a
/// version it does not know refuses rather than guessing, following the ledger
/// precedent in `aprender-contrastive-data`.
pub const BUNDLE_SCHEMA_VERSION: u32 = 1;

/// Maximum accepted length of a serialized bundle, in bytes (512 MiB).
///
/// The full pinned MiniLM-L6-v2 projects to roughly 182 MB in this format (see
/// `bundle_limits_clear_the_full_minilm_figures`, which computes the projection
/// rather than quoting it), so this clears the largest artifact the phase can
/// legitimately produce by about 2.9x.
pub const MAX_BUNDLE_BYTES: u64 = 536_870_912;

/// Maximum accepted number of named tensors.
///
/// The full pin carries 101. 4096 is roughly 40x that, which leaves room for a
/// much deeper encoder while still refusing a payload that claims millions of
/// tensors in order to make the map allocation itself the attack.
pub const MAX_TENSOR_COUNT: u64 = 4_096;

/// Maximum accepted elements in any single tensor (2^27).
///
/// The full pin's largest tensor is the 30522x384 word-embedding table at
/// 11,720,448 elements; this clears it by about 11.5x.
pub const MAX_ELEMENTS_PER_TENSOR: u64 = 134_217_728;

/// Maximum accepted elements summed over every tensor (2^28).
///
/// The full pin totals 22,565,376 elements; this clears it by about 11.9x.
///
/// The total is a SEPARATE limit rather than an implication of the two above: the
/// product of the tensor-count limit and the per-tensor limit is around 5.5e11
/// elements, which is not a bound anyone would call one.
pub const MAX_TOTAL_ELEMENTS: u64 = 268_435_456;

/// The four deserialization bounds, as one value.
///
/// # Why the bounds are a parameter at all
///
/// Every one of the four has to be shown BITING, and three of them bite only on
/// payloads far larger than any fixture. Proving the input-length bound by
/// materializing half a gigabyte of bytes in a unit test would make the suite
/// pay 512 MB to learn that a comparison compares. So the bound VALUES and the
/// bound MECHANISM are falsified separately: the mechanism against a deliberately
/// tiny set of bounds on a real bundle, the values against the contract they are
/// frozen in.
///
/// The fields are private and the only value production can name is
/// [`Self::CONTRACTED`]; the shrinking constructor is `#[cfg(test)]`. This follows
/// `TuningProbes` in `tune.rs` for the same reason it does: a knob that could
/// weaken a bound in a shipped build would be worse than the attack it tests for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleLimits {
    max_bundle_bytes: u64,
    max_tensor_count: u64,
    max_elements_per_tensor: u64,
    max_total_elements: u64,
}

impl BundleLimits {
    /// The contracted bounds — the only value a shipped build can construct.
    pub const CONTRACTED: Self = Self {
        max_bundle_bytes: MAX_BUNDLE_BYTES,
        max_tensor_count: MAX_TENSOR_COUNT,
        max_elements_per_tensor: MAX_ELEMENTS_PER_TENSOR,
        max_total_elements: MAX_TOTAL_ELEMENTS,
    };

    /// Deliberately tiny bounds, so each one can be shown biting on a real bundle.
    #[cfg(test)]
    pub(crate) const fn tiny(
        max_bundle_bytes: u64,
        max_tensor_count: u64,
        max_elements_per_tensor: u64,
        max_total_elements: u64,
    ) -> Self {
        Self { max_bundle_bytes, max_tensor_count, max_elements_per_tensor, max_total_elements }
    }
}

// ===========================================================================================
// Errors
// ===========================================================================================

/// Failure modes of bundle serialization and parsing.
///
/// `#[non_exhaustive]`: phase 4's format lands behind the same codec seam and may
/// need variants this one does not.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum BundleError {
    /// The bundle could not be serialized or the bytes could not be parsed.
    Serialization {
        /// What was being (de)serialized.
        context: String,
        /// The underlying diagnostic, including the parse position when serde has one.
        detail: String,
    },
    /// The payload declares a schema version this build does not implement.
    UnsupportedSchemaVersion {
        /// The version found.
        got: u32,
        /// The version this build writes and reads.
        supported: u32,
    },
    /// The payload declares a format identifier the reading codec does not own.
    UnsupportedFormatId {
        /// The identifier found.
        got: String,
        /// The identifier the codec expects.
        expected: String,
    },
    /// A contracted deserialization limit was exceeded, BEFORE the allocation it bounds.
    BundleLimitExceeded {
        /// Which limit: `input_bytes`, `tensor_count`, `tensor_elements` or `total_elements`.
        what: &'static str,
        /// The contracted limit.
        limit: u64,
        /// The value observed in the payload.
        observed: u64,
    },
    /// A hex-encoded payload was not decodable, or did not describe whole `f32`s.
    MalformedHexPayload {
        /// Which field: a tensor name, `tokenizer_bytes`, `head_weights`, `head_intercepts`.
        field: String,
        /// Why it is unusable.
        reason: String,
    },
    /// A tensor's declared shape disagrees with the element count its data carries.
    TensorShapeMismatch {
        /// The tensor's name.
        tensor: String,
        /// Elements implied by the declared shape.
        expected: u64,
        /// Elements the data actually carries.
        observed: u64,
    },
}

impl core::fmt::Display for BundleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Serialization { context, detail } => {
                write!(f, "setfit bundle {context} failed: {detail}")
            }
            Self::UnsupportedSchemaVersion { got, supported } => write!(
                f,
                "setfit bundle declares schema version {got}, but this build implements \
                 {supported}; a bundle from a different schema is refused rather than \
                 partially interpreted",
            ),
            Self::UnsupportedFormatId { got, expected } => write!(
                f,
                "setfit bundle declares format `{got}`, but the codec reading it owns \
                 `{expected}`",
            ),
            Self::BundleLimitExceeded { what, limit, observed } => write!(
                f,
                "setfit bundle exceeds the contracted {what} limit: observed {observed}, \
                 limit {limit}; the payload is refused before the allocation it would have \
                 requested (contract setfit-train-lifecycle-v1, equation bundle_limits)",
            ),
            Self::MalformedHexPayload { field, reason } => {
                write!(f, "setfit bundle field `{field}` is unusable: {reason}")
            }
            Self::TensorShapeMismatch { tensor, expected, observed } => write!(
                f,
                "setfit bundle tensor `{tensor}` declares a shape implying {expected} elements \
                 but carries {observed}",
            ),
        }
    }
}

impl std::error::Error for BundleError {}

// ===========================================================================================
// Wire members
// ===========================================================================================

/// One named encoder tensor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleTensor {
    /// Row-major shape.
    pub(crate) shape: Vec<usize>,
    /// Lowercase hex of the little-endian `f32` bit patterns, in row-major order.
    pub(crate) data_hex: String,
}

/// The RESOLVED configuration as a PROVENANCE RECORD.
///
/// It is written on serialize and read back for inspection and comparison, and
/// there is deliberately no conversion from this record back into a runtime
/// resolved configuration, and no other path that reconstructs one from bytes.
/// (The forbidden conversion is not spelled out here on purpose: the guard
/// against it is a literal scan, and a doc comment that names the thing it
/// forbids is indistinguishable from the thing.)
///
/// # Why this type exists at all
///
/// Plan 03-03 made [`ResolvedSetFitConfig`] `Serialize`-but-NOT-`Deserialize` on
/// purpose (T-3-50): a resolved device is a fact about the host that probed it,
/// and a probed device arriving from a file is a claim wearing a measurement's
/// clothes. Embedding that type here would make this struct underivable, and the
/// cheapest way out of the resulting compile error would have been to derive
/// `Deserialize` on it — silently retiring an acceptance criterion 03-03 already
/// passed. A separate record is the fix: the fact travels, the capability does
/// not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedConfigRecord {
    /// The device tag the run resolved to when it executed.
    pub(crate) resolved_device: String,
}

impl From<&ResolvedSetFitConfig> for ResolvedConfigRecord {
    fn from(value: &ResolvedSetFitConfig) -> Self {
        Self { resolved_device: value.device().tag() }
    }
}

// ===========================================================================================
// The bundle
// ===========================================================================================

/// Everything a finished run needs to be rebuilt from bytes alone.
///
/// The field order below IS the wire order, and it is the normative completeness
/// list the `bundle_completeness` equation restates. A field missing from it is a
/// finding, not a simplification.
///
/// Fields are `pub(crate)` with read accessors: nothing outside this crate builds
/// a bundle, and the codec trait that transports one is sealed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetFitBundle {
    /// Wire schema version.
    pub(crate) schema_version: u32,
    /// The identifier of the codec that wrote this payload.
    pub(crate) format_id: String,
    /// The encoder architecture record, including the vocabulary remap.
    pub(crate) architecture: EncoderArchitecture,
    /// The exact `tokenizer.json` bytes, lowercase hex.
    pub(crate) tokenizer_bytes_hex: String,
    /// The pooling policy identifier the encoder applied.
    pub(crate) pooling: String,
    /// The normalization policy identifier the encoder applied.
    pub(crate) normalization: String,
    /// The epsilon the L2 normalization clamped with.
    pub(crate) l2_epsilon: f32,
    /// The tokenizer's truncation bound.
    pub(crate) truncation_max_sequence_length: u32,
    /// The tokenizer's padding mode.
    pub(crate) padding_mode: String,
    /// The run's requested max sequence length.
    pub(crate) max_length: u32,
    /// The root seed every dropout stream derives from.
    pub(crate) root_seed: u64,
    /// Every named encoder tensor, in name order.
    pub(crate) tensors: BTreeMap<String, BundleTensor>,
    /// The head's `K * d` weights, row-major, lowercase hex.
    pub(crate) head_weights_hex: String,
    /// The head's `K` intercepts, lowercase hex.
    pub(crate) head_intercepts_hex: String,
    /// The head's fitted feature dimension.
    pub(crate) head_n_features: usize,
    /// The ordered labels the head's weight rows are indexed by.
    pub(crate) ordered_labels: Vec<String>,
    /// The configuration that was REQUESTED.
    pub(crate) requested_config: SetFitTrainConfig,
    /// The configuration that was RESOLVED, as provenance.
    pub(crate) resolved_config: ResolvedConfigRecord,
    /// The bound evidence summary, carrying its own table hash.
    pub(crate) evidence: EvidenceSummary,
}

impl SetFitBundle {
    /// Assemble a bundle from a live run's parts.
    ///
    /// `pub(crate)`: the only caller is the verify transition, which owns the
    /// order in which the parts are read and then dropped.
    ///
    /// Every architectural and policy value is READ OFF the encoder that is about
    /// to be serialized, never restated from the configuration that asked for it.
    ///
    /// # Errors
    ///
    /// [`BundleError::Serialization`] if the head reports no fitted feature
    /// dimension — a head that never fitted has no coefficients to store.
    pub(crate) fn from_run_parts(
        format_id: &str,
        encoder: &SetFitMiniLm,
        head: &MultinomialLogisticRegression,
        ordered_labels: &[String],
        config: &ResolvedSetFitConfig,
        evidence: &EvidenceSummary,
    ) -> Result<Self, BundleError> {
        let head_n_features = head.n_features().ok_or_else(|| BundleError::Serialization {
            context: "assembly".to_string(),
            detail: "the head reports no fitted feature dimension".to_string(),
        })?;

        let mut tensors = BTreeMap::new();
        for (name, tensor) in encoder.named_parameters() {
            tensors.insert(
                name,
                BundleTensor {
                    shape: tensor.shape().to_vec(),
                    data_hex: f32_to_hex(tensor.data()),
                },
            );
        }

        Ok(Self {
            schema_version: BUNDLE_SCHEMA_VERSION,
            format_id: format_id.to_string(),
            architecture: encoder.architecture(),
            tokenizer_bytes_hex: hex::encode(encoder.tokenizer_bytes()),
            pooling: POOLING_POLICY.to_string(),
            normalization: NORMALIZATION_POLICY.to_string(),
            l2_epsilon: L2_EPS,
            truncation_max_sequence_length: MAX_SEQUENCE_LENGTH as u32,
            padding_mode: PADDING_MODE.to_string(),
            max_length: config.requested().max_length(),
            root_seed: encoder.root_seed(),
            tensors,
            head_weights_hex: f32_to_hex(head.weights()),
            head_intercepts_hex: f32_to_hex(head.intercepts()),
            head_n_features,
            ordered_labels: ordered_labels.to_vec(),
            requested_config: config.requested().clone(),
            resolved_config: ResolvedConfigRecord::from(config),
            evidence: evidence.clone(),
        })
    }

    /// The canonical bytes.
    ///
    /// Compact JSON over a struct with a fixed field order and a `BTreeMap` of
    /// tensors, with every float carried as its bit pattern. There is no map with
    /// unspecified iteration order, no timestamp and no floating-point formatting
    /// in the payload, so two runs that produced the same state produce the same
    /// bytes — and re-serializing a parsed bundle reproduces its input exactly,
    /// which is the property the verify policy's round-trip closure check rests on.
    ///
    /// # Errors
    ///
    /// [`BundleError::Serialization`].
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, BundleError> {
        serde_json::to_vec(self).map_err(|e| BundleError::Serialization {
            context: "serialization".to_string(),
            detail: e.to_string(),
        })
    }

    /// Parse canonical bytes, refusing an oversized or unknown payload first.
    ///
    /// # Order is load-bearing
    ///
    /// 1. The input-length limit, on the raw slice, before serde is handed anything.
    /// 2. The parse, whose only unbounded allocations are strings shorter than
    ///    the input that step 1 already bounded.
    /// 3. The schema version.
    /// 4. The tensor-count limit, then the per-tensor and cumulative element
    ///    limits — all computed from hex string LENGTHS, so no `Vec<f32>` has
    ///    been allocated when they are enforced.
    ///
    /// Decoding to `f32` happens only in the accessors, after every limit held.
    ///
    /// # Errors
    ///
    /// [`BundleError::BundleLimitExceeded`] naming the limit,
    /// [`BundleError::Serialization`] naming the parse failure and its position,
    /// or [`BundleError::UnsupportedSchemaVersion`].
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, BundleError> {
        Self::from_canonical_bytes_within(bytes, &BundleLimits::CONTRACTED)
    }

    /// [`Self::from_canonical_bytes`] at a caller-chosen set of bounds.
    ///
    /// Module-private, and it stays that way: the shipped door above is the only
    /// one that names a bound, and the only bound it names is the contracted one.
    ///
    /// # Errors
    ///
    /// As [`Self::from_canonical_bytes`].
    fn from_canonical_bytes_within(
        bytes: &[u8],
        limits: &BundleLimits,
    ) -> Result<Self, BundleError> {
        let observed = bytes.len() as u64;
        if observed > limits.max_bundle_bytes {
            return Err(BundleError::BundleLimitExceeded {
                what: "input_bytes",
                limit: limits.max_bundle_bytes,
                observed,
            });
        }

        let bundle: Self = serde_json::from_slice(bytes).map_err(|e| {
            BundleError::Serialization { context: "parse".to_string(), detail: e.to_string() }
        })?;

        if bundle.schema_version != BUNDLE_SCHEMA_VERSION {
            return Err(BundleError::UnsupportedSchemaVersion {
                got: bundle.schema_version,
                supported: BUNDLE_SCHEMA_VERSION,
            });
        }

        let tensor_count = bundle.tensors.len() as u64;
        if tensor_count > limits.max_tensor_count {
            return Err(BundleError::BundleLimitExceeded {
                what: "tensor_count",
                limit: limits.max_tensor_count,
                observed: tensor_count,
            });
        }

        let mut total: u64 = 0;
        for tensor in bundle.tensors.values() {
            // Two hex characters per byte, four bytes per f32. The division is on
            // the STRING LENGTH, so this is known without decoding anything.
            let elements = (tensor.data_hex.len() / 8) as u64;
            if elements > limits.max_elements_per_tensor {
                return Err(BundleError::BundleLimitExceeded {
                    what: "tensor_elements",
                    limit: limits.max_elements_per_tensor,
                    observed: elements,
                });
            }
            total = total.saturating_add(elements);
            if total > limits.max_total_elements {
                return Err(BundleError::BundleLimitExceeded {
                    what: "total_elements",
                    limit: limits.max_total_elements,
                    observed: total,
                });
            }
        }

        Ok(bundle)
    }

    // -----------------------------------------------------------------------
    // Read accessors
    // -----------------------------------------------------------------------

    /// The wire schema version this payload declares.
    #[must_use]
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// The identifier of the codec that wrote this payload.
    #[must_use]
    pub fn format_id(&self) -> &str {
        &self.format_id
    }

    /// The encoder architecture record.
    #[must_use]
    pub fn architecture(&self) -> &EncoderArchitecture {
        &self.architecture
    }

    /// The root seed the encoder was built with.
    #[must_use]
    pub fn root_seed(&self) -> u64 {
        self.root_seed
    }

    /// The ordered labels the head's weight rows are indexed by.
    #[must_use]
    pub fn ordered_labels(&self) -> &[String] {
        &self.ordered_labels
    }

    /// The bound evidence summary.
    #[must_use]
    pub fn evidence(&self) -> &EvidenceSummary {
        &self.evidence
    }

    /// The exact tokenizer bytes.
    ///
    /// # Errors
    ///
    /// [`BundleError::MalformedHexPayload`] if the field is not valid hex.
    pub fn tokenizer_bytes(&self) -> Result<Vec<u8>, BundleError> {
        hex::decode(&self.tokenizer_bytes_hex).map_err(|e| BundleError::MalformedHexPayload {
            field: "tokenizer_bytes".to_string(),
            reason: e.to_string(),
        })
    }

    /// Every named encoder tensor, decoded.
    ///
    /// # Errors
    ///
    /// [`BundleError::MalformedHexPayload`] naming the tensor whose data is not
    /// valid hex or does not describe whole `f32`s, or
    /// [`BundleError::TensorShapeMismatch`] when the declared shape and the data
    /// length disagree.
    pub fn named_tensors(&self) -> Result<BTreeMap<String, (Vec<usize>, Vec<f32>)>, BundleError> {
        let mut out = BTreeMap::new();
        for (name, tensor) in &self.tensors {
            let data = hex_to_f32(name, &tensor.data_hex)?;
            let expected: usize = tensor.shape.iter().product();
            if data.len() != expected {
                return Err(BundleError::TensorShapeMismatch {
                    tensor: name.clone(),
                    expected: expected as u64,
                    observed: data.len() as u64,
                });
            }
            out.insert(name.clone(), (tensor.shape.clone(), data));
        }
        Ok(out)
    }

    /// The head's stored coefficients: `(ordered_labels, n_features, weights, intercepts)`.
    ///
    /// # Errors
    ///
    /// [`BundleError::MalformedHexPayload`] naming the offending array.
    pub fn head_parts(&self) -> Result<(Vec<String>, usize, Vec<f32>, Vec<f32>), BundleError> {
        let weights = hex_to_f32("head_weights", &self.head_weights_hex)?;
        let intercepts = hex_to_f32("head_intercepts", &self.head_intercepts_hex)?;
        Ok((self.ordered_labels.clone(), self.head_n_features, weights, intercepts))
    }
}

// ===========================================================================================
// Hex codec for f32 payloads
// ===========================================================================================

/// Lowercase hex of the little-endian bit patterns, in slice order.
///
/// Encodes STRAIGHT INTO the output string rather than materializing a `4N`-byte scratch
/// buffer to hand `hex::encode` a contiguous slice. The scratch was pure waste and it was
/// not small: this runs once per tensor, and the word-embedding table alone is 11.7M
/// elements, so the intermediate was a 47 MB allocation on its own (~90 MB across a full
/// pin) — paid twice per verify, since the policy serializes both the live bundle and the
/// reloaded one.
fn f32_to_hex(values: &[f32]) -> String {
    let mut out = String::with_capacity(values.len() * 8);
    let mut word = [0u8; 8];
    for value in values {
        // Both `expect`s are structural, not hopeful: `encode_to_slice` fails only when the
        // destination is not exactly twice the source length (4 into `[u8; 8]` is, by
        // construction), and it writes only lowercase ASCII. They are `expect` rather than a
        // silent fallback ON PURPOSE — a skipped value would shorten the payload, and a
        // codec whose contract is byte-exactness must not have a path that quietly truncates.
        hex::encode_to_slice(value.to_bits().to_le_bytes(), &mut word)
            .expect("4 source bytes into an 8-byte destination is exactly 2x");
        out.push_str(
            std::str::from_utf8(&word).expect("hex::encode_to_slice writes ASCII hex only"),
        );
    }
    out
}

/// The inverse of [`f32_to_hex`], naming the field on failure.
fn hex_to_f32(field: &str, encoded: &str) -> Result<Vec<f32>, BundleError> {
    if encoded.len() % 8 != 0 {
        return Err(BundleError::MalformedHexPayload {
            field: field.to_string(),
            reason: format!(
                "{} hex characters is not a whole number of 4-byte f32 values",
                encoded.len()
            ),
        });
    }
    // Decoded PER VALUE rather than through one `hex::decode` of the whole payload. The
    // whole-payload form allocated a `4N`-byte scratch `Vec<u8>` that existed only to be
    // walked once into the `Vec<f32>` — ~90 MB of it on a full pin, on the reload path,
    // where it stacked with the hex `String` itself and the decoded tensors.
    // The length check above makes `len / 8` the exact element count, so this reserves once.
    let mut out = Vec::with_capacity(encoded.len() / 8);
    let mut word = [0u8; 4];
    for chunk in encoded.as_bytes().chunks_exact(8) {
        hex::decode_to_slice(chunk, &mut word).map_err(|e| BundleError::MalformedHexPayload {
            field: field.to_string(),
            reason: e.to_string(),
        })?;
        out.push(f32::from_le_bytes(word));
    }
    Ok(out)
}

#[cfg(test)]
#[path = "bundle_tests.rs"]
mod bundle_tests;
