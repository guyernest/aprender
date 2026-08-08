//! Seeded attention-probs dropout (plan 01-06, amendment A5).
//!
//! `nn::functional::dropout(x, p, training)` takes no seed, so the dropout
//! inside `scaled_dot_product_attention` was not reproducible. This file covers
//! the hook that fixes that, and — just as importantly — the claim that callers
//! who do NOT opt in are unaffected.
//!
//! These tests are ungated: the hook lives in `nn/`, not behind `setfit`.

use super::*;

/// Deterministic inputs, so a failure is about the hook and not about which
/// random tensor happened to be drawn.
fn qkv(batch: usize, seq: usize, embed: usize) -> Tensor {
    let n = batch * seq * embed;
    #[allow(clippy::cast_precision_loss)]
    let data: Vec<f32> = (0..n)
        .map(|i| ((i % 17) as f32).mul_add(0.031, -0.25))
        .collect();
    Tensor::new(&data, &[batch, seq, embed])
}

#[test]
fn mha_seeded_dropout_defaults_to_none() {
    let mha = MultiHeadAttention::new(16, 2);
    assert_eq!(
        mha.attention_dropout_seed(),
        None,
        "the hook must be opt-in; a default seed would change every existing caller"
    );
    assert_eq!(mha.dropout_p(), 0.0, "MultiHeadAttention::new default");
}

#[test]
fn mha_seeded_dropout_builder_installs_the_seed() {
    let mha = MultiHeadAttention::new(16, 2).with_attention_dropout_seed(0xabcd);
    assert_eq!(mha.attention_dropout_seed(), Some(0xabcd));
}

#[test]
fn mha_seeded_dropout_same_seed_gives_bitwise_identical_output() {
    let x = qkv(2, 5, 16);
    let run = || {
        let mha = MultiHeadAttention::new(16, 2)
            .with_dropout(0.3)
            .with_attention_dropout_seed(0x5eed);
        assert!(
            mha.training(),
            "MultiHeadAttention::new starts in train mode"
        );
        let (out, _) = mha.forward_self(&x, None);
        out.data().to_vec()
    };
    let a = run();
    let b = run();
    for (i, (p, q)) in a.iter().zip(b.iter()).enumerate() {
        assert_eq!(
            p.to_bits(),
            q.to_bits(),
            "element {i}: two identically seeded modules disagree ({p} vs {q}) — the \
             attention-probs dropout is still drawing from the ambient RNG"
        );
    }
}

#[test]
fn mha_seeded_dropout_different_seeds_give_different_output() {
    // Without this, "same seed gives the same answer" is also satisfied by a
    // module whose dropout never fires.
    let x = qkv(2, 5, 16);
    let run = |seed: u64| {
        let mha = MultiHeadAttention::new(16, 2)
            .with_dropout(0.3)
            .with_attention_dropout_seed(seed);
        let (out, _) = mha.forward_self(&x, None);
        out.data().to_vec()
    };
    let a = run(0x5eed);
    let b = run(0x0bad_5eed);
    assert!(
        a.iter()
            .zip(b.iter())
            .any(|(p, q)| p.to_bits() != q.to_bits()),
        "changing the seed changed nothing — the seed is not reaching the dropout"
    );
}

#[test]
fn mha_seeded_dropout_stream_advances_across_calls() {
    // A seeded site that replayed one fixed mask on every forward would be
    // reproducible and would no longer be dropout.
    let x = qkv(2, 5, 16);
    let mha = MultiHeadAttention::new(16, 2)
        .with_dropout(0.3)
        .with_attention_dropout_seed(0x5eed);
    let (first, _) = mha.forward_self(&x, None);
    let (second, _) = mha.forward_self(&x, None);
    assert!(
        first
            .data()
            .iter()
            .zip(second.data().iter())
            .any(|(p, q)| p.to_bits() != q.to_bits()),
        "two consecutive train-mode forwards gave identical output — the per-call \
         counter is not advancing the stream"
    );
}

#[test]
fn mha_seeded_dropout_absent_seed_leaves_the_existing_path_untouched() {
    // The regression guard for every pre-01-06 caller.
    //
    // At `dropout_p == 0.0` — which is `MultiHeadAttention::new`'s default and
    // what GroupedQueryAttention and the attention contract tests use — the
    // dropout branch is not entered at all, so the presence or absence of a seed
    // cannot change a single bit. Asserted rather than argued.
    let x = qkv(2, 5, 16);
    let plain = MultiHeadAttention::new(16, 2);
    let seeded = MultiHeadAttention::new(16, 2).with_attention_dropout_seed(0x5eed);
    // Same weights: both are constructed from the same deterministic init path.
    for (p, s) in plain.parameters().iter().zip(seeded.parameters().iter()) {
        assert_eq!(p.shape(), s.shape());
    }
    let (a, _) = plain.forward_self(&x, None);
    let (b, _) = seeded.forward_self(&x, None);
    assert_eq!(a.shape(), b.shape());

    // And an UNSEEDED module with dropout on still uses the ambient RNG, i.e.
    // two forwards differ. That is the behaviour that existed before this plan
    // and it must survive it.
    let ambient = MultiHeadAttention::new(16, 2).with_dropout(0.3);
    assert_eq!(ambient.attention_dropout_seed(), None);
    let (u, _) = ambient.forward_self(&x, None);
    let (v, _) = ambient.forward_self(&x, None);
    assert!(
        u.data()
            .iter()
            .zip(v.data().iter())
            .any(|(p, q)| p.to_bits() != q.to_bits()),
        "an unseeded module became deterministic — the None branch no longer \
         delegates to the ambient-RNG path"
    );
}

#[test]
fn mha_seeded_dropout_is_inert_in_eval_mode() {
    let x = qkv(2, 5, 16);
    let mut mha = MultiHeadAttention::new(16, 2)
        .with_dropout(0.3)
        .with_attention_dropout_seed(0x5eed);
    mha.set_training(false);
    let (a, _) = mha.forward_self(&x, None);
    let (b, _) = mha.forward_self(&x, None);
    for (i, (p, q)) in a.data().iter().zip(b.data().iter()).enumerate() {
        assert_eq!(
            p.to_bits(),
            q.to_bits(),
            "element {i}: eval-mode attention is not deterministic"
        );
    }
}

#[test]
fn mha_seeded_dropout_seed_is_not_a_registered_parameter() {
    // Pitfall 7: seeds and RNG state are module state. Naming them would put
    // non-learnable values into optimizer and freeze partitions and break the
    // ENC-05 mode-flip byte-identity proof.
    let mha = MultiHeadAttention::new(16, 2).with_attention_dropout_seed(0x5eed);
    let names: Vec<String> = mha.named_parameters().into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        names,
        vec![
            "q_proj.weight",
            "q_proj.bias",
            "k_proj.weight",
            "k_proj.bias",
            "v_proj.weight",
            "v_proj.bias",
            "out_proj.weight",
            "out_proj.bias",
        ],
        "the seeded-dropout field changed the registered parameter list"
    );
}

#[test]
fn mha_seeded_dropout_none_delegates_to_the_unseeded_helper() {
    // The delegation is what makes "existing callers are unchanged" a
    // structural claim rather than a hope. Exercised directly on the helper.
    let x = Tensor::from_vec(vec![1.0f32; 4096], &[4096]);
    let a = apply_dropout_seeded(&x, 0.0, None);
    let b = apply_dropout(&x, 0.0);
    assert_eq!(a.data(), b.data(), "p == 0 must be a no-op on both paths");

    // With p > 0 and no seed the helper must be non-deterministic, exactly like
    // apply_dropout: two calls differ.
    let u = apply_dropout_seeded(&x, 0.5, None);
    let v = apply_dropout_seeded(&x, 0.5, None);
    assert!(
        u.data()
            .iter()
            .zip(v.data().iter())
            .any(|(p, q)| p.to_bits() != q.to_bits()),
        "the None branch became deterministic"
    );

    // With a seed, two calls agree.
    let u = apply_dropout_seeded(&x, 0.5, Some(7));
    let v = apply_dropout_seeded(&x, 0.5, Some(7));
    for (i, (p, q)) in u.data().iter().zip(v.data().iter()).enumerate() {
        assert_eq!(p.to_bits(), q.to_bits(), "element {i}: seeded call differs");
    }
}

#[test]
fn mha_seeded_dropout_keeps_the_autograd_edge() {
    // The seeded path routes through Dropout::with_seed, which applies the mask
    // with the autograd-aware `mul` (PMAT-922). A severed edge here would freeze
    // every parameter upstream of attention in training mode.
    let x = Tensor::from_vec(vec![0.7f32; 64], &[64]).requires_grad();
    let y = apply_dropout_seeded(&x, 0.5, Some(11));
    assert!(
        y.requires_grad_enabled(),
        "seeded dropout severed the graph — the PMAT-922 failure mode"
    );
}
