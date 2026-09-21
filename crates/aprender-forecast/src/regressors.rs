//! External regressors (covariates) spliced onto the shipped Prophet design (D-22, SC1).
//!
//! # The shape of the change, and why it is additive
//!
//! Nothing here forks `prophet.rs`. The base design still comes from
//! [`crate::prophet::make_design`] and the base feature rows still come from
//! [`crate::prophet::feature_row`]; only the regressor columns are new code. That is the
//! whole finding spike 011 retired: the SHIPPED types admit regressors without
//! restructuring, in 219 additive lines.
//!
//! Regressor columns APPEND, after the seasonality block and the name-sorted holiday block
//! `prophet::columns` already produces. No existing column index moves — which is precisely
//! what makes the D-19 no-argument invariance gate free rather than expensive.
//!
//! # Identifying a regressor column: structure, not name
//!
//! The spike prototype identified regressor columns by NAME membership. That is O(R) per
//! column and silently WRONG under a name collision: a regressor literally named
//! `yearly_delim_1` would be summed into `extra_regressors_additive`. This module uses the
//! STRUCTURAL fact the splice guarantees instead — regressors occupy the contiguous
//! trailing index range `base_k..k`, where `base_k = d.k - regs.len()`.
//!
//! This is also why [`crate::prophet::Column`]'s two-valued `holiday` discriminator is left
//! alone. Promoting it to a three-valued `ColumnKind` is the textbook move and is refused
//! deliberately: `Column` is `pub` with `pub` fields, so changing the discriminator rewrites
//! `feature_row`'s filter, `predict`'s holiday roll-up and every construction site — a
//! rewrite of the thing this phase is committed not to rewrite — and it would restructure
//! the very type the invariance gate compares. What would force the promotion later: a
//! FOURTH column family, or any change that lets a regressor occupy a non-trailing index.

use crate::prophet::{Column, Design, Mode};

/// What the caller asked for, before the history is seen.
#[derive(Clone, Debug)]
pub struct RegressorSpec {
    pub name: String,
    pub mode: Mode,
    pub prior_scale: f64,
    /// `None` is Prophet's "auto".
    pub standardize: Option<bool>,
}

/// A spec plus the standardisation constants derived from the history rows.
#[derive(Clone, Debug)]
pub struct Standardized {
    pub name: String,
    pub mu: f64,
    pub std: f64,
    pub mode: Mode,
    pub prior_scale: f64,
}

/// Prophet 1.4's `initialize_scales`, over the HISTORY rows only.
///
/// # The auto rule, stated exactly
///
/// "auto" means: do not standardise a column whose unique history values are EXACTLY the
/// two-element set {0, 1} — **both present**. This is a two-element requirement, not a
/// subset test, and the distinction is behavioural rather than pedantic:
///
/// - `{0, 1}` both present  -> exempt, `mu = 0.0`, `std = 1.0` (an indicator column).
/// - all zeros (or all ones) -> ONE unique value, therefore NOT exempt, therefore
///   standardised, therefore `std = 0.0`, which the door refuses as a constant column.
///
/// Do not "fix" this into a subset test: that would silently exempt a constant column and
/// hand `splice` a division by zero instead of a refusal.
///
/// # The ddof trap
///
/// The spread is pandas `Series.std()` — the SAMPLE standard deviation, variance divided by
/// `(n - 1)`, i.e. ddof = 1. numpy's default is ddof = 0 and gives `6.500320` where the
/// oracle stores `6.511441581253706` for `price`: a silent 0.17% shift in every coefficient,
/// surfacing as a parity failure that looks like arithmetic noise rather than a wrong
/// divisor.
///
/// A constant non-binary column yields a spread of exactly `0.0`. This function does NOT
/// divide by it and does NOT refuse — it RETURNS the zero so the caller can see it. The door
/// owns that refusal, because a library must not decide policy for every caller.
#[must_use]
pub fn standardize_one(spec: &RegressorSpec, history: &[f64]) -> Standardized {
    // BOTH 0 and 1 must be present, and nothing else: a subset test would exempt a
    // constant column and hand `splice` a division by zero instead of a refusal.
    // One pass, short-circuiting on the first non-{0,1} element — the common case is a
    // continuous column (`price`, `weather`), which exits at element 0.
    let binary = {
        let (mut zero, mut one) = (false, false);
        history.iter().all(|&v| {
            if v == 0.0 {
                zero = true;
            } else if v == 1.0 {
                one = true;
            } else {
                return false;
            }
            true
        }) && zero
            && one
    };
    let do_std = spec.standardize.unwrap_or(!binary);
    let (mu, std) = if do_std {
        let n = history.len() as f64;
        let mu = history.iter().sum::<f64>() / n;
        // ddof = 1 — pandas, not numpy. See the doc comment.
        let var = history.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / (n - 1.0);
        (mu, var.sqrt())
    } else {
        (0.0, 1.0)
    };
    Standardized {
        name: spec.name.clone(),
        mu,
        std,
        mode: spec.mode,
        prior_scale: spec.prior_scale,
    }
}

/// Append one column per regressor, in INSERTION order, after everything already present.
///
/// Measured against Prophet 1.4.0: the oracle's column order is seasonalities, then
/// name-sorted holidays, then extra regressors in insertion order, so a regressor never
/// reorders an existing column.
///
/// **An empty `regs` is an immediate no-op** — not "a rebuild that happens to produce the
/// same bytes". That early return is what makes the D-19 gate free: with no regressors,
/// `Design.k`, `cols`, `x`, `s_a`, `s_m` and `prior_scales` are not merely equal afterwards,
/// they are untouched.
pub fn splice(d: &mut Design, regs: &[Standardized], values_history: &[Vec<f64>]) {
    if regs.is_empty() {
        return;
    }
    let base_k = d.k;
    let t = d.t.len();
    let new_k = base_k + regs.len();
    let mut x = vec![0.0f64; t * new_k];
    for i in 0..t {
        x[i * new_k..i * new_k + base_k].copy_from_slice(&d.x[i * base_k..(i + 1) * base_k]);
        for (j, reg) in regs.iter().enumerate() {
            // `values_history[j]` is the history PREFIX, length `t`; the door splits the
            // caller's `ds.len() + horizon` array before calling this.
            x[i * new_k + base_k + j] = (values_history[j][i] - reg.mu) / reg.std;
        }
    }
    for reg in regs {
        d.cols.push(Column {
            name: reg.name.clone(),
            component: reg.name.clone(),
            mode: reg.mode,
            prior_scale: reg.prior_scale,
            // `None` here now means "seasonality OR regressor". The ambiguity is contained
            // by the trailing-index rule in the module docs, never by reading this field.
            holiday: None,
        });
        d.prior_scales.push(reg.prior_scale);
        d.s_a
            .push(if reg.mode == Mode::Additive { 1.0 } else { 0.0 });
        d.s_m.push(if reg.mode == Mode::Multiplicative {
            1.0
        } else {
            0.0
        });
    }
    d.x = x;
    d.k = new_k;
}

/// The per-row value channel handed to [`crate::prophet::predict`].
///
/// A regressor value is NOT a function of the day, so it cannot come from `feature_row`,
/// which is keyed on `day`. It has to travel beside the design as caller data — that is the
/// whole reason `predict` grows a parameter rather than reading more out of `Design`.
#[derive(Clone, Copy, Debug)]
pub struct RegressorChannel<'a> {
    pub specs: &'a [Standardized],
    /// One array per regressor, each exactly as long as the `ds_days` being predicted.
    pub values: &'a [Vec<f64>],
}

impl RegressorChannel<'_> {
    /// The inert channel. Passed EXPLICITLY at every call site that has no regressors, so
    /// the inert case is visible in the source rather than defaulted into invisibility.
    pub const NONE: RegressorChannel<'static> = RegressorChannel {
        specs: &[],
        values: &[],
    };

    /// How many trailing design columns this channel owns.
    #[must_use]
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }
}

#[cfg(test)]
mod tracer;

#[cfg(test)]
mod tests {
    use super::{splice, standardize_one, RegressorSpec, Standardized};
    use crate::prophet::{make_design, Mode, Seasonality, Spec};

    fn spec_auto(name: &str) -> RegressorSpec {
        RegressorSpec {
            name: name.into(),
            mode: Mode::Additive,
            prior_scale: 10.0,
            standardize: None,
        }
    }

    /// The `{0, 1}` carve-out fires only when BOTH values are present.
    #[test]
    fn auto_leaves_a_binary_column_unstandardised() {
        let s = standardize_one(&spec_auto("promo"), &[0.0, 1.0, 0.0, 1.0, 1.0]);
        assert!(
            (s.mu - 0.0).abs() < f64::EPSILON && (s.std - 1.0).abs() < f64::EPSILON,
            "a {{0,1}} column under auto must come back mu=0 std=1, got mu={} std={}",
            s.mu,
            s.std
        );
    }

    /// The other half of the same rule, and the one the prototype and the control disagreed
    /// about: ONE unique value is not the two-element set, so it IS standardised and comes
    /// out at zero spread. The door refuses that; this test pins that it reaches the door.
    #[test]
    fn auto_standardises_an_all_zero_column_to_zero_spread() {
        let s = standardize_one(&spec_auto("dead"), &[0.0; 12]);
        assert!(
            (s.std - 0.0).abs() < f64::EPSILON,
            "an all-zero column has ONE unique value, is not auto-exempt, and must come \
             back with std = 0 for the door to refuse; got std={}",
            s.std
        );
    }

    /// ddof = 1, measured against the oracle's own constant rather than against a formula
    /// written twice.
    #[test]
    fn the_sample_standard_deviation_uses_ddof_one() {
        let h = [1.0, 2.0, 3.0, 4.0];
        let s = standardize_one(&spec_auto("x"), &h);
        // mean 2.5; ddof=1 variance = (2.25+0.25+0.25+2.25)/3 = 5/3
        let want = (5.0f64 / 3.0).sqrt();
        assert!(
            (s.std - want).abs() < 1e-15,
            "ddof must be 1 (pandas), got {} want {want}",
            s.std
        );
        // The ddof=0 answer, which must NOT be what we produce.
        let ddof0 = (5.0f64 / 4.0).sqrt();
        assert!(
            (s.std - ddof0).abs() > 1e-9,
            "ddof=0 would give {ddof0}; that divisor shifts every coefficient by ~0.17%"
        );
    }

    /// The D-19 mechanism, asserted at the splice itself: an empty regressor slice leaves
    /// every design field bit-identical.
    #[test]
    fn splicing_zero_regressors_is_bit_identical() {
        let t0 = crate::dates::days_from_civil(2020, 1, 1);
        let ds: Vec<i64> = (0..120).map(|i| t0 + i).collect();
        let y: Vec<f64> = (0..120).map(|i| 10.0 + f64::from(i) * 0.1).collect();
        let spec = Spec::default_linear(vec![Seasonality {
            name: "weekly".into(),
            period: 7.0,
            order: 3,
            prior_scale: 10.0,
            mode: Mode::Additive,
        }]);
        let plain = make_design(&ds, &y, &spec);
        let mut spliced = make_design(&ds, &y, &spec);
        let none: Vec<Standardized> = Vec::new();
        let novals: Vec<Vec<f64>> = Vec::new();
        splice(&mut spliced, &none, &novals);

        assert_eq!(plain.k, spliced.k);
        assert_eq!(plain.cols.len(), spliced.cols.len());
        let bits = |a: &[f64], b: &[f64]| a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits());
        assert!(bits(&plain.x, &spliced.x), "x must be bit-identical");
        assert!(bits(&plain.s_a, &spliced.s_a), "s_a must be bit-identical");
        assert!(bits(&plain.s_m, &spliced.s_m), "s_m must be bit-identical");
        assert!(
            bits(&plain.prior_scales, &spliced.prior_scales),
            "prior_scales must be bit-identical"
        );
    }

    /// Regressors APPEND: no existing column index moves, which is what the invariance gate
    /// rests on.
    #[test]
    fn splicing_appends_and_moves_no_existing_column() {
        let t0 = crate::dates::days_from_civil(2020, 1, 1);
        let ds: Vec<i64> = (0..60).map(|i| t0 + i).collect();
        let y: Vec<f64> = (0..60).map(|i| 5.0 + f64::from(i)).collect();
        let spec = Spec::default_linear(vec![Seasonality {
            name: "weekly".into(),
            period: 7.0,
            order: 2,
            prior_scale: 10.0,
            mode: Mode::Additive,
        }]);
        let plain = make_design(&ds, &y, &spec);
        let mut d = make_design(&ds, &y, &spec);
        let base_k = d.k;
        let regs = vec![Standardized {
            name: "promo".into(),
            mu: 0.0,
            std: 1.0,
            mode: Mode::Multiplicative,
            prior_scale: 5.0,
        }];
        let vals = vec![(0..60).map(|i| f64::from(i % 2)).collect::<Vec<f64>>()];
        splice(&mut d, &regs, &vals);

        assert_eq!(d.k, base_k + 1, "one regressor adds exactly one column");
        for (i, c) in plain.cols.iter().enumerate() {
            assert_eq!(
                d.cols[i].name, c.name,
                "existing column {i} must keep its index"
            );
        }
        assert_eq!(d.cols[base_k].name, "promo");
        assert!(
            (d.s_m[base_k] - 1.0).abs() < f64::EPSILON && d.s_a[base_k].abs() < f64::EPSILON,
            "a multiplicative regressor lands in s_m, not s_a"
        );
        // Every preserved row prefix is bit-identical to the unspliced design.
        for i in 0..60 {
            for c in 0..base_k {
                assert_eq!(
                    d.x[i * d.k + c].to_bits(),
                    plain.x[i * base_k + c].to_bits(),
                    "row {i} col {c} moved"
                );
            }
        }
    }
}
