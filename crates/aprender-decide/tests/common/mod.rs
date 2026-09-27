//! Shared plumbing for the env-gated integration targets: the verify policy read from the
//! contracts exactly as `examples/pack_laya.rs` reads it (never a literal).
//!
//! Each target compiles this module on its own and uses a subset of it.
#![allow(dead_code)]

use aprender_decide::verify::VerifyPolicy;
use std::path::{Path, PathBuf};

/// The workspace root (from this crate's manifest dir, never the environment).
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `contracts/<name>` parsed.
pub fn contract(name: &str) -> serde_yaml::Value {
    let path = workspace_root().join("contracts").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {name}: {e}"));
    serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("parse {name}: {e}"))
}

fn at<'a>(v: &'a serde_yaml::Value, keys: &[&str]) -> &'a serde_yaml::Value {
    keys.iter().fold(v, |v, k| &v[*k])
}

/// A number at `keys`.
pub fn f64_at(v: &serde_yaml::Value, keys: &[&str]) -> f64 {
    at(v, keys)
        .as_f64()
        .unwrap_or_else(|| panic!("contract value {}", keys.join(".")))
}

/// A string at `keys`.
pub fn str_at(v: &serde_yaml::Value, keys: &[&str]) -> String {
    at(v, keys)
        .as_str()
        .unwrap_or_else(|| panic!("contract value {}", keys.join(".")))
        .to_string()
}

/// The contract policy: laya-finetune-gate-v1 constants, base and seed policy; laya-parity-v1
/// floor, noise multiplier and ceiling.
pub fn policy() -> VerifyPolicy {
    let gate = contract("laya-finetune-gate-v1.yaml");
    let parity = contract("laya-parity-v1.yaml");
    let c = |k: &'static str| ["constants", k];
    VerifyPolicy {
        min_macro_f1_margin: f64_at(&gate, &c("gate_min_macro_f1_margin")),
        max_ece: f64_at(&gate, &c("gate_max_ece")),
        ece_bins: f64_at(&gate, &c("ece_bins")) as u64,
        metric_recompute_abs: f64_at(&gate, &c("gate_metric_recompute_abs")),
        rescore_probs_abs: f64_at(
            &parity,
            &["equations", "pack_rescore_probs_abs", "float_tolerance"],
        ),
        rescore_noise_k: f64_at(&parity, &c("pack_rescore_noise_k")),
        rescore_bound_max_abs: f64_at(&parity, &c("pack_rescore_bound_max_abs")),
        calibration_slice_min_per_class: f64_at(&gate, &c("calibration_slice_min_per_class"))
            as u64,
        base_sha256: str_at(&gate, &["base", "model_safetensors_sha256"]),
    }
}
