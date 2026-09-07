---
status: testing
phase: 06-native-time-series-forecasting-stack
source: [06-VERIFICATION.md]
started: 2026-09-07T20:16:59Z
updated: 2026-09-07T20:31:41Z
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
expected: A bound whose VALUE is resolved per deployment envelope, with the accepted region written down and asserted.
result: DECIDED 2026-09-07 by Guy — option (c), neither (a) nor (b).
decision: |
  "Make the limits such as CR-01 more flexible as some algorithms might have bigger
  requirements."

  This supersedes the two options previously offered (raise to ~18M, or narrow the
  advertised n_lags). Both assumed one hard constant was correct and only its value was
  in question. The correct frame is that there is no single right number, because the
  product ships four deployment envelopes that differ by orders of magnitude:
  CloudFlare Workers WASM (tightest, possibly below our requirements), AWS Lambda,
  Docker on GCP/Azure, and customer-hosted pmcp.run.

  FLEXIBLE IS NOT UNBOUNDED. Round 3's whole thesis — every cost axis enumerated, every
  bound enforced at the door, no axis pending — is preserved. What changes is only where
  the VALUE comes from: from a hard `pub const` to a resolved limits profile with today's
  constants as the default. The invariant strengthens rather than weakens:
    before: "cost axis C-08 is bounded at 15_000_000"
    after:  "cost axis C-08 is always bounded; its value comes from the resolved profile;
             no profile can disable a bound or set it above the tier's structural maximum"
follow_on: |
  Converts into a gap-closure/next-phase plan, not a one-line constant edit:
    - the 13 `pub const MAX_*` in types.rs become a `DoorLimits` policy struct with a
      `Default` equal to today's values
    - contracts/forecast-tool-boundary-v1.yaml `constants:` becomes the DEFAULT profile;
      `types::tests::cost_bounds_match_contract` keeps pinning the default
    - named tier profiles whose ceilings come from each envelope's real structural maximum
    - the accepted region gains a test at every profile — the gap CR-01 actually exposed

### 5. Decide whether the forecast/chronos gates get an automatic surface
expected: Either the wiring lands, or the recipe headers and binding.yaml's `sc1_wall_swept ... status: implemented` note say explicitly that the gate is MANUAL, so `implemented` is not read as `running`.
how: Decide whether `just forecast-sc1-sweep` (and chronos-bench / chronos-coldstart / forecast-pool-ratio / forecast-holiday-bench) belong in a `make` tier or a scheduled workflow shaped like toolchain-ceiling.yml.
why_human: Every round-3 plan fences off .github/workflows/*.yml and states that CI wiring is a human decision. No reference to forecast-sc1-sweep exists in .github/, Makefile or scripts/. The in-suite sc1_wall_sweep that CI does run is a debug build and returns before the 2 s assertion by design.
result: [pending]

## Summary

total: 5
passed: 0
decided: 1
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps
