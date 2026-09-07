//! The tool boundary's argument and response types, and the DoS bounds that make
//! [`crate::forecast::forecast`] safe to expose to untrusted JSON-RPC arguments (D-11).
//!
//! Ported verbatim from `sources/004-forecast-mcp-thin-server/src/lib.rs:26-103` (D-08).
//! The doc comments on [`ForecastArgs`] are load-bearing: `schemars` lifts them into the
//! advertised MCP tool schema.

// schemars' JsonSchema derive expands to .unwrap() internally, and the derive's
// generated impl lands at file scope where a struct-level allow cannot reach it.
// Same precedent as aprender-mcp-setfit/src/lib.rs:29-32.
#![allow(clippy::disallowed_methods)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Hard upper bound on history length (T-06-02: this is what actually bounds the work).
pub const MAX_POINTS: usize = 20_000;
/// Hard upper bound on the number of future periods.
pub const MAX_HORIZON: usize = 3_650;
/// Fewer points than this and there is nothing to fit.
pub const MIN_POINTS: usize = 10;

/// Hard upper bound on the DAILY SPAN (`last - first + 1`) a fit may cover.
///
/// `MAX_POINTS` bounds how many points arrive; it does NOT bound how far apart they are.
/// `np::NpData::new` materialises an imputed DAILY GRID over `first..=last`, so ten points
/// a millennium apart bought a 3.6-million-row grid and hundreds of MB of feature matrices
/// from a ~200-byte request. Equal to `MAX_POINTS` so the grid can never exceed the rows a
/// fully dense series at the point ceiling would produce (D-11).
pub const MAX_SPAN_DAYS: i64 = MAX_POINTS as i64;

/// Hard upper bound on `|lower_window|` and `upper_window` for a single holiday.
///
/// `prophet::columns` emits one design column per offset in `lower_window..=upper_window`,
/// so the two `i64` fields were an unbounded column multiplier: only their SIGN was
/// checked. A year of window either side is past anything Prophet's own users write.
pub const MAX_HOLIDAY_WINDOW: i64 = 365;

/// Hard upper bound on the total number of holiday design columns across all holidays.
///
/// Bounds `sum(window_width)` the way [`MAX_HOLIDAY_WINDOW`] bounds one term of it, so a
/// long list of individually legal holidays cannot multiply back into the same blow-up.
pub const MAX_HOLIDAY_COLUMNS: usize = 1_000;

/// Hard upper bound on the number of dates one holiday may carry.
///
/// `prophet::feature_row` scans this list per row per holiday column, so it is a second
/// multiplier on the design build.
pub const MAX_HOLIDAY_DATES: usize = 1_000;

/// Default router-pool size for the streamable-HTTP server (`constants.pool_default`).
///
/// Lives here, beside the other contract-mirrored bounds, because this is the crate that
/// owns the memoized contract reader — so the value is ASSERTED equal to the YAML by
/// `cost_bounds_match_contract` below. It previously sat in `aprender-mcp-forecast` as two
/// separate literal `8`s whose only "mirrors the contract" evidence was a doc comment and
/// a test that compared the constant to ITSELF.
pub const DEFAULT_POOL: usize = 8;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HolidayArg {
    /// Holiday name (becomes a component).
    pub name: String,
    /// Dates the holiday occurs on, YYYY-MM-DD (past AND future occurrences).
    pub dates: Vec<String>,
    /// Days before the date to include (≤ 0). Default 0.
    #[serde(default)]
    pub lower_window: i64,
    /// Days after the date to include (≥ 0). Default 0.
    #[serde(default)]
    pub upper_window: i64,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ForecastArgs {
    /// Timestamps, YYYY-MM-DD (a time part is ignored), ascending, unique.
    pub ds: Vec<String>,
    /// Observed values, same length as `ds`.
    pub y: Vec<f64>,
    /// Number of future periods to forecast (1 … 3650).
    pub horizon: usize,
    /// Period of the future steps: "D" (default), "W", or "MS" (month start).
    #[serde(default)]
    pub freq: Option<String>,
    /// "prophet" (default) or "neuralprophet".
    #[serde(default)]
    pub model: Option<String>,
    /// Prophet growth: "linear" (default), "logistic" (needs `cap`) or "flat".
    #[serde(default)]
    pub growth: Option<String>,
    /// Carrying capacity for logistic growth (original units).
    #[serde(default)]
    pub cap: Option<f64>,
    /// "additive" (default) or "multiplicative" seasonality.
    #[serde(default)]
    pub seasonality_mode: Option<String>,
    /// Width of the uncertainty band, default 0.8.
    #[serde(default)]
    pub interval_width: Option<f64>,
    /// Holidays / events with optional windows (Prophet only).
    #[serde(default)]
    pub holidays: Option<Vec<HolidayArg>>,
    /// NeuralProphet only: number of autoregressive lags (0 = trend + seasonality only).
    #[serde(default)]
    pub n_lags: Option<usize>,
    /// Random seed for the uncertainty simulation / training (default 42).
    #[serde(default)]
    pub seed: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ForecastResponse {
    pub model: String,
    pub freq: String,
    pub n_history: usize,
    pub fit_seconds: f64,
    pub predict_seconds: f64,
    pub ds: Vec<String>,
    pub yhat: Vec<f64>,
    pub yhat_lower: Vec<f64>,
    pub yhat_upper: Vec<f64>,
    pub trend: Vec<f64>,
    pub components: serde_json::Map<String, serde_json::Value>,
    pub diagnostics: serde_json::Value,
}

/// Every refusal the door can produce. `Validation` is the caller's fault and maps to
/// `pmcp::Error::validation`; `Internal` maps to `pmcp::Error::internal`.
#[derive(Debug)]
pub enum ForecastError {
    Validation(String),
    Internal(String),
}

impl std::fmt::Display for ForecastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(s) | Self::Internal(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for ForecastError {}

#[cfg(test)]
mod tests {
    use super::{
        ForecastArgs, ForecastError, DEFAULT_POOL, MAX_HOLIDAY_COLUMNS, MAX_HOLIDAY_DATES,
        MAX_HOLIDAY_WINDOW, MAX_HORIZON, MAX_POINTS, MAX_SPAN_DAYS, MIN_POINTS,
    };
    use crate::test_support::constant_u64;

    /// The fit server's three bounds are EQUAL to the contract, not merely similar.
    ///
    /// D-15: a bound written twice can be loosened in one place. The contract is the
    /// source and this test is what makes the Rust constant a mirror of it —
    /// `constants.fit_max_horizon: 3651` in the YAML alone turns this red.
    #[test]
    fn bounds_match_contract() {
        assert_eq!(
            MIN_POINTS as u64,
            constant_u64("forecast-tool-boundary-v1", "fit_min_points"),
            "types::MIN_POINTS must equal constants.fit_min_points in forecast-tool-boundary-v1"
        );
        assert_eq!(
            MAX_POINTS as u64,
            constant_u64("forecast-tool-boundary-v1", "fit_max_points"),
            "types::MAX_POINTS must equal constants.fit_max_points in forecast-tool-boundary-v1"
        );
        assert_eq!(
            MAX_HORIZON as u64,
            constant_u64("forecast-tool-boundary-v1", "fit_max_horizon"),
            "types::MAX_HORIZON must equal constants.fit_max_horizon in forecast-tool-boundary-v1"
        );
    }

    /// The four cost bounds against the SAME contract, for the SAME reason.
    ///
    /// `fit_max_points` bounds how many points arrive; these bound the work each one can
    /// buy. They are the bounds a request can be inside all three headline limits and
    /// still blow past — a span-based daily grid, and a holiday window that is a design
    /// column multiplier — so they are contract-owned exactly like the headline three.
    #[test]
    fn cost_bounds_match_contract() {
        for (name, key, value) in [
            ("MAX_SPAN_DAYS", "fit_max_span_days", MAX_SPAN_DAYS as u64),
            (
                "MAX_HOLIDAY_WINDOW",
                "fit_max_holiday_window",
                MAX_HOLIDAY_WINDOW as u64,
            ),
            (
                "MAX_HOLIDAY_COLUMNS",
                "fit_max_holiday_columns",
                MAX_HOLIDAY_COLUMNS as u64,
            ),
            (
                "MAX_HOLIDAY_DATES",
                "fit_max_holiday_dates",
                MAX_HOLIDAY_DATES as u64,
            ),
            ("DEFAULT_POOL", "pool_default", DEFAULT_POOL as u64),
        ] {
            assert_eq!(
                value,
                constant_u64("forecast-tool-boundary-v1", key),
                "types::{name} must equal constants.{key} in forecast-tool-boundary-v1"
            );
        }
    }

    /// The Chronos door's three bounds against the SAME contract.
    ///
    /// Unconditional: plan 06-05 is a declared dependency of 06-06, so `crate::chronos`
    /// exists. `chronos_native_horizon` (64) is a property of the WEIGHTS, not of this
    /// crate, and is asserted by 06-07 against the shipped config fixture.
    #[test]
    fn chronos_bounds_match_contract() {
        assert_eq!(
            crate::chronos::CHRONOS_MIN_POINTS as u64,
            constant_u64("forecast-tool-boundary-v1", "chronos_min_points"),
            "chronos::CHRONOS_MIN_POINTS must equal constants.chronos_min_points in forecast-tool-boundary-v1"
        );
        assert_eq!(
            crate::chronos::CHRONOS_MAX_POINTS as u64,
            constant_u64("forecast-tool-boundary-v1", "chronos_max_points"),
            "chronos::CHRONOS_MAX_POINTS must equal constants.chronos_max_points in forecast-tool-boundary-v1"
        );
        assert_eq!(
            crate::chronos::CHRONOS_MAX_HORIZON as u64,
            constant_u64("forecast-tool-boundary-v1", "chronos_max_horizon"),
            "chronos::CHRONOS_MAX_HORIZON must equal constants.chronos_max_horizon in forecast-tool-boundary-v1"
        );
    }

    #[test]
    fn an_unknown_argument_key_is_refused_not_ignored() {
        let err = serde_json::from_value::<ForecastArgs>(serde_json::json!({
            "ds": ["2020-01-01"], "y": [1.0], "horizon": 1, "temperature": 0.7
        }))
        .expect_err("deny_unknown_fields must refuse `temperature`");
        assert!(err.to_string().contains("temperature"), "{err}");
    }

    #[test]
    fn the_advertised_schema_is_strict_and_names_the_three_required_fields() {
        let schema =
            serde_json::to_value(schemars::schema_for!(ForecastArgs)).expect("schema serializes");
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
        let required = schema["required"].as_array().expect("required array");
        for field in ["ds", "y", "horizon"] {
            assert!(
                required.contains(&serde_json::json!(field)),
                "{field} must be required: {schema}"
            );
        }
        assert!(
            !required.contains(&serde_json::json!("freq")),
            "freq is optional on every surface"
        );
    }

    #[test]
    fn the_two_error_variants_display_their_message() {
        assert_eq!(ForecastError::Validation("nope".into()).to_string(), "nope");
        assert_eq!(ForecastError::Internal("boom".into()).to_string(), "boom");
    }
}
