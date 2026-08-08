//! The MiniLM tokenizer boundary (ENC-02, D-07).
//!
//! Wraps the pinned HuggingFace `tokenizers` WordPiece tokenizer and returns a
//! [`SentenceBatch`]: ordered **canonical** vocabulary ids, token type ids, an
//! attention mask, per-input truncation facts, per-input provenance, and the
//! sha256 of the tokenizer that produced them.
//!
//! # Canonical ids stay canonical
//!
//! `input_ids` are always the tokenizer's real vocabulary ids. The slice
//! fixtures' `orig_to_slice` remap is carried by the *import* and applied inside
//! the encoder at gather time — a `SentenceBatch` is never rewritten to fit a
//! slice. Rewriting it would make tokenizer identity a function of which model
//! happened to consume the batch.
//!
//! # Truncation-fact strategy
//!
//! STRATEGY IN FORCE: **one user-facing call; the batch is tokenized once, and
//! rows that were actually truncated are re-tokenized once more without
//! truncation to recover their true length.** The extra pass is restricted to
//! truncated rows and is therefore empty for every non-truncating input.
//!
//! This is recorded rather than glossed because the plan's preferred
//! single-pass derivation — `original_len = ids.len() + sum(overflowing.len())`
//! — does **not** hold for the pinned `tokenizers` 0.23.1 with padding enabled.
//! Measured on the frozen `truncation_long` fixture (482 real tokens):
//! post-processing adds `[CLS]`/`[SEP]` to *each* overflow chunk and padding
//! then pads each chunk to the batch-longest width, so the naive sum reports
//! **512**, not 482. Deriving the count from a formula that is off by the
//! specials-and-padding of every overflow chunk would make ENC-02's reported
//! `original_len` quietly wrong exactly on the inputs it exists to describe.
//! ENC-02's guarantee is therefore stated as "one user-facing call", not
//! "one tokenizer pass".
//!
//! # Sealing (D-08 + W1)
//!
//! [`MiniLmTokenizer::from_bytes`] is `pub(crate)`: out-of-crate callers obtain a
//! tokenizer only through `SetFitMiniLm`, which builds the tokenizer and the
//! encoder together from one source, so a mismatched pair is not constructible.
//! [`SentenceBatch`]'s fields are `pub(crate)` with read-only accessors, so
//! out-of-crate code can neither forge a batch stamped with a borrowed
//! `tokenizer_sha256` nor mutate the ids of a batch it legitimately received.

use sha2::{Digest, Sha256};

use super::error::SetFitError;

/// Maximum sequence length of the pinned sentence-transformers configuration.
///
/// `sentence_bert_config.json` for all-MiniLM-L6-v2 sets `max_seq_length: 256`;
/// the frozen fixtures were generated at that bound.
pub const MAX_SEQUENCE_LENGTH: usize = 256;

/// What truncation did to one input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncationFact {
    /// Whether the input was longer than [`MAX_SEQUENCE_LENGTH`].
    pub truncated: bool,
    /// Token count of the input **before** truncation, special tokens included.
    pub original_len: usize,
}

/// Which input produced a row, and what that input was.
///
/// Carried so a downstream embedding can be traced back to the exact bytes that
/// produced it without retaining the text itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputProvenance {
    /// Position of this input in the `texts` slice passed to `encode_batch`.
    pub index: usize,
    /// Lowercase-hex sha256 of the input text's UTF-8 bytes.
    pub text_sha256: String,
}

/// A tokenized batch, ready for the encoder.
///
/// # Read-only outside the crate (W1)
///
/// Every field is `pub(crate)`. In-crate code (the encoder, 01-06) reads the
/// fields directly; out-of-crate code reads through the accessors below and can
/// neither build a `SentenceBatch` literal nor mutate one it received.
///
/// This is what makes the encoder's `tokenizer_sha256` equality check
/// meaningful. With `pub` fields the check would compare a value the caller
/// controls: out-of-crate code could hand-build a batch stamped with a
/// legitimate hash, or take a batch from `SetFitMiniLm::tokenize()` and mutate
/// `input_ids` while leaving the hash intact. Both defeat the check while the
/// D-08 constructor seal remains formally intact, so the seal is enforced at the
/// data layer too.
///
/// `#[non_exhaustive]` is deliberately NOT used: it is redundant once the fields
/// are `pub(crate)`, which already blocks out-of-crate struct-literal
/// construction and exhaustive destructuring, and it is a no-op within the
/// defining crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentenceBatch {
    /// Row-major `[batch * seq]` CANONICAL vocabulary ids.
    pub(crate) input_ids: Vec<u32>,
    /// Row-major `[batch * seq]` token type ids.
    pub(crate) token_type_ids: Vec<u32>,
    /// Row-major `[batch * seq]` mask: `1` keep, `0` padding.
    pub(crate) attention_mask: Vec<u8>,
    /// Number of inputs in the batch.
    pub(crate) batch: usize,
    /// Padded sequence length (longest row in the batch, capped at the max).
    pub(crate) seq: usize,
    /// Per-input truncation facts, in input order.
    pub(crate) truncation: Vec<TruncationFact>,
    /// Per-input provenance, in input order.
    pub(crate) provenance: Vec<InputProvenance>,
    /// Sha256 of the tokenizer that produced this batch (D-08 defense in depth).
    pub(crate) tokenizer_sha256: String,
}

impl SentenceBatch {
    /// Row-major `[batch * seq]` canonical vocabulary ids.
    #[must_use]
    pub fn input_ids(&self) -> &[u32] {
        &self.input_ids
    }

    /// Row-major `[batch * seq]` token type ids.
    #[must_use]
    pub fn token_type_ids(&self) -> &[u32] {
        &self.token_type_ids
    }

    /// Row-major `[batch * seq]` attention mask (`1` keep, `0` padding).
    #[must_use]
    pub fn attention_mask(&self) -> &[u8] {
        &self.attention_mask
    }

    /// Number of inputs in the batch.
    #[must_use]
    pub fn batch(&self) -> usize {
        self.batch
    }

    /// Padded sequence length.
    #[must_use]
    pub fn seq(&self) -> usize {
        self.seq
    }

    /// Per-input truncation facts, in input order.
    #[must_use]
    pub fn truncation(&self) -> &[TruncationFact] {
        &self.truncation
    }

    /// Per-input provenance, in input order.
    #[must_use]
    pub fn provenance(&self) -> &[InputProvenance] {
        &self.provenance
    }

    /// Sha256 of the tokenizer that produced this batch.
    #[must_use]
    pub fn tokenizer_sha256(&self) -> &str {
        &self.tokenizer_sha256
    }
}

/// The pinned MiniLM WordPiece tokenizer.
pub struct MiniLmTokenizer {
    /// Configured with truncation at [`MAX_SEQUENCE_LENGTH`] and
    /// batch-longest padding.
    inner: tokenizers::Tokenizer,
    /// Same vocabulary, no truncation and no padding. Used only to recover the
    /// true length of rows that the truncating pass actually cut.
    untruncated: tokenizers::Tokenizer,
    /// Lowercase-hex sha256 of the bytes this tokenizer was built from.
    tokenizer_sha256: String,
}

impl std::fmt::Debug for MiniLmTokenizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MiniLmTokenizer")
            .field("tokenizer_sha256", &self.tokenizer_sha256)
            .field("max_sequence_length", &MAX_SEQUENCE_LENGTH)
            .finish()
    }
}

/// Lowercase-hex sha256 of a byte slice.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        // `write!` into a String is infallible; the Result is discarded rather
        // than unwrapped so no panic path exists here.
        let _ = write!(out, "{b:02x}");
    }
    out
}

impl MiniLmTokenizer {
    /// Build a tokenizer from `tokenizer.json` bytes.
    ///
    /// SEALED (D-08): `pub(crate)`. Out-of-crate callers reach a tokenizer only
    /// via `SetFitMiniLm`, which constructs the tokenizer and the encoder
    /// together — so a mismatched pair is not constructible.
    ///
    /// # Errors
    ///
    /// [`SetFitError::TokenizerLoad`] if the bytes are not a parseable
    /// `tokenizers` serialization, or if truncation/padding cannot be
    /// configured on it.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, SetFitError> {
        let _ = bytes;
        Err(SetFitError::BatchInvalid {
            reason: "MiniLmTokenizer::from_bytes is not implemented".to_string(),
        })
    }

    /// Sha256 of the bytes this tokenizer was built from.
    #[must_use]
    pub fn tokenizer_sha256(&self) -> &str {
        &self.tokenizer_sha256
    }

    /// Tokenize a batch of texts.
    ///
    /// Truncates at [`MAX_SEQUENCE_LENGTH`] and pads to the longest row in the
    /// batch. Ids are canonical.
    ///
    /// # Errors
    ///
    /// [`SetFitError::BatchInvalid`] if `texts` is empty or the tokenizer
    /// returns a malformed encoding; [`SetFitError::TokenizerLoad`] if the
    /// underlying tokenizer fails on an input.
    pub fn encode_batch(&self, texts: &[&str]) -> Result<SentenceBatch, SetFitError> {
        let _ = texts;
        Err(SetFitError::BatchInvalid {
            reason: "MiniLmTokenizer::encode_batch is not implemented".to_string(),
        })
    }
}

#[cfg(all(test, feature = "setfit"))]
#[path = "tokenizer_tests.rs"]
mod tokenizer_tests;
