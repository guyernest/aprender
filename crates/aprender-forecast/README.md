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
rung, `prophet::parity::peyton_manning_objective_at_python_map`, evaluates the Rust
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

## Chronos-Bolt weights

Weights are **never committed** (D-18). The Chronos-Bolt zero-shot path reads
[`amazon/chronos-bolt-tiny`](https://huggingface.co/amazon/chronos-bolt-tiny) — **Apache-2.0**,
8.65 M parameters — at one pinned revision:

```
just fetch-chronos-tiny
```

| What | Value |
|---|---|
| Repo | `amazon/chronos-bolt-tiny` |
| Revision | `a0e552de83495b5c28c14c71c374f3e33280b340` |
| `f32/model.safetensors` sha256 | `75068728d376d2bec670379eeef4bfb4d24c0cfe24d957451f8d19b447030a32` (33.0 MB, upstream pin) |
| `f32/config.json` sha256 | `278f0086733031635fb1c861cb01c1bad6477420c7fcb19381a2993e335785e0` (1.1 KB, upstream pin) |
| `f16/model.safetensors` sha256 | `f5dc2ef53533c8896bcb120a754c52c39d8917c15750a9e845192014dfa74a67` (16.5 MB, **derived locally**) |

The recipe writes into `/models/chronos-bolt-tiny/`, which is root-anchored gitignored
(CB-510), and it **verifies on every run, not only on download** — a file that is already
present, cached, or mounted is re-hashed, and a mismatch exits non-zero naming the file.
The f32 shas pin *upstream provenance*; the f16 sha is re-derived from the verified f32 and
so pins *local derivation integrity* only. Spike 007 recorded
`f9a033b42bc516e17ae5756317cb946121afdb59c94b4acfcd30fef93317cd4c` for the same weights: the
two f16 files differ in exactly six bytes — the key order inside the `__metadata__` object —
and their tensor bodies are byte-identical. Newer `safetensors` writes those two keys in the
other order.

Two environment variables, two different jobs:

| Variable | Read at | Does |
|---|---|---|
| `CHRONOS_MODEL_DIR` | runtime **and** build time | The directory the model is loaded from. At build time its presence is what arms the weight-dependent tests: `build.rs` emits `cfg(chronos_weights)` only when `$CHRONOS_MODEL_DIR/model.safetensors` is a file, so without weights those tests are **counted, reasoned skips** (`N ignored`), never a silent green. |
| `CHRONOS_EMBED_DIR` | build time, in the server crate | Stages `model.safetensors` + `config.json` into `OUT_DIR` for `include_bytes!`, so the deployed binary carries its own weights (D-13). |

## Public API changes

Two items in the 0.63.0 line are **breaking for external callers** of this crate. Both were
raised by the Phase 6 incremental code review (`IN-02`, `IN-04`) and are recorded here
because a consumer reads the crate README, not a plan. The release-time capture belongs in
the repository-root `CHANGELOG.md`.

| Symbol | Change | Breaking? | Why |
|---|---|---|---|
| `prophet::feature_row` | Gained a fourth parameter, `hol_sets: &[HashSet<i64>]`, before `out` | **Yes** — the signature changed | The function used to answer "is `day` in this holiday's window at offset `off`?" with a linear `.any()` over `days`, i.e. `rows x holiday_columns x dates` comparisons per design build. The caller now hoists the membership-set construction out of the row loop and passes it in — `prophet::holiday_day_sets(spec)` builds exactly what the parameter wants. This is a measured design-build improvement (plan 06-11) and is deliberately **not** being reverted. |
| `safetensors::load` | **Removed** (`pub fn load(path: &str) -> Result<(Weights, String), String>`) | **Yes** — the item is gone | It had no remaining caller in `crates/` or `src/` and was deleted as dead code. `safetensors::load_bytes` remains and is what the Chronos path actually uses (`chronos::load_model_from_dir` reads the file and hands over bytes), so a caller that needs the old behaviour composes `std::fs::read` with `load_bytes`. |

**On `feature_row`'s index safety.** `hi` comes from `Column.holiday`, i.e. from the `cols`
argument, and it looks up `hol_sets`, a *different* argument that no type ties to it. Since
every `Design` field is `pub`, a caller can construct one whose `cols` and `spec.holidays`
disagree. That lookup is now **total** (`hol_sets.get(hi)`), so a mismatched slice yields a
zero column instead of an out-of-bounds panic raised inside this library. Note what that
does and does not buy: neither a panic nor a zero column is *correct output* for a caller
who built the slice wrong — the point is only that a library should not abort your process
over it. Build `hol_sets` with `holiday_day_sets` from the same `Spec` you pass in, which is
what both in-crate callers do.
