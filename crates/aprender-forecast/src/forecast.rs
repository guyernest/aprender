//! THE validation door. The transport re-checks nothing: every bound, every refusal and
//! every default lives here (D-11), so the stdio server, the streamable-HTTP server and a
//! direct library caller all get identical answers.
//!
//! Ported from `sources/004-forecast-mcp-thin-server/src/lib.rs:174-275` (D-08). Every
//! refusal message is verbatim — plan 06-06's e2e cases string-match them.

// serde_json::json! expands to .unwrap() internally, and the expansion lands at file
// scope where a statement-level allow cannot reach it. Same precedent as
// aprender-mcp-setfit/src/lib.rs:29-32.
#![allow(clippy::disallowed_methods)]

use std::time::Instant;

use crate::dates::{format_ymd, future_days, parse_date};
use crate::fit::{fit_prophet, FIT_BUDGET_SECS, MAX_ITERS_PER_ROUND};
use crate::np;
use crate::prophet::{
    auto_seasonalities, changepoint_count, make_design, predict, Growth, Holiday, Mode,
    Seasonality, Spec,
};
use crate::types::{
    ForecastArgs, ForecastError, ForecastResponse, MAX_HOLIDAY_COLUMNS, MAX_HOLIDAY_DATES,
    MAX_HOLIDAY_DATES_TOTAL, MAX_HOLIDAY_DESIGN_COST, MAX_HOLIDAY_WINDOW, MAX_HORIZON,
    MAX_LOGISTIC_CHANGEPOINT_LAMBDA, MAX_POINTS, MAX_SPAN_DAYS, MIN_POINTS,
};

/// Fit and forecast in ONE stateless call (D-01: no fit -> artifact -> forecast round-trip).
///
/// # Errors
///
/// [`ForecastError::Validation`] for anything the caller can fix (shape, bounds, dates,
/// unsupported options); [`ForecastError::Internal`] for a failure inside a fit.
pub fn forecast(args: &ForecastArgs) -> Result<ForecastResponse, ForecastError> {
    // ---- validation (the transport re-checks nothing; this is THE door) ----
    if args.ds.len() != args.y.len() {
        return Err(ForecastError::Validation(format!(
            "ds has {} entries but y has {}",
            args.ds.len(),
            args.y.len()
        )));
    }
    if args.ds.len() < MIN_POINTS {
        return Err(ForecastError::Validation(format!(
            "need at least {MIN_POINTS} points, got {}",
            args.ds.len()
        )));
    }
    if args.ds.len() > MAX_POINTS {
        return Err(ForecastError::Validation(format!(
            "{} points exceeds max_points {MAX_POINTS}",
            args.ds.len()
        )));
    }
    if args.horizon == 0 || args.horizon > MAX_HORIZON {
        return Err(ForecastError::Validation(format!(
            "horizon must be 1..={MAX_HORIZON}, got {}",
            args.horizon
        )));
    }
    if args.y.iter().any(|v| !v.is_finite()) {
        return Err(ForecastError::Validation(
            "y contains a non-finite value".into(),
        ));
    }
    let ds: Vec<i64> = args
        .ds
        .iter()
        .map(|s| parse_date(s))
        .collect::<Result<_, _>>()?;
    if !ds.windows(2).all(|w| w[0] < w[1]) {
        return Err(ForecastError::Validation(
            "ds must be strictly ascending with no duplicates".into(),
        ));
    }
    // MAX_POINTS bounds how MANY points arrive, never how far apart they are, and the
    // NeuralProphet path materialises an imputed DAILY grid over the whole span — ten
    // points a millennium apart bought a 3.6-million-row grid. Bound the span too.
    let span_days = ds[ds.len() - 1] - ds[0] + 1;
    if span_days > MAX_SPAN_DAYS {
        return Err(ForecastError::Validation(format!(
            "ds spans {span_days} days, which exceeds max_span_days {MAX_SPAN_DAYS}"
        )));
    }
    let freq = args.freq.clone().unwrap_or_else(|| "D".into());
    let fut = future_days(ds[ds.len() - 1], args.horizon, &freq)?;
    let model_name = args.model.clone().unwrap_or_else(|| "prophet".into());
    let interval_width = args.interval_width.unwrap_or(0.8);
    if !(0.0 < interval_width && interval_width < 1.0) {
        return Err(ForecastError::Validation(
            "interval_width must be in (0, 1)".into(),
        ));
    }
    let seed = args.seed.unwrap_or(42);
    let (y_min, y_max) = args
        .y
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
            (a.min(*v), b.max(*v))
        });
    if y_min == y_max {
        return Err(ForecastError::Validation(
            "y is constant; nothing to fit (Prophet special-cases this too)".into(),
        ));
    }

    // An option that belongs to the OTHER model is refused, never silently dropped: the
    // worst failure class at this door is the caller getting a plausible answer to a
    // question they did not ask (D-11). `deny_unknown_fields` catches a misspelt key; only
    // this catches a well-formed key aimed at the wrong arm.
    if model_name == "prophet" && args.n_lags.is_some() {
        return Err(ForecastError::Validation(
            "n_lags is neuralprophet-only; set model to \"neuralprophet\"".into(),
        ));
    }
    if model_name == "neuralprophet" {
        for (name, present) in [
            ("growth", args.growth.is_some()),
            ("cap", args.cap.is_some()),
            ("seasonality_mode", args.seasonality_mode.is_some()),
            ("holidays", args.holidays.is_some()),
        ] {
            if present {
                return Err(ForecastError::Validation(format!(
                    "{name} is prophet-only; set model to \"prophet\""
                )));
            }
        }
    }

    match model_name.as_str() {
        "prophet" => {
            let mode = match args.seasonality_mode.as_deref() {
                None | Some("additive") => Mode::Additive,
                Some("multiplicative") => Mode::Multiplicative,
                Some(o) => {
                    return Err(ForecastError::Validation(format!(
                        "seasonality_mode {o:?}: additive or multiplicative"
                    )))
                }
            };
            let growth = match args.growth.as_deref() {
                None | Some("linear") => Growth::Linear,
                Some("logistic") => Growth::Logistic,
                Some("flat") => Growth::Flat,
                Some(o) => {
                    return Err(ForecastError::Validation(format!(
                        "growth {o:?}: linear, logistic or flat"
                    )))
                }
            };
            // `cap` belongs to the LOGISTIC growth arm, not merely to the prophet MODEL:
            // `make_design` matches only `(Growth::Logistic, Some(c))` and falls to
            // `_ => None` for every other pair, so a cap sent on the linear, flat or
            // DEFAULTED arm was accepted and provably inert — the caller got a plausible
            // answer to a question they did not ask (D-11, FALSIFY-BOUNDARY-005). This
            // runs AFTER the `growth` enum is parsed, so an unknown growth string still
            // refuses with its own message first.
            if growth != Growth::Logistic && args.cap.is_some() {
                return Err(ForecastError::Validation(
                    "cap is logistic-only; set growth to \"logistic\"".into(),
                ));
            }
            // The ONLY path that puts a `Some` here is the one below that validated it,
            // so `Spec` structurally cannot carry a cap the door did not check.
            let mut checked_cap: Option<f64> = None;
            if growth == Growth::Logistic {
                let cap = args
                    .cap
                    .ok_or_else(|| ForecastError::Validation("logistic growth needs cap".into()))?;
                // `y` is checked for finiteness above; `cap` was not, and JSON `1e400`
                // parses to `f64::INFINITY`, for which `cap <= y_max` is false. An infinite
                // cap makes every trend value infinite, which serialises as JSON `null`.
                if !cap.is_finite() {
                    return Err(ForecastError::Validation(
                        "cap must be a finite number".into(),
                    ));
                }
                if cap <= y_max {
                    return Err(ForecastError::Validation(format!(
                        "cap {cap} must exceed max(y) = {y_max}"
                    )));
                }
                checked_cap = Some(cap);
            }
            let mut holidays = Vec::new();
            // `prophet::columns` emits one design column per offset in the window, so an
            // unbounded window is an unbounded column count from a ~200-byte request.
            let mut holiday_columns = 0usize;
            let mut holiday_dates_total = 0usize;
            for h in args.holidays.as_deref().unwrap_or(&[]) {
                if h.lower_window > 0 || h.upper_window < 0 {
                    return Err(ForecastError::Validation(format!(
                        "holiday {:?}: lower_window ≤ 0 ≤ upper_window",
                        h.name
                    )));
                }
                if h.lower_window < -MAX_HOLIDAY_WINDOW || h.upper_window > MAX_HOLIDAY_WINDOW {
                    return Err(ForecastError::Validation(format!(
                        "holiday {:?}: windows must be within ±{MAX_HOLIDAY_WINDOW} days",
                        h.name
                    )));
                }
                if h.dates.len() > MAX_HOLIDAY_DATES {
                    return Err(ForecastError::Validation(format!(
                        "holiday {:?}: {} dates exceeds max_holiday_dates {MAX_HOLIDAY_DATES}",
                        h.name,
                        h.dates.len()
                    )));
                }
                // Each term is now <= MAX_HOLIDAY_DATES, and the running sum is refused
                // below the moment the aggregate ceiling is passed.
                holiday_dates_total += h.dates.len();
                // Both windows are now within ±MAX_HOLIDAY_WINDOW, so the width is small
                // enough that this sum cannot overflow before the ceiling refuses it.
                holiday_columns += (h.upper_window - h.lower_window + 1) as usize;
                if holiday_columns > MAX_HOLIDAY_COLUMNS {
                    return Err(ForecastError::Validation(format!(
                        "holiday windows expand to more than max_holiday_columns \
                         {MAX_HOLIDAY_COLUMNS} design columns"
                    )));
                }
                let days: Vec<i64> = h
                    .dates
                    .iter()
                    .map(|s| parse_date(s))
                    .collect::<Result<_, _>>()?;
                holidays.push(Holiday {
                    name: h.name.clone(),
                    days,
                    lower_window: h.lower_window,
                    upper_window: h.upper_window,
                    prior_scale: 10.0,
                });
            }
            // ---- the PRODUCT bounds, at THE door and BEFORE make_design ----
            //
            // Every per-holiday bound above has now fired with its own message, so
            // `holiday_columns <= MAX_HOLIDAY_COLUMNS` and each `dates.len() <=
            // MAX_HOLIDAY_DATES`. What was never checked is what they multiply to.
            // `FIT_BUDGET_SECS` cannot cover it: it is a COOPERATIVE ROUND-BOUNDARY
            // budget, so it is structurally blind to `make_design` (which runs below,
            // before `fit_prophet` is entered) and it overshoots by a whole round inside
            // the fit — a 20 000-point, 1 000-column request measured 70.089 s against a
            // 15 s budget. The refusal has to be HERE.
            if holiday_dates_total > MAX_HOLIDAY_DATES_TOTAL {
                return Err(ForecastError::Validation(format!(
                    "holidays carry {holiday_dates_total} dates in total, which exceeds \
                     max_holiday_dates_total {MAX_HOLIDAY_DATES_TOTAL}; send fewer holidays \
                     or fewer dates per holiday"
                )));
            }
            // `ds.len() <= MAX_POINTS` (20 000), `args.horizon <= MAX_HORIZON` (3 650) and
            // `holiday_columns <= MAX_HOLIDAY_COLUMNS` (1 000) are all already refused
            // above, so this product is at most 23 650 000 — six orders of magnitude below
            // `usize::MAX` on every supported target. A plain multiply therefore cannot
            // overflow here, and `saturating_mul` would only obscure that the factors are
            // bounded rather than add safety.
            let design_cells = (ds.len() + args.horizon) * holiday_columns;
            if design_cells > MAX_HOLIDAY_DESIGN_COST {
                return Err(ForecastError::Validation(format!(
                    "holidays expand to {design_cells} design feature cells \
                     ((points + horizon) x holiday_columns = ({} + {}) x {holiday_columns}), \
                     which exceeds max_holiday_design_cost {MAX_HOLIDAY_DESIGN_COST}; reduce \
                     the holiday windows, the number of holidays, the history length or the \
                     horizon",
                    ds.len(),
                    args.horizon
                )));
            }
            // With no holidays `holiday_columns` and `holiday_dates_total` are both 0, so
            // neither refusal above can fire and the no-holiday SC1 path is untouched.
            let mut spec = Spec::default_linear(auto_seasonalities(&ds, 10.0, mode));
            spec.growth = growth;
            spec.cap = checked_cap;
            spec.holidays = holidays;
            spec.holidays_mode = mode;
            spec.interval_width = interval_width;
            if spec.seasonalities.is_empty() && spec.holidays.is_empty() {
                // Prophet fits a 'zeros' column here; the design needs K >= 1 — give it a
                // harmless weekly term with a tiny prior.
                spec.seasonalities.push(Seasonality {
                    name: "weekly".into(),
                    period: 7.0,
                    order: 1,
                    prior_scale: 1e-3,
                    mode,
                });
            }
            // ---- the LOGISTIC CHANGEPOINT LAMBDA bound, at THE door and BEFORE make_design ----
            //
            // `predict`'s `Growth::Logistic` arm draws `poisson(lambda)` NEW changepoints on
            // every one of the 1 000 simulation rows, with
            // `lambda = changepoints_t.len() * (t_max - 1)`, and the per-row work
            // (`logistic_gammas` + a sort + a full `piecewise_logistic`) is LINEAR in that
            // count. Nothing bounded lambda: `MAX_POINTS`, `MAX_SPAN_DAYS` and `MAX_HORIZON`
            // are checked in isolation and it is their RATIO that sizes this, with
            // `dates::future_days` multiplying the horizon COUNT by 7 for "W" and ~30.44 for
            // "MS". `FIT_BUDGET_SECS` cannot cover it either — that budget is entered inside
            // `fit_prophet`, and this cost is spent in `predict`, which runs after the fit
            // returns with no budget at all. A 1 132-byte accepted request walled at 2.334 s
            // against SC1's 2 s bar (06-REVIEW.md CR-01).
            //
            // The count comes from `prophet::changepoint_count`, the SAME function
            // `make_design` is tied to, so the door's lambda IS the lambda `predict` draws
            // rather than a second inline copy of the arithmetic that could drift from it.
            if growth == Growth::Logistic {
                // `ds` is strictly ascending and `ds.len() >= MIN_POINTS` (10), both already
                // refused above, so `t_scale_days` is strictly positive and no division
                // guard is needed — adding one would imply a case that cannot occur.
                let t_scale_days = (ds[ds.len() - 1] - ds[0]) as f64;
                let future_span_days = fut[fut.len() - 1] - ds[0];
                let t_max = future_span_days as f64 / t_scale_days;
                let n_changepoints = changepoint_count(ds.len(), &spec);
                let lambda = n_changepoints as f64 * (t_max - 1.0);
                if lambda > MAX_LOGISTIC_CHANGEPOINT_LAMBDA {
                    return Err(ForecastError::Validation(format!(
                        "the logistic uncertainty simulation would draw a Poisson mean of \
                         {lambda:.1} new changepoints per sample \
                         (changepoints x (t_max - 1) = {n_changepoints} x ({t_max:.3} - 1), \
                         where t_max is the {future_span_days}-day future span over the \
                         {t_scale_days}-day history span), which exceeds \
                         max_logistic_changepoint_lambda {MAX_LOGISTIC_CHANGEPOINT_LAMBDA}; \
                         shorten the horizon, use freq \"D\" instead of \"W\" or \"MS\", or \
                         send a longer history"
                    )));
                }
            }
            let design = make_design(&ds, &args.y, &spec);
            let t0 = Instant::now();
            let (p, info) = fit_prophet(&design, 8);
            let fit_seconds = t0.elapsed().as_secs_f64();
            let t1 = Instant::now();
            let fc = predict(&design, &p, &fut, seed);
            let predict_seconds = t1.elapsed().as_secs_f64();
            let mut components = serde_json::Map::new();
            for (n, v) in &fc.components {
                components.insert(n.clone(), serde_json::json!(v));
            }
            Ok(ForecastResponse {
                model: "prophet".into(),
                freq,
                n_history: ds.len(),
                fit_seconds,
                predict_seconds,
                ds: fut.iter().map(|d| format_ymd(*d)).collect(),
                yhat: fc.yhat,
                yhat_lower: fc.yhat_lower,
                yhat_upper: fc.yhat_upper,
                trend: fc.trend,
                components,
                diagnostics: serde_json::json!({
                    "growth": format!("{growth:?}"),
                    "seasonality_mode": format!("{mode:?}"),
                    "seasonalities": spec.seasonalities.iter().map(|s| format!("{} (order {})", s.name, s.order)).collect::<Vec<_>>(),
                    "n_changepoints": design.changepoints_t.len(),
                    "active_changepoints": p.delta.iter().filter(|d| d.abs() > 1e-3).count(),
                    "sigma_obs": p.sigma_obs,
                    "lbfgs": {
                        "rounds": info.rounds,
                        "iterations": info.iterations,
                        "evaluations": info.evals,
                        "objective": info.objective,
                        "last_status": info.status,
                        "budget_hit": info.budget_hit,
                        "iters_per_round_cap": MAX_ITERS_PER_ROUND,
                        "budget_secs": FIT_BUDGET_SECS
                    },
                    "interval_width": interval_width,
                    "uncertainty_samples": spec.uncertainty_samples
                }),
            })
        }
        // Ported verbatim from `sources/004-forecast-mcp-thin-server/src/lib.rs:226-262`
        // (D-08), replacing 06-01's refusing tracer stub (REVIEW-06-06). Every training
        // rule below is D-10 and is pinned by an invariant test in `np::parity`.
        "neuralprophet" => {
            if freq != "D" {
                return Err(ForecastError::Validation(
                    "neuralprophet supports freq D only".into(),
                ));
            }
            let n_lags = args.n_lags.unwrap_or(0);
            if n_lags > 365 {
                return Err(ForecastError::Validation("n_lags ≤ 365".into()));
            }
            let n_train = ds.len();
            let d = np::NpData::new(&ds, &args.y, n_train, 10, 0.8);
            if n_lags >= d.n_train_grid {
                return Err(ForecastError::Validation(
                    "n_lags must be smaller than the series span in days".into(),
                ));
            }
            let t0 = Instant::now();
            // spike-002 lesson: a short lr sweep selected by TRAIN loss stands in for NP's
            // range test (D-10 — never select by test error); 4x the auto epochs when lags
            // are on (the linear AR case needs the budget), capped at 320.
            let mut best: Option<(f64, f64, np::NpModel, np::TrainLog)> = None;
            let lrs: &[f64] = if n_lags > 0 {
                &[0.03, 0.1]
            } else {
                &[0.01, 0.03, 0.1]
            };
            for &lr in lrs {
                let cfg = np::TrainConfig {
                    n_lags,
                    ar_layers: if n_lags > 0 { vec![32] } else { vec![] },
                    max_lr: lr,
                    epochs: if n_lags > 0 {
                        Some(np::auto_epochs(n_train).min(320))
                    } else {
                        None
                    },
                    batch: None,
                    weight_decay: 1e-3,
                    huber_beta: 0.3,
                    newer_w: 2.0,
                    seed,
                };
                let (m, log) = np::train(&d, &cfg, false);
                let fl = *log.epoch_loss.last().unwrap_or(&f64::INFINITY);
                if !fl.is_finite() {
                    continue;
                }
                if best.as_ref().is_none_or(|b| fl < b.0) {
                    best = Some((fl, lr, m, log));
                }
            }
            let (train_loss, selected_lr, m, log) = best.ok_or_else(|| {
                ForecastError::Internal("training diverged for every learning rate".into())
            })?;
            let fit_seconds = t0.elapsed().as_secs_f64();
            let t1 = Instant::now();
            // `predict_trend` is branch-independent; only the yhat path differs.
            let trend = np::predict_trend(&d, &m, &fut);
            let yhat = if n_lags == 0 {
                np::predict_ts(&d, &m, &fut)
            } else {
                np::predict_ar_recursive(&d, &m, &fut)
            };
            // Residual-based band. NeuralProphet itself would use quantile regression; that
            // was NOT spiked (CONTEXT deferred), and the diagnostics say so rather than
            // implying a coverage guarantee this band does not have.
            let fitted = if n_lags == 0 {
                np::predict_ts(&d, &m, &ds)
            } else {
                let idx: Vec<usize> = ds
                    .iter()
                    .map(|day| (day - d.t0) as usize)
                    .filter(|i| *i >= n_lags)
                    .collect();
                let pr = np::predict_ar_1step(&d, &m, &idx);
                let mut out = vec![f64::NAN; ds.len()];
                let mut k = 0;
                for (i, day) in ds.iter().enumerate() {
                    if (day - d.t0) as usize >= n_lags {
                        out[i] = pr[k];
                        k += 1;
                    }
                }
                out
            };
            let resid: Vec<f64> = fitted
                .iter()
                .zip(&args.y)
                .filter(|(f, _)| f.is_finite())
                .map(|(f, y)| y - f)
                .collect();
            let sd = (resid.iter().map(|r| r * r).sum::<f64>() / resid.len().max(1) as f64).sqrt();
            let z = normal_quantile((1.0 + interval_width) / 2.0);
            let predict_seconds = t1.elapsed().as_secs_f64();
            let mut components = serde_json::Map::new();
            components.insert("trend".into(), serde_json::json!(trend));
            // Bound before the move so the literal stays in declaration order
            // (clippy::inconsistent_struct_constructor is a workspace `warn`).
            let yhat_lower: Vec<f64> = yhat.iter().map(|v| v - z * sd).collect();
            let yhat_upper: Vec<f64> = yhat.iter().map(|v| v + z * sd).collect();
            Ok(ForecastResponse {
                model: "neuralprophet".into(),
                freq,
                n_history: ds.len(),
                fit_seconds,
                predict_seconds,
                ds: fut.iter().map(|d| format_ymd(*d)).collect(),
                yhat,
                yhat_lower,
                yhat_upper,
                trend,
                components,
                diagnostics: serde_json::json!({
                    "n_lags": n_lags,
                    "ar_layers": if n_lags > 0 { vec![32] } else { vec![] },
                    "epochs": log.epochs,
                    "batch": log.batch,
                    "steps": log.steps,
                    "params": log.n_params,
                    "selected_lr": selected_lr,
                    "final_train_loss": train_loss,
                    "residual_sd": sd,
                    "band": "residual-sd based, not NeuralProphet's quantile regression",
                    "seasonalities": d.seasons.iter().map(|s| format!("{} (order {})", s.name, s.order)).collect::<Vec<_>>()
                }),
            })
        }
        other => Err(ForecastError::Validation(format!(
            "model {other:?}: prophet or neuralprophet"
        ))),
    }
}

/// Acklam's inverse normal CDF (enough for band z-scores).
///
/// The clamp is load-bearing and lives in `aprender::monte_carlo::engine::inverse_normal_cdf`,
/// which this now delegates to. Without it the tails divide infinity by infinity:
/// `interval_width = 0.9999999999999999` is the largest value the door accepts, and
/// `(1.0 + w) / 2.0` rounds to EXACTLY 1.0, so `(1.0 - p).ln()` is `-inf`, `q` is `inf`
/// and the returned z is `NaN` — which `serde_json` then writes as JSON `null` for every
/// `yhat_lower`/`yhat_upper` in an otherwise successful response.
#[must_use]
pub fn normal_quantile(p: f64) -> f64 {
    // Delegates rather than transcribes: proven bit-identical to core over a
    // 100k-point grid plus both clamp shoulders and both branch boundaries.
    aprender::monte_carlo::engine::inverse_normal_cdf(p)
}

#[cfg(test)]
mod tests {
    use super::{forecast, normal_quantile};
    use crate::dates::{days_from_civil, format_ymd};
    use crate::prophet::Rng;
    use crate::types::{ForecastArgs, ForecastError};
    use aprender::autograd::{clear_graph, graph_tape_len};

    /// A 120-point synthetic DAILY series: linear trend + a weekly sine + a little noise.
    /// Deliberately short — these tests prove dispatch, refusals and tape hygiene, never
    /// accuracy. Correctness against the NeuralProphet 0.9.0 oracle is `np::parity`.
    fn synthetic_daily(n: usize) -> (Vec<String>, Vec<f64>) {
        let mut rng = Rng::new(7);
        let t0 = days_from_civil(2020, 1, 1);
        let mut ds = Vec::with_capacity(n);
        let mut y = Vec::with_capacity(n);
        for i in 0..n {
            ds.push(format_ymd(t0 + i as i64));
            let t = i as f64;
            y.push(
                10.0 + 0.01 * t
                    + (2.0 * std::f64::consts::PI * t / 7.0).sin()
                    + 0.05 * rng.normal(),
            );
        }
        (ds, y)
    }

    fn np_args(n: usize, horizon: usize) -> ForecastArgs {
        let (ds, y) = synthetic_daily(n);
        ForecastArgs {
            ds,
            y,
            horizon,
            freq: None,
            model: Some("neuralprophet".into()),
            growth: None,
            cap: None,
            seasonality_mode: None,
            interval_width: None,
            holidays: None,
            n_lags: None,
            seed: None,
        }
    }

    #[test]
    fn neuralprophet_arm_dispatches_and_returns_a_band() {
        clear_graph();
        let args = np_args(120, 14);
        let r = forecast(&args).expect("the neuralprophet arm must dispatch, not refuse");
        assert_eq!(r.model, "neuralprophet");
        assert_eq!(r.ds.len(), 14, "one row per horizon step");
        assert_eq!(r.yhat.len(), 14);
        assert_eq!(r.trend.len(), 14);
        for i in 0..14 {
            assert!(
                r.yhat_lower[i] < r.yhat_upper[i],
                "row {i}: band must be strictly ordered ({} !< {})",
                r.yhat_lower[i],
                r.yhat_upper[i]
            );
            assert!(r.yhat[i].is_finite(), "row {i}: yhat must be finite");
        }
        let lr = r.diagnostics["selected_lr"]
            .as_f64()
            .expect("diagnostics.selected_lr");
        assert!(
            [0.01, 0.03, 0.1].iter().any(|c| (c - lr).abs() < 1e-12),
            "the lag-free sweep is {{0.01, 0.03, 0.1}} selected by TRAIN loss (D-10); got {lr}"
        );
        assert!(
            r.diagnostics["band"]
                .as_str()
                .expect("diagnostics.band")
                .contains("not NeuralProphet's quantile regression"),
            "the band must SAY it is residual-sd based, not quantile regression"
        );
        // D-10 tape hygiene: `clear_graph()` after every step means a completed fit leaves
        // the thread-local tape empty for the next caller on this thread.
        assert_eq!(
            graph_tape_len(),
            0,
            "the autograd tape must be empty after a completed neuralprophet fit"
        );
    }

    #[test]
    fn neuralprophet_refuses_non_daily_freq() {
        let mut args = np_args(120, 14);
        args.freq = Some("W".into());
        refusal(&args, "supports freq D only");
    }

    #[test]
    fn neuralprophet_refuses_n_lags_above_365() {
        let mut args = np_args(120, 14);
        args.n_lags = Some(366);
        refusal(&args, "365");
    }

    #[test]
    fn neuralprophet_refuses_n_lags_at_or_above_span() {
        // 120 consecutive daily points => n_train_grid == 120, so n_lags 120 leaves no
        // complete window. This must refuse at the door, never panic inside the fit.
        let mut args = np_args(120, 14);
        args.n_lags = Some(120);
        refusal(&args, "smaller than the series span");
    }

    // ---------------------------------------------------------------- door hardening ---
    // Every case below reached a crash, an unbounded allocation or a silently wrong
    // answer before it was closed. They live in `--lib` (which CI runs) rather than in
    // `tests/`, which is not on ci.yml's explicit `--test` line.

    fn refusal(args: &ForecastArgs, needle: &str) {
        match forecast(args) {
            Err(ForecastError::Validation(m)) => assert!(
                m.contains(needle),
                "message must name the fix; wanted {needle:?}, got {m:?}"
            ),
            other => panic!(
                "expected a Validation refusal, got {:?}",
                other.map(|r| r.model)
            ),
        }
    }

    /// `MAX_POINTS` bounds how MANY points arrive, never how far apart they are, and the
    /// NeuralProphet path materialises an imputed daily grid over the whole span.
    #[test]
    fn a_span_far_wider_than_the_point_count_is_refused() {
        let mut args = np_args(12, 7);
        args.ds = (0..12)
            .map(|i| format_ymd(days_from_civil(1500, 1, 1) + i * 30_000))
            .collect();
        refusal(&args, "max_span_days");
    }

    /// The two window fields were only SIGN-checked, so two integers expanded into an
    /// unbounded number of design columns from a ~200-byte request.
    #[test]
    fn an_enormous_holiday_window_is_refused() {
        let (ds, y) = synthetic_daily(60);
        let mut args = np_args(60, 7);
        args.model = None;
        args.ds = ds;
        args.y = y;
        args.holidays = Some(vec![crate::types::HolidayArg {
            name: "x".into(),
            dates: vec!["2020-02-01".into()],
            lower_window: -2_000_000_000,
            upper_window: 0,
        }]);
        refusal(&args, "365");
    }

    /// A prophet request carrying `n_holidays` holidays, each `width` design columns wide
    /// and each carrying `dates` occurrences. Every knob here is INSIDE its own bound; the
    /// tests below vary only which PRODUCT goes over.
    fn holiday_args(
        points: usize,
        horizon: usize,
        n_holidays: usize,
        width: i64,
        dates: usize,
    ) -> ForecastArgs {
        let (ds, y) = synthetic_daily(points);
        let mut args = np_args(points, horizon);
        args.model = None;
        args.ds = ds;
        args.y = y;
        let t0 = days_from_civil(2020, 1, 1);
        let lower = -((width - 1) / 2);
        args.holidays = Some(
            (0..n_holidays)
                .map(|h| crate::types::HolidayArg {
                    name: format!("h{h}"),
                    dates: (0..dates).map(|k| format_ymd(t0 + k as i64)).collect(),
                    lower_window: lower,
                    upper_window: width - 1 + lower,
                })
                .collect(),
        );
        args
    }

    /// MAX_POINTS, MAX_HORIZON, MAX_HOLIDAY_COLUMNS and MAX_HOLIDAY_DATES are each checked
    /// in ISOLATION; their product was not. 200 points, a 100-step horizon and one
    /// 400-column holiday are all comfortably legal and together buy 120 000 design
    /// feature cells — and every fit iteration is `O(rows * K)` over exactly that matrix.
    #[test]
    fn an_in_bounds_holiday_spec_whose_product_is_not_is_refused() {
        let args = holiday_args(200, 100, 1, 400, 5);
        refusal(&args, "max_holiday_design_cost");
    }

    /// The NEAR MISS. Exactly at the bound — (50 + 50) x 500 = 50 000 — which the door
    /// must ACCEPT, because a bound proven only to refuse is not proven to refuse just
    /// what it claims.
    #[test]
    fn a_holiday_spec_just_under_the_design_cost_bound_is_accepted() {
        let args = holiday_args(50, 50, 1, 500, 5);
        let r = forecast(&args).expect("a request exactly at the bound must fit, not refuse");
        assert_eq!(r.yhat.len(), 50, "one row per horizon step");
        assert_eq!(r.yhat_lower.len(), 50);
        assert_eq!(r.yhat_upper.len(), 50);
    }

    /// MAX_HOLIDAY_DATES bounds ONE holiday's list; nothing bounded the SUM. Eleven
    /// holidays at the per-holiday ceiling are 11 000 dates from one request — and only
    /// 11 design columns, so the design-cost bound cannot see them.
    #[test]
    fn holidays_carrying_more_dates_than_the_total_bound_are_refused() {
        let args = holiday_args(60, 7, 11, 1, 1_000);
        refusal(&args, "max_holiday_dates_total");
    }

    /// WR-03 — the aggregate ceiling is enforced INSIDE the loop, not after it.
    ///
    /// This test discriminates POSITION, not presence. The running sum crosses
    /// [`MAX_HOLIDAY_DATES_TOTAL`](crate::types::MAX_HOLIDAY_DATES_TOTAL) at the ELEVENTH
    /// holiday, and every LATER holiday carries a date string `parse_date`'s SHAPE gate
    /// rejects (`"2020-1-01"` — nine bytes, so it is refused for its shape and not for an
    /// out-of-range calendar field, which is a different message). If the aggregate refusal
    /// fired after the loop, those later holidays would be parsed first and the caller would
    /// receive the DATE-SHAPE refusal instead. The message that comes back therefore names
    /// which check ran first — something no grep for the constant could establish.
    #[test]
    fn the_aggregate_dates_refusal_fires_inside_the_holiday_loop() {
        let (ds, y) = synthetic_daily(60);
        let mut args = np_args(60, 7);
        args.model = None;
        args.ds = ds;
        args.y = y;
        let base = days_from_civil(1990, 1, 1);
        // 11 x 1 000 = 11 000 > MAX_HOLIDAY_DATES_TOTAL (10 000): the running sum crosses at
        // the eleventh holiday, with four holidays still unparsed behind it.
        let mut holidays: Vec<crate::types::HolidayArg> = (0..11i64)
            .map(|h| crate::types::HolidayArg {
                name: format!("h{h}"),
                dates: (0..1_000i64)
                    .map(|d| format_ymd(base + h * 1_000 + d))
                    .collect(),
                lower_window: 0,
                upper_window: 0,
            })
            .collect();
        for h in 11..15i64 {
            holidays.push(crate::types::HolidayArg {
                name: format!("late{h}"),
                dates: vec!["2020-1-01".into()],
                lower_window: 0,
                upper_window: 0,
            });
        }
        args.holidays = Some(holidays);
        match forecast(&args) {
            Err(ForecastError::Validation(m)) => {
                assert!(
                    !m.contains("want YYYY-MM-DD"),
                    "a DATE-SHAPE refusal means the loop kept parsing holidays after the door \
                     already had the information to refuse — the aggregate check is still \
                     AFTER the loop; got {m:?}"
                );
                assert!(
                    m.contains("max_holiday_dates_total"),
                    "the aggregate ceiling must refuse, naming its own key; got {m:?}"
                );
            }
            other => panic!(
                "expected a Validation refusal, got {:?}",
                other.map(|r| r.model)
            ),
        }
    }

    /// The two-sided control for WR-03's move: a request whose aggregate is EXACTLY at
    /// `MAX_HOLIDAY_DATES_TOTAL` is still accepted. The in-loop comparison must be the same
    /// `>` the post-loop one uses; written as `>=` it would over-refuse by one date.
    #[test]
    fn holidays_carrying_exactly_the_total_bound_are_accepted() {
        let (ds, y) = synthetic_daily(60);
        let mut args = np_args(60, 7);
        args.model = None;
        args.ds = ds;
        args.y = y;
        let base = days_from_civil(1990, 1, 1);
        // 10 x 1 000 == MAX_HOLIDAY_DATES_TOTAL exactly; 10 columns and (60 + 7) x 10 = 670
        // design cells, both far inside their own bounds, so only the aggregate is at issue.
        args.holidays = Some(
            (0..10i64)
                .map(|h| crate::types::HolidayArg {
                    name: format!("h{h}"),
                    dates: (0..1_000i64)
                        .map(|d| format_ymd(base + h * 1_000 + d))
                        .collect(),
                    lower_window: 0,
                    upper_window: 0,
                })
                .collect(),
        );
        let total: usize = args
            .holidays
            .as_deref()
            .expect("holidays")
            .iter()
            .map(|h| h.dates.len())
            .sum();
        assert_eq!(
            total,
            crate::types::MAX_HOLIDAY_DATES_TOTAL,
            "this control is only a NEAR MISS if it sits exactly ON the bound"
        );
        let r = forecast(&args).expect("a request exactly at the aggregate bound must fit");
        assert_eq!(r.yhat.len(), 7, "one row per horizon step");
    }

    /// A well-formed option aimed at the wrong arm was silently DROPPED: the caller got a
    /// plausible answer to a question they did not ask.
    #[test]
    fn an_option_belonging_to_the_other_model_is_refused_not_dropped() {
        let mut prophet = np_args(60, 7);
        prophet.model = None;
        prophet.n_lags = Some(7);
        refusal(&prophet, "neuralprophet-only");

        let mut np = np_args(60, 7);
        np.growth = Some("logistic".into());
        refusal(&np, "prophet-only");

        // The cross-MODEL refusal keeps its OWN older message: a cap aimed at the
        // neuralprophet arm still says `cap is prophet-only`, never the new growth-arm one.
        let mut np_cap = np_args(60, 7);
        np_cap.cap = Some(100.0);
        refusal(&np_cap, "cap is prophet-only");

        // `cap` belongs to the LOGISTIC growth arm, not merely to the prophet MODEL. It
        // was accepted and provably dropped on every other growth arm: `make_design`
        // matches only `(Growth::Logistic, Some(c))`, so a cap sent with linear or flat
        // growth fell to `_ => None` and the caller got a plausible answer to a question
        // they did not ask. Four shapes, because one failing input is an anecdote.
        let mut linear_cap = np_args(60, 7);
        linear_cap.model = None;
        linear_cap.growth = Some("linear".into());
        linear_cap.cap = Some(100.0);
        refusal(&linear_cap, "logistic-only");

        // The DEFAULTED arm — no `growth` key at all — is the one a real caller hits, and
        // only this case proves the check is not keyed on the PRESENCE of `growth`.
        let mut bare_cap = np_args(60, 7);
        bare_cap.model = None;
        bare_cap.cap = Some(100.0);
        refusal(&bare_cap, "logistic-only");

        let mut flat_cap = np_args(60, 7);
        flat_cap.model = None;
        flat_cap.growth = Some("flat".into());
        flat_cap.cap = Some(100.0);
        refusal(&flat_cap, "logistic-only");

        // A cap BELOW max(y) off the logistic arm must refuse for being off-arm, not
        // sneak through the `cap <= y_max` rule that only guards the logistic branch.
        let mut linear_low_cap = np_args(60, 7);
        linear_low_cap.model = None;
        linear_low_cap.growth = Some("linear".into());
        linear_low_cap.cap = Some(0.5);
        refusal(&linear_low_cap, "logistic-only");

        // POSITIVE CONTROL: the refusal must not swallow the logistic happy path. A cap
        // strictly above the series maximum, computed from the args themselves so the
        // helper series may change without silently disarming this assertion.
        let mut logistic_ok = np_args(60, 7);
        logistic_ok.model = None;
        logistic_ok.growth = Some("logistic".into());
        let y_max = logistic_ok
            .y
            .iter()
            .fold(f64::NEG_INFINITY, |a, v| a.max(*v));
        logistic_ok.cap = Some(y_max + 1.0);
        let r = forecast(&logistic_ok)
            .expect("logistic growth with a cap above max(y) is a legal request");
        assert_eq!(r.yhat.len(), 7, "the logistic arm must still return a band");
    }

    // ------------------------------------------ the logistic changepoint lambda bound ---
    // `06-REVIEW.md` CR-01. A 1 132-byte request, inside every door bound, buys a Poisson
    // mean of ~86 790 new changepoints on each of 1 000 simulation rows and walls at
    // 2.334 s against SC1's 2 s bar. Three cases, because a one-sided assertion would pass
    // for a bound that refuses everything and for a bound keyed on the wrong arm.

    /// The review's exact geometry, as a library request: 33 daily points (the tightest
    /// history that still earns all 25 changepoints), `horizon: 3650`, `freq: "MS"`.
    fn logistic_lambda_args(points: usize, horizon: usize, freq: &str) -> ForecastArgs {
        let t0 = days_from_civil(2015, 1, 1);
        let ds: Vec<String> = (0..points as i64).map(|i| format_ymd(t0 + i)).collect();
        let y: Vec<f64> = (0..points)
            .map(|i| {
                let t = i as f64;
                10.0 + 0.01 * t + (2.0 * std::f64::consts::PI * t / 7.0).sin()
            })
            .collect();
        ForecastArgs {
            ds,
            y,
            horizon,
            freq: Some(freq.into()),
            growth: Some("logistic".into()),
            cap: Some(50.0),
            ..ForecastArgs::default()
        }
    }

    /// The lambda a given request will actually make `predict` draw, computed the way the
    /// door computes it — through `prophet::changepoint_count`, so a test named "over"
    /// cannot quietly become a test of something under the bound if the constant moves.
    fn lambda_of(args: &ForecastArgs) -> f64 {
        use crate::dates::parse_date;
        use crate::prophet::{auto_seasonalities, changepoint_count, Mode, Spec};
        let ds: Vec<i64> = args
            .ds
            .iter()
            .map(|s| parse_date(s).expect("the helper builds valid dates"))
            .collect();
        let fut = crate::dates::future_days(
            ds[ds.len() - 1],
            args.horizon,
            args.freq.as_deref().unwrap_or("D"),
        )
        .expect("the helper builds a valid freq");
        let t_scale = (ds[ds.len() - 1] - ds[0]) as f64;
        let t_max = (fut[fut.len() - 1] - ds[0]) as f64 / t_scale;
        let spec = Spec::default_linear(auto_seasonalities(&ds, 10.0, Mode::Additive));
        changepoint_count(ds.len(), &spec) as f64 * (t_max - 1.0)
    }

    /// The measured CR-01 request is refused, and the refusal names the bound key.
    #[test]
    fn a_logistic_request_over_the_changepoint_lambda_bound_is_refused() {
        let args = logistic_lambda_args(33, 3650, "MS");
        let lambda = lambda_of(&args);
        assert!(
            lambda > crate::types::MAX_LOGISTIC_CHANGEPOINT_LAMBDA,
            "the OVER geometry must actually be over the bound it is testing: \
             lambda={lambda:.1} vs {}",
            crate::types::MAX_LOGISTIC_CHANGEPOINT_LAMBDA
        );
        refusal(&args, "max_logistic_changepoint_lambda");
    }

    /// The NEAR MISS, on the review's own control axis. Same 33 points, same `MS`
    /// frequency, a horizon that pulls lambda just under the bound — and it must still
    /// FIT and still return a full-length band. A bound proven only to refuse is not
    /// proven to refuse just what it claims.
    #[test]
    fn a_logistic_request_just_under_the_changepoint_lambda_bound_is_accepted() {
        let horizon = 840usize;
        let args = logistic_lambda_args(33, horizon, "MS");
        let lambda = lambda_of(&args);
        assert!(
            lambda <= crate::types::MAX_LOGISTIC_CHANGEPOINT_LAMBDA,
            "the UNDER geometry must actually sit under the bound: lambda={lambda:.1}"
        );
        assert!(
            lambda > 0.9 * crate::types::MAX_LOGISTIC_CHANGEPOINT_LAMBDA,
            "the near miss must be NEAR: lambda={lambda:.1} is not within 10% of the bound, \
             so this control would pass for a bound an order of magnitude away"
        );
        let r = forecast(&args).expect("a request just under the bound must fit, not refuse");
        assert_eq!(r.yhat.len(), horizon, "one row per horizon step");
        assert_eq!(r.yhat_lower.len(), horizon);
        assert_eq!(r.yhat_upper.len(), horizon);
        assert_eq!(r.trend.len(), horizon);
        assert!(r.yhat.iter().all(|v| v.is_finite()));
    }

    /// The SAME geometry on the LINEAR arm is still accepted.
    ///
    /// `predict`'s linear arm never calls `poisson` — the review measured it flat at
    /// 0.149 s while the logistic arm went to 2.334 s — so the bound must be keyed on
    /// `Growth::Logistic`. Without this case, a refusal that fired for every growth arm
    /// would pass both cases above while silently refusing the majority of real requests.
    #[test]
    fn the_linear_arm_at_the_same_geometry_is_still_accepted() {
        let mut args = logistic_lambda_args(33, 3650, "MS");
        assert!(
            lambda_of(&args) > crate::types::MAX_LOGISTIC_CHANGEPOINT_LAMBDA,
            "the geometry must be one the LOGISTIC arm would refuse, or this proves nothing"
        );
        args.growth = Some("linear".into());
        // `cap` is logistic-only (06-10), so the linear request drops it.
        args.cap = None;
        let r = forecast(&args).expect("the linear arm has no changepoint-lambda cost to bound");
        assert_eq!(r.yhat.len(), 3650, "one row per horizon step");
    }

    /// JSON `1e400` parses to `f64::INFINITY`, for which `cap <= y_max` is false.
    #[test]
    fn an_infinite_cap_is_refused() {
        let mut args = np_args(60, 7);
        args.model = None;
        args.growth = Some("logistic".into());
        args.cap = Some(f64::INFINITY);
        refusal(&args, "finite");
    }

    /// `(1.0 + w) / 2.0` rounds to EXACTLY 1.0 for the largest accepted `interval_width`,
    /// and the un-clamped tail then divided infinity by infinity.
    #[test]
    fn the_widest_accepted_interval_still_yields_a_finite_z() {
        let w = 0.999_999_999_999_999_9_f64;
        assert!(w < 1.0, "the door accepts anything strictly below 1.0");
        assert!(
            ((1.0 + w) / 2.0 - 1.0).abs() < f64::EPSILON,
            "the midpoint really does round to 1.0"
        );
        let z = normal_quantile((1.0 + w) / 2.0);
        assert!(z.is_finite(), "band z must stay finite, got {z}");
    }

    /// 20 points seven days apart select NO auto seasonality, and `season_dim() == 0`
    /// tripped trueno's `Contract transpose: input is empty` inside the fit.
    #[test]
    fn a_weekly_spaced_series_still_fits_instead_of_panicking() {
        let mut args = np_args(20, 7);
        args.ds = (0..20)
            .map(|i| format_ymd(days_from_civil(2020, 1, 1) + i * 7))
            .collect();
        args.y = (0..20).map(|i| 10.0 + f64::from(i) * 0.5).collect();
        let r = forecast(&args).expect("a weekly-spaced series is a legal request");
        assert_eq!(r.yhat.len(), 7);
        assert!(r.yhat.iter().all(|v| v.is_finite()));
    }
}
