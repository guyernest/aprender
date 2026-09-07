---
status: testing
phase: 06-native-time-series-forecasting-stack
source: [06-VERIFICATION.md]
started: 2026-09-07T20:16:59Z
updated: 2026-09-07T20:16:59Z
---

## Current Test

number: 1
name: Both MCP demo pages complete the handshake and chart a forecast
expected: |
  Both pages complete the MCP handshake same-origin under /mcp and render a band chart.
awaiting: user response

## Tests

### 1. Demo pages: MCP handshake + chart rendering
expected: Both pages complete the MCP handshake same-origin under /mcp and render a band chart.
how: `cargo run -p aprender-mcp-forecast -- --http 8080` and `cargo run -p aprender-mcp-chronos -- --http 8081`, open each demo page, drive initialize -> tools/list -> tools/call.
why_human: CARRIED FORWARD, still open. 06-08 declared this a human end-of-phase check (visual rendering + a real browser MCP client). No automated test covers the pages; re-confirmed at HEAD.
result: [pending]

### 2. Chronos x86_64 parity measurement replaces the PROVISIONAL tolerance
expected: A measured max|delta| replaces the PROVISIONAL 5.0e-6 headroom value in contracts/chronos-bolt-parity-v1.yaml.
how: On an x86_64 host, `CHRONOS_MODEL_DIR=... cargo test -p aprender-forecast --lib bolt::parity chronos::parity -- --nocapture`, read the printed max|delta|, tighten `quantiles_abs_f32_nonaarch64` in a `pv diff`-visible edit.
why_human: CARRIED FORWARD, still open. This box is aarch64 (arm64); every CI runner is X64 with no Chronos leg. D-ITEM-06-04 / REVIEW-06-02.
result: [pending]

### 3. Decide whether SC4's Chronos ladder staying DARK in CI is acceptable
expected: An explicit decision recorded against D-ITEM-06-03.
how: Decide whether the ci.yml embedded-weights leg should now be applied, or the ladder stays a manual local gate.
why_human: CARRIED FORWARD, still open. `.github/workflows/ci.yml` contains ZERO `chronos` or `forecast` matches; every SC4 parity claim rests on a manual local gate.
result: [pending]

### 4. Decide the disposition of CR-01 (MAX_NP_TRAIN_COST over-refuses in-spec requests)
expected: One of the two options below, with the ACCEPTED region written down and asserted — today only the refused region has evidence.
how: |
  (a) Raise MAX_NP_TRAIN_COST toward the measured 2 s crossing (~18 000 000, the largest round
      value under the rejected-candidate wall of 2.089 s at 19 991 000) and ship an
      accepted-region test; OR
  (b) Keep 15 000 000 and narrow the advertised contract: change ForecastArgs.n_lags's doc and
      door_surface.knobs.n_lags.enforced_by to state the reachable range depends on history
      length, and make the refusal message name the reachable n_lags for the history sent.
why_human: |
  Choosing a bound VALUE is a design decision with a measured trade-off, and this phase's own
  prohibition says "never add a NeuralProphet cost bound without measuring first". The
  arithmetic is reproduced (2 x 50 x 19 993 x 8 = 15 994 400 at 20 000 points x n_lags=7,
  6.63% over the bound) but that request's WALL was not measured. The advertised n_lags <= 365
  versus the reachable 0..=6 at maximum history is the part that reads as a mis-advertised knob.
result: [pending]

### 5. Decide whether the forecast/chronos gates get an automatic surface
expected: Either the wiring lands, or the recipe headers and binding.yaml's `sc1_wall_swept ... status: implemented` note say explicitly that the gate is MANUAL, so `implemented` is not read as `running`.
how: Decide whether `just forecast-sc1-sweep` (and chronos-bench / chronos-coldstart / forecast-pool-ratio / forecast-holiday-bench) belong in a `make` tier or a scheduled workflow shaped like toolchain-ceiling.yml.
why_human: Every round-3 plan fences off .github/workflows/*.yml and states that CI wiring is a human decision. No reference to forecast-sc1-sweep exists in .github/, Makefile or scripts/. The in-suite sc1_wall_sweep that CI does run is a debug build and returns before the 2 s assertion by design.
result: [pending]

## Summary

total: 5
passed: 0
issues: 0
pending: 5
skipped: 0
blocked: 0

## Gaps
