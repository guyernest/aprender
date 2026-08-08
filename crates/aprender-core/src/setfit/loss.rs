//! The SetFit pair objective (ENC-06).
//!
//! Contract: `setfit-encoder-conformance-v1`, equation `pair_cosine_mse`.
//!
//! One function, [`pair_cosine_mse`], composed from exactly two 01-03
//! primitives: `cosine_similarity_rows` and `mse_loss`. It computes
//!
//! ```text
//! L = (1/B) * Σ_b ( cos(za[b], zb[b]) - labels[b] )²
//! ```
//!
//! and returns a **graph-connected `[1]` tensor**, not an `f32`.
//!
//! # Why this is its own equation, and why it is not in `nn/`
//!
//! The `nn/loss.rs` and `nn/self_supervised.rs` helpers return `f32`. An `f32`
//! carries no `grad_fn`, so a training loop built on one reports a falling loss
//! while every encoder weight stays exactly where it started — PF-001, the trap
//! this phase exists to close. Nothing here imports from, calls into, or
//! re-exports either module, and a source assertion in `loss_tests.rs` holds
//! that line.
//!
//! The contract annotation names `pair_cosine_mse`, **not** `mse_loss`. They are
//! different obligations: `mse_loss` relates a prediction vector to a target
//! vector, while this relates two `[B,H]` embedding matrices to a vector of
//! binary pair labels. Annotating this wrapper as raw MSE would misdescribe its
//! inputs and quietly drop the two-embedding-input obligation ENC-06 gates.

use crate::autograd::{cosine_similarity_rows, mse_loss, OpError, Tensor};

use super::error::SetFitError;

/// Epsilon floor on each cosine norm.
///
/// The same explicit constant the encoder's trailing L2 normalization uses
/// (`encoder::L2_EPS`), so the pair objective and the embeddings it consumes
/// agree about what "degenerate" means. `pair_loss_epsilon_agrees_with_the_
/// encoder_normalize_path` asserts the equality rather than trusting two
/// literals to stay in step.
pub(crate) const PAIR_COSINE_EPS: f32 = 1e-12;

/// Mean-squared error between row-wise cosine similarity and binary pair
/// labels, as a graph-connected `[1]` tensor.
///
/// `za` and `zb` are the two siamese branches' `[B, H]` sentence embeddings;
/// `labels[b]` is `1.0` when the pair is a positive and `0.0` when it is a
/// negative. The backward reaches BOTH inputs, so one backward pass updates the
/// shared encoder body through both branches.
///
/// # Validation order
///
/// Shapes first (so nothing is computed on mismatched inputs), then labels:
/// **length**, then **finiteness**, then **binary membership**. The finiteness
/// check is explicit and comes first on purpose. `NaN != 0.0 && NaN != 1.0` is
/// true, so the membership test happens to reject `NaN` today — but only
/// incidentally, and it would report "not in {0,1}" for a value whose real
/// problem is that it is not a number.
///
/// # Errors
///
/// * [`SetFitError::Op`] wrapping [`OpError::ShapeMismatch`] — either input is
///   not rank 2, or the two shapes differ.
/// * [`SetFitError::BatchInvalid`] — `labels.len()` disagrees with the batch, a
///   label is non-finite, or a finite label is outside `{0.0, 1.0}`.
/// * [`SetFitError::Op`] — anything the two composed primitives reject
///   (zero dimension, non-finite embedding).
#[provable_contracts_macros::contract(
    "setfit-encoder-conformance-v1",
    equation = "pair_cosine_mse"
)]
pub fn pair_cosine_mse(za: &Tensor, zb: &Tensor, labels: &[f32]) -> Result<Tensor, SetFitError> {
    // RED STUB (plan 01-07 Task 1). Returns an error no test expects, so every
    // assertion below is proven reachable rather than the file merely failing to
    // compile, and the branch builds at every commit.
    let _ = (za, zb, labels);
    Err(SetFitError::RemapInvalid {
        reason: "RED STUB: pair_cosine_mse is not implemented yet".to_string(),
    })
}

/// Silence the unused-import warning while the RED stub is in place.
#[allow(dead_code)]
fn _red_stub_keeps_the_imports_live(a: &Tensor, b: &Tensor, t: &[f32]) -> Result<Tensor, OpError> {
    let s = cosine_similarity_rows(a, b, PAIR_COSINE_EPS)?;
    mse_loss(&s, t)
}

#[cfg(all(test, feature = "setfit"))]
#[path = "loss_tests.rs"]
mod loss_tests;
