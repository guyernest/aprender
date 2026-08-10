//! The frozen evidence-gate thresholds — THE single Rust source, tied to the contract.
//!
//! Every number here is measured, not chosen. `contracts/setfit-train-lifecycle-v1.yaml`
//! records the derivation; this module records the values the gate actually compares against,
//! and [`tests::thresholds_match_the_contract`] PARSES that contract and asserts typed
//! equality field by field. Editing either side alone turns that test red, which is what makes
//! loosening an epsilon after a failing comparison require a contract edit `pv diff` flags
//! (T-3-21) rather than a one-line change nobody reviews.
//!
//! # Why a parse and not a substring search
//!
//! A `grep` for `1.1e-5` in the YAML passes whether the number sits under `embedding` or under
//! `projection_weight`, and passes just as happily if it appears only in a prose paragraph.
//! `aprender-train` already depends on `serde_yaml`, so the test deserializes the contract into
//! typed structs and compares per class. A number in the wrong slot is red.
//!
//! # One class carries no epsilon, and that is the point
//!
//! `attention_key_bias` has `eps: None` and `gated: false`. Its gradient is analytically zero
//! by softmax shift-invariance, so its movement is f32 cancellation residue and cannot testify
//! that tuning occurred — in either direction. See the `gradient_free_parameters` equation of
//! the contract for the mechanism and the measurements. It is RECORDED in the evidence table
//! and excluded from the verdict.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::evidence::ParameterClass;

/// The contract this module is frozen against, embedded at compile time.
///
/// `include_str!` rather than a runtime read: a test that silently skips when a file is absent
/// is a test that proves nothing, and a path that resolves differently under `cargo test` and
/// under the packaged crate is a defect waiting for a release.
pub(crate) const CONTRACT_YAML: &str =
    include_str!("../../../../../contracts/setfit-train-lifecycle-v1.yaml");

/// The calibration regimes these thresholds were MEASURED in.
///
/// Exactly one entry. A run whose recorded `calibration_regime_id` is not in this set is
/// refused with `UncalibratedRegime` BEFORE any threshold below is applied to it — an epsilon
/// measured on a 2-layer/64-hidden/97-vocab slice is not evidence about a
/// 6-layer/384-hidden/30522-vocab model.
///
/// Extending this set requires a calibration run on the target encoder AND a deliberate
/// contract edit (D-10(c)). It is not something a downstream executor may widen inline to
/// unblock a benchmark.
pub(crate) const CALIBRATED_REGIMES: &[&str] =
    &["minilm-slice-h64-l2-a2-i256-v97@1110a243|seeds=1,42,7|cells=s16e2b8,s8e1b4"];

/// One class's frozen entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClassThreshold {
    /// The frozen epsilon, or `None` for a class that carries no threshold.
    ///
    /// `None` is not "zero" and not "not yet decided". It means the class was measured and
    /// found unable to support one; see [`Self::gated`].
    pub(crate) eps: Option<f64>,
    /// The positive scale floor `s_class` applied to the denominator.
    pub(crate) scale_floor: f64,
    /// Whether the denominator is restricted to the delta's support.
    pub(crate) sparse: bool,
    /// Whether members of this class contribute to the verdict at all.
    pub(crate) gated: bool,
}

/// The complete frozen table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Thresholds {
    /// Per class, in `ParameterClass::ALL` order.
    classes: BTreeMap<&'static str, ClassThreshold>,
    /// The run-level floor on the sparse class's MEDIAN relative delta.
    embedding_delta_floor: f64,
    /// The regimes these values were measured in.
    calibrated_regimes: &'static [&'static str],
}

impl Thresholds {
    /// The frozen table, as committed to `setfit-train-lifecycle-v1.yaml`.
    #[must_use]
    pub(crate) fn frozen() -> Self {
        let mut classes = BTreeMap::new();
        classes.insert(
            ParameterClass::Embedding.tag(),
            ClassThreshold { eps: Some(1.1e-5), scale_floor: 1.0, sparse: true, gated: true },
        );
        classes.insert(
            ParameterClass::LayerNormWeight.tag(),
            ClassThreshold { eps: Some(2.9e-6), scale_floor: 1.0, sparse: false, gated: true },
        );
        classes.insert(
            ParameterClass::LayerNormBias.tag(),
            ClassThreshold { eps: Some(1.1e-5), scale_floor: 1.0, sparse: false, gated: true },
        );
        classes.insert(
            ParameterClass::ProjectionWeight.tag(),
            ClassThreshold { eps: Some(1.8e-5), scale_floor: 1.0, sparse: false, gated: true },
        );
        classes.insert(
            ParameterClass::ProjectionBias.tag(),
            ClassThreshold { eps: Some(8.3e-6), scale_floor: 1.0, sparse: false, gated: true },
        );
        classes.insert(
            ParameterClass::AttentionKeyBias.tag(),
            ClassThreshold { eps: None, scale_floor: 1.0, sparse: false, gated: false },
        );
        Self { classes, embedding_delta_floor: 2.7e-5, calibrated_regimes: CALIBRATED_REGIMES }
    }

    /// The entry for a class. Total over [`ParameterClass::ALL`] by construction.
    #[must_use]
    pub(crate) fn of(&self, class: ParameterClass) -> ClassThreshold {
        self.classes.get(class.tag()).copied().unwrap_or(ClassThreshold {
            eps: None,
            scale_floor: 1.0,
            sparse: false,
            gated: false,
        })
    }

    /// The run-level sparse-class floor.
    #[must_use]
    pub(crate) fn embedding_delta_floor(&self) -> f64 {
        self.embedding_delta_floor
    }

    /// Whether a recorded regime id is one these numbers were measured in.
    #[must_use]
    pub(crate) fn is_calibrated(&self, regime_id: &str) -> bool {
        self.calibrated_regimes.contains(&regime_id)
    }

    /// The calibrated set, for a diagnostic that does not require reading the contract.
    #[must_use]
    pub(crate) fn calibrated_regimes(&self) -> &'static [&'static str] {
        self.calibrated_regimes
    }
}

// ===========================================================================================
// The contract's machine-readable shape — deserialization targets for the provenance test
// ===========================================================================================

/// Just enough of the contract to reach the frozen numbers.
#[derive(Debug, Deserialize)]
struct ContractFile {
    equations: ContractEquations,
}

#[derive(Debug, Deserialize)]
struct ContractEquations {
    evidence_gate: EvidenceGateEquation,
    calibration_regime: CalibrationRegimeEquation,
}

#[derive(Debug, Deserialize)]
struct EvidenceGateEquation {
    frozen_thresholds: BTreeMap<String, ContractClassThreshold>,
    embedding_delta_floor_value: f64,
}

#[derive(Debug, Deserialize)]
struct CalibrationRegimeEquation {
    calibrated_regimes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ContractClassThreshold {
    eps: Option<f64>,
    scale_floor: f64,
    sparse: bool,
    gated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE provenance gate: every Rust constant is PARSED from the contract and compared.
    ///
    /// Not a substring search. The contract is deserialized into typed structs and compared
    /// per class and per field, so a number that is right but in the wrong class slot is red,
    /// and so is a number that appears only in a prose paragraph.
    #[test]
    fn thresholds_match_the_contract() {
        let parsed: ContractFile =
            serde_yaml::from_str(CONTRACT_YAML).expect("the committed contract must deserialize");
        let frozen = Thresholds::frozen();

        // Non-vacuity FIRST. An empty map would make every per-class assertion below hold.
        assert_eq!(
            parsed.equations.evidence_gate.frozen_thresholds.len(),
            ParameterClass::ALL.len(),
            "the contract must carry one entry per class; a missing class would make the \
             comparison below vacuous for exactly the class that went missing",
        );

        for class in ParameterClass::ALL {
            let contracted = parsed
                .equations
                .evidence_gate
                .frozen_thresholds
                .get(class.tag())
                .unwrap_or_else(|| panic!("contract has no entry for class `{}`", class.tag()));
            let rust = frozen.of(class);

            assert_eq!(
                rust.eps,
                contracted.eps,
                "{}: Rust epsilon {:?} != contract epsilon {:?}. Edit BOTH or neither -- a \
                 one-sided change here is how an epsilon gets loosened after a failing \
                 comparison (T-3-21).",
                class.tag(),
                rust.eps,
                contracted.eps,
            );
            assert_eq!(rust.scale_floor, contracted.scale_floor, "{}: scale floor", class.tag());
            assert_eq!(rust.sparse, contracted.sparse, "{}: sparse denominator", class.tag());
            assert_eq!(rust.gated, contracted.gated, "{}: gated", class.tag());

            // The class's own opinion about its denominator must agree with the contract's.
            assert_eq!(
                class.is_sparse(),
                contracted.sparse,
                "{}: ParameterClass::is_sparse disagrees with the contract",
                class.tag(),
            );
            assert_eq!(
                class.scale_floor(),
                contracted.scale_floor,
                "{}: ParameterClass::scale_floor disagrees with the contract",
                class.tag(),
            );
        }

        assert_eq!(
            frozen.embedding_delta_floor(),
            parsed.equations.evidence_gate.embedding_delta_floor_value,
            "the embedding delta floor must match the contract",
        );

        let contracted_regimes = &parsed.equations.calibration_regime.calibrated_regimes;
        assert_eq!(
            contracted_regimes.len(),
            1,
            "exactly ONE calibrated fingerprint; a second would mean numbers measured on one \
             architecture are being applied to another",
        );
        assert_eq!(
            frozen.calibrated_regimes().len(),
            contracted_regimes.len(),
            "the Rust calibrated set and the contract's must have the same size",
        );
        for regime in contracted_regimes {
            assert!(
                frozen.is_calibrated(regime),
                "the contract lists regime `{regime}` which the Rust constant does not carry",
            );
        }
    }

    /// The gradient-free class is the ONLY ungated one, and it is ungated deliberately.
    #[test]
    fn thresholds_gate_every_class_except_the_gradient_free_one() {
        let frozen = Thresholds::frozen();
        let ungated: Vec<&str> = ParameterClass::ALL
            .into_iter()
            .filter(|c| !frozen.of(*c).gated)
            .map(ParameterClass::tag)
            .collect();
        assert_eq!(
            ungated,
            vec![ParameterClass::AttentionKeyBias.tag()],
            "exactly one class is excluded from the verdict, and it is the one whose gradient \
             is analytically zero",
        );

        // A gated class without an epsilon would silently gate on nothing.
        for class in ParameterClass::ALL {
            let entry = frozen.of(class);
            assert_eq!(
                entry.gated,
                entry.eps.is_some(),
                "{}: `gated` and the presence of an epsilon must agree, otherwise a class is \
                 either compared against nothing or carries a threshold nobody applies",
                class.tag(),
            );
        }
    }

    /// Every frozen epsilon is positive and finite.
    ///
    /// A zero or negative epsilon would make `relative_delta > eps` true for anything that
    /// moved at all, which is the strict predicate wearing a threshold's name.
    #[test]
    fn thresholds_are_positive_and_finite() {
        let frozen = Thresholds::frozen();
        for class in ParameterClass::ALL {
            let entry = frozen.of(class);
            if let Some(eps) = entry.eps {
                assert!(eps.is_finite() && eps > 0.0, "{}: epsilon {eps:e}", class.tag());
            }
            assert!(entry.scale_floor.is_finite() && entry.scale_floor > 0.0, "{}", class.tag());
        }
        let floor = frozen.embedding_delta_floor();
        assert!(floor.is_finite() && floor > 0.0);
    }
}
