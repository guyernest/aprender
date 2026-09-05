# Spike Conventions

Patterns and stack choices established across spike sessions. New spikes follow these unless the question requires otherwise.

## Stack
- **Rust, standalone crate per spike** under `.planning/spikes/NNN-name/` with an empty `[workspace]` table and
  `aprender = { path = "../../../crates/aprender-core", package = "aprender-core", default-features = false }`.
  Copy the workspace `Cargo.lock` in so versions match. Build with `CARGO_TARGET_DIR=../../../target` so
  all spikes share one compiled `aprender-core` (23 s the first time, cached after). `.planning/spikes/.gitignore` ignores `target/`.
- **Python oracles via `uv run --with …`**, never a project venv: `prophet==1.4.0` (`--with prophet --with pandas`),
  `neuralprophet==0.9.0` (`--with neuralprophet --with "pandas<3" --with "numpy<2.3" --with "torch<2.6"`).
  `chronos-forecasting==2.3.1` (`--with chronos-forecasting --with pandas --with safetensors`; pulls torch + transformers).
  Oracle scripts live in the spike's `tools/`, their outputs in `fixtures/*.json`, committed. Model weights are NOT
  committed: `models/` is gitignored; the oracle downloads them and the README says where to copy from.

## Structure
- `src/main.rs` driver prints Markdown (tables) to stdout → saved as `RUN-OUTPUT.md`; `results.json` for numbers;
  `report*.html` self-contained SVG pages (no JS libraries) for what the user should *see*.
- Multi-fixture drivers loop over `fixtures/*.json`; the first fixture gets the deep probes.
- MCP spikes: `src/lib.rs` (tool + `http_app`), `src/main.rs` (`--stdio` default, `--http PORT`, `--bench`),
  `static/index.html` (a real MCP client), `tests/e2e.rs` (in-process streamable-HTTP).
- The `rtk` hook filters `cargo test` / `cargo clippy` output; run the `target/release/deps/<test>-*` binary directly
  for `println!` lines, and `rtk proxy cargo clippy …` when the raw warning list matters.
- Zero-shot model spikes: `models/<name>` is a symlink to the HF snapshot or a sibling spike's weights (gitignored);
  `tools/oracle.py` dumps a ladder (scaling → features → embeddings → hidden rows → quantiles, model AND pipeline
  outputs, timings) plus edge probes into one `fixtures/*_fixture.json`; the driver prints one ladder table.

## Patterns
- **Parity ladder before optimiser claims:** data prep (0 diff) → objective at the oracle's MAP (1e-12) → finite-difference
  gradient at a *perturbed* point → fit → forecast diff vs oracle → **control**: how much does the oracle disagree with
  itself (Newton vs L-BFGS, another seed / lr)? A diff inside that band is parity.
- **Datasets are sorted by `ds` and de-duplicated at load, in BOTH the Rust driver and the Python oracle** (`wp_log_R.csv` is not chronological; spike 006's cross-check caught the mismatch).
- **Accuracy claims use rolling origins and MASE** (period 7 daily / 12 monthly), with naive + seasonal-naive rows and a horizon slice; a single split flatters every model.
- **Never select by test error**: learning rates and epochs are selected by *train* loss; test MAE is only reported.
- **Prophet fit config:** exact L1, objective ÷ T, non-finite guard, `LbfgsF64::new(2000, 1e-7, 20)`, restart from
  the stall point until improvement < 1e-6 relative (≤ 8 rounds), cache `value_and_grad` by x, wall-clock budget.
  `Stalled` is the success status.
- **NeuralProphet training:** `clear_graph()` after every step, Huber from ops with a constant mask (core `SmoothL1Loss`
  is detached), lr sweep {0.01, 0.03, 0.1} lag-free / {0.03, 0.1} with lags, ~4× NP's auto epochs for linear AR.
- Dates are `i64` days since epoch via civil-date arithmetic (`days_from_civil`/`civil_from_days`); no `chrono`.
- **Model ports:** dump a ladder of intermediate tensors from the oracle (scaling, embeddings, hidden states) and
  compare bottom-up before comparing outputs; add edge probes (NaN, short, constant, huge scale) as a second fixture.
- **Hot loops:** write dot products with 8 independent accumulators (LLVM will not vectorise a strict-order float
  reduction). Since spike 008 `blis::gemm_blis` has a NEON 8×6 microkernel on aarch64 (65–77 GFLOP/s, 0.7× faer):
  route every multi-row product through it (projections, feed-forward, patch embeddings, attention scores/context);
  keep `dot8` for single rows. The rayon `blis::gemm` (feature `parallel`) splits M only in 128-row blocks — expect
  ≤ 1.4× on transformer shapes below ~500 tokens.
- **Kernel or core changes are parity work:** measure before/after with the *maintainer's own instrument*
  (`benches/gemm_comparison.rs` for GEMM) on the same machine, run the control on untouched `upstream/main` in a
  git worktree (`git worktree add ../aprender-<topic> -b <branch> upstream/main`), diff raw clippy output between the
  trees, and ship a `contracts/*.yaml` with falsification tests. A flaky test is reported with its solo-run
  failure rate on both trees, never attributed to the change.
- **Model ports keep only the transposed `[in, out]` weights** (the plain-loop A/B copies double memory: Chronos-2
  peaked at 1.45 GB). `safetensors.rs` from spike 007 decodes F32/F16/BF16 from a byte slice, so embedded and
  on-disk weights share one loader; write f16 copies with `tools/to_f16.py` and *measure* the accuracy cost per
  model (0.1 % of scale for Bolt-tiny, 0.3–0.6 % for Chronos-2, 3 % on five-point series).
- **Embedded weights:** `build.rs` stages `model.safetensors` + `config.json` from a build-time env var
  (`CHRONOS_EMBED_DIR`) into `OUT_DIR` for `include_bytes!`, empty markers when unset, and the runtime falls back
  to the same-named `*_MODEL_DIR` path — the `aprender-mcp-setfit-lambda` pattern.
- **Concurrency probes carry a throughput row.** pmcp's streamable-HTTP router holds one `Arc<Mutex<Server>>`
  across each tool call, so a single router serialises fits; a pool of K routers behind a round-robin `fallback`
  handler (spike 010) restores parallelism with bit-identical outputs.

## Tools & Libraries
- `pmcp = { version = "2.19", features = ["streamable-http", "schema-generation"] }`, `schemars = "1.0"`, `axum = "0.8"`,
  `reqwest = "0.12"` (dev) — all already in the workspace lock.
- `half = "2.7"` (F16/BF16 decode), `tower = "0.5"` (router pool `oneshot`), `trueno` feature `parallel` only to
  measure; `uv run --python 3.12 --with huggingface_hub` for weight downloads.
- Avoid: `aprender::nn::loss::SmoothL1Loss` (no gradient); `FISTA` for f64 problems (f32, fixed step); relying on
  `Instant::now()` deltas being non-zero on Apple Silicon (41.67 ns ticks).
