//! The differentiable MiniLM sentence encoder (ENC-03, ENC-05).
//!
//! RED STUB (plan 01-06, Task 1). Every method returns a fixed sentinel error so
//! the tests in `encoder_tests.rs` are proven reachable and failing for their own
//! reason before the implementation lands. Replaced wholesale by the GREEN commit.

use crate::autograd::{OpError, Tensor};
use crate::nn::Module;

use super::error::SetFitError;
use super::import::MiniLmImport;
use super::tokenizer::SentenceBatch;

/// A graph-connected BERT sentence encoder built from a validated import.
pub struct BertSentenceEncoder {
    training: bool,
    stub: Tensor,
}

/// The sentinel the RED stub returns. No test expects it, so the RED count is a
/// clean "every behaviour test fails".
fn red_stub() -> SetFitError {
    SetFitError::Op(OpError::ShapeOverflow { dims: Vec::new() })
}

impl BertSentenceEncoder {
    /// SEALED (D-08): `pub(crate)`.
    ///
    /// # Errors
    ///
    /// RED stub: always.
    pub(crate) fn from_import(_import: &MiniLmImport, _root_seed: u64) -> Result<Self, SetFitError> {
        Err(red_stub())
    }

    /// Graph-connected token states `[B, S, H]`.
    ///
    /// # Errors
    ///
    /// RED stub: always.
    pub fn forward_tokens(&self, _batch: &SentenceBatch) -> Result<Tensor, SetFitError> {
        Err(red_stub())
    }

    /// Per-layer intermediates for the D-15 localization gate.
    ///
    /// # Errors
    ///
    /// RED stub: always.
    #[cfg(feature = "conformance-fixtures")]
    pub fn forward_tokens_per_layer(
        &self,
        _batch: &SentenceBatch,
    ) -> Result<(Tensor, Vec<Tensor>), SetFitError> {
        Err(red_stub())
    }

    /// Unit-norm sentence embeddings `[B, H]`.
    ///
    /// # Errors
    ///
    /// RED stub: always.
    pub fn encode(&self, _batch: &SentenceBatch) -> Result<Tensor, SetFitError> {
        Err(red_stub())
    }

    /// Maximum accepted padded sequence length.
    #[must_use]
    pub fn max_seq(&self) -> usize {
        0
    }
}

impl Module for BertSentenceEncoder {
    fn forward(&self, input: &Tensor) -> Tensor {
        input.clone()
    }

    fn parameters(&self) -> Vec<&Tensor> {
        vec![&self.stub]
    }

    fn parameters_mut(&mut self) -> Vec<&mut Tensor> {
        vec![&mut self.stub]
    }

    fn training(&self) -> bool {
        self.training
    }
}

#[cfg(all(test, feature = "setfit"))]
#[path = "encoder_tests.rs"]
mod encoder_tests;
