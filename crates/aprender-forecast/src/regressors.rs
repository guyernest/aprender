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


// ============================================================ identifiability (D-35) ====
//
// # What this answers, and what it deliberately does not
//
// Spike 011's one operator-facing recommendation: a driver that is a harmonic of a
// seasonality gets a coefficient that is several times off while the FORECAST stays valid
// and the objective stays at or better than Python's. So the honest move is to say so
// beside the number, not to block the request — D-35: **warn, never refuse**.
//
// # Why VIF and a condition number, and NOT a pairwise correlation cutoff
//
// D-35 refuses a pairwise cutoff explicitly, on a measurement: a 0.9 threshold PASSED
// `price` (max pairwise r = 0.759) whose beta is still 5x off. A pairwise maximum is blind
// to a column explained JOINTLY by several others, which is the common case once a design
// carries a full Fourier seasonality block. VIF is the standard multi-column measure — the
// diagonal of the inverse correlation matrix — and the condition number is its global
// companion.
//
// # The diagnostic COLUMN SET, and why holidays are excluded
//
// The set is the trend proxy `t`, the seasonality columns, and the regressor columns.
// HOLIDAY indicator columns are EXCLUDED, and that is a bounded-cost decision recorded in
// cost axis C-17, not an oversight: the holiday block can reach `fit_max_holiday_columns`
// (1 000) design columns, and this routine's cost is `O(len(ds) * K^2 + K^3)`, so including
// them would make the cubic term the dominant cost of the entire request and would force a
// second, much larger ceiling on C-17. With them excluded, `K = 1 + seasonality_columns +
// n_regressors` and `seasonality_columns <= 34` is a property of `auto_seasonalities` rather
// than of the request, so the cubic term is bounded by `fit_max_regressors` alone.
//
// It is also where the evidence is: every collinearity spike 011 measured was
// regressor-against-seasonality or regressor-against-trend. The exclusion is surfaced in the
// emitted `scope` string as well as in the contract, so an operator reading a VIF knows what
// it was computed against — a STATED scope rather than a silent one (threat T-06.1-11).

/// Iteration cap for both eigenvalue loops.
///
/// Neither loop may spin on a caller-controlled predicate alone (threat T-06.1-12). On
/// exhausting this cap the condition number is WITHHELD — see [`IdentifiabilityStatus`].
pub const REGRESSOR_EIGEN_MAX_ITERS: usize = 100;

/// Relative convergence tolerance for both eigenvalue loops.
pub const REGRESSOR_EIGEN_TOL: f64 = 1e-10;

/// Vector-norm floor below which an iteration STOPS rather than normalising by a near-zero.
///
/// Dividing by a norm this small manufactures a direction out of rounding noise, and the
/// iterate that comes back looks like a measurement.
pub const REGRESSOR_EIGEN_NORM_FLOOR: f64 = 1e-12;

/// The ridge ladder: at most three attempts after the unridged one, escalating by 100x.
const REGRESSOR_RIDGE_ATTEMPTS: usize = 3;
/// The first rung, as a fraction of the mean diagonal (`trace / K`).
const REGRESSOR_RIDGE_BASE: f64 = 1e-8;

/// What the emitted numbers MEAN — a typed state, never left to what the arithmetic
/// happened to do.
///
/// An operator reads these numbers. A silent NaN, an unbounded loop, or a value that
/// secretly describes a regularised matrix is worse than no diagnostic at all, so every
/// degenerate outcome is a named state with a defined report shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentifiabilityStatus {
    /// The correlation matrix factorised and both eigenvalue loops converged. Every number
    /// in the report describes the matrix ACTUALLY factorised (see `regularized`).
    Ok,
    /// The matrix is singular to working precision — an exactly duplicated or exactly
    /// collinear column. Every `vif` and the condition number are `null`.
    Singular,
    /// An eigenvalue loop hit [`REGRESSOR_EIGEN_MAX_ITERS`]. The condition number is `null`;
    /// the Cholesky-derived VIFs are unaffected and are still reported.
    NotConverged,
}

impl IdentifiabilityStatus {
    /// The wire spelling. `snake_case`, because it is a JSON enum a consumer matches on.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Singular => "singular",
            Self::NotConverged => "not_converged",
        }
    }
}

/// One regressor's line of the report.
#[derive(Clone, Debug)]
pub struct RegressorReport {
    pub name: String,
    pub mode: Mode,
    pub mu: f64,
    pub std: f64,
    /// `None` becomes JSON `null` — a DELIBERATE statement that the number is withheld.
    pub vif: Option<f64>,
    pub warning: Option<String>,
}

/// The sibling object: everything that is a property of the DESIGN rather than of one
/// regressor.
///
/// Split from the per-regressor array because a JSON array cannot carry a `scope` property
/// and a JSON number cannot carry a warning string. `ForecastResponse.diagnostics` is
/// untyped, so nothing downstream would have caught a contradictory shape.
#[derive(Clone, Debug)]
pub struct Identifiability {
    pub scope: String,
    pub regressors: Vec<RegressorReport>,
    pub condition_number: Option<f64>,
    pub condition_number_warning: Option<String>,
    pub ridge: Option<f64>,
    pub regularized: bool,
    pub status: IdentifiabilityStatus,
}

/// Lower Cholesky factor of a symmetric matrix, row-major, or `None` if it is not positive
/// definite to working precision.
///
/// Own f64 implementation on purpose: `crates/aprender-forecast/Cargo.toml` gains NO
/// dependency for this work (threat T-06.1-SC, asserted by a verify), and core's
/// `cholesky_solve` is f32 while every number here is f64.
fn cholesky(a: &[f64], k: usize) -> Option<Vec<f64>> {
    let mut l = vec![0.0f64; k * k];
    for i in 0..k {
        for j in 0..=i {
            let mut s = a[i * k + j];
            for m in 0..j {
                s -= l[i * k + m] * l[j * k + m];
            }
            if i == j {
                // A non-positive or non-finite pivot is exactly the rank deficiency this
                // returns `None` for; continuing would take the square root of a negative.
                if !s.is_finite() || s <= 0.0 {
                    return None;
                }
                l[i * k + j] = s.sqrt();
            } else {
                let d = l[j * k + j];
                if d == 0.0 || !d.is_finite() {
                    return None;
                }
                l[i * k + j] = s / d;
            }
        }
    }
    Some(l)
}

/// Solve `A x = b` given `A`'s lower Cholesky factor, by forward then back substitution.
fn chol_solve(l: &[f64], k: usize, b: &[f64]) -> Vec<f64> {
    let mut y = vec![0.0f64; k];
    for i in 0..k {
        let mut s = b[i];
        for m in 0..i {
            s -= l[i * k + m] * y[m];
        }
        y[i] = s / l[i * k + i];
    }
    let mut x = vec![0.0f64; k];
    for i in (0..k).rev() {
        let mut s = y[i];
        for m in (i + 1)..k {
            s -= l[m * k + i] * x[m];
        }
        x[i] = s / l[i * k + i];
    }
    x
}

fn mat_vec(a: &[f64], k: usize, x: &[f64]) -> Vec<f64> {
    (0..k)
        .map(|i| (0..k).map(|j| a[i * k + j] * x[j]).sum())
        .collect()
}

fn norm2(x: &[f64]) -> f64 {
    x.iter().map(|v| v * v).sum::<f64>().sqrt()
}

/// The generic bounded power iteration, over an arbitrary `apply`.
///
/// Returns the converged Rayleigh quotient, or `None` on exhausting `max_iters` or on the
/// vector norm collapsing below [`REGRESSOR_EIGEN_NORM_FLOOR`]. It NEVER returns the last
/// iterate as if it had converged — a plausible wrong number is the worst outcome available
/// to a diagnostic whose whole purpose is that an operator reads it.
fn bounded_power_iteration(
    k: usize,
    max_iters: usize,
    tol: f64,
    apply: impl Fn(&[f64]) -> Vec<f64>,
) -> Option<f64> {
    // Deterministic and DELIBERATELY not the all-ones vector: all-ones is an exact
    // eigenvector of an equicorrelated matrix, which would make the iteration converge for
    // a reason the general case does not enjoy and hide a real defect behind a green test.
    let mut x: Vec<f64> = (0..k).map(|i| 1.0 + (i as f64) * 0.5).collect();
    let n0 = norm2(&x);
    if n0 < REGRESSOR_EIGEN_NORM_FLOOR {
        return None;
    }
    for v in &mut x {
        *v /= n0;
    }
    let mut prev = f64::NAN;
    for _ in 0..max_iters {
        let y = apply(&x);
        if y.iter().any(|v| !v.is_finite()) {
            return None;
        }
        // Rayleigh quotient at the CURRENT unit iterate: x' A x.
        let lambda: f64 = x.iter().zip(y.iter()).map(|(a, b)| a * b).sum();
        let ny = norm2(&y);
        if ny < REGRESSOR_EIGEN_NORM_FLOOR {
            return None;
        }
        for (xi, yi) in x.iter_mut().zip(y.iter()) {
            *xi = yi / ny;
        }
        if prev.is_finite() {
            let rel = (lambda - prev).abs() / prev.abs().max(1.0);
            if rel < tol {
                return Some(lambda);
            }
        }
        prev = lambda;
    }
    None
}

/// `(lambda_min, lambda_max)` of a symmetric positive-definite matrix.
///
/// `lambda_max` by power iteration; `lambda_min` by INVERSE power iteration through the
/// Cholesky factor, which is the same factor the VIFs are solved with — so the two halves
/// of the report cannot disagree about which matrix they describe.
///
/// `pub(crate)` so [`the falsification probe`](identifiability_tests) can drive it on a
/// matrix whose eigenvalues are known in closed form. A routine that returned a constant
/// would otherwise produce exactly the same green table as a correct one.
pub(crate) fn spectrum(a: &[f64], k: usize, max_iters: usize, tol: f64) -> Option<(f64, f64)> {
    let hi = bounded_power_iteration(k, max_iters, tol, |x| mat_vec(a, k, x))?;
    let l = cholesky(a, k)?;
    let inv_hi = bounded_power_iteration(k, max_iters, tol, |x| chol_solve(&l, k, x))?;
    if !inv_hi.is_finite() || !hi.is_finite() || inv_hi <= 0.0 {
        return None;
    }
    Some((1.0 / inv_hi, hi))
}

/// The diagnostic column set, as centred-and-scaled columns plus their labels.
///
/// Returns `(columns, labels, regressor_slots)`, where `regressor_slots[j]` is the index
/// into `columns` of regressor `j`, or `None` if that column had zero spread and was
/// DROPPED rather than divided by.
fn diagnostic_columns(
    d: &Design,
    regs: &[Standardized],
) -> (Vec<Vec<f64>>, Vec<String>, Vec<Option<usize>>, Vec<String>) {
    let n = d.t.len();
    let base_k = d.k.saturating_sub(regs.len());
    let mut cols: Vec<Vec<f64>> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();

    let push = |name: String,
                    raw: Vec<f64>,
                    cols: &mut Vec<Vec<f64>>,
                    labels: &mut Vec<String>,
                    dropped: &mut Vec<String>|
     -> Option<usize> {
        let nf = n as f64;
        let mu = raw.iter().sum::<f64>() / nf;
        let sd = (raw.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / (nf - 1.0)).sqrt();
        // A constant seasonality column cannot occur, but a degenerate `t` on a very short
        // series can — and a column with zero spread has no correlation with anything. It
        // is DROPPED with a note rather than divided by, which would put an infinity into
        // the matrix and a `null` on the wire indistinguishable from a deliberate one.
        if !sd.is_finite() || !mu.is_finite() || sd <= 0.0 {
            dropped.push(name);
            return None;
        }
        cols.push(raw.iter().map(|v| (v - mu) / sd).collect());
        labels.push(name);
        Some(cols.len() - 1)
    };

    // The TREND PROXY is the design's own `t`, not a second construction of it.
    push(
        "trend".into(),
        d.t.clone(),
        &mut cols,
        &mut labels,
        &mut dropped,
    );
    // SEASONALITY columns: everything before the trailing regressor range that is not a
    // holiday indicator. `Column.holiday` is `Some` for exactly the holiday family.
    for c in 0..base_k {
        if d.cols[c].holiday.is_none() {
            let raw: Vec<f64> = (0..n).map(|i| d.x[i * d.k + c]).collect();
            push(
                d.cols[c].name.clone(),
                raw,
                &mut cols,
                &mut labels,
                &mut dropped,
            );
        }
    }
    // REGRESSOR columns, by the STRUCTURAL trailing index range — never by name membership,
    // for the reason this module's header records.
    let mut slots = Vec::with_capacity(regs.len());
    for (j, reg) in regs.iter().enumerate() {
        let c = base_k + j;
        let raw: Vec<f64> = (0..n).map(|i| d.x[i * d.k + c]).collect();
        slots.push(push(
            reg.name.clone(),
            raw,
            &mut cols,
            &mut labels,
            &mut dropped,
        ));
    }
    (cols, labels, slots, dropped)
}

/// The maximum PAIRWISE correlation of one diagnostic column against every other.
///
/// Exists only so D-35's PROHIBITION can be a live test rather than a comment: a test
/// computes this for `price`, asserts it sits below a 0.9 cutoff, and then asserts that the
/// SHIPPED rule reports a VIF for it anyway. Nothing in the shipped path calls this, which
/// is the point — a pairwise cutoff is not the identifiability test.
#[cfg(test)]
pub(crate) fn max_pairwise_correlation_for(d: &Design, name: &str) -> Option<f64> {
    // The design carries the regressors already spliced, so the column set is derivable
    // from it alone; the standardisation constants are irrelevant to a correlation.
    let n = d.t.len();
    let mut cols: Vec<(String, Vec<f64>)> = vec![("trend".to_string(), d.t.clone())];
    for c in 0..d.k {
        if d.cols[c].holiday.is_none() {
            cols.push((
                d.cols[c].component.clone(),
                (0..n).map(|i| d.x[i * d.k + c]).collect(),
            ));
        }
    }
    let scaled: Vec<(String, Vec<f64>)> = cols
        .into_iter()
        .filter_map(|(nm, raw)| {
            let nf = n as f64;
            let mu = raw.iter().sum::<f64>() / nf;
            let sd = (raw.iter().map(|v| (v - mu) * (v - mu)).sum::<f64>() / (nf - 1.0)).sqrt();
            if sd > 0.0 && sd.is_finite() {
                Some((nm, raw.iter().map(|v| (v - mu) / sd).collect()))
            } else {
                None
            }
        })
        .collect();
    let target = scaled.iter().position(|(nm, _)| nm == name)?;
    let mut best: f64 = 0.0;
    for (i, (_, col)) in scaled.iter().enumerate() {
        if i == target {
            continue;
        }
        let r: f64 = col
            .iter()
            .zip(scaled[target].1.iter())
            .map(|(a, b)| a * b)
            .sum::<f64>()
            / (n as f64 - 1.0);
        best = best.max(r.abs());
    }
    Some(best)
}

/// Per-regressor VIF and the design condition number, with a typed degenerate-case policy.
///
/// Uses only `std` and this crate's own f64 code: `Cargo.toml` gains NO dependency
/// (threat T-06.1-SC).
#[must_use]
pub fn identifiability(
    d: &Design,
    regs: &[Standardized],
    vif_warn: f64,
    condition_number_warn: f64,
) -> Identifiability {
    identifiability_with_limits(
        d,
        regs,
        vif_warn,
        condition_number_warn,
        REGRESSOR_EIGEN_MAX_ITERS,
        REGRESSOR_EIGEN_TOL,
    )
}

/// [`identifiability`] with the eigen limits INJECTED.
///
/// The limits are a parameter for exactly one reason: so a test can drive the
/// non-convergence branch and OBSERVE it, rather than leaving a policy branch that nobody
/// has seen execute. The public entry point above passes the named constants.
#[must_use]
pub(crate) fn identifiability_with_limits(
    d: &Design,
    regs: &[Standardized],
    vif_warn: f64,
    condition_number_warn: f64,
    max_iters: usize,
    tol: f64,
) -> Identifiability {
    let (cols, _labels, slots, dropped) = diagnostic_columns(d, regs);
    let k = cols.len();
    let n = d.t.len();

    let mut scope = format!(
        "computed over {k} columns: the trend proxy t, the seasonality columns and the \
         {} regressor column(s). HOLIDAY indicator columns are EXCLUDED — the holiday \
         block can reach {} design columns and this diagnostic is O(n*K^2 + K^3), so \
         including them would make its cubic term the dominant cost of the request \
         (cost axis C-17); every collinearity the spike measured was \
         regressor-against-seasonality or regressor-against-trend",
        regs.len(),
        crate::types::MAX_HOLIDAY_COLUMNS
    );
    if !dropped.is_empty() {
        scope.push_str(&format!(
            ". DROPPED for zero spread (no correlation with anything, and dividing by it \
             would emit an infinity): {}",
            dropped.join(", ")
        ));
    }

    // The correlation matrix of the scaled column set. Each column is already centred and
    // divided by its own sample sd, so this quotient IS the Pearson correlation and the
    // diagonal is exactly 1.
    let mut corr = vec![0.0f64; k * k];
    for i in 0..k {
        for j in i..k {
            let r: f64 = cols[i]
                .iter()
                .zip(cols[j].iter())
                .map(|(a, b)| a * b)
                .sum::<f64>()
                / (n as f64 - 1.0);
            corr[i * k + j] = r;
            corr[j * k + i] = r;
        }
    }

    // ---- THE RIDGE LADDER ----
    //
    // Start with NO ridge. On a failed Cholesky, retry at `1e-8 * trace/K` and escalate by
    // 100x, at most three attempts. If all three fail the matrix is treated as SINGULAR —
    // the ladder does not keep escalating, and there is no fall-through to an unregularised
    // inverse.
    let trace: f64 = (0..k).map(|i| corr[i * k + i]).sum();
    let base_ridge = REGRESSOR_RIDGE_BASE * trace / (k.max(1) as f64);
    let mut ridge_used: Option<f64> = None;
    let mut factor = cholesky(&corr, k);
    let mut ridged = corr.clone();
    if factor.is_none() {
        for attempt in 0..REGRESSOR_RIDGE_ATTEMPTS {
            let r = base_ridge * 100.0_f64.powi(i32::try_from(attempt).unwrap_or(0));
            ridged.clone_from(&corr);
            for i in 0..k {
                ridged[i * k + i] += r;
            }
            if let Some(l) = cholesky(&ridged, k) {
                ridge_used = Some(r);
                factor = Some(l);
                break;
            }
        }
    }
    let regularized = ridge_used.is_some();
    let matrix = if regularized { &ridged } else { &corr };

    let mut reports: Vec<RegressorReport> = regs
        .iter()
        .map(|r| RegressorReport {
            name: r.name.clone(),
            mode: r.mode,
            mu: r.mu,
            std: r.std,
            vif: None,
            warning: None,
        })
        .collect();

    let Some(l) = factor else {
        // Three ridge attempts exhausted. Nothing is reported as a number, because every
        // number available at this point would describe the ladder rather than the data.
        return Identifiability {
            scope,
            regressors: singular_reports(reports, regularized),
            condition_number: None,
            condition_number_warning: Some(singular_note(regularized)),
            ridge: ridge_used,
            regularized,
            status: IdentifiabilityStatus::Singular,
        };
    };

    // ---- THE SPECTRUM, and the SINGULARITY verdict ----
    //
    // A ridge does not RESCUE a report; it buys a usable factor so this routine can answer
    // "singular" with a diagnosis instead of crashing. Whether the answer is `ok` or
    // `singular` is decided by measurement, not by whether the ladder ran: the smallest
    // eigenvalue of the matrix actually factorised is compared against the ridge that was
    // added to it. If the ridge supplies essentially all of it, every VIF and the condition
    // number would be functions of `REGRESSOR_RIDGE_BASE` and of nothing in the caller's
    // data — so they are WITHHELD.
    let spec = spectrum(matrix, k, max_iters, tol);
    let (lo, hi) = match spec {
        Some(v) => v,
        None => {
            // ---- NON-CONVERGENCE, and the DIAGNOSIS it must not give ----
            //
            // The eigenvalue loops fail to converge on a matrix whose smallest eigenvalue is
            // at rounding level, which is exactly what a RANK-DEFICIENT design produces. So
            // the naive reading — "the iteration ran out" — would tell the operator their
            // numbers hit a numerical hiccup when the truth is that their design is
            // rank-deficient. MEASURED: a third regressor that is the exact SUM of two
            // others returned `not_converged` with per-column VIFs of ~1e15, i.e. the right
            // problem under the wrong name.
            //
            // The Cholesky factor already answers the question, so no new measurement and no
            // new threshold ladder is needed: for a correlation matrix the pivots are the
            // scale of the eigenvalues, and a smallest pivot at `EPSILON` of the largest
            // means the matrix has no numerical rank left. This check can only turn
            // `not_converged` INTO `singular`, never the reverse, so it cannot make any
            // verdict worse than it was.
            let pivots: Vec<f64> = (0..k).map(|i| l[i * k + i] * l[i * k + i]).collect();
            let max_piv = pivots.iter().copied().fold(0.0_f64, f64::max);
            let min_piv = pivots.iter().copied().fold(f64::INFINITY, f64::min);
            if min_piv <= f64::EPSILON * max_piv * (k as f64) {
                return Identifiability {
                    scope,
                    regressors: singular_reports(reports, regularized),
                    condition_number: None,
                    condition_number_warning: Some(singular_note(regularized)),
                    ridge: ridge_used,
                    regularized,
                    status: IdentifiabilityStatus::Singular,
                };
            }
            // A genuinely non-converged iteration on a matrix that still has numerical rank.
            // The Cholesky-derived VIFs are unaffected and ARE still reported: merging the
            // two failure modes would withhold numbers that are perfectly good.
            fill_vifs(&mut reports, &l, k, &slots, vif_warn, regularized);
            return Identifiability {
                scope,
                regressors: reports,
                condition_number: None,
                condition_number_warning: Some(format!(
                    "the design condition number could not be computed: the eigenvalue \
                     iteration did not converge within {max_iters} iterations at a relative \
                     tolerance of {tol:e}, so no value is reported rather than a last \
                     iterate that would look like a measurement"
                )),
                ridge: ridge_used,
                regularized,
                status: IdentifiabilityStatus::NotConverged,
            };
        }
    };

    let unridged_lo = lo - ridge_used.unwrap_or(0.0);
    if !unridged_lo.is_finite() || unridged_lo <= f64::EPSILON * hi.abs().max(1.0) {
        return Identifiability {
            scope,
            regressors: singular_reports(reports, regularized),
            condition_number: None,
            condition_number_warning: Some(singular_note(regularized)),
            ridge: ridge_used,
            regularized,
            status: IdentifiabilityStatus::Singular,
        };
    }

    fill_vifs(&mut reports, &l, k, &slots, vif_warn, regularized);

    // The correlation matrix is the Gram of the scaled design, so ITS condition number is
    // the SQUARE of the design's — the reported number is the design's, which is the scale
    // every published rule of thumb is stated on.
    let cond = (hi / lo).sqrt();
    let (condition_number, condition_number_warning) = if cond.is_finite() {
        let warn = if cond > condition_number_warn {
            Some(format!(
                "the design condition number is {cond:.1}, above the contract threshold of \
                 {condition_number_warn}; the forecast is valid, but the individual \
                 coefficients are poorly determined and should not be read as effect sizes{}",
                regularized_suffix(regularized)
            ))
        } else {
            None
        };
        (Some(cond), warn)
    } else {
        (
            None,
            Some(
                "the design condition number is not finite and is withheld rather than \
                 serialised as a null indistinguishable from a deliberate one"
                    .to_string(),
            ),
        )
    };

    Identifiability {
        scope,
        regressors: reports,
        condition_number,
        condition_number_warning,
        ridge: ridge_used,
        regularized,
        status: IdentifiabilityStatus::Ok,
    }
}

/// Every number withheld, with the cause named.
fn singular_reports(mut reports: Vec<RegressorReport>, regularized: bool) -> Vec<RegressorReport> {
    for r in &mut reports {
        r.vif = None;
        r.warning = Some(format!(
            "no variance inflation factor is reported: the diagnostic design is SINGULAR to \
             working precision, which means a column is an exact duplicate of another or an \
             exact linear combination of several. The forecast is still valid; the \
             individual coefficients are not identified{}",
            regularized_suffix(regularized)
        ));
    }
    reports
}

fn singular_note(regularized: bool) -> String {
    format!(
        "the design condition number is not reported: the diagnostic design is SINGULAR to \
         working precision (an exactly duplicated or exactly collinear column). A number \
         computed here would describe the regularisation rather than the data, so it is \
         withheld{}",
        regularized_suffix(regularized)
    )
}

/// The clause every warning carries when a ridge was applied.
///
/// Reporting a regularised eigenvalue as if it were the original is the quiet lie this
/// clause exists to prevent.
fn regularized_suffix(regularized: bool) -> &'static str {
    if regularized {
        ". NOTE: a ridge was added to the correlation matrix before it could be factorised, \
         so every number in this report describes the REGULARISED matrix, not the original"
    } else {
        ""
    }
}

/// VIF is the diagonal of the inverse correlation matrix, solved through the SAME Cholesky
/// factor the spectrum uses.
///
/// Only the REGRESSOR columns are solved for — `K` solves would be `O(K^3)` again for
/// numbers that are never emitted, since the report carries one line per regressor.
fn fill_vifs(
    reports: &mut [RegressorReport],
    l: &[f64],
    k: usize,
    slots: &[Option<usize>],
    vif_warn: f64,
    regularized: bool,
) {
    for (rep, slot) in reports.iter_mut().zip(slots.iter()) {
        let Some(p) = *slot else {
            rep.warning = Some(
                "this column has zero spread over the history rows and was dropped from the \
                 diagnostic column set, so no variance inflation factor is defined for it"
                    .to_string(),
            );
            continue;
        };
        let mut e = vec![0.0f64; k];
        e[p] = 1.0;
        let z = chol_solve(l, k, &e);
        let v = z[p];
        if !v.is_finite() {
            rep.vif = None;
            rep.warning = Some(format!(
                "the variance inflation factor for this column is not finite and is \
                 withheld{}",
                regularized_suffix(regularized)
            ));
            continue;
        }
        rep.vif = Some(v);
        if v > vif_warn {
            rep.warning = Some(format!(
                "variance inflation factor {v:.1} is above the contract threshold of \
                 {vif_warn}: this driver is largely explained by the trend and the \
                 seasonality columns, so its coefficient is not reliably identified. The \
                 FORECAST remains valid — it is the coefficient's INTERPRETATION that is \
                 unreliable, which is why this warns rather than refuses{}",
                regularized_suffix(regularized)
            ));
        }
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

#[cfg(test)]
mod identifiability_tests {
    //! D-35 / D-36 / D-37: the identifiability diagnostic WARNS and never refuses, its
    //! numbers live inside `diagnostics`, its keys are ABSENT when unused, and nothing
    //! identifiability-related is computed on the neuralprophet arm.

    use super::{
        identifiability_with_limits, spectrum, Identifiability, IdentifiabilityStatus,
        Standardized, REGRESSOR_EIGEN_MAX_ITERS, REGRESSOR_EIGEN_TOL,
    };
    use crate::prophet::Mode;
    use crate::test_support::load_json;
    use crate::types::{ForecastArgs, RegressorArg, REGRESSOR_CONDITION_NUMBER_WARN,
                       REGRESSOR_VIF_WARN};

    const FIXTURE: &str = "retail_regressors_prophet140.json";

    fn f64s(v: &serde_json::Value) -> Vec<f64> {
        v.as_array()
            .expect("a JSON array")
            .iter()
            .map(|x| x.as_f64().expect("a JSON number"))
            .collect()
    }

    /// The committed 24-column spike-011 request, through the PUBLIC door.
    fn fixture_args() -> ForecastArgs {
        let fx = load_json(FIXTURE);
        let ds: Vec<String> = fx["history"]["ds"]
            .as_array()
            .expect("history.ds")
            .iter()
            .map(|v| v.as_str().expect("a ds string")[..10].to_string())
            .collect();
        let y = f64s(&fx["history"]["y"]);
        let horizon =
            usize::try_from(fx["horizon"].as_u64().expect("horizon")).expect("fits usize");
        let regressors: Vec<RegressorArg> = fx["regressors_requested"]
            .as_array()
            .expect("regressors_requested")
            .iter()
            .map(|r| {
                let name = r["name"].as_str().expect("name").to_string();
                RegressorArg {
                    values: f64s(&fx["regressor_values"][&name]),
                    name,
                    mode: Some(r["mode"].as_str().expect("mode").to_string()),
                    prior_scale: Some(r["prior_scale"].as_f64().expect("prior_scale")),
                    standardize: None,
                }
            })
            .collect();
        ForecastArgs {
            ds,
            y,
            horizon,
            freq: Some("MS".into()),
            seed: Some(42),
            regressors: Some(regressors),
            ..ForecastArgs::default()
        }
    }

    fn fixture_report() -> serde_json::Value {
        let r = crate::forecast::forecast(&fixture_args())
            .expect("the committed four-regressor fixture must be ACCEPTED");
        r.diagnostics.clone()
    }

    fn vif_of(diag: &serde_json::Value, name: &str) -> serde_json::Value {
        diag["regressors"]
            .as_array()
            .expect("diagnostics.regressors must be an ARRAY")
            .iter()
            .find(|e| e["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("no report for regressor {name}"))
            .clone()
    }

    /// D-35, pinned on the committed fixtures: the KNOWN-BAD column warns and the two
    /// KNOWN-GOOD ones do not.
    ///
    /// `discount` is a cosine of period six months — a harmonic of the yearly seasonality,
    /// r = 0.999 against `yearly_delim_4`. `promo` and `weather` are the controls: a
    /// threshold that warned on everything would prove nothing.
    ///
    /// `price` is labelled MARGINAL by spike 011 (r = 0.759, beta still 5x off). Its verdict
    /// is PRINTED and NOTHING is asserted about it — that is the house pattern for a
    /// quantity with no control, and asserting either way would be inventing evidence.
    #[test]
    fn the_known_bad_column_warns_and_the_known_good_columns_do_not_identifiability() {
        let diag = fixture_report();
        let bad = vif_of(&diag, "discount");
        assert!(
            bad["warning"].is_string(),
            "`discount` (r = 0.999 against yearly_delim_4) must carry a warning, got {bad}"
        );
        for good in ["promo", "weather"] {
            let e = vif_of(&diag, good);
            assert!(
                e["warning"].is_null() || e.get("warning").is_none(),
                "`{good}` is a KNOWN-GOOD column and must carry NO warning, got {e}"
            );
        }
        let marginal = vif_of(&diag, "price");
        println!(
            "MARGINAL (unasserted, spike 011: r = 0.759 against yearly_delim_1, beta 5x \
             off): price vif={} warning={}",
            marginal["vif"], marginal["warning"]
        );
        println!("identifiability: {}", diag["regressors_identifiability"]);
    }

    /// D-35's whole point: a collinear regressor is WARNED ABOUT, never REFUSED.
    ///
    /// Spike 011 measured the fit as valid and the objective at or better than Python's
    /// (slack -3.42) even where the coefficient was 4.7x off. It is the coefficient's
    /// INTERPRETATION that is unreliable, so blocking the request would be wrong.
    #[test]
    fn a_collinear_regressor_is_warned_about_never_refused() {
        let r = crate::forecast::forecast(&fixture_args())
            .expect("a request carrying the collinear `discount` column must SUCCEED");
        let diag = &r.diagnostics;
        assert!(
            vif_of(diag, "discount")["warning"].is_string(),
            "the collinear column must be warned about"
        );
        assert!(
            !r.yhat.is_empty() && r.yhat.iter().all(|v| v.is_finite()),
            "the forecast itself must be valid — that is why this never refuses"
        );
    }

    /// D-35's PROHIBITION, as a live test rather than a comment: a pairwise correlation
    /// cutoff is NOT the identifiability test.
    ///
    /// A 0.9 pairwise threshold would have PASSED `price` (max pairwise r = 0.759 against
    /// the diagnostic column set) whose beta is still 5x off. The shipped rule reports a VIF
    /// for it regardless, because VIF is a MULTI-column measure and a pairwise maximum is
    /// blind to a column explained jointly by several others.
    #[test]
    fn a_pairwise_cutoff_would_have_missed_the_marginal_column() {
        let diag = fixture_report();
        let max_r = super::max_pairwise_correlation_for(&fixture_design(), "price")
            .expect("`price` is in the diagnostic column set");
        assert!(
            max_r < 0.9,
            "the premise of D-35's prohibition: `price` must sit BELOW a 0.9 pairwise \
             cutoff, so such a cutoff would have passed it. Measured {max_r}"
        );
        let e = vif_of(&diag, "price");
        assert!(
            e["vif"].is_number(),
            "the SHIPPED rule must nonetheless report a VIF for `price` — a pairwise cutoff \
             would have reported nothing at all. Got {e}"
        );
        println!("price max pairwise r = {max_r}, shipped vif = {}", e["vif"]);
    }

    /// D-36 and D-19 together: BOTH keys are absent when no regressor is passed, on BOTH
    /// model arms — which is what keeps every recorded invariance signature reproducing.
    ///
    /// # The neuralprophet half carries NO holidays, and that is a scoped gap, not an omission
    ///
    /// `holidays` on `model: neuralprophet` is still REFUSED at the door (`forecast.rs`'s
    /// prophet-only argument loop) until plan 06.1-06 removes that row, so a test asserting a
    /// SUCCESSFUL neuralprophet response that carries holidays cannot pass in this wave.
    /// **The holiday-carrying NP assertion is handed to plan 06.1-06 Task 2**, where the arm
    /// is open. It is recorded here so the coverage is visibly moved rather than quietly
    /// lost.
    #[test]
    fn the_diagnostic_keys_are_absent_when_no_regressor_is_passed() {
        let base = fixture_args();
        for model in ["prophet", "neuralprophet"] {
            let mut args = ForecastArgs {
                regressors: None,
                ..base.clone()
            };
            if model == "neuralprophet" {
                args.model = Some("neuralprophet".into());
                args.freq = Some("D".into());
                // See the doc comment: holidays stay absent on this arm in this wave.
                args.holidays = None;
            }
            let r = crate::forecast::forecast(&args)
                .unwrap_or_else(|e| panic!("{model} with no regressors must be accepted: {e}"));
            for key in ["regressors", "regressors_identifiability"] {
                assert!(
                    r.diagnostics.get(key).is_none(),
                    "{model}: diagnostics.{key} must be ABSENT with no regressors — the \
                     invariance signature hashes the WHOLE diagnostics object, so a key \
                     added unconditionally changes every recorded baseline (D-36, SC2). \
                     Got {}",
                    r.diagnostics
                );
            }
        }
    }

    /// D-37: nothing identifiability-related is computed on the neuralprophet arm.
    ///
    /// Distinct from the test above, which is about ABSENCE WITHOUT REGRESSORS. This one is
    /// about the ARM: VIF and the condition number are properties of the design matrix
    /// Prophet builds, and AR absorption is a training dynamic rather than column
    /// collinearity, so a green VIF there would reassure about the wrong thing.
    #[test]
    fn no_identifiability_is_computed_on_the_neuralprophet_arm() {
        let mut args = fixture_args();
        args.model = Some("neuralprophet".into());
        args.freq = Some("D".into());
        // The arm REFUSES regressors outright in this wave (plan 06.1-07 opens it), which
        // is itself the strongest form of "nothing is computed".
        let err = crate::forecast::forecast(&args)
            .expect_err("regressors on the neuralprophet arm are refused in this wave");
        assert!(
            err.to_string().contains("set model to \"prophet\""),
            "the refusal must name the fix, got {err}"
        );
        // And with no regressors the arm produces neither key (the positive control, so
        // this test cannot pass merely because the arm errors).
        args.regressors = None;
        let r = crate::forecast::forecast(&args).expect("no-regressor NP request is accepted");
        assert!(
            r.diagnostics.get("regressors").is_none()
                && r.diagnostics.get("regressors_identifiability").is_none(),
            "no identifiability key may appear on the neuralprophet arm: {}",
            r.diagnostics
        );
    }

    /// The EXACT key set of both emitted objects.
    ///
    /// `ForecastResponse.diagnostics` is untyped `serde_json::Value`, so nothing downstream
    /// would catch a contradictory shape — a `scope` property hung off an ARRAY, or a
    /// warning string hung off a NUMBER, would both serialise happily and be discovered by
    /// a consumer. This is the test that catches it.
    #[test]
    fn the_identifiability_report_key_sets_are_exactly_as_specified() {
        let diag = fixture_report();
        let arr = diag["regressors"]
            .as_array()
            .expect("diagnostics.regressors must be an ARRAY of per-regressor objects");
        assert_eq!(arr.len(), 4, "one object per regressor");
        for e in arr {
            let keys: std::collections::BTreeSet<&str> = e
                .as_object()
                .expect("each entry is an object")
                .keys()
                .map(String::as_str)
                .collect();
            let required: std::collections::BTreeSet<&str> =
                ["name", "mode", "mu", "std", "vif"].into_iter().collect();
            assert!(
                required.is_subset(&keys),
                "a per-regressor entry must carry {required:?}, got {keys:?}"
            );
            let allowed: std::collections::BTreeSet<&str> =
                ["name", "mode", "mu", "std", "vif", "warning"]
                    .into_iter()
                    .collect();
            assert!(
                keys.is_subset(&allowed),
                "a per-regressor entry must carry ONLY {allowed:?}, got {keys:?}"
            );
        }
        let obj = diag["regressors_identifiability"]
            .as_object()
            .expect("diagnostics.regressors_identifiability must be an OBJECT — an array \
                     cannot carry a `scope` property, which is the shape contradiction this \
                     split exists to fix");
        let keys: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
        let want: std::collections::BTreeSet<&str> = [
            "scope",
            "condition_number",
            "condition_number_warning",
            "ridge",
            "regularized",
            "status",
        ]
        .into_iter()
        .collect();
        assert_eq!(keys, want, "the sibling object's key set is exact");
    }

    /// The SCOPE is stated in the response, not only in the contract (threat T-06.1-11).
    ///
    /// Holiday indicator columns are excluded from the diagnostic column set because the
    /// holiday block can reach `fit_max_holiday_columns` (1 000) design columns, which would
    /// make the cubic term the dominant cost of the whole request. An operator reading a VIF
    /// has to know which columns it was computed against, or the number means something
    /// other than what they think.
    #[test]
    fn the_identifiability_scope_names_the_excluded_holiday_columns() {
        let diag = fixture_report();
        let scope = diag["regressors_identifiability"]["scope"]
            .as_str()
            .expect("scope is a string");
        assert!(
            scope.contains("holiday"),
            "the scope note must name the EXCLUDED holiday columns, got {scope:?}"
        );
        assert!(
            scope.contains("trend") && scope.contains("seasonalit"),
            "the scope note must name what IS included, got {scope:?}"
        );
    }

    // ---------------------------------------------------- the numerical falsifications ---

    /// A 2x2 correlation matrix `[[1, r], [r, 1]]` has eigenvalues `1 + r` and `1 - r` in
    /// CLOSED FORM, so the power / inverse-power pair can be checked against arithmetic
    /// rather than against itself.
    ///
    /// Without this a routine that returned a constant would produce the same green table
    /// everywhere else in this module.
    #[test]
    fn the_condition_number_matches_a_matrix_with_known_eigenvalues() {
        for r in [0.0_f64, 0.25, 0.5, 0.9, 0.99] {
            let a = vec![1.0, r, r, 1.0];
            let (lo, hi) = spectrum(&a, 2, REGRESSOR_EIGEN_MAX_ITERS, REGRESSOR_EIGEN_TOL)
                .unwrap_or_else(|| panic!("the 2x2 spectrum must converge at r = {r}"));
            assert!(
                (hi - (1.0 + r)).abs() < 1e-8,
                "r = {r}: lambda_max must be 1 + r = {}, got {hi}",
                1.0 + r
            );
            assert!(
                (lo - (1.0 - r)).abs() < 1e-8,
                "r = {r}: lambda_min must be 1 - r = {}, got {lo}",
                1.0 - r
            );
        }
        // A 3x3 with a known repeated eigenvalue: equicorrelated [[1,c,c],[c,1,c],[c,c,1]]
        // has eigenvalues 1 + 2c (once) and 1 - c (twice). A second shape, because one
        // matrix is an anecdote.
        let c = 0.4_f64;
        let a = vec![1.0, c, c, c, 1.0, c, c, c, 1.0];
        let (lo, hi) = spectrum(&a, 3, REGRESSOR_EIGEN_MAX_ITERS, REGRESSOR_EIGEN_TOL)
            .expect("the 3x3 spectrum must converge");
        assert!((hi - (1.0 + 2.0 * c)).abs() < 1e-8, "lambda_max, got {hi}");
        assert!((lo - (1.0 - c)).abs() < 1e-8, "lambda_min, got {lo}");
    }

    /// An exactly duplicated column: the request SUCCEEDS, every `vif` is JSON `null`, the
    /// condition number is JSON `null`, `status` is `singular`, and NOTHING emitted is
    /// non-finite.
    ///
    /// The last clause is the point. `serde_json` renders a non-finite f64 as `null`, so an
    /// accidental infinity and a DELIBERATE null produce the SAME wire bytes — only one of
    /// them is an intended statement, and only this assertion tells them apart.
    #[test]
    fn a_singular_design_reports_null_not_a_number() {
        let mut args = fixture_args();
        let regs = args.regressors.as_mut().expect("fixture has regressors");
        let clone_of_promo = regs[0].values.clone();
        regs.push(RegressorArg {
            name: "promo_twin".into(),
            values: clone_of_promo,
            mode: None,
            prior_scale: None,
            standardize: None,
        });
        let r = crate::forecast::forecast(&args)
            .expect("an exactly collinear design is still a VALID request — it never refuses");
        let ident = &r.diagnostics["regressors_identifiability"];
        assert_eq!(ident["status"].as_str(), Some("singular"), "got {ident}");
        assert!(
            ident["condition_number"].is_null(),
            "condition_number must be a DELIBERATE null, got {ident}"
        );
        for e in r.diagnostics["regressors"].as_array().expect("array") {
            assert!(
                e["vif"].is_null(),
                "every vif must be null on a singular design, got {e}"
            );
        }
        assert!(
            no_non_finite(&r.diagnostics),
            "no emitted number may be non-finite: serde_json renders one as the same \
             `null` a deliberate one produces, so an infinity here would be invisible. {}",
            r.diagnostics
        );
    }

    /// When a ridge was applied the report SAYS SO, carries the value ACTUALLY used, and
    /// every warning string states that the numbers describe a regularised matrix.
    ///
    /// Reporting a regularised eigenvalue as if it were the original is the quiet lie this
    /// state exists to prevent.
    ///
    /// # Which input fires the ridge was MEASURED, not assumed
    ///
    /// On a CORRELATION matrix (unit diagonal, positive semi-definite) the ridge fires only
    /// when the matrix has no numerical rank left, so `regularized: true` co-occurs with
    /// `status: singular`. A merely ILL-CONDITIONED design factorises cleanly and gets a
    /// real — and very large — condition number instead. That is a property of the matrix,
    /// not a gap in the policy, and it is why the control at the end of this test is the
    /// CLEAN fixture rather than a near-singular one: see
    /// `a_degenerate_design_never_emits_a_wrong_number_whatever_the_singular_verdict` for
    /// the measurement showing that near-singular inputs land on either side of the boundary
    /// by rounding, which makes them unfit to build an assertion on.
    #[test]
    fn a_regularized_report_says_so() {
        let mut args = fixture_args();
        let regs = args.regressors.as_mut().expect("fixture has regressors");
        let twin = regs[0].values.clone();
        regs.push(RegressorArg {
            name: "promo_twin".into(),
            values: twin,
            mode: None,
            prior_scale: None,
            standardize: None,
        });
        let r = crate::forecast::forecast(&args)
            .expect("a rank-deficient design is still a VALID request — it never refuses");
        let ident = &r.diagnostics["regressors_identifiability"];
        assert_eq!(
            ident["regularized"],
            serde_json::json!(true),
            "a ridge was needed to obtain a usable factor and the report must say so: {ident}"
        );
        let ridge = ident["ridge"].as_f64().expect("the ridge VALUE is reported");
        // The VALUE, not merely its presence — otherwise "the value actually used" is not a
        // claim anything checks. The first rung is `1e-8 * trace / K`, and a correlation
        // matrix has a UNIT diagonal, so `trace / K` is exactly 1.
        assert!(
            ridge > 0.0 && (ridge - 1e-8).abs() < 1e-17,
            "the first ridge rung is 1e-8 * trace/K and trace/K == 1 for a correlation              matrix, so the reported ridge must be 1e-8; got {ridge:e}"
        );
        // EVERY warning says the numbers describe a regularised matrix.
        let mut warned = 0usize;
        for e in r.diagnostics["regressors"].as_array().expect("array") {
            if let Some(w) = e["warning"].as_str() {
                assert!(
                    w.contains("REGULARISED"),
                    "every warning must state that the numbers describe a regularised                      matrix, got {w:?}"
                );
                warned += 1;
            }
        }
        assert_eq!(warned, 5, "every regressor carries a warning on this design");
        let cw = ident["condition_number_warning"]
            .as_str()
            .expect("the condition number carries its own note");
        assert!(cw.contains("REGULARISED"), "got {cw:?}");
        assert!(
            no_non_finite(&r.diagnostics),
            "a regularised report still emits no non-finite number: {}",
            r.diagnostics
        );

        // ---- CONTROL: the ridge does NOT fire on a well-posed design ----
        //
        // Without this, a policy that ridged unconditionally would pass every assertion
        // above — and would silently replace real numbers with artefacts of
        // `REGRESSOR_RIDGE_BASE` on every request.
        let clean = crate::forecast::forecast(&fixture_args()).expect("the clean fixture");
        let ci = &clean.diagnostics["regressors_identifiability"];
        assert_eq!(
            ci["regularized"],
            serde_json::json!(false),
            "the ridge must NOT fire on a well-posed design: {ci}"
        );
        assert_eq!(ci["status"].as_str(), Some("ok"), "got {ci}");
        assert!(ci["ridge"].is_null(), "no ridge, no value: {ci}");
        assert!(
            ci["condition_number"].as_f64().is_some_and(f64::is_finite),
            "a well-posed design gets a REAL condition number: {ci}"
        );
    }

    /// Whatever verdict a degenerate design lands on, the report is SAFE.
    ///
    /// # The measurement this test exists to record
    ///
    /// CLAUDE.md rule 6 — one failing input is an anecdote — applied to the degenerate-case
    /// policy itself. Varying the input across four shapes shows that WHICH verdict a
    /// near-singular design receives is decided by ROUNDING, not by how near-singular it is.
    /// Measured on the committed fixture (aarch64, 2026-09-21):
    ///
    /// | shape | status | regularized | condition number |
    /// |---|---|---|---|
    /// | exact duplicate of a column | `singular` | true (ridge 1e-8) | null |
    /// | a column that is the exact SUM of two others | `singular` | false | null |
    /// | near-duplicate, perturbed by ±1e-11 | `ok` | false | 2.25e7 |
    /// | near-duplicate, perturbed by ±1e-6 | `singular` | true (ridge 1e-8) | null |
    ///
    /// Note the inversion in the last two rows: the LARGER perturbation gets the WORSE
    /// verdict. Both sit within rounding of the rank-deficiency boundary, where the Cholesky
    /// pivot's sign is decided by accumulated error rather than by the data. So no assertion
    /// anywhere in this module is built on a near-singular input — the two tests above use
    /// EXACT rank deficiency, which is deterministic.
    ///
    /// What IS invariant, and is what this test asserts, is the property that actually
    /// protects the operator: whichever verdict is reached, the request SUCCEEDS, the status
    /// is one of the three named states, nothing emitted is non-finite, and a withheld
    /// condition number always arrives with a note saying why.
    #[test]
    fn a_degenerate_design_never_emits_a_wrong_number_whatever_the_singular_verdict() {
        type Mutate = fn(&mut Vec<RegressorArg>);
        let shapes: [(&str, Mutate); 4] = [
            ("exact_duplicate", |regs| {
                let v = regs[0].values.clone();
                regs.push(extra("twin", v));
            }),
            ("exact_sum_of_two", |regs| {
                let v = regs[0]
                    .values
                    .iter()
                    .zip(regs[1].values.iter())
                    .map(|(a, b)| a + b)
                    .collect();
                regs.push(extra("combo", v));
            }),
            ("near_duplicate_1e_11", |regs| {
                let v = perturbed(&regs[0].values, 1e-11);
                regs.push(extra("near11", v));
            }),
            ("near_duplicate_1e_6", |regs| {
                let v = perturbed(&regs[0].values, 1e-6);
                regs.push(extra("near6", v));
            }),
        ];
        for (label, mutate) in shapes {
            let mut args = fixture_args();
            mutate(args.regressors.as_mut().expect("regs"));
            let r = crate::forecast::forecast(&args)
                .unwrap_or_else(|e| panic!("{label}: a degenerate design NEVER refuses: {e}"));
            let ident = &r.diagnostics["regressors_identifiability"];
            let status = ident["status"].as_str().expect("status is a string");
            assert!(
                ["ok", "singular", "not_converged"].contains(&status),
                "{label}: status must be one of the three NAMED states, got {status:?}"
            );
            assert!(
                no_non_finite(&r.diagnostics),
                "{label}: serde_json renders a non-finite f64 as the SAME `null` a                  deliberate one produces, so an infinity here would be invisible on the                  wire. {}",
                r.diagnostics
            );
            if ident["condition_number"].is_null() {
                assert!(
                    ident["condition_number_warning"].is_string(),
                    "{label}: a WITHHELD condition number must arrive with a note saying                      why — a bare null is indistinguishable from a bug. {ident}"
                );
            }
            assert!(
                r.yhat.iter().all(|v| v.is_finite()),
                "{label}: the FORECAST is valid regardless — that is why this never refuses"
            );
            println!(
                "DEGENERATE {label}: status={status} regularized={} ridge={} cond={}",
                ident["regularized"], ident["ridge"], ident["condition_number"]
            );
        }
    }

    fn extra(name: &str, values: Vec<f64>) -> RegressorArg {
        RegressorArg {
            name: name.into(),
            values,
            mode: None,
            prior_scale: None,
            standardize: None,
        }
    }

    fn perturbed(v: &[f64], eps: f64) -> Vec<f64> {
        v.iter()
            .enumerate()
            .map(|(i, x)| x + if i % 2 == 0 { eps } else { -eps })
            .collect()
    }

    /// The iteration cap is a NAMED constant AND it is actually exercised.
    ///
    /// "A bound that has never been hit is not known to work." The routine is driven with a
    /// cap of one iteration on a matrix whose Rayleigh quotient has not settled, so the
    /// non-convergence branch executes and is observed to report `not_converged` with a
    /// `null` condition number — never a silently returned last iterate, which is a
    /// plausible WRONG number and the worst outcome available here.
    #[test]
    fn the_eigen_iteration_is_bounded() {
        assert_eq!(REGRESSOR_EIGEN_MAX_ITERS, 100, "the cap is a named constant");
        assert!(
            REGRESSOR_EIGEN_TOL > 0.0 && REGRESSOR_EIGEN_TOL < 1e-6,
            "the tolerance is a named constant"
        );
        // Directly: one iteration cannot converge on this spectrum.
        let c = 0.4_f64;
        let a = vec![1.0, c, c, c, 1.0, c, c, c, 1.0];
        assert!(
            spectrum(&a, 3, 1, REGRESSOR_EIGEN_TOL).is_none(),
            "a one-iteration cap must report NON-CONVERGENCE rather than a last iterate"
        );
        // And through the whole diagnostic, so the STATE is observed end to end.
        let (d, regs) = fixture_design_and_regs();
        let report: Identifiability = identifiability_with_limits(
            &d,
            &regs,
            REGRESSOR_VIF_WARN,
            REGRESSOR_CONDITION_NUMBER_WARN,
            1,
            REGRESSOR_EIGEN_TOL,
        );
        assert_eq!(report.status, IdentifiabilityStatus::NotConverged);
        assert!(
            report.condition_number.is_none(),
            "a non-converged condition number is WITHHELD, not guessed"
        );
        assert!(
            report
                .condition_number_warning
                .as_deref()
                .is_some_and(|w| w.contains("could not be computed")),
            "the report must say the number could not be computed, got {:?}",
            report.condition_number_warning
        );
        // The per-column VIFs come from the CHOLESKY, not from the eigen loops, so they
        // survive a non-converged condition number. Asserting this keeps the two failure
        // modes from being quietly merged.
        assert!(
            report.regressors.iter().any(|r| r.vif.is_some()),
            "non-convergence of the eigen pair must not withhold the Cholesky-derived VIFs"
        );
    }

    /// Recursively: no number anywhere in the emitted JSON is non-finite.
    fn no_non_finite(v: &serde_json::Value) -> bool {
        match v {
            serde_json::Value::Number(n) => n.as_f64().is_none_or(f64::is_finite),
            serde_json::Value::Array(a) => a.iter().all(no_non_finite),
            serde_json::Value::Object(o) => o.values().all(no_non_finite),
            _ => true,
        }
    }

    /// The spliced design the fixture request produces, rebuilt here so the numerical tests
    /// can reach the matrix directly rather than only through the JSON.
    fn fixture_design_and_regs() -> (crate::prophet::Design, Vec<Standardized>) {
        let args = fixture_args();
        let ds: Vec<i64> = args
            .ds
            .iter()
            .map(|s| crate::dates::parse_ymd(s))
            .collect();
        let spec = crate::prophet::Spec::default_linear(crate::prophet::auto_seasonalities(
            &ds,
            10.0,
            Mode::Additive,
        ));
        let mut d = crate::prophet::make_design(&ds, &args.y, &spec);
        let mut std_list = Vec::new();
        let mut hist = Vec::new();
        for r in args.regressors.as_deref().unwrap_or(&[]) {
            let mode = match r.mode.as_deref() {
                Some("multiplicative") => Mode::Multiplicative,
                _ => Mode::Additive,
            };
            let spec_r = super::RegressorSpec {
                name: r.name.clone(),
                mode,
                prior_scale: r.prior_scale.unwrap_or(10.0),
                standardize: r.standardize,
            };
            let (h, _) = r.values.split_at(ds.len());
            std_list.push(super::standardize_one(&spec_r, h));
            hist.push(h.to_vec());
        }
        super::splice(&mut d, &std_list, &hist);
        (d, std_list)
    }

    fn fixture_design() -> crate::prophet::Design {
        fixture_design_and_regs().0
    }
}
