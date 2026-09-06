# aprender-forecast

Pure-Rust time-series forecasting for [aprender](https://github.com/paiml/aprender):
a port of Facebook **Prophet 1.4.0** (piecewise-linear / logistic / flat trend with
Laplace-prior changepoints, Fourier seasonality, holiday windows, additive and
multiplicative modes, MAP fit by L-BFGS, simulated-changepoint uncertainty) behind one
stateless entry point. NeuralProphet-lite (`np`) lands in plan 06-04 and the Chronos
zero-shot forward in 06-05.

```rust
use aprender_forecast::{forecast, ForecastArgs};

let response = forecast(&ForecastArgs {
    ds: ds, y: y, horizon: 365,
    freq: None, model: None, growth: None, cap: None, seasonality_mode: None,
    interval_width: None, holidays: None, n_lags: None, seed: None,
})?;
```

One call carries the series and the horizon; the fit runs inside the call. There is no
fit → artifact → forecast round-trip, and no model to store. `aprender-mcp-forecast`
wraps this crate as a thin MCP server.

## Read the bands as measured, not as nominal

**The nominal 80 % interval covered 0.60 (Prophet) and 0.66 (NeuralProphet-lite) of
held-out points across 17 rolling-origin windows** (spike 006). Report and consume the
*empirical* coverage: `yhat_lower`/`yhat_upper` are a roughly 60–66 % band, not an 80 %
one. This is a property of Prophet's uncertainty model on real series, not a defect in
the port — Python Prophet behaves the same way — but a number labelled 80 % that covers
0.60 is a number that will mislead someone.

## Why forecasting lives here and not in `realizar`

CLAUDE.md's Realizar-first rule sends all *inference* through `realizar`. Forecasting is
a documented exception class, for the same reason SetFit is one: a Prophet forecast **is
a fit**, there is no trained artifact to serve, and the only conformance-proven
implementation of these numerics is this crate's, measured against Python Prophet 1.4.0
on committed oracle fixtures. `aprender-mcp-forecast` owns the transport — tool schema,
routes, readiness — and calls `forecast()`; it re-implements nothing (OPS-03).

## Correctness bar

Not self-consistency: **parity with Python Prophet 1.4.0**. The ladder's load-bearing
rung, `prophet::parity::peyton_objective_at_python_map_within_1e9`, evaluates the Rust
objective at Python's MAP on the Peyton Manning series and compares it to Python's own
unnormalised log posterior. The oracle fixtures, their provenance, their generating
environments and the commands that regenerate them are documented in
[`tests/fixtures/README.md`](tests/fixtures/README.md). They are committed; their absence
is a defect, and no test in this crate skips because one is missing.

## Bounds

The library — not the transport — is the door, so every caller gets the same refusals:
at least 10 and at most 20 000 points, a horizon of 1…3 650, strictly ascending unique
`YYYY-MM-DD` dates, finite non-constant `y`, `freq` in `D`/`W`/`MS`, and
`interval_width` strictly inside (0, 1). The L-BFGS fit is capped at 2 000 iterations per
round; `FIT_BUDGET_SECS` (15 s) is a **cooperative** round-boundary budget, not a hard
wall-clock cap.

Dates are `i64` days since the epoch via civil-date arithmetic — no calendar-library
dependency anywhere in the Phase 6 crates.
