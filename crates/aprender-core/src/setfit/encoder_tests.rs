//! `BertSentenceEncoder` conformance tests (plan 01-06).
//!
//! Everything here runs against the REAL 2-layer / hidden-64 slice weights, not
//! synthetic values: 01-03's spike already proved graph flow on synthetic
//! weights at hidden 16, and repeating that would add no evidence. What is new
//! here is the real import, the real remap, the real tokenizer boundary and the
//! real HF parameter names.
//!
//! Test names all start `encoder_` (or `mha_seeded_dropout_` for the attention
//! hook) so the plan's single positional filter selects exactly this file.

use std::path::PathBuf;

use super::*;

use crate::autograd::{self, OpError};
use crate::setfit::import::{SliceConfig, VocabRemap};
use crate::setfit::tokenizer::MiniLmTokenizer;

// ---------------------------------------------------------------------------
// Fixture plumbing
// ---------------------------------------------------------------------------

fn fixtures_dir() -> PathBuf {
    if let Ok(p) = std::env::var("APRENDER_SETFIT_FIXTURES") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return p;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/setfit")
}

fn read_fixture(name: &str) -> Vec<u8> {
    let path = fixtures_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn encoder_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/setfit/encoder.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// Source assertions — these hold in RED as well as GREEN, on purpose: they
// describe the SHAPE of the implementation, not its behaviour.
// ---------------------------------------------------------------------------

#[test]
fn encoder_from_import_is_sealed_to_pub_crate() {
    let src = encoder_source();
    assert!(
        src.contains("pub(crate) fn from_import("),
        "D-08: from_import must be declared pub(crate)"
    );
    assert!(
        !src.contains("pub fn from_import("),
        "D-08 seal broken: a bare `pub fn from_import(` exists in setfit/encoder.rs"
    );
}

#[test]
fn encoder_does_not_import_the_asserting_bert_embeddings_path() {
    let src = encoder_source();
    // models/bert/embeddings.rs asserts on over-length input and then slices
    // unchecked. D-01 keeps that path out of this encoder entirely.
    assert!(
        !src.contains("models::bert::embeddings"),
        "encoder.rs reaches into the asserting BERT embeddings path"
    );
}

#[test]
fn encoder_dropout_probability_agrees_with_the_enc01_pin() {
    use crate::setfit::import::{PINNED_ATTENTION_DROPOUT_PROB, PINNED_HIDDEN_DROPOUT_PROB};
    // Compared at f32, the precision the model actually computes in: 0.1f32 and
    // 0.1f64 are different numbers, so an f64 comparison would reject the pin's
    // own value (the same narrowing rule 01-05 applied to layer_norm_eps).
    #[allow(clippy::cast_possible_truncation)]
    let hidden = PINNED_HIDDEN_DROPOUT_PROB as f32;
    #[allow(clippy::cast_possible_truncation)]
    let attention = PINNED_ATTENTION_DROPOUT_PROB as f32;
    assert_eq!(super::DROPOUT_P, hidden, "hidden_dropout_prob");
    assert_eq!(super::DROPOUT_P, attention, "attention_probs_dropout_prob");
}

#[test]
fn encoder_defines_no_competing_op_error_conversion() {
    let src = encoder_source();
    // W5: SetFitError::Op + its From impl are 01-05's. A second conversion here
    // would give two ways for the same op failure to reach a caller.
    assert!(
        !src.contains("impl From<OpError>"),
        "encoder.rs defines a bespoke OpError conversion; use `?` on the 01-05 impl"
    );
}

// ---------------------------------------------------------------------------
// Slice-backed tests
// ---------------------------------------------------------------------------

#[cfg(feature = "conformance-fixtures")]
mod slice {
    use super::*;

    /// Seed used by every encoder built here unless a test varies it.
    const SEED: u64 = 0x0106_5E7F_1701;

    fn slice_config() -> SliceConfig {
        SliceConfig::from_json_bytes(&read_fixture("slice_config.json")).expect("slice_config.json")
    }

    fn slice_remap(vocab: usize) -> VocabRemap {
        VocabRemap::from_json_bytes(&read_fixture("vocab_remap.json"), vocab)
            .expect("vocab_remap.json")
    }

    fn slice_import() -> MiniLmImport {
        let cfg = slice_config();
        let remap = slice_remap(cfg.vocab);
        MiniLmImport::open_slice_fixture(&fixtures_dir().join("slice_model.apr"), &cfg, &remap)
            .expect("the frozen slice fixture must open")
    }

    fn encoder() -> BertSentenceEncoder {
        BertSentenceEncoder::from_import(&slice_import(), SEED).expect("encoder must build")
    }

    fn tokenizer() -> MiniLmTokenizer {
        MiniLmTokenizer::from_bytes(&read_fixture("tokenizer.json")).expect("tokenizer must build")
    }

    /// The frozen `mixed_length_pair` case: 5 valid tokens in row 0, 20 in row 1.
    fn mixed_batch() -> SentenceBatch {
        tokenizer()
            .encode_batch(&[
                "Short text.",
                "This sentence is deliberately longer so the batch holds two different \
                 lengths and padding is exercised.",
            ])
            .expect("tokenize")
    }

    fn single_batch() -> SentenceBatch {
        tokenizer()
            .encode_batch(&["A quick brown fox jumps over the lazy dog."])
            .expect("tokenize")
    }

    fn parameter_order() -> Vec<String> {
        let v: serde_json::Value =
            serde_json::from_slice(&read_fixture("gradients.json")).expect("gradients.json");
        v["parameter_order"]
            .as_array()
            .expect("parameter_order is an ordered ARRAY, not an object")
            .iter()
            .map(|s| s.as_str().expect("name").to_string())
            .collect()
    }

    fn analytically_zero() -> Vec<String> {
        let v: serde_json::Value =
            serde_json::from_slice(&read_fixture("gradients.json")).expect("gradients.json");
        v["analytically_zero"]
            .as_array()
            .expect("analytically_zero array")
            .iter()
            .map(|e| e["name"].as_str().expect("name").to_string())
            .collect()
    }

    fn l2(v: &[f32]) -> f64 {
        v.iter()
            .map(|x| f64::from(*x) * f64::from(*x))
            .sum::<f64>()
            .sqrt()
    }

    // -----------------------------------------------------------------------
    // Naming (D-18)
    // -----------------------------------------------------------------------

    #[test]
    fn encoder_named_parameters_match_the_frozen_parameter_order_exactly() {
        let enc = encoder();
        let got: Vec<String> = enc.named_parameters().into_iter().map(|(n, _)| n).collect();
        // Vec<String> equality: same names, same COUNT, same ORDER. Compared
        // against an ordered array rather than JSON object keys, which carry no
        // ordering guarantee.
        assert_eq!(
            got,
            parameter_order(),
            "HF dotted names must equal torch named_parameters() verbatim and in order"
        );
    }

    #[test]
    fn encoder_named_parameters_exclude_the_pooler() {
        let enc = encoder();
        for (name, _) in enc.named_parameters() {
            assert!(
                !name.starts_with("pooler"),
                "pooler.* must not be registered: {name}"
            );
        }
    }

    #[test]
    fn encoder_named_parameters_agree_with_positional_in_arity_and_order() {
        let enc = encoder();
        let positional = enc.parameters();
        let named = enc.named_parameters();
        assert_eq!(named.len(), positional.len(), "named/positional arity");
        for (i, ((name, nt), pt)) in named.iter().zip(positional.iter()).enumerate() {
            assert_eq!(
                nt.id(),
                pt.id(),
                "slot {i} (`{name}`) refers to a different tensor in the two traversals"
            );
        }
        let mut unique: Vec<&String> = named.iter().map(|(n, _)| n).collect();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), named.len(), "duplicate parameter name");
    }

    #[test]
    fn encoder_named_parameters_mut_mirrors_named_parameters() {
        let mut enc = encoder();
        let names: Vec<String> = enc.named_parameters().into_iter().map(|(n, _)| n).collect();
        let mut_names: Vec<String> = enc
            .named_parameters_mut()
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(names, mut_names);
    }

    // -----------------------------------------------------------------------
    // Sequence bound
    // -----------------------------------------------------------------------

    #[test]
    fn encoder_max_seq_is_the_min_of_the_sentence_bound_and_the_position_table() {
        let enc = encoder();
        // The slice has only 64 position rows, so a hardcoded `<= 256` would
        // admit an out-of-range position gather.
        assert_eq!(enc.max_seq(), 64, "min(256, max_position_embeddings=64)");
        assert!(
            enc.max_seq() < crate::setfit::tokenizer::MAX_SEQUENCE_LENGTH,
            "this test is only meaningful while the two bounds differ"
        );
    }

    // -----------------------------------------------------------------------
    // Forward shape and graph connectivity
    // -----------------------------------------------------------------------

    #[test]
    fn encoder_forward_tokens_returns_a_graph_connected_batch() {
        autograd::clear_graph();
        let enc = encoder();
        let batch = mixed_batch();
        let out = enc.forward_tokens(&batch).expect("forward");
        assert_eq!(out.shape(), &[batch.batch(), batch.seq(), 64]);
        assert!(
            out.requires_grad_enabled(),
            "the encoder weights require grad, so the output must too"
        );
    }

    #[test]
    fn encoder_forward_tokens_per_layer_returns_embeddings_plus_one_output_per_layer() {
        autograd::clear_graph();
        let enc = encoder();
        let batch = mixed_batch();
        let (embeddings_out, layer_outputs) =
            enc.forward_tokens_per_layer(&batch).expect("per-layer");

        let want = &[batch.batch(), batch.seq(), 64][..];
        assert_eq!(embeddings_out.shape(), want);
        // The fixture records exactly one layer_outputs entry per encoder layer.
        let fixture: serde_json::Value =
            serde_json::from_slice(&read_fixture("forward_per_layer.json")).expect("fixture");
        let fixture_layers = fixture["cases"][0]["layer_outputs"]
            .as_array()
            .expect("layer_outputs")
            .len();
        assert_eq!(layer_outputs.len(), fixture_layers, "layer count");
        assert_eq!(layer_outputs.len(), 2, "the slice has 2 encoder layers");

        for (i, t) in layer_outputs.iter().enumerate() {
            assert_eq!(t.shape(), want, "layer {i} shape");
            assert!(
                t.requires_grad_enabled(),
                "layer {i} output is detached from the graph"
            );
        }
        assert!(embeddings_out.requires_grad_enabled());
    }

    #[test]
    fn encoder_forward_tokens_is_bitwise_identical_to_the_last_per_layer_output() {
        // EVAL MODE is mandatory here: these are two separate calls, so in train
        // mode the seeded dropout RNG advances between them and the comparison
        // would fail for a reason unrelated to divergence. Weakening this test
        // to a tolerance would destroy the one structural proof that both public
        // entry points route through the single `forward_layers`.
        autograd::clear_graph();
        let mut enc = encoder();
        enc.set_training(false);
        let batch = mixed_batch();

        let direct = enc.forward_tokens(&batch).expect("forward");
        let (_, per_layer) = enc.forward_tokens_per_layer(&batch).expect("per-layer");
        let last = per_layer.last().expect("at least one layer");

        assert_eq!(direct.shape(), last.shape());
        for (i, (a, b)) in direct.data().iter().zip(last.data().iter()).enumerate() {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "element {i}: forward_tokens gave {a}, layer_outputs.last() gave {b} — \
                 the two entry points are running DIFFERENT forward implementations"
            );
        }
    }

    #[test]
    fn encoder_has_exactly_one_layer_loop() {
        let src = encoder_source();
        let needle = "for layer in &self.layers {";
        assert_eq!(
            src.matches(needle).count(),
            1,
            "the compute layer loop must appear exactly once, inside forward_layers"
        );
        let loop_at = src.find(needle).expect("the loop");
        let impl_at = src
            .find("fn forward_layers(")
            .expect("forward_layers must exist");
        assert!(
            loop_at > impl_at,
            "the single layer loop must live inside forward_layers"
        );
    }

    #[test]
    fn encoder_forward_tokens_per_layer_is_public_and_conformance_gated() {
        let src = encoder_source();
        assert_eq!(
            src.matches("pub fn forward_tokens_per_layer").count(),
            1,
            "01-08 is out-of-crate and reaches this through SetFitMiniLm::encoder()"
        );
        let at = src
            .find("pub fn forward_tokens_per_layer")
            .expect("declaration");
        let head = &src[..at];
        assert!(
            head.rfind("#[cfg(feature = \"conformance-fixtures\")]")
                .is_some_and(|g| head[g..].matches("fn ").count() == 0),
            "forward_tokens_per_layer must be immediately preceded by the \
             conformance-fixtures gate"
        );
    }

    #[test]
    fn encoder_uses_the_exact_erf_gelu() {
        let src = encoder_source();
        assert!(src.contains("gelu_exact"), "the FFN must call gelu_exact");
        assert!(
            !src.contains(".gelu()"),
            "the tanh gelu is a DIFFERENT function (4.73e-4 apart) and is rejected by ENC-01"
        );
    }

    // -----------------------------------------------------------------------
    // Boundary rejection matrix (T-1-11, T-1-21)
    // -----------------------------------------------------------------------

    #[test]
    fn encoder_rejects_a_batch_from_a_foreign_tokenizer_through_both_entry_points() {
        let enc = encoder();
        let mut batch = mixed_batch();
        batch.tokenizer_sha256 = "0".repeat(64);

        let err = enc.forward_tokens(&batch).expect_err("must reject");
        assert!(
            matches!(err, SetFitError::TokenizerHashMismatch { .. }),
            "got {err:?}"
        );
        // Through the per-layer entry point too: validation must live in the
        // SHARED path, not be duplicated into whichever caller remembered it.
        let err = enc
            .forward_tokens_per_layer(&batch)
            .expect_err("must reject");
        assert!(
            matches!(err, SetFitError::TokenizerHashMismatch { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn encoder_rejects_a_canonical_id_outside_the_slice_closure() {
        let enc = encoder();
        let mut batch = mixed_batch();
        // 30522 is one past the pinned BERT vocabulary, so it is in no closure.
        batch.input_ids[3] = 30_522;
        let err = enc.forward_tokens(&batch).expect_err("must reject");
        assert_eq!(
            err,
            SetFitError::VocabOutOfSlice {
                canonical_id: 30_522
            },
            "a zero row would be indistinguishable from a legitimate embedding"
        );
    }

    #[test]
    fn encoder_rejects_a_mask_length_mismatch() {
        let enc = encoder();
        let mut batch = mixed_batch();
        batch.attention_mask.pop();
        let err = enc.forward_tokens(&batch).expect_err("must reject");
        match err {
            SetFitError::BatchInvalid { reason } => {
                assert!(reason.contains("attention_mask"), "got {reason}");
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn encoder_rejects_an_all_padding_row() {
        let enc = encoder();
        let mut batch = mixed_batch();
        let seq = batch.seq();
        for v in &mut batch.attention_mask[..seq] {
            *v = 0;
        }
        let err = enc.forward_tokens(&batch).expect_err("must reject");
        assert_eq!(err, SetFitError::Op(OpError::AllPaddingRow { row: 0 }));
    }

    #[test]
    fn encoder_rejects_a_sequence_over_max_seq_even_when_it_is_under_256() {
        let enc = encoder();
        // ~100 real tokens: over the slice's 64 position rows, under the
        // tokenizer's 256 bound, so only the min() form rejects it.
        let long = "alpha beta gamma delta epsilon zeta eta theta iota kappa "
            .repeat(10)
            .trim_end()
            .to_string();
        let batch = tokenizer()
            .encode_batch(&[long.as_str()])
            .expect("tokenize");
        assert!(
            batch.seq() > enc.max_seq() && batch.seq() <= 256,
            "the probe must sit strictly between max_seq ({}) and 256, got {}",
            enc.max_seq(),
            batch.seq()
        );
        let err = enc.forward_tokens(&batch).expect_err("must reject");
        assert_eq!(
            err,
            SetFitError::OversizeInput {
                len: batch.seq(),
                max: 64
            }
        );
    }

    #[test]
    fn encoder_accepts_a_single_sentence_batch() {
        // The rejection matrix above would be satisfied by an encoder that
        // refuses everything. This is the other side of the line.
        autograd::clear_graph();
        let enc = encoder();
        let batch = single_batch();
        let out = enc.forward_tokens(&batch).expect("must accept");
        assert_eq!(out.shape(), &[1, batch.seq(), 64]);
    }

    // -----------------------------------------------------------------------
    // Remap
    // -----------------------------------------------------------------------

    #[test]
    fn encoder_remaps_canonical_ids_internally_without_mutating_the_batch() {
        autograd::clear_graph();
        let enc = encoder();
        let batch = mixed_batch();
        let before = batch.input_ids().to_vec();
        // Canonical ids run far above the 97-row slice table (14108 appears in
        // the frozen mixed_length_pair case), so a forward that succeeds proves
        // the remap ran; without it embedding_gather returns OutOfVocabulary.
        assert!(
            before.iter().any(|id| *id as usize >= 97),
            "the fixture batch must carry ids above the slice vocabulary"
        );
        enc.forward_tokens(&batch).expect("forward");
        assert_eq!(batch.input_ids(), &before[..], "the batch was mutated");
    }

    // -----------------------------------------------------------------------
    // ENC-04 gradient flow on REAL slice weights
    // -----------------------------------------------------------------------

    /// One scalar loss over the whole mixed-length batch, plus the per-name
    /// gradients it produced.
    fn mixed_batch_gradients() -> Vec<(String, Vec<f32>)> {
        autograd::clear_graph();
        let mut enc = encoder();
        // Eval mode: dropout inert, so the measurement is deterministic and
        // reproduces the mode the fixtures were generated in (D-16).
        enc.set_training(false);
        let batch = mixed_batch();
        let tokens = enc.forward_tokens(&batch).expect("forward");

        // A weighted sum rather than a plain sum: a plain sum can cancel
        // structurally and hand back a zero gradient for reasons unrelated to
        // graph connectivity.
        let n = tokens.numel();
        let sel: Vec<f32> = (0..n).map(|i| 0.31 + 0.017 * (i % 13) as f32).collect();
        let loss = tokens
            .mul(&crate::autograd::Tensor::new(&sel, tokens.shape()))
            .sum();
        assert!(loss.item().is_finite(), "loss is {}", loss.item());
        loss.backward();

        enc.named_parameters()
            .into_iter()
            .map(|(name, t)| {
                let g = autograd::get_grad(t.id()).unwrap_or_else(|| {
                    panic!("`{name}` received NO gradient — the graph is severed upstream of it")
                });
                assert_eq!(g.numel(), t.numel(), "`{name}` gradient arity");
                (name, g.data().to_vec())
            })
            .collect()
    }

    #[test]
    fn encoder_backward_gives_a_finite_gradient_to_every_named_parameter() {
        let grads = mixed_batch_gradients();
        assert_eq!(grads.len(), 37, "the slice has 37 registered tensors");
        for (name, g) in &grads {
            if let Some(pos) = g.iter().position(|v| !v.is_finite()) {
                panic!(
                    "`{name}`: non-finite gradient at element {pos} ({})",
                    g[pos]
                );
            }
        }
    }

    #[test]
    fn encoder_backward_gives_a_non_zero_aggregate_gradient_to_every_enc04_component() {
        let grads = mixed_batch_gradients();
        // Per COMPONENT, not per tensor: the key biases are analytically zero,
        // so "every tensor has a non-zero gradient" is unsatisfiable against a
        // correct implementation (01-03 T-1-16).
        let component = |name: &str| -> String {
            if name.starts_with("embeddings") {
                return "embeddings".to_string();
            }
            let layer = name.split('.').nth(2).unwrap_or("?").to_string();
            if name.contains(".attention.self.") || name.contains(".attention.output.dense") {
                format!("layer{layer}.attention")
            } else if name.contains("LayerNorm") {
                format!("layer{layer}.norm")
            } else {
                format!("layer{layer}.ffn")
            }
        };

        let mut components: Vec<String> = grads.iter().map(|(n, _)| component(n)).collect();
        components.sort();
        components.dedup();
        assert_eq!(
            components.len(),
            1 + 2 * 3,
            "expected embeddings + {{attention, ffn, norm}} x 2 layers, got {components:?}"
        );

        for c in &components {
            let acc: f64 = grads
                .iter()
                .filter(|(n, _)| component(n) == *c)
                .flat_map(|(_, g)| g.iter())
                .map(|v| f64::from(*v) * f64::from(*v))
                .sum();
            assert!(
                acc.sqrt() > 1e-9,
                "component `{c}` aggregate gradient L2 is {:e} — gradient is not reaching it",
                acc.sqrt()
            );
        }
    }

    #[test]
    fn encoder_key_biases_are_near_zero_while_the_other_biases_carry_real_gradient() {
        let grads = mixed_batch_gradients();
        let zero_names = analytically_zero();
        assert_eq!(zero_names.len(), 2, "one key bias per layer");

        let mut checked = 0;
        for (name, g) in &grads {
            if !zero_names.contains(name) {
                continue;
            }
            checked += 1;
            for (i, v) in g.iter().enumerate() {
                assert!(
                    v.abs() <= 1e-5,
                    "`{name}`[{i}] = {v:e}: the key bias is analytically zero by softmax \
                     shift invariance. A value this large means the constant shift is NOT \
                     cancelling — suspect the mask, the softmax, or the head-axis broadcast."
                );
            }
        }
        assert_eq!(checked, 2);

        // SECOND SIDE. Near-zero is only evidence if the other biases are not:
        // a backward returning zeros for every bias would sail through above.
        let l2_of = |suffix: &str| -> f64 {
            grads
                .iter()
                .filter(|(n, _)| n.ends_with(suffix))
                .map(|(_, g)| l2(g))
                .sum()
        };
        let k = l2_of("attention.self.key.bias");
        let q = l2_of("attention.self.query.bias");
        let v = l2_of("attention.self.value.bias");
        assert!(
            q > 1e-4 && v > 1e-4,
            "query ({q:e}) and value ({v:e}) biases must carry REAL gradient, else the \
             key-bias assertion is vacuous"
        );
        assert!(
            q > k * 1e3,
            "the key bias ({k:e}) must be orders below the query bias ({q:e})"
        );
    }
}
