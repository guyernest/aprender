
// ============================================================================
// Batched embedding gather (plan 01-01)
// Contract: setfit-encoder-conformance-v1, equation `embedding_gather`
// ============================================================================

/// Gather a `[B, S, H]` batch of token embeddings from a `[V, H]` weight table.
///
/// Row-major throughout (LAYOUT-001): the output element `(b, s, h)` lives at
/// `b * S * H + s * H + h`, and weight row `id` starts at `id * H`.
///
/// # Gradient
///
/// When `weight` requires grad, the result carries the existing
/// [`EmbeddingBackward`] edge with the **flattened** `B*S` index list. That
/// backward SCATTER-ADDs `grad_output[i]` into `dW[ids[i]]`, so a token id that
/// appears `k` times in the batch accumulates `k` gradient rows into its weight
/// row. Overwriting instead of accumulating would silently drop gradient for
/// every repeated token — the single most common way a gather backward is wrong.
///
/// The token ids are integers and carry no gradient.
///
/// # Errors
///
/// Fails **closed** — never a zero-filled row, never a panic:
///
/// * [`OpError::ShapeMismatch`] — `weight` is not 2-D, or `ids.len()` is not
///   `batch * seq`.
/// * [`OpError::ZeroDimension`] — `batch`, `seq`, `hidden` or `vocab_size` is 0.
///   An empty tensor is refused because it silently no-ops downstream.
/// * [`OpError::ShapeOverflow`] — `batch * seq * hidden` overflows `usize`.
///   Checked with `checked_mul` BEFORE the output buffer is allocated.
/// * [`OpError::NonFiniteInput`] — `weight` contains `NaN` or `±Inf`.
/// * [`OpError::OutOfVocabulary`] — an id is at or beyond `vocab_size`.
///   Deliberately NOT the `aprender-train` zero-fill nor the `qwen2` N-09
///   warn-and-emit-zeros escape: both hide the defect until it shows up as
///   unexplained accuracy loss.
#[provable_contracts_macros::contract(
    "setfit-encoder-conformance-v1",
    equation = "embedding_gather"
)]
pub fn embedding_gather(
    weight: &Tensor,
    ids: &[u32],
    batch: usize,
    seq: usize,
) -> Result<Tensor, OpError> {
    // RED stub — the real implementation lands in the GREEN commit.
    let _ = (weight, ids, batch, seq);
    Err(OpError::ShapeMismatch {
        expected: Vec::new(),
        got: Vec::new(),
    })
}
