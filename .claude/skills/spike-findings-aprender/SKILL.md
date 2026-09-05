---
name: spike-findings-aprender
description: Implementation blueprint from spike experiments. Requirements, proven patterns, and verified knowledge for building aprender's time-series forecasting stack (Prophet, NeuralProphet, Chronos ports; stateless forecast MCP servers; NEON GEMM kernel). Auto-loaded during implementation work.
---

<context>
## Project: aprender

**Idea `prophet-forecast-mcp`** — Implement Facebook Prophet (piecewise-linear/logistic trend with
Laplace-prior changepoints, Fourier seasonality, holidays, MAP fit via L-BFGS, simulated-changepoint
uncertainty) and NeuralProphet (the same decomposition plus AR-Net and lagged/future regressors,
trained by gradient descent) natively in aprender, and expose them the way SetFit is exposed: a thin,
single-purpose MCP server (pmcp, Lambda/pmcp.run) for time-series forecasting. Chronos (Amazon's
zero-shot foundation models: Bolt-tiny/small, Chronos-2) joined as a third forecaster with its own
thin server; a NEON GEMM microkernel for `crates/aprender-compute` was built as an upstream
contribution because every Chronos forward is GEMM-bound.

Spike sessions wrapped: 2026-09-03 → 2026-09-05 (spikes 001–010, all VALIDATED). Wrap-up 2026-09-05.
</context>

<requirements>
## Requirements

Idea `prophet-forecast-mcp` (from `.planning/spikes/MANIFEST.md`). Non-negotiable design decisions
that emerged from the user's choices while spiking; every reference below honours them.

- **MCP serving shape is a STATELESS `forecast` tool**: one call carries `ds[]`, `y[]`, horizon
  (and freq); the server fits and forecasts inside that call. No fit → artifact → forecast
  round-trip (decided 2026-09-04 at spike alignment).
- **Both Prophet (MAP / L-BFGS) and NeuralProphet (autograd / AdamW) are in scope**; NeuralProphet
  is spiked, not deferred.
- **Chronos (zero-shot foundation model) joins the idea as a THIRD forecaster with its own thin
  server** (one model per server); Chronos-Bolt first, Chronos-2 later (decided 2026-09-04).
- **Correctness bar for the Prophet port is parity with Python Prophet 1.4.0 on the Peyton Manning
  dataset** (fixture: `.planning/spikes/001-prophet-map-fit-lbfgs/fixtures/peyton_manning_prophet140.json`),
  not self-consistency.
- **Build order (frontier session 2026-09-05): 008 (NEON GEMM kernel, an upstream contribution) →
  007 (Chronos thin server) → 009 (Chronos-2) → 010 (concurrency probe).** The 008 kernel lives in
  `crates/aprender-compute` on a branch cut from `upstream/main`; **opening the PR is a checkpoint,
  not automatic.**
</requirements>

<findings_index>
## Feature Areas

| Area | Reference | Key Finding |
|------|-----------|-------------|
| Prophet port (fit, intervals, components, holidays, logistic, multiplicative) | `references/prophet-fit-and-predict.md` | `LbfgsF64` reaches Prophet 1.4.0 parity **only** with objective ÷ T, a non-finite guard, `Stalled` accepted as success, restarts from the stall point, and an x-keyed value+grad cache; constant `y` must be special-cased |
| NeuralProphet-lite on the autograd | `references/neuralprophet-autograd.md` | Trains to NP-level holdout error 500× faster on existing ops; core `SmoothL1Loss` is detached from the graph (build Huber from ops); never full-batch; select lr by train loss |
| Forecast MCP thin server (Prophet + NP) | `references/forecast-mcp-thin-server.md` | One typed stateless tool, refusals at the boundary, iteration cap + 15 s budget, same-origin page as MCP client; pmcp's router holds one `Arc<Mutex<Server>>` across each tool call — pool K routers (3.9×) |
| Chronos zero-shot ports (Bolt, Chronos-2) | `references/chronos-zero-shot-port.md` | From-scratch T5 forwards match Python to f32 rounding via a bottom-up parity ladder + edge probes; 9-path re-quantiled rollout; transposed weights only; route GEMMs through `gemm_blis` |
| Chronos MCP server (embedded weights, Lambda) | `references/chronos-mcp-server.md` | tiny-f16 embedded: 24 MB binary, 52 ms cold start, 18.5 ms forward, f16 costs 0.1 % of std; horizon > 64 gated by `allow_long_horizon` with a warning |
| Forecaster evaluation and routing | `references/forecaster-evaluation-and-routing.md` | 17 rolling-origin windows: NP-lite 1.03 / Prophet 1.09 / Bolt-small 1.18 mean MASE; Chronos wins monthly and ≤ 64 steps, loses past 64; all nominal 80 % bands cover 0.60–0.69 |
| NEON GEMM kernel + parity-work protocol | `references/gemm-neon-kernel-upstream.md` | 8×6 NEON kernel honouring the packing contract: 7.4 → 65–77 GFLOP/s (0.7× faer), Chronos forward 137 → 21 ms; measure with the maintainer's own bench, control on `upstream/main`, ship a contract |

## Source Files

Original spike source files are preserved in `sources/NNN-spike-name/` (README.md, Cargo.toml,
build.rs, `src/`, `tools/`, `tests/`, `static/`, RUN-OUTPUT*.md, results*.json, PR.md, BENCH.md).
Not copied, to keep the skill small: parity fixtures (`fixtures/*.json`, ~10 MB, committed under
`.planning/spikes/NNN-*/fixtures/`), model weights (`models/`, gitignored — download from the Hub),
`report*.html` and raw `*.log` files (still in `.planning/spikes/`).

## Conventions

How these spikes were built (stack, structure, oracle envs, parity ladder, fit configs) is in
`.planning/spikes/CONVENTIONS.md`. Follow it for new spikes and for porting spike code into crates.
</findings_index>

<metadata>
## Processed Spikes

- 001-prophet-map-fit-lbfgs
- 002-neuralprophet-autograd
- 003-prophet-intervals-and-components
- 004-forecast-mcp-thin-server
- 005-chronos-bolt-tiny-parity
- 006-chronos-vs-prophet-holdout
- 007-chronos-mcp-thin-server
- 008-neon-gemm-microkernel-upstream
- 009-chronos-2-parity
- 010-forecast-server-concurrency
</metadata>
