//! The SC1 wall, SWEPT over the surface where the cost is actually decided.
//!
//! # Which run is the gate, and which run is not (read this before quoting a green)
//!
//! There are TWO runs of this harness and they claim different things.
//!
//! * The **in-suite run** — a plain `cargo test -p aprender-forecast --lib` — is a **SHAPE
//!   CHECK**. It proves every composition is still ACCEPTED by the door, that
//!   [`max_legal_horizon`] still derives a legal horizon for the frequency the door refuses
//!   at the cap, that each still emits one parsable `SC1 WALL:` line, and that none has
//!   rotted. It runs on whatever profile the caller used and it asserts **NO** SC1 bar. A
//!   green `--lib` run is therefore NOT an SC1 guarantee, and reading it as one is the exact
//!   posture WR-04 describes.
//! * The **gate** is `just forecast-sc1-sweep`. It forces `--release` — the only profile on
//!   which a wall-clock number is the SC1 bar — widens the NeuralProphet row to its
//!   at-the-bound geometry, and asserts the 2 s bar in the harness AND again in the recipe
//!   over the printed lines, so neither can pass vacuously.
//!
//! **The Prophet half of the DEFAULT matrix is already at the bound, and that is a measured
//! choice, not an oversight.** The plan budgeted for a reduced default; the measurement did
//! not require one. The full 18-composition Prophet cross product at the tightest legal
//! history span costs **7.19 s of test time (21 s including the compile) on a debug build**,
//! against a 60 s in-suite budget — so shrinking the horizon would have bought CI headroom
//! nothing needed AND would have stopped exercising `max_legal_horizon` altogether: `"MS"`
//! only clamps when the requested cap exceeds ~841, so a default below that leaves the CR-01
//! axis derivation unrun in the suite. The one row that genuinely cannot be at its bound
//! in-suite is NeuralProphet: 06-15 measured its at-the-bound composition at **45.6 s on a
//! debug profile**, which alone would blow the budget, so it defaults small and the gate
//! widens it by environment.
//!
//! # Why the bar is asserted twice
//!
//! Once here, per composition, on release only; and again in `just forecast-sc1-sweep`, over
//! the `SC1 WALL:` lines it re-parses out of the log. Neither is redundant: the in-harness
//! assertion names the composition, and the recipe's re-check is what catches a harness that
//! silently stopped emitting lines (`SC1 SWEEP OK` reports the count it checked, so a gate
//! that checked zero lines cannot report success).
//!
//! # Why a sweep and not a third bench (WR-04)
//!
//! Each round of this phase added a bench hard-coded to the geometry it was born from:
//! `forecast-bench` sweeps point count with no holidays, `forecast-holiday-bench` sweeps the
//! holiday axis, and `prophet::sampler::logistic_band_wall` was `#[ignore]`d with no recipe
//! and no bar. Nothing in the phase covered `freq`, and `freq` is where CR-01 lived: at the
//! same points and horizon the review measured 0.231 s on `"D"` and **2.334 s** on `"MS"`,
//! because `dates::future_days` multiplies the horizon COUNT by 1 / 7 / ~30.44 and therefore
//! multiplies `t_max`, and therefore the logistic simulation's Poisson mean.
//!
//! So the matrix is a CROSS PRODUCT — freq x growth x holiday shape — and shrinking it for
//! the in-suite run shrinks the POINTS and the HORIZON, never an axis. Dropping an axis is
//! the defect.
//!
//! # Why the bar is release-only (CLAUDE.md rule 2)
//!
//! This crate carries `[profile.dev.package.aprender-forecast] opt-level = 3`, which covers
//! this crate and NOT its dependencies — so a debug wall looks plausible and still is not the
//! SC1 bar. Every line therefore carries `profile=`, derived from `cfg!(debug_assertions)`
//! rather than from intent, and the bar is asserted only when that says `release`.

use crate::dates::{days_from_civil, format_ymd, future_days, parse_date};
use crate::prophet::{auto_seasonalities, changepoint_count, Mode, Spec};
use crate::types::{
    ForecastArgs, HolidayArg, MAX_HOLIDAY_DESIGN_COST, MAX_HOLIDAY_WINDOW, MAX_HORIZON,
    MAX_LOGISTIC_CHANGEPOINT_LAMBDA,
};

/// `.unwrap()` is banned by `.clippy.toml`; an unparseable selector falls back to the
/// default rather than aborting the run with a panic that looks like a measurement.
pub(crate) fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

/// Build holidays whose windows sum to exactly `columns` design columns.
///
/// One holiday cannot exceed `2 * MAX_HOLIDAY_WINDOW + 1` = 731 columns, so a wider request
/// is split across as few holidays as that ceiling allows. Each carries `dates_per_holiday`
/// occurrences spread across the series span.
///
/// THE ONE SPLITTER IN THE CRATE (WR-04). It used to live inside
/// `prophet::design_cost`, where only that one bench could reach it; it lives here so the
/// sweep and every folded single-composition entry point build the same geometry.
pub(crate) fn holidays_for(
    columns: usize,
    dates_per_holiday: usize,
    t0: i64,
    points: usize,
) -> Vec<HolidayArg> {
    let max_width = (2 * MAX_HOLIDAY_WINDOW + 1) as usize;
    let step = (points / dates_per_holiday.max(1)).max(1) as i64;
    let mut remaining = columns;
    let mut out: Vec<HolidayArg> = Vec::new();
    while remaining > 0 {
        let width = remaining.min(max_width);
        remaining -= width;
        let lower = -(((width - 1) / 2) as i64);
        let upper = (width - 1) as i64 + lower;
        out.push(HolidayArg {
            name: format!("h{}", out.len()),
            dates: (0..dates_per_holiday)
                .map(|k| format_ymd(t0 + k as i64 * step))
                .collect(),
            lower_window: lower,
            upper_window: upper,
        });
    }
    out
}

/// `"debug"` or `"release"`, derived from `cfg!(debug_assertions)` and NEVER from intent.
///
/// CLAUDE.md rule 2: a debug wall labelled `release` is the class of error that turns a
/// measurement into a confident wrong answer. Shared so the sweep and every folded
/// single-composition entry point cannot label a run two different ways.
pub(crate) fn profile_token() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

/// Put a request through the door, ASSERT it was accepted, and return it with its wall.
///
/// The acceptance assertion is the point: a composition the door REFUSES is timed at
/// ~0.001 s and looks like the fastest pass in the matrix. Shared so no harness can measure
/// a refusal and report a wall.
pub(crate) fn time_accepted(
    args: &ForecastArgs,
    label: &str,
) -> (crate::types::ForecastResponse, f64) {
    let t = std::time::Instant::now();
    let r = crate::forecast::forecast(args).unwrap_or_else(|e| {
        panic!(
            "the composition [{label}] must be ACCEPTED by the door, or its wall is a \
             meaningless fast pass — the door refused it: {e}"
        )
    });
    let total = t.elapsed().as_secs_f64();
    assert_eq!(r.yhat.len(), args.horizon, "one row per horizon step");
    (r, total)
}

/// The mean uncertainty-band width over the horizon.
///
/// The OUTPUT the logistic changepoint sampler is about: a sampler that draws fewer
/// changepoints than the law asks for spreads its simulated trends less, so the interval
/// comes back narrower than the `interval_width` it advertises. Printed on every line so the
/// bound is visibly refusing COST rather than quietly truncating the simulation — which is
/// the distinction `MAX_LOGISTIC_CHANGEPOINT_LAMBDA`'s doc rests on.
fn mean_band_width(r: &crate::types::ForecastResponse) -> f64 {
    if r.yhat.is_empty() {
        return 0.0;
    }
    r.yhat_upper
        .iter()
        .zip(r.yhat_lower.iter())
        .map(|(u, l)| u - l)
        .sum::<f64>()
        / r.yhat.len() as f64
}

/// A strictly-ascending DAILY series of `points` points, which is the TIGHTEST legal history
/// span for that point count — and therefore the geometry that maximises `t_max` and so the
/// logistic simulation's Poisson mean. This is the geometry `06-REVIEW.md` CR-01 measured.
///
/// THE ONE SERIES BUILDER (WR-04). `prophet::design_cost::holiday_design_wall` built a
/// byte-identical series of its own until this plan folded it onto this function.
pub(crate) fn tight_daily_series(points: usize) -> (Vec<String>, Vec<f64>, i64) {
    let t0 = days_from_civil(2015, 1, 1);
    let ds: Vec<String> = (0..points).map(|i| format_ymd(t0 + i as i64)).collect();
    let y: Vec<f64> = (0..points)
        .map(|i| {
            let t = i as f64;
            10.0 + 0.01 * t + (2.0 * std::f64::consts::PI * t / 7.0).sin()
        })
        .collect();
    (ds, y, t0)
}

/// The Poisson mean the logistic uncertainty simulation would draw per sample, computed the
/// way `forecast::forecast` computes it — through [`changepoint_count`], the SAME function
/// `make_design` is tied to — so the number printed on the line is the number the door
/// bounds rather than a second inline copy of the arithmetic.
fn lambda_for(ds: &[String], horizon: usize, freq: &str) -> f64 {
    let days: Vec<i64> = ds
        .iter()
        .map(|s| parse_date(s).expect("the sweep builds valid dates"))
        .collect();
    let last = days[days.len() - 1];
    let fut = future_days(last, horizon, freq).expect("the sweep builds a valid freq");
    let t_scale = (last - days[0]) as f64;
    let t_max = (fut[fut.len() - 1] - days[0]) as f64 / t_scale;
    let spec = Spec::default_linear(auto_seasonalities(&days, 10.0, Mode::Additive));
    changepoint_count(days.len(), &spec) as f64 * (t_max - 1.0)
}

/// The largest horizon this composition can legally ask for, at or below `cap`.
///
/// The door bounds the PRODUCT `changepoint_count * (t_max - 1)`, and `dates::future_days`
/// multiplies the horizon COUNT by 1 / 7 / ~30.44 for `D` / `W` / `MS` — so the same legal
/// horizon buys a 30x longer future span on one frequency than on another, and "the widest
/// legal horizon" is a different number per frequency. That is the CR-01 axis.
///
/// Searched through [`lambda_for`], which computes the mean through
/// [`changepoint_count`] and [`future_days`] exactly as the door does, rather than through a
/// second copy of the 1 / 7 / 30.44 multipliers — a second copy is the drift this phase has
/// spent three plans closing. `lambda_for` is monotonic non-decreasing in the horizon for
/// every accepted frequency, which is what makes the bisection well-defined.
fn max_legal_horizon(ds: &[String], freq: &str, growth: &str, cap: usize) -> usize {
    let cap = cap.clamp(1, MAX_HORIZON);
    // The lambda bound is LOGISTIC-ONLY at the door: the linear and flat arms never enter
    // `predict`'s simulation, so nothing there is bounded by it.
    if growth != "logistic" || lambda_for(ds, cap, freq) <= MAX_LOGISTIC_CHANGEPOINT_LAMBDA {
        return cap;
    }
    // Invariant: `lo` is legal, `hi` is not. `lo = 1` is legal for every frequency at every
    // point count this sweep builds (one future step over a >= 9-day history keeps t_max
    // near 1), and it is asserted rather than assumed below.
    assert!(
        lambda_for(ds, 1, freq) <= MAX_LOGISTIC_CHANGEPOINT_LAMBDA,
        "freq={freq}: even a horizon of 1 is refused, so this composition has no legal \
         horizon and the sweep would be measuring a refusal"
    );
    let (mut lo, mut hi) = (1usize, cap + 1);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if lambda_for(ds, mid, freq) <= MAX_LOGISTIC_CHANGEPOINT_LAMBDA {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

/// One composition of the matrix.
#[derive(Clone, Copy)]
struct Case {
    model: &'static str,
    freq: &'static str,
    growth: &'static str,
    holidays: bool,
}

/// The realised request for a case, at the tightest legal history span.
struct Built {
    args: ForecastArgs,
    horizon: usize,
    lambda: f64,
    cells: usize,
}

/// Build an ACCEPTED request for any point of the matrix, at the TIGHTEST LEGAL HISTORY SPAN
/// and the WIDEST HORIZON that composition can legally ask for.
///
/// The horizon is DERIVED per composition ([`max_legal_horizon`]), never taken as given: the
/// door's lambda bound is on a product the frequency multiplies, so a single horizon cap is
/// legal on `"D"`, legal on `"W"` and refused on `"MS"` — which is exactly the failure this
/// function's first cut was observed making (the RED of plan 06-16 Task 1).
fn build(case: Case, points: usize, horizon_cap: usize) -> Built {
    let (ds, y, t0) = tight_daily_series(points);
    let horizon = max_legal_horizon(&ds, case.freq, case.growth, horizon_cap);
    let lambda = lambda_for(&ds, horizon, case.freq);

    let mut args = ForecastArgs {
        ds,
        y,
        horizon,
        ..ForecastArgs::default()
    };
    args.freq = Some(case.freq.to_string());
    args.model = Some(case.model.to_string());

    let mut cells = 0usize;
    if case.model == "prophet" {
        args.growth = Some(case.growth.to_string());
        if case.growth == "logistic" {
            // Derived from the series rather than hardcoded, so a change to the synthetic
            // series cannot silently make the composition invalid (`cap` must exceed max(y)).
            let y_max = args.y.iter().fold(f64::NEG_INFINITY, |a, v| a.max(*v));
            args.cap = Some(y_max + 10.0);
        }
        if case.holidays {
            // AT the design-cost bound for THIS composition's (points + horizon), which is
            // what "the at-the-design-cost-bound holiday shape" means once the horizon is a
            // free axis: `(points + horizon) * columns <= MAX_HOLIDAY_DESIGN_COST`.
            let columns = (MAX_HOLIDAY_DESIGN_COST / (points + horizon)).max(1);
            let hol = holidays_for(columns, 8, t0, points);
            cells = (points + horizon) * columns;
            args.holidays = Some(hol);
        }
    }
    Built {
        args,
        horizon,
        lambda,
        cells,
    }
}

/// The prophet cross product: freq x growth x holiday shape.
fn prophet_matrix() -> Vec<Case> {
    let mut out = Vec::new();
    for freq in ["D", "W", "MS"] {
        for growth in ["linear", "logistic", "flat"] {
            for holidays in [false, true] {
                out.push(Case {
                    model: "prophet",
                    freq,
                    growth,
                    holidays,
                });
            }
        }
    }
    out
}

/// One measured composition.
struct Row {
    label: String,
    total_s: f64,
}

#[allow(clippy::too_many_lines)]
fn measure(case: Case, points: usize, horizon_cap: usize) -> Row {
    let built = build(case, points, horizon_cap);
    let label = format!(
        "model={} freq={} growth={} holidays={}",
        case.model,
        case.freq,
        case.growth,
        if case.holidays { "at_bound" } else { "none" }
    );

    let scope = format!("{label} points={points} horizon={}", built.horizon);
    let (r, total) = time_accepted(&built.args, &scope);

    println!(
        "SC1 WALL: {label} points={points} horizon={} lambda={:.1} cells={} total_s={total:.3} \
         fit_s={:.3} predict_s={:.3} band_width={:.4} arch={} profile={}",
        built.horizon,
        built.lambda,
        built.cells,
        r.fit_seconds,
        r.predict_seconds,
        mean_band_width(&r),
        std::env::consts::ARCH,
        profile_token()
    );
    Row {
        label: format!("{label} points={points} horizon={}", built.horizon),
        total_s: total,
    }
}

/// The NeuralProphet arm, which is NOT part of the cross product: it accepts `freq: "D"`
/// only and refuses `growth`, `cap`, `seasonality_mode` and `holidays` outright, so it is one
/// row rather than an axis. `cells=` on its line is its door-computed TRAIN COST proxy — the
/// dominant work count for that arm — not a design-cell count.
fn measure_np(points: usize, n_lags: usize, horizon: usize) -> Row {
    let (ds, y, _) = tight_daily_series(points);
    let mut args = ForecastArgs {
        ds,
        y,
        horizon,
        ..ForecastArgs::default()
    };
    args.model = Some("neuralprophet".into());
    args.freq = Some("D".into());
    if n_lags > 0 {
        args.n_lags = Some(n_lags);
    }
    let label = format!("model=neuralprophet freq=D growth=n_a holidays=none n_lags={n_lags}");

    let days: Vec<i64> = args
        .ds
        .iter()
        .map(|s| parse_date(s).expect("the sweep builds valid dates"))
        .collect();
    let d = crate::np::NpData::new(&days, &args.y, days.len(), 10, 0.8);
    let cost = crate::np::request_train_cost(&d, days.len(), n_lags);

    let scope = format!("{label} points={points} horizon={horizon}");
    let (r, total) = time_accepted(&args, &scope);

    println!(
        "SC1 WALL: {label} points={points} horizon={horizon} lambda=0.0 cells={cost} \
         total_s={total:.3} fit_s={:.3} predict_s={:.3} band_width={:.4} arch={} profile={}",
        r.fit_seconds,
        r.predict_seconds,
        mean_band_width(&r),
        std::env::consts::ARCH,
        profile_token()
    );
    Row {
        label: format!("{label} points={points} horizon={horizon}"),
        total_s: total,
    }
}

/// ONE test, deliberately: libtest runs tests in the same binary CONCURRENTLY, and two
/// wall-clock harnesses racing each other measure the scheduler rather than the code.
#[test]
fn sc1_wall_sweep() {
    let points = env_usize("SC1_SWEEP_POINTS", 33);
    let horizon_cap = env_usize("SC1_SWEEP_HORIZON", MAX_HORIZON);
    let np_points = env_usize("SC1_SWEEP_NP_POINTS", 200);
    let np_lags = env_usize("SC1_SWEEP_NP_LAGS", 5);
    let np_horizon = env_usize("SC1_SWEEP_NP_HORIZON", 30);
    let budget_secs = env_usize("SC1_SWEEP_BUDGET_SECS", 120);

    let started = std::time::Instant::now();
    let mut rows: Vec<Row> = Vec::new();
    // MEASURE AND PRINT EVERY COMPOSITION FIRST, THEN ASSERT. A print-and-assert loop aborts
    // at the first failure and hides the shape of the failure across the rest of the matrix,
    // which is exactly how a one-axis bench stays convincing.
    for case in prophet_matrix() {
        rows.push(measure(case, points, horizon_cap));
    }
    rows.push(measure_np(np_points, np_lags, np_horizon));

    let elapsed = started.elapsed().as_secs_f64();
    println!(
        "SC1 SWEEP: compositions={} elapsed_s={elapsed:.3} profile={}",
        rows.len(),
        profile_token()
    );

    // NOT an SC1 bar, and deliberately not throttling-sensitive the way a 2 s bar is: this
    // module joins a `workspace-test` job already running 80 604 tests, so a runaway matrix
    // is a real CI regression. Twice the 60 s in-suite target, which the default matrix
    // MEASURES at 7.19 s of test time on a debug build (aarch64, plan 06-16).
    assert!(
        elapsed < budget_secs as f64,
        "the sweep took {elapsed:.1} s, over its {budget_secs} s CI BUDGET. This is NOT the \
         SC1 bar — the SC1 bar is 2 s per composition and is asserted on release only; this \
         guard exists so an 18-composition matrix cannot quietly become a CI regression. \
         Shrink SC1_SWEEP_POINTS / SC1_SWEEP_HORIZON / the NeuralProphet row, never an axis: \
         dropping an axis is the WR-04 defect this harness replaced."
    );

    if cfg!(debug_assertions) {
        // A wall-clock assertion in the debug sweep would flake with CPU throttling, and this
        // crate's `opt-level = 3` does not cover its dependencies. The bar lives on release.
        return;
    }
    let over: Vec<String> = rows
        .iter()
        .filter(|r| r.total_s >= 2.0)
        .map(|r| format!("[{}] {:.3} s", r.label, r.total_s))
        .collect();
    assert!(
        over.is_empty(),
        "{} of {} compositions are at or above the 2.0 s SC1 bar: {}",
        over.len(),
        rows.len(),
        over.join("; ")
    );
}

// ------------------------------------------------- the REGRESSOR cost axis (C-17) ----

/// One composition of the regressor cost sweep.
///
/// Every field is DERIVED from the two ceilings under test, never hard-coded: the whole
/// point of the sweep is that the same PRODUCT is reached by five different shapes, so a
/// ceiling certified on one geometry is not being read as a ceiling on the axis.
#[derive(Clone, Debug)]
pub(crate) struct RegComposition {
    pub label: &'static str,
    pub points: usize,
    pub horizon: usize,
    pub regressors: usize,
    /// Holiday design columns carried ALONGSIDE the regressors. Non-zero on exactly one
    /// composition: a caller may send both, and the holiday design cost and the regressor
    /// design cost are checked by DIFFERENT constants against the SAME 2 s bar.
    pub holiday_columns: usize,
}

/// The five compositions of `(points + horizon) * n_regressors == cost`, at `max_r`.
///
/// Three of them differ in every factor of the product (many rows / few regressors,
/// balanced, few rows / many regressors); the fourth is the MAXIMUM-WIDTH case, where the
/// identifiability diagnostic's `N * K^2` term peaks for this product; the fifth carries
/// holidays as well, because the two design-cost ceilings are independent and a caller can
/// buy both at once.
pub(crate) fn regressor_compositions(cost: usize, max_r: usize) -> Vec<RegComposition> {
    let row_budget = |r: usize| (cost / r.max(1)).max(crate::types::MIN_POINTS + 1);
    let mut out = Vec::new();

    // 1. MANY ROWS, FEW REGRESSORS.
    let rows = row_budget(4).min(crate::types::MAX_POINTS);
    let horizon = 365.min(rows / 2);
    out.push(RegComposition {
        label: "many_rows_few_regressors",
        points: rows - horizon,
        horizon,
        regressors: 4,
        holiday_columns: 0,
    });

    // 2. BALANCED.
    let rows = row_budget(20).min(crate::types::MAX_POINTS);
    let horizon = 365.min(rows / 2);
    out.push(RegComposition {
        label: "balanced",
        points: rows - horizon,
        horizon,
        regressors: 20,
        holiday_columns: 0,
    });

    // 3. FEW ROWS, MANY REGRESSORS.
    //
    // The horizon is capped so the HISTORY still clears `MIN_POINTS`. Without that cap a
    // large `max_r` shrinks `rows` until the even split leaves fewer than ten history
    // points and the door refuses the composition outright — the harness would then be
    // measuring a refusal, which `time_accepted` correctly panics on. Found by running the
    // ladder at max_r = 2 000; the always-run geometry test exercises only the SHIPPED
    // constants, so it could not have caught it.
    let rows = row_budget(max_r);
    let horizon = (rows / 2).max(1).min(rows - crate::types::MIN_POINTS);
    out.push(RegComposition {
        label: "few_rows_many_regressors",
        points: rows - horizon,
        horizon,
        regressors: max_r,
        holiday_columns: 0,
    });

    // 4. MAXIMUM WIDTH: the count ceiling with the LARGEST len(ds) the product allows, so
    //    the diagnostic's `N * K^2` term is at its peak for this product.
    let rows = row_budget(max_r);
    out.push(RegComposition {
        label: "max_width_n_times_k_squared_peak",
        points: rows - 1,
        horizon: 1,
        regressors: max_r,
        holiday_columns: 0,
    });

    // 5. COMBINED: regressors at the count ceiling AND holidays at their own at-the-bound
    //    column count for these rows. Both products are inside their own constants; the 2 s
    //    bar is shared.
    let rows = row_budget(max_r);
    let hc =
        (crate::types::MAX_HOLIDAY_DESIGN_COST / rows).clamp(1, crate::types::MAX_HOLIDAY_COLUMNS);
    out.push(RegComposition {
        label: "combined_holidays_and_regressors",
        points: rows - 1,
        horizon: 1,
        regressors: max_r,
        holiday_columns: hc,
    });

    out
}

/// `n` distinct, finite, non-constant regressor columns of length `len`.
///
/// Distinct on purpose: an exactly duplicated column makes the design singular, which is a
/// legitimate request the door accepts but is NOT the shape a cost ceiling should be derived
/// on — the optimiser's iteration count on a degenerate design is not representative.
pub(crate) fn regressor_values(n: usize, len: usize) -> Vec<Vec<f64>> {
    (0..n)
        .map(|j| {
            let w = 0.017 * (j + 1) as f64;
            (0..len)
                .map(|i| (i as f64 * w).sin() + 0.0013 * i as f64 + j as f64)
                .collect()
        })
        .collect()
}

/// Build the ACCEPTED request for one composition.
pub(crate) fn build_regressor_args(c: &RegComposition) -> ForecastArgs {
    let (ds, y, t0) = tight_daily_series(c.points);
    let vals = regressor_values(c.regressors, c.points + c.horizon);
    let regressors: Vec<crate::types::RegressorArg> = vals
        .into_iter()
        .enumerate()
        .map(|(j, values)| crate::types::RegressorArg {
            // `r{j}` collides with nothing in the three-part reserved set: it is neither a
            // `{name}_delim_{n}` form, nor a seasonality/holiday component name, nor one of
            // the eleven reserved response keys.
            name: format!("r{j}"),
            values,
            mode: None,
            prior_scale: None,
            standardize: None,
        })
        .collect();
    let mut args = ForecastArgs {
        ds,
        y,
        horizon: c.horizon,
        regressors: Some(regressors),
        ..ForecastArgs::default()
    };
    if c.holiday_columns > 0 {
        args.holidays = Some(holidays_for(c.holiday_columns, 8, t0, c.points));
    }
    args
}

/// Measure one composition and print one machine-parsable line.
fn measure_regressor(c: &RegComposition) -> Row {
    let args = build_regressor_args(c);
    let cells = (c.points + c.horizon) * c.regressors;
    let scope = format!(
        "{} points={} horizon={} regressors={} holiday_columns={}",
        c.label, c.points, c.horizon, c.regressors, c.holiday_columns
    );
    let (r, total) = time_accepted(&args, &scope);
    // K as cost axis C-17 defines it for the diagnostic: the trend proxy, the seasonality
    // columns and the regressor columns. Holiday columns are EXCLUDED, which is exactly why
    // composition 5 can carry them without moving the cubic term.
    // Derived from the SAME function the door calls, never from the diagnostics string
    // list: `seasonalities` there is one entry per seasonality while the COLUMN count is
    // `2 * order` per seasonality, so counting entries undercounts K by up to 6x.
    let days: Vec<i64> = args
        .ds
        .iter()
        .map(|s| crate::dates::parse_date(s).expect("the bench builds valid dates"))
        .collect();
    let seasonality_columns: usize =
        crate::prophet::auto_seasonalities(&days, 10.0, crate::prophet::Mode::Additive)
            .iter()
            .map(|s| 2 * s.order)
            .sum();
    let k = 1 + seasonality_columns + c.regressors;
    println!(
        "REGRESSOR WALL: composition={} points={} horizon={} regressors={} \
         holiday_columns={} cells={cells} k={k} n_times_k_squared={} k_cubed={} \
         total_s={total:.3} fit_s={:.3} predict_s={:.3} other_s={:.3} arch={} profile={}",
        c.label,
        c.points,
        c.horizon,
        c.regressors,
        c.holiday_columns,
        c.points * k * k,
        k * k * k,
        r.fit_seconds,
        r.predict_seconds,
        total - r.fit_seconds - r.predict_seconds,
        std::env::consts::ARCH,
        profile_token()
    );
    Row {
        label: scope,
        total_s: total,
    }
}

/// The regressor cost sweep. Release-only by recipe (`just forecast-regressor-bench`).
///
/// `#[ignore]`d for the reason `holiday_design_wall` is: at the ceiling one composition is
/// tens of seconds on a debug profile, and paying that in every `cargo test` run would be a
/// large regression on this crate's suite. The always-run coverage of these ceilings is the
/// refusal table in `forecast::tests`, which pins the comparison at the boundary without
/// paying for it.
#[test]
#[ignore = "release-profile wall-clock measurement; run via just forecast-regressor-bench"]
fn regressor_design_wall() {
    let cost = env_usize(
        "REGRESSOR_BENCH_COST",
        crate::types::MAX_REGRESSOR_DESIGN_COST,
    );
    let max_r = env_usize(
        "REGRESSOR_BENCH_MAX_REGRESSORS",
        crate::types::MAX_REGRESSORS,
    );
    let comps = regressor_compositions(cost, max_r);
    // MEASURE AND PRINT EVERY COMPOSITION FIRST, THEN ASSERT — a print-and-assert loop
    // aborts at the first failure and hides the shape of the failure across the rest of the
    // matrix, which is exactly how a one-geometry bench stays convincing.
    let rows: Vec<Row> = comps.iter().map(measure_regressor).collect();
    println!(
        "REGRESSOR SWEEP: compositions={} cost={cost} max_regressors={max_r} profile={}",
        rows.len(),
        profile_token()
    );
    if cfg!(debug_assertions) {
        // The bar lives on release (CLAUDE.md rule 2): this crate carries
        // `[profile.dev.package.aprender-forecast] opt-level = 3`, which does NOT cover its
        // dependencies, so a debug wall looks plausible and still is not the SC1 bar.
        return;
    }
    let over: Vec<String> = rows
        .iter()
        .filter(|r| r.total_s >= 2.0)
        .map(|r| format!("[{}] {:.3} s", r.label, r.total_s))
        .collect();
    assert!(
        over.is_empty(),
        "{} of {} regressor compositions are at or above the 2.0 s SC1 bar at cost={cost} \
         max_regressors={max_r}: {}",
        over.len(),
        rows.len(),
        over.join("; ")
    );
}

#[cfg(test)]
mod regressor_geometry {
    //! The sweep's own shape checks, which run in the ALWAYS-RUN suite.
    //!
    //! `regressor_design_wall` is `#[ignore]`d, so without these a composition builder that
    //! silently produced an illegal or trivial geometry would only be discovered by someone
    //! running the release recipe.
    use super::{regressor_compositions, RegComposition};
    use crate::types::{MAX_HORIZON, MAX_POINTS, MIN_POINTS};

    #[test]
    fn every_composition_is_a_legal_request_at_the_same_product() {
        let (cost, max_r) = (
            crate::types::MAX_REGRESSOR_DESIGN_COST,
            crate::types::MAX_REGRESSORS,
        );
        let comps = regressor_compositions(cost, max_r);
        assert_eq!(comps.len(), 5, "the sweep is five compositions");
        for c in &comps {
            assert!(
                c.points >= MIN_POINTS && c.points <= MAX_POINTS,
                "{c:?} has an illegal point count"
            );
            assert!(
                c.horizon >= 1 && c.horizon <= MAX_HORIZON,
                "{c:?} has an illegal horizon"
            );
            assert!(
                (c.points + c.horizon) * c.regressors <= cost,
                "{c:?} is OVER the product ceiling the sweep is certifying, so the door \
                 would refuse it and the sweep would measure a refusal"
            );
            assert!(
                c.regressors <= max_r,
                "{c:?} is over the count ceiling the sweep is certifying"
            );
            assert!(
                (c.points + c.horizon) * c.holiday_columns <= crate::types::MAX_HOLIDAY_DESIGN_COST,
                "{c:?} is over the HOLIDAY design cost ceiling, which is a different \
                 constant against the same bar"
            );
        }
    }

    /// The compositions actually DIFFER in every factor — three shapes that all reach the
    /// same product is the claim, and a builder collapsing to one shape would still satisfy
    /// the legality checks above.
    #[test]
    fn the_compositions_differ_in_every_factor() {
        let comps = regressor_compositions(
            crate::types::MAX_REGRESSOR_DESIGN_COST,
            crate::types::MAX_REGRESSORS,
        );
        let distinct = |f: fn(&RegComposition) -> usize| {
            comps
                .iter()
                .map(f)
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert!(
            distinct(|c| c.regressors).len() >= 3,
            "the regressor count must vary across the sweep"
        );
        assert!(
            distinct(|c| c.points).len() >= 3,
            "the history length must vary across the sweep"
        );
        assert!(
            distinct(|c| c.horizon).len() >= 3,
            "the horizon must vary across the sweep"
        );
        assert!(
            comps.iter().any(|c| c.holiday_columns > 0),
            "exactly one composition must carry holidays BESIDE the regressors: the two \
             design-cost ceilings are independent constants against the SAME 2 s bar"
        );
    }
}
