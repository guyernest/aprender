//! Pure-Rust time-series forecasting: the Prophet 1.4.0 port (MAP fit via core's L-BFGS)
//! behind one stateless [`forecast`] entry point.
//!
//! # Where this sits (D-07)
//!
//! CLAUDE.md's Realizar-first rule says all *inference* goes through `realizar`. Phase 6
//! adds a documented exception class for forecasting, for the same reason SetFit is one:
//! the only conformance-proven implementation of these numerics is here, measured against
//! Python Prophet 1.4.0 on committed oracle fixtures. A fit is training-side work, and a
//! Prophet forecast is a fit — there is no artifact to serve. `aprender-mcp-forecast` owns
//! the transport (tool schema, routes, readiness) and calls [`forecast`]; it reimplements
//! nothing (OPS-03).
//!
//! # Shape (D-01)
//!
//! One call carries the series and the horizon; the fit runs inside the call. There is no
//! fit -> artifact -> forecast round-trip.
//!
//! ```no_run
//! use aprender_forecast::{forecast, ForecastArgs};
//! let args = ForecastArgs {
//!     ds: vec!["2020-01-01".into()], y: vec![1.0], horizon: 7,
//!     freq: None, model: None, growth: None, cap: None, seasonality_mode: None,
//!     interval_width: None, holidays: None, n_lags: None, seed: None,
//! };
//! assert!(forecast(&args).is_err()); // refused: fewer than MIN_POINTS points
//! ```
//!
//! # Modules
//!
//! [`dates`] is civil-date arithmetic (no calendar library, D-17); [`types`] is the tool
//! boundary; [`fit`] is the D-09 L-BFGS recipe; [`prophet`] is the port itself;
//! [`forecast`](forecast()) is the door; [`np`] is the NeuralProphet-lite port (D-10).
//!
//! The Chronos-Bolt zero-shot stack is [`bolt`] (the T5 forward, D-14 GEMM routing),
//! [`safetensors`] (the F32/F16/BF16 byte-slice decoder) and [`chronos`] (its own door:
//! [`chronos::validate`] then [`chronos::forecast`]). It is a SECOND door, not a `model:` arm of
//! the first: Chronos is zero-shot with no fit, nullable `y`, its own bounds (D-11: 4 points,
//! horizon 1024) and its own server (D-03, one model per server).

pub mod dates;
pub mod types;

pub mod bolt;
pub mod chronos;
pub mod fit;
pub mod forecast;
pub mod np;
pub mod prophet;
pub mod safetensors;

#[cfg(test)]
pub(crate) mod test_support;

pub use forecast::forecast;
pub use types::{
    ForecastArgs, ForecastError, ForecastResponse, HolidayArg, MAX_HORIZON, MAX_POINTS, MIN_POINTS,
};
