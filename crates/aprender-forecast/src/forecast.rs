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
use crate::prophet::{
    auto_seasonalities, make_design, predict, Growth, Holiday, Mode, Seasonality, Spec,
};
use crate::types::{
    ForecastArgs, ForecastError, ForecastResponse, MAX_HORIZON, MAX_POINTS, MIN_POINTS,
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
            if growth == Growth::Logistic {
                let cap = args
                    .cap
                    .ok_or_else(|| ForecastError::Validation("logistic growth needs cap".into()))?;
                if cap <= y_max {
                    return Err(ForecastError::Validation(format!(
                        "cap {cap} must exceed max(y) = {y_max}"
                    )));
                }
            }
            let mut holidays = Vec::new();
            for h in args.holidays.as_deref().unwrap_or(&[]) {
                if h.lower_window > 0 || h.upper_window < 0 {
                    return Err(ForecastError::Validation(format!(
                        "holiday {:?}: lower_window ≤ 0 ≤ upper_window",
                        h.name
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
            let mut spec = Spec::default_linear(auto_seasonalities(&ds, 10.0, mode));
            spec.growth = growth;
            spec.cap = args.cap;
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
        // THE ONE TRACER STUB (REVIEW-06-06). The arm, the `ForecastArgs` field set and
        // this dispatch site are already in their final form; plan 06-04 Task 1 replaces
        // exactly this arm body with the verbatim spike arm and deletes this sentence.
        "neuralprophet" => Err(ForecastError::Validation(
            "model neuralprophet is ported in plan 06-04".into(),
        )),
        other => Err(ForecastError::Validation(format!(
            "model {other:?}: prophet or neuralprophet"
        ))),
    }
}

/// Acklam's inverse normal CDF (enough for band z-scores).
#[must_use]
pub fn normal_quantile(p: f64) -> f64 {
    let a = [
        -3.969683028665376e1,
        2.209460984245205e2,
        -2.759285104469687e2,
        1.383577518672690e2,
        -3.066479806614716e1,
        2.506628277459239,
    ];
    let b = [
        -5.447609879822406e1,
        1.615858368580409e2,
        -1.556989798598866e2,
        6.680131188771972e1,
        -1.328068155288572e1,
    ];
    let c = [
        -7.784894002430293e-3,
        -3.223964580411365e-1,
        -2.400758277161838,
        -2.549732539343734,
        4.374664141464968,
        2.938163982698783,
    ];
    let d = [
        7.784695709041462e-3,
        3.224671290700398e-1,
        2.445134137142996,
        3.754408661907416,
    ];
    let pl = 0.02425;
    if p < pl {
        let q = (-2.0 * p.ln()).sqrt();
        (((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5])
            / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0)
    } else if p <= 1.0 - pl {
        let q = p - 0.5;
        let r = q * q;
        (((((a[0] * r + a[1]) * r + a[2]) * r + a[3]) * r + a[4]) * r + a[5]) * q
            / (((((b[0] * r + b[1]) * r + b[2]) * r + b[3]) * r + b[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((c[0] * q + c[1]) * q + c[2]) * q + c[3]) * q + c[4]) * q + c[5])
            / ((((d[0] * q + d[1]) * q + d[2]) * q + d[3]) * q + 1.0)
    }
}
