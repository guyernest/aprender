---
name: spike-findings-aprender
description: Implementation blueprint from spike experiments. Requirements, proven patterns, and verified knowledge for building aprender's time-series forecasting stack (Prophet, NeuralProphet, Chronos ports; stateless forecast MCP servers; exogenous inputs — regressors and events; NEON GEMM kernel) and its Qwen3.5 LLM decision classifier (Kev few-shot vs SetFit, upstream sync, Python→GGUF→Rust handoff, batched prefill, Lambda / Lambda Managed Instances deployment). Auto-loaded during implementation work.
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

**Idea `forecast-exogenous-inputs`** — Let operators supply the information the forecast models
cannot infer: known future events (holidays) and numeric drivers (price, promotions). Requested by
Forecast Coach, an MCP app that consumes `aprender-forecast` as a git dependency pinned by tag and
calls the crate's one stateless door for both `prophet` and `neuralprophet`. Extends the ports built
under `prophet-forecast-mcp`. Two of the change request's four premises were refuted against its own
pinned tag (`aprender-forecast-v0.63.0` = `fdf6b1802`) before any spike was built. The real work is
regressors on both models, events on NeuralProphet, and proving a tag bump is safe.

**Idea `llm-decision-classifier`** — Evaluate Kev (github.com/jaredpalmer/kev, Apache-2.0), the open
replica of the Jev decision model, as a classifier family in aprender alongside SetFit. Kev is a
Qwen3.5 base plus a rank-16 LoRA and a pointer head that scores each option's `</opt>` hidden state
against the question's `<decide>` hidden state; it answers yes/no (`noul`), `choice` and `score`
questions with calibrated probabilities and never generates text. Upstream aprender (`paiml/aprender`)
already runs the Qwen3.5 Gated-DeltaNet hybrid on CPU and CUDA, but the fork was 280 commits behind it
(2026-09-23), and upstream's `Qwen35Model` is GGUF, token-at-a-time and logits-only.

Spike sessions wrapped: 2026-09-03 → 2026-09-05 (spikes 001–010, `prophet-forecast-mcp`);
2026-09-20 (spikes 011–014, `forecast-exogenous-inputs`); 2026-09-23 (spikes 015–017, 019, 020,
`llm-decision-classifier` — 3 VALIDATED, 2 PARTIAL; spike 018 was dropped before it was built).
</context>

<requirements>
## Requirements

Non-negotiable design decisions that emerged from the user's choices while spiking; every reference
below honours the requirements of the idea it belongs to. Grouped by idea key.

### Idea `prophet-forecast-mcp`

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

### Idea `forecast-exogenous-inputs`

- **Byte-identical when the new arguments are absent.** Every committed fixture must reproduce its
  pre-change `ForecastResponse` exactly after the plumbing lands. This is the consumer's acceptance
  gate, not a nice-to-have (CR constraint 1, 2026-09-20).
- **No new required arguments.** Every new field is `Option<_>` with a serde default;
  `#[serde(deny_unknown_fields)]` stays as it is.
- **Errors, not silence.** An unsupported combination (a driver on a model that cannot use it) is
  refused at the door with a message naming the limitation — the existing D-11 pattern.
- **One `RegressorArg` shape serves both models**, so the caller sends one argument to `prophet`
  and `neuralprophet` alike.
- **Correctness bar for regressors is parity with Python Prophet 1.4.0**, measured by extending the
  existing rung ladder (data prep → predict-at-Python-params → components), not self-consistency.
- **One tag per release**; the consumer bumps one line plus the lock entry and builds `--locked`.
- The public entry point is **`forecast(&ForecastArgs)`**. `fitted_forecast` does not exist at HEAD
  or at the pinned tag — the CR names an API the crate does not ship (recorded 2026-09-20).

### Idea `llm-decision-classifier`

- **Few-shot steering is the product.** The business supplies a handful of labelled examples
  (SetFit's 8–64 per class regime) to add its own knowledge and bias; massive datasets are out of
  scope. Zero-shot Kev with `criteria` descriptions is the baseline few-shot must beat.
- **Training may stay in Python; inference must be Rust on aprender.** Fine-tuning is a back-office
  process whose only output is weights; the served path (speed, security, AWS Lambda) is Rust.
  Rust-side training is a bonus, not a requirement (decided 2026-09-23).
- **The Python-to-Rust weight handoff is part of the contract**: a Kev checkpoint (adapter + head)
  must export to an artifact the Rust inference path loads, with probability parity to Python fp32.
- **Qwen3.5 support comes from upstream, not a fork-local re-port.** Sync `paiml/aprender` first and
  extend its `Qwen35Model` rather than writing a second implementation (OPS-03).
- **Lambda is the deployment target**: size, memory, cold start and latency are measured against
  Lambda limits, not assumed.
- **Kev needs a batched prefill before it is servable on CPU** (token-at-a-time is 5–7 s per
  decision). Rust head training (spike 018) was dropped because training stays in Python.
- **Lambda was measured by a local proxy** (6 threads, 10 GB cap), not a real AWS deploy, for the
  2026-09-23 session.
- **Lambda Managed Instances is the intended host for Qwen-sized models**, alongside the existing
  smaller Rust MCP servers (recorded 2026-09-23 at wrap-up). Nothing on LMI is measured yet.
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
| **Prophet external regressors** | `references/prophet-external-regressors.md` | **Additive change, not a restructure** — a 219-line splice on the shipped `Design` hits full parity (column order identical at 24/30 cols, `s_a`/`s_m` exact 0.0, yhat 4.5e-16 of y_scale). `std` is pandas ddof = 1. A driver collinear with trend/seasonality has **no reproducible lift** |
| **NeuralProphet exogenous inputs (events, regressors)** | `references/neuralprophet-exogenous-inputs.md` | Events compose as one `Linear(E,1)` beside `NpModel` and recover a planted effect to 5.8 % — but **`train_cost` has no event term**: at `MAX_HOLIDAY_COLUMNS` a request buys **7.6×** the priced work. Lag-free never reads an imputed-day regressor value; **lagged reads all of it** (~30 % of scale) |
| **No-argument bitwise invariance gate** | `references/no-argument-invariance-gate.md` | The consumer's tag-bump gate is **free**, because regressor columns append: 8/8 bit-identical, plumbing inert at zero regressors on 3 datasets, and the signature is mutation-proven to detect 1 ULP |

| **Kev few-shot evaluation (vs SetFit)** | `references/kev-few-shot-evaluation.md` | **Kev wins at 0–16 shots, SetFit at 32+ on lexical tasks.** Kev-4B zero-shot (0.607) beats SetFit@64 (0.561) on stance, and 3 floats of `bias` lift it to 0.642. SetFit wins emotion at 64 shots, 0.705 vs 0.577. 0.8B is only SetFit-level. head_ft needs a refit temperature (ECE up to 0.37) |
| **Upstream sync** | `references/upstream-sync.md` | 280 commits resolve as 34 conflicts by 5 rules plus 3 `cargo check`-only fixes. The one red SetFit golden is already red on the fork, as a control worktree proved. Merge `895c654de` is local, not pushed |
| **Qwen3.5 decision inference in Rust** | `references/qwen35-decision-inference.md` | PEFT merge → llama.cpp GGUF → upstream `Qwen35Model` + hidden readout + 40-line head gives **probs to 1.5e-6, 12/12**. The MTP `block_count` loader defect hits every fresh GGUF. A batched prefill (GEMM projections, **DeltaNet per head in parallel**) takes an 87-token decision from 6.6 s to **0.36 s on 6 threads**. F32-only |
| **LLM classifier on Lambda / Lambda Managed Instances** | `references/llm-classifier-lambda-deployment.md` | Kev-0.8B fits default Lambda: 1.4 MB binary + 3 GB weights, first decision 0.82 s, 0.2–0.4 s on M4 (est. 1–1.5 s Graviton2). Kev-4B does not. **LMI (32 GB / 16 vCPU, no cold starts, concurrent Rust) fits 4B only after the loader fixes.** Keep small servers on scale-to-zero Lambda |

## Source Files

Original spike source files are preserved in `sources/NNN-spike-name/` (README.md, Cargo.toml,
build.rs, `src/`, `tools/`, `tests/`, `static/index.html` (the demo pages — real MCP clients),
RUN-OUTPUT*.md, results*.json, baseline.json, PR.md, BENCH.md).
Not copied, to keep the skill small: parity fixtures (`fixtures/*.json`, ~10 MB, committed under
`.planning/spikes/NNN-*/fixtures/`), model weights (`models/`, gitignored — download from the Hub),
`report*.html`, raw `*.log` files, and spike 011's 256 KB `results.json` (chart-feeding data for its
`report.py`) — all still in `.planning/spikes/`. For 015–020, these are also left out: the
frozen-feature `runs/*.npz` (~200 MB), `vendor/kev` and `vendor/llama.cpp` checkouts, the
GGUF/safetensors models (up to 3 GB each), and spike 017's 3.3 MB `kev-0.8b_fixture.json`.

## Conventions

How these spikes were built (stack, structure, oracle envs, parity ladder, fit configs, falsification
probes, premise-checking against a consumer's pinned tag) is in `.planning/spikes/CONVENTIONS.md`.
Follow it for new spikes and for porting spike code into crates.
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
- 011-prophet-regressor-parity
- 012-no-arg-bitwise-invariance
- 013-np-events-autograd
- 014-np-gap-imputation-regressors
- 015-kev-vs-setfit-few-shot
- 016-upstream-sync-qwen35
- 017-kev-rust-forward-parity
- 019-kev-lambda-inference
- 020-qwen35-batched-prefill
</metadata>
