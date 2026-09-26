//! Laya's calibrated temperature buckets and the tempered softmax.
//!
//! Port of `laya/common.py` `temp_bucket` + `clamp_temperature` and Laya's lookup
//! (`agent.py`: `temperature_by_options.get(temp_bucket(qt, k), temperature[qt])`).
//! The clamp bounds mirror `laya-finetune-gate-v1` `calibration_temp_min` /
//! `calibration_temp_max` (a test reads them from the YAML): a fitted temperature
//! below 0.5 sharpens logits instead of softening them and is never applied.

use super::{AgentConfig, QType};

/// Lower clamp bound (Laya `TEMP_MIN`).
pub const TEMP_MIN: f64 = 0.5;
/// Upper clamp bound (Laya `TEMP_MAX`).
pub const TEMP_MAX: f64 = 5.0;

/// Laya's option-count bucket: `"{qtype}:2"`, `":3-5"`, `":6-10"` or `":11+"`.
#[must_use]
pub fn bucket_key(qtype: QType, k: usize) -> String {
    let size = match k {
        0..=2 => "2",
        3..=5 => "3-5",
        6..=10 => "6-10",
        _ => "11+",
    };
    format!("{}:{size}", qtype.name())
}

/// A usable temperature: `t` confined to `[TEMP_MIN, TEMP_MAX]`, or 1.0 when it is
/// not finite (Laya `clamp_temperature`).
#[must_use]
pub fn clamp_temperature(t: f64) -> f64 {
    if t.is_finite() {
        t.clamp(TEMP_MIN, TEMP_MAX)
    } else {
        1.0
    }
}

/// The temperature Laya applies to a `qtype` question with `k` options:
/// `temperature_by_options[bucket]`, else `temperature[qtype]`, else 1.0 — clamped.
#[must_use]
pub fn temperature_for(agent: &AgentConfig, qtype: QType, k: usize) -> f32 {
    let t = agent
        .temperature_by_options
        .get(&bucket_key(qtype, k))
        .or_else(|| agent.temperature.get(qtype.index()))
        .copied()
        .unwrap_or(1.0);
    // Laya divides an f32 logit tensor by this Python float; the division runs in f32.
    clamp_temperature(t) as f32
}

/// `softmax(z / t)` with the spike's f64 accumulation (max-shifted in f32).
#[must_use]
pub fn softmax_t(z: &[f32], t: f32) -> Vec<f32> {
    let m = z.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b / t));
    let e: Vec<f64> = z.iter().map(|&v| f64::from(v / t - m).exp()).collect();
    let s: f64 = e.iter().sum();
    e.iter().map(|v| (v / s) as f32).collect()
}
