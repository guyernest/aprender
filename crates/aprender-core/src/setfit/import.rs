//! The pinned-revision import contract (ENC-01).
//!
//! Two constructors, deliberately separate (RESEARCH Pitfall 3):
//!
//! * [`MiniLmImport::open`] — the **full pin**. Every behavior-affecting field
//!   of `config.json` must equal the pinned all-MiniLM-L6-v2 architecture, the
//!   sentence-transformers module stack must be mean-pool + L2-normalize, and
//!   the tokenizer bytes must hash to the pinned digest.
//! * [`MiniLmImport::open_slice_fixture`] — **fixtures only**, behind
//!   `conformance-fixtures`. Bypasses ONLY the equality-with-the-pin checks.
//!   Every structural, shape-consistency and finiteness check still runs.
//!
//! Keeping them separate is what stops the fixture gates and the pin gates from
//! contradicting each other. The tempting "fix" — parameterising the pin path by
//! caller-supplied dimensions — is PF-011's exact failure mode: it turns the pin
//! into a formality that agrees with whatever it is handed.
//!
//! # Unknown metadata is tolerated, on purpose
//!
//! `serde(deny_unknown_fields)` is **not** used. The real pinned `config.json`
//! carries `_name_or_path`, `gradient_checkpointing`, `initializer_range`,
//! `model_type`, `transformers_version` and `use_cache`, none of which this
//! struct models. Denying unknown fields would reject the pinned model itself
//! and turn ENC-01 into a gate that fails on the correct artifact. The security
//! property comes from validating every field that can CHANGE BEHAVIOR, not from
//! refusing to parse metadata.
//!
//! # Sealing (D-08)
//!
//! Both constructors are `pub(crate)`. Nothing outside the crate builds a
//! `MiniLmImport`; `SetFitMiniLm::from_pretrained_dir` / `from_slice_fixture`
//! (01-07) are the public entry points and they load the tokenizer from the same
//! source, so a mismatched tokenizer/encoder pair cannot be assembled.

use std::collections::HashMap;
use std::path::Path;

use crate::autograd::Tensor;
use crate::format::v2::AprV2Reader;
use crate::models::bert::config::BertConfig;
use crate::models::bert::load::{detect_bert_prefix, read_tensor};

use super::error::SetFitError;
use super::tokenizer::sha256_hex;

/// The pinned upstream revision of `sentence-transformers/all-MiniLM-L6-v2`.
///
/// Recorded as **data**, never fetched by branch name (T-1-10): a mutable ref
/// can be repointed, an immutable commit sha cannot. This value is asserted
/// against `tests/fixtures/setfit/upstream_manifest.json` by
/// `import_pin_revision_agrees_with_the_frozen_upstream_manifest`, so it cannot
/// drift from the artifact set 01-04 froze.
pub const PINNED_REVISION: &str = "1110a243fdf4706b3f48f1d95db1a4f5529b4d41";

/// Sha256 of `tokenizer.json` at [`PINNED_REVISION`].
///
/// Also asserted against `upstream_manifest.json` rather than transcribed on
/// trust.
pub const PINNED_TOKENIZER_SHA256: &str =
    "be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037";

/// The pinned sentence-transformers maximum sequence length.
pub const PINNED_MAX_SEQ_LENGTH: usize = 256;

/// The only activation this crate implements for the pinned model.
///
/// `"gelu"` is the exact **erf** form (`Tensor::gelu_exact`, 01-09).
/// `"gelu_new"` / `"gelu_pytorch_tanh"` select the tanh approximation, which
/// differs from it by a measured 4.734993e-04 — two orders above the frozen
/// activation tolerance of 4.47e-06 — so they are rejected, never coerced.
pub const PINNED_ACTIVATION: &str = "gelu";

// ---------------------------------------------------------------------------
// Vocabulary remap (slice fixtures only)
// ---------------------------------------------------------------------------

/// Canonical vocabulary id <-> slice embedding row.
///
/// Deserialized from `vocab_remap.json` (01-04). The encoder applies it to
/// canonical ids at gather time; a [`super::SentenceBatch`] is never rewritten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VocabRemap {
    orig_to_slice: HashMap<u32, u32>,
    slice_to_orig: Vec<u32>,
}

impl VocabRemap {
    /// Parse and fully validate a remap against the slice vocabulary size.
    ///
    /// # Errors
    ///
    /// [`SetFitError::RemapInvalid`] if the two directions disagree, if any
    /// slice row is out of range, or if the arity does not match `slice_vocab`.
    #[cfg(feature = "conformance-fixtures")]
    pub(crate) fn from_json_bytes(bytes: &[u8], slice_vocab: usize) -> Result<Self, SetFitError> {
        let _ = (bytes, slice_vocab);
        Err(SetFitError::RemapInvalid {
            reason: "VocabRemap::from_json_bytes is not implemented".to_string(),
        })
    }

    /// Map a canonical vocabulary id to its slice embedding row.
    ///
    /// # Errors
    ///
    /// [`SetFitError::VocabOutOfSlice`] when the id is outside the slice
    /// closure. Returned rather than zero-filled: a zero row is
    /// indistinguishable from a legitimately zero embedding downstream.
    pub fn to_slice_row(&self, canonical: u32) -> Result<u32, SetFitError> {
        self.orig_to_slice
            .get(&canonical)
            .copied()
            .ok_or(SetFitError::VocabOutOfSlice {
                canonical_id: canonical,
            })
    }

    /// Number of rows in the slice embedding table.
    #[must_use]
    pub fn slice_vocab(&self) -> usize {
        self.slice_to_orig.len()
    }

    /// Canonical id for a slice row, if the row exists.
    #[must_use]
    pub fn to_canonical(&self, slice_row: u32) -> Option<u32> {
        self.slice_to_orig.get(slice_row as usize).copied()
    }
}

/// Wire form of `vocab_remap.json`.
#[derive(serde::Deserialize)]
struct VocabRemapWire {
    orig_to_slice: HashMap<u32, u32>,
    slice_to_orig: Vec<u32>,
}

// ---------------------------------------------------------------------------
// Model dimensions and slice configuration
// ---------------------------------------------------------------------------

/// The dimensions an import actually loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDims {
    /// Hidden width.
    pub hidden: usize,
    /// Encoder layer count.
    pub layers: usize,
    /// Attention head count.
    pub heads: usize,
    /// FFN intermediate width.
    pub intermediate: usize,
    /// Embedding table rows.
    pub vocab: usize,
    /// Position table rows.
    pub max_positions: usize,
    /// Token-type table rows.
    pub type_vocab: usize,
    /// Padding token id.
    pub pad_token_id: u32,
}

/// Wire form of `slice_config.json` (01-04).
///
/// The `source_*` fields the generator also writes are intentionally unmodelled
/// — same reason `deny_unknown_fields` is absent from the pin path.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SliceConfig {
    /// Hidden width of the slice.
    pub hidden: usize,
    /// Attention heads retained.
    pub heads: usize,
    /// Per-head dimension (unchanged from the source model).
    pub head_dim: usize,
    /// Encoder layers retained.
    pub num_layers: usize,
    /// FFN intermediate width.
    pub intermediate: usize,
    /// Rows of the remapped embedding table.
    pub vocab: usize,
    /// Position table rows.
    pub positions: usize,
    /// Token-type table rows.
    pub type_vocab_size: usize,
    /// LayerNorm epsilon.
    pub layer_norm_eps: f64,
    /// Padding token id.
    pub pad_token_id: u32,
    /// Activation; must still be the pinned exact-erf `"gelu"`.
    pub hidden_act: String,
    /// Upstream revision the slice was cut from.
    pub source_revision: String,
    /// Sha256 of the tokenizer the slice was cut against.
    pub tokenizer_sha256: String,
}

impl SliceConfig {
    /// Parse `slice_config.json`.
    ///
    /// # Errors
    ///
    /// [`SetFitError::ImportIo`] if the bytes are not parseable.
    #[cfg(feature = "conformance-fixtures")]
    pub(crate) fn from_json_bytes(bytes: &[u8]) -> Result<Self, SetFitError> {
        serde_json::from_slice(bytes).map_err(|e| SetFitError::ImportIo {
            path: "slice_config.json".to_string(),
            reason: e.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// HuggingFace config wire forms
// ---------------------------------------------------------------------------

/// The BEHAVIOR-AFFECTING subset of `config.json`.
///
/// No `deny_unknown_fields` — see the module docs.
#[derive(Debug, Clone, serde::Deserialize)]
struct HfBertConfig {
    architectures: Vec<String>,
    attention_probs_dropout_prob: f64,
    hidden_act: String,
    hidden_dropout_prob: f64,
    hidden_size: usize,
    intermediate_size: usize,
    layer_norm_eps: f64,
    max_position_embeddings: usize,
    model_type: String,
    num_attention_heads: usize,
    num_hidden_layers: usize,
    pad_token_id: u32,
    position_embedding_type: String,
    type_vocab_size: usize,
    vocab_size: usize,
}

/// One entry of `modules.json`.
#[derive(Debug, Clone, serde::Deserialize)]
struct SentenceTransformerModule {
    #[allow(dead_code)]
    idx: usize,
    #[allow(dead_code)]
    path: String,
    #[serde(rename = "type")]
    kind: String,
}

/// `1_Pooling/config.json`.
#[derive(Debug, Clone, serde::Deserialize)]
struct PoolingConfig {
    word_embedding_dimension: usize,
    pooling_mode_cls_token: bool,
    pooling_mode_mean_tokens: bool,
    pooling_mode_max_tokens: bool,
    pooling_mode_mean_sqrt_len_tokens: bool,
}

/// `sentence_bert_config.json`, when the checkout carries one.
#[derive(Debug, Clone, serde::Deserialize)]
struct SentenceBertConfig {
    max_seq_length: usize,
}

// ---------------------------------------------------------------------------
// The import
// ---------------------------------------------------------------------------

/// A validated MiniLM checkpoint: dimensions, weights reader, and provenance.
pub struct MiniLmImport {
    dims: ModelDims,
    layer_norm_eps: f32,
    reader: AprV2Reader,
    tensor_prefix: &'static str,
    revision: String,
    tokenizer_sha256: String,
    vocab_remap: Option<VocabRemap>,
}

impl std::fmt::Debug for MiniLmImport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MiniLmImport")
            .field("dims", &self.dims)
            .field("revision", &self.revision)
            .field("tokenizer_sha256", &self.tokenizer_sha256)
            .field("is_slice", &self.vocab_remap.is_some())
            .finish()
    }
}

impl MiniLmImport {
    /// Open a pinned all-MiniLM-L6-v2 checkout.
    ///
    /// SEALED (D-08): `pub(crate)`. The public full-pin entry point is
    /// `SetFitMiniLm::from_pretrained_dir` (01-07).
    ///
    /// # Errors
    ///
    /// A typed [`SetFitError`] naming the field, module, or tensor that failed.
    pub(crate) fn open(dir: &Path) -> Result<Self, SetFitError> {
        let _ = dir;
        Err(SetFitError::BatchInvalid {
            reason: "MiniLmImport::open is not implemented".to_string(),
        })
    }

    /// Open a slice fixture APR.
    ///
    /// Bypasses ONLY the equality-with-the-pin checks. Structural consistency,
    /// tensor presence/shape, remap validity and finiteness all still apply.
    ///
    /// SEALED (D-08): `pub(crate)`. The public fixture entry point is
    /// `SetFitMiniLm::from_slice_fixture` (01-07).
    ///
    /// # Errors
    ///
    /// A typed [`SetFitError`] naming what failed.
    #[cfg(feature = "conformance-fixtures")]
    pub(crate) fn open_slice_fixture(
        apr: &Path,
        config: &SliceConfig,
        remap: &VocabRemap,
    ) -> Result<Self, SetFitError> {
        let _ = (apr, config, remap);
        Err(SetFitError::BatchInvalid {
            reason: "MiniLmImport::open_slice_fixture is not implemented".to_string(),
        })
    }

    /// Dimensions this import loaded.
    #[must_use]
    pub fn dims(&self) -> &ModelDims {
        &self.dims
    }

    /// LayerNorm epsilon.
    #[must_use]
    pub fn layer_norm_eps(&self) -> f32 {
        self.layer_norm_eps
    }

    /// Upstream revision this import was validated against.
    #[must_use]
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Sha256 of the tokenizer this import is paired with.
    #[must_use]
    pub fn tokenizer_sha256(&self) -> &str {
        &self.tokenizer_sha256
    }

    /// `None` after a full-pin open; `Some` after a slice open.
    #[must_use]
    pub fn vocab_remap(&self) -> Option<&VocabRemap> {
        self.vocab_remap.as_ref()
    }

    /// The weights reader, for the encoder (01-06).
    pub(crate) fn reader(&self) -> &AprV2Reader {
        &self.reader
    }

    /// The `bert.` / `` prefix the checkpoint uses.
    pub(crate) fn tensor_prefix(&self) -> &'static str {
        self.tensor_prefix
    }
}

#[cfg(all(test, feature = "setfit"))]
#[path = "import_tests.rs"]
mod import_tests;
