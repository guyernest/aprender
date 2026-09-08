---
phase: 06-native-time-series-forecasting-stack
verified: 2026-09-08T00:58:45Z
status: passed
score: 5/5 must-haves verified
covered_files:
  - .planning/ROADMAP.md
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
  - .planning/phases/06-native-time-series-forecasting-stack/06-SECURITY.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-UAT.md
  - .planning/phases/06-native-time-series-forecasting-stack/06-VALIDATION.md
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
  - crates/aprender-mcp-chronos/static/index.html
  - crates/aprender-mcp-forecast/src/lib.rs
  - crates/aprender-mcp-forecast/src/main.rs
  - crates/aprender-mcp-forecast/static/index.html
  - crates/aprender-mcp-forecast/tests/e2e_stdio.rs
  - justfile
  - scripts/assert_measurement_under.sh
  - scripts/check_assert_measurement_under_cases.sh
covered_digest: "v1:sha256:c31c79406326ac03ae29bc1e0f1f7b56302caf77a9f890078a0fe01acb3e7002"
behavior_unverified: 0
overrides_applied: 0
decision_coverage:
  honored: 18
  total: 18
  not_honored: []
re_verification:
  previous_status: human_needed
  previous_score: 5/5
  previous_verified: 2026-09-07T20:12:35Z
  gaps_closed: []
  gaps_remaining: []
  regressions: []
  human_items_closed:
    - "Demo pages complete the MCP handshake and render a band chart — FAILED first, then FIXED (4da4980d5) and re-verified live by me at HEAD on both servers"
    - "x86_64 Chronos measurement replaces the PROVISIONAL tolerance — bar tightened 5.0e-6 -> 2.0e-6, contract 1.0.0 -> 2.0.0 (c3cb00d95)"
    - "SC4's Chronos ladder staying dark in CI — DECIDED accept-dark (06-UAT item 3); binding.yaml amended, justfile NOT (see advisory A4)"
    - "CR-01 disposition — DECIDED option (c), tier-resolved limits; ROADMAP Phase 7 created (fcd371e46)"
    - "Automatic surface for the SC1 gate — WIRED into make tier3 via forecast-sc1-gate (6ce4ff7d8), verified rc=0 by me at HEAD"
gaps: []
deferred:
  - truth: "CR-01 — MAX_NP_TRAIN_COST = 15 000 000 refuses well-formed in-spec NeuralProphet requests (20 000 points x n_lags=7 prices at 15 994 400, 6.63 % over), so the advertised n_lags <= 365 is reachable only to 6 at the advertised maximum history"
    addressed_in: "Phase 7 — Tier-Resolved Door Limits"
    evidence: "ROADMAP.md Phase 7 'Why this is a phase and not a constant edit' names CR-01 and its exact arithmetic; Phase 7 SC1/SC2/SC4 convert the 13 pub const MAX_* into a resolved DoorLimits profile and require an ACCEPTED-region test at every profile, with CR-01's own case (20 000 x n_lags=7) accepted under the tier that can afford it. Disposition decided by the owner in 06-UAT.md item 4 (option c)."
  - truth: "WR-02 — C-07's `no_structural_maximum: true` rests on a false premise; pmcp 2.19.3 StreamableHttpServerConfig::stateless() sets max_request_bytes = 4 MiB, enforced with a 413"
    addressed_in: "Phase 7 — Tier-Resolved Door Limits"
    evidence: "ROADMAP.md Phase 7 Success Criterion 5 names WR-02 verbatim, cites types.rs:188-194, three places in forecast-tool-boundary-v1.yaml and binding.yaml, and requires the stdio-vs-HTTP distinction to be stated precisely."
advisory:
  - finding: "A1 (WR-01, carried) — the post-loop `holiday_dates_total` refusal at forecast.rs:289-295 is unreachable for every input: the in-loop check at :242 returns Err the instant the running sum crosses and the loop body has no `continue`, so the EXACT-total message can never be delivered. Milder than the previous verification recorded: the source comment at :285-288 now ADMITS this ('Unreachable for a request whose sum crosses the ceiling mid-loop'). The contract's `door_surface.knobs.dates.enforced_by` and cost axis C-03 still describe it as firing."
    category: architectural
    reason: "Re-read forecast.rs:236-296 at HEAD (file unmodified since the previous pass). Not a correctness hole — the in-loop refusal fires and its message honestly says 'a running total, not the request's total' — and SC1's enumerated malformed-input list does not include the aggregate-dates ceiling, so no Success Criterion is falsified. Resolved by deleting the dead block and correcting the contract, or by hoisting the O(1) sum above the loop."
    evidence_status: "reproduced by verifier (source read at HEAD)"
  - finding: "A2 (WR-09, carried) — crates/aprender-forecast/README.md:99-113 describes `prophet::feature_row`'s signature change and `safetensors::load`'s removal as 'breaking for external callers' in 'the 0.63.0 line', but crates/aprender-forecast/Cargo.toml:15 is `publish = false` ('not a published API yet') and the crate was created in this phase. The same premise appears in bolt.rs's `transpose` comment."
    category: other
    reason: "Re-confirmed at HEAD by reading the manifest and README. Cosmetic in effect, misleading for a future maintainer. Not an SC."
    evidence_status: "reproduced by verifier (manifest + README read)"
  - finding: "A3 (NEW) — contracts/chronos-bolt-parity-v1.yaml still describes its own non-aarch64 bar as PROVISIONAL 5.0e-6 in three places after c3cb00d95 tightened it to 2.0e-6: the header at :58-59 ('carries a PROVISIONAL 5.0e-6 ... it is headroom, not evidence'), FALSIFY-CHRONOS-001's prediction at :368 ('on any other target it is <= the provisional 5.0e-6'), and all of FALSIFY-CHRONOS-002 at :373-376, whose rule is still stated as an open obligation ('must be replaced by a measurement') that the same commit discharged."
    category: other
    reason: "Reproduced by reading the file at HEAD. Enforcement is CORRECT and tighter — `equations.quantiles_abs_f32_nonaarch64.float_tolerance` is 2.0e-6, `metadata.version` is 2.0.0, `pv validate` rc=0, and the equation's own notes carry the full measurement rationale. The stale text cannot cause a false pass (2.0e-6 implies <= 5.0e-6) and SC4's literal 1e-6 f32 bar is the aarch64 equation, untouched and measured at 9.5367e-7 on BOTH architectures. This is the same paperwork-drift class the previous verification named, landing in the commit that closed a human item."
    evidence_status: "reproduced by verifier (contract read at HEAD)"
  - finding: "A4 (NEW) — commit 6ce4ff7d8's message states 'The chronos-gate header and binding.yaml's sc1_wall_swept note now say so outright'. The binding.yaml half is TRUE (binding.yaml:2040 carries the MANUAL wording). The chronos-gate half is FALSE: `git show --stat 6ce4ff7d8` shows the commit touched only Makefile and contracts/aprender/binding.yaml, and `grep -in manual justfile` at HEAD returns ZERO matches. justfile:511 still reads 'the CI leg this phase proposes', pointing the opposite way from the decision. 06-UAT item 3's own follow_on required 'Amend the chronos-gate recipe header ... to state MANUAL'."
    category: other
    reason: "Reproduced with two commands at HEAD. Does not falsify an SC — no SC requires the Chronos ladder to run in CI, and the accept-dark decision is recorded in 06-UAT.md item 3 and in 06-SECURITY.md's scope note. But item 3's decision text says the obligation IS the wording ('an unstated dark gate is indistinguishable from a gate nobody noticed stopped running'), and the chronos-gate recipe is precisely the dark gate it names. One-line fix: add the MANUAL sentence to the chronos-gate header and correct justfile:511's future tense."
    evidence_status: "reproduced by verifier (git show --stat + grep, both at HEAD)"
  - finding: "A5 (NEW) — 06-UAT.md bookkeeping does not close. Item 5's block still ends `result: [pending]` with no recorded outcome, while the Summary declares total: 5, passed: 2, decided: 2, pending: 0 — 2 + 2 = 4 of 5. Commit 6ce4ff7d8 landed item 5's substance (the tier3 wiring) but did not touch 06-UAT.md."
    category: other
    reason: "Reproduced by reading 06-UAT.md:122-137 and `git show --stat 6ce4ff7d8` at HEAD. The substance IS done and I verified it (`make forecast-sc1-gate` rc=0, tier3 calls it at Makefile:527). This is an artifact-accuracy defect, not an unverified item, so it does not reopen human verification."
    evidence_status: "reproduced by verifier (file read + git show)"
  - finding: "A6 (NEW, narrow residual on the T-06-32 remediation) — `every_cost_ceiling_constant_is_named_by_an_axis` derives its ceiling list from the `constants:` mapping, but by NAMING CONVENTION: it filters keys on the `fit_max_` / `chronos_max_` prefixes. A future cost ceiling named outside that convention is invisible to it, and the vacuity guard (>= 10) has 3 of headroom against the 13 ceilings present, so it would not catch the drift either."
    category: architectural
    reason: "Read at types.rs:675-745. The commit itself states the broader residual (an expensive path introducing no constant AND no axis is still invisible to all four tests; the detector for that is the SC1 sweep, now in tier3). Recorded so the convention is understood as load-bearing. It does NOT weaken the remediation, which I falsified by mutation — see the Probe table."
    evidence_status: "reproduced by verifier (source read); the test's RED side proven by injected mutation"
  - finding: "A7 (carried from 06-UAT's own `missing` list) — no automated test drives either demo page's tools/call payload. 06-UAT.md records that the forecast page was broken for four plans and three gap-closure rounds precisely because none exists, and lists such a test under `missing`; it was not added by the fix commit."
    category: other
    reason: "Confirmed: `grep -rn index.html` finds the page only at the two `include_str!` sites (mcp-forecast/src/lib.rs:92, mcp-chronos/src/lib.rs:143), with no test referencing it. Not an SC (ROADMAP UI hint: no — 'not a product UI'), and I closed the evidence gap for THIS pass by driving both pages' exact per-arm payloads live against HEAD builds (see Behavioral Spot-Checks). The regression risk is future, not present."
    evidence_status: "reproduced by verifier (grep + live drive)"
  - finding: "A8 (carried, self-recorded in the tree) — the 2 s bar holds for SC1's stated shape and for every swept composition, not for every accepted request. types.rs:76-81 records that 'a 4 700-point, 5-column request is only 25 000 cells — half this bound — and still walls at 4.2 s, reproducibly', because the fit's iteration count is data-dependent and no payload statistic predicts it."
    category: architectural
    reason: "Not a new finding and not hidden — the phase wrote it into the constant's doc comment. SC1's literal criterion is a 3 000-point daily series, measured at 0.212 s this session. Recorded so '19/19 under 2 s' is not read as a universal guarantee."
    evidence_status: "self-recorded in the tree; the 4.2 s wall NOT independently reproduced by verifier"
  - finding: "A9 (margin) — the tier3 SC1 gate's worst composition is the NeuralProphet row at 1.471 s this session (73.6 % of the 2.0 s bar); 6ce4ff7d8 recorded 1.420 s then 1.579 s on the same geometry, an 11.2 % run-to-run spread. Makefile:515-524 states this and states the correct fix is a quieter machine or narrower geometry, never a raised bar."
    category: other
    reason: "Reproduced: `make forecast-sc1-gate` rc=0, target/p06-forecast-sc1-sweep.log line 166. The geometry cannot be widened to add headroom without tripping CR-01's refusal, which the Makefile comment calls out as independent confirmation that the bound is too tight — deferred to Phase 7."
    evidence_status: "reproduced by verifier (gate run at HEAD)"
behavior_unverified_items: []
coincidental_reliance_items: []
human_verification: []
---

# Phase 6: Native Time-Series Forecasting Stack — Verification Report

**Phase Goal:** Users can forecast a time series in one stateless MCP call — `ds[]`, `y[]`, horizon in; forecast with bands and components out — from pure-Rust Prophet and NeuralProphet ports and an embedded zero-shot Chronos-Bolt, each proven to parity with its Python original and served the way SetFit is served (thin pmcp servers, stdio + streamable-HTTP, Lambda-shaped).
**Verified:** 2026-09-08T00:58:45Z (HEAD `554cf1576`, branch `gsd/phase-2-contract-gate`, host `aarch64-apple-darwin`)
**Status:** passed
**Re-verification:** Yes — third pass, after the five human-verification items from `1a91aedb1` were resolved (11 commits).

## Headline

**The phase goal is met, and this pass proved it by driving the shipped servers rather than
by reading SUMMARYs.** I started a HEAD build of each server on a fresh port (after checking
for the stale listeners 06-UAT.md warned about) and drove `initialize` → `tools/list` →
`tools/call` with the exact per-arm payloads the two demo pages construct. Both Prophet and
NeuralProphet return banded forecasts with components, timing and diagnostics; the Chronos
server returns nine native quantiles from embedded f16 weights and enforces its horizon gate.
All ten of SC1's enumerated malformed-input classes are refused with validation errors over
the live transport.

**All five human items are genuinely closed.** Two passed on human test (one after a real
FAILURE that was found, root-caused and fixed in the same session), two were decided by the
owner and recorded, and the fifth's wiring landed and works. I re-verified each against the
tree, not against 06-UAT.md.

**Nothing in `06-REVIEW.md`'s open findings falsifies a Success Criterion.** CR-01 and WR-02
are deferred to the newly-created Phase 7 with explicit, specific ROADMAP evidence. WR-01 and
WR-09 are real but are documentation-accuracy defects that leave every SC intact — WR-01's
refusal still fires (from the in-loop check), and WR-09 describes a semver relationship that
cannot exist for a `publish = false` crate.

**What I found that nobody reported: three of the commits that closed human items left claims
behind that the tree falsifies** (advisories A3, A4, A5), including a commit message asserting
an edit to a file it did not touch. None blocks the goal. All are one-line fixes. This is the
same class the previous verification named, recurring inside the very round that closed it.

## Goal Achievement

### Observable Truths (ROADMAP Phase 6 Success Criteria)

| # | Truth | Status | Evidence generated this session at HEAD `554cf1576` |
|---|---|---|---|
| SC1 | One stateless `forecast` tool on `aprender-mcp-forecast` over stdio + streamable-HTTP with `ds`/`y`/`horizon`/`freq`/`model`; returns `yhat`, `yhat_lower`, `yhat_upper`, `trend`, named components, timing and diagnostics in under 2 s for a 3 000-point daily series; every malformed input is a validation error, never a silent default | ✓ VERIFIED | **Driven live over HTTP on a fresh port from a HEAD build.** `model=prophet` → `n_ds=30`, ordered bands, `components=[weekly, additive_terms, multiplicative_terms]`, `fit_s=0.0709`, `predict_s=0.0017`, `diagnostics` present. `model=neuralprophet` → ordered bands, `components=[trend]`, `fit_s=0.5274`, `diagnostics` present. **All 10 SC1-enumerated refusal classes REFUSED** (unknown field, unsorted, duplicate, impossible date, constant `y`, horizon 0, horizon 3651, freq `H`, logistic without `cap`, 5 points) — each with a message naming the fix. `just forecast-bench` → `ROUND TRIP (SC1) OK: 0.212 < 2.0`, rc=0. `make forecast-sc1-gate` → 19 compositions, worst 1.471 s, rc=0. `e2e_stdio` spawned-binary round trip → 1 passed, rc=0 |
| SC2 | Prophet port reproduces Python Prophet 1.4.0 on the four committed fixtures, seven rungs, as tests that run in CI | ✓ VERIFIED | `prophet::parity` enumerates **32 tests — unmoved** from the previous two verifications, green in a 161-test run with 0 failed and no `--skip`. The three crates are workspace members not in CI's `--exclude` list, so `cargo nextest run --profile ci --workspace --lib` executes them. `pv validate contracts/prophet-parity-v1.yaml` rc=0 |
| SC3 | NeuralProphet lag-free 365-day holdout MAE ≤ 0.47; AR-Net (`n_lags=30`) beats naive one-step; graph-connected Huber; data prep matches the oracle | ✓ VERIFIED | `np::parity` enumerates **9 tests**, green in the same 161-test run (incl. `ar_net_30_lags_beats_naive_one_step` at 37.7 s). Bars read at test time from `neuralprophet-parity-v1.yaml`; `pv validate` rc=0. See margin note 1 |
| SC4 | Chronos-Bolt-tiny f16 embedded; nine quantiles match the 2.3.1 oracle through the server; 2 048-ctx < 100 ms; `horizon > 64` gated + warned; binary < 30 MB; cold start < 150 ms | ✓ VERIFIED | **Fully re-measured at HEAD.** Armed ladder `bolt:: chronos::` → **24 passed, 0 failed**, rc=0. `aprender-mcp-chronos --lib` armed → **9 passed, 0 skipped**, rc=0. `just chronos-embed-build` → **24 673 632 < 30 000 000**. `just chronos-coldstart 5` → **36 ms < 150 ms**. `just chronos-bench` → **18.2 ms < 100 ms** at 2 048 ctx. **Driven live**: 9 quantile keys `0.1…0.9`, ordered bands, `diagnostics.weights_dtype=F16`, `weights_source=embedded` (mechanism PROVEN, not assumed — CLAUDE.md rule 2); `horizon=365` without the flag REFUSED naming `allow_long_horizon`, with the flag accepted carrying the MASE warning |
| SC5 | Eight concurrent requests bit-identical to sequential in under half the wall; every gate green | ✓ VERIFIED | `just forecast-pool-ratio` → **best 5.137x ≥ 2.0**, `seq=7424ms conc=1445ms workers=4 cpus=14`, rc=0. Bit-identity by `pool_equality::eight_concurrent_requests_are_bit_identical_to_sequential` + `sixteen_concurrent_neuralprophet_fits_stay_identical`, green in the 161-run. `cargo fmt --all -- --check` rc=0. `cargo clippy -p {aprender-forecast, aprender-mcp-forecast, aprender-mcp-chronos} --all-targets --no-deps -- -D warnings` rc=0 each. `pv validate` rc=0 on all four contracts. `make contract-audit-phase6` rc=0, **zero BIND-** |

**Score:** 5/5 truths verified (0 present, behavior-unverified)

Every Success Criterion here is behavior-dependent — refusal invariants, state transitions
across a transport, cleanup/ordering under concurrency. **Not one is carried by symbol
presence.** Each is carried by a behavioral test or a measured gate I executed in this session
against a HEAD build. That is why the score is a clean 5/5 rather than a presence count.

### Binding decisions D-01..D-05 (the phase's requirements, per ROADMAP's Requirements note)

| # | Decision | Status | Evidence |
|---|---|---|---|
| D-01 | The MCP serving shape is a STATELESS `forecast` tool; one call carries `ds[]`, `y[]`, `horizon`, `freq` and the server fits and forecasts inside it | ✓ VERIFIED | Driven live: one `tools/call` returns a complete forecast; no artifact round trip exists in the tool list (`tools: ['forecast']` on both servers) |
| D-02 | Both Prophet and NeuralProphet ship behind the one tool, selected by `model:` | ✓ VERIFIED | Both arms driven live against the same tool name on the same server, returning `model=prophet` and `model=neuralprophet` |
| D-03 | Chronos is a THIRD forecaster with its OWN thin server, sharing the request/response shape | ✓ VERIFIED | `aprender-mcp-chronos` serves `TOOL_NAME=forecast` with the same `ds`/`y`/`horizon`/`freq` shape; driven live, `serverInfo.name = aprender-chronos` |
| D-04 | The correctness bar is parity with the Python originals, not self-consistency | ✓ VERIFIED | 32 + 9 + 24 parity tests against committed oracle fixtures from Prophet 1.4.0, NeuralProphet 0.9.0 and chronos-forecasting 2.3.1; provenance is EXTERNAL, not circular (see Test Quality Audit) |
| D-05 | Build order satisfied; the NEON kernel is assumed, not modified | ✓ VERIFIED | `crates/aprender-compute/src/blis/microkernels/neon.rs` present and untouched by this phase (`git diff --stat 1a91aedb1..HEAD` touches 6 files, none in aprender-compute) |

**Decision coverage gate:** `check.decision-coverage-verify` → **18/18 honored**, `not_honored: []`.

### The five human-verification items, checked against the tree

| # | Item | Prior status | Now | How I verified it — not from 06-UAT.md |
|---|---|---|---|---|
| 1 | Demo pages complete the MCP handshake and render a band chart | open | ✓ CLOSED | The item FAILED first (`35111f58d`), was root-caused to `static/index.html` building `args` unconditionally, and fixed in `4da4980d5`. I read the fix (`run()` now branches on `model`; `syncModelKnobs()` disables off-arm controls) AND drove both arms' exact payloads live → both return banded forecasts. I also drove the OLD payload and confirmed it is still refused, proving the door policy is unchanged and the fix was in the page. I separately drove the **Chronos** page's payload (9 quantiles, horizon gate, warning) since 06-UAT's original gap noted it untested |
| 2 | x86_64 Chronos measurement replaces the PROVISIONAL tolerance | open | ✓ CLOSED | `contracts/chronos-bolt-parity-v1.yaml` `metadata.version: 2.0.0`, `equations.quantiles_abs_f32_nonaarch64.float_tolerance: 2.0e-6` with notes recording the measured 9.5367e-7 and `ARCH=x86_64`. The aarch64 bar `quantiles_abs_f32 = 1.0e-6` is unchanged, and the armed ladder is green here at 24/24. `pv validate` rc=0. **Caveat A3**: three prose sites still say PROVISIONAL 5.0e-6 |
| 3 | Decide whether SC4's ladder staying dark in CI is acceptable | open | ✓ CLOSED (decision), ⚠️ follow-on partial | 06-UAT.md item 3 records `DECIDED 2026-09-07 by Guy — ACCEPT DARK`. binding.yaml:2040 carries the MANUAL wording. **The justfile does not** — `grep -in manual justfile` returns zero matches and `6ce4ff7d8` never touched the file, contradicting its own commit message (**advisory A4**) |
| 4 | Decide the disposition of CR-01 | open | ✓ CLOSED | 06-UAT.md item 4 records option (c) — tier-resolved limits — and ROADMAP.md now carries **Phase 7: Tier-Resolved Door Limits** (`fcd371e46`) with five draft SCs that name CR-01's arithmetic and WR-02 explicitly. Recorded as `deferred`, not as a gap |
| 5 | Decide whether the gates get an automatic surface | open | ✓ CLOSED | `Makefile:530` defines `forecast-sc1-gate`; `Makefile:527` calls it from tier3; the target FAILS LOUDLY when `just` is absent rather than skipping. **I ran `make forecast-sc1-gate` → rc=0, 19 compositions.** `6ce4ff7d8` records an INDUCED failure (bar 2.0 → 1.0, NP row 1.579 s, exit 2). **Caveat A5**: 06-UAT.md item 5 still reads `result: [pending]` |

### Required Artifacts

| Artifact | Expected | Status |
|---|---|---|
| `crates/aprender-mcp-forecast/static/index.html` | per-arm `args` construction; off-arm controls disabled | ✓ VERIFIED — `run()` branches on `model`; `syncModelKnobs()` present and bound to DOMContentLoaded + change; behavior confirmed live |
| `contracts/chronos-bolt-parity-v1.yaml` | measured non-aarch64 bar, MAJOR version bump | ✓ present, `2.0.0` / `2.0e-6`, `pv validate` rc=0 — with stale prose (A3) |
| `Makefile` `forecast-sc1-gate` | tier3-wired SC1 sweep, non-skippable | ✓ VERIFIED — defined `:530`, called `:527`, run rc=0 |
| `crates/aprender-forecast/src/types.rs` | `every_cost_ceiling_constant_is_named_by_an_axis` deriving ceilings from `constants:` with a real vacuity guard | ✓ VERIFIED **by mutation**, not by reading — see Probe Execution |
| `contracts/forecast-tool-boundary-v1.yaml` | `ChronosArgs` knobs + Chronos cost axes + `ceilings_subsumed` | ✓ present; `schema_knobs()` now feeds ForecastArgs ∪ HolidayArg ∪ ChronosArgs (`types.rs:566-583`); `pv validate` rc=0 |
| `.planning/ROADMAP.md` Phase 7 | CR-01's disposition as a phase, not a constant edit | ✓ present at `:445`, depends on Phase 6, five draft SCs naming CR-01 and WR-02 |
| `06-SECURITY.md` | `verdict: SECURED`, `threats_open: 0` | ✓ present; T-06-32 row 50 CLOSED with a Remediation section |
| `06-UAT.md` | `status: complete`, 0 pending | ✓ present — with the counter inconsistency at A5 |
| `COVERAGE.md` | declaration under the 200-char gate limit | ✓ present, reasoned no-external-API declaration |
| `crates/aprender-forecast/README.md` | 0.63.0 public-API notes | ✓ present — and factually wrong on a `publish = false` crate (A2) |

### Key Link Verification

| From | To | Via | Status |
|---|---|---|---|
| `static/index.html` `run()` | `forecast.rs` arm-scoping refusals | per-arm `args`; the door refuses off-arm options on presence (D-11) | ✓ WIRED — both directions confirmed live (new payload accepted, old payload refused) |
| `types.rs` `every_cost_ceiling_constant_is_named_by_an_axis` | `contracts/…` `constants:` | prefix-derived ceiling set, `ceilings_subsumed` checked both directions | ✓ WIRED (derived) — RED side proven by injected mutation |
| `types.rs` `schema_knobs()` | `schemars::schema_for!` × 3 structs | set equality against `door_surface.knobs` | ✓ WIRED (derived) — now covers the SECOND door (`ChronosArgs`) |
| `Makefile` tier3 | `just forecast-sc1-sweep` | `forecast-sc1-gate`, non-skippable | ✓ WIRED — run rc=0 |
| `bolt.rs` / `chronos.rs` tests | `chronos-bolt-parity-v1.yaml` | `equation_float` read at test time | ✓ WIRED — 24 armed tests green against the tightened bar |
| `justfile chronos-gate` header | 06-UAT item 3's MANUAL obligation | — | ✗ NOT WIRED — the file has no `manual` text; see A4 |
| `.github/workflows/ci.yml` | the release-profile Phase 6 gates | — | ✗ NOT WIRED, **and deliberately so** — decided accept-dark (06-UAT item 3); the correctness suites DO run in CI as workspace `--lib` members |

### Data-Flow Trace (Level 4)

| Artifact | Value | Source | Real data | Status |
|---|---|---|---|---|
| `mcp-forecast` `forecast` (prophet) | `yhat`/bands/`trend`/components | `aprender_forecast::forecast` → `make_design` → `fit_prophet` (L-BFGS) → `predict` | Yes — live response carries real per-request `fit_seconds=0.0709`, three named components, ordered bands | ✓ FLOWING |
| `mcp-forecast` `forecast` (neuralprophet) | `yhat`/bands/`trend` | AdamW autograd path on `spawn_blocking` | Yes — `fit_seconds=0.5274`, distinct band widths from the prophet arm | ✓ FLOWING |
| `mcp-chronos` `forecast` | 9 quantiles | `chronos::forecast` → `Bolt::predict` over `EMBEDDED_WEIGHTS` | Yes — `weights_source=embedded`, `weights_dtype=F16`, `forwards=1`, `rollouts=0`, `params=8652672` | ✓ FLOWING |
| SC1 sweep `total_s` | per-composition wall | `sc1_wall::time_accepted` around a real `forecast` call | Yes — 19 rows vary sensibly by freq/growth/holiday shape; `lambda` and `cells` track the composition | ✓ FLOWING |
| every assertion bar | tolerances + constants | `test_support::{constant_u64, constant_f64, equation_float}` reading YAML at test time | Yes — the binding audit resolves every Phase 6 equation with zero BIND- | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|---|---|---|---|
| Both model arms complete `tools/call` with the fixed page's payload | live HTTP drive against a HEAD `aprender-mcp-forecast` on :8791 | prophet OK (3 components), neuralprophet OK (trend); bands ordered on both | ✓ PASS |
| The OLD (broken) page payload is still refused | same, all knobs unconditionally | `n_lags is neuralprophet-only` / `growth is prophet-only` | ✓ PASS (fix was in the page; door unchanged) |
| SC1's ten refusal classes | same, one call per class | 10/10 REFUSED with fix-naming messages | ✓ PASS |
| Chronos page payload, horizon gate, warning | live HTTP drive against the release embedded binary on :8792 | 9 quantiles; `horizon=365` refused without flag, accepted with flag + MASE warning | ✓ PASS |
| Chronos embedded-weights mechanism engaged | same, read `diagnostics` | `weights_source=embedded`, `weights_dtype=F16` | ✓ PASS |
| Three crates' lib suites | `cargo nextest run -p aprender-forecast -p aprender-mcp-forecast -p aprender-mcp-chronos --lib` | 161 passed, 0 failed, 14 skipped (weight-gated), rc=0 | ✓ PASS |
| Parity ladders unmoved | grep the run log | `prophet::parity` 32, `np::parity` 9 — identical to both prior verifications | ✓ PASS |
| SC1 stated bar | `just forecast-bench` | `ROUND TRIP (SC1) OK: 0.212 < 2.0`, rc=0 | ✓ PASS |
| SC1 swept gate through its NEW tier3 entry point | `make forecast-sc1-gate` | 19 compositions, worst 1.471 s (NP row), rc=0 | ✓ PASS |
| SC1 stdio transport | `cargo nextest run -p aprender-mcp-forecast --test e2e_stdio` | 1 passed, rc=0 | ✓ PASS |
| SC4 armed ladder | `CHRONOS_MODEL_DIR=…/f32 cargo nextest run -p aprender-forecast --lib -E 'test(bolt::) or test(chronos::)'` | 24 passed, 0 failed, rc=0 | ✓ PASS |
| SC4 through the server, armed | `CHRONOS_MODEL_DIR=…/f32 cargo nextest run -p aprender-mcp-chronos --lib` | 9 passed, **0 skipped**, rc=0 | ✓ PASS |
| SC4 binary size | `just chronos-embed-build` | 24 673 632 < 30 000 000 | ✓ PASS |
| SC4 cold start | `just chronos-coldstart 5` | median 36 ms < 150 ms | ✓ PASS |
| SC4 forward | `just chronos-bench` | 18.2 ms < 100 ms at 2 048 ctx | ✓ PASS |
| SC5 pool ratio | `just forecast-pool-ratio` | best 5.137x ≥ 2.0 | ✓ PASS |
| SC5 fmt | `cargo fmt --all -- --check` | rc=0 | ✓ PASS |
| SC5 clippy ×3 | `cargo clippy -p <crate> --all-targets --no-deps -- -D warnings` | rc=0 each | ✓ PASS |
| A4 — justfile MANUAL wording | `grep -in manual justfile` | zero matches | ✗ obligation not discharged (advisory) |
| A3 — chronos contract stale prose | read `:58-59`, `:368`, `:373-376` | three sites still say PROVISIONAL 5.0e-6 | ✗ stale (advisory) |
| A1 — WR-01 reachability | read `forecast.rs:236-296` | in-loop check returns Err; no `continue`; post-loop block dead — and the comment now admits it | ✗ dead code (advisory) |

### Probe Execution

| Probe | Command | Result | Status |
|---|---|---|---|
| Validator case table | `bash scripts/check_assert_measurement_under_cases.sh` (run inside `make forecast-sc1-gate`) | `TABLE OK: 23/23 rows behaved as tabled` — including `abc`, empty, `1.2.3`, `-1`, `1e3` and an unknown mode as MUST_FAIL | PASS |
| Contract validation ×4 | `target/release/pv validate contracts/{forecast-tool-boundary,prophet-parity,neuralprophet-parity,chronos-bolt-parity}-v1.yaml` | rc=0, `0 error(s), 0 warning(s)` each | PASS |
| Phase 6 binding audit | `make contract-audit-phase6` | rc=0, `No binding gaps found`, **zero `BIND-`** | PASS |
| Decision coverage | `gsd-tools query check.decision-coverage-verify` | 18/18 honored | PASS |
| **T-06-32 remediation falsified, not read** | injected `fit_max_bogus_ceiling: 99` into `constants:`, ran `every_cost_ceiling_constant_is_named_by_an_axis` | **rc=100, RED**, naming `["fit_max_bogus_ceiling"]` in the T-06-32 message; contract restored → rc=0 green, `git diff` clean | PASS (RED side proven) |

**On the security remediation specifically, since it was done by the orchestrator rather than
an independent party:** the ceiling list at `types.rs:684-690` is derived from
`doc["constants"].keys()` filtered on the `fit_max_` / `chronos_max_` prefixes — a real
derivation, not a hand-kept list. The vacuity guard at `:691-696` (`ceilings.len() >= 10`
against 13 present) is real and would fire if the filter stopped matching. `ceilings_subsumed`
is checked in both directions (phantom constant at `:715-721`, dangling `subsumed_by` at
`:723-730`). I did not take any of this on the reading — I injected a ceiling and watched the
test go red with the correct name. The one residual is the naming convention itself (A6).

### Requirements Coverage

**No REQ-IDs apply, and none is reported as missing.** `.planning/REQUIREMENTS.md` is the
SetFit milestone's document and carries no forecasting IDs; ROADMAP.md's Phase 6 section says
so in a `**Requirements note**` and binds the phase to D-01..D-05 (verified above, table
"Binding decisions") plus SC1..SC5 (verified above). No orphaned REQ-ID maps to Phase 6.

### Test Quality Audit

| Concern | Finding | Verdict |
|---|---|---|
| Disabled tests on a requirement | 3 `#[ignore]` attributes in the phase crates (`forecast.rs:1383`, `prophet.rs:2217`, `np.rs:1352`), each a wall-clock MEASUREMENT harness with a stated reason and a `just` recipe entry point. No SC's proof rests on an ignored test — every SC bar is asserted by a recipe I ran or a non-ignored test in the 161-test run | ✓ no blocker |
| Circular expected values | None. Expected values come from committed oracle fixtures captured from Python Prophet 1.4.0, NeuralProphet 0.9.0 and chronos-forecasting 2.3.1 — an EXTERNAL system, which is D-04's whole point ("parity with Python Prophet 1.4.0 … not self-consistency") | ✓ VALID provenance |
| Assertion strength | Value-level and behavioral throughout: absolute/relative tolerances read from contracts at test time, bit-identity comparisons under load, live-transport refusal assertions on message content | ✓ sufficient |
| Coverage quantity | SC2's seven-rung ladder → 32 tests; SC3 → 9; SC4 → 24 + 9. Counts unmoved across three verification passes, which every round-3 prohibition required | ✓ met |
| Filtered/skipped in the run | 14 skipped in the unarmed run = the weight-gated Chronos tests, by design (D-18: a missing-weights run SKIPS with a visible non-zero count, never a silent green). Armed, they run 24 + 9 with **0 skipped** | ✓ non-vacuous, two-sided |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|---|---|---|---|---|
| all Phase 6 source, both scripts, all four contracts | — | `TBD` / `FIXME` / `XXX` | **none found** (grep exit 1, positive control confirms the scan ran) | Debt-marker gate CLEAN |
| all Phase 6 source | — | `TODO` / `HACK` / `PLACEHOLDER` / `todo!` / `unimplemented!` | **none found** | Clean |
| `contracts/chronos-bolt-parity-v1.yaml` | 58-59, 368, 373-376 | contract prose describing a bar the same commit tightened | ⚠️ Warning | Advisory A3 |
| `justfile` | chronos-gate header, 511 | a stated obligation not discharged; a commit message asserting otherwise | ⚠️ Warning | Advisory A4 |
| `06-UAT.md` | 122-137 | item 5 `[pending]` while the Summary says `pending: 0`; counts sum to 4/5 | ⚠️ Warning | Advisory A5 |
| `crates/aprender-forecast/src/forecast.rs` | 283-295 | unreachable branch the contract describes as firing | ⚠️ Warning | Advisory A1 |
| `crates/aprender-forecast/README.md` | 99-113 | semver breakage claimed on a `publish = false` crate | ⚠️ Warning | Advisory A2 |
| `crates/aprender-forecast/src/types.rs` | 684-690 | derivation bound to a naming convention | ℹ️ Info | Advisory A6 |

**No 🛑 Blocker was found.** Per the re-verification evidence gate this distinction is
load-bearing, so I state how I applied it rather than asserting the outcome:

- Every one of the seven findings above is a ⚠️ Warning or ℹ️ Info under Step 7's taxonomy —
  none prevents the phase goal and none is an unresolved debt marker. Step 9 Rule 1 promotes
  only 🛑 Blockers, so none triggers `gaps_found`.
- I **considered and rejected** promoting A3 and A4 to Blocker. A3's file WAS modified since
  the prior `verified:` timestamp, so a Blocker there would block unconditionally — but the
  ENFORCED tolerance is correct and strictly tighter, SC4's literal bar is a different
  equation that is unchanged and green, and stale descriptive prose cannot produce a false
  pass. A4's file was NOT modified since the prior pass and is not a carried-forward gap, so
  it is new-scope; it would need deterministic evidence of a DEFECT to block, and what I have
  is deterministic evidence of an undelivered documentation edit.
- Prior `gaps:` was empty, so no carried-forward gap exists to block on.

### Advisory (New Scope, Unevidenced)

Per the evidence gate this section must appear even when empty. Every item in the
`advisory:` frontmatter block (A1-A9) is reported here rather than suppressed, and each
carries the command or read that reproduced it. **None was downgraded from Blocker for lack
of evidence** — all are genuinely Warning/Info class, and all were independently reproduced.

| # | Finding | Category | Why advisory |
|---|---|---|---|
| A1 | WR-01 — post-loop exact-total refusal unreachable | architectural | reproduced; no SC falsified (the in-loop refusal fires) |
| A2 | WR-09 — README semver on a `publish = false` crate | other | reproduced; cosmetic |
| A3 | chronos contract still calls its bar PROVISIONAL 5.0e-6 | other | reproduced; enforcement correct and tighter |
| A4 | `chronos-gate` header never got the MANUAL wording its commit claims | other | reproduced; no SC requires CI, decision itself is recorded |
| A5 | 06-UAT item 5 `[pending]` vs `pending: 0` | other | reproduced; the substance landed and I ran it |
| A6 | T-06-32 ceiling derivation is naming-convention-bound | architectural | reproduced; the remediation itself falsified green/red |
| A7 | no automated test drives either demo page | other | reproduced; not an SC (UI hint: no); closed for this pass by live drive |
| A8 | the 2 s bar is not universal over accepted requests | architectural | self-recorded in the tree |
| A9 | tier3's NP row runs at 73.6 % of its bar with 11 % spread | other | reproduced; stated in the Makefile |

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|---|---|---|
| 1 | CR-01 — `MAX_NP_TRAIN_COST` over-refuses in-spec NeuralProphet requests | Phase 7 — Tier-Resolved Door Limits | ROADMAP Phase 7 "Why this is a phase and not a constant edit" reproduces CR-01's arithmetic (`2 x 50 x 19 993 x 8 = 15 994 400`, 6.63 % over) and SC4 requires CR-01's own case accepted under the tier that can afford it. Owner decision recorded at 06-UAT item 4 |
| 2 | WR-02 — C-07's `no_structural_maximum: true` false for HTTP | Phase 7 — Tier-Resolved Door Limits | ROADMAP Phase 7 SC5 names WR-02, cites `types.rs:188-194`, three sites in the boundary contract and binding.yaml, and pmcp's `limits.rs:46` / `streamable_http_server.rs:4571` |

Deferred items do not affect status.

### Margin notes (recorded, not gaps)

1. **SC3 is accepted through the door by 3.0 %.** `np::parity`'s Peyton geometry prices at
   14 552 640 against `MAX_NP_TRAIN_COST = 15 000 000`. Not coincidental reliance:
   `the_np_parity_ladder_geometry_prices_under_the_train_cost_bound` pins the relationship in
   a test, so the code establishes the precondition rather than depending on it incidentally.
   Phase 7 removes the tension by making the value tier-resolved.
2. **SC2's `objective at Python's MAP` rung binds 3 of 7 fixtures.** Unchanged across all
   three verification passes; the asymmetry is stated in three places and those fixtures are
   held by `fitted_objective_slack` plus the `predict_path_rel_yscale` chain, which does bind
   all seven. Judged sound.
3. **SC5's clippy clause is met at `--no-deps`.** SC5's text is already scoped "on the new
   crates"; with deps, `-D warnings` reaches `crates/aprender-compute` and fails identically
   for an untouched sibling. D-ITEM-06-09 restates the boundary.
4. **Out-of-scope failures are not attributed to this phase.** `make test` cannot pass on
   macOS at all (`crates/aprender-profile/examples/validate_golden_trace.rs` imports a
   `#[cfg(target_os = "linux")]` module), and the full-workspace `--lib` run has 83
   pre-existing failures across 10 crates, none of which depends on `aprender-forecast`
   (`cargo tree -i` returns no match for all ten).
5. **Method note on this pass.** CLAUDE.md's Verification Discipline applies to my own work.
   I hit rule 1 directly: `make forecast-sc1-gate > log` produced a log containing the literal
   string `... (21 lines truncated)` — the `rtk` hook truncated the command's output *before*
   the redirect, so the file I would have cited as evidence was incomplete. I re-read the
   recipe's own `target/p06-forecast-sc1-sweep.log` (171 lines, 19 `SC1 WALL` rows) instead.
   Every rc in this report was captured off the command itself, never through a pipe, and
   `rtk proxy` was used wherever raw output was load-bearing.

### Gaps Summary

**There are no gaps.** All five ROADMAP Success Criteria hold on measurements and live
transport drives I performed this session at HEAD `554cf1576`. All five human-verification
items from the previous pass are closed, and I verified each against the tree rather than
against `06-UAT.md`. `06-REVIEW.md` remains `status: issues_found`, and that is correct and
expected: its Critical (CR-01) and one Warning (WR-02) are deferred to Phase 7 with specific
ROADMAP evidence, and the two unfixed Warnings I was asked to assess (WR-01, WR-09) are
documentation-accuracy defects that falsify no Success Criterion — WR-01's refusal still fires
from the in-loop check and SC1's enumerated malformed-input list does not include the
aggregate-dates ceiling at all; WR-09 describes a semver relationship that cannot exist for a
crate that has never been published.

What remains is a short, consistent debt: **six artifacts describe a tree that differs from
the one that shipped** — an unreachable exact-total refusal the contract says fires, a
`publish = false` crate's semver breakage, a Chronos bar three sites still call PROVISIONAL
after it was measured and tightened, a `chronos-gate` MANUAL sentence a commit message claims
to have written and did not, a UAT item still marked pending whose work landed, and a
ceiling-derivation whose completeness depends on a naming convention nobody states as
load-bearing. None changes what the code does. Every one changes what the next reader will
believe the code does — and for a phase whose thesis is "a guard that cannot fail is theater",
that is the failure mode worth naming twice. All six are one-line fixes and none needs a plan.

---

_Verified: 2026-09-08T00:58:45Z_
_Verifier: Claude (gsd-verifier)_
