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
- The `rtk` hook filters `cargo test` output; run the `target/release/deps/<test>-*` binary directly for `println!` lines.

## Patterns
- **Parity ladder before optimiser claims:** data prep (0 diff) → objective at the oracle's MAP (1e-12) → finite-difference
  gradient at a *perturbed* point → fit → forecast diff vs oracle → **control**: how much does the oracle disagree with
  itself (Newton vs L-BFGS, another seed / lr)? A diff inside that band is parity.
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
  reduction); measure `trueno::Matrix::matmul` / `blis::gemm_blis` on the actual shapes before relying on them —
  in spike 005 both were 4× slower than plain loops on 129×256×1024.

## Tools & Libraries
- `pmcp = { version = "2.19", features = ["streamable-http", "schema-generation"] }`, `schemars = "1.0"`, `axum = "0.8"`,
  `reqwest = "0.12"` (dev) — all already in the workspace lock.
- Avoid: `aprender::nn::loss::SmoothL1Loss` (no gradient); `FISTA` for f64 problems (f32, fixed step).
