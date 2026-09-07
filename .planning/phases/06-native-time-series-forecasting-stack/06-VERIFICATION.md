---
phase: 06-native-time-series-forecasting-stack
verified: 2026-09-07T20:12:35Z
status: human_needed
score: 5/5 must-haves verified
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
  - .planning/phases/06-native-time-series-forecasting-stack/06-10-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-10-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-11-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-11-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-12-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-12-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-13-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-13-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-14-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-14-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-15-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-15-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-16-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-16-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-17-PLAN.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-17-SUMMARY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-CONTEXT.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-EVIDENCE.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-REVIEW.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-REVIEWS.md
  - .planning/phases/06-native-time-series-forecasting-stack/COVERAGE.md
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
  - crates/aprender-forecast/README.md
  - crates/aprender-forecast/build.rs
  - crates/aprender-forecast/examples/mase_rolling_origin.rs
  - crates/aprender-forecast/src/bolt.rs
  - crates/aprender-forecast/src/chronos.rs
  - crates/aprender-forecast/src/dates.rs
  - crates/aprender-forecast/src/fit.rs
  - crates/aprender-forecast/src/forecast.rs
  - crates/aprender-forecast/src/lib.rs
  - crates/aprender-forecast/src/np.rs
  - crates/aprender-forecast/src/prophet.rs
  - crates/aprender-forecast/src/safetensors.rs
  - crates/aprender-forecast/src/sc1_wall.rs
  - crates/aprender-forecast/src/test_support.rs
  - crates/aprender-forecast/src/types.rs
  - crates/aprender-mcp-chronos/build.rs
  - crates/aprender-mcp-chronos/src/lib.rs
  - crates/aprender-mcp-chronos/src/main.rs
  - crates/aprender-mcp-forecast/src/lib.rs
  - crates/aprender-mcp-forecast/src/main.rs
  - crates/aprender-mcp-forecast/tests/e2e_stdio.rs
  - justfile
  - scripts/assert_measurement_under.sh
  - scripts/check_assert_measurement_under_cases.sh
covered_digest: "v1:sha256:cda6709ca3d127e040555b5035f029fa808190ad3062d03fcb89be717fedd933"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 4/5
  gaps_closed:
    - "SC1 — the forecast door refuses every malformed input, never a silent default (`cap` inert off the logistic arm)"
    - "SC1 — a 3 000-point daily series answers in under 2 s, and the tool-boundary constants are the HARD ceiling on the work one request can buy"
    - "SC1 — the response's yhat_lower / yhat_upper are the bands they claim to be (Poisson sampler outside its numerical domain)"
  gaps_remaining: []
  regressions: []
gaps: []
deferred: []
advisory:
  - finding: "WR-01 — the post-loop `holiday_dates_total` refusal (forecast.rs:289-295) is unreachable dead code. The in-loop check at :242 returns Err the instant the running sum crosses, and the loop body has no `continue`, so no caller ever receives the EXACT-total message. Three artifacts state that it fires: the code comment at :283-288, `door_surface.knobs.dates.enforced_by` and `cost_axes` C-03. 06-15's own truth 'the caller still learns the exact total' is false as shipped."
    category: architectural
    reason: "Independently reproduced by reading forecast.rs:225-296 — the control flow is unambiguous. Not a correctness hole (the in-loop refusal fires and its message honestly says 'a running total, not the request's total'), so it does not block SC1. Resolved by deleting the dead block and correcting the two contract claims, or by hoisting the O(1) `dates.len()` sum above the loop so the exact-total message becomes reachable."
    evidence_status: "reproduced by verifier (source read)"
  - finding: "WR-02 — C-07's `no_structural_maximum: true` disposition rests on a false premise. Four artifacts (types.rs:188-194, forecast-tool-boundary-v1.yaml:303/:401/:457, binding.yaml notes) state that neither transport caps the request body. `http_app` (aprender-mcp-forecast/src/lib.rs:84) builds `StreamableHttpServerConfig::stateless()`, which in the LOCKED pmcp 2.19.3 sets `max_request_bytes: DEFAULT_MAX_REQUEST_BYTES` = 4 MiB, enforced before JSON parsing by `read_body_with_limit(body, state.config.max_request_bytes)`."
    category: architectural
    reason: "Independently reproduced: Cargo.lock pins pmcp 2.19.3; pmcp-2.19.3/src/server/streamable_http_server.rs:471 sets the field in `stateless()`, limits.rs:46 defines the 4 MiB default, and :4571 enforces it. The HTTP transport therefore DOES have a structural maximum. Enforcement is unaffected — C-07 was closed with MAX_HOLIDAY_NAME_LEN = 200, the more conservative of the two dispositions — so this is a false justification, not a missing bound. The stdio half of the claim is unchecked by me."
    evidence_status: "reproduced by verifier (dependency source read)"
  - finding: "CR-01 — `MAX_NP_TRAIN_COST = 15_000_000` refuses well-formed in-spec NeuralProphet requests. At the advertised maximum history (`fit_max_points` = 20 000 daily points) the reachable `n_lags` range is 0..=6, against an advertised and range-checked `n_lags <= 365`."
    category: other
    reason: "Arithmetic independently reproduced from the shipped pricing functions, not taken from the review: auto_epochs(20 000) = 50, n_samples = 20 000 - 7 = 19 993, sweep width 2 → 2 x 50 x 19 993 x 8 = 15 994 400, which is 6.63 % over the bound — the review's figure to the digit. The nearest measured neighbour recorded in MAX_NP_TRAIN_COST's own doc (20 000 points, n_lags 6, cost 13 995 800) walls at 1.642 s, and the rejected 20 M candidate measured 2.089 s, so the 2 s crossing sits near 19-20 M and the shipped bound discards roughly a quarter of the region SC1's bar would allow. It does NOT falsify an SC: SC1's timing bar is a 3 000-point daily series (measured 0.212 s here) and SC3's stated `n_lags = 30` Peyton geometry prices at 14 552 640 and is accepted — but only by 3.0 %. Resolution is a human decision (raise to ~18 M with a measured wall, or narrow the advertised range and make the refusal message name the reachable n_lags for the history sent); see human_verification."
    evidence_status: "arithmetic reproduced by verifier; the 2 s wall at the refused point NOT independently measured"
  - finding: "WR-03/WR-04 — the class invariant's two halves are not equally strong, and the contract prose does not say so. The KNOBS half is structural (`schema_knobs()` derives from `schemars::schema_for!` and `every_request_knob_is_enumerated` asserts set equality in both directions). The COST-AXES half is not: `enumerated_axes()` reads `door_surface.cost_axes` out of the YAML and nothing else, so an axis that is NOT LISTED is invisible to both axis tests. Separately, `schema_knobs()` feeds only `ForecastArgs` and `HolidayArg`, so the Chronos door (`ChronosArgs`, five caller-settable fields, four `chronos_*` constants in the same file) is outside the enumeration while the block header reads 'THE DOOR'S WHOLE SURFACE'."
    category: architectural
    reason: "Reproduced by reading types.rs:520-726 and the contract's `door_surface_is_complete.formula`, which is honest (`forall a in cost_axes: ...`, a per-entry predicate, never a completeness claim). Both plans' must_have truths are met AS WRITTEN — 06-14 says the enumeration is 'by INSPECTION' and scopes the knobs to ForecastArgs + HolidayArg. What overclaims is the surrounding prose. Also confirmed: the contract's line that MISSING 'is how CR-01's cost axis got in' is wrong — C-06 is a function of `growth`, `freq`, `horizon` and ds spacing, four knobs that all had entries."
    evidence_status: "reproduced by verifier (source + contract read)"
  - finding: "WR-06 — `scripts/assert_measurement_under.sh`'s header calls itself 'the ONE numeric bar check every wall-clock gate in this repository calls'; `just chronos-coldstart` (justfile:656) asserts its 150 ms SC4 bar with `[ \"$med\" -ge 150 ]` and does not call it. `justfile:740` still runs `awk -v a=... 'BEGIN { exit (a + 0 > b + 0) ? 0 : 1 }'` as the best-of-three selector."
    category: other
    reason: "Reproduced by reading the justfile. 06-16's must_have is met as written (no SC1/SC4/SC5 BAR reads a measurement through the `awk -v v=... < BAR` coercion — the pool bar at :751 goes through the validator; :740 is a selector). Both remaining sites fail closed today (`chronos-coldstart`'s sed emits digits only and `[ -ge ]` errors otherwise; the validator refuses `1.2.3` downstream of :740). The defect is the script header's claim, which is false as written."
    evidence_status: "reproduced by verifier (justfile read); case table re-run green, 23/23"
  - finding: "WR-08 — `just forecast-sc1-sweep` pins `points = 33` in both the harness default and the recipe, so the gate sweeps freq x growth x holiday-shape at one history size. The two slowest holiday compositions on record (the 800 x 50 `forecast-holiday-bench` default at 1.692 s, and the accepted 4 700-point / 5-column request `types.rs:79` records at 4.2 s) are outside the swept matrix."
    category: other
    reason: "Reproduced: I ran the gate and every Prophet row printed `points=33`; the slowest was 0.713 s. 06-16's must_have specifies 'at the tightest legal history span', so the matrix is what was planned — the tightest span maximises the lambda axis and does not maximise the design-cost axis. Adding `points` as a gate axis is the fix the review proposes and it is not this round's stated scope."
    evidence_status: "reproduced by verifier (gate run, 19 compositions, /tmp/sc1sweep.log)"
  - finding: "WR-09 — `crates/aprender-forecast/README.md:99-113` describes `prophet::feature_row`'s signature change and `safetensors::load`'s removal as 'breaking for external callers of this crate' in 'the 0.63.0 line'. `crates/aprender-forecast/Cargo.toml:15` is `publish = false` ('not a published API yet') and the crate was created in this phase. The same premise appears in `bolt.rs`'s `transpose` comment as the reason it was not widened to `Result`."
    category: other
    reason: "Reproduced by reading the manifest and the README. 06-17's must_have IN-04 truth ('the removal of safetensors::load from a PUBLISHED crate's public surface') rests on a false premise; the note artifact exists but describes a semver relationship that cannot exist. Cosmetic in effect, misleading for a future maintainer."
    evidence_status: "reproduced by verifier (manifest + README read)"
  - finding: "IN-01 — `prophet::design_cost::holiday_design_wall`'s in-code defaults (3000/181/84/365) price at 609 065 design cells against `MAX_HOLIDAY_DESIGN_COST` = 50 000, so the door refuses them and `sc1_wall::time_accepted` panics. Only `just forecast-holiday-bench` (defaults 800/50/84/200) is a working entry point, and the justfile says so at :810-811."
    category: other
    reason: "Reproduced by reading prophet.rs:2216-2222 and justfile:808-820. 06-16's must_have — '`forecast-holiday-bench` keeps working as a single-composition entry point onto the same code' — is TRUE; the documented `cargo test --release --ignored` invocation is what breaks. Point the code defaults at the recipe's values."
    evidence_status: "reproduced by verifier (source read)"
  - finding: "IN-02 — two of the three dispositions `every_cost_axis_names_a_real_bound` validates are reached by no shipped row. All 14 `cost_axes` entries name a real `constants:` key; none carries `bound: measured_at_structural_maximum` (C-08 carries `measured_seconds: 47.924` but its `bound:` is `fit_max_np_train_cost`) and none carries an `unbounded_pending_*` marker. IN-04 — the sweep prints `lambda=86789.8` on `growth=linear` rows that never call `poisson`, 4.3x `MAX_LOGISTIC_CHANGEPOINT_LAMBDA`, on lines the gate reports as OK."
    category: other
    reason: "Both reproduced — the axis dispositions by reading the 14 entries at contracts/forecast-tool-boundary-v1.yaml:251-355, the lambda print by reading the gate's own output. Neither affects enforcement."
    evidence_status: "reproduced by verifier (contract read; gate run)"
  - finding: "PRE-EXISTING AND SELF-RECORDED — the 2 s bar holds for SC1's stated shape and for every swept composition, not for every accepted request. `types.rs:76-81` records, in the phase's own words, that 'a 4 700-point, 5-column request is only 25 000 cells — half this bound — and still walls at 4.2 s, reproducibly', because the fit's iteration count is data-dependent and no payload statistic predicts it."
    category: architectural
    reason: "Not a new finding and not hidden: the phase wrote it into the constant's doc comment. SC1's literal criterion is a 3 000-point daily series, measured here at 0.212 s, and `MAX_HOLIDAY_DESIGN_COST` bounds the per-iteration arithmetic rather than the wall. Recorded so a later reader does not read '19/19 under 2 s' as a universal guarantee."
    evidence_status: "self-recorded in the tree; the 4.2 s wall NOT independently reproduced by verifier"
behavior_unverified_items: []
coincidental_reliance_items: []
human_verification:
  - test: "Start both servers (`cargo run -p aprender-mcp-forecast -- --http 8080`, `cargo run -p aprender-mcp-chronos -- --http 8081`) and open each demo page; drive initialize -> tools/list -> tools/call and confirm a forecast is charted."
    expected: "Both pages complete the MCP handshake same-origin under /mcp and render a band chart."
    why_human: "CARRIED FORWARD, still open. 06-08 declared this a human end-of-phase check (visual rendering + a real browser MCP client). No automated test covers the pages; confirmed again at HEAD."
  - test: "Run the Chronos parity ladder on an x86_64 host: `CHRONOS_MODEL_DIR=... cargo test -p aprender-forecast --lib bolt::parity chronos::parity -- --nocapture`, read the printed max|delta|, and tighten `quantiles_abs_f32_nonaarch64` in contracts/chronos-bolt-parity-v1.yaml to that measurement in a `pv diff`-visible edit."
    expected: "A measured max|delta| replaces the PROVISIONAL 5.0e-6 headroom value."
    why_human: "CARRIED FORWARD, still open — re-confirmed at HEAD: contracts/chronos-bolt-parity-v1.yaml:58-60 and :100-103 still describe the value as PROVISIONAL headroom, not evidence. This box is aarch64 (uname -m = arm64) and every CI runner is X64 with no Chronos leg. D-ITEM-06-04 / REVIEW-06-02."
  - test: "Decide whether SC4's Chronos parity ladder staying DARK in CI is acceptable for the milestone, or whether the ci.yml embedded-weights leg should now be applied."
    expected: "An explicit decision recorded against D-ITEM-06-03."
    why_human: "CARRIED FORWARD, still open — re-confirmed at HEAD: `.github/workflows/ci.yml` contains ZERO `chronos` or `forecast` matches. Every SC4 parity claim rests on a manual local gate (which I ran green this session: 24 + 9 tests armed)."
  - test: "Decide the disposition of CR-01. Either (a) raise `MAX_NP_TRAIN_COST` toward the measured 2 s crossing (~18 000 000, the largest round value under the rejected-candidate wall of 2.089 s at 19 991 000) and ship the accepted-region test the review drafts, or (b) keep 15 000 000 and narrow the advertised contract: change `ForecastArgs.n_lags`'s doc and `door_surface.knobs.n_lags.enforced_by` to state that the reachable range depends on history length, and make the refusal message name the reachable `n_lags` for the history the caller sent."
    expected: "One of the two, with the accepted region written down and asserted — today only the refused region has evidence."
    why_human: "Choosing a bound VALUE is a design decision with a measured trade-off (refusing legal requests versus admitting requests that may exceed the 2 s bar), and this phase's own prohibition says 'never add a NeuralProphet cost bound without measuring first'. I reproduced the arithmetic (15 994 400 at 20 000 x n_lags=7, 6.63 % over) but did not measure that request's wall, so I cannot decide it for you. The advertised `n_lags <= 365` versus the reachable 0..=6 at maximum history is the part that reads as a mis-advertised knob."
  - test: "Decide whether `just forecast-sc1-sweep` (and `chronos-bench` / `chronos-coldstart` / `forecast-pool-ratio` / `forecast-holiday-bench`) should be wired to an automatic surface — a `make` tier or a scheduled workflow in the shape of `toolchain-ceiling.yml` — or whether they stay manual."
    expected: "Either the wiring lands, or the recipe headers and `binding.yaml`'s `sc1_wall_swept … status: implemented` note say explicitly that the gate is manual, so `implemented` is not read as `running`."
    why_human: "Every round-3 plan fences off `.github/workflows/*.yml` and states that wiring the gate into CI is a human decision. Confirmed at HEAD: no reference to `forecast-sc1-sweep` exists in `.github/`, `Makefile` or `scripts/`. The in-suite `sc1_wall::sc1_wall_sweep` that CI does run is a debug build and returns before the 2 s assertion by design (06-16 states this openly)."
---

# Phase 6: Native Time-Series Forecasting Stack — Verification Report

**Phase Goal:** Users can forecast a time series in one stateless MCP call — `ds[]`, `y[]`, horizon in; forecast with bands and components out — from pure-Rust Prophet and NeuralProphet ports and an embedded zero-shot Chronos-Bolt, each proven to parity with its Python original and served the way SetFit is served (thin pmcp servers, stdio + streamable-HTTP, Lambda-shaped).
**Verified:** 2026-09-07T20:12:35Z (HEAD `86e485e4f`, branch `gsd/phase-2-contract-gate`, host `aarch64-apple-darwin`)
**Status:** human_needed
**Re-verification:** Yes — after gap-closure round 3 (plans 06-14..06-17, waves 12-15, commits `e1b441944..f1cb0dc13`)

## Headline

**The three gaps this round targeted are closed, and I confirmed each one against the tree
rather than against a SUMMARY.** All five ROADMAP Success Criteria now hold on evidence I
generated in this session, including a release-profile SC1 sweep, an armed Chronos ladder, a
re-measured embedded binary and a re-measured pool ratio.

**The phase goal is met; the phase's paperwork is not yet accurate.** The fresh review
(`06-REVIEW.md`, commit `86e485e4f`) is largely correct, and I reproduced eight of its findings
independently. None of them falsifies a Success Criterion. Two of them do falsify a *plan-level*
must_have truth as written — 06-15's "the caller still learns the exact total" (the block that
would say so is unreachable) and 06-17's "a published crate's public surface" (the crate is
`publish = false`). One of them, CR-01, is a real over-refusal that I reproduced arithmetically
and that needs a human decision on a bound value.

So: **the gaps this round targeted are closed. The phase goal is met. Five items need a human**
— three carried forward unchanged from the previous verification, two new.

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence generated this session |
|---|---|---|---|
| SC1 | One stateless `forecast` tool over stdio + streamable-HTTP; full response shape; under 2 s for a 3 000-point daily series; every malformed input a validation error, never a silent default | ✓ VERIFIED | `just forecast-bench` → **0.212 s** for the 3 000-point daily fit + 365-step predict (`ROUND TRIP (SC1) OK: 0.212 < 2.0`, rc=0). `just forecast-sc1-sweep` → **19 compositions, every one under 2.0 s** on release/aarch64, worst 0.713 s. Spawned-binary stdio round trip → 1 passed, rc=0. All three previously-failing refusal classes now refused at the door (below) |
| SC2 | Prophet port reproduces Python Prophet 1.4.0 on the four named fixtures, seven rungs, as tests that run in CI | ✓ VERIFIED | `prophet::parity` enumerates **32 tests** — unmoved across the whole round, exactly as every round-3 prohibition requires. Green in the 155-test run for the two crates (0 failed, no `--skip`). `pv validate contracts/prophet-parity-v1.yaml` rc=0 |
| SC3 | NeuralProphet lag-free 365-day holdout MAE ≤ 0.47; AR-Net (`n_lags=30`) beats naive one-step; graph-connected Huber; data prep matches the oracle | ✓ VERIFIED | `np::parity` enumerates **9 tests**, green in the same run; bars read from `neuralprophet-parity-v1.yaml`, which `pv validate` passes rc=0. See the margin note below |
| SC4 | Chronos-Bolt-tiny f16 embedded; 9 quantiles match the 2.3.1 oracle through the server; 2 048-ctx < 100 ms; `horizon > 64` gated + warned; binary < 30 MB; cold start < 150 ms | ✓ VERIFIED | **Fully re-measured at HEAD**, because round 3 edited `bolt.rs` and `aprender-mcp-chronos/src/lib.rs`. Armed ladder `bolt:: chronos::` → **24 passed, 0 failed**. `aprender-mcp-chronos --lib` armed → **9 passed, 0 failed** incl. `long_horizon_refused_without_flag_and_warned_with_it`. `just chronos-embed-build` → **24 673 632 bytes < 30 000 000**. `just chronos-coldstart` → **34 ms < 150 ms**. `just chronos-bench` → **18.0 ms < 100 ms** at 2 048 context |
| SC5 | 8 concurrent requests bit-identical to sequential in < half the wall; every gate green | ✓ VERIFIED | `just forecast-pool-ratio` → **5.125x / 5.085x / 5.110x, best 5.125 ≥ 2.0**, rc=0. Bit-identity by `eight_concurrent_requests_are_bit_identical_to_sequential` + `sixteen_concurrent_neuralprophet_fits_stay_identical` in the green 155-test run. `cargo fmt --all -- --check` rc=0. `clippy -p {aprender-forecast, aprender-mcp-forecast, aprender-mcp-chronos} --all-targets --no-deps -- -D warnings` rc=0, 0 errors each. `pv validate` rc=0 on all four contracts. `make contract-audit-phase6` rc=0, **63 Phase 6 binding rows resolved, zero BIND-** |

**Score:** 5/5 truths verified (0 present, behavior-unverified)

Every SC here is behavior-dependent — state transitions and refusal/cleanup invariants — and
every one is carried by a behavioral test or a measured gate I ran myself, not by symbol
presence. That is why the score is a clean 5/5 rather than a presence count.

### Gap closure: the three previous gaps, checked against the tree

| Previous gap | Closed by | Verified how |
|---|---|---|
| **Gap 1** — `cap` accepted and provably inert on every non-logistic growth arm | 06-10 | `forecast.rs:162` returns `"cap is logistic-only; set growth to \"logistic\""`. Four library refusal cases at `:1173/:1180/:1186/:1194` (linear+cap, bare cap, flat+cap, linear+below-max-y cap) plus a logistic positive control; two e2e refusals through a live server at `mcp-forecast/lib.rs:679/:692`. Contract equation at `forecast-tool-boundary-v1.yaml:522` and the knob's `enforced_by` at `:207`. All green in the 155-test run |
| **Gap 2** — an in-bounds 3 000-point daily request with in-bounds holidays walled at 16.113 s | 06-11 / 06-12 | `MAX_HOLIDAY_DESIGN_COST = 50_000` (`types.rs:82`) on `(points + horizon) x holiday_columns`, `MAX_HOLIDAY_DATES_TOTAL = 10_000` (`:97`), both mirrored to `constants:` by `cost_bounds_match_contract`. The verifier's original 3000/181/84 geometry prices at 609 065 cells and is now **refused** — the justfile says so at `:810-811`. The measured replacement: `just forecast-bench` at 0.212 s and `just forecast-sc1-sweep` at 19/19 under 2 s |
| **Gap 3** — Knuth Poisson outside its numerical domain, bands silently too narrow | 06-13 / 06-17 | `POISSON_NORMAL_BRANCH_LAMBDA = 30.0` (`prophet.rs:743`) with the normal branch at `:771`, contract-owned via `constants.poisson_normal_branch_lambda` and asserted by `cost_bounds_match_contract`. 06-17 then added **variance** and **zero-mass** bars beside the mean bar, all three contract-owned (`prophet-parity-v1.yaml:186-188`) with written sigma arithmetic at every sweep point — the weakest shipped bar is the MEAN at lambda = 3, 4.24 sigma at N = 60 000. `poisson_mean_and_variance_track_lambda_across_its_whole_domain` is in the run set and green |

The CR-01 instance the round was shaped around — round 2's own fix walking into round 3 — is
closed at the door by `MAX_LOGISTIC_CHANGEPOINT_LAMBDA = 20_000.0` (`types.rs:168`), computed
through the single `prophet::changepoint_count` the design build also uses, with the review's
exact payload as an e2e refusal (`refuses_logistic_changepoint_lambda_over_bound`) and a
near-miss acceptance beside it. The gate output shows it working: the `freq=MS growth=logistic`
composition now runs at `horizon=841` (clamped by `max_legal_horizon` from the door's own bound)
rather than the 3 650 that produced the review's 2.334 s.

### The class invariant, checked rather than narrated

| Half | Mechanism | Status |
|---|---|---|
| KNOBS | `schema_knobs()` derives the field set from `schemars::schema_for!(ForecastArgs)` ∪ `schema_for!(HolidayArg)` — the same generator that produces the advertised tool schema — and `every_request_knob_is_enumerated` asserts **set equality**, so both MISSING and PHANTOM are structural | ✓ genuinely derived |
| COST AXES | `enumerated_axes()` reads `door_surface.cost_axes` out of the YAML and nothing else. `every_cost_axis_names_a_real_bound` checks each listed axis names a real `constants:` key; `no_cost_axis_is_pending` checks no listed axis carries an `unbounded_pending_*` marker | ⚠️ per-entry predicates on a hand-kept list — an unlisted axis is invisible (see advisory WR-03) |

Both tests are in the run set and green. All 14 axes (C-01..C-14) name a real `constants:` key;
**no pending marker survives**, which is the observable event 06-14 shipped red and 06-15 turned
green. The contract's own `door_surface_is_complete.formula` is honest about the asymmetry
(`knobs(...) ==` an equality, `forall a in cost_axes:` a per-entry predicate); the surrounding
prose is not, and that is what the advisory records.

### Required Artifacts

Every artifact declared by 06-14..06-17 exists and is substantive:

| Artifact | Expected | Status |
|---|---|---|
| `contracts/forecast-tool-boundary-v1.yaml` | `door_surface:` (16 knobs + 14 cost axes), the new constants and equations | ✓ present, `pv validate` rc=0 |
| `crates/aprender-forecast/src/types.rs` (39.0 K) | `MAX_LOGISTIC_CHANGEPOINT_LAMBDA`, `MAX_HOLIDAY_NAME_LEN`, `MAX_NP_TRAIN_COST`, the three completeness tests, `pool_default_matches_contract` split out | ✓ all present at :168 / :208 / :258 / :580 / :636 / :704 / :481 |
| `crates/aprender-forecast/src/forecast.rs` (66.9 K) | lambda refusal before `make_design`, in-loop aggregate refusal, holiday-name refusal, NP train-cost check | ✓ present |
| `crates/aprender-forecast/src/np.rs` (57.9 K) | `train_cost` / `door_lr_sweep` / `door_epochs` / `request_train_cost` as the door's single pricing path | ✓ present at :464-:511 |
| `crates/aprender-forecast/src/sc1_wall.rs` (19.7 K, new) | the parameterized sweep, one composition builder, no `#[ignore]` | ✓ present; `sc1_wall::sc1_wall_sweep` is in the `--lib` run set |
| `scripts/assert_measurement_under.sh` (6.4 K, new) | the shared numeric-shape validator | ✓ present |
| `scripts/check_assert_measurement_under_cases.sh` (5.2 K, new) | the must-match / must-not-match table | ✓ present, **run: 23/23 rows behaved as tabled**, rc=0 |
| `contracts/prophet-parity-v1.yaml` | `variance_tolerance: 0.05`, `zero_mass_tolerance: 0.22` beside an untouched `float_tolerance: 0.01` | ✓ present at :186-188 |
| `crates/aprender-forecast/README.md` | the 0.63.0 public-API notes | ✓ present — and factually wrong, see advisory WR-09 |
| `contracts/aprender/binding.yaml` | binding rows for the new equations | ✓ 63 Phase 6 rows resolved to definition sites, zero BIND-, zero RESOLVE- |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| `forecast.rs` | `forecast-tool-boundary-v1.yaml` | seven `constant_u64` + `constant_f64` mirrors asserted by `cost_bounds_match_contract` (D-15: contract is the source, code the mirror) | ✓ WIRED |
| `forecast.rs` (door) | `prophet::changepoint_count` | the ONE effective-count implementation, called by the door and by `make_design`, so the bound cannot be evaded by disagreement | ✓ WIRED |
| `forecast.rs` (door) | `np::request_train_cost` | the door prices with the same three functions it then uses to configure the sweep | ✓ WIRED |
| `types.rs` | `schemars::schema_for!` | `every_request_knob_is_enumerated` walks the generated schema, not a hand-kept list | ✓ WIRED (derived) |
| `types.rs` | `door_surface.cost_axes` | `every_cost_axis_names_a_real_bound` / `no_cost_axis_is_pending` iterate the YAML list | ⚠️ PARTIAL — per-entry only, not a completeness check |
| `justfile` | `scripts/assert_measurement_under.sh` | four SC bar sites at :625 / :686 / :751 / :869 plus the new sweep at :960 | ⚠️ PARTIAL — `chronos-coldstart` at :656 still uses `[ -ge ]` (fails closed) |
| `justfile forecast-sc1-sweep` | `sc1_wall.rs` | runs the sweep under `--release` and re-checks each `SC1 WALL:` line's `total_s` through the validator, so the bar is asserted in two places | ✓ WIRED — observed, 19/19 |
| `prophet.rs` | `prophet-parity-v1.yaml` | `equation_float(..., "variance_tolerance")` / `"zero_mass_tolerance"` read at test time | ✓ WIRED |
| `.github/workflows/ci.yml` | any Phase 6 gate | — | ✗ NOT WIRED, and deliberately so — zero `chronos`/`forecast` matches; routed to human decision |

### Data-Flow Trace (Level 4)

| Artifact | Value | Source | Real data | Status |
|---|---|---|---|---|
| `mcp-forecast` `forecast` tool | `yhat/lower/upper/trend/components` | `aprender_forecast::forecast` → `make_design` → `fit_prophet` (L-BFGS) → `predict` | Yes — `just forecast-bench` reports real L-BFGS round/iter/eval counts (4/794/1530 at 3 000 points) | ✓ FLOWING |
| `mcp-chronos` `forecast` tool | 9 quantiles | `chronos::forecast` → `Bolt::predict` over `EMBEDDED_WEIGHTS` | Yes — matched against the Python 2.3.1 oracle through the HTTP server, armed, 9/9 | ✓ FLOWING |
| SC1 sweep `total_s` | per-composition wall | `sc1_wall::time_accepted` around a real `forecast` call | Yes — the gate prints `fit_s` / `predict_s` / `band_width` / `cells` and they vary sensibly per composition | ✓ FLOWING |
| every assertion bar | tolerances + constants | `test_support::{constant_u64, constant_f64, equation_tolerance, equation_float}` reading the YAML at test time | Yes — the audit resolves all 63 Phase 6 binding rows to definition sites | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Round-3 tests are in the run set, unfiltered | `cargo nextest list -p aprender-forecast -p aprender-mcp-forecast --lib` | `no_cost_axis_is_pending`, `every_cost_axis_names_a_real_bound`, `every_request_knob_is_enumerated`, `cost_bounds_match_contract`, `poisson_mean_and_variance_track_lambda_across_its_whole_domain`, `sc1_wall::sc1_wall_sweep`, `refuses_logistic_changepoint_lambda_over_bound`, `refuses_holiday_name_over_length_bound` — all listed | ✓ PASS |
| Both crates' lib suites | `cargo nextest run -p aprender-forecast -p aprender-mcp-forecast --lib` (from the run brief) | 155 passed / 0 failed, rc=0, no `--skip` | ✓ PASS |
| SC1 stated bar | `just forecast-bench` | `ROUND TRIP (SC1) OK: 0.212 < 2.0`, rc=0 | ✓ PASS |
| SC1 swept gate (the round's headline) | `just forecast-sc1-sweep` | `SC1 SWEEP OK: 19 compositions, every one under the 2.0 s SC1 bar`, rc=0, `profile=release arch=aarch64` | ✓ PASS |
| SC1 stdio transport | `cargo nextest run -p aprender-mcp-forecast --test e2e_stdio` | 1 passed, rc=0 | ✓ PASS |
| SC4 armed parity ladder | `CHRONOS_MODEL_DIR=.../f32 cargo nextest run -p aprender-forecast --lib -E 'test(bolt::) or test(chronos::)'` | 24 passed, 0 failed, rc=0 | ✓ PASS |
| SC4 through the server | `CHRONOS_MODEL_DIR=.../f32 cargo nextest run -p aprender-mcp-chronos --lib` | 9 passed, 0 failed, rc=0 | ✓ PASS |
| SC4 binary size, re-measured at HEAD | `just chronos-embed-build` | 24 673 632 < 30 000 000, rc=0 | ✓ PASS |
| SC4 cold start, re-measured | `just chronos-coldstart` | median 34 ms < 150 ms, rc=0 | ✓ PASS |
| SC4 forward bench, re-measured | `just chronos-bench` | 18.0 ms < 100 ms at 2 048 ctx, rc=0 | ✓ PASS |
| SC5 pool ratio | `just forecast-pool-ratio` | best 5.125x ≥ 2.0, rc=0, `workers=4 cpus=14` | ✓ PASS |
| SC5 formatting | `cargo fmt --all -- --check` | rc=0 | ✓ PASS |
| SC5 clippy on the three new crates | `cargo clippy -p <crate> --all-targets --no-deps -- -D warnings` | rc=0, 0 errors, each | ✓ PASS |
| pmcp body-cap claim (WR-02) | read `Cargo.lock` + `pmcp-2.19.3/src/server/{streamable_http_server.rs,limits.rs}` | `stateless()` sets `max_request_bytes = 4 MiB`; enforced at `:4571` | ✗ the CONTRACT's claim is FALSE (enforcement unaffected) |
| CR-01 arithmetic | derived from `auto_epochs`, `n_training_samples`, `train_cost`, `door_lr_sweep` | 20 000 points, `n_lags=7` → 15 994 400 > 15 000 000, refused by 6.63 % | ✗ over-refusal reproduced |
| WR-01 reachability | read `forecast.rs:225-296` | in-loop check returns Err; no `continue`; post-loop block unreachable | ✗ dead code reproduced |

### Probe Execution

| Probe | Command | Result | Status |
|---|---|---|---|
| Validator case table | `bash scripts/check_assert_measurement_under_cases.sh` | rc=0, `TABLE OK: 23/23 rows behaved as tabled` — including `abc`, empty, `1.2.3` and an unknown mode all MUST_FAIL | PASS |
| Contract validation ×4 | `target/release/pv validate contracts/{forecast-tool-boundary,prophet-parity,neuralprophet-parity,chronos-bolt-parity}-v1.yaml` (pv 0.63.0) | rc=0, `0 error(s), 0 warning(s)` each | PASS |
| Phase 6 binding audit | `make contract-audit-phase6` | rc=0, 18/18 equations implemented, 324 obligations covered, **63 Phase 6 rows resolved**, zero BIND- | PASS |

### Requirements Coverage

As instructed and re-confirmed: `.planning/REQUIREMENTS.md` carries **no forecasting REQ-IDs** —
it is the SetFit milestone's document. ROADMAP.md's Phase 6 section says so in a
`**Requirements note**` and binds the phase to the five `prophet-forecast-mcp` decisions
(transcribed as D-01..D-18 in `06-CONTEXT.md`) plus SC1..SC5. Verification ran against those.
**This is not a traceability gap and is not reported as one.** No orphaned REQ-ID maps to Phase 6.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| all Phase 6 source + the two new scripts + the four contracts | — | `TBD` / `FIXME` / `XXX` | **none found** | Debt-marker gate clean |
| all Phase 6 source | — | `TODO` / `HACK` / `PLACEHOLDER` | **none found** | Clean |
| `crates/aprender-forecast/src/forecast.rs` | 283-295 | unreachable branch that three artifacts describe as firing | ⚠️ Warning | Advisory WR-01 |
| `contracts/forecast-tool-boundary-v1.yaml` | 303, 401, 457 | a disposition justified by a premise that the locked dependency falsifies | ⚠️ Warning | Advisory WR-02 |
| `crates/aprender-forecast/README.md` | 99-113 | semver breakage claimed on a `publish = false` crate | ⚠️ Warning | Advisory WR-09 |
| `crates/aprender-forecast/src/types.rs` | 258 | a bound whose accepted region has no evidence, only its refused region | ⚠️ Warning | Advisory CR-01 → human decision |
| `crates/aprender-forecast/src/prophet.rs` | 2216-2222 | in-code defaults the door refuses | ℹ️ Info | Advisory IN-01 |

No 🛑 Blocker was found. Per the re-verification evidence gate this matters: every flagged file
WAS git-modified since the previous `verified:` timestamp, so a Blocker classification here would
block unconditionally. I considered CR-01 for that classification and rejected it — it restricts
a corner of the parameter space rather than preventing the goal, SC1's stated timing shape
measures 0.212 s, and SC3's stated `n_lags = 30` geometry is accepted.

### Margin notes (recorded, not gaps)

1. **SC3 is accepted through the door by 3.0 %.** `np::parity`'s Peyton geometry prices at
   14 552 640 against `MAX_NP_TRAIN_COST = 15 000 000` — 97.0 % of the bound. It is not
   coincidental: `the_np_parity_ladder_geometry_prices_under_the_train_cost_bound` pins the
   relationship in a test, so the code establishes the precondition rather than relying on it
   incidentally. But the headroom is thin enough to state.
2. **SC2's `objective at Python's MAP` rung binds 3 of 7 fixtures.** Unchanged from the previous
   verification and unchanged by this round; the asymmetry is stated in three places and those
   fixtures are held by `fitted_objective_slack` plus the `predict_path_rel_yscale` chain, which
   does bind all seven. Judged sound. Consider an override on the SC2 wording.
3. **SC5's clippy clause is met at `--no-deps`.** With deps, `-D warnings` reaches
   `crates/aprender-compute` and fails identically for an untouched sibling crate. SC5's text is
   already scoped "on the new crates"; D-ITEM-06-09 restates the boundary.
4. **`make test` cannot pass on macOS at all** (`crates/aprender-profile/examples/validate_golden_trace.rs`
   imports a `#[cfg(target_os = "linux")]` module), and the full-workspace `--lib` run has 83
   pre-existing failures in 10 crates, none of which depends on `aprender-forecast`
   (`cargo tree -i` returns no match for all ten). Both are out of this phase's scope and are not
   attributed to it.

### Gaps Summary

**There are no gaps.** All three previous gaps are closed with mechanisms I verified in the
source and exercised through gates I ran. All five Success Criteria hold on measurements taken
this session at HEAD `86e485e4f`.

What is left is one design decision and a paperwork debt. The design decision is CR-01: a bound
whose refused region has four kinds of evidence and whose accepted region has none, which today
makes the advertised `n_lags <= 365` reachable only up to 6 at the advertised maximum history.
The paperwork debt is that four artifacts describe behavior the tree does not have — an
unreachable exact-total refusal, a "no transport caps the body" premise the locked pmcp 2.19.3
falsifies, a `publish = false` crate's semver breakage, and a cost-axis enumeration whose
completeness is inspected rather than derived while the prose calls it "THE DOOR'S WHOLE
SURFACE". None of it changes what the code does; all of it changes what the next reader will
believe the code does, which for a phase whose entire thesis is "a guard that cannot fail is
theater" is the failure mode worth naming.

---

_Verified: 2026-09-07T20:12:35Z_
_Verifier: Claude (gsd-verifier)_
