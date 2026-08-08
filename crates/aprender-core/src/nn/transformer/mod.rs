//! Transformer architecture components (Vaswani et al., 2017).
//!
//! Implements the attention mechanism and transformer layers for
//! sequence-to-sequence modeling.
//!
//! # Example
//!
//! ```ignore
//! use aprender::nn::{MultiHeadAttention, TransformerEncoderLayer, Module};
//! use aprender::autograd::Tensor;
//!
//! // Create a transformer encoder layer
//! let encoder = TransformerEncoderLayer::new(512, 8, 2048);
//!
//! // Process a sequence
//! let x = Tensor::randn(&[32, 10, 512]);  // [batch, seq_len, d_model]
//! let y = encoder.forward(&x);            // [batch, seq_len, d_model]
//! ```
//!
//! # References
//!
//! - Vaswani, A., et al. (2017). Attention is all you need. `NeurIPS`.

use std::sync::atomic::{AtomicU64, Ordering};

use super::dropout::Dropout;
use super::linear::Linear;
use super::module::Module;
use super::normalization::LayerNorm;
use crate::autograd::Tensor;
use trueno::Matrix;

/// Scaled Dot-Product Attention.
///
/// ```text
/// Attention(Q, K, V) = softmax(Q * K^T / sqrt(d_k)) * V
/// ```
///
/// Unseeded: the attention-probs dropout draws from the ambient RNG, exactly as
/// before plan 01-06. The signature is deliberately unchanged so every existing
/// caller — `GroupedQueryAttention` and the attention contract tests — compiles
/// and behaves identically; the seeded hook is a SEPARATE entry point rather
/// than a seventh parameter (see [`scaled_dot_product_attention_seeded`]).
#[provable_contracts_macros::contract("attention-kernel-v1", equation = "attention")]
fn scaled_dot_product_attention(
    query: &Tensor,
    key: &Tensor,
    value: &Tensor,
    attn_mask: Option<&Tensor>,
    dropout_p: f32,
    training: bool,
) -> (Tensor, Tensor) {
    scaled_dot_product_attention_seeded(query, key, value, attn_mask, dropout_p, training, None)
}

/// Scaled Dot-Product Attention with an optional seeded attention-probs dropout.
///
/// This is where the attention math lives; [`scaled_dot_product_attention`] is a
/// delegator that passes `None`. Splitting rather than extending the existing
/// signature was a deliberate choice (plan 01-06): the alternative required
/// editing ten call sites across `attention_gqa.rs` and
/// `tests_attention_contract.rs` — files this change has no business touching —
/// while the alternative the plan warned against, re-deriving the dropout from
/// outside, would mean re-implementing the `attn_weights @ V` product.
///
/// `dropout_seed` is consulted ONLY when `training && dropout_p > 0.0`, so an
/// unseeded caller's numerics are untouched at every dropout setting.
#[provable_contracts_macros::contract("attention-kernel-v1", equation = "attention")]
fn scaled_dot_product_attention_seeded(
    query: &Tensor,
    key: &Tensor,
    value: &Tensor,
    attn_mask: Option<&Tensor>,
    dropout_p: f32,
    training: bool,
    dropout_seed: Option<u64>,
) -> (Tensor, Tensor) {
    contract_pre_attention!(query.data());
    contract_pre_scaled_dot_product!();
    contract_pre_numerical_stability!();
    let d_k = query.shape()[query.ndim() - 1] as f32;
    let scale = 1.0 / d_k.sqrt();

    // Compute attention scores: Q @ K^T / sqrt(d_k)
    let key_t = transpose_last_two(key);
    let scores = matmul_batched(query, &key_t);
    let scores = scale_tensor(&scores, scale);

    // Apply mask (for causal attention or padding)
    let scores = match attn_mask {
        Some(mask) => add_mask(&scores, mask),
        None => scores,
    };

    // Softmax over last dimension
    let attn_weights = softmax_last_dim(&scores);

    // Apply dropout if training
    let attn_weights = if training && dropout_p > 0.0 {
        apply_dropout_seeded(&attn_weights, dropout_p, dropout_seed)
    } else {
        attn_weights
    };

    // Weighted sum: attn_weights @ V
    let output = matmul_batched(&attn_weights, value);

    contract_post_attention!(output.data());
    (output, attn_weights)
}

/// Multi-Head Attention (Vaswani et al., 2017).
///
/// Allows the model to jointly attend to information from different
/// representation subspaces at different positions.
///
/// # Example
///
/// ```ignore
/// let mha = MultiHeadAttention::new(512, 8);  // d_model=512, num_heads=8
/// let q = Tensor::randn(&[32, 10, 512]);
/// let k = Tensor::randn(&[32, 20, 512]);
/// let v = Tensor::randn(&[32, 20, 512]);
/// let (output, attn_weights) = mha.forward_qkv(&q, &k, &v, None);
/// ```
pub struct MultiHeadAttention {
    embed_dim: usize,
    num_heads: usize,
    head_dim: usize,
    dropout_p: f32,

    /// Query projection
    q_proj: Linear,
    /// Key projection
    k_proj: Linear,
    /// Value projection
    v_proj: Linear,
    /// Output projection
    out_proj: Linear,

    /// Optional seed for the attention-probs dropout (plan 01-06, A5).
    ///
    /// `None` — the default — preserves the pre-01-06 behaviour exactly: the
    /// ambient RNG, no reproducibility. `Some` makes the site's mask a function
    /// of the seed and the call index, which is what the SetFit encoder's
    /// deterministic dropout policy needs. Never a parameter (Pitfall 7): seeds
    /// and RNG state are module state and must not appear in
    /// `named_parameters`, or the ENC-05 mode-flip byte-identity proof would
    /// compare values that legitimately change.
    attention_dropout_seed: Option<u64>,

    /// Number of times the seeded dropout has fired.
    ///
    /// Mixed into the per-call seed so the stream ADVANCES across forward
    /// passes. Without it, a seeded site would replay one fixed mask on every
    /// training step — reproducible, but no longer dropout. Two freshly built
    /// modules with the same seed still produce the same sequence, which is the
    /// property the determinism tests assert.
    attention_dropout_calls: AtomicU64,

    training: bool,
}

/// Fold a call index into a site seed (`SplitMix64` finaliser).
fn mix_call_seed(seed: u64, call: u64) -> u64 {
    let mut z = seed ^ call.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl MultiHeadAttention {
    /// Create a new Multi-Head Attention layer.
    ///
    /// # Arguments
    ///
    /// * `embed_dim` - Total dimension of the model (must be divisible by `num_heads`)
    /// * `num_heads` - Number of attention heads
    ///
    /// # Panics
    ///
    /// Panics if `embed_dim` is not divisible by `num_heads`.
    #[must_use]
    pub fn new(embed_dim: usize, num_heads: usize) -> Self {
        assert!(
            embed_dim.is_multiple_of(num_heads),
            "embed_dim ({embed_dim}) must be divisible by num_heads ({num_heads})"
        );

        let head_dim = embed_dim / num_heads;

        Self {
            embed_dim,
            num_heads,
            head_dim,
            dropout_p: 0.0,
            q_proj: Linear::new(embed_dim, embed_dim),
            k_proj: Linear::new(embed_dim, embed_dim),
            v_proj: Linear::new(embed_dim, embed_dim),
            out_proj: Linear::new(embed_dim, embed_dim),
            attention_dropout_seed: None,
            attention_dropout_calls: AtomicU64::new(0),
            training: true,
        }
    }

    /// Set dropout probability.
    #[must_use]
    pub fn with_dropout(mut self, dropout_p: f32) -> Self {
        self.dropout_p = dropout_p;
        self
    }

    /// Make the attention-probs dropout reproducible from `seed` (plan 01-06).
    ///
    /// Opt-in and additive: a `MultiHeadAttention` built without this call keeps
    /// the ambient-RNG behaviour it had before, at every dropout setting.
    #[must_use]
    pub fn with_attention_dropout_seed(mut self, seed: u64) -> Self {
        self.attention_dropout_seed = Some(seed);
        self
    }

    /// The attention-probs dropout seed, if one was installed.
    #[must_use]
    pub fn attention_dropout_seed(&self) -> Option<u64> {
        self.attention_dropout_seed
    }

    /// The attention-probs dropout probability.
    #[must_use]
    pub fn dropout_p(&self) -> f32 {
        self.dropout_p
    }

    /// Mutable access to the Q-projection (GH-326).
    ///
    /// Used by BERT / Llama / similar model loaders to install pre-trained
    /// weights via `q_proj_mut().set_weight(w)` after constructing the MHA
    /// with `MultiHeadAttention::new`. Mirrors the existing
    /// `Linear::placeholder + set_weight + set_bias` lazy-load pattern.
    pub fn q_proj_mut(&mut self) -> &mut Linear {
        &mut self.q_proj
    }

    /// Mutable access to the K-projection (GH-326).
    pub fn k_proj_mut(&mut self) -> &mut Linear {
        &mut self.k_proj
    }

    /// Mutable access to the V-projection (GH-326).
    pub fn v_proj_mut(&mut self) -> &mut Linear {
        &mut self.v_proj
    }

    /// Mutable access to the output projection (GH-326).
    pub fn out_proj_mut(&mut self) -> &mut Linear {
        &mut self.out_proj
    }

    /// Forward pass with separate query, key, value inputs.
    ///
    /// # Arguments
    ///
    /// * `query` - Query tensor [batch, `target_len`, `embed_dim`]
    /// * `key` - Key tensor [batch, `source_len`, `embed_dim`]
    /// * `value` - Value tensor [batch, `source_len`, `embed_dim`]
    /// * `attn_mask` - Optional attention mask [batch, `target_len`, `source_len`]
    ///
    /// # Returns
    ///
    /// Tuple of (output, `attention_weights`)
    #[must_use]
    pub fn forward_qkv(
        &self,
        query: &Tensor,
        key: &Tensor,
        value: &Tensor,
        attn_mask: Option<&Tensor>,
    ) -> (Tensor, Tensor) {
        let batch_size = query.shape()[0];
        let tgt_len = query.shape()[1];
        let src_len = key.shape()[1];

        // Project Q, K, V
        let q = self.q_proj.forward(query);
        let k = self.k_proj.forward(key);
        let v = self.v_proj.forward(value);

        // Reshape for multi-head: [batch, seq, embed] -> [batch, heads, seq, head_dim]
        let q = reshape_for_attention(&q, batch_size, tgt_len, self.num_heads, self.head_dim);
        let k = reshape_for_attention(&k, batch_size, src_len, self.num_heads, self.head_dim);
        let v = reshape_for_attention(&v, batch_size, src_len, self.num_heads, self.head_dim);

        // Scaled dot-product attention. The call counter advances ONLY when the
        // dropout will actually fire, so an eval-mode or p==0 forward leaves the
        // stream exactly where it was — two encoders with the same seed stay in
        // lockstep regardless of how many inference passes they ran.
        let dropout_seed = if self.training && self.dropout_p > 0.0 {
            self.attention_dropout_seed.map(|seed| {
                let call = self.attention_dropout_calls.fetch_add(1, Ordering::Relaxed);
                mix_call_seed(seed, call)
            })
        } else {
            None
        };
        let (attn_output, attn_weights) = scaled_dot_product_attention_seeded(
            &q,
            &k,
            &v,
            attn_mask,
            self.dropout_p,
            self.training,
            dropout_seed,
        );

        // Reshape back: [batch, heads, seq, head_dim] -> [batch, seq, embed]
        let attn_output = reshape_from_attention(&attn_output, batch_size, tgt_len, self.embed_dim);

        // Output projection
        let output = self.out_proj.forward(&attn_output);

        (output, attn_weights)
    }

    /// Self-attention: query, key, value are the same.
    #[must_use]
    pub fn forward_self(&self, x: &Tensor, attn_mask: Option<&Tensor>) -> (Tensor, Tensor) {
        contract_pre_bidirectional_attention!(x.shape());
        self.forward_qkv(x, x, x, attn_mask)
    }

    /// Get `embed_dim`.
    #[must_use]
    pub fn embed_dim(&self) -> usize {
        self.embed_dim
    }

    /// Get `num_heads`.
    #[must_use]
    pub fn num_heads(&self) -> usize {
        self.num_heads
    }
}

impl Module for MultiHeadAttention {
    #[provable_contracts_macros::contract("gqa-kernel-v1", equation = "gqa")]
    fn forward(&self, input: &Tensor) -> Tensor {
        let (output, _) = self.forward_self(input, None);
        output
    }

    fn parameters(&self) -> Vec<&Tensor> {
        let mut params = self.q_proj.parameters();
        params.extend(self.k_proj.parameters());
        params.extend(self.v_proj.parameters());
        params.extend(self.out_proj.parameters());
        params
    }

    fn parameters_mut(&mut self) -> Vec<&mut Tensor> {
        let mut params = self.q_proj.parameters_mut();
        params.extend(self.k_proj.parameters_mut());
        params.extend(self.v_proj.parameters_mut());
        params.extend(self.out_proj.parameters_mut());
        params
    }

    /// Semantic names prefixed `q_proj.` / `k_proj.` / `v_proj.` / `out_proj.`,
    /// in exactly the order `parameters()` above uses.
    ///
    /// MHA sits on the BERT encoder path, so freeze groups address these tensors
    /// by prefix. A positional fallback here (`"0".."7"`) would make a
    /// `LayerAttention(n)` prefix match zero tensors — silently training or
    /// freezing the wrong set with no error raised.
    fn named_parameters(&self) -> Vec<(String, &Tensor)> {
        let mut params: Vec<(String, &Tensor)> = self
            .q_proj
            .named_parameters()
            .into_iter()
            .map(|(n, t)| (format!("q_proj.{n}"), t))
            .collect();
        params.extend(
            self.k_proj
                .named_parameters()
                .into_iter()
                .map(|(n, t)| (format!("k_proj.{n}"), t)),
        );
        params.extend(
            self.v_proj
                .named_parameters()
                .into_iter()
                .map(|(n, t)| (format!("v_proj.{n}"), t)),
        );
        params.extend(
            self.out_proj
                .named_parameters()
                .into_iter()
                .map(|(n, t)| (format!("out_proj.{n}"), t)),
        );
        params
    }

    fn named_parameters_mut(&mut self) -> Vec<(String, &mut Tensor)> {
        let mut params: Vec<(String, &mut Tensor)> = self
            .q_proj
            .named_parameters_mut()
            .into_iter()
            .map(|(n, t)| (format!("q_proj.{n}"), t))
            .collect();
        params.extend(
            self.k_proj
                .named_parameters_mut()
                .into_iter()
                .map(|(n, t)| (format!("k_proj.{n}"), t)),
        );
        params.extend(
            self.v_proj
                .named_parameters_mut()
                .into_iter()
                .map(|(n, t)| (format!("v_proj.{n}"), t)),
        );
        params.extend(
            self.out_proj
                .named_parameters_mut()
                .into_iter()
                .map(|(n, t)| (format!("out_proj.{n}"), t)),
        );
        params
    }

    /// Record the mode locally and propagate it to the four projections through
    /// the `set_training` channel.
    fn set_training(&mut self, training: bool) {
        self.training = training;
        self.q_proj.set_training(training);
        self.k_proj.set_training(training);
        self.v_proj.set_training(training);
        self.out_proj.set_training(training);
    }

    fn train(&mut self) {
        self.training = true;
    }

    fn eval(&mut self) {
        self.training = false;
    }

    fn training(&self) -> bool {
        self.training
    }
}

impl std::fmt::Debug for MultiHeadAttention {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiHeadAttention")
            .field("embed_dim", &self.embed_dim)
            .field("num_heads", &self.num_heads)
            .field("head_dim", &self.head_dim)
            .field("dropout_p", &self.dropout_p)
            .finish_non_exhaustive()
    }
}

/// Transformer Encoder Layer.
///
/// Consists of self-attention followed by a feed-forward network,
/// with residual connections and layer normalization.
///
/// ```text
/// x = x + Dropout(SelfAttention(LayerNorm(x)))
/// x = x + Dropout(FFN(LayerNorm(x)))
/// ```
pub struct TransformerEncoderLayer {
    self_attn: MultiHeadAttention,
    linear1: Linear,
    linear2: Linear,
    norm1: LayerNorm,
    norm2: LayerNorm,
    dropout: Dropout,
    dropout1: Dropout,
    dropout2: Dropout,
    d_model: usize,
    training: bool,
}

impl TransformerEncoderLayer {
    /// Create a new Transformer Encoder Layer.
    ///
    /// # Arguments
    ///
    /// * `d_model` - Dimension of the model
    /// * `nhead` - Number of attention heads
    /// * `dim_feedforward` - Dimension of the feedforward network (typically 4 * `d_model`)
    #[must_use]
    pub fn new(d_model: usize, nhead: usize, dim_feedforward: usize) -> Self {
        Self {
            self_attn: MultiHeadAttention::new(d_model, nhead),
            linear1: Linear::new(d_model, dim_feedforward),
            linear2: Linear::new(dim_feedforward, d_model),
            norm1: LayerNorm::new(&[d_model]),
            norm2: LayerNorm::new(&[d_model]),
            dropout: Dropout::new(0.1),
            dropout1: Dropout::new(0.1),
            dropout2: Dropout::new(0.1),
            d_model,
            training: true,
        }
    }

    /// Set dropout probability.
    pub fn with_dropout(mut self, dropout: f32) -> Self {
        self.dropout = Dropout::new(dropout);
        self.dropout1 = Dropout::new(dropout);
        self.dropout2 = Dropout::new(dropout);
        self.self_attn = self.self_attn.with_dropout(dropout);
        self
    }

    /// Forward with optional attention mask.
    pub fn forward_with_mask(&self, src: &Tensor, src_mask: Option<&Tensor>) -> Tensor {
        contract_pre_encoder_layer!(src.shape());
        // Pre-norm architecture (more stable)
        // Self-attention block
        let src_norm = self.norm1.forward(src);
        let (attn_out, _) = self.self_attn.forward_self(&src_norm, src_mask);
        let attn_out = self.dropout1.forward(&attn_out);
        let src = src.add(&attn_out);

        // Feed-forward block
        let src_norm = self.norm2.forward(&src);
        let ff_out = self.linear1.forward(&src_norm);
        // PMAT-921: use the autograd-aware Tensor::gelu (NOT nn::functional::gelu,
        // which builds its output via Tensor::from_vec and SEVERS the graph). The
        // functional path froze linear1 + norm2 in any end-to-end training run —
        // the per-layer attention gradcheck never exercised the FFN composition,
        // so the severed FFN GELU went unnoticed until the e2e training proof.
        // Both paths use the identical tanh GELU approximation, so forward
        // numerics are unchanged; only the backward edge is restored.
        let ff_out = ff_out.gelu();
        let ff_out = self.dropout.forward(&ff_out);
        let ff_out = self.linear2.forward(&ff_out);
        let ff_out = self.dropout2.forward(&ff_out);

        src.add(&ff_out)
    }
}

impl Module for TransformerEncoderLayer {
    fn forward(&self, input: &Tensor) -> Tensor {
        self.forward_with_mask(input, None)
    }

    fn parameters(&self) -> Vec<&Tensor> {
        let mut params = self.self_attn.parameters();
        params.extend(self.linear1.parameters());
        params.extend(self.linear2.parameters());
        params.extend(self.norm1.parameters());
        params.extend(self.norm2.parameters());
        params
    }

    fn parameters_mut(&mut self) -> Vec<&mut Tensor> {
        let mut params = self.self_attn.parameters_mut();
        params.extend(self.linear1.parameters_mut());
        params.extend(self.linear2.parameters_mut());
        params.extend(self.norm1.parameters_mut());
        params.extend(self.norm2.parameters_mut());
        params
    }

    fn train(&mut self) {
        self.training = true;
        self.self_attn.train();
        self.dropout.train();
        self.dropout1.train();
        self.dropout2.train();
    }

    fn eval(&mut self) {
        self.training = false;
        self.self_attn.eval();
        self.dropout.eval();
        self.dropout1.eval();
        self.dropout2.eval();
    }

    fn training(&self) -> bool {
        self.training
    }
}

impl std::fmt::Debug for TransformerEncoderLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransformerEncoderLayer")
            .field("d_model", &self.d_model)
            .field("self_attn", &self.self_attn)
            .finish_non_exhaustive()
    }
}

/// Transformer Decoder Layer.
///
/// Like encoder but with an additional cross-attention layer.
pub struct TransformerDecoderLayer {
    pub(crate) self_attn: MultiHeadAttention,
    pub(crate) cross_attn: MultiHeadAttention,
    pub(crate) linear1: Linear,
    pub(crate) linear2: Linear,
    pub(crate) norm1: LayerNorm,
    pub(crate) norm2: LayerNorm,
    pub(crate) norm3: LayerNorm,
    pub(crate) dropout: Dropout,
    pub(crate) dropout1: Dropout,
    pub(crate) dropout2: Dropout,
    pub(crate) dropout3: Dropout,
    pub(crate) d_model: usize,
    pub(crate) training: bool,
}

#[path = "positional_encoding.rs"]
mod positional_encoding;
pub use positional_encoding::*;
// ONE PATH: Re-export canonical attention utilities for crate-internal use (UCBD §4).
pub(crate) use positional_encoding::{matmul_batched, reshape_from_attention, transpose_last_two};

#[path = "attention_gqa.rs"]
mod attention_gqa;
pub use attention_gqa::*;

#[path = "attention_helpers.rs"]
mod attention_helpers;
pub use attention_helpers::*;

#[path = "attention.rs"]
mod attention;
