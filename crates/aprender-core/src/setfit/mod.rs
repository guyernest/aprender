//! SetFit / all-MiniLM-L6-v2 conformance boundary (feature `setfit`).
//!
//! Contract: `setfit-encoder-conformance-v1`. Requirements ENC-01 (pinned
//! import with typed rejection of every unsupported variant) and ENC-02 (exact
//! tokenizer parity against the frozen fixtures in
//! `crates/aprender-core/tests/fixtures/setfit/`).
//!
//! # Feature isolation (D-05 / D-06)
//!
//! The whole module is behind `setfit`, which is the only feature that enables
//! the `tokenizers` dependency. A build without `setfit` must not contain a
//! `tokenizers` node at all — minimal consumers of this crate pay nothing for a
//! conformance path they do not use. `setfit` also enables `sha2`, which the
//! module uses in production code to hash tokenizer bytes and input text, so the
//! feature is dependency-closed and `--features setfit` builds on its own.
//!
//! `conformance-fixtures` implies `setfit` and additionally unlocks the
//! **test-only** slice constructor. That separation is deliberate (RESEARCH
//! Pitfall 3): the fixture gates run against a 2-layer/64-hidden index slice,
//! while the pin gates run against the full pinned architecture. Without two
//! distinct constructors the two families of gate contradict each other, and
//! the usual "fix" — parameterising the pin path by caller-supplied dims — is
//! precisely the failure mode PF-011 records.
//!
//! # Visibility rule for this module (D-08, user decision 2026-08-08)
//!
//! Every constructor that produces a tokenizer or an import is `pub(crate)`, and
//! [`SentenceBatch`]'s fields are `pub(crate)` with read-only public accessors.
//! The bound type (`SetFitMiniLm`, 01-07) is the sole public entry point, and it
//! builds the tokenizer and the encoder together from one source — so a
//! mismatched tokenizer/encoder pair is **not constructible** from outside the
//! crate, rather than merely being detected at runtime. The `tokenizer_sha256`
//! equality check the encoder performs at every forward call is retained as
//! defense in depth for in-crate misuse; it is meaningful precisely because the
//! value it compares is not out-of-crate-writable.
//!
//! Re-exports below are **types only**. Never re-export a sealed constructor as
//! a free function — that would reopen the seal through a path the source
//! assertions do not scan.

pub mod error;
pub mod import;
pub mod tokenizer;

pub use error::SetFitError;
pub use import::{
    MiniLmImport, ModelDims, SliceConfig, VocabRemap, PINNED_ACTIVATION, PINNED_MAX_SEQ_LENGTH,
    PINNED_REVISION, PINNED_TOKENIZER_SHA256,
};
pub use tokenizer::{
    InputProvenance, MiniLmTokenizer, SentenceBatch, TruncationFact, MAX_SEQUENCE_LENGTH,
};
