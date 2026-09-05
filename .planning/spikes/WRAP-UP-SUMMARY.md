# Spike Wrap-Up Summary

**Date:** 2026-09-05
**Spikes processed:** 10 (001–010, all VALIDATED)
**Idea:** `prophet-forecast-mcp`
**Feature areas:** Prophet port · NeuralProphet-lite · Forecast MCP thin server · Chronos zero-shot ports · Chronos MCP server · Forecaster evaluation and routing · NEON GEMM kernel
**Skill output:** `./.claude/skills/spike-findings-aprender/` (SKILL.md, 7 references, sources for 10 spikes)

## Processed Spikes

| # | Name | Type | Verdict | Feature Area |
|---|------|------|---------|--------------|
| 001 | prophet-map-fit-lbfgs | standard | VALIDATED | Prophet port (`prophet-fit-and-predict.md`) |
| 002 | neuralprophet-autograd | standard | VALIDATED | NeuralProphet-lite (`neuralprophet-autograd.md`) |
| 003 | prophet-intervals-and-components | standard | VALIDATED | Prophet port (`prophet-fit-and-predict.md`) |
| 004 | forecast-mcp-thin-server | standard | VALIDATED | Forecast MCP thin server (`forecast-mcp-thin-server.md`) |
| 005 | chronos-bolt-tiny-parity | standard | VALIDATED | Chronos zero-shot ports (`chronos-zero-shot-port.md`) |
| 006 | chronos-vs-prophet-holdout | standard | VALIDATED | Forecaster evaluation and routing (`forecaster-evaluation-and-routing.md`) |
| 007 | chronos-mcp-thin-server | standard | VALIDATED | Chronos MCP server (`chronos-mcp-server.md`) |
| 008 | neon-gemm-microkernel-upstream | standard | VALIDATED | NEON GEMM kernel (`gemm-neon-kernel-upstream.md`) |
| 009 | chronos-2-parity | standard | VALIDATED | Chronos zero-shot ports (`chronos-zero-shot-port.md`) |
| 010 | forecast-server-concurrency | standard | VALIDATED | Forecast MCP thin server (`forecast-mcp-thin-server.md`) |

## Key Findings

**Prophet (001, 003).** The Rust port of Prophet 1.4.0's Stan model is bit-for-bit on data prep and
predict and reaches the MAP with `aprender::optim::LbfgsF64` — but only with objective ÷ T, a
non-finite guard, `Stalled` accepted as success, restarts from the stall point (≤ 8 rounds) and an
x-keyed value+gradient cache. Forecasts land inside Prophet's own Newton-vs-L-BFGS band on four
datasets; intervals match to ~1 %; components reconstruct yhat to 1e-16. Constant `y` diverges and
must be refused before fitting. Two core candidates: Stan-style initial step / non-finite
backtracking in `WolfeSearch`, and stop re-evaluating `f`/`∇f` at `x` inside the line search.

**NeuralProphet (002).** NP's default model trains on the existing f32 autograd to NP-level holdout
error (0.451 vs 0.461) in 0.04 s vs 20.9 s. Core `SmoothL1Loss` is detached from the graph (defect);
Huber is built from ops with a constant mask. Full-batch training collapses; lr is selected by train
loss; ~4× NP's auto epochs for linear AR. The AR-Net (0.25 vs naive 0.35 one step ahead) is where NP
earns its keep; trend + seasonality alone is no better than Prophet.

**Forecast MCP server (004, 010).** One typed stateless `forecast` tool over both models, refusals at
the boundary, per-round iteration cap + 15 s wall-clock budget (a 20k fit ran 66 s uncapped),
diagnostics in the response, and a same-origin page that is itself an MCP client. Peyton round trip
1.41 s. Under load every response is bit-identical (8/8 ×3, 16/16) — but pmcp 2.19's streamable-HTTP
router holds one `Arc<Mutex<Server>>` across each tool call, so a single router serialises fits;
a pool of 8 routers behind a round-robin `fallback` gives 3.9× / 6×.

**Chronos ports (005, 009).** Chronos-Bolt (tiny, small) and Chronos-2 run in Rust with no torch and
match `chronos-forecasting` 2.3.1 to f32 rounding (Bolt 1e-6 abs; Chronos-2 2e-5 of scale) on the
main series, 6–7 edge probes and the autoregressive rollout — because a bottom-up parity ladder made
every architectural assumption checkable at its own rung. The 2025 rollout scheme (9 re-quantiled
paths) differs from the median-only scheme by 0.5. Keep only transposed weights (Chronos-2 peaked at
1.45 GB with both). The rayon GEMM splits M only: ≤ 1.4× below ~500 tokens.

**Chronos server (007).** Embedded tiny-f16: 24 MB binary, 52 ms cold start to first forecast,
18.5 ms per 2048-point forward, parity 9.5e-7 through the server; f16 costs 0.11 % of std. Small-f16:
103 MB / 280 ms / 98 ms (S3 or container, not a Lambda zip). Horizon > 64 refused unless
`allow_long_horizon`, then a warning citing the 006 measurement.

**Evaluation (006).** 17 rolling-origin windows on four series: NP-lite 1.029, Prophet 1.094,
Bolt-small 1.178, Bolt-tiny 1.238 mean MASE; baselines 1.69 / 2.02. Chronos wins monthly (0.74–0.80)
and is within 0.1 MASE inside 64 steps; past 64 its rollout degrades to 1.73. Every nominal 80 % band
covers 0.60–0.69 out of sample. Routing rule: monthly or ≤ 64 steps → Chronos, else NP-lite/Prophet.
A rolling-origin gate belongs in the release check.

**NEON GEMM (008).** `gemm_blis` on aarch64 ran the scalar microkernel (7.4 GFLOP/s, 14× behind faer);
the one NEON kernel in the tree was uncallable (wrong panel stride). A 78-line 8×6 kernel honouring
the packing contract gives 65–77 GFLOP/s (0.7× faer), 7.3–10.7× on the crate's own criterion bench,
Chronos forward 137 → 21 ms, rollout 6.4 → 0.97 s, parity unchanged. Packaged with
`contracts/neon-blis-v1.yaml`, three tests, and a PR body on `perf/neon-gemm-8x6-microkernel`
(worktree `~/Development/machine-learning/aprender-neon-upstream`); the only red test is a
pre-existing `Instant::now()` resolution flake reproduced 4/8 on untouched `upstream/main`.
Opening the PR is a checkpoint.

## Open Items Surfaced (not spiked)

- Core: fix `SmoothL1Loss` graph connectivity + a connectivity test for every `nn::loss`; add
  `where`/`clamp`, `cat` to the autograd; `WolfeSearch` initial-step scaling / non-finite backtracking;
  `LbfgsF64` double evaluation at `x`; f64 proximal solver.
- Compute (upstream): 8×12 / 12×8 NEON tile with per-arch `NR`; partition the parallel GEMM along N
  for M ≤ 256; `test_brick_profiler_reset_v2` timer flake; streaming safetensors loader.
- pmcp (upstream): take the router lock only to route, not across the tool future.
- Forecasting features: `freq` H (fractional days), country-holiday calendars, NP quantile regression
  and `n_forecasts > 1`, Chronos-2 long-horizon unrolling and covariates, `model: auto`.
