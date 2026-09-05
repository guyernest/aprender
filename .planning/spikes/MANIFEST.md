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
- Chronos (zero-shot foundation model) joins the idea as a THIRD forecaster with its own thin server (one model per server); Chronos-Bolt first, Chronos-2 later (decided 2026-09-04).
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
| 005 | prophet-forecast-mcp | chronos-bolt-tiny-parity | standard | Given `amazon/chronos-bolt-tiny` safetensors, when its T5 encoder-decoder (patch embedding, instance scaling, REG token, relative position bias, quantile head) is run in Rust, then the 9 quantiles match the Python `chronos-forecasting` pipeline on Peyton and air passengers within a committed tolerance, including the autoregressive rollout past 64 steps | VALIDATED ✓ (quantiles to 1e-6, rollout to 1.5e-5, 6 edge probes; 36 ms/forward plain Rust vs 5.9 ms torch; trueno GEMMs 4× slower than loops) | chronos, t5, zero-shot, safetensors, parity |
| 006 | prophet-forecast-mcp | chronos-vs-prophet-holdout | standard | Given the spike-001/003 series with rolling-origin holdouts (Peyton daily; air, retail monthly; wp_log_R daily), when zero-shot Chronos-Bolt (tiny and small) and the fitted Prophet / NeuralProphet-lite ports forecast the same windows, then MAE/MASE, 80% coverage/width and latency are compared per series with naive and seasonal-naive baselines, so the value of a Chronos server versus the fitted models is measured, not assumed | VALIDATED ✓ (17 windows: NP-lite 1.03, Prophet 1.09, Bolt-small 1.18 mean MASE; Chronos wins monthly and ≤64 steps, loses past 64; all 80% bands cover 0.60–0.69) | chronos, prophet, benchmark, holdout, coverage |
