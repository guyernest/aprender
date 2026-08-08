
// ============================================================================
// Masked mean pooling (plan 01-01)
// Contract: setfit-encoder-conformance-v1, equation `masked_mean_pool`
// ============================================================================

/// Reduce a `[B, S, H]` token batch to `[B, H]` sentence embeddings by averaging
/// only the VALID positions of each row.
///
/// `out[b][h] = (Σ_s mask[b*S + s] · hidden[b][s][h]) / n_b`, where
/// `n_b = Σ_s mask[b*S + s]`.
///
/// Row-major throughout (LAYOUT-001): `hidden` is indexed
/// `b * S * H + s * H + h`.
///
/// # Why the denominator is checked
///
/// This is the D-03 checked denominator. A row with no valid position would
/// divide by zero and emit `NaN`, which then silently poisons every parameter
/// gradient downstream — the failure surfaces nowhere near its cause. The row is
/// rejected with a typed error BEFORE the pool is computed, so no `NaN` output
/// is reachable.
///
/// The divisor is also **per row**, not shared. A single batch-wide denominator
/// is invisible on a uniform-length batch and wrong on every mixed-length one.
///
/// # Gradient
///
/// When `hidden` requires grad, the result carries a `MaskedMeanPoolBackward`
/// edge that routes `grad_output[b][h] / n_b` to every valid position and
/// exactly `0.0` to every padded position.
///
/// # Not checked: finiteness of `hidden`
///
/// Unlike `embedding_gather` — whose weight table is loaded from an untrusted
/// model file — `hidden` is a computed graph intermediate produced by the
/// encoder itself. Rejecting a non-finite activation mid-graph would convert a
/// training-dynamics signal into a hard error at an arbitrary layer. Gradient
/// finiteness is asserted where it is meaningful: at the ENC-04 gate.
///
/// # Errors
///
/// * [`OpError::ShapeMismatch`] — `hidden` is not 3-D.
/// * [`OpError::ZeroDimension`] — `batch`, `seq` or `hidden` is 0.
/// * [`OpError::ShapeOverflow`] — `batch * seq` overflows `usize`.
/// * [`OpError::LengthMismatch`] — `mask.len()` is not `batch * seq`.
/// * [`OpError::NonBinaryMaskValue`] — a mask entry is neither `0` nor `1`.
/// * [`OpError::AllPaddingRow`] — a row has no valid position (the checked
///   denominator).
#[provable_contracts_macros::contract(
    "setfit-encoder-conformance-v1",
    equation = "masked_mean_pool"
)]
pub fn masked_mean_pool(hidden: &Tensor, mask: &[u8]) -> Result<Tensor, OpError> {
    // RED stub — the real implementation lands in the GREEN commit.
    let _ = (hidden, mask);
    Err(OpError::ShapeMismatch {
        expected: Vec::new(),
        got: Vec::new(),
    })
}
