---
phase: 06-native-time-series-forecasting-stack
verified: 2026-09-07T00:26:24Z
status: gaps_found
score: 4/5 must-haves verified
covered_files:
  - .planning/phases/06-native-time-series-forecasting-stack/06-01-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-01-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-02-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-02-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-03-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-03-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-04-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-04-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-05-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-05-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-06-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-06-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-07-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-07-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-08-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-08-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-09-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-09-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-CONTEXT.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-EVIDENCE.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-REVIEW.md
  - .planning/phases/06-native-time-series-forecasting-stack/deferred-items.md
  - CLAUDE.md
  - Cargo.toml
  - Makefile
  - README.md
  - contracts/aprender/binding.yaml
  - contracts/chronos-bolt-parity-v1.yaml
  - contracts/forecast-tool-boundary-v1.yaml
  - contracts/neuralprophet-parity-v1.yaml
  - contracts/prophet-parity-v1.yaml
  - crates/aprender-core/tests/monorepo_invariants.rs
  - crates/aprender-forecast/build.rs
  - crates/aprender-forecast/examples/mase_rolling_origin.rs
  - crates/aprender-forecast/src/bolt.rs
  - crates/aprender-forecast/src/chronos.rs
  - crates/aprender-forecast/src/dates.rs
  - crates/aprender-forecast/src/fit.rs
  - crates/aprender-forecast/src/forecast.rs
  - crates/aprender-forecast/src/np.rs
  - crates/aprender-forecast/src/prophet.rs
  - crates/aprender-forecast/src/safetensors.rs
  - crates/aprender-forecast/src/test_support.rs
  - crates/aprender-forecast/src/types.rs
  - crates/aprender-mcp-chronos/build.rs
  - crates/aprender-mcp-chronos/src/lib.rs
  - crates/aprender-mcp-chronos/src/main.rs
  - crates/aprender-mcp-forecast/src/lib.rs
  - crates/aprender-mcp-forecast/src/main.rs
  - crates/aprender-mcp-forecast/tests/e2e_stdio.rs
  - justfile
covered_digest: "v1:sha256:6abb4e3fb3323ebc20f781a296906f509322064b5c64fbf024579f91eae0cece"
behavior_unverified: 0
overrides_applied: 0
gaps:
  - truth: "SC1 — the forecast door refuses every malformed input, never a silent default"
    status: failed
    reason: >-
      MEASURED, not read off the review. `cap` is accepted and completely inert on
      every non-logistic growth arm, and the logistic controls prove the refusal
      machinery exists and is simply never reached. Seven varied probes through
      `aprender_forecast::forecast` at HEAD, printed side by side: baseline-no-cap,
      linear+cap100, bare-cap100 and linear+cap0.5 (BELOW max(y)=28.9) all returned
      yhat_max=35.8994 with a byte-identical `diagnostics` object — the cap changes
      nothing and is not echoed anywhere in the response; flat+cap100 likewise
      accepted; while logistic-no-cap refused with "logistic growth needs cap" and
      logistic+cap0.5 refused with "cap 0.5 must exceed max(y) = 28.9". Even the
      cap-sanity check is skipped, not merely the cap itself. `contracts/forecast-tool-boundary-v1.yaml`
      FALSIFY-BOUNDARY-005 `if_fails` names this exact class in its own words:
      "D-11 is broken: a knob the caller set is being ignored. This is the worst
      failure class in this contract."
    artifacts:
      - path: "crates/aprender-forecast/src/forecast.rs"
        issue: "lines 150-167 read `cap` only under `Growth::Logistic`; line 216 assigns `spec.cap = args.cap` unconditionally; the cross-model refusal loop at 113-127 covers `cap` for the neuralprophet arm only, so no cross-GROWTH check exists"
      - path: "crates/aprender-forecast/src/prophet.rs"
        issue: "make_design lines 229-233 match `(Growth::Logistic, Some(c))` and fall to `_ => None`; `spec.cap` reaches the numerics only through `cap_scaled`, so it is provably inert elsewhere"
      - path: "crates/aprender-mcp-forecast/src/lib.rs"
        issue: "the 16-case e2e refusal suite has no cross-growth cap case (`refuses_logistic_without_cap` and `refuses_logistic_cap_below_max_y` cover only the logistic direction)"
    missing:
      - "Refuse `cap` when growth != logistic, in the same shape as the neighbouring cross-model refusals: `cap is logistic-only; set growth to \"logistic\"`"
      - "Extend `forecast::tests::an_option_belonging_to_the_other_model_is_refused_not_dropped` with `{growth: linear, cap: N}` and a bare `{cap: N}`"
      - "Add one e2e refusal case in `crates/aprender-mcp-forecast/src/lib.rs` so the bar is asserted through the server, as every other SC1 refusal is"

  - truth: "SC1 — a 3 000-point daily series answers in under 2 s, and the tool-boundary constants are the HARD ceiling on the work one request can buy (contract T-06-02)"
    status: failed
    reason: >-
      MEASURED on this host at HEAD, three points, monotone in the cell count:
      a 3 000-point daily series carrying an IN-BOUNDS holiday spec (181 columns,
      84 dates = 4.56e7 design cells) took **16.113 s** — 8x SC1's 2 s bar. The
      same shape at 2 000x121x84 took 8.926 s and at 1 000x61x84 took 2.664 s, so
      the cost is linear in rows x holiday_columns x holiday_dates, exactly as
      `prophet::feature_row` (lines 178-182, a linear `.any()` scan of every
      holiday date per column per row) implies. `MAX_POINTS` (20 000),
      `MAX_HOLIDAY_COLUMNS` (1 000 -> 731 from a single +-365 window) and
      `MAX_HOLIDAY_DATES` (1 000) are each checked in isolation and their PRODUCT
      is not: `types::tests::cost_bounds_match_contract` mirrors the four factors
      to the contract one at a time and asserts nothing about the product; a repo-wide
      grep for `DESIGN_COST|design_cost|holiday_design` finds nothing in either the
      source or the contract. Extrapolating the measured ~0.35 us/cell to the
      in-bounds maximum (1.46e10 cells) gives ~85 minutes for ONE request, before
      `pooled_app`'s default K=8 multiplies it. `FIT_BUDGET_SECS` cannot see any of
      this: `make_design` runs before `fit_prophet` is entered. The contract line
      "these bounds are the HARD ceiling on the work one request can buy (T-06-02)"
      (forecast-tool-boundary-v1.yaml:102) is therefore false as shipped.
    artifacts:
      - path: "crates/aprender-forecast/src/prophet.rs"
        issue: "feature_row lines 178-182 scan every date of every holiday for every column on every row; make_design (247-249) and predict (686-689) both call it per row"
      - path: "crates/aprender-forecast/src/types.rs"
        issue: "lines 17-49 bound each factor; the comment at :45-49 states the multiplier risk in words and then bounds only the multiplicand"
      - path: "contracts/forecast-tool-boundary-v1.yaml"
        issue: "line 102 asserts a HARD work ceiling that no constant enforces"
    missing:
      - "A `MAX_HOLIDAY_DESIGN_COST` bound on rows x holiday_columns x total_holiday_dates, added to `constants:` in the contract and asserted equal by `cost_bounds_match_contract` as the other four bounds are"
      - "Replace the per-row linear `.any()` with a prebuilt `HashSet<i64>` per holiday in `make_design`/`predict` — membership is membership, so no parity rung moves"
      - "One e2e case proving an over-cost request is refused (the bar must be shown to turn RED, not just added)"

  - truth: "SC1 — the response's yhat_lower / yhat_upper are the bands they claim to be"
    status: failed
    reason: >-
      Confirmed by reading `prophet.rs:794-805` directly. The logistic uncertainty
      path draws its simulated changepoint count with Knuth's product method, whose
      `l = (-lambda).exp()` is exactly 0.0 for lambda > 745.13. Past that the loop
      exits only when `pp` itself underflows, so `n_changes` becomes a function of
      f64 subnormal exhaustion rather than of the Poisson law. The regime is reachable
      in-bounds: 100 consecutive daily points (span 99, n_changepoints 25) with
      `horizon: 3650, growth: "logistic"` gives t_max ~= 37.9 and lambda ~= 922; the
      10-point minimum with the same horizon gives lambda ~= 2 839. Python Prophet
      calls `np.random.poisson`, which is correct at any lambda. `wp_log_R_logistic`
      is the only logistic fixture and exercises no such horizon ratio, so no parity
      rung sees it — I confirmed the parity ladder passes (32 tests) while this path
      is wrong. Bands come back too narrow, with no warning and nothing in `diagnostics`.
    artifacts:
      - path: "crates/aprender-forecast/src/prophet.rs"
        issue: "lines 794-805: Knuth Poisson with no branch for lambda above the exp-underflow threshold"
    missing:
      - "A normal-approximation (or transformed-rejection) branch above lambda ~= 30, or an explicit refusal/clamp rather than silent degradation"
      - "A falsification test asserting the sampler's mean at lambda = 900 is within a few percent of 900 — today the equivalent saturates near 745 regardless of lambda"
deferred: []
human_verification:
  - test: "Start both servers (`cargo run -p aprender-mcp-forecast -- --http 8080`, `cargo run -p aprender-mcp-chronos -- --http 8081`) and open each demo page; drive initialize -> tools/list -> tools/call and confirm a forecast is charted."
    expected: "Both pages complete the MCP handshake same-origin under /mcp and render a band chart."
    why_human: "06-08 declared this a human end-of-phase check (visual rendering + a real browser MCP client). No automated test covers the pages."
  - test: "Run the Chronos parity ladder on an x86_64 host: `CHRONOS_MODEL_DIR=... cargo test -p aprender-forecast --lib bolt::parity chronos::parity -- --nocapture`, read the printed max|delta|, and tighten `quantiles_abs_f32_nonaarch64` in contracts/chronos-bolt-parity-v1.yaml to that measurement in a `pv diff`-visible edit."
    expected: "A measured max|delta| replaces the PROVISIONAL 5.0e-6 headroom value."
    why_human: "The bar is unmeasured by construction — this box is aarch64 and every CI runner is X64 with no Chronos leg (measure-x86-first). Recorded as D-ITEM-06-04 / REVIEW-06-02, and confirmed still provisional at HEAD; it is carried openly, not silently."
  - test: "Decide whether SC4's Chronos parity ladder staying DARK in CI is acceptable for the milestone, or whether the ci.yml embedded-weights leg should now be applied."
    expected: "An explicit decision recorded against D-ITEM-06-03."
    why_human: "The `measure-x86-first` CI decision is correctly applied (ci.yml carries zero `chronos`/`forecast` mentions, verified) and the gap is named as an open item, but it means every SC4 parity claim rests on a manual local gate."
---

# Phase 6: Native Time-Series Forecasting Stack — Verification Report

**Phase Goal:** Users can forecast a time series in one stateless MCP call — `ds[]`, `y[]`, horizon in; forecast with bands and components out — from pure-Rust Prophet and NeuralProphet ports and an embedded zero-shot Chronos-Bolt, each proven to parity with its Python original and served the way SetFit is served (thin pmcp servers, stdio + streamable-HTTP, Lambda-shaped).
**Verified:** 2026-09-07T00:26:24Z (HEAD `ce3e5a8ea`, branch `gsd/phase-2-contract-gate`, host `aarch64-apple-darwin`)
**Status:** gaps_found
**Re-verification:** No — initial verification

## Goal Achievement

The engineering here is unusually solid and the phase's own honesty machinery is real, not
decorative: I re-ran four of its two-sided controls myself and every one turned RED on demand.
The goal fails on one Success Criterion — SC1's door — and it fails on two independently
measured grounds, not on a reviewer's opinion.

### Observable Truths

| # | Truth (ROADMAP Success Criterion) | Status | Evidence |
|---|---|---|---|
| SC1 | One stateless `forecast` tool over stdio + streamable-HTTP; full response shape; **under 2 s** for a 3 000-point daily series; **every malformed input a validation error, never a silent default** | ✗ FAILED | Transport, shape and the 16 enumerated refusals all VERIFIED by run. But two measured falsifications: (a) `cap` is accepted and provably inert on every non-logistic arm — 5 accept probes vs 2 refusing logistic controls, byte-identical diagnostics; (b) an in-bounds 3 000-point daily request with an in-bounds holiday spec took **16.113 s**, 8x the bar, because the design cost is unbounded in the product of three separately-bounded factors |
| SC2 | Prophet port reproduces Python Prophet 1.4.0 on 4 named fixtures, seven rungs, as tests that run in CI | ✓ VERIFIED | `cargo test -p aprender-forecast --lib` → **86 passed, 0 failed**; 32 `prophet::parity` tests across 7 fixtures covering all seven rung types; every tolerance read from `contracts/prophet-parity-v1.yaml` via `equation_tolerance` (11 call sites, no literals). One documented deviation, below |
| SC3 | NeuralProphet lag-free 365-day holdout MAE ≤ 0.47; AR-Net (`n_lags=30`) beats naive one-step; graph-connected Huber; data prep matches the oracle | ✓ VERIFIED | Same run: `np::parity::lag_free_365_day_mae_within_contract`, `ar_net_30_lags_beats_naive_one_step`, `weighted_huber_is_graph_connected`, `data_prep_matches_np_oracle`, `train_clears_the_tape`, `full_batch_is_not_what_auto_batch_returns` — 9/9 pass, bars read from `neuralprophet-parity-v1.yaml` |
| SC4 | Chronos-Bolt-tiny f16 embedded; 9 quantiles match the 2.3.1 oracle (1e-6 f32 / 2 % f16) **through the server**; 2 048-ctx < 100 ms; `horizon > 64` gated + warned; binary < 30 MB; cold start < 150 ms | ✓ VERIFIED | Armed with real weights: `aprender-forecast --lib bolt:: chronos::` → **24 passed, 0 failed, 0 ignored**; `aprender-mcp-chronos --lib` → **9 passed, 0 failed** incl. `forecast_tool_over_streamable_http_matches_oracle` and `f16_weights_within_two_percent_of_std_through_the_server`. Binary size **independently re-measured at HEAD: 24 671 472 bytes**, byte-identical to 06-EVIDENCE.md. Bench 18.0 ms and cold start 53 ms accepted from EVIDENCE's recorded commands + logs |
| SC5 | 8 concurrent requests bit-identical to sequential in < half the wall; every gate green | ✓ VERIFIED | `aprender-mcp-forecast --lib` → **28 passed, 0 failed** incl. `eight_concurrent_requests_are_bit_identical_to_sequential` and `sixteen_concurrent_neuralprophet_fits_stay_identical`; stdio integration target passes; `cargo fmt --all -- --check` rc=0; `pv validate` rc=0 on all four contracts; clippy green on the three new crates; ratio 5.146/5.150/5.162 (≥ 2.0 bar) from `forecast-pool-ratio`, whose recipe I read and which asserts hard and captures `rc=$?` without a pipe |

**Score:** 4/5 truths verified (0 present, behavior-unverified)

### Two-sided controls I re-ran myself (not taken from any SUMMARY)

The phase repeatedly claims a gate "can turn RED". Four of those claims were re-tested here.
All four held.

| Control | Green side | Red side |
|---|---|---|
| `just fetch-chronos-tiny` verify-always (REVIEW-06-03) | rc=0 with all files already present; re-hashed f32 model, config AND derived f16 against their pins without downloading | Copied the dir, flipped the last byte of `model.safetensors`, ran `just chronos_dir=<copy> fetch-chronos-tiny` → **rc=1**, `FAIL: sha256 mismatch — a supply-chain event, not a cache miss`, naming the file and both hashes |
| D-18 counted-skip discipline | Armed (`CHRONOS_MODEL_DIR` set): the same 7 tests run and pass, **0 ignored** | Unarmed: `7 ignored`, each printing `CHRONOS_MODEL_DIR unset or has no model.safetensors` — a counted skip, never a println-and-return |
| `make contract-audit-phase6` source resolution (REVIEW-06-U3) | rc=0; 55 Phase 6 binding rows resolved to definition sites; 0 BIND- findings across all four contracts | Pointed one row's `function:` at `totally_invented_function_xyz` → **rc=2**, `RESOLVE- prophet-parity-v1.yaml data_prep_exact ... (no definition site in .../prophet.rs)`, `FAIL: 1 Phase 6 binding row(s) name a function with no definition site`. Reverted; tree clean |
| SC5 clippy scoping (D-ITEM-06-01-b) | `clippy -p <crate> --all-targets --no-deps -- -D warnings` → rc=0, 0 errors, on all three new crates | Untouched sibling control `clippy -p aprender-mcp-setfit --all-targets -- -D warnings` → rc=101, the same 19 errors, **all in `crates/aprender-compute`**. The workspace-wide redness is pre-existing and crate-external, exactly as D-ITEM-06-09 states |

### Required Artifacts

All 29 declared artifacts exist and are substantive (no stub is anywhere near the
`min_lines` floors). Line counts: `prophet.rs` 1774, `bolt.rs` 1642, `mcp-forecast/lib.rs` 1235,
`np.rs` 1189, `mcp-chronos/lib.rs` 938, `chronos.rs` 727, `forecast.rs` 704, `mase_rolling_origin.rs` 541,
plus the four contracts (399/355/536/523), both `static/index.html` pages (65/59), all three
READMEs added by 06-02, `e2e_stdio.rs` (229), both `build.rs` files, `dates.rs`, `fit.rs`,
`safetensors.rs`, `test_support.rs`, `types.rs`, the committed `chronos_bolt_tiny_config.json`.

### Key Link Verification

All 33 declared key links verified by their own declared patterns. Highlights:

| From | To | Via | Status |
|---|---|---|---|
| `aprender-mcp-forecast/src/lib.rs` | `aprender-forecast/src/forecast.rs` | `spawn_blocking(move \|\| aprender_forecast::forecast` | ✓ WIRED |
| `aprender-mcp-chronos/src/lib.rs` | `aprender-forecast/src/chronos.rs` | `chronos::forecast(&model, &args)` | ✓ WIRED |
| `forecast.rs` | `fit.rs` | `fit_prophet(&design, 8)` | ✓ WIRED |
| `fit.rs` | core L-BFGS | `LbfgsF64::new(MAX_ITERS_PER_ROUND, 1e-7, 20)` | ✓ WIRED |
| `bolt.rs` | `aprender-compute` BLIS | `gemm_blis(` ×3, with `single_row_routing_is_dot8_at_production_defaults` and `multi_row_routing_is_gemm_blis_at_production_defaults` both PASSING armed — D-14 exclusive routing is proven, not grepped | ✓ WIRED |
| `types.rs` | `forecast-tool-boundary-v1.yaml` | `constant_u64("forecast-tool-boundary-v1"` ×7 | ✓ WIRED |
| `prophet.rs` / `np.rs` | their parity contracts | `equation_tolerance(...)` ×11 / ×4 — no duplicated literals | ✓ WIRED |
| `mcp-chronos/build.rs` | `mcp-chronos/src/lib.rs` | `include_bytes!(concat!(env!("OUT_DIR")` ×2 | ✓ WIRED |
| `justfile` | `models/chronos-bolt-tiny/` | `hf_hub_download` at the pinned rev + sha256 | ✓ WIRED (and proven red) |
| `Makefile` | all four contracts | `PHASE6_CONTRACTS` ×6 + `$(CONTRACTS)` | ✓ WIRED |
| `Cargo.toml` | `aprender-forecast` | `[profile.dev.package.aprender-forecast] opt-level = 3` | ✓ WIRED |
| `monorepo_invariants.rs` | both server crates | `deployment_unit_bins` allowlist names both by name with the D-06 reason | ✓ WIRED |

### Data-Flow Trace (Level 4)

| Artifact | Value | Source | Real data | Status |
|---|---|---|---|---|
| `mcp-forecast` `forecast` tool | `yhat/lower/upper/trend/components` | `aprender_forecast::forecast` → `make_design` → `fit_prophet` (L-BFGS) → `predict` | Yes — the e2e happy paths assert real numbers, and the pool-equality test compares two independently computed responses | ✓ FLOWING |
| `mcp-chronos` `forecast` tool | 9 quantiles | `chronos::forecast` → `Bolt::predict` over `EMBEDDED_WEIGHTS`/`CHRONOS_MODEL_DIR` | Yes — matched against the Python 2.3.1 oracle through the HTTP server | ✓ FLOWING |
| Prophet/NP/Chronos tolerances | every assertion bar | `test_support::equation_tolerance` / `constant_u64` reading the YAML at test time | Yes — 22 call sites; the audit resolves all 55 binding rows to real definition sites | ✓ FLOWING |
| `README.md` counts | 86 crates / 1790 contracts | `cargo metadata` / `find contracts -name '*.yaml'` | Yes — I re-derived both **right now** and got exactly 86 and 1790 | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Prophet + NP parity ladders (SC2, SC3) | `cargo test -p aprender-forecast --lib` | 86 passed, 0 failed, 7 ignored (unarmed Chronos), rc=0 | ✓ PASS |
| Chronos parity, armed (SC4) | `CHRONOS_MODEL_DIR=.../f32 cargo test -p aprender-forecast --lib -- bolt:: chronos::` | 24 passed, 0 failed, **0 ignored**, rc=0 | ✓ PASS |
| Fit-server e2e + pool equality (SC1, SC5) | `cargo test -p aprender-mcp-forecast --lib` | 28 passed, 0 failed, rc=0 | ✓ PASS |
| Chronos server e2e, armed (SC4) | `CHRONOS_MODEL_DIR=.../f32 cargo test -p aprender-mcp-chronos --lib` | 9 passed, 0 failed, rc=0 | ✓ PASS |
| Spawned-binary stdio round trip (SC1 stdio) | `cargo test -p aprender-mcp-forecast --test e2e_stdio` | 1 passed, rc=0 | ✓ PASS |
| `cap` on non-logistic growth (SC1) | 7-variant probe through `aprender_forecast::forecast` | 5 ACCEPTED with identical output, 2 logistic controls REFUSED | ✗ **FAIL** |
| Holiday design cost (SC1 2 s bar / T-06-02) | 3-point scaling probe through the same door | 2.664 s / 8.926 s / **16.113 s** at 5.1e6 / 2.0e7 / 4.6e7 cells | ✗ **FAIL** |
| `cargo fmt --all -- --check` (SC5) | as written | rc=0 | ✓ PASS |
| clippy on the three new crates (SC5) | `--all-targets --no-deps -- -D warnings` | rc=0, 0 errors, each crate | ✓ PASS |
| SC4 binary size, re-measured at HEAD | `just chronos-embed-build` | 24 671 472 bytes < 30 000 000, rc=0 | ✓ PASS |

### Probe Execution

| Probe | Command | Result | Status |
|---|---|---|---|
| Weight integrity, verify-always | `just fetch-chronos-tiny` | rc=0, three pins re-hashed without download | PASS |
| Weight integrity, tamper control | `just chronos_dir=<flipped copy> fetch-chronos-tiny` | rc=1, sha256 mismatch named | PASS (red side proven) |
| Contract validation ×4 | `pv validate contracts/{forecast-tool-boundary,prophet-parity,neuralprophet-parity,chronos-bolt-parity}-v1.yaml` | rc=0, `0 error(s), 0 warning(s)` each | PASS |
| Contract counts | `pv status` ×4 | 16/16/16, 12/12/12, 9/10/10, 18/18/18 equations/obligations/falsification tests — all non-zero | PASS |
| Phase 6 binding audit | `make contract-audit-phase6` | rc=0, 55 rows resolved, 0 BIND- | PASS |
| Binding audit, negative control | one row → invented function | rc=2, `RESOLVE-` finding, reverted | PASS (red side proven) |

### Requirements Coverage

Confirmed as instructed: `.planning/REQUIREMENTS.md` carries **no forecasting REQ-IDs** — the
document is the SetFit milestone's. ROADMAP.md's Phase 6 section says so in a
`**Requirements note**` and points at the five `prophet-forecast-mcp` decisions in
`.planning/spikes/MANIFEST.md`, transcribed as D-01..D-18 in `06-CONTEXT.md`. **This is not a
traceability gap** and is not reported as one. Verification ran against SC1..SC5 and the
D-numbered decisions. No orphaned requirement IDs map to Phase 6.

### D-numbered decision spot-checks

| Decision | Status | Evidence |
|---|---|---|
| D-01 stateless, no fit→artifact→forecast round-trip | ✓ | The tool closure calls `forecast` directly inside `spawn_blocking`; no artifact type exists |
| D-06 thin-server template, stdio + HTTP | ✓ | `run_stdio` in both `main.rs`; `monorepo_invariants` allowlist names both crates with the D-06 reason |
| D-07 Realizar-first exception row | ✓ | CLAUDE.md:201 carries the Forecasting row and :221 the Chronos paragraph; `readme_contract` (path gate) green per the pre-established run |
| D-11 the door refuses, never defaults | ✗ | See gap 1 — `cap` is the exception the contract's own `if_fails` text calls the worst failure class |
| D-12 router pool | ✓ | `pooled_app` + round-robin `fetch_add(1, Ordering::Relaxed) % routers.len()`; `--pool` default 8 with a MAX_POOL ceiling; equality asserted, ratio host-gated |
| D-13 memory clause, AMENDED | ✓ | Both layouts ship; `weight_layout_is_dual_and_its_cost_is_stated` passes armed; the amendment is recorded as D-ITEM-06-08 with its D-13/D-14 conflict and the ratifying human checkpoint |
| D-14 `gemm_blis` multi-row / `dot8` single-row | ✓ | Both routing tests pass armed at production defaults — exclusive routing proven, not grepped |
| D-15 tolerances live in contracts | ✓ | 22 `equation_tolerance`/`constant_u64` call sites; the audit resolves every binding row |
| D-16 EMPIRICAL coverage, never nominal 80 % | ✓ | Stated in both tool descriptions and READMEs; the MASE harness ships as a compiled example |
| D-17 no calendar dependency | ✓ | `dates.rs` civil-date arithmetic only; `future_days` covers D/W/MS and refuses H |
| D-18 counted skips + never-committed weights | ✓ | Two-sided skip proof above; `just fetch-chronos-tiny` verify-always proven red; only the 1.1 KB config.json is committed |
| CI decision `measure-x86-first` | ✓ | `.github/workflows/ci.yml` carries **zero** `chronos`/`forecast` mentions; last commit touching it (`2196de281`) predates the phase; the gap is named as D-ITEM-06-03 and the stdio target's darkness as D-ITEM-06-06 |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| — | — | `TBD` / `FIXME` / `XXX` / `TODO` / `HACK` across all Phase 6 source | none found | Debt-marker gate clean |
| `crates/aprender-forecast/src/forecast.rs` | 150-167, 216 | Accepted-but-inert option (`cap` off the logistic arm) | 🛑 Blocker | Gap 1 |
| `crates/aprender-forecast/src/prophet.rs` | 178-182, 247-249 | Unbounded product of separately-bounded factors + per-row linear scan | 🛑 Blocker | Gap 2 |
| `crates/aprender-forecast/src/prophet.rs` | 794-805 | Numerically invalid sampler outside its domain, degrading silently | 🛑 Blocker | Gap 3 |

### Documented deviations accepted (not gaps)

1. **SC2's `objective at Python's MAP` rung binds 3 of 7 fixtures, not 4 of 4.** I checked the
   fixture keys myself: `wp_log_R_logistic_prophet140.json`, `peyton_default`, `peyton_holidays`
   and `air_multiplicative` publish no `log_posterior_at_map_unnormalized`; only the three
   spike-001 fixtures do. A seventh test would have compared Rust to Rust. The asymmetry is
   stated in three places — `06-03-SUMMARY.md` Deviation 1, the contract's top description
   (lines 21-27) and `objective_at_python_map_abs.invariants` — and those fixtures are held by
   `fitted_objective_slack` plus the full `predict_path_rel_yscale` chain, which DOES bind all
   seven. Judged sound; the SC2 wording is what should move, not the tests. Consider an override.
2. **SC5's clippy clause is met at `--no-deps`.** With deps, `-D warnings` reaches
   `crates/aprender-compute` and fails — I confirmed by running the untouched-sibling control,
   which fails identically. SC5's text is already scoped ("on the new crates") and D-ITEM-06-09
   restates the boundary. Not a gap; the `aprender-compute` cleanup is a real ticket for someone.
3. **06-EVIDENCE.md's commit provenance is honest about a dirty tree.** It records that the
   measurements ran at `adc8a560a` **plus** an uncommitted 59 848-byte delta, with the delta's
   sha256 — and that delta was later committed as `1d170f383` with per-file attribution. The only
   Phase 6 source change after it is `mase_rolling_origin.rs`, which is not in any measured
   binary. I re-measured the binary-size bar at HEAD and got the identical 24 671 472 bytes,
   which independently corroborates the table.
4. **`quantiles_abs_f32_nonaarch64 = 5.0e-6` is PROVISIONAL and unmeasured** — carried openly in
   the contract description, `FALSIFY-CHRONOS-002`, D-ITEM-06-04 and the 06-05/06-07 truths.
   Recorded, not silently carried. Routed to human verification.
5. **`cargo test -p apr-cli --test cli_commands` FALSIFY-CLI-006 failure is pre-existing phase-02
   work** and is explicitly NOT attributed to Phase 6, per the established finding.

### Gaps Summary

Nine plans, 29 artifacts, 33 key links, four contracts and 55 binding rows: everything the
phase said it built, it built, and the parts that can be proven mechanically are proven with
controls that I confirmed fire in both directions. SC2, SC3, SC4 and SC5 hold on evidence I
generated in this session, including a byte-exact reproduction of the SC4 binary-size bar.

SC1 does not hold, and the failure is not marginal. The validation door — the single surface
this phase's own contract calls the place where "the worst failure class" lives — has two
measured holes and one confirmed numerical hole:

- A caller who sets `cap` on the default (linear) arm gets a straight-line forecast, no
  saturating curve, and a response whose diagnostics do not mention `cap` at all. The same
  request on the logistic arm is refused twice over, which proves the refusal machinery is
  present and simply not reached. Five accepting probes returned output byte-identical to the
  no-cap baseline.
- A 3 000-point daily request carrying an in-bounds holiday spec takes 16 s against a 2 s bar,
  because three bounds are checked one at a time and their product — the actual work — is not.
  At the in-bounds maximum the same arithmetic gives ~85 minutes for one request, times the
  default pool of 8. `FIT_BUDGET_SECS` is structurally blind to it: the cost is spent before
  `fit_prophet` is entered. The contract line claiming a HARD ceiling is false as shipped.
- Long-horizon logistic requests get bands computed from a Poisson sampler operating outside
  its numerical domain, so they come back too narrow with no warning.

All three are small, local, well-understood fixes with a clear place to assert them (the
existing refusal e2e suite and the existing `constants:` block), and none of them touch a
parity rung. Fix the door, add the two red-side cases, and this phase is done.

---

_Verified: 2026-09-07T00:26:24Z_
_Verifier: Claude (gsd-verifier)_
