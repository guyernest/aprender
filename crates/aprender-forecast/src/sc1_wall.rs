//! The SC1 wall, SWEPT over the surface where the cost is actually decided.
//!
//! # Which run is the gate, and which run is not (read this before quoting a green)
//!
//! There are TWO runs of this harness and they claim different things.
//!
//! * The **in-suite run** — a plain `cargo test -p aprender-forecast --lib` — is a **SHAPE
//!   CHECK**. It proves every composition in the matrix is still ACCEPTED by the door, still
//!   emits one parsable `SC1 WALL:` line, and has not rotted. It runs at a REDUCED
//!   points/horizon geometry, on whatever profile the caller used, and it asserts **NO** SC1
//!   bar. A green `--lib` run is NOT an SC1 guarantee, and reading it as one is the exact
//!   posture WR-04 describes.
//! * The **gate** is `just forecast-sc1-sweep`. It widens the matrix by environment to the
//!   AT-THE-BOUND geometry, forces `--release`, and asserts the 2 s SC1 bar — in the harness
//!   AND again in the recipe, over the printed lines, so neither can pass vacuously.
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

/// A strictly-ascending DAILY series of `points` points, which is the TIGHTEST legal history
/// span for that point count — and therefore the geometry that maximises `t_max` and so the
/// logistic simulation's Poisson mean. This is the geometry `06-REVIEW.md` CR-01 measured.
fn tight_daily_series(points: usize) -> (Vec<String>, Vec<f64>, i64) {
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

    let t = std::time::Instant::now();
    // A REFUSED composition timed as a wall is a 0.001 s pass that means nothing, so the
    // acceptance is asserted here, naming the composition and the door's own message.
    let r = crate::forecast::forecast(&built.args).unwrap_or_else(|e| {
        panic!(
            "the sweep composition [{label} points={points} horizon={}] must be ACCEPTED by \
             the door, or its wall is a meaningless fast pass — the door refused it: {e}",
            built.horizon
        )
    });
    let total = t.elapsed().as_secs_f64();
    assert_eq!(r.yhat.len(), built.horizon, "one row per horizon step");

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!(
        "SC1 WALL: {label} points={points} horizon={} lambda={:.1} cells={} total_s={total:.3} \
         fit_s={:.3} predict_s={:.3} arch={} profile={profile}",
        built.horizon,
        built.lambda,
        built.cells,
        r.fit_seconds,
        r.predict_seconds,
        std::env::consts::ARCH
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

    let t = std::time::Instant::now();
    let r = crate::forecast::forecast(&args).unwrap_or_else(|e| {
        panic!(
            "the sweep composition [{label} points={points} horizon={horizon}] must be \
             ACCEPTED by the door — the door refused it: {e}"
        )
    });
    let total = t.elapsed().as_secs_f64();
    assert_eq!(r.yhat.len(), horizon, "one row per horizon step");

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!(
        "SC1 WALL: {label} points={points} horizon={horizon} lambda=0.0 cells={cost} \
         total_s={total:.3} fit_s={:.3} predict_s={:.3} arch={} profile={profile}",
        r.fit_seconds,
        r.predict_seconds,
        std::env::consts::ARCH
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
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );

    // NOT an SC1 bar, and deliberately not throttling-sensitive the way a 2 s bar is: this
    // module joins a `workspace-test` job already running 80 604 tests, so a runaway matrix
    // is a real CI regression. Twice the 60 s in-suite target.
    assert!(
        elapsed < budget_secs as f64,
        "the sweep took {elapsed:.1} s, over its {budget_secs} s CI BUDGET (not the SC1 bar — \
         the SC1 bar is 2 s per composition and is asserted on release only). Shrink \
         SC1_SWEEP_POINTS / SC1_SWEEP_HORIZON, never an axis."
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
