# Spike Manifest

## Ideas

### prophet-forecast-mcp
Implement Facebook Prophet (piecewise-linear/logistic trend with Laplace-prior changepoints,
Fourier seasonality, holidays, MAP fit via L-BFGS, simulated-changepoint uncertainty) and
NeuralProphet (the same decomposition plus AR-Net and lagged/future regressors, trained by
gradient descent) natively in aprender, and expose them the way SetFit is exposed: a thin,
single-purpose MCP server (pmcp, Lambda/pmcp.run) for time-series forecasting.

**Requirements:**
- MCP serving shape is a STATELESS `forecast` tool: one call carries `ds[]`, `y[]`, horizon
  (and freq); the server fits and forecasts inside that call. No fit → artifact → forecast
  round-trip (decided 2026-09-04 at spike alignment).
- Both Prophet (MAP / L-BFGS) and NeuralProphet (autograd / AdamW) are in scope; NeuralProphet
  is spiked in this session, not deferred.
- Correctness bar for the Prophet port is parity with Python Prophet 1.4.0 on the Peyton Manning
  dataset (fixture: `001-prophet-map-fit-lbfgs/fixtures/peyton_manning_prophet140.json`), not
  self-consistency.

## Spikes

| # | Idea | Name | Type | Validates | Verdict | Tags |
|---|------|------|------|-----------|---------|------|
| 001 | prophet-forecast-mcp | prophet-map-fit-lbfgs | standard | Given Peyton Manning, when Prophet's Stan objective + analytic gradient is minimized with `LbfgsF64`, then it converges and yhat matches Python Prophet 1.4.0 within tolerance | VALIDATED ✓ (inside Prophet's own Newton-vs-LBFGS band on 3 datasets; needs f/T scaling + non-finite guard) | prophet, lbfgs, changepoints, fourier |
| 002 | prophet-forecast-mcp | neuralprophet-autograd | standard | Given the same series, when trend + Fourier + AR-Net is trained with AdamW + SmoothL1 on the f32 autograd, then holdout MAE is comparable to 001 using only existing ops | VALIDATED ✓ (test MAE 0.451 vs NP 0.461, 500× faster; core SmoothL1Loss is detached) | neuralprophet, autograd, ar-net |
| 003 | prophet-forecast-mcp | prophet-intervals-and-components | standard | Given the 001 fit, when future changepoints are simulated and components decomposed, then the 80% band covers ~80% of a holdout and components sum to yhat; logistic, multiplicative and holidays fit | VALIDATED ✓ (bands within ~1% of Python, holdout coverage 0.81–0.83 both; L-BFGS needs restarts) | prophet, uncertainty, holidays |
| 004 | prophet-forecast-mcp | forecast-mcp-thin-server | standard | Given a pmcp thin server with one stateless `forecast` tool, when called with ds/y/horizon, then fit + forecast returns in under 2s for 3k points and a browser page charts it | VALIDATED ✓ (Peyton 1.41 s round trip; 3k pts 0.21 s; iteration cap + budget for 20k; page is an MCP client) | mcp, pmcp, latency, ui |
