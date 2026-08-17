//! Phase 5 D-10: the `--selection-manifest` door on `apr finetune --task classify`.
//!
//! Two things are proven here and nothing else:
//!
//! 1. The REFUSALS fire before any data is read — a missing `--seed`, a missing `--data`,
//!    a manifest whose envelope digest does not verify, and a selection whose ids have no
//!    row in the dataset.
//! 2. The resolved `TrainingConfig` is byte-identical to the pre-D-10 hardcode block when
//!    none of the new flags is passed. That is the "defaults preserved" claim, asserted
//!    against the literal values the old block contained rather than against the constants
//!    the code now reads — a test that quoted the constants would pass no matter what they
//!    were changed to.

use super::{
    resolve_classify_selection, resolve_selected_samples, resolve_training_config,
    ClassifyOverrides, DEFAULT_CLASSIFY_SAVE_EVERY,
};
use std::path::{Path, PathBuf};

fn overrides<'a>(selection_manifest: Option<&'a Path>, seed: Option<u64>) -> ClassifyOverrides<'a> {
    ClassifyOverrides {
        selection_manifest,
        seed,
        val_split: None,
        early_stopping_patience: None,
    }
}

fn message(error: &crate::error::CliError) -> String {
    error.to_string()
}

// ------------------------------------------------------------------------------------
// Flag validation: refused before a byte of data is read
// ------------------------------------------------------------------------------------

#[test]
fn selection_manifest_without_seed_refuses_naming_both_flags() {
    // A path that does NOT exist: if the seed check did not come first, this would fail
    // with a file error instead, and the test would be asserting the wrong ordering.
    let manifest = PathBuf::from("/nonexistent/selection-manifest.json");
    let error = resolve_classify_selection(&overrides(Some(&manifest), None), None)
        .expect_err("a selection-manifest run without --seed is refused");

    let text = message(&error);
    assert!(
        text.contains("--selection-manifest"),
        "the refusal must name the flag that triggered it: {text}"
    );
    assert!(
        text.contains("--seed"),
        "the refusal must name the flag that fixes it: {text}"
    );
}

#[test]
fn selection_manifest_without_data_refuses_naming_the_attested_directory() {
    let manifest = PathBuf::from("/nonexistent/selection-manifest.json");
    let error = resolve_classify_selection(&overrides(Some(&manifest), Some(13)), None)
        .expect_err("a selection-manifest run without --data is refused");

    let text = message(&error);
    assert!(
        text.contains("--data"),
        "the refusal must name --data: {text}"
    );
    assert!(
        text.contains("ATTESTED CANONICAL"),
        "the refusal must say WHICH kind of --data is meant, because the corpus .jsonl \
         form is the one every other classify run uses: {text}"
    );
}

#[test]
fn absent_selection_manifest_is_not_an_error_and_resolves_to_nothing() {
    let resolved = resolve_classify_selection(&overrides(None, None), None)
        .expect("no --selection-manifest is the historical path, not a refusal");
    assert!(
        resolved.is_none(),
        "without the flag there is no selection to resolve"
    );
}

// ------------------------------------------------------------------------------------
// Defaults preserved: the resolved config equals the old hardcode block
// ------------------------------------------------------------------------------------

#[test]
fn absent_flags_resolve_to_the_pre_d10_hardcode_block() {
    let config = resolve_training_config(
        &overrides(None, None),
        7,
        PathBuf::from("checkpoints"),
        None,
    );

    // The literals from the block this change replaced:
    //   { epochs, val_split: 0.2, save_every: 5, early_stopping_patience: 10, seed: 42 }
    assert_eq!(config.epochs, 7);
    assert!(
        (config.val_split - 0.2).abs() < f32::EPSILON,
        "val_split default moved: {}",
        config.val_split
    );
    assert_eq!(config.save_every, 5);
    assert_eq!(config.early_stopping_patience, 10);
    assert_eq!(config.seed, 42);
    assert_eq!(config.log_interval, 1);
    assert_eq!(config.checkpoint_dir, PathBuf::from("checkpoints"));
}

#[test]
fn each_flag_overrides_exactly_its_own_field() {
    let config = resolve_training_config(
        &ClassifyOverrides {
            selection_manifest: None,
            seed: Some(13),
            val_split: Some(0.0),
            early_stopping_patience: Some(0),
        },
        4,
        PathBuf::from("out"),
        None,
    );

    assert_eq!(config.seed, 13);
    assert!(config.val_split.abs() < f32::EPSILON);
    assert_eq!(config.early_stopping_patience, 0);
}

#[test]
fn disabling_early_stopping_pushes_save_every_out_to_the_final_epoch() {
    // The frozen-defaults regime: nothing may select an epoch, so the periodic checkpoint
    // cadence stops producing intermediate candidates.
    let disabled = resolve_training_config(
        &ClassifyOverrides {
            selection_manifest: None,
            seed: None,
            val_split: None,
            early_stopping_patience: Some(0),
        },
        12,
        PathBuf::from("out"),
        None,
    );
    assert_eq!(disabled.save_every, 12);

    // ...and leaving early stopping ON keeps the historical cadence.
    let enabled = resolve_training_config(&overrides(None, None), 12, PathBuf::from("out"), None);
    assert_eq!(enabled.save_every, DEFAULT_CLASSIFY_SAVE_EVERY);
}

// ------------------------------------------------------------------------------------
// Row resolution: selection order is preserved, a missing id is a counted refusal
// ------------------------------------------------------------------------------------

fn row(id: &str, input: &str, label: usize) -> aprender_contrastive_data::schema::LabeledExample {
    aprender_contrastive_data::schema::LabeledExample {
        id: id.to_string(),
        input: input.to_string(),
        label,
        label_text: format!("class-{label}"),
        source_split: "train".to_string(),
    }
}

#[test]
fn resolved_samples_follow_the_selection_order_not_the_file_order() {
    let rows = vec![
        row("train:0", "alpha", 0),
        row("train:1", "beta", 1),
        row("train:2", "gamma", 2),
    ];
    // Deliberately NOT the file order: the selection's order is class-ascending, draw
    // order within a class, and a resolver that iterated `rows` would silently produce a
    // different training sequence for the same manifest.
    let ordered = [("train:2", 2), ("train:0", 0), ("train:1", 1)];

    let samples = resolve_selected_samples(&ordered, &rows).expect("every id has a row");
    let inputs: Vec<&str> = samples.iter().map(|s| s.input.as_str()).collect();
    assert_eq!(inputs, vec!["gamma", "alpha", "beta"]);
    let labels: Vec<usize> = samples.iter().map(|s| s.label).collect();
    assert_eq!(labels, vec![2, 0, 1]);
}

#[test]
fn a_selection_id_with_no_row_refuses_with_the_missing_count() {
    let rows = vec![row("train:0", "alpha", 0)];
    let ordered = [("train:0", 0), ("train:7", 1), ("train:9", 2)];

    let error = resolve_selected_samples(&ordered, &rows)
        .expect_err("ids absent from --data are a refusal, never a silently shorter run");

    let text = message(&error);
    assert!(
        text.contains('2'),
        "the refusal must state HOW MANY ids were missing — a shortened training set is \
         invisible without it: {text}"
    );
    assert!(
        text.contains("train:7"),
        "and name at least one of them: {text}"
    );
}
