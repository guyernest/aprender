---
phase: 06-native-time-series-forecasting-stack
reviewed: 2026-09-06T00:00:00Z
depth: standard
files_reviewed: 30
files_reviewed_list:
  - crates/aprender-forecast/src/lib.rs
  - crates/aprender-forecast/src/types.rs
  - crates/aprender-forecast/src/dates.rs
  - crates/aprender-forecast/src/fit.rs
  - crates/aprender-forecast/src/forecast.rs
  - crates/aprender-forecast/src/prophet.rs
  - crates/aprender-forecast/src/np.rs
  - crates/aprender-forecast/src/bolt.rs
  - crates/aprender-forecast/src/chronos.rs
  - crates/aprender-forecast/src/safetensors.rs
  - crates/aprender-forecast/src/test_support.rs
  - crates/aprender-forecast/build.rs
  - crates/aprender-forecast/Cargo.toml
  - crates/aprender-forecast/examples/mase_rolling_origin.rs
  - crates/aprender-mcp-forecast/src/lib.rs
  - crates/aprender-mcp-forecast/src/main.rs
  - crates/aprender-mcp-forecast/tests/e2e_stdio.rs
  - crates/aprender-mcp-forecast/Cargo.toml
  - crates/aprender-mcp-chronos/src/lib.rs
  - crates/aprender-mcp-chronos/src/main.rs
  - crates/aprender-mcp-chronos/build.rs
  - crates/aprender-mcp-chronos/Cargo.toml
  - crates/aprender-core/tests/monorepo_invariants.rs
  - contracts/forecast-tool-boundary-v1.yaml
  - contracts/prophet-parity-v1.yaml
  - contracts/neuralprophet-parity-v1.yaml
  - contracts/chronos-bolt-parity-v1.yaml
  - contracts/aprender/binding.yaml
  - justfile
  - Makefile
  - Cargo.toml
findings:
  critical: 3
  warning: 10
  info: 0
  total: 13
status: issues_found
---

# Phase 6: Code Review Report

**Reviewed:** 2026-09-06
**Depth:** standard
**Files Reviewed:** 30
**Status:** issues_found

## Summary

Three pure-Rust forecaster ports plus two thin pmcp servers. The transport/library
boundary is genuinely respected — neither server crate contains numerics, both call
`aprender_forecast::forecast` / `chronos::forecast` and map errors, and the `pooled_app`
round-robin owns no shared mutable state beyond an `AtomicUsize` counter. Panic-safety in
`safetensors::load_bytes` is real (`checked_add`, `checked_mul`, `get(a..b)`, `chunks_exact`
with a post-hoc `numel` check) and the justfile/Makefile gate recipes capture `rc=$?` on its
own line throughout — the piped-`$?` class this repo shipped twice does not recur here.

What does not hold up is the **cost model behind the tool boundary**. The three holiday
constants bound each factor of the design build individually and nothing bounds their
product: one legal ~460 KB request buys ~1.5e10 date comparisons and a 117 MB design matrix,
in `make_design`, *before* the cooperative fit budget starts observing anything. Second, the
Prophet logistic uncertainty path uses Knuth's Poisson sampler, which silently degenerates
once `lambda` exceeds ~745 — reachable with 100 daily points and a multi-year horizon, and
untested. Third, the door refuses every cross-model option except `cap`, which is silently
dropped for linear and flat growth — precisely the failure class the adjacent comment block
calls "the worst failure class at this door".

Secondary theme: D-15 ("the Rust constants are asserted equal to the contract, never
duplicated as literals") is only half-implemented. Ten bound constants are asserted; six —
`pool_default`, `default_seed`, `default_interval_width_x100`, `concurrency_requests`,
`pool_speedup_min_x10`, `pool_speedup_best_of` — are hand-copied literals carrying a comment
that claims otherwise, and one test is *named* for a contract assertion it never makes.

## Critical Issues

### CR-01: The holiday design build is unbounded in the product of its three bounds

**File:** `crates/aprender-forecast/src/forecast.rs:168-213`, `crates/aprender-forecast/src/prophet.rs:169-183`, `crates/aprender-forecast/src/prophet.rs:247-249`

**Issue:** `MAX_HOLIDAY_WINDOW` (731 columns per holiday), `MAX_HOLIDAY_DATES` (1 000 dates
per holiday) and `MAX_POINTS` (20 000 rows) are each checked in isolation. `feature_row`
scans *every date of the holiday* for *every holiday column* on *every row*:

```rust
// prophet.rs:178-182
for c in cols.iter().filter(|c| c.holiday.is_some()) {
    let (hi, off) = c.holiday.expect("holiday col");
    let hit = spec.holidays[hi].days.iter().any(|&d| d + off == day);   // O(dates)
    out.push(if hit { 1.0 } else { 0.0 });
}
```

so `make_design` (prophet.rs:247-249) is `O(rows × holiday_columns × dates_per_holiday)`.

A single request that passes **every** door check:

- `ds`/`y`: 19 999 consecutive daily points (≤ `MAX_POINTS`, span ≤ `MAX_SPAN_DAYS`)
- `holidays`: **one** holiday, `lower_window: -365`, `upper_window: 365` (731 columns, ≤
  `MAX_HOLIDAY_COLUMNS` = 1 000), `dates`: 1 000 dates chosen far from the series so `.any()`
  never short-circuits

yields `19 999 × 731 × 1 000 ≈ 1.46e10` comparisons in `make_design`, plus another `3 650 ×
731 × 1 000 ≈ 2.7e9` in `predict` (prophet.rs:686-689), and a design matrix of
`19 999 × 731 × 8 B ≈ 117 MB`. The JSON payload is ≈ 460 KB — under any plausible body limit.

Two aggravating facts:
1. This happens **before** `fit_prophet` is entered, so `FIT_BUDGET_SECS` (documented as
   cooperative and round-boundary-only, `fit.rs:65-75`) never observes it. There is no budget
   of any kind on design construction.
2. `pooled_app` gives K routers, so K such requests run concurrently: at the default
   `--pool 8` that is ~940 MB of design matrices and 8 cores pinned for minutes.

`types.rs:45-49` states the intent ("`prophet::feature_row` scans this list per row per
holiday column, so it is a second multiplier on the design build") and then bounds only the
multiplicand. `contracts/forecast-tool-boundary-v1.yaml` claims these bounds are "the HARD
ceiling on the work one request can buy (T-06-02)"; they are not.

**Fix:** Bound the product, and make the holiday lookup not linear in `dates`. Both:

```rust
// forecast.rs, inside the holiday loop — bound the DESIGN COST, not just each factor.
// Add to types.rs and to constants: in the contract, then assert equality as the
// other seven bounds already are.
pub const MAX_HOLIDAY_DESIGN_COST: u128 = 50_000_000; // rows * columns * dates

let cost = (ds.len() as u128)
    * (holiday_columns as u128)
    * (total_holiday_dates as u128);
if cost > MAX_HOLIDAY_DESIGN_COST {
    return Err(ForecastError::Validation(format!(
        "holidays expand to {cost} design cells (rows x columns x dates), which exceeds \
         max_holiday_design_cost {MAX_HOLIDAY_DESIGN_COST}"
    )));
}
```

and replace the linear scan with a set lookup so the `dates` factor disappears from the
inner loop entirely (this alone removes three orders of magnitude and changes no number the
parity ladder measures, because membership is membership):

```rust
// prophet.rs — build once in make_design/predict, not per row.
use std::collections::HashSet;
let day_sets: Vec<HashSet<i64>> =
    spec.holidays.iter().map(|h| h.days.iter().copied().collect()).collect();
// ...
let hit = day_sets[hi].contains(&(day - off));
```

### CR-02: The logistic uncertainty path's Poisson sampler underflows and returns wrong bands

**File:** `crates/aprender-forecast/src/prophet.rs:794-805`

**Issue:** The simulated-changepoint count uses Knuth's product method:

```rust
let lambda = s_cnt * (t_max - 1.0);
let mut n_changes = 0usize;
let mut pp = 1.0;
let l = (-lambda).exp();
loop {
    pp *= rng.uniform();
    if pp <= l { break; }
    n_changes += 1;
}
```

`(-lambda).exp()` is exactly `0.0` for `lambda > 745.13`. The loop then terminates only when
`pp` itself underflows to zero — after ≈ 745 multiplications by U(0,1) — so `n_changes`
becomes a function of f64 subnormal exhaustion rather than of the Poisson law it is meant to
draw from.

That regime is reachable from an in-bounds request. `lambda = n_changepoints × (t_max - 1)`,
and `t_max = (last_future_day - start_day) / history_span_days`. With 100 consecutive daily
points (span 99, `n_changepoints = 25`) and `horizon: 3650, growth: "logistic"`:
`t_max = 3749/99 ≈ 37.9`, `lambda ≈ 25 × 36.9 = 922 > 745`. With the 10-point minimum
(`n_changepoints = 7`, span 9) and the same horizon, `lambda ≈ 2 839`.

Python Prophet calls `np.random.poisson(lambda)`, which is correct at any `lambda`. The port
therefore returns `yhat_lower` / `yhat_upper` / `trend_lower` / `trend_upper` that are
silently wrong (too few simulated changepoints, so bands that are too narrow) for long-horizon
logistic requests, with no warning and no diagnostic. `wp_log_R_logistic_prophet140.json` is
the only logistic fixture and does not exercise a horizon anywhere near this ratio, so no
parity rung catches it.

**Fix:** Switch to a transformed-rejection / normal-approximation branch above the Knuth
regime, and refuse or clamp explicitly rather than degrade:

```rust
// prophet.rs
fn poisson(rng: &mut Rng, lambda: f64) -> usize {
    if lambda < 30.0 {
        // Knuth product method — valid while exp(-lambda) is representable.
        let (mut n, mut pp, l) = (0usize, 1.0, (-lambda).exp());
        loop { pp *= rng.uniform(); if pp <= l { return n; } n += 1; }
    }
    // Normal approximation with continuity correction; exact enough for a band simulation
    // and, unlike the above, defined for every lambda this door can produce.
    (lambda + rng.normal() * lambda.sqrt() + 0.5).max(0.0) as usize
}
```

Add a falsification test that asserts `poisson(rng, 900.0)` has a sample mean within a few
percent of 900 — today the equivalent returns ≈ 745 regardless of `lambda`.

### CR-03: `cap` is silently dropped for linear and flat growth

**File:** `crates/aprender-forecast/src/forecast.rs:109-127`, `crates/aprender-forecast/src/forecast.rs:150-167`, `crates/aprender-forecast/src/forecast.rs:216`

**Issue:** The door refuses every well-formed option aimed at the wrong arm — `n_lags` on
Prophet, and `growth`/`cap`/`seasonality_mode`/`holidays` on NeuralProphet — under a comment
that names the reason: "the worst failure class at this door is the caller getting a plausible
answer to a question they did not ask (D-11)". The `cap` check is then made conditional on
growth:

```rust
if growth == Growth::Logistic {
    let cap = args.cap.ok_or_else(...)?;   // only path that reads cap
    ...
}
// ...
spec.cap = args.cap;                       // forecast.rs:216, unconditional
```

`make_design` matches `(Growth::Logistic, Some(c))` and falls through to `_ => None` for every
other growth (prophet.rs:222-226), so `{"growth": "linear", "cap": 100}` — or `cap` with no
`growth` at all, i.e. the default linear arm — is accepted, silently ignored, and answered
with an unbounded linear forecast. The caller asked for a saturating curve and was given a
straight line, with nothing in `diagnostics` saying so.

`forecast::tests::an_option_belonging_to_the_other_model_is_refused_not_dropped`
(forecast.rs:655-665) asserts exactly this class for the cross-model case and does not cover
the cross-growth case.

**Fix:** Refuse, in the same shape as the neighbours:

```rust
// forecast.rs, in the "prophet" arm, immediately after `growth` is resolved
if growth != Growth::Logistic && args.cap.is_some() {
    return Err(ForecastError::Validation(
        "cap is logistic-only; set growth to \"logistic\"".into(),
    ));
}
```

and extend `an_option_belonging_to_the_other_model_is_refused_not_dropped` (plus one e2e
case in `aprender-mcp-forecast/src/lib.rs`) with `{"growth": "linear", "cap": ...}` and a
bare `{"cap": ...}`.

## Warnings

### WR-01: Six contract constants are hand-duplicated as literals, contradicting the contract's own D-15 claim

**File:** `contracts/forecast-tool-boundary-v1.yaml:11-13,88-98`, `crates/aprender-mcp-forecast/src/main.rs:29-32`, `crates/aprender-mcp-forecast/src/lib.rs:986-988`, `crates/aprender-forecast/src/forecast.rs:86,92`, `justfile:703-766`

**Issue:** The contract header states "the Rust constants are asserted equal to it at test
time via `test_support::constant_u64`, never duplicated as literals (D-15)". Ten constants
are (`types.rs:151-226`, `aprender-mcp-chronos/src/lib.rs:745-771`). Six are not:

| Contract key | Duplicated as | Asserted? |
|---|---|---|
| `pool_default: 8` | `main.rs:32 DEFAULT_POOL = 8` and `lib.rs:988 POOL = 8` | no — comment only |
| `default_seed: 42` | `forecast.rs:92 args.seed.unwrap_or(42)` | no |
| `default_interval_width_x100: 80` | `forecast.rs:86 unwrap_or(0.8)` | no |
| `concurrency_requests: 8` | `lib.rs:1053-1080` (eight literal requests) | no |
| `pool_speedup_min_x10: 20` | `justfile:762 v + 0 >= 2.0` | no |
| `pool_speedup_best_of: 3` | `justfile:710 for i in 1 2 3` | no |

A grep for these keys across `crates/**/*.rs`, `justfile` and `Makefile` returns only the
comments that claim the mirroring. Each is a bound that can be loosened in one place — the
exact defect D-15 exists to prevent.

**Fix:** Assert them the way the other ten are:

```rust
// crates/aprender-mcp-forecast — a small contract reader like the chronos crate's
// contract_f64, or widen aprender_forecast::test_support beyond pub(crate).
#[test]
fn pool_default_matches_contract() {
    assert_eq!(
        DEFAULT_POOL as u64,
        contract_u64("forecast-tool-boundary-v1", "constants.pool_default"),
    );
}
```

and have `just forecast-pool-ratio` read the bar out of the contract rather than embedding
`2.0` and `1 2 3`:

```bash
bar=$(python3 -c "import yaml,sys;print(yaml.safe_load(open('contracts/forecast-tool-boundary-v1.yaml'))['constants']['pool_speedup_min_x10']/10)")
```

(or, preferring the in-tree tool per CLAUDE.md, expose the value through `pv`).

### WR-02: A test named for a contract assertion never reads the contract

**File:** `crates/aprender-mcp-forecast/src/main.rs:230-240`

**Issue:**

```rust
#[test]
fn http_args_default_to_the_contract_pool_size() {
    assert_eq!(parse_http_args(&argv(&[])), Some((DEFAULT_PORT, DEFAULT_POOL)));
```

The assertion compares `parse_http_args`'s default against the same constant
`parse_http_args` reads, so it can only fail if the parser stops honouring its own default.
Editing `constants.pool_default` to any value leaves this green. The name asserts a claim
the body does not make — the vacuous-guard class CLAUDE.md's Verification Discipline #5
names.

**Fix:** Either rename it (`http_args_default_to_DEFAULT_POOL`) or, better, make it earn its
name by reading `constants.pool_default` per WR-01 and comparing that to `DEFAULT_POOL`.

### WR-03: The advertised D-16 coverage numbers are measured on a re-implementation, not on the shipped door

**File:** `crates/aprender-forecast/examples/mase_rolling_origin.rs:239-297`, `crates/aprender-mcp-forecast/src/lib.rs:33-34`

**Issue:** Both tool descriptions advertise measured coverage ("covered 0.60 (Prophet) and
0.66 (NeuralProphet-lite) of held-out points across 17 rolling-origin windows") as a caller-
facing honesty claim. That number is produced by `mase_rolling_origin.rs`, which does **not**
call `aprender_forecast::forecast`; it rebuilds the pipeline and diverges from it in three
ways:

1. `np_lite_row` uses `seed: 7` (line 271); the door defaults to `42` (`forecast.rs:92`).
   Different init and different mini-batch shuffles ⇒ a different model ⇒ different coverage.
2. `Z80 = 1.2816` (line 62) is a hand-written literal; the door computes
   `normal_quantile((1.0 + 0.8) / 2.0)` (`forecast.rs:372`).
3. `prophet_row` (line 240) builds `Spec::default_linear(auto_seasonalities(...))` **without**
   the `seasonalities.is_empty() && holidays.is_empty()` weekly fallback the door adds
   (`forecast.rs:220-230`) — so the example and the server can build different designs for
   the same series.

The claim printed to every MCP client therefore describes a code path no client can reach.

**Fix:** Have the example call the shipped door for both fit models, so the number measured
is the number served:

```rust
fn prophet_row(ds: &[String], y: &[f64], horizon: usize) -> (String, Fc) {
    let args = ForecastArgs { ds: ds.to_vec(), y: y.to_vec(), horizon,
        model: Some("prophet".into()), ..Default::default() };
    let r = aprender_forecast::forecast(&args).expect("in-bounds window");
    ...
}
```

If the example must stay decoupled for speed, at minimum align `seed` and `Z80` and add the
same seasonality fallback, and say in the header that the numbers are a proxy.

### WR-04: `MAX_POINTS` is enforced only after full deserialization; no request-size bound exists in either crate

**File:** `crates/aprender-forecast/src/forecast.rs:34-52`, `crates/aprender-mcp-forecast/src/lib.rs:77-115`, `crates/aprender-mcp-chronos/src/lib.rs:128-166`

**Issue:** `ForecastArgs.ds: Vec<String>` and `y: Vec<f64>` are deserialized in full by pmcp
before `forecast()` sees them; `args.ds.len() > MAX_POINTS` is checked at
`forecast.rs:47`, after the allocation has already happened. Neither `http_app` installs a
`DefaultBodyLimit`, and the stdio transport (`server.run_stdio()`, the transport every MCP
desktop client actually spawns) has no line-length or message-size limit at all. A 500 MB
single JSON-RPC line over stdio is allocated and then refused.

The contract frames `fit_max_points` as the T-06-02 bound on "the work one request can buy";
it bounds the work *after* the memory has been spent.

**Fix:** Add an explicit, contract-owned body limit on the HTTP side and state the stdio
exposure:

```rust
use axum::extract::DefaultBodyLimit;
axum::Router::new()
    // ... routes ...
    .nest("/mcp", mcp)
    .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
```

with `MAX_REQUEST_BYTES` derived from `fit_max_points` (20 000 × ~40 B for ds+y, plus the
holiday allowance) and asserted against a new `constants.max_request_bytes`.

### WR-05: File-scope `#![allow(clippy::disallowed_methods)]` disables the repo-wide `unwrap()` ban across ~4 000 lines

**File:** `crates/aprender-forecast/src/types.rs:11`, `crates/aprender-forecast/src/forecast.rs:11`, `crates/aprender-forecast/src/chronos.rs:19`, `crates/aprender-mcp-forecast/src/lib.rs:13`, `crates/aprender-mcp-chronos/src/lib.rs:19`, `crates/aprender-mcp-chronos/src/main.rs:22`

**Issue:** CLAUDE.md: "`unwrap()` banned outright" via `.clippy.toml` disallowed-methods. Six
files turn the lint off for their entire module because `schemars`' derive and
`serde_json::json!` expand to `.unwrap()` at file scope. The justification is real, but the
blast radius is not scoped to the macro: `forecast.rs` (704 lines), `chronos.rs` (727) and
the two server `lib.rs` files (1 235 + 938) are now unlinted for the project's single hardest
rule. No `.unwrap()` exists in them today — this is a latent hole, not a live defect, and it
will not be caught the day one appears.

**Fix:** Move the macro-heavy code behind a narrower boundary so the allow shrinks with it,
e.g. put the `json!`-built diagnostics in a `mod diagnostics { #![allow(...)] ... }` inner
module, and put the `JsonSchema` derives in a `mod schema` submodule. Where that is not
practical, add a guard test that fails on a literal `.unwrap()` in these files (the same
file-wide-grep discipline `np.rs:12-18` already uses for the D-10 Huber ban).

### WR-06: `chronos::forecast` documents a lossy-rounding hazard, then performs it in `diagnostics.quantile_levels`

**File:** `crates/aprender-forecast/src/chronos.rs:301-305` vs `crates/aprender-forecast/src/chronos.rs:342`

**Issue:** Lines 301-304 explain at length why `{lvl:.1}` is unacceptable ("a grid like
[0.05, 0.1, 0.5, 0.9, 0.95] collapses 0.05/0.1 and 0.9/0.95 onto the same key... the surviving
'0.9' series is q95 while claiming to be q90"), and the map key correctly uses
`lvl.to_string()`. Line 342 then reports the levels with exactly that transform:

```rust
"quantile_levels": cfg.quantiles.iter()
    .map(|q| (f64::from(*q) * 10.0).round() / 10.0).collect::<Vec<f64>>(),
```

For a `[0.05, …, 0.95]` checkpoint this reports `[0.1, …, 1.0]` — including the impossible
level 1.0 — while `quantiles` reports the true keys. The two fields of one response then
disagree, and the comment reads as if the hazard had been closed everywhere. It is
inert for the pinned tiny checkpoint (`[0.1 … 0.9]`) and wrong for any other.

**Fix:** Report the levels verbatim, matching the map keys:

```rust
"quantile_levels": cfg.quantiles.iter().map(|q| f64::from(*q)).collect::<Vec<f64>>(),
```

and add an assertion that `diagnostics.quantile_levels` and the `quantiles` map keys name the
same set.

### WR-07: Peak per-request allocation scales with `uncertainty_samples × horizon` and is unrelated to `MAX_POOL`

**File:** `crates/aprender-forecast/src/prophet.rs:71,747`, `crates/aprender-mcp-forecast/src/main.rs:36`

**Issue:** `predict` allocates `let mut unc = vec![vec![0.0; n]; ns];` with `ns = 1000`
(`Spec::default_linear`, not caller-configurable) and `n = horizon`. At `MAX_HORIZON = 3650`
that is 3.65 M f64 ≈ 29 MB per in-flight request, plus 1 000 `Vec` headers. `MAX_POOL = 256`
(`main.rs:36`) permits 256 concurrent fits, so an operator following the documented flag
range can configure ~7.5 GB of peak transient allocation with no diagnostic. The
`MAX_POOL` comment reasons about tokio's blocking-thread budget and about `Vec::with_capacity`
overflow, never about the per-request working set.

**Fix:** Either allocate the uncertainty scratch column-wise (only `ns` values are needed at a
time for the percentile step — `col_y`/`col_t` already exist), or relate `MAX_POOL` to the
per-request peak:

```rust
/// K x the per-request peak (uncertainty_samples x MAX_HORIZON x 8 B ~= 29 MB) must stay
/// inside a plausible container limit; 32 routers ~= 1 GB.
const MAX_POOL: usize = 32;
```

### WR-08: The Phase 6 binding resolver is weaker than its comment block claims

**File:** `Makefile:2357-2411`

**Issue:** The target's rationale is that `pv audit` "does not open the file `module_path`
names", so this resolver does. Two gaps remain in the resolver itself:

1. The `awk` extractor emits a row only when a `function:` key is seen
   (`Makefile:2363-2365`). A binding row with `contract` + `equation` + `module_path` but no
   `function:` produces no line in `$rows`, is counted by neither `resolved` nor
   `unresolvable`, and passes silently. All 55 current rows carry `function:` (verified), so
   this is latent — but it is a hole in the guard that exists to catch missing rows.
2. `pattern="(^|[^[:alnum:]_])fn[[:space:]]+$name[[:space:]]*[(<]"` matches anywhere in the
   file, including inside `#[cfg(test)] mod`, doc-comment code fences and string literals. A
   row can resolve to a test helper of the same name. (No current row does — verified by
   re-running the resolver and checking every match precedes the first `#[cfg(test)]`.)

**Fix:** Make a missing `function:` an explicit failure and exclude test modules:

```awk
/^- contract:/ { if (c != "" && index(want," " c " ")>0 && f=="") \
                    print c, e, m, "MISSING_FUNCTION_KEY"; c=$3; e=""; m=""; f=""; next }
```

and, for the grep, strip from the first `#[cfg(test)]` before matching:

```bash
sed '/^#\[cfg(test)\]/,$d' "$file" | grep -Eq "$pattern"
```

Re-run the RED-turning mutation in the new scope (CLAUDE.md Verification Discipline #4)
after changing either.

### WR-09: `--bench` silently drops unparseable sizes and falls back to the defaults

**File:** `crates/aprender-mcp-forecast/src/main.rs:192-200`

**Issue:**

```rust
let sizes: Vec<usize> = args[1..].iter().filter_map(|a| a.parse().ok()).collect();
bench(if sizes.is_empty() { &[1_000, 3_000, 10_000, 20_000] } else { &sizes });
```

`--bench 3O00` (letter O) silently benchmarks 1 000/3 000/10 000/20 000 instead, and
`--bench 1000 abc 3000` silently benchmarks only 1 000 and 3 000. `parse_http_args` in the
same file refuses rather than defaults, under a comment naming that as "the same
refuse-never-default rule the tool boundary itself follows". `just forecast-bench` parses the
resulting table by size, so a typo produces a table that looks right and measures something
else.

**Fix:**

```rust
let mut sizes = Vec::new();
for a in &args[1..] {
    match a.parse::<usize>() {
        Ok(n) => sizes.push(n),
        Err(_) => {
            eprintln!("error: --bench takes point counts; {a:?} is not a number");
            return ExitCode::from(2);
        }
    }
}
```

### WR-10: Public numeric entry points carry undocumented indexing preconditions

**File:** `crates/aprender-forecast/src/np.rs:160-192`, `crates/aprender-forecast/src/np.rs:196`, `crates/aprender-forecast/src/prophet.rs:207-211`, `crates/aprender-forecast/src/prophet.rs:75-77`

**Issue:** `forecast()` is the only caller that establishes the preconditions these `pub`
functions index on, and none of them documents a `# Panics` section:

- `NpData::new` — `ds_days[0]` and `ds_days[len-1]` panic on an empty slice;
  `ds_days[n_train_rows - 1]` underflows on `n_train_rows == 0`; the imputation loop reads
  `grid_y[i - 1]` at `i == 0` and scans forward with `while grid_y[b].is_nan() { b += 1 }`,
  both of which are safe only because the first and last grid cells are observed and `y` is
  finite; `obs.sort_by(|a, b| a.partial_cmp(b).expect("finite"))` panics on a `NaN`.
- `make_design` — `assert!(n >= 2 && strictly ascending)` and
  `panic!("logistic growth needs cap")` (prophet.rs:226).
- `auto_seasonalities` — `ds_days[ds_days.len() - 1]` on an empty slice.

`mase_rolling_origin.rs:260` already calls `NpData::new` directly and is one bad fixture row
away from tripping the `expect("finite")`. With `clippy::pedantic` set to `warn`
workspace-wide, `missing_panics_doc` should already be flagging these.

**Fix:** Add `# Panics` sections naming each precondition (the crate does this well elsewhere
— `dates::parse_ymd`, `bolt::Config::from_json`, `bolt::linear_fast`), or return
`Result<_, ForecastError>` from `NpData::new` so the example and any future caller cannot
skip the door's guarantees.

### WR-11: `Rng::new` collapses seeds 0 and 1 onto one stream

**File:** `crates/aprender-forecast/src/prophet.rs:628-630`, `crates/aprender-forecast/src/np.rs:30-32`

**Issue:** `Rng(seed.max(1))` — `seed` is a documented public knob ("Random seed for the
uncertainty simulation / training (default 42)", `types.rs:99-101`), so `{"seed": 0}` and
`{"seed": 1}` return bit-identical forecasts. A caller sweeping seeds from 0 gets a silent
duplicate in the first two runs. The clamp exists because xorshift64 has a fixed point at 0;
that is a reason to remap, not to alias.

**Fix:** Remap rather than clamp, so every distinct seed gives a distinct stream:

```rust
pub fn new(seed: u64) -> Self {
    // SplitMix64 finalizer: bijective, and 0 no longer maps onto 1.
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    Rng((z ^ (z >> 31)).max(1))
}
```

Note this moves every seeded number, so it must land together with a regeneration of the
band fixtures in `contracts/prophet-parity-v1.yaml`'s ladder — or be deferred and recorded.

---

_Reviewed: 2026-09-06_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
