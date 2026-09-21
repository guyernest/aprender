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
    MAX_HOLIDAY_DATES_TOTAL, MAX_HOLIDAY_DESIGN_COST, MAX_HOLIDAY_NAME_LEN, MAX_HOLIDAY_WINDOW,
    MAX_HORIZON, MAX_LOGISTIC_CHANGEPOINT_LAMBDA, MAX_NP_TRAIN_COST, MAX_POINTS, MAX_REGRESSORS,
    MAX_REGRESSOR_DESIGN_COST, MAX_SPAN_DAYS, MIN_POINTS, REGRESSOR_CONDITION_NUMBER_WARN,
    REGRESSOR_PRIOR_SCALE_MAX, REGRESSOR_PRIOR_SCALE_MIN, REGRESSOR_VIF_WARN,
};

/// Response keys a regressor name must not shadow — the THIRD part of the collision
/// reserved set, and the only part that is not derivable from any column.
///
/// Parts (a) generated column names and (b) per-component names both come out of the spec,
/// so a check built from the spec alone finds them. These eleven do not: they are pushed by
/// [`crate::prophet::predict`] AFTER the per-component loop, or they are top-level
/// [`ForecastResponse`] fields, and neither is a `Column.component` value anywhere.
///
/// Why it matters that this is a refusal and not a rename: the response component map is
/// built by `components.insert(name, value)` — a MAP INSERT — so a regressor named
/// `additive_terms` is computed as its own component and then silently OVERWRITTEN by the
/// real aggregate that `predict` pushes afterwards. The regressor's own contribution
/// vanishes from the response with no error anywhere, which is the D-21 silent-ignore class
/// this phase exists to close (threat T-06.1-13).
///
/// The first five are component keys `predict` pushes unconditionally or on the regressor
/// path (`prophet.rs`'s `holidays`, `extra_regressors_additive`,
/// `extra_regressors_multiplicative`, `additive_terms`, `multiplicative_terms`); the last
/// six are top-level `ForecastResponse` fields a component key must not shadow in a consumer
/// that flattens the response. `the_reserved_response_keys_slice_is_not_empty` pins the list
/// so a later edit cannot empty it and make the check vacuous.
/// A finite `f64` as a JSON number, and ANYTHING ELSE as an explicit `null`.
///
/// `serde_json` already renders a non-finite `f64` as `null`, so an accidental infinity and
/// a DELIBERATE withholding produce the SAME wire bytes — and only one of them is an
/// intended statement. Routing every emitted number through here makes the null a decision
/// taken in one place instead of an accident that is invisible on the wire. The same defect
/// is on record in this project as CR-03 (`UpdateEvidence::table_hash` not injective over
/// inf/NaN), which is why the fix is a funnel rather than a comment.
fn finite_or_null(v: f64) -> serde_json::Value {
    if v.is_finite() {
        serde_json::Number::from_f64(v).map_or(serde_json::Value::Null, serde_json::Value::Number)
    } else {
        serde_json::Value::Null
    }
}

/// The WIRE spelling of a mode — the string the caller sent, or the default.
///
/// `format!("{mode:?}")` would emit `Additive`, which is the Rust spelling and not the one
/// the caller used or the schema advertises.
fn mode_wire_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Additive => "additive",
        Mode::Multiplicative => "multiplicative",
    }
}

pub(crate) const RESERVED_RESPONSE_KEYS: [&str; 11] = [
    "additive_terms",
    "multiplicative_terms",
    "holidays",
    "extra_regressors_additive",
    "extra_regressors_multiplicative",
    "trend",
    "yhat",
    "yhat_lower",
    "yhat_upper",
    "ds",
    "cap",
];

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
                // FIRST in the loop, deliberately (C-07). `prophet::columns` clones this
                // name TWICE per design column and then makes it the key of an O(C log C)
                // byte-wise comparison sort, so at MAX_HOLIDAY_COLUMNS one payload
                // occurrence is amplified ~2 000:1 — and nothing bounded it at HEAD. Being
                // first also bounds every OTHER refusal message in this loop, each of which
                // formats `h.name` back to the caller.
                //
                // BYTES, not chars: `String::len` is bytes, and bytes are exactly what the
                // clone and the comparison cost. Do NOT "fix" this to `chars().count()` —
                // `a_holiday_name_whose_char_count_fits_but_whose_byte_length_does_not_is_refused`
                // is the test that catches that rewrite.
                //
                // The message names the POSITION and the LENGTH and never the name itself:
                // echoing an oversized string back re-materialises the very bytes the bound
                // refuses and reflects attacker-controlled content into logs (T-06-38).
                if h.name.len() > MAX_HOLIDAY_NAME_LEN {
                    return Err(ForecastError::Validation(format!(
                        "holiday at index {}: name is {} bytes, which exceeds \
                         max_holiday_name_len {MAX_HOLIDAY_NAME_LEN}; use a shorter label \
                         (the name is not echoed back — its LENGTH is what is at issue)",
                        holidays.len(),
                        h.name.len()
                    )));
                }
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
                // Each term is now <= MAX_HOLIDAY_DATES; the running sum is refused HERE,
                // the moment the aggregate ceiling is passed, so the remaining holidays'
                // dates are never parsed and never allocated (WR-03). The post-loop check
                // below is KEPT: this one can only report a PARTIAL sum, and a partial sum
                // reported as "the total" would be a false statement in an error message.
                holiday_dates_total += h.dates.len();
                if holiday_dates_total > MAX_HOLIDAY_DATES_TOTAL {
                    return Err(ForecastError::Validation(format!(
                        "holidays carry {holiday_dates_total} dates in the first {} holidays \
                         alone (a running total, not the request's total), which already \
                         exceeds max_holiday_dates_total {MAX_HOLIDAY_DATES_TOTAL}; send \
                         fewer holidays or fewer dates per holiday",
                        holidays.len() + 1
                    )));
                }
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
            // KEPT alongside the in-loop refusal above (WR-03). The in-loop one fires
            // early and therefore knows only a RUNNING total; this one has seen every
            // holiday, so it is the only one that can honestly report the request's EXACT
            // total. Both are O(1) and they say different true things. Unreachable for a
            // request whose sum crosses the ceiling mid-loop — which is exactly why the
            // position test asserts on the MESSAGE and not on the constant's presence.
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
            // ---- EXTERNAL REGRESSORS (D-22) ----
            //
            // Split each caller array into the HISTORY prefix (`ds.len()` rows, what the
            // standardisation constants are computed over) and the FUTURE tail (`horizon`
            // rows, what `predict` is handed). `fut` has length `horizon`, NOT
            // `ds.len() + horizon`, so handing `predict` the whole array would silently
            // offset every future feature — `predict` re-checks the length for that reason.
            let mut reg_std: Vec<crate::regressors::Standardized> = Vec::new();
            let mut reg_hist: Vec<Vec<f64>> = Vec::new();
            let mut reg_fut: Vec<Vec<f64>> = Vec::new();
            if let Some(regs) = args.regressors.as_ref() {
                // ---- 8. NAME LENGTH, in BYTES, FIRST in the block ----
                //
                // FIRST for the reason the holiday loop's identical check is first (C-07):
                // every OTHER refusal below formats caller data — six of them format the
                // index, two format the NAME — so an unbounded name would be reflected back
                // through whichever check fires. The length check bounds all of them.
                //
                // Reuses MAX_HOLIDAY_NAME_LEN rather than adding a fourth name ceiling: the
                // amplification argument is the SAME one C-07 records. `regressors::splice`
                // clones the name TWICE per design column (the `Column.name` and the
                // `Column.component`), `predict`'s component-name dedup compares it
                // O(C * distinct_components) times, and it becomes a key of the serialized
                // `components` map. One ceiling, one derivation, one place to raise it.
                //
                // BYTES, not chars: `String::len` is bytes and bytes are what the clone and
                // the comparison cost. The message names the INDEX and the LENGTH and NEVER
                // the name — echoing an oversized string back re-materialises the very bytes
                // the bound refuses and reflects attacker-controlled content into logs
                // (T-06.1-14).
                if let Some((i, r)) = regs
                    .iter()
                    .enumerate()
                    .find(|(_, r)| r.name.len() > MAX_HOLIDAY_NAME_LEN)
                {
                    return Err(ForecastError::Validation(format!(
                        "regressor at index {i}: name is {} bytes, which exceeds \
                         max_holiday_name_len {MAX_HOLIDAY_NAME_LEN}; use a shorter label \
                         (the name is not echoed back — its LENGTH is what is at issue)",
                        r.name.len()
                    )));
                }
                // ---- 7. THE COUNT CEILING, MEASURED (replaces the wave-1 interim 50) ----
                //
                // Before any allocation proportional to the payload. `fit_max_regressors` is
                // its own ceiling rather than a consequence of the product below, because
                // the product is satisfiable by trading rows for columns while the
                // identifiability diagnostic's O(K^3) factorisation is a function of the
                // COUNT alone (cost axis C-17).
                if regs.len() > MAX_REGRESSORS {
                    return Err(ForecastError::Validation(format!(
                        "the request carries {} regressors, which exceeds \
                         max_regressors {MAX_REGRESSORS}; send fewer regressors",
                        regs.len()
                    )));
                }
                // ---- 9. THE DESIGN-COST PRODUCT, and it STATES ITS OPERANDS ----
                //
                // `ds.len() <= MAX_POINTS` (20 000), `args.horizon <= MAX_HORIZON` (3 650)
                // and `regs.len() <= MAX_REGRESSORS` are all already refused above, so this
                // product is at most 4 730 000 — six orders of magnitude below `usize::MAX`
                // on every supported target. A plain multiply cannot overflow here, and
                // `saturating_mul` would only obscure that the factors are bounded.
                let regressor_cells = (ds.len() + args.horizon) * regs.len();
                if regressor_cells > MAX_REGRESSOR_DESIGN_COST {
                    return Err(ForecastError::Validation(format!(
                        "regressors expand to {regressor_cells} design feature cells \
                         ((points + horizon) x n_regressors = ({} + {}) x {}), which \
                         exceeds max_regressor_design_cost {MAX_REGRESSOR_DESIGN_COST}; \
                         send fewer regressors, a shorter history or a shorter horizon",
                        ds.len(),
                        args.horizon,
                        regs.len()
                    )));
                }
                // ---- 10. NAME COLLISION WITH A RESPONSE COMPONENT KEY ----
                //
                // The response component map is `components.insert(name, value)` — a MAP
                // INSERT — so a duplicate key silently OVERWRITES the earlier entry and the
                // operator sees ONE component where TWO were computed. Check 5 below closes
                // regressor-against-regressor; this closes regressor-against-everything-else,
                // over a reserved set with THREE parts (T-06.1-13):
                //
                //   (a) GENERATED COLUMN NAMES — `prophet::columns(&spec)`, i.e. the
                //       `{seasonality}_delim_{n}` and `{holiday}_delim_{sign}{offset}` forms.
                //   (b) PER-COMPONENT NAMES — the `Column.component` values behind those
                //       columns. `predict` pushes one component per DISTINCT `component`
                //       value, so a regressor named `yearly` collides even though `yearly`
                //       is not itself a generated column name; and a regressor named after a
                //       declared holiday collides for the same reason.
                //   (c) RESERVED RESPONSE KEYS — [`RESERVED_RESPONSE_KEYS`], which are
                //       invisible to (a) and (b) because they are pushed after the
                //       per-component loop or are top-level response fields.
                //
                // The spec is fully assembled by this point (seasonalities, holidays and the
                // empty-design weekly fallback are all settled above), so the reserved set is
                // built ONCE from the real spec rather than from a reconstruction of it.
                let mut reserved: std::collections::BTreeMap<String, &'static str> =
                    std::collections::BTreeMap::new();
                for c in crate::prophet::columns(&spec) {
                    reserved.insert(c.name, "a generated design column name");
                    reserved.insert(c.component, "a response component name");
                }
                for k in RESERVED_RESPONSE_KEYS {
                    reserved.insert(k.to_string(), "a reserved response key");
                }
                if let Some((i, r, part)) = regs
                    .iter()
                    .enumerate()
                    .find_map(|(i, r)| reserved.get(r.name.as_str()).map(|part| (i, r, *part)))
                {
                    return Err(ForecastError::Validation(format!(
                        "regressor {i} is named {:?}, which is already {part} in the \
                         response; the component map is keyed by name and one would \
                         silently overwrite the other, so rename the regressor",
                        r.name
                    )));
                }
                // ---- 5. DUPLICATE NAMES ----
                //
                // O(R log R) via a BTreeSet, not a quadratic scan: the component map is
                // keyed by name, so the second of a pair would silently overwrite the first.
                let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
                for r in regs {
                    if !seen.insert(r.name.as_str()) {
                        return Err(ForecastError::Validation(format!(
                            "regressor name {:?} appears more than once; the response \
                             component map is keyed by name and the second would silently \
                             replace the first, so give each regressor a distinct name",
                            r.name
                        )));
                    }
                }

                for (i, r) in regs.iter().enumerate() {
                    // ---- 1. LENGTH TIE, before anything is copied ----
                    let want = ds.len() + args.horizon;
                    if r.values.len() != want {
                        return Err(ForecastError::Validation(format!(
                            "regressor {i} carries {} values but {want} are required \
                             (points + horizon = {} + {}); the array must cover the \
                             history rows AND the horizon rows",
                            r.values.len(),
                            ds.len(),
                            args.horizon
                        )));
                    }
                    // ---- 2. INPUT FINITENESS ----
                    //
                    // The same class the `cap` finiteness check closed: JSON `1e400` parses
                    // to infinity. The row index is named, the VALUE is not echoed.
                    if let Some(bad) = r.values.iter().position(|v| !v.is_finite()) {
                        return Err(ForecastError::Validation(format!(
                            "regressor {i} carries a non-finite value at row {bad}; every \
                             value must be finite (note that JSON 1e400 parses to infinity)"
                        )));
                    }
                    // ---- 3. MODE ALLOWLIST, refused BY NAME ----
                    let mode = match r.mode.as_deref() {
                        None | Some("additive") => Mode::Additive,
                        Some("multiplicative") => Mode::Multiplicative,
                        Some(other) => {
                            return Err(ForecastError::Validation(format!(
                                "regressor {i} mode {other:?} is not supported; use \
                                 \"additive\" or \"multiplicative\""
                            )))
                        }
                    };
                    // ---- 4. PRIOR SCALE, in a NUMERICALLY USABLE range ----
                    //
                    // Not merely "> 0": the objective and gradient SQUARE this into a
                    // denominator (prophet.rs:536, :685), so f64::MIN_POSITIVE squares to
                    // exactly 0.0 and the initial zero coefficients meet 0.0/0.0.
                    let prior_scale = r.prior_scale.unwrap_or(10.0);
                    if !prior_scale.is_finite()
                        || prior_scale < REGRESSOR_PRIOR_SCALE_MIN
                        || prior_scale > REGRESSOR_PRIOR_SCALE_MAX
                    {
                        return Err(ForecastError::Validation(format!(
                            "regressor {i} prior_scale {prior_scale:e} is outside the \
                             usable range [{REGRESSOR_PRIOR_SCALE_MIN:e}, \
                             {REGRESSOR_PRIOR_SCALE_MAX:e}]; the objective squares it into \
                             a denominator, so a smaller value underflows to zero and \
                             yields a NaN fit"
                        )));
                    }

                    let spec_r = crate::regressors::RegressorSpec {
                        name: r.name.clone(),
                        mode,
                        prior_scale,
                        standardize: r.standardize,
                    };
                    let (hist, futv) = r.values.split_at(ds.len());
                    let st = crate::regressors::standardize_one(&spec_r, hist);

                    // ---- 6a. ZERO SPREAD ----
                    //
                    // Only when the column was actually STANDARDISED: the auto {0,1}
                    // carve-out legitimately returns std = 1.0 and must not be caught here.
                    if st.std == 0.0 {
                        return Err(ForecastError::Validation(format!(
                            "regressor {i} is constant over the history rows and carries \
                             no information; drop the regressor"
                        )));
                    }
                    // ---- 6b. POST-ARITHMETIC FINITENESS ----
                    //
                    // A SEPARATE check from 2, and both are needed: finite inputs are not
                    // sufficient, because 1e300 values overflow the sum of squares in the
                    // variance. Every DERIVED quantity is re-checked.
                    if !st.mu.is_finite() {
                        return Err(ForecastError::Validation(format!(
                            "regressor {i} has a non-finite mean after standardisation; \
                             the values are individually finite but their sum overflows"
                        )));
                    }
                    if !st.std.is_finite() {
                        return Err(ForecastError::Validation(format!(
                            "regressor {i} has a non-finite standard deviation after \
                             standardisation; the values are individually finite but the \
                             sum of squares overflows"
                        )));
                    }
                    if let Some(bad) = hist
                        .iter()
                        .position(|v| !((v - st.mu) / st.std).is_finite())
                    {
                        return Err(ForecastError::Validation(format!(
                            "regressor {i} has a non-finite standardised cell at row \
                             {bad}; the value is finite but (value - mu) / std is not"
                        )));
                    }

                    reg_std.push(st);
                    reg_hist.push(hist.to_vec());
                    reg_fut.push(futv.to_vec());
                }
            }

            let mut design = make_design(&ds, &args.y, &spec);
            crate::regressors::splice(&mut design, &reg_std, &reg_hist);
            let t0 = Instant::now();
            // UNCHANGED: the optimiser and the gradient need no regressor awareness — the
            // spliced columns are ordinary design columns with ordinary prior scales.
            let (p, info) = fit_prophet(&design, 8);
            let fit_seconds = t0.elapsed().as_secs_f64();
            let t1 = Instant::now();
            let fc = predict(
                &design,
                &p,
                &fut,
                seed,
                &crate::regressors::RegressorChannel {
                    specs: &reg_std,
                    values: &reg_fut,
                },
            )
            // The door BUILT this channel (`reg_std`/`reg_fut` above); the caller cannot
            // shape it. `predict`'s three channel invariants are `Validation` because a
            // direct Rust caller really is supplying the channel — but here a breach is the
            // door's own arithmetic, so reporting it as the caller's bad input would be a
            // lie the transport then repeats (`map_error`: "the caller's fault stays the
            // caller's fault"). Matches 06.1-07's rule for the n_grid/n_train guard:
            // `Internal` rather than `Validation` because the input was already accepted.
            .map_err(|e| {
                ForecastError::Internal(format!("the door built a malformed regressor channel: {e}"))
            })?;
            let predict_seconds = t1.elapsed().as_secs_f64();
            let mut components = serde_json::Map::new();
            for (n, v) in &fc.components {
                components.insert(n.clone(), serde_json::json!(v));
            }
            // ---- THE IDENTIFIABILITY DIAGNOSTIC (D-35, D-36) ----
            //
            // Computed AFTER the fit and only when the request actually carried regressors.
            // Both keys are built CONDITIONALLY and inserted into the map below rather than
            // being fields with a `skip_serializing_if`: a serialisation attribute that
            // misfires emits `"regressors": []` on EVERY response, and `invariance::
            // signature` hashes the WHOLE `diagnostics` object, so that would change every
            // recorded baseline and break SC2. The mechanism has to be "the key is never
            // constructed", not "the key is usually omitted".
            //
            // Nothing equivalent exists on the neuralprophet arm, deliberately (D-37): VIF
            // and the condition number are properties of the design matrix PROPHET builds,
            // and AR absorption is a training dynamic rather than column collinearity, so a
            // green VIF there would reassure about the wrong thing.
            let identifiability = if reg_std.is_empty() {
                None
            } else {
                Some(crate::regressors::identifiability(
                    &design,
                    &reg_std,
                    &p.beta,
                    REGRESSOR_VIF_WARN,
                    REGRESSOR_CONDITION_NUMBER_WARN,
                ))
            };
            let mut response = ForecastResponse {
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
            };
            if let Some(id) = identifiability {
                let diag = response
                    .diagnostics
                    .as_object_mut()
                    .expect("the prophet diagnostics value is built as a JSON object above");
                let mut rows = Vec::with_capacity(id.regressors.len());
                for r in &id.regressors {
                    let mut o = serde_json::Map::new();
                    o.insert("name".into(), serde_json::Value::String(r.name.clone()));
                    o.insert(
                        "mode".into(),
                        serde_json::Value::String(mode_wire_name(r.mode).into()),
                    );
                    o.insert("mu".into(), finite_or_null(r.mu));
                    o.insert("std".into(), finite_or_null(r.std));
                    o.insert(
                        "vif".into(),
                        r.vif.map_or(serde_json::Value::Null, finite_or_null),
                    );
                    // `warning` is ABSENT, not null, when the column is clean: a key that is
                    // always present with a null is a key a consumer has to branch on.
                    if let Some(w) = &r.warning {
                        o.insert("warning".into(), serde_json::Value::String(w.clone()));
                    }
                    rows.push(serde_json::Value::Object(o));
                }
                diag.insert("regressors".into(), serde_json::Value::Array(rows));
                let mut sib = serde_json::Map::new();
                sib.insert("scope".into(), serde_json::Value::String(id.scope.clone()));
                sib.insert(
                    "condition_number".into(),
                    id.condition_number
                        .map_or(serde_json::Value::Null, finite_or_null),
                );
                sib.insert(
                    "condition_number_warning".into(),
                    id.condition_number_warning
                        .clone()
                        .map_or(serde_json::Value::Null, serde_json::Value::String),
                );
                sib.insert(
                    "ridge".into(),
                    id.ridge.map_or(serde_json::Value::Null, finite_or_null),
                );
                sib.insert(
                    "regularized".into(),
                    serde_json::Value::Bool(id.regularized),
                );
                sib.insert(
                    "status".into(),
                    serde_json::Value::String(id.status.as_str().into()),
                );
                diag.insert(
                    "regressors_identifiability".into(),
                    serde_json::Value::Object(sib),
                );
            }
            Ok(response)
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
            // REFUSED, not accepted-and-ignored (D-21). An argument that is silently
            // dropped is the exact failure this phase exists to prevent: the caller gets a
            // forecast that looks like it used their covariate and did not. Plan 06.1-07
            // replaces this message with the D-26/D-28 rules that make it work here.
            if args.regressors.is_some() {
                return Err(ForecastError::Validation(
                    "regressors on model \"neuralprophet\" are not enabled yet; set model \
                     to \"prophet\""
                        .into(),
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
            // ---- the NEURALPROPHET TRAINING COST bound, at THE door and BEFORE np::train ----
            //
            // Cost axis C-08, and the ONLY axis in this door that had no budget of any kind
            // on its path: `fit::FIT_BUDGET_SECS` is read at exactly one place, inside
            // `fit::fit_prophet`, and this arm never enters that function. `n_lags <= 365`,
            // `n_samples <= n_train_grid <= MAX_SPAN_DAYS` and `epochs <= 500` are each
            // enforced on their own; what was never checked is their PRODUCT, times the 2-or-3
            // learning-rate sweep run below. MEASURED: the worst legal request (20 000
            // contiguous daily points, n_lags 365, horizon 3650) walls at 47.924 s on release,
            // 24x over SC1's 2 s bar.
            //
            // The price is computed by `np::request_train_cost`, built out of the SAME three
            // functions used to configure the sweep below (`door_lr_sweep`, `door_epochs`,
            // `n_training_samples`), so the door cannot price a request differently from the
            // work it then spends.
            let np_cost = np::request_train_cost(&d, n_train, n_lags);
            if np_train_cost_is_over(np_cost) {
                let n_samples = np::n_training_samples(&d, n_lags);
                let epochs = np::door_epochs(n_train, n_samples, n_lags);
                let sweep = np::door_lr_sweep(n_lags).len();
                return Err(ForecastError::Validation(format!(
                    "this neuralprophet request buys {np_cost} units of training work \
                     (learning-rate sweep {sweep} x epochs {epochs} x samples {n_samples} x \
                     (n_lags + 1) {}), which exceeds max_np_train_cost {MAX_NP_TRAIN_COST}; \
                     reduce n_lags, shorten the history, or narrow the series span",
                    n_lags + 1
                )));
            }
            let t0 = Instant::now();
            // spike-002 lesson: a short lr sweep selected by TRAIN loss stands in for NP's
            // range test (D-10 — never select by test error); 4x the auto epochs when lags
            // are on (the linear AR case needs the budget), capped at 320.
            let mut best: Option<(f64, f64, np::NpModel, np::TrainLog)> = None;
            // The sweep and the epoch rule come from `np`, not from literals here, so the
            // cost priced above is the cost this loop actually spends (C-08).
            let lrs: &[f64] = np::door_lr_sweep(n_lags);
            let n_samples = np::n_training_samples(&d, n_lags);
            for &lr in lrs {
                let cfg = np::TrainConfig {
                    n_lags,
                    ar_layers: if n_lags > 0 { vec![32] } else { vec![] },
                    max_lr: lr,
                    epochs: Some(np::door_epochs(n_train, n_samples, n_lags)),
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
/// The door's C-08 comparison, named ONCE so the boundary can be tested without paying it.
///
/// It is `>`, not `>=`: a request pricing EXACTLY at the bound is accepted. That one-off is
/// invisible to any near-miss control this crate can afford to run — at the bound a single
/// request costs 45.619 s on a debug profile — so
/// [`tests::the_np_train_cost_bound_is_exclusive_not_inclusive`] pins the comparison here
/// instead.
fn np_train_cost_is_over(cost: u64) -> bool {
    cost > MAX_NP_TRAIN_COST
}

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
    use crate::types::{
        ForecastArgs, ForecastError, MAX_NP_TRAIN_COST, REGRESSOR_PRIOR_SCALE_MAX,
        REGRESSOR_PRIOR_SCALE_MIN,
    };
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
            regressors: None,
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

    /// C-08 — the NeuralProphet training path had NO budget of any kind.
    ///
    /// The worst LEGAL request (20 000 contiguous daily points, `n_lags` 365, horizon 3650)
    /// prices at 718 641 000 and walled at **47.924 s** on a release build — 24x SC1's 2 s
    /// bar — because `fit::FIT_BUDGET_SECS` is read only inside `fit::fit_prophet`, which
    /// this arm never enters. The refusal names the observed cost, all four factors, the
    /// bound key and what to reduce.
    #[test]
    fn a_neuralprophet_request_over_the_train_cost_bound_is_refused() {
        // Same shape as the structural maximum, at a history short enough that the REFUSAL
        // (which fires before any training) stays instant.
        let mut args = np_args(2_000, 365);
        args.n_lags = Some(365);
        refusal(&args, "max_np_train_cost");
    }

    /// The positive control the always-run suite CAN afford: a real NeuralProphet request
    /// with lags on, priced under the bound, accepted through the door and returning the
    /// full response shape. The at-the-bound near miss is in `np::wall::np_train_wall`
    /// (`NP_WALL_MODE=at_bound_*`, three compositions at 93-99% of the bound, 1.402-1.642 s
    /// on release) because at the bound ONE request costs 45.619 s on a debug profile.
    #[test]
    fn a_neuralprophet_request_under_the_train_cost_bound_is_accepted() {
        let mut args = np_args(120, 7);
        args.n_lags = Some(7);
        let r = forecast(&args).expect("a request under the training-cost bound must fit");
        assert_eq!(r.model, "neuralprophet");
        assert_eq!(r.yhat.len(), 7, "one row per horizon step");
        assert_eq!(r.yhat_lower.len(), 7);
        assert_eq!(r.yhat_upper.len(), 7);
    }

    /// The bound must not refuse the geometry the NeuralProphet parity ladder proves the
    /// model CORRECT on.
    ///
    /// `np::parity` runs Peyton Manning (2 905 rows over a 2 964-day span) at `n_lags` 0 and
    /// 30. Through the door those price at 697 200 and 14 552 640, and the second is 97% of
    /// the bound — so a value of 14 000 000, which the wall measurements would also have
    /// allowed, would refuse the ladder's own request. This is arithmetic on the door's own
    /// pricing functions, so it costs nothing to run and turns red the moment the bound is
    /// lowered past the ladder.
    #[test]
    fn the_np_parity_ladder_geometry_prices_under_the_train_cost_bound() {
        const PEYTON_ROWS: usize = 2_905;
        const PEYTON_SPAN_DAYS: usize = 2_964;
        for n_lags in [0usize, 30] {
            let n_samples = if n_lags == 0 {
                PEYTON_ROWS
            } else {
                PEYTON_SPAN_DAYS - n_lags
            };
            let epochs = crate::np::door_epochs(PEYTON_ROWS, n_samples, n_lags);
            let sweep = crate::np::door_lr_sweep(n_lags).len() as u64;
            let cost = crate::np::train_cost(n_samples, epochs, n_lags) * sweep;
            assert!(
                cost <= MAX_NP_TRAIN_COST,
                "np::parity's own Peyton rung at n_lags={n_lags} prices at {cost}, which the \
                 bound {MAX_NP_TRAIN_COST} would REFUSE — a bound below the geometry the \
                 ladder proves correctness on is wrong"
            );
        }
    }

    /// The comparison is `>`, not `>=`: a request pricing EXACTLY at the bound is accepted.
    ///
    /// No near-miss this crate can afford to run reaches the boundary itself — at the bound
    /// one request costs 45.619 s on a debug profile — so the off-by-one is pinned on the
    /// door's own comparison instead of on a request nobody will pay for.
    #[test]
    fn the_np_train_cost_bound_is_exclusive_not_inclusive() {
        assert!(
            !super::np_train_cost_is_over(MAX_NP_TRAIN_COST),
            "a request pricing EXACTLY at max_np_train_cost must be accepted"
        );
        assert!(
            super::np_train_cost_is_over(MAX_NP_TRAIN_COST + 1),
            "one unit over the bound must be refused"
        );
    }

    /// The door prices a request with the SAME functions it then uses to configure the
    /// sweep, so the bound cannot be evaded by the two disagreeing — the drift hazard
    /// `prophet::changepoint_count` was extracted to avoid in 06-14.
    #[test]
    fn the_door_prices_a_request_at_exactly_the_work_it_configures() {
        let (ds, y) = synthetic_daily(120);
        let days: Vec<i64> = ds
            .iter()
            .map(|s| crate::dates::parse_date(s).expect("valid"))
            .collect();
        for n_lags in [0usize, 7, 30] {
            let d = crate::np::NpData::new(&days, &y, days.len(), 10, 0.8);
            let n_samples = crate::np::n_training_samples(&d, n_lags);
            let epochs = crate::np::door_epochs(days.len(), n_samples, n_lags);
            let sweep = crate::np::door_lr_sweep(n_lags).len() as u64;
            assert_eq!(
                crate::np::request_train_cost(&d, days.len(), n_lags),
                crate::np::train_cost(n_samples, epochs, n_lags) * sweep,
                "request_train_cost must be exactly sweep x train_cost at n_lags={n_lags}"
            );
        }
    }

    /// C-07 — `holidays[].name` had NO enforcement of any kind at the close of 06-14.
    ///
    /// One payload occurrence becomes TWO owned `String`s per design column in
    /// `prophet::columns` and then the sort key of an `O(C log C)` byte-wise comparison
    /// sort, so at `MAX_HOLIDAY_COLUMNS` a single name is amplified ~2 000:1. The refusal
    /// names the holiday's POSITION and the observed LENGTH and never the name itself —
    /// echoing an oversized string back re-materialises the very bytes the bound refuses
    /// (T-06-38).
    #[test]
    fn a_holiday_name_over_the_length_bound_is_refused() {
        let args = named_holiday_args("n".repeat(crate::types::MAX_HOLIDAY_NAME_LEN + 1));
        refusal(&args, "max_holiday_name_len");
    }

    /// The NEAR MISS: exactly at the bound, which must still be accepted and must still
    /// return the full response. A comparison written as `>=` fails here and nowhere else.
    #[test]
    fn a_holiday_name_at_the_length_bound_is_accepted() {
        let name = "n".repeat(crate::types::MAX_HOLIDAY_NAME_LEN);
        assert_eq!(
            name.len(),
            crate::types::MAX_HOLIDAY_NAME_LEN,
            "this control is only a NEAR MISS if it sits exactly ON the bound"
        );
        let args = named_holiday_args(name);
        let r = forecast(&args).expect("a holiday name exactly at the bound must fit");
        assert_eq!(r.yhat.len(), 7, "one row per horizon step");
    }

    /// The bound is on BYTES, and this is the case that says so. `é` is two UTF-8 bytes, so
    /// this name's CHARACTER count is comfortably under the bound while its BYTE length is
    /// over it — and bytes are what the clone and the byte-wise comparison actually cost.
    /// A rewrite of `h.name.len()` to `h.name.chars().count()` turns this test red and
    /// nothing else in the suite.
    #[test]
    fn a_holiday_name_whose_char_count_fits_but_whose_byte_length_does_not_is_refused() {
        let chars = crate::types::MAX_HOLIDAY_NAME_LEN / 2 + 1;
        let name = "é".repeat(chars);
        assert!(
            name.chars().count() <= crate::types::MAX_HOLIDAY_NAME_LEN,
            "the CHAR count must sit under the bound, or this case proves nothing about \
             which quantity is measured"
        );
        assert!(
            name.len() > crate::types::MAX_HOLIDAY_NAME_LEN,
            "the BYTE length must sit over the bound"
        );
        let args = named_holiday_args(name);
        refusal(&args, "max_holiday_name_len");
    }

    /// T-06-38 — the refusal names the LENGTH, never the name.
    ///
    /// Echoing an oversized attacker-controlled string back re-materialises the very bytes
    /// the bound refuses and reflects that content into every log the refusal reaches. The
    /// assertion is non-vacuous by construction: `TOKEN` is what the message WOULD contain
    /// if `h.name` were interpolated, and every OTHER refusal in the same loop does
    /// interpolate `h.name` — which is exactly why this check is FIRST in the loop.
    #[test]
    fn the_oversized_name_refusal_names_the_length_and_not_the_name() {
        const TOKEN: &str = "SECRETHOLIDAYLABEL";
        let over = crate::types::MAX_HOLIDAY_NAME_LEN + 1;
        let name = TOKEN.repeat(over.div_ceil(TOKEN.len()))[..over].to_string();
        assert!(
            name.contains(TOKEN),
            "the probe token must survive truncation"
        );
        let args = named_holiday_args(name.clone());
        match forecast(&args) {
            Err(ForecastError::Validation(m)) => {
                assert!(
                    m.contains("max_holiday_name_len"),
                    "the refusal must name the bound key; got {m:?}"
                );
                assert!(
                    m.contains(&over.to_string()),
                    "the refusal must report the OBSERVED byte length ({over}); got {m:?}"
                );
                assert!(
                    m.contains("index 0"),
                    "the refusal must name WHICH holiday; got {m:?}"
                );
                assert!(
                    !m.contains(TOKEN),
                    "the refusal must NOT echo the oversized name back (T-06-38); got {m:?}"
                );
            }
            other => panic!(
                "expected a Validation refusal, got {:?}",
                other.map(|r| r.model)
            ),
        }
    }

    /// The bound is ONE-SIDED by design. An empty name is a separate question (does a
    /// component need a label at all?) that this plan does not open, and a length bound
    /// that quietly grew a lower side would be answering it by accident.
    #[test]
    fn an_empty_holiday_name_is_not_refused_by_the_length_bound() {
        let args = named_holiday_args(String::new());
        let r = forecast(&args).expect("the length bound is one-sided: it must not refuse 0");
        assert_eq!(r.yhat.len(), 7);
    }

    /// A prophet request carrying exactly one holiday whose NAME is the variable under test.
    /// Every other knob is comfortably inside its own bound, so only the name can refuse.
    fn named_holiday_args(name: String) -> ForecastArgs {
        let (ds, y) = synthetic_daily(60);
        let mut args = np_args(60, 7);
        args.model = None;
        args.ds = ds;
        args.y = y;
        args.holidays = Some(vec![crate::types::HolidayArg {
            name,
            dates: vec![format_ymd(days_from_civil(2020, 2, 1))],
            lower_window: 0,
            upper_window: 0,
        }]);
        args
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

    // ---------------------------------------------------------------------------------
    // Plan 06.1-01 Task 2: the six external-regressor door refusals.
    //
    // Every refusal is paired with a POSITIVE CONTROL that is ACCEPTED, so no check can
    // pass by refusing everything — the failure mode a refusal-only suite cannot see.
    // ---------------------------------------------------------------------------------

    /// A prophet request over `n` daily points with `horizon` future steps and the given
    /// regressors, none of which is refused by anything except what the test is probing.
    fn reg_args(n: usize, horizon: usize, regs: Vec<crate::types::RegressorArg>) -> ForecastArgs {
        let (ds, y) = synthetic_daily(n);
        ForecastArgs {
            ds,
            y,
            horizon,
            seed: Some(42),
            regressors: Some(regs),
            ..ForecastArgs::default()
        }
    }

    /// A well-formed regressor of exactly the required length, varying enough to have a
    /// non-zero spread.
    fn good_reg(name: &str, n: usize, horizon: usize) -> crate::types::RegressorArg {
        crate::types::RegressorArg {
            name: name.into(),
            values: (0..n + horizon)
                .map(|i| f64::from(i as u32) * 0.5 + 1.0)
                .collect(),
            mode: None,
            prior_scale: None,
            standardize: None,
        }
    }

    /// CONTROL for every refusal below: a well-formed four-regressor request is ACCEPTED
    /// and its regressors reach the response as components.
    #[test]
    fn regressors_that_are_well_formed_are_accepted() {
        let args = reg_args(60, 7, vec![good_reg("promo", 60, 7)]);
        let r = forecast(&args).expect("a well-formed regressor request must be accepted");
        assert!(
            r.components.contains_key("promo")
                && r.components.contains_key("extra_regressors_additive"),
            "the regressor must reach the response; got {:?}",
            r.components.keys().collect::<Vec<_>>()
        );
    }

    /// CHECK 1, the length tie, from BOTH sides plus the exact value.
    ///
    /// Three shapes because one input is an anecdote (CLAUDE.md rule 6): a check keyed on
    /// the wrong side of the comparison passes two of the three.
    #[test]
    fn a_regressor_whose_values_length_is_wrong_is_refused_from_both_sides() {
        for (label, len) in [("too short", 66usize), ("too long", 68)] {
            let mut reg = good_reg("promo", 60, 7);
            reg.values = vec![1.0; len];
            let args = reg_args(60, 7, vec![reg]);
            match forecast(&args) {
                Err(ForecastError::Validation(m)) => {
                    assert!(
                        m.contains("67"),
                        "{label}: must state the required length, got {m:?}"
                    );
                    assert!(
                        m.contains(&len.to_string()),
                        "{label}: must state the length received, got {m:?}"
                    );
                }
                other => panic!("{label} must be refused, got {:?}", other.map(|r| r.model)),
            }
        }
        // POSITIVE CONTROL: exactly points + horizon is accepted.
        let args = reg_args(60, 7, vec![good_reg("promo", 60, 7)]);
        assert_eq!(args.regressors.as_ref().expect("regs")[0].values.len(), 67);
        forecast(&args).expect("exactly points + horizon must be accepted");
    }

    /// CHECK 2, input finiteness. JSON `1e400` parses to infinity — the same class the
    /// `cap` finiteness check closed.
    #[test]
    fn a_regressor_with_a_non_finite_value_is_refused() {
        for bad in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            let mut reg = good_reg("promo", 60, 7);
            reg.values[13] = bad;
            let args = reg_args(60, 7, vec![reg]);
            refusal(&args, "row 13");
        }
        // POSITIVE CONTROL: a large-but-finite value is accepted.
        let mut reg = good_reg("promo", 60, 7);
        reg.values[13] = 1.0e12;
        forecast(&reg_args(60, 7, vec![reg])).expect("a large finite value must be accepted");
    }

    /// CHECK 3, the mode allowlist, refused BY NAME.
    #[test]
    fn a_regressor_mode_outside_the_allowlist_is_refused_by_name() {
        let mut reg = good_reg("promo", 60, 7);
        reg.mode = Some("exponential".into());
        refusal(&reg_args(60, 7, vec![reg]), "exponential");
        // POSITIVE CONTROLS: both accepted values.
        for m in ["additive", "multiplicative"] {
            let mut reg = good_reg("promo", 60, 7);
            reg.mode = Some(m.into());
            forecast(&reg_args(60, 7, vec![reg]))
                .unwrap_or_else(|e| panic!("mode {m:?} must be accepted: {e}"));
        }
    }

    /// CHECK 4, the prior-scale domain.
    ///
    /// The bound is not "greater than zero": the objective SQUARES it into a denominator
    /// (`prophet.rs:536`, `:685`), so `f64::MIN_POSITIVE` squares to exactly `0.0` and the
    /// initial zero coefficients meet `0.0 / 0.0`.
    #[test]
    fn a_regressor_prior_scale_outside_the_usable_range_is_refused() {
        for (label, bad) in [
            ("MIN_POSITIVE squares to zero", f64::MIN_POSITIVE),
            (
                "one ULP below the floor",
                f64::from_bits(REGRESSOR_PRIOR_SCALE_MIN.to_bits() - 1),
            ),
            ("above the ceiling", REGRESSOR_PRIOR_SCALE_MAX * 10.0),
            ("zero", 0.0),
            ("negative", -1.0),
            ("infinite", f64::INFINITY),
            ("nan", f64::NAN),
        ] {
            let mut reg = good_reg("promo", 60, 7);
            reg.prior_scale = Some(bad);
            match forecast(&reg_args(60, 7, vec![reg])) {
                Err(ForecastError::Validation(m)) => {
                    assert!(m.contains("prior_scale"), "{label}: got {m:?}");
                }
                other => panic!("{label} must be refused, got {:?}", other.map(|r| r.model)),
            }
        }
    }

    /// The POSITIVE CONTROL for check 4: a bound that has not been shown to keep the
    /// arithmetic finite is a guess.
    ///
    /// NOTE what this does and does not establish. It pins FINITENESS at both ends, which
    /// is this plan's acceptance criterion. It does NOT establish that a prior scale at the
    /// floor produces a meaningful fit — measurement says it does not, and
    /// `the_prior_scale_floor_is_a_representability_bound_not_a_usability_bound` below
    /// records that explicitly rather than leaving it as a comment nobody re-runs.
    #[test]
    fn a_regressor_prior_scale_at_the_floor_fits_finitely() {
        for (label, ps) in [
            ("floor", REGRESSOR_PRIOR_SCALE_MIN),
            ("ceiling", REGRESSOR_PRIOR_SCALE_MAX),
            ("default-ish", 10.0),
        ] {
            let mut reg = good_reg("promo", 60, 7);
            reg.prior_scale = Some(ps);
            let r = forecast(&reg_args(60, 7, vec![reg]))
                .unwrap_or_else(|e| panic!("{label} ({ps:e}) must be accepted: {e}"));
            assert!(
                r.yhat.iter().all(|v| v.is_finite()) && r.trend.iter().all(|v| v.is_finite()),
                "{label} ({ps:e}): every yhat and trend value must be finite — a NaN here is \
                 the 0/0 the bound exists to prevent"
            );
            let objective = r.diagnostics["lbfgs"]["objective"]
                .as_f64()
                .expect("the prophet arm reports its objective");
            assert!(
                objective.is_finite(),
                "{label} ({ps:e}): the fitted objective must be finite, got {objective}"
            );
        }
    }

    /// CHECK 5, duplicate names. The component map is keyed by name.
    #[test]
    fn regressors_sharing_a_name_are_refused() {
        let args = reg_args(
            60,
            7,
            vec![good_reg("promo", 60, 7), good_reg("promo", 60, 7)],
        );
        refusal(&args, "promo");
        // POSITIVE CONTROL: two DISTINCT names are accepted.
        let args = reg_args(
            60,
            7,
            vec![good_reg("promo", 60, 7), good_reg("price", 60, 7)],
        );
        forecast(&args).expect("two distinct names must be accepted");
    }

    /// CHECK 6a, zero spread over the history rows.
    ///
    /// An all-zero column has ONE unique value, so it is NOT auto-exempt, IS standardised,
    /// and comes out at `std = 0.0`. This is what stops `splice` dividing by zero.
    #[test]
    fn a_regressor_constant_over_the_history_is_refused() {
        for constant in [0.0, 1.0, 7.5] {
            let mut reg = good_reg("flat", 60, 7);
            reg.values = vec![constant; 67];
            refusal(&reg_args(60, 7, vec![reg]), "constant over the history");
        }
        // POSITIVE CONTROL, and the OTHER half of the auto rule: a {0,1} column has TWO
        // unique values, is auto-exempt, keeps std = 1.0, and is accepted.
        let mut reg = good_reg("binary", 60, 7);
        reg.values = (0..67).map(|i| f64::from(i as u32 % 2)).collect();
        forecast(&reg_args(60, 7, vec![reg]))
            .expect("a {0,1} indicator column is auto-exempt and must be accepted");
    }

    /// CHECK 6b, post-arithmetic finiteness. A SEPARATE check from input finiteness, and
    /// both are needed: these inputs are each finite, but their sum of squares overflows.
    #[test]
    fn a_regressor_whose_derived_statistics_overflow_is_refused() {
        let mut reg = good_reg("huge", 60, 7);
        reg.values = (0..67)
            .map(|i| if i % 2 == 0 { 1.0e300 } else { -1.0e300 })
            .collect();
        assert!(
            reg.values.iter().all(|v| v.is_finite()),
            "the probe's own inputs must be finite, or it is testing check 2 by accident"
        );
        refusal(&reg_args(60, 7, vec![reg]), "non-finite");
        // POSITIVE CONTROL at a large-but-safe magnitude, so the check cannot pass by
        // refusing every large column.
        let mut reg = good_reg("big", 60, 7);
        reg.values = (0..67)
            .map(|i| 1.0e100 + f64::from(i as u32) * 1.0e98)
            .collect();
        forecast(&reg_args(60, 7, vec![reg]))
            .expect("a large but non-overflowing column must be accepted");
    }

    /// CHECK 7, the MEASURED count ceiling — replaces plan 06.1-01's interim literal 50.
    ///
    /// The ceiling is read from `MAX_REGRESSORS` rather than written as a literal, so
    /// lowering the constant after a second measurement cannot leave this test pinning a
    /// number the door no longer enforces.
    #[test]
    fn regressors_beyond_the_measured_count_ceiling_are_refused() {
        let over: Vec<_> = (0..=crate::types::MAX_REGRESSORS)
            .map(|i| good_reg(&format!("r{i}"), 60, 7))
            .collect();
        // The probe must be INSIDE the product ceiling, or the refusal it triggers is the
        // product check (which runs just after) and this test would be green for the wrong
        // reason — a real risk, because both ceilings move together in Task 2 step 5.
        assert!(
            (60 + 7) * over.len() <= crate::types::MAX_REGRESSOR_DESIGN_COST,
            "the over-count probe must violate ONLY the count ceiling"
        );
        refusal(&reg_args(60, 7, over), "max_regressors");
        // POSITIVE CONTROL: exactly AT the ceiling is accepted. Re-derived against the
        // MEASURED constant, so a refusal that refused everything would show here.
        let at: Vec<_> = (0..crate::types::MAX_REGRESSORS)
            .map(|i| good_reg(&format!("r{i}"), 60, 7))
            .collect();
        assert_eq!(at.len(), crate::types::MAX_REGRESSORS);
        assert!(
            (60 + 7) * at.len() <= crate::types::MAX_REGRESSOR_DESIGN_COST,
            "the control must be inside the PRODUCT ceiling too, or it would be refused by \
             the other check and prove nothing about this one"
        );
        forecast(&reg_args(60, 7, at)).expect("exactly the count ceiling must be accepted");
    }

    /// The fixed history geometry the product-ceiling probes use.
    ///
    /// 2 000 points and a 365-step horizon: both individually legal by a wide margin, so
    /// the only thing a refusal here can be about is the PRODUCT.
    const PRODUCT_PROBE_POINTS: usize = 2_000;
    const PRODUCT_PROBE_HORIZON: usize = 365;

    /// The smallest regressor count whose product EXCEEDS the ceiling, and the largest that
    /// does not — both DERIVED from the constant.
    ///
    /// Derived rather than written down because the ceiling MOVES: plan 06.1-03 Task 1
    /// derives a provisional value and Task 2 step 5 re-derives it with the identifiability
    /// diagnostic live. A hardcoded pair silently stops straddling the boundary the moment
    /// the constant changes — which is exactly what happened when the Task 1 sweep lowered
    /// it from the starting candidate, and is why this is a function.
    fn product_probe_counts() -> (usize, usize) {
        let rows = PRODUCT_PROBE_POINTS + PRODUCT_PROBE_HORIZON;
        let under = crate::types::MAX_REGRESSOR_DESIGN_COST / rows;
        (under + 1, under)
    }

    /// CHECK 9, the design-cost PRODUCT, from both sides.
    ///
    /// The point of a product bound is that each factor is individually legal: this request
    /// is inside `fit_max_points`, inside `fit_max_horizon` and inside `fit_max_regressors`,
    /// and only their product is refused.
    #[test]
    fn a_regressor_design_cost_over_the_product_ceiling_is_refused() {
        let (n, horizon) = (PRODUCT_PROBE_POINTS, PRODUCT_PROBE_HORIZON);
        let (over_count, under_count) = product_probe_counts();
        let over: Vec<_> = (0..over_count)
            .map(|i| good_reg(&format!("r{i}"), n, horizon))
            .collect();
        assert!(
            n <= crate::types::MAX_POINTS
                && horizon <= crate::types::MAX_HORIZON
                && over.len() <= crate::types::MAX_REGRESSORS,
            "every factor must be individually legal, or this tests the wrong check"
        );
        assert!(
            (n + horizon) * over.len() > crate::types::MAX_REGRESSOR_DESIGN_COST,
            "the probe must actually be over the product ceiling"
        );
        let args = reg_args(n, horizon, over);
        refusal(&args, "max_regressor_design_cost");
        // The message STATES THE OPERANDS it used, the way the holiday design-cost message
        // does — a bound that reports only its own value leaves the caller guessing which
        // factor to shrink.
        match forecast(&args) {
            Err(ForecastError::Validation(m)) => {
                let cells = (n + horizon) * over_count;
                for needle in [
                    "(points + horizon) x n_regressors".to_string(),
                    format!("({n} + {horizon}) x {over_count}"),
                    cells.to_string(),
                ] {
                    assert!(
                        m.contains(&needle),
                        "message must contain {needle:?}, got {m:?}"
                    );
                }
            }
            other => panic!(
                "expected a Validation refusal, got {:?}",
                other.map(|r| r.model)
            ),
        }
        // POSITIVE CONTROL, at the LARGEST count the same geometry admits — one below the
        // refused one, so the two straddle the boundary rather than sitting near it.
        let under: Vec<_> = (0..under_count)
            .map(|i| good_reg(&format!("r{i}"), n, horizon))
            .collect();
        assert_eq!(
            under.len() + 1,
            over_count,
            "the pair must straddle the ceiling"
        );
        assert!((n + horizon) * under.len() <= crate::types::MAX_REGRESSOR_DESIGN_COST);
        forecast(&reg_args(n, horizon, under))
            .expect("the largest request under the product ceiling must be accepted");
    }

    /// CHECK 8, the name byte bound — and it must not echo the name back (T-06.1-14).
    ///
    /// Reuses `fit_max_holiday_name_len`: the amplification argument is the same one C-07
    /// records, so a fourth name ceiling would be a second number to keep in step.
    #[test]
    fn a_regressor_name_longer_than_the_byte_ceiling_is_refused_without_echoing_it() {
        let long = "z".repeat(crate::types::MAX_HOLIDAY_NAME_LEN + 1);
        let mut reg = good_reg(&long, 60, 7);
        reg.name.clone_from(&long);
        match forecast(&reg_args(60, 7, vec![reg])) {
            Err(ForecastError::Validation(m)) => {
                assert!(
                    m.contains("the name is not echoed back"),
                    "the message must say the name is withheld, got {m:?}"
                );
                assert!(
                    !m.contains(&long),
                    "the message must NOT echo the oversized name — echoing it \
                     re-materialises the very bytes the bound refuses"
                );
                assert!(
                    m.contains("index 0") && m.contains("201 bytes"),
                    "the message must name the INDEX and the LENGTH, got {m:?}"
                );
            }
            other => panic!(
                "expected a Validation refusal, got {:?}",
                other.map(|r| r.model)
            ),
        }
        // POSITIVE CONTROL: exactly AT the ceiling is accepted.
        let at = "z".repeat(crate::types::MAX_HOLIDAY_NAME_LEN);
        forecast(&reg_args(60, 7, vec![good_reg(&at, 60, 7)]))
            .expect("a name of exactly the ceiling must be accepted");
    }

    /// BYTES, not characters — the rewrite this test exists to catch.
    ///
    /// The holiday side already carries this case
    /// (`a_holiday_name_whose_char_count_fits_but_whose_byte_length_does_not_is_refused`);
    /// the regressor side needs its own, because a rewrite to `chars().count()` would be
    /// made in one place and would have to be caught in both.
    #[test]
    fn a_regressor_name_whose_char_count_fits_but_whose_byte_length_does_not_is_refused() {
        // 'é' is two UTF-8 bytes: 120 chars, 240 bytes.
        let name = "é".repeat(120);
        assert!(
            name.chars().count() <= crate::types::MAX_HOLIDAY_NAME_LEN
                && name.len() > crate::types::MAX_HOLIDAY_NAME_LEN,
            "the probe must fit on chars and NOT on bytes, or it tests nothing"
        );
        refusal(&reg_args(60, 7, vec![good_reg(&name, 60, 7)]), "bytes");
    }

    /// CHECK 10 part (a): a regressor named exactly like a GENERATED design column.
    #[test]
    fn a_regressor_colliding_with_a_generated_column_name_is_refused() {
        // 60 daily points gives a weekly seasonality, so `weekly_delim_1` is a real
        // generated column name for this request — derived, not assumed.
        let args = reg_args(60, 7, vec![good_reg("promo", 60, 7)]);
        let probe = forecast(&args).expect("the control request must be accepted");
        let generated = probe
            .components
            .keys()
            .find(|k| k.contains("_delim_"))
            .cloned();
        // The components map carries COMPONENT names, not column names, so derive the
        // column name from the spec the same way the door does.
        let ds: Vec<i64> = args
            .ds
            .iter()
            .map(|s| crate::dates::parse_date(s).expect("valid"))
            .collect();
        let spec = crate::prophet::Spec::default_linear(crate::prophet::auto_seasonalities(
            &ds,
            10.0,
            crate::prophet::Mode::Additive,
        ));
        let col = crate::prophet::columns(&spec)
            .into_iter()
            .map(|c| c.name)
            .find(|n| n.contains("_delim_"))
            .expect("this geometry must generate at least one _delim_ column");
        assert!(
            generated.is_none(),
            "a _delim_ name is a COLUMN name, not a component key; if one appeared as a \
             component this test's premise changed"
        );
        match forecast(&reg_args(60, 7, vec![good_reg(&col, 60, 7)])) {
            Err(ForecastError::Validation(m)) => assert!(
                m.contains("a generated design column name"),
                "the message must say WHICH part of the reserved set matched, got {m:?}"
            ),
            other => panic!(
                "expected a Validation refusal, got {:?}",
                other.map(|r| r.model)
            ),
        }
    }

    /// CHECK 10 part (b): a regressor named after a SEASONALITY component.
    ///
    /// `weekly` is not itself a generated column name — the columns are `weekly_delim_1`,
    /// `weekly_delim_2`, … — so part (a) alone would let this through, and `predict` would
    /// push a `weekly` component that the regressor's own entry then overwrote.
    #[test]
    fn a_regressor_colliding_with_a_seasonality_component_name_is_refused() {
        match forecast(&reg_args(60, 7, vec![good_reg("weekly", 60, 7)])) {
            Err(ForecastError::Validation(m)) => assert!(
                m.contains("a response component name"),
                "the message must say WHICH part matched, got {m:?}"
            ),
            other => panic!(
                "expected a Validation refusal, got {:?}",
                other.map(|r| r.model)
            ),
        }
    }

    /// CHECK 10 part (b), the HOLIDAY half — and its accepted control, which is what makes
    /// the check a collision test rather than a blocklist.
    #[test]
    fn a_regressor_colliding_with_a_declared_holiday_name_is_refused_but_not_otherwise() {
        let mut args = reg_args(60, 7, vec![good_reg("blackfriday", 60, 7)]);
        // WITHOUT the holiday declared, `blackfriday` is an ordinary name and is ACCEPTED.
        forecast(&args)
            .expect("`blackfriday` collides with nothing when no such holiday is declared");
        // WITH it declared, the same name is refused.
        args.holidays = Some(vec![crate::types::HolidayArg {
            name: "blackfriday".into(),
            dates: vec!["2020-01-10".into()],
            lower_window: 0,
            upper_window: 0,
        }]);
        match forecast(&args) {
            Err(ForecastError::Validation(m)) => assert!(
                m.contains("a response component name") && m.contains("blackfriday"),
                "the message must name the collision and its part, got {m:?}"
            ),
            other => panic!(
                "expected a Validation refusal, got {:?}",
                other.map(|r| r.model)
            ),
        }
    }

    /// CHECK 10 part (c): a RESERVED RESPONSE KEY, which is invisible to parts (a) and (b).
    ///
    /// This is the case the review found: `components.insert(name, value)` is a MAP INSERT,
    /// so a regressor named `additive_terms` is computed as its own component and then
    /// silently OVERWRITTEN by the aggregate `predict` pushes afterwards.
    #[test]
    fn a_regressor_colliding_with_a_reserved_response_key_is_refused() {
        for key in [
            "additive_terms",
            "extra_regressors_additive",
            "yhat",
            "trend",
        ] {
            match forecast(&reg_args(60, 7, vec![good_reg(key, 60, 7)])) {
                Err(ForecastError::Validation(m)) => assert!(
                    m.contains("a reserved response key") && m.contains(key),
                    "{key}: the message must name the collision and its part, got {m:?}"
                ),
                other => panic!(
                    "{key}: expected a Validation refusal, got {:?}",
                    other.map(|r| r.model)
                ),
            }
        }
        // POSITIVE CONTROL across all three parts: an ordinary name clears every one.
        forecast(&reg_args(60, 7, vec![good_reg("promo", 60, 7)]))
            .expect("`promo` must clear all three parts of the reserved set");
    }

    /// The part-(c) list cannot be silently emptied into vacuity.
    ///
    /// Parts (a) and (b) are DERIVED from the spec, so they cannot rot. Part (c) is a
    /// hand-written slice, which is exactly the shape that goes stale — so it is pinned
    /// here, by name, in one place.
    #[test]
    fn the_reserved_response_keys_slice_is_not_empty() {
        assert_eq!(
            super::RESERVED_RESPONSE_KEYS.len(),
            11,
            "the reserved response key list must carry all eleven names"
        );
        for want in [
            "additive_terms",
            "multiplicative_terms",
            "holidays",
            "extra_regressors_additive",
            "extra_regressors_multiplicative",
            "trend",
            "yhat",
            "yhat_lower",
            "yhat_upper",
            "ds",
            "cap",
        ] {
            assert!(
                super::RESERVED_RESPONSE_KEYS.contains(&want),
                "{want} must be in the reserved response key list"
            );
        }
    }

    /// THE REFUSAL COVERAGE, pinned by MESSAGES EXERCISED rather than by test-function names.
    ///
    /// A floor on the number of test FUNCTIONS is coupled to how an executor chose to group
    /// them: six checks can legitimately be six functions or one table. This test drives one
    /// probe per refusal the regressor surface can produce, asserts each one actually fires
    /// with the expected needle, and asserts its OWN row count — so the coverage number is a
    /// property of the refusals rather than of the file's layout.
    #[test]
    fn the_regressor_refusal_table_is_complete() {
        type Probe = fn() -> ForecastArgs;
        let long_name = "z".repeat(crate::types::MAX_HOLIDAY_NAME_LEN + 1);
        let table: Vec<(&str, &str, Box<dyn Fn() -> ForecastArgs>)> = vec![
            (
                "name byte length",
                "max_holiday_name_len",
                Box::new(move || reg_args(60, 7, vec![good_reg(&long_name, 60, 7)])),
            ),
            (
                "count ceiling",
                "max_regressors",
                Box::new(|| {
                    reg_args(
                        60,
                        7,
                        (0..=crate::types::MAX_REGRESSORS)
                            .map(|i| good_reg(&format!("r{i}"), 60, 7))
                            .collect(),
                    )
                }),
            ),
            (
                "design cost product",
                "max_regressor_design_cost",
                Box::new(|| {
                    let (over_count, _) = product_probe_counts();
                    reg_args(
                        PRODUCT_PROBE_POINTS,
                        PRODUCT_PROBE_HORIZON,
                        (0..over_count)
                            .map(|i| {
                                good_reg(
                                    &format!("r{i}"),
                                    PRODUCT_PROBE_POINTS,
                                    PRODUCT_PROBE_HORIZON,
                                )
                            })
                            .collect(),
                    )
                }),
            ),
            (
                "collision: generated column name",
                "a generated design column name",
                Box::new(|| reg_args(60, 7, vec![good_reg("weekly_delim_1", 60, 7)])),
            ),
            (
                "collision: component name",
                "a response component name",
                Box::new(|| reg_args(60, 7, vec![good_reg("weekly", 60, 7)])),
            ),
            (
                "collision: reserved response key",
                "a reserved response key",
                Box::new(|| reg_args(60, 7, vec![good_reg("additive_terms", 60, 7)])),
            ),
            (
                "duplicate names",
                "appears more than once",
                Box::new(|| {
                    reg_args(
                        60,
                        7,
                        vec![good_reg("promo", 60, 7), good_reg("promo", 60, 7)],
                    )
                }),
            ),
            (
                "length tie",
                "values but 67 are required",
                Box::new(|| {
                    let mut r = good_reg("promo", 60, 7);
                    r.values.truncate(10);
                    reg_args(60, 7, vec![r])
                }),
            ),
            (
                "input finiteness",
                "non-finite value at row",
                Box::new(|| {
                    let mut r = good_reg("promo", 60, 7);
                    r.values[3] = f64::INFINITY;
                    reg_args(60, 7, vec![r])
                }),
            ),
            (
                "mode allowlist",
                "is not supported",
                Box::new(|| {
                    let mut r = good_reg("promo", 60, 7);
                    r.mode = Some("exponential".into());
                    reg_args(60, 7, vec![r])
                }),
            ),
            (
                "prior scale range",
                "outside the",
                Box::new(|| {
                    let mut r = good_reg("promo", 60, 7);
                    r.prior_scale = Some(0.0);
                    reg_args(60, 7, vec![r])
                }),
            ),
            (
                "zero spread",
                "carries no information",
                Box::new(|| {
                    let mut r = good_reg("flat", 60, 7);
                    r.values = vec![2.0; 67];
                    reg_args(60, 7, vec![r])
                }),
            ),
            (
                "post-arithmetic mean overflow",
                "non-finite mean after standardisation",
                Box::new(|| {
                    let mut r = good_reg("huge", 60, 7);
                    r.values = (0..67).map(|i| 1.0e307 + f64::from(i as u32)).collect();
                    reg_args(60, 7, vec![r])
                }),
            ),
            (
                "neuralprophet arm",
                "set model to \"prophet\"",
                Box::new(|| {
                    let mut a = reg_args(60, 7, vec![good_reg("promo", 60, 7)]);
                    a.model = Some("neuralprophet".into());
                    a
                }),
            ),
        ];
        // The floor is asserted on the TABLE, so the coverage claim survives any regrouping
        // of the test functions around it.
        assert!(
            table.len() >= 14,
            "the regressor refusal table must cover at least 14 distinct refusals, has {}",
            table.len()
        );
        let _: Option<Probe> = None;
        for (label, needle, build) in &table {
            match forecast(&build()) {
                Err(ForecastError::Validation(m)) => assert!(
                    m.contains(needle),
                    "{label}: the refusal must name {needle:?}, got {m:?}"
                ),
                other => panic!(
                    "{label}: this probe must be REFUSED — a row that no longer refuses is a \
                     silently removed check, got {:?}",
                    other.map(|r| r.model)
                ),
            }
        }
    }

    /// D-21: the neuralprophet arm REFUSES regressors rather than silently dropping them.
    #[test]
    fn regressors_on_the_neuralprophet_arm_are_refused_not_dropped() {
        let mut args = reg_args(60, 7, vec![good_reg("promo", 60, 7)]);
        args.model = Some("neuralprophet".into());
        refusal(&args, "set model to \"prophet\"");
    }

    /// Every new refusal message separates the LIMITATION from the FIX with a semicolon,
    /// matching the existing door message shape, and none echoes a caller-supplied VALUE.
    #[test]
    fn a_regressor_refusal_names_the_limitation_and_the_fix() {
        let mut short = good_reg("promo", 60, 7);
        short.values = vec![1.0; 3];
        let mut bad_mode = good_reg("promo", 60, 7);
        bad_mode.mode = Some("exponential".into());
        let mut flat = good_reg("flat", 60, 7);
        flat.values = vec![2.0; 67];
        for (label, reg) in [("length", short), ("mode", bad_mode), ("constant", flat)] {
            match forecast(&reg_args(60, 7, vec![reg])) {
                Err(ForecastError::Validation(m)) => assert!(
                    m.contains(';'),
                    "{label}: the message must separate the limitation from the fix with a \
                     semicolon, got {m:?}"
                ),
                other => panic!("{label} must be refused, got {:?}", other.map(|r| r.model)),
            }
        }
    }

    /// The measured LIMIT of the prior-scale floor, recorded as a test so it cannot quietly
    /// become folklore.
    ///
    /// `REGRESSOR_PRIOR_SCALE_MIN` is a REPRESENTABILITY bound: it keeps `sc * sc` normal
    /// and the objective finite. It is NOT a usability bound. At the floor the fit is
    /// degenerate — L-BFGS performs zero iterations and the regressor contributes exactly
    /// nothing — while the door returns an ordinary-looking forecast.
    ///
    /// This test asserts BOTH halves, so the day either changes it goes red:
    ///   - at the floor, the regressor contribution is exactly zero (the degenerate case
    ///     the door currently ACCEPTS);
    ///   - at a normal prior scale, it is not (so the assertion above is about the floor
    ///     and not about the whole mechanism being broken).
    ///
    /// The contract records the sweep this came from: every `prior_scale <= 1e-9` measured
    /// 0-1 iterations with a zero contribution; `>= 1e-7` fits normally. Closing that gap
    /// needs a measurement campaign across series shapes and belongs to a later plan.
    #[test]
    fn the_prior_scale_floor_is_a_representability_bound_not_a_usability_bound() {
        let contribution = |ps: f64| -> f64 {
            let mut reg = good_reg("promo", 60, 7);
            reg.prior_scale = Some(ps);
            let r = forecast(&reg_args(60, 7, vec![reg]))
                .unwrap_or_else(|e| panic!("prior_scale {ps:e} must be accepted: {e}"));
            r.components["extra_regressors_additive"]
                .as_array()
                .expect("extra_regressors_additive is an array")
                .iter()
                .map(|v| v.as_f64().unwrap_or(f64::NAN).abs())
                .fold(0.0_f64, f64::max)
        };
        assert_eq!(
            contribution(REGRESSOR_PRIOR_SCALE_MIN),
            0.0,
            "at the representability floor the regressor contributes exactly nothing — if \
             this ever becomes non-zero the floor has become a usability bound and the \
             contract comment must be re-measured"
        );
        assert!(
            contribution(10.0) > 0.0,
            "at a normal prior scale the regressor MUST contribute, or the assertion above \
             is passing because the whole mechanism is dead rather than because the floor \
             is degenerate"
        );
    }
}

// ------------------------------------------- the WR-03 wall harness (ignored) ----

/// Wall-clock harness for **WR-03**: the work the in-loop aggregate-dates refusal avoids.
///
/// `#[ignore]`d, because it is a MEASUREMENT and not an assertion about correctness. Run it
/// deliberately, on a RELEASE build:
///
/// ```text
/// cargo test --release -p aprender-forecast --lib wr03_aggregate_dates_wall \
///     -- --ignored --nocapture
/// ```
///
/// It drives the review's exact trigger — 1 000 holidays, each with `lower_window: 0,
/// upper_window: 0` (so `holiday_columns` reaches only 1 000 and never trips) and each
/// carrying 1 000 dates. To reproduce the BEFORE side, delete the in-loop refusal in
/// `forecast` and re-run: the same payload then parses ~1 000 000 dates and allocates
/// ~8 MB of `Vec<i64>` before the post-loop check discards all of it.
#[cfg(test)]
mod wr03_wall {
    /// One machine-parsable line. `profile=` is derived, never asserted (CLAUDE.md rule 2).
    #[test]
    #[ignore = "wall-clock measurement; run with --release -- --ignored --nocapture"]
    fn wr03_aggregate_dates_wall() {
        // A short valid series; the refusal fires at the door, so the history is only
        // required to be legal in the dimension this payload is not perturbing.
        let t0d = crate::dates::days_from_civil(2020, 1, 1);
        let ds: Vec<String> = (0..60i64)
            .map(|i| crate::dates::format_ymd(t0d + i))
            .collect();
        let y: Vec<f64> = (0..60)
            .map(|i| 10.0 + 0.05 * f64::from(i) + f64::from(i % 7))
            .collect();
        let base = crate::dates::days_from_civil(1990, 1, 1);
        let holidays: Vec<crate::types::HolidayArg> = (0..1_000i64)
            .map(|h| crate::types::HolidayArg {
                name: format!("h{h}"),
                dates: (0..1_000i64)
                    .map(|d| crate::dates::format_ymd(base + h * 1_000 + d))
                    .collect(),
                lower_window: 0,
                upper_window: 0,
            })
            .collect();
        let dates_sent: usize = holidays.iter().map(|h| h.dates.len()).sum();
        let args = crate::types::ForecastArgs {
            ds,
            y,
            horizon: 7,
            holidays: Some(holidays),
            ..crate::types::ForecastArgs::default()
        };
        let t0 = std::time::Instant::now();
        let outcome = match super::forecast(&args) {
            Err(crate::types::ForecastError::Validation(_)) => "refused",
            Ok(_) => "accepted",
            Err(e) => panic!("unexpected {e:?}"),
        };
        println!(
            "WR03 AGGREGATE DATES WALL: holidays=1000 dates_per_holiday=1000 \
             dates_sent={dates_sent} outcome={outcome} total_s={:.6} profile={}",
            t0.elapsed().as_secs_f64(),
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
        );
    }
}
