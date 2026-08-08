//! The differentiable MiniLM sentence encoder (ENC-03, ENC-05).
//!
//! Contract: `setfit-encoder-conformance-v1`, equation `setfit_encoder_forward`.
//!
//! # One forward implementation
//!
//! [`BertSentenceEncoder::forward_layers`] is the ONLY place a layer is run.
//! [`BertSentenceEncoder::forward_tokens`] returns its last layer output and
//! [`BertSentenceEncoder::forward_tokens_per_layer`] returns the whole
//! `(embeddings_out, layer_outputs)` pair. Collecting the intermediates is
//! unconditional — the `Vec` is built in every build — so the conformance build
//! and the production build compute identically; only the public accessor is
//! `cfg`-gated. A `to_bits` test asserts the two entry points agree elementwise,
//! which is what makes "one implementation" structural rather than aspirational
//! (T-1-28): a per-layer parity gate that compared against a path production
//! never runs would localize failures in a model nobody ships.
//!
//! # Composition
//!
//! Built ONLY from `nn/` building blocks and the ungated autograd primitives.
//! `models/bert/` is untouched beyond 01-05's sanctioned A-01 `read_tensor`
//! visibility line (D-01) — in particular the BERT embeddings module, which
//! `assert!`s on over-length input and then slices unchecked, is never reached.
//!
//! Every intermediate flows through an autograd-aware operation. A
//! `Tensor::new` / `Tensor::from_vec` over a COMPUTED value with no adjacent
//! `grad_fn` is the PMAT-913/914/922 severed-graph class and is forbidden here;
//! the only raw tensors this module builds are the position/token-type id
//! vectors, which are integers and carry no gradient by construction.
//!
//! # Mode (ENC-05)
//!
//! [`Module::set_training`] is the recursive propagation channel (D-17) and
//! [`Module::train`] / [`Module::eval`] delegate to it, so both spellings flip
//! every dropout site including the one inside `MultiHeadAttention`. Flipping
//! mode never adds, removes or mutates a registered parameter.
//!
//! `from_import` returns an encoder in **eval** mode, matching HuggingFace
//! `from_pretrained`, which calls `model.eval()` before handing the model back.
//! That is also the mode the frozen fixtures were generated in (D-16). Training
//! callers flip it explicitly with `set_training(true)`.

// Same D-08 consequence import.rs records: `from_import` is `pub(crate)` under
// the seal and has no non-test caller until 01-07's `SetFitMiniLm`, so a
// library-only build walks everything it reaches — `site_seed`, `DROPOUT_P`,
// `install_projection`, the site-name helpers — as unreachable. Targeted
// `#[allow]`s were tried first and MEASURED to be whack-a-mole: silencing
// `install_projection` and `EMBEDDINGS_DROPOUT_SITE` simply moved the finding to
// `site_seed`, because the whole construction path hangs off one sealed entry
// point. Widening the visibility to silence it would break the seal, which is
// the wrong trade. Delete this the moment 01-07 wires `SetFitMiniLm`.
#![allow(dead_code)]

use crate::autograd::{
    additive_attention_mask, embedding_gather, l2_normalize_rows, masked_mean_pool, OpError, Tensor,
};
use crate::models::bert::load::read_tensor;
use crate::nn::{Dropout, LayerNorm, Linear, Module, MultiHeadAttention};

use super::error::SetFitError;
use super::import::{MiniLmImport, ModelDims, VocabRemap};
use super::tokenizer::{SentenceBatch, MAX_SEQUENCE_LENGTH};

/// Epsilon of the trailing L2 normalization, matching the pinned
/// sentence-transformers `Normalize` module.
///
/// `pub(crate)` so the pair objective (01-07 `setfit/loss.rs`) clamps its cosine
/// norms with the SAME constant this encoder normalized with, rather than a
/// second literal that can drift. Same single-source-of-truth reasoning 01-06
/// applied to the two pinned dropout probabilities.
pub(crate) const L2_EPS: f32 = 1e-12;

/// Dropout probability at every HF-verified site.
///
/// Single source of truth check: the ENC-01 pin rejects any checkpoint whose
/// `hidden_dropout_prob` or `attention_probs_dropout_prob` differs from `0.1`
/// (`import::PINNED_HIDDEN_DROPOUT_PROB` / `PINNED_ATTENTION_DROPOUT_PROB`), and
/// `encoder_dropout_probability_agrees_with_the_enc01_pin` asserts this constant
/// equals both of them at `f32`. So the value cannot drift in one place only.
const DROPOUT_P: f32 = 0.1;

// ---------------------------------------------------------------------------
// Dropout site naming and seeding
// ---------------------------------------------------------------------------

/// Site 1: embeddings, after LayerNorm.
const EMBEDDINGS_DROPOUT_SITE: &str = "embeddings.dropout";

/// Site 2 of layer `i`: attention probabilities, after softmax and before `@V`.
///
/// Named for HF's `BertSelfAttention.dropout`.
fn attention_probs_site(layer: usize) -> String {
    format!("encoder.layer.{layer}.attention.self.dropout")
}

/// Site 3 of layer `i`: attention output dense, before the residual add.
fn attention_output_site(layer: usize) -> String {
    format!("encoder.layer.{layer}.attention.output.dropout")
}

/// Site 4 of layer `i`: FFN output dense, before the residual add.
fn ffn_output_site(layer: usize) -> String {
    format!("encoder.layer.{layer}.output.dropout")
}

/// Derive a site's RNG seed from the root seed and its DOTTED NAME.
///
/// Non-positional on purpose: inserting a layer must not renumber the streams of
/// the layers after it. The trade is that RENAMING a site changes its stream,
/// which is acceptable — the names are the HF ones and are pinned by the
/// parameter-order gate — whereas positional drift is not, because it silently
/// re-addresses every site downstream of an edit.
///
/// FNV-1a over the name, folded with the root seed through SplitMix64's
/// finaliser so a one-character name change moves the whole stream.
fn site_seed(root_seed: u64, site: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in site.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let mut z = h ^ root_seed.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

// ---------------------------------------------------------------------------
// Layer
// ---------------------------------------------------------------------------

/// One HF BERT encoder layer (post-norm).
///
/// `attention.out_proj` IS `attention.output.dense`: `MultiHeadAttention`
/// applies it inside `forward_self`, so this struct holds only what comes after.
struct EncoderLayer {
    attention: MultiHeadAttention,
    attention_output_dropout: Dropout,
    attention_layer_norm: LayerNorm,
    intermediate: Linear,
    output_dense: Linear,
    output_dropout: Dropout,
    output_layer_norm: LayerNorm,
}

// ---------------------------------------------------------------------------
// Encoder
// ---------------------------------------------------------------------------

/// A graph-connected BERT sentence encoder built from a validated import.
pub struct BertSentenceEncoder {
    word_embeddings: Tensor,
    position_embeddings: Tensor,
    token_type_embeddings: Tensor,
    embeddings_layer_norm: LayerNorm,
    embeddings_dropout: Dropout,
    layers: Vec<EncoderLayer>,
    dims: ModelDims,
    /// `Some` for a slice fixture; `None` for the full pin.
    remap: Option<VocabRemap>,
    /// Sha256 of the tokenizer this encoder is paired with (D-08 defense in
    /// depth — the structural guarantee is the `pub(crate)` constructor).
    tokenizer_sha256: String,
    root_seed: u64,
    training: bool,
}

impl std::fmt::Debug for BertSentenceEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BertSentenceEncoder")
            .field("dims", &self.dims)
            .field("max_seq", &self.max_seq())
            .field("is_slice", &self.remap.is_some())
            .field("training", &self.training)
            .finish_non_exhaustive()
    }
}

impl BertSentenceEncoder {
    /// Build an encoder from a validated import.
    ///
    /// SEALED (D-08, user decision 2026-08-08): `pub(crate)`. Out-of-crate
    /// callers construct via `SetFitMiniLm` (01-07), which pairs this encoder
    /// with the tokenizer from the same source, so a mismatched pair is not
    /// CONSTRUCTIBLE rather than merely detected. The read and forward methods
    /// stay `pub`.
    ///
    /// Returns an encoder in eval mode (see the module docs).
    ///
    /// # Errors
    ///
    /// [`SetFitError::ImportTensor`] if a tensor is missing or the wrong size.
    /// `MiniLmImport` has already validated presence, shape and finiteness of
    /// every tensor read here, so this is defense in depth rather than the
    /// primary gate.
    pub(crate) fn from_import(import: &MiniLmImport, root_seed: u64) -> Result<Self, SetFitError> {
        let dims = import.dims().clone();
        let reader = import.reader();
        let prefix = import.tensor_prefix();
        let eps = import.layer_norm_eps();
        let h = dims.hidden;

        let read = |name: &str, shape: &[usize]| -> Result<Tensor, SetFitError> {
            // A-01 reuse: the checked-read semantics (presence, dtype path,
            // element count) come from the loader they were written for, they
            // are not reimplemented here.
            Ok(read_tensor(reader, &format!("{prefix}{name}"), shape)?.requires_grad())
        };

        let mut embeddings_layer_norm = LayerNorm::with_eps(&[h], eps);
        embeddings_layer_norm.set_weight(read("embeddings.LayerNorm.weight", &[h])?);
        embeddings_layer_norm.set_bias(read("embeddings.LayerNorm.bias", &[h])?);

        let mut layers = Vec::with_capacity(dims.layers);
        for i in 0..dims.layers {
            let p = format!("encoder.layer.{i}");

            // Site 2 lives INSIDE MultiHeadAttention, between softmax and @V.
            // It was the one unseedable site (A5); the hook added by this plan
            // is what makes the whole policy reproducible.
            let mut attention = MultiHeadAttention::new(h, dims.heads)
                .with_dropout(DROPOUT_P)
                .with_attention_dropout_seed(site_seed(root_seed, &attention_probs_site(i)));
            install_projection(attention.q_proj_mut(), &read, &p, "query", h)?;
            install_projection(attention.k_proj_mut(), &read, &p, "key", h)?;
            install_projection(attention.v_proj_mut(), &read, &p, "value", h)?;
            let out_proj = attention.out_proj_mut();
            out_proj.set_weight(read(
                &format!("{p}.attention.output.dense.weight"),
                &[h, h],
            )?);
            out_proj.set_bias(read(&format!("{p}.attention.output.dense.bias"), &[h])?);

            let mut attention_layer_norm = LayerNorm::with_eps(&[h], eps);
            attention_layer_norm.set_weight(read(
                &format!("{p}.attention.output.LayerNorm.weight"),
                &[h],
            )?);
            attention_layer_norm
                .set_bias(read(&format!("{p}.attention.output.LayerNorm.bias"), &[h])?);

            let im = dims.intermediate;
            let mut intermediate = Linear::new(h, im);
            intermediate.set_weight(read(&format!("{p}.intermediate.dense.weight"), &[im, h])?);
            intermediate.set_bias(read(&format!("{p}.intermediate.dense.bias"), &[im])?);

            let mut output_dense = Linear::new(im, h);
            output_dense.set_weight(read(&format!("{p}.output.dense.weight"), &[h, im])?);
            output_dense.set_bias(read(&format!("{p}.output.dense.bias"), &[h])?);

            let mut output_layer_norm = LayerNorm::with_eps(&[h], eps);
            output_layer_norm.set_weight(read(&format!("{p}.output.LayerNorm.weight"), &[h])?);
            output_layer_norm.set_bias(read(&format!("{p}.output.LayerNorm.bias"), &[h])?);

            layers.push(EncoderLayer {
                attention,
                attention_output_dropout: Dropout::with_seed(
                    DROPOUT_P,
                    site_seed(root_seed, &attention_output_site(i)),
                ),
                attention_layer_norm,
                intermediate,
                output_dense,
                output_dropout: Dropout::with_seed(
                    DROPOUT_P,
                    site_seed(root_seed, &ffn_output_site(i)),
                ),
                output_layer_norm,
            });
        }

        let mut encoder = Self {
            word_embeddings: read("embeddings.word_embeddings.weight", &[dims.vocab, h])?,
            position_embeddings: read(
                "embeddings.position_embeddings.weight",
                &[dims.max_positions, h],
            )?,
            token_type_embeddings: read(
                "embeddings.token_type_embeddings.weight",
                &[dims.type_vocab, h],
            )?,
            embeddings_layer_norm,
            embeddings_dropout: Dropout::with_seed(
                DROPOUT_P,
                site_seed(root_seed, EMBEDDINGS_DROPOUT_SITE),
            ),
            layers,
            dims,
            remap: import.vocab_remap().cloned(),
            tokenizer_sha256: import.tokenizer_sha256().to_string(),
            root_seed,
            training: true,
        };
        // HF `from_pretrained` hands back an eval-mode model; so does this.
        encoder.set_training(false);
        Ok(encoder)
    }

    /// Maximum accepted padded sequence length for THIS encoder.
    ///
    /// `min(MAX_SEQUENCE_LENGTH, max_position_embeddings)`. The minimum, not the
    /// constant: the slice carries only 64 position rows, so a hardcoded `<=
    /// 256` would admit a batch whose position gather runs off the end of the
    /// table.
    #[must_use]
    pub fn max_seq(&self) -> usize {
        MAX_SEQUENCE_LENGTH.min(self.dims.max_positions)
    }

    /// The root seed every dropout site's stream is derived from.
    #[must_use]
    pub fn root_seed(&self) -> u64 {
        self.root_seed
    }

    /// Number of encoder layers this model was built with.
    ///
    /// A READ accessor (the D-08 seal is about constructors). 01-07's
    /// `FreezeGroup` validation needs it: `LayerAttention(7)` against a 2-layer
    /// slice must be a typed rejection, and the only honest source for "how many
    /// layers" is the encoder that was actually built.
    #[must_use]
    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }

    /// Sha256 of the tokenizer this encoder is paired with.
    ///
    /// A READ accessor. It exists so the pairing `SetFitMiniLm` establishes can
    /// be ASSERTED rather than assumed — the forward-time equality check is the
    /// runtime half, this is what lets a test see the value it compares.
    #[must_use]
    pub fn tokenizer_sha256(&self) -> &str {
        &self.tokenizer_sha256
    }

    /// Ordered dotted names of every ACTIVE dropout site.
    ///
    /// Real introspection, not a re-derived name list: each entry is emitted
    /// only if the module that implements it exists AND is active, so a site
    /// that was never wired cannot appear. That distinction is the whole point —
    /// a behavioural proxy ("the output changed") cannot tell "site missing"
    /// from "site present but `p` effectively 0", and both are ways ENC-05's
    /// dropout placement can be quietly wrong.
    #[cfg(test)]
    pub(crate) fn dropout_sites(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.embeddings_dropout.probability() > 0.0 {
            out.push(EMBEDDINGS_DROPOUT_SITE.to_string());
        }
        for (i, layer) in self.layers.iter().enumerate() {
            if layer.attention.dropout_p() > 0.0
                && layer.attention.attention_dropout_seed().is_some()
            {
                out.push(attention_probs_site(i));
            }
            if layer.attention_output_dropout.probability() > 0.0 {
                out.push(attention_output_site(i));
            }
            if layer.output_dropout.probability() > 0.0 {
                out.push(ffn_output_site(i));
            }
        }
        out
    }

    /// Graph-connected token states `[B, S, H]`.
    ///
    /// A thin wrapper over `forward_layers` — it runs no layer of its own.
    ///
    /// # Errors
    ///
    /// A typed [`SetFitError`] for a foreign tokenizer, a malformed batch, an
    /// oversize sequence, or an id outside the vocabulary / slice closure.
    /// Op failures arrive as [`SetFitError::Op`].
    #[provable_contracts_macros::contract(
        "setfit-encoder-conformance-v1",
        equation = "setfit_encoder_forward"
    )]
    pub fn forward_tokens(&self, batch: &SentenceBatch) -> Result<Tensor, SetFitError> {
        contract_pre_setfit_encoder_forward!(batch.input_ids());
        let (_, mut layer_outputs) = self.forward_layers(batch)?;
        // A zero-layer configuration is rejected at import, so an empty vec is
        // an internal invariant violation rather than a user-reachable state.
        let result = layer_outputs.pop().ok_or(SetFitError::BatchInvalid {
            reason: "encoder has no layers".to_string(),
        })?;
        contract_post_setfit_encoder_forward!(result.data());
        Ok(result)
    }

    /// Per-layer intermediates for the D-15 localization gate (01-08).
    ///
    /// Returns `(embeddings_out [B,S,H], layer_outputs: one [B,S,H] per encoder
    /// layer, in order)`. `layer_outputs.last()` IS the tensor
    /// [`Self::forward_tokens`] returns — the fixture's `final_tokens`.
    ///
    /// `pub`, not `pub(crate)`: 01-08 is an out-of-crate integration test and
    /// reaches this through 01-07's conformance-gated `SetFitMiniLm::encoder()`.
    /// It is a READ method — it constructs no encoder and no tokenizer, so the
    /// D-08 seal is untouched.
    ///
    /// # Errors
    ///
    /// Identical to [`Self::forward_tokens`]: both inherit the one boundary
    /// validation inside `forward_layers`.
    #[cfg(feature = "conformance-fixtures")]
    pub fn forward_tokens_per_layer(
        &self,
        batch: &SentenceBatch,
    ) -> Result<(Tensor, Vec<Tensor>), SetFitError> {
        self.forward_layers(batch)
    }

    /// `forward_tokens` -> masked mean pool -> L2 normalize: `[B, H]`,
    /// graph-connected, unit-norm rows.
    ///
    /// # Errors
    ///
    /// As [`Self::forward_tokens`], plus [`SetFitError::Op`] from the pooling
    /// and normalization primitives.
    pub fn encode(&self, batch: &SentenceBatch) -> Result<Tensor, SetFitError> {
        let tokens = self.forward_tokens(batch)?;
        let pooled = masked_mean_pool(&tokens, batch.attention_mask())?;
        Ok(l2_normalize_rows(&pooled, L2_EPS)?)
    }

    // -----------------------------------------------------------------------
    // The ONE forward implementation
    // -----------------------------------------------------------------------

    /// Validate the boundary once, gather embeddings, run every layer.
    ///
    /// Both public entry points delegate here, so neither can drift from the
    /// other and neither can skip the validation (T-1-11, T-1-28).
    fn forward_layers(&self, batch: &SentenceBatch) -> Result<(Tensor, Vec<Tensor>), SetFitError> {
        let ids = self.validate(batch)?;
        let b = batch.batch;
        let s = batch.seq;

        // ---- Embeddings --------------------------------------------------
        let word = embedding_gather(&self.word_embeddings, &ids, b, s)?;
        // Position ids are 0..S for every row. Integers, no gradient.
        let position_ids: Vec<u32> = (0..b)
            .flat_map(|_| (0..s).map(|p| u32::try_from(p).unwrap_or(u32::MAX)))
            .collect();
        let position = embedding_gather(&self.position_embeddings, &position_ids, b, s)?;
        let token_type =
            embedding_gather(&self.token_type_embeddings, &batch.token_type_ids, b, s)?;

        let summed = word.add(&position).add(&token_type);
        let normalized = self.embeddings_layer_norm.forward(&summed);
        // The tensor AFTER site 1 is `embeddings_out` — the same quantity
        // forward_per_layer.json records. Dropout is inert in eval, the mode the
        // fixtures were generated in (D-16).
        let embeddings_out = self.embeddings_dropout.forward(&normalized);

        // ---- Mask ---------------------------------------------------------
        // [B,1,1,S] so it broadcasts over [B,heads,S,S] scores through 01-09's
        // repaired `add_mask`, which keeps the autograd edge.
        let attention_mask = additive_attention_mask(&batch.attention_mask, b, s)?;

        // ---- Layers -------------------------------------------------------
        let mut x = embeddings_out.clone();
        let mut layer_outputs = Vec::with_capacity(self.layers.len());
        for layer in &self.layers {
            let (attended, _) = layer.attention.forward_self(&x, Some(&attention_mask));
            let attended = layer.attention_output_dropout.forward(&attended);
            x = layer.attention_layer_norm.forward(&x.add(&attended));

            // gelu_exact (01-09), matching the pin's `hidden_act: "gelu"`. The
            // tanh form is a different function, 4.73e-4 away, and ENC-01
            // rejects any checkpoint that asks for it.
            let intermediate = layer.intermediate.forward(&x).gelu_exact();
            let ffn = layer.output_dense.forward(&intermediate);
            let ffn = layer.output_dropout.forward(&ffn);
            x = layer.output_layer_norm.forward(&x.add(&ffn));

            layer_outputs.push(x.clone());
        }

        Ok((embeddings_out, layer_outputs))
    }

    /// Fail-closed boundary validation, run exactly once per forward.
    ///
    /// Returns the EMBEDDING-TABLE row for each position: canonical ids resolved
    /// through the remap when the import carries one, canonical ids verbatim
    /// otherwise. The [`SentenceBatch`] itself is never mutated.
    fn validate(&self, batch: &SentenceBatch) -> Result<Vec<u32>, SetFitError> {
        // 1. Tokenizer identity, BEFORE any compute (D-08 defense in depth).
        if batch.tokenizer_sha256 != self.tokenizer_sha256 {
            return Err(SetFitError::TokenizerHashMismatch {
                expected: self.tokenizer_sha256.clone(),
                got: batch.tokenizer_sha256.clone(),
            });
        }

        // 2. Shape.
        let b = batch.batch;
        let s = batch.seq;
        if b == 0 || s == 0 {
            return Err(SetFitError::BatchInvalid {
                reason: format!("batch {b} x seq {s}: neither dimension may be zero"),
            });
        }
        let positions = b.checked_mul(s).ok_or_else(|| SetFitError::BatchInvalid {
            reason: format!("batch {b} x seq {s} overflows usize"),
        })?;
        for (field, len) in [
            ("input_ids", batch.input_ids.len()),
            ("token_type_ids", batch.token_type_ids.len()),
            ("attention_mask", batch.attention_mask.len()),
        ] {
            if len != positions {
                return Err(SetFitError::BatchInvalid {
                    reason: format!(
                        "{field} has {len} entries but batch {b} x seq {s} needs {positions}"
                    ),
                });
            }
        }

        // 3. Sequence bound — min(256, max_position_embeddings), so an
        //    over-long batch cannot drive an out-of-range position gather.
        let max = self.max_seq();
        if s > max {
            return Err(SetFitError::OversizeInput { len: s, max });
        }

        // 4. Mask values, then per-row validity. Order matters: a row of all
        //    `2`s is a broken mask, not a padded row.
        for (position, v) in batch.attention_mask.iter().enumerate() {
            if *v > 1 {
                return Err(OpError::NonBinaryMaskValue {
                    value: *v,
                    position,
                }
                .into());
            }
        }
        for row in 0..b {
            let base = row * s;
            if !batch.attention_mask[base..base + s].iter().any(|v| *v == 1) {
                return Err(OpError::AllPaddingRow { row }.into());
            }
        }

        // 5. Token type ids inside the type table.
        for (position, t) in batch.token_type_ids.iter().enumerate() {
            if *t as usize >= self.dims.type_vocab {
                return Err(OpError::OutOfVocabulary {
                    id: *t,
                    vocab_size: self.dims.type_vocab,
                    position,
                }
                .into());
            }
        }

        // 6. Word ids. Canonical ids are remapped to slice rows HERE; the batch
        //    keeps its canonical ids so provenance survives the forward.
        let mut rows = Vec::with_capacity(positions);
        match &self.remap {
            Some(remap) => {
                for id in &batch.input_ids {
                    rows.push(remap.to_slice_row(*id)?);
                }
            }
            None => {
                for (position, id) in batch.input_ids.iter().enumerate() {
                    if *id as usize >= self.dims.vocab {
                        return Err(OpError::OutOfVocabulary {
                            id: *id,
                            vocab_size: self.dims.vocab,
                            position,
                        }
                        .into());
                    }
                    rows.push(*id);
                }
            }
        }
        Ok(rows)
    }

    // -----------------------------------------------------------------------
    // Naming (D-18)
    // -----------------------------------------------------------------------

    /// HF dotted prefix of layer `i`.
    fn layer_prefix(i: usize) -> String {
        format!("encoder.layer.{i}")
    }
}

/// Install one `attention.self.{query,key,value}` projection.
///
/// A free function rather than an inline loop: `q_proj_mut()`, `k_proj_mut()`
/// and `v_proj_mut()` each borrow the whole `MultiHeadAttention` mutably, so
/// they cannot be collected into one iterable.
fn install_projection<F>(
    proj: &mut Linear,
    read: &F,
    layer_prefix: &str,
    hf_name: &str,
    hidden: usize,
) -> Result<(), SetFitError>
where
    F: Fn(&str, &[usize]) -> Result<Tensor, SetFitError>,
{
    proj.set_weight(read(
        &format!("{layer_prefix}.attention.self.{hf_name}.weight"),
        &[hidden, hidden],
    )?);
    proj.set_bias(read(
        &format!("{layer_prefix}.attention.self.{hf_name}.bias"),
        &[hidden],
    )?);
    Ok(())
}

/// Translate a `MultiHeadAttention` local parameter name to its HF path.
///
/// The mapping is by NAME, not by position, so a reordering inside
/// `MultiHeadAttention` surfaces as an out-of-order parameter list against the
/// frozen `parameter_order` rather than as silently mislabelled tensors. An
/// unrecognised local name is passed through verbatim, which fails that same
/// gate loudly instead of inventing a plausible HF path.
fn hf_attention_name(local: &str) -> String {
    match local.split_once('.') {
        Some(("q_proj", leaf)) => format!("attention.self.query.{leaf}"),
        Some(("k_proj", leaf)) => format!("attention.self.key.{leaf}"),
        Some(("v_proj", leaf)) => format!("attention.self.value.{leaf}"),
        Some(("out_proj", leaf)) => format!("attention.output.dense.{leaf}"),
        _ => format!("attention.{local}"),
    }
}

impl Module for BertSentenceEncoder {
    /// Not the ENC-03 entry point.
    ///
    /// The `Module` trait's `forward` takes a bare tensor, which carries no
    /// attention mask and no tokenizer identity — the two things this encoder
    /// validates. Running the layer stack from here would silently attend to
    /// padding. Use [`BertSentenceEncoder::encode`] or
    /// [`BertSentenceEncoder::forward_tokens`]; this impl exists so the encoder
    /// participates in parameter traversal and mode propagation.
    fn forward(&self, input: &Tensor) -> Tensor {
        input.clone()
    }

    fn parameters(&self) -> Vec<&Tensor> {
        self.named_parameters()
            .into_iter()
            .map(|(_, t)| t)
            .collect()
    }

    fn parameters_mut(&mut self) -> Vec<&mut Tensor> {
        self.named_parameters_mut()
            .into_iter()
            .map(|(_, t)| t)
            .collect()
    }

    /// HF dotted names, verbatim, in torch `named_parameters()` order, pooler
    /// excluded.
    ///
    /// Verbatim is the whole point (D-18): `gradients.json`'s keys are torch's
    /// own names, so any translation layer between them and these is a place the
    /// two can disagree. The order is asserted against `parameter_order`, an
    /// ordered ARRAY — JSON object keys carry no ordering guarantee.
    fn named_parameters(&self) -> Vec<(String, &Tensor)> {
        let mut out: Vec<(String, &Tensor)> = vec![
            (
                "embeddings.word_embeddings.weight".to_string(),
                &self.word_embeddings,
            ),
            (
                "embeddings.position_embeddings.weight".to_string(),
                &self.position_embeddings,
            ),
            (
                "embeddings.token_type_embeddings.weight".to_string(),
                &self.token_type_embeddings,
            ),
        ];
        out.extend(
            self.embeddings_layer_norm
                .named_parameters()
                .into_iter()
                .map(|(n, t)| (format!("embeddings.LayerNorm.{n}"), t)),
        );

        for (i, layer) in self.layers.iter().enumerate() {
            let p = Self::layer_prefix(i);
            out.extend(
                layer
                    .attention
                    .named_parameters()
                    .into_iter()
                    .map(|(n, t)| (format!("{p}.{}", hf_attention_name(&n)), t)),
            );
            out.extend(
                layer
                    .attention_layer_norm
                    .named_parameters()
                    .into_iter()
                    .map(|(n, t)| (format!("{p}.attention.output.LayerNorm.{n}"), t)),
            );
            out.extend(
                layer
                    .intermediate
                    .named_parameters()
                    .into_iter()
                    .map(|(n, t)| (format!("{p}.intermediate.dense.{n}"), t)),
            );
            out.extend(
                layer
                    .output_dense
                    .named_parameters()
                    .into_iter()
                    .map(|(n, t)| (format!("{p}.output.dense.{n}"), t)),
            );
            out.extend(
                layer
                    .output_layer_norm
                    .named_parameters()
                    .into_iter()
                    .map(|(n, t)| (format!("{p}.output.LayerNorm.{n}"), t)),
            );
        }
        out
    }

    fn named_parameters_mut(&mut self) -> Vec<(String, &mut Tensor)> {
        let mut out: Vec<(String, &mut Tensor)> = vec![
            (
                "embeddings.word_embeddings.weight".to_string(),
                &mut self.word_embeddings,
            ),
            (
                "embeddings.position_embeddings.weight".to_string(),
                &mut self.position_embeddings,
            ),
            (
                "embeddings.token_type_embeddings.weight".to_string(),
                &mut self.token_type_embeddings,
            ),
        ];
        out.extend(
            self.embeddings_layer_norm
                .named_parameters_mut()
                .into_iter()
                .map(|(n, t)| (format!("embeddings.LayerNorm.{n}"), t)),
        );

        for (i, layer) in self.layers.iter_mut().enumerate() {
            let p = Self::layer_prefix(i);
            out.extend(
                layer
                    .attention
                    .named_parameters_mut()
                    .into_iter()
                    .map(|(n, t)| (format!("{p}.{}", hf_attention_name(&n)), t)),
            );
            out.extend(
                layer
                    .attention_layer_norm
                    .named_parameters_mut()
                    .into_iter()
                    .map(|(n, t)| (format!("{p}.attention.output.LayerNorm.{n}"), t)),
            );
            out.extend(
                layer
                    .intermediate
                    .named_parameters_mut()
                    .into_iter()
                    .map(|(n, t)| (format!("{p}.intermediate.dense.{n}"), t)),
            );
            out.extend(
                layer
                    .output_dense
                    .named_parameters_mut()
                    .into_iter()
                    .map(|(n, t)| (format!("{p}.output.dense.{n}"), t)),
            );
            out.extend(
                layer
                    .output_layer_norm
                    .named_parameters_mut()
                    .into_iter()
                    .map(|(n, t)| (format!("{p}.output.LayerNorm.{n}"), t)),
            );
        }
        out
    }

    /// ENC-05: flip every dropout site, recursively, and nothing else.
    ///
    /// RNG state, seeds and the mode flag are module state, never parameters, so
    /// this changes no registered tensor — proven bytewise by the
    /// train -> eval -> train snapshot test.
    fn set_training(&mut self, training: bool) {
        self.training = training;
        self.embeddings_dropout.set_training(training);
        self.embeddings_layer_norm.set_training(training);
        for layer in &mut self.layers {
            // MultiHeadAttention owns site 2 (attention probs) and propagates
            // into its four projections.
            layer.attention.set_training(training);
            layer.attention_output_dropout.set_training(training);
            layer.attention_layer_norm.set_training(training);
            layer.intermediate.set_training(training);
            layer.output_dense.set_training(training);
            layer.output_dropout.set_training(training);
            layer.output_layer_norm.set_training(training);
        }
    }

    /// Delegates to [`Module::set_training`].
    ///
    /// The crate convention is that `train`/`eval` are leaf-local and
    /// `set_training` is the propagation channel (D-17). That is a real footgun
    /// on a module whose whole point is dropout placement: `encoder.eval()`
    /// leaving dropout active would silently make every "inference" run
    /// stochastic. Both spellings therefore route through the one channel.
    fn train(&mut self) {
        self.set_training(true);
    }

    /// Delegates to [`Module::set_training`]; see [`Module::train`].
    fn eval(&mut self) {
        self.set_training(false);
    }

    fn training(&self) -> bool {
        self.training
    }
}

#[cfg(all(test, feature = "setfit"))]
#[path = "encoder_tests.rs"]
mod encoder_tests;
