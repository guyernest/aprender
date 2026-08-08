//! MODEL A — the ALL-TRAINABLE gradient and controlled-step gate (ENC-04).
//!
//! Obligations: `OBLIG-ENC-04-NAMED-GRADIENT-PARITY`,
//! `OBLIG-ENC-04-GRADIENT-AND-STEP-GATE`,
//! `OBLIG-ENC-04-POST-STEP-PARAMETER-PARITY`.
//!
//! # Why this file is all-trainable and the frozen proof lives elsewhere
//!
//! `optimizer_step.json` records a step in which EVERY parameter was trainable
//! (`all_trainable: true`, SetFit's full-body fine-tuning default, D-20). A Rust
//! model carrying a freeze policy cannot match it, so one test state cannot
//! prove both outcomes. This file asserts `freeze_policy()` is EMPTY and
//! compares against the fixture; `frozen_gate.rs` uses a separate clean model
//! and proves a purely internal property with no Python reference.
//!
//! # D42 — where the optimizer's parameter set comes from
//!
//! From `SetFitMiniLm::trainable_parameters_mut()`, via [`super::trainable_grads`].
//! `requires_grad(false)` does NOT on its own stop a parameter receiving a
//! gradient or moving under a step — 01-07 measured exactly that. Exclusion from
//! that method is the mechanism. Nothing in this file may build the parameter
//! set any other way, and no parity gate here could detect it if it did.

use aprender::autograd::{self, Tensor};
use aprender::nn::{AdamW, Module};
use aprender::setfit::{pair_cosine_mse, SetFitMiniLm};

use super::{
    assert_close, encode, max_abs, pair_batch, read_fixture, slice_model, snapshot, tol,
    trainable_grads, GateInput, GradientsFixture, OptimizerStepFixture,
};

/// Model A, freshly constructed and asserted all-trainable.
fn model_a() -> SetFitMiniLm {
    let m = slice_model();
    assert!(
        m.freeze_policy().is_empty(),
        "Model A must be all-trainable to match optimizer_step.json's recorded configuration"
    );
    m
}

#[test]
fn gradient_gate_named_gradients_match_the_frozen_reference() {
    autograd::clear_graph();
    let mut model = model_a();
    let g: GradientsFixture = read_fixture("gradients.json");
    let pair = pair_batch(&model, &g.source);

    // Name coverage is TOTAL: a name in the reference and absent from the Rust
    // traversal is a failure, not a skip.
    let names: Vec<String> = model
        .encoder()
        .named_parameters()
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(
        names, g.parameter_order,
        "the Rust named traversal disagrees with gradients.json.parameter_order — in \
         CONTENT or in ORDER; parameter_order is an ordered array, not object keys"
    );
    assert_eq!(
        g.grads.len(),
        names.len(),
        "gradients.json records {} tensors but the encoder names {}",
        g.grads.len(),
        names.len()
    );

    let za = encode(&model, &pair.a);
    let zb = encode(&model, &pair.b);
    let loss = pair_cosine_mse(&za, &zb, &pair.labels).expect("pair objective");
    loss.backward();

    let rust = trainable_grads(&mut model);
    for (name, fixture) in &g.grads {
        let (_, actual) = rust.iter().find(|(n, _)| n == name).unwrap_or_else(|| {
            panic!("`{name}` is in gradients.json but not in the Rust traversal")
        });
        let actual = actual
            .as_ref()
            .unwrap_or_else(|| panic!("`{name}` received NO gradient"));
        assert_eq!(
            actual.len(),
            fixture.grad.len(),
            "`{name}`: gradient has {} elements, fixture shape {:?} has {}",
            actual.len(),
            fixture.shape,
            fixture.grad.len()
        );
        assert_close(
            actual,
            &fixture.grad,
            tol::GRADIENTS,
            &format!("grad `{name}`"),
        );
    }
}

#[test]
fn gradient_gate_enc04_holds_on_an_all_trainable_model() {
    autograd::clear_graph();
    let mut model = model_a();
    let g: GradientsFixture = read_fixture("gradients.json");
    let pair = pair_batch(&model, &g.source);
    let layers = model.num_layers();

    let za = encode(&model, &pair.a);
    let zb = encode(&model, &pair.b);
    let loss = pair_cosine_mse(&za, &zb, &pair.labels).expect("pair objective");
    loss.backward();

    let grads = trainable_grads(&mut model);
    let exemptions = g.exempt_names();
    assert!(
        !exemptions.is_empty(),
        "the analytically-zero list is empty, so clause (e) would be vacuous"
    );

    super::assert_encoder_updates(&GateInput {
        grads: &grads,
        deltas: None,
        exemptions: &exemptions,
        floor: tol::ZERO_GRAD_FLOOR,
        layers,
    })
    .expect("the ENC-04 gate must hold on a correct all-trainable model");
}

#[test]
fn gradient_gate_the_exemption_is_two_sided_against_real_measurements() {
    // Clause (e) is only meaningful if the exempt tensors really are near zero
    // AND their non-exempt neighbours really are not. Asserting one side alone
    // would also be satisfied by a backward that returned zeros for every bias.
    autograd::clear_graph();
    let mut model = model_a();
    let g: GradientsFixture = read_fixture("gradients.json");
    let pair = pair_batch(&model, &g.source);

    let za = encode(&model, &pair.a);
    let zb = encode(&model, &pair.b);
    let loss = pair_cosine_mse(&za, &zb, &pair.labels).expect("pair objective");
    loss.backward();
    let grads = trainable_grads(&mut model);

    let lookup = |name: &str| -> Vec<f32> {
        grads
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("`{name}` received no gradient"))
    };

    for entry in &g.analytically_zero {
        let m = max_abs(&lookup(&entry.name));
        assert!(
            m <= g.zero_grad_floor,
            "`{}` is predicted analytically zero ({}) but measured max|g| = {m:e}",
            entry.name,
            entry.justification
        );
        // The other side: the sibling query bias on the same layer must carry a
        // real gradient, otherwise "near zero" is a property of the backward
        // rather than of the key bias.
        let sibling = entry.name.replace("key.bias", "query.bias");
        assert_ne!(sibling, entry.name, "exemption naming changed shape");
        let sm = max_abs(&lookup(&sibling));
        assert!(
            sm > g.zero_grad_floor,
            "`{sibling}` also measures below the zero-grad floor ({sm:e}); the exemption \
             would then be describing the backward, not the key bias"
        );
    }
}

#[test]
fn gradient_gate_controlled_adamw_step_matches_the_frozen_reference() {
    autograd::clear_graph();
    let mut model = model_a();
    let g: GradientsFixture = read_fixture("gradients.json");
    let o: OptimizerStepFixture = read_fixture("optimizer_step.json");
    assert!(
        o.all_trainable,
        "optimizer_step.json no longer describes an all-trainable step; this gate's model \
         configuration would no longer match it"
    );
    let pair = pair_batch(&model, &o.source);
    let layers = model.num_layers();

    // Hyperparameter agreement, asserted before anything is configured from it.
    assert_eq!(o.adamw.betas.len(), 2, "adamw.betas must be a pair");
    let (lr, b1, b2, eps, wd) = (
        o.adamw.lr,
        o.adamw.betas[0],
        o.adamw.betas[1],
        o.adamw.eps,
        o.adamw.weight_decay,
    );
    assert!(
        wd > 0.0,
        "weight_decay is {wd}; with decay disabled this step would not exercise AdamW's \
         DECOUPLED decay at all, which is the half that distinguishes it from Adam"
    );

    let before = snapshot(&model);

    let za = encode(&model, &pair.a);
    let zb = encode(&model, &pair.b);
    let loss = pair_cosine_mse(&za, &zb, &pair.labels).expect("pair objective");
    assert_close(
        &[loss.item()],
        &[o.loss_before],
        tol::LOSS_PAIR,
        "loss_before",
    );
    loss.backward();

    // Every hyperparameter is sourced from the fixture. Builder defaults happen
    // to agree today; relying on them would make this gate silently drift the
    // day a default changes, and the fixture is the arbiter.
    let mut opt = {
        let params: Vec<&mut Tensor> = model
            .trainable_parameters_mut()
            .into_iter()
            .map(|(_, t)| t)
            .collect();
        AdamW::new(params, lr)
            .betas(b1, b2)
            .eps(eps)
            .weight_decay(wd)
    };
    {
        let mut named = model.trainable_parameters_mut();
        let mut refs: Vec<&mut Tensor> = named.iter_mut().map(|(_, t)| &mut **t).collect();
        opt.step_with_params(&mut refs);
    }

    let after = snapshot(&model);

    // Post-step parity, every named parameter, including the weight-decayed
    // ones. If this ever fails specifically on decay semantics the fix is in
    // rm_sprop.rs or a documented divergence — never a widened tolerance.
    assert_eq!(o.post_step.len(), after.len());
    for (name, bits) in &after {
        let expected = o
            .post_step
            .get(name)
            .unwrap_or_else(|| panic!("`{name}` is missing from optimizer_step.json.post_step"));
        let values: Vec<f32> = bits.iter().map(|b| f32::from_bits(*b)).collect();
        assert_close(
            &values,
            expected,
            tol::OPTIMIZER_STEP,
            &format!("post-step `{name}`"),
        );
    }

    // (f): the gate's post-step half, over the SAME helper.
    let deltas: Vec<(String, f32)> = before
        .iter()
        .map(|(name, b)| {
            let a = &after
                .iter()
                .find(|(n, _)| n == name)
                .expect("name present in both snapshots")
                .1;
            let d = b
                .iter()
                .zip(a.iter())
                .map(|(x, y)| (f32::from_bits(*x) - f32::from_bits(*y)).abs())
                .fold(0.0f32, f32::max);
            (name.clone(), d)
        })
        .collect();
    let grads = trainable_grads(&mut model);
    super::assert_encoder_updates(&GateInput {
        grads: &grads,
        deltas: Some(&deltas),
        exemptions: &g.exempt_names(),
        floor: tol::ZERO_GRAD_FLOOR,
        layers,
    })
    .expect("the ENC-04 gate must hold across the controlled step");

    // Loss decreases, to the RECORDED value. "loss went down" alone compares
    // against an unpinned expectation a broken optimizer can still satisfy.
    autograd::clear_graph();
    let za2 = encode(&model, &pair.a);
    let zb2 = encode(&model, &pair.b);
    let loss2 = pair_cosine_mse(&za2, &zb2, &pair.labels).expect("pair objective after step");
    assert_close(
        &[loss2.item()],
        &[o.loss_after],
        tol::LOSS_PAIR,
        "loss_after",
    );
    assert!(
        o.loss_after < o.loss_before,
        "the fixture itself does not record a decreasing loss"
    );
    assert!(
        loss2.item() < o.loss_before,
        "loss did not decrease across the controlled step: {} -> {}",
        o.loss_before,
        loss2.item()
    );
}
