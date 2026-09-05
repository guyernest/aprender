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
- Frontier session 2026-09-05: build order 008 (NEON GEMM kernel, an upstream contribution) → 007 (Chronos thin server) → 009 (Chronos-2) → 010 (concurrency probe). The 008 kernel lives in `crates/aprender-compute` on a branch cut from `upstream/main`; opening the PR is a checkpoint, not automatic.

## Spikes

| # | Idea | Name | Type | Validates | Verdict | Tags |
|---|------|------|------|-----------|---------|------|
| 001 | prophet-forecast-mcp | prophet-map-fit-lbfgs | standard | Given Peyton Manning, when Prophet's Stan objective + analytic gradient is minimized with `LbfgsF64`, then it converges and yhat matches Python Prophet 1.4.0 within tolerance | VALIDATED ✓ (inside Prophet's own Newton-vs-LBFGS band on 3 datasets; needs f/T scaling + non-finite guard) | prophet, lbfgs, changepoints, fourier |
| 002 | prophet-forecast-mcp | neuralprophet-autograd | standard | Given the same series, when trend + Fourier + AR-Net is trained with AdamW + SmoothL1 on the f32 autograd, then holdout MAE is comparable to 001 using only existing ops | VALIDATED ✓ (test MAE 0.451 vs NP 0.461, 500× faster; core SmoothL1Loss is detached) | neuralprophet, autograd, ar-net |
| 003 | prophet-forecast-mcp | prophet-intervals-and-components | standard | Given the 001 fit, when future changepoints are simulated and components decomposed, then the 80% band covers ~80% of a holdout and components sum to yhat; logistic, multiplicative and holidays fit | VALIDATED ✓ (bands within ~1% of Python, holdout coverage 0.81–0.83 both; L-BFGS needs restarts) | prophet, uncertainty, holidays |
| 004 | prophet-forecast-mcp | forecast-mcp-thin-server | standard | Given a pmcp thin server with one stateless `forecast` tool, when called with ds/y/horizon, then fit + forecast returns in under 2s for 3k points and a browser page charts it | VALIDATED ✓ (Peyton 1.41 s round trip; 3k pts 0.21 s; iteration cap + budget for 20k; page is an MCP client) | mcp, pmcp, latency, ui |
| 005 | prophet-forecast-mcp | chronos-bolt-tiny-parity | standard | Given `amazon/chronos-bolt-tiny` safetensors, when its T5 encoder-decoder (patch embedding, instance scaling, REG token, relative position bias, quantile head) is run in Rust, then the 9 quantiles match the Python `chronos-forecasting` pipeline on Peyton and air passengers within a committed tolerance, including the autoregressive rollout past 64 steps | VALIDATED ✓ (quantiles to 1e-6, rollout to 1.5e-5, 6 edge probes; 36 ms/forward plain Rust vs 5.9 ms torch; trueno GEMMs 4× slower than loops) | chronos, t5, zero-shot, safetensors, parity |
| 006 | prophet-forecast-mcp | chronos-vs-prophet-holdout | standard | Given the spike-001/003 series with rolling-origin holdouts (Peyton daily; air, retail monthly; wp_log_R daily), when zero-shot Chronos-Bolt (tiny and small) and the fitted Prophet / NeuralProphet-lite ports forecast the same windows, then MAE/MASE, 80% coverage/width and latency are compared per series with naive and seasonal-naive baselines, so the value of a Chronos server versus the fitted models is measured, not assumed | VALIDATED ✓ (17 windows: NP-lite 1.03, Prophet 1.09, Bolt-small 1.18 mean MASE; Chronos wins monthly and ≤64 steps, loses past 64; all 80% bands cover 0.60–0.69) | chronos, prophet, benchmark, holdout, coverage |
| 008 | prophet-forecast-mcp | neon-gemm-microkernel-upstream | standard | Given `gemm_blis` on aarch64 falls to `microkernel_scalar` because the only NEON kernel is 8×8 while panels are packed 8×6, when an 8×6 NEON FMA kernel honouring the packed-panel contract is added and dispatched under `cfg(aarch64)`, then it agrees with the scalar kernel within FMA rounding on the existing microkernel test matrix, `cargo test -p aprender-compute --lib` stays green, and a before/after table on spike-005 shapes (129×256×1024) plus square perf-gate shapes shows the packed GEMM beating plain loops, with the Chronos forward dropping from 36 ms toward torch's 6 ms | VALIDATED ✓ (7.4 → 65–77 GFLOP/s, 7.3–10.7× on the crate's own bench, 0.7× faer; suite green except a pre-existing timer flake reproduced upstream; Chronos forward 137 → 21 ms; PR branch ready) | gemm, neon, blis, upstream, performance |
| 007 | prophet-forecast-mcp | chronos-mcp-thin-server | standard | Given Chronos-Bolt embedded in a pmcp thin server exposing the spike-004 `forecast` shape, when called over streamable-HTTP and stdio with ds/y/horizon, then native quantile bands return in under 100 ms for a 2048-point context at horizon ≤ 64, horizon > 64 is refused unless explicitly allowed (with a warning in the response), the browser page charts it, and binary size and cold start with embedded weights (f32 vs f16) are measured for Lambda | VALIDATED ✓ (embedded tiny-f16: 24 MB binary, 52 ms cold start to first forecast, 18.5 ms forward on 2048 pts, parity 9.5e-7 through the server; small-f16 103 MB / 280 ms / 98 ms; f16 costs 0.1 % of std; horizon > 64 gated by allow_long_horizon) | chronos, mcp, pmcp, lambda, latency |
| 009 | prophet-forecast-mcp | chronos-2-parity | standard | Given `amazon/chronos-2` safetensors (120M, RoPE, arcsinh scaling, 21 quantiles, covariates), when ported on the spike-005 ladder (scaling → embeddings → hidden states → quantiles), then Python parity holds on Peyton, air and the edge probes, forward cost is measured with and without 008, and a verdict is given on whether 120M is servable in one binary | PENDING | chronos-2, t5, rope, parity |
| 010 | prophet-forecast-mcp | forecast-server-concurrency | standard | Given the spike-004 server, when 8 NeuralProphet and Prophet requests arrive concurrently over streamable-HTTP, then every response matches its sequential result and no fit is corrupted by another thread's autograd tape | PENDING | mcp, concurrency, autograd |
