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
//!
//! # Membership is COMPONENT-WISE, and it has to be
//!
//! A calibrated entry enumerates every seed and every cell the epsilons were measured over; a
//! run executes at ONE seed in ONE cell. Their ids can therefore never be string-equal, so
//! `contains(&regime_id)` had exactly two possible behaviours: refuse every honest run, or —
//! as it did — be satisfied by stamping the enumerated string onto runs that never executed
//! those coordinates. [`RegimeCoordinates`] parses both sides into
//! `(architecture, seed set, cell set)` and asks whether the calibrated entry COVERS the run.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
use serde::Deserialize;

use super::evidence::ParameterClass;

/// The contract this module is frozen against, embedded at compile time.
///
/// `include_str!` rather than a runtime read: a test that silently skips when a file is absent
/// is a test that proves nothing, and a path that resolves differently under `cargo test` and
/// under the packaged crate is a defect waiting for a release.
#[cfg(test)]
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

/// The `seeds=` field marker of the regime grammar.
const SEEDS_PREFIX: &str = "seeds=";

/// The `cells=` field marker of the regime grammar.
const CELLS_PREFIX: &str = "cells=";

/// A regime id parsed into the three things a calibration is indexed by.
///
/// # One grammar, two cardinalities
///
/// `<architecture>@<revision>|seeds=<u64,...>|cells=<label,...>`. A RUN renders its own
/// coordinates through [`Self::render_run`], so both of its sets are singletons; a CALIBRATED
/// entry enumerates every seed and cell the epsilons were measured over. The two are compared
/// by [`Self::covers`], never by string equality — see this module's header for why equality
/// could not work in either direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RegimeCoordinates {
    /// `<architecture fingerprint>@<source revision>`, compared for exact equality: an epsilon
    /// measured on one architecture is not evidence about another, so there is no notion of a
    /// "close enough" architecture here.
    architecture: String,
    /// Every root seed the id names.
    seeds: BTreeSet<u64>,
    /// Every cell label the id names.
    cells: BTreeSet<String>,
}

impl RegimeCoordinates {
    /// The canonical rendering of ONE run's coordinates.
    ///
    /// The writer and the reader share this function's grammar constants, so a run cannot be
    /// stamped in a shape the membership check cannot parse.
    pub(crate) fn render_run(architecture: &str, seed: u64, cell: &str) -> String {
        format!("{architecture}|{SEEDS_PREFIX}{seed}|{CELLS_PREFIX}{cell}")
    }

    /// Parse an id in the grammar above.
    ///
    /// # Errors
    ///
    /// [`RegimeParseError`] — every malformed shape is a distinct, named variant. Nothing is
    /// tolerated silently: an id this function cannot read is an id whose coordinates are
    /// unknown, and unknown coordinates must never be treated as calibrated ones.
    pub(crate) fn parse(id: &str) -> Result<Self, RegimeParseError> {
        let mut fields = id.split('|');
        let (Some(architecture), Some(seed_field), Some(cell_field), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err(RegimeParseError::FieldCount { id: id.to_string() });
        };
        if architecture.is_empty() {
            return Err(RegimeParseError::EmptyArchitecture { id: id.to_string() });
        }

        let seed_list = seed_field.strip_prefix(SEEDS_PREFIX).ok_or_else(|| {
            RegimeParseError::MissingPrefix { expected: SEEDS_PREFIX, id: id.to_string() }
        })?;
        let cell_list = cell_field.strip_prefix(CELLS_PREFIX).ok_or_else(|| {
            RegimeParseError::MissingPrefix { expected: CELLS_PREFIX, id: id.to_string() }
        })?;

        let mut seeds = BTreeSet::new();
        for item in seed_list.split(',') {
            let seed = item.parse::<u64>().map_err(|_| RegimeParseError::NotASeed {
                value: item.to_string(),
                id: id.to_string(),
            })?;
            seeds.insert(seed);
        }
        let mut cells = BTreeSet::new();
        for item in cell_list.split(',') {
            if item.is_empty() {
                return Err(RegimeParseError::EmptyCellLabel { id: id.to_string() });
            }
            cells.insert(item.to_string());
        }
        // `str::split` always yields at least one item, and both loops above reject the empty
        // one, so neither set can be empty here. Asserted rather than assumed, because an
        // empty run-side set would make the subset test below vacuously true.
        debug_assert!(!seeds.is_empty(), "an empty seed set would pass every subset test");
        debug_assert!(!cells.is_empty(), "an empty cell set would pass every subset test");

        Ok(Self { architecture: architecture.to_string(), seeds, cells })
    }

    /// Whether THIS (calibrated) entry covers `run`'s coordinates.
    ///
    /// Same architecture, and every seed and every cell the run names was measured. For the
    /// singleton sets a run always carries this reads as "the run's seed and cell are both in
    /// the measured sets"; the subset form is what lets the calibrated entry itself — which
    /// names all three seeds and both cells — be checked by the same function.
    pub(crate) fn covers(&self, run: &Self) -> bool {
        self.architecture == run.architecture
            && run.seeds.is_subset(&self.seeds)
            && run.cells.is_subset(&self.cells)
    }
}

/// Why a regime id could not be read.
///
/// One variant per malformed shape, each carrying the offending id, so a diagnosis never has
/// to guess which of the three fields was wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RegimeParseError {
    /// Not exactly three `|`-separated fields.
    FieldCount {
        /// The offending id.
        id: String,
    },
    /// The architecture field is empty.
    EmptyArchitecture {
        /// The offending id.
        id: String,
    },
    /// A field did not open with its marker.
    MissingPrefix {
        /// The marker that was expected.
        expected: &'static str,
        /// The offending id.
        id: String,
    },
    /// A seed item is not a `u64`.
    NotASeed {
        /// The item that failed to parse.
        value: String,
        /// The offending id.
        id: String,
    },
    /// A cell item is the empty string.
    EmptyCellLabel {
        /// The offending id.
        id: String,
    },
}

impl fmt::Display for RegimeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FieldCount { id } => write!(
                f,
                "`{id}` is not a regime id: expected exactly three `|`-separated fields, \
                 `<architecture>@<revision>|{SEEDS_PREFIX}<u64,...>|{CELLS_PREFIX}<label,...>`",
            ),
            Self::EmptyArchitecture { id } => {
                write!(f, "`{id}` has an empty architecture field")
            }
            Self::MissingPrefix { expected, id } => {
                write!(f, "`{id}` is missing the `{expected}` marker")
            }
            Self::NotASeed { value, id } => {
                write!(f, "`{id}` names seed `{value}`, which is not a u64")
            }
            Self::EmptyCellLabel { id } => write!(f, "`{id}` names an empty cell label"),
        }
    }
}

/// Parse a CALIBRATED entry, aborting if it is malformed.
///
/// Deliberately not a `Result`. A calibrated entry is a compile-time constant this crate
/// commits to and `thresholds_match_the_contract` compares against the contract; if one cannot
/// be read, the honest outcome is to stop. Treating it as "matches nothing" would turn a typo
/// into a gate that refuses every run while still reporting the entry as calibrated —
/// fail-closed in appearance and broken in fact, and green in every test that only ever checks
/// that bad runs are refused.
///
/// # Panics
///
/// If `entry` is not in the regime grammar.
fn parse_calibrated(entry: &str) -> RegimeCoordinates {
    RegimeCoordinates::parse(entry).unwrap_or_else(|err| {
        panic!(
            "malformed calibrated regime entry: {err}. A calibrated entry that cannot be \
             parsed ABORTS rather than being skipped, because a skipped entry leaves a gate \
             that refuses every run while still reporting the entry as calibrated",
        )
    })
}

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

    /// Whether a recorded regime id names an architecture, seed and cell these numbers were
    /// measured in.
    ///
    /// COMPONENT-WISE, not string equality: the run's architecture must equal a calibrated
    /// entry's, and the run's seed and cell must both be members of that entry's measured sets.
    ///
    /// An id this crate cannot PARSE is not calibrated. That is the fail-closed direction: an
    /// unreadable id is one whose coordinates are unknown, and unknown coordinates are exactly
    /// what the gate exists to refuse. The opposite treatment is reserved for a malformed
    /// CALIBRATED entry, which aborts (see [`parse_calibrated`]).
    #[must_use]
    pub(crate) fn is_calibrated(&self, regime_id: &str) -> bool {
        let Ok(observed) = RegimeCoordinates::parse(regime_id) else {
            return false;
        };
        self.calibrated_regimes.iter().any(|entry| parse_calibrated(entry).covers(&observed))
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
#[cfg(test)]
#[derive(Debug, Deserialize)]
struct ContractFile {
    equations: ContractEquations,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
struct ContractEquations {
    evidence_gate: EvidenceGateEquation,
    calibration_regime: CalibrationRegimeEquation,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
struct EvidenceGateEquation {
    frozen_thresholds: BTreeMap<String, ContractClassThreshold>,
    embedding_delta_floor_value: f64,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
struct CalibrationRegimeEquation {
    calibrated_regimes: Vec<String>,
}

#[cfg(test)]
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

    /// The architecture component of the single calibrated entry, for the tests below.
    fn calibrated_architecture() -> String {
        let entry = CALIBRATED_REGIMES.first().expect("exactly one calibrated entry");
        RegimeCoordinates::parse(entry).expect("the calibrated entry must parse").architecture
    }

    /// A run id rendered by the writer is readable by the reader, field for field.
    ///
    /// Writer and reader sharing one grammar is what makes the membership check below a check
    /// on the run rather than on a string convention two places happen to agree about.
    #[test]
    fn regime_render_and_parse_round_trip() {
        let rendered = RegimeCoordinates::render_run("arch@rev", 42, "s8e1b4");
        assert_eq!(rendered, "arch@rev|seeds=42|cells=s8e1b4");

        let parsed = RegimeCoordinates::parse(&rendered).expect("a rendered id must parse");
        assert_eq!(parsed.architecture, "arch@rev");
        assert_eq!(parsed.seeds, BTreeSet::from([42]));
        assert_eq!(parsed.cells, BTreeSet::from(["s8e1b4".to_string()]));
    }

    /// The seed and cell SETS are order-insensitive, so the frozen entry's `1,42,7` ordering
    /// is a rendering detail and not a third thing to keep in sync.
    #[test]
    fn regime_parse_reads_sets_not_ordered_lists() {
        let a = RegimeCoordinates::parse("x@y|seeds=1,42,7|cells=b,a").expect("parses");
        let b = RegimeCoordinates::parse("x@y|seeds=7,1,42|cells=a,b").expect("parses");
        assert_eq!(a, b, "a regime id names two SETS; their written order carries no meaning");
    }

    /// Every malformed shape is REJECTED, and each by its own named variant.
    ///
    /// A case table rather than a spot check: the parser is the thing standing between a
    /// typo and a gate that silently stops discriminating, and four of these five shapes are
    /// one keystroke away from a well-formed id.
    #[test]
    fn regime_parse_rejects_every_malformed_shape() {
        let cases: [(&str, RegimeParseError); 7] = [
            (
                "arch@rev|seeds=1",
                RegimeParseError::FieldCount { id: "arch@rev|seeds=1".to_string() },
            ),
            (
                "arch@rev|seeds=1|cells=a|extra",
                RegimeParseError::FieldCount { id: "arch@rev|seeds=1|cells=a|extra".to_string() },
            ),
            (
                "|seeds=1|cells=a",
                RegimeParseError::EmptyArchitecture { id: "|seeds=1|cells=a".to_string() },
            ),
            (
                "arch@rev|seed=1|cells=a",
                RegimeParseError::MissingPrefix {
                    expected: SEEDS_PREFIX,
                    id: "arch@rev|seed=1|cells=a".to_string(),
                },
            ),
            (
                "arch@rev|seeds=1|cell=a",
                RegimeParseError::MissingPrefix {
                    expected: CELLS_PREFIX,
                    id: "arch@rev|seeds=1|cell=a".to_string(),
                },
            ),
            (
                "arch@rev|seeds=|cells=a",
                RegimeParseError::NotASeed {
                    value: String::new(),
                    id: "arch@rev|seeds=|cells=a".to_string(),
                },
            ),
            (
                "arch@rev|seeds=1|cells=",
                RegimeParseError::EmptyCellLabel { id: "arch@rev|seeds=1|cells=".to_string() },
            ),
        ];
        for (id, expected) in cases {
            assert_eq!(
                RegimeCoordinates::parse(id),
                Err(expected),
                "`{id}` must be rejected, and by the variant that names what is wrong",
            );
            assert!(
                !Thresholds::frozen().is_calibrated(id),
                "an id the parser cannot read must never be treated as calibrated: `{id}`",
            );
        }
    }

    /// Membership is COMPONENT-WISE. This is the check the gate rests on.
    ///
    /// A single run carries one seed and one cell, so its id can never string-equal an entry
    /// enumerating three seeds; the negatives below are what distinguish this from a check
    /// that accepts anything on the right architecture.
    #[test]
    fn regime_membership_is_component_wise() {
        let frozen = Thresholds::frozen();
        let arch = calibrated_architecture();

        // POSITIVE: each measured (seed, cell) pair, one run at a time.
        for seed in [1_u64, 7, 42] {
            for cell in ["s8e1b4", "s16e2b8"] {
                assert!(
                    frozen.is_calibrated(&RegimeCoordinates::render_run(&arch, seed, cell)),
                    "seed {seed} cell {cell} was measured and must be accepted",
                );
            }
        }

        // NEGATIVE: the seed alone is wrong.
        assert!(
            !frozen.is_calibrated(&RegimeCoordinates::render_run(&arch, 2, "s8e1b4")),
            "seed 2 was never swept; a calibrated cell does not make it calibrated",
        );
        // NEGATIVE: the cell alone is wrong.
        assert!(
            !frozen.is_calibrated(&RegimeCoordinates::render_run(&arch, 1, "s8e1b3")),
            "cell s8e1b3 was never measured; a calibrated seed does not make it calibrated",
        );
        // NEGATIVE: the architecture alone is wrong.
        assert!(
            !frozen.is_calibrated(&RegimeCoordinates::render_run(
                "minilm-full-h384-l6-a12-i1536-v30522@production",
                1,
                "s8e1b4",
            )),
            "the production encoder is not calibrated at any seed or cell (Phase 5 is blocked \
             by this, deliberately)",
        );
        // NEGATIVE: a superset of the measured seeds is not covered either.
        assert!(
            !frozen.is_calibrated(&format!("{arch}|seeds=1,7,42,99|cells=s8e1b4")),
            "one unmeasured seed in the set is enough to refuse",
        );
    }

    /// A malformed CALIBRATED entry ABORTS rather than quietly matching nothing.
    ///
    /// The opposite of the run side: a typo in the frozen constant must not be able to
    /// masquerade as a gate that is merely very strict.
    #[test]
    #[should_panic(expected = "malformed calibrated regime entry")]
    fn regime_a_malformed_calibrated_entry_aborts() {
        let _ = parse_calibrated("minilm-slice-h64-l2-a2-i256-v97@1110a243|seeds=1,42,7");
    }

    /// Every committed calibrated entry is in the grammar the gate parses.
    #[test]
    fn regime_every_calibrated_entry_parses() {
        assert!(!CALIBRATED_REGIMES.is_empty(), "non-vacuity: the set must not be empty");
        for entry in CALIBRATED_REGIMES {
            let parsed = RegimeCoordinates::parse(entry)
                .unwrap_or_else(|err| panic!("calibrated entry `{entry}` must parse: {err}"));
            assert!(!parsed.seeds.is_empty(), "`{entry}` names no seed");
            assert!(!parsed.cells.is_empty(), "`{entry}` names no cell");
            assert!(
                parsed.architecture.contains('@'),
                "`{entry}` must name an architecture AND a source revision",
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
