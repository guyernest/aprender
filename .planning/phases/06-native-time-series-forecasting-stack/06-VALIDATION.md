---
phase: 6
slug: native-time-series-forecasting-stack
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-09-05
updated: 2026-09-05
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution. Seeded by plan-phase from
> `06-RESEARCH.md` § Validation Architecture (measured on this branch, 2026-09-05); the per-task map
> is completed by the planner's task IDs and finalised by `/gsd-validate-phase 6`.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust libtest via `cargo test` 1.93.0; CI runs `cargo nextest run --profile ci` 0.9.102 (`.config/nextest.toml`: retries 2, fail-fast, slow-timeout 60 s × 20) |
| **Config file** | `.config/nextest.toml` (existing); root `Cargo.toml` gains `[profile.dev.package.aprender-forecast] opt-level = 3` if Wave 0 timing demands it (debug-profile Prophet fits measured 35–39× slower than release — 9.15 s vs 0.26 s for one Peyton round) |
| **Quick run command** | `cargo test -p aprender-forecast --lib prophet::parity::peyton` (one fixture, one ladder) |
| **Full suite command** | `cargo nextest run --profile ci --workspace --lib --exclude aprender-gpu --exclude aprender-cuda-edge --exclude aprender-compute` (CI's exact leg) **plus** `cargo test -p aprender-core --test monorepo_invariants --test readme_contract` (the two CI-wired drift gates, currently 5 failures on this branch) |
| **Estimated runtime** | quick ≈ 1–6 s release / up to 40 s debug; full workspace lib leg tens of minutes (80k tests); the two drift gates < 10 s |

---

## Sampling Rate

- **After every task commit:** `cargo clippy -p <crate> --all-targets -- -D warnings && cargo fmt --all -- --check && cargo test -p <crate> --lib`
- **After every plan wave:** CI's nextest lib leg + `cargo test -p aprender-core --test monorepo_invariants --test readme_contract` + `make contract-validate`
- **Before `/gsd-verify-work`:** all of the above green, plus `just chronos-gate` (embedded build, `0 ignored`), `just forecast-bench`, `just chronos-coldstart` on the aarch64 host with results recorded as evidence; the FALSIFY-MONO-011 allowlist decision recorded
- **Max feedback latency:** 60 s for the per-task quick loop (release-profile parity tests); the full leg is per wave, not per task

---

## Per-Task Verification Map

Task IDs are assigned by the planner; the rows below are the requirement → test mapping the plans must realise (from `06-RESEARCH.md` § Validation Architecture). `/gsd-validate-phase 6` rewrites this table against the executed plans.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | 0 | D-04 / SC2 (Prophet parity ladder on peyton, air, retail, wp_log_R + holidays, logistic, multiplicative) | — | N/A | unit (`--lib`, release-profile override) | `cargo test -p aprender-forecast --lib prophet::parity` | ❌ W0 | ⬜ pending |
| TBD | TBD | 0 | D-10 / SC3 (NP data prep vs `np_oracle_peyton.json`; lag-free MAE ≤ 0.47; `n_lags = 30` beats naive; Huber gradient connected) | — | N/A | unit | `cargo test -p aprender-forecast --lib np::parity` | ❌ W0 | ⬜ pending |
| TBD | TBD | 0 | D-13 / SC4 (Bolt ladder vs `chronos_bolt_tiny_fixture.json` 1e-6 f32; probes; 365-step rollout 1.5e-5; f16 within 2 % of std) | — | N/A | unit, `cfg_attr`-ignored without weights (counted skip) | `CHRONOS_MODEL_DIR=models/chronos-bolt-tiny/f32 cargo test -p aprender-forecast --lib bolt::parity` | ❌ W0 + `just fetch-chronos-tiny` | ⬜ pending |
| TBD | TBD | 1 | D-11 / SC1 (every refusal is `pmcp::Error::validation` naming the fix; prophet + NP happy paths over in-process streamable-HTTP) | T-06-01 input validation | malformed input never reaches a fit | unit `#[cfg(test)] mod e2e` | `cargo test -p aprender-mcp-forecast --lib e2e` | ❌ W0 | ⬜ pending |
| TBD | TBD | 1 | D-11 / SC4 (Chronos refusals; `allow_long_horizon` → `warning` + `forwards: 46`; parity through the server) | T-06-01 | as above | unit, `cfg_attr`-ignored without weights | `CHRONOS_MODEL_DIR=… cargo test -p aprender-mcp-chronos --lib e2e` | ❌ W0 | ⬜ pending |
| TBD | TBD | 1 | D-12 / SC5 (8 concurrent requests bit-identical to sequential; wall < ½ sequential with pool K = 8) | T-06-02 resource exhaustion | bounded pool, per-request budget | unit (equality asserted; speed-up asserted only on aarch64 release, else logged) | `cargo test -p aprender-mcp-forecast --lib pool_equality` | ❌ W0 | ⬜ pending |
| TBD | TBD | 0 | D-18 (missing weights → counted skip with reason; armed → tests run) | — | N/A | meta-test of the skip mechanism | `cargo test -p aprender-mcp-chronos --lib 2>&1 \| grep -c "ignored,"` (≥ 1 unarmed, 0 armed) | ❌ W0 (`build.rs` cfg) | ⬜ pending |
| TBD | TBD | 0 | D-15 (every new contract validates; obligations/falsification counts non-zero) | — | N/A | shell gate | `cargo run -p aprender-contracts-cli --bin pv -- validate contracts/<f>.yaml` + `pv status` + `make contract-validate` | ❌ W0 (3 YAMLs + Makefile lines) | ⬜ pending |
| TBD | TBD | all | SC5 lint/fmt on the new crates | — | N/A | shell | `cargo clippy -p aprender-forecast -p aprender-mcp-forecast -p aprender-mcp-chronos --all-targets -- -D warnings && cargo fmt --all -- --check` | ✅ | ⬜ pending |
| TBD | TBD | 0 | F7 drift gates (README counts, per-crate READMEs with the `paiml/aprender` link, CLAUDE.md paths, FALSIFY-MONO-011 bin allowlist) | — | N/A | integration (CI-wired) | `cargo test -p aprender-core --test monorepo_invariants --test readme_contract` | ✅ exists — **5 failures today** | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/aprender-forecast/tests/fixtures/` — the 18 committed oracle files from spikes 001, 002, 003, 005, 007 (≈ 7 MB), or a recorded decision to reference `.planning/spikes/` (RESEARCH Open Question 2)
- [ ] `crates/aprender-forecast/build.rs` + `crates/aprender-mcp-chronos/build.rs` — `cargo::rustc-check-cfg=cfg(chronos_weights)` + `rustc-cfg` when `CHRONOS_MODEL_DIR/model.safetensors` exists (RESEARCH F6); server `build.rs` also stages `CHRONOS_EMBED_DIR` (D-13)
- [ ] `justfile` recipes: `fetch-chronos-tiny` (pinned revision `a0e552de83495b5c28c14c71c374f3e33280b340`, sha256-checked), `chronos-gate`, `forecast-bench`, `chronos-coldstart`
- [ ] `models/chronos-bolt-tiny/{f32,f16}/` populated locally (gitignored — `*.safetensors` is ignored globally); shas recorded in the crate README
- [ ] Root `Cargo.toml`: three `members` lines; `[profile.dev.package.aprender-forecast] opt-level = 3` after measuring debug parity time
- [ ] `contracts/forecast-tool-boundary-v1.yaml`, `contracts/prophet-parity-v1.yaml`, `contracts/chronos-bolt-parity-v1.yaml` on the `setfit-apr-v1.yaml` shape (NOT `neon-blis-v1.yaml`, which `pv status` shows as hollow) + Makefile `$(CONTRACTS)` lines
- [ ] Human checkpoint: FALSIFY-MONO-011 `[[bin]]` allowlist (`crates/aprender-core/tests/monorepo_invariants.rs:235-395`) — an exemption class for thin MCP deployment units, baseline 27 → 33, decided before any executor edits it
- [ ] README fixes the gates demand: `README.md` crate/contract counts re-derived; `crates/aprender-mcp-setfit/README.md` monorepo link; READMEs for `aprender-mcp-setfit-lambda` and `aprender-contrastive-data`; three new crate READMEs with the `paiml/aprender` link
- [ ] Framework install: none — cargo, nextest, just, uv present

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Peyton round trip < 2 s; 2 048-pt Chronos forward < 100 ms | SC1, SC4 | Timing is host- and profile-dependent; the X64 CI box and debug builds cannot assert it | `just forecast-bench`, `just chronos-bench` on the aarch64 host, release profile; paste the tables into the SUMMARY |
| Release binary < 30 MB with embedded tiny-f16 | SC4 | Needs the embedded build, which has no CI leg without a workflow edit (human check-in) | `CHRONOS_EMBED_DIR=models/chronos-bolt-tiny/f16 cargo build --release -p aprender-mcp-chronos && ls -l target/release/aprender-mcp-chronos` |
| Cold start to first forecast over stdio < 150 ms | SC4 | Spawn timing, aarch64 release only | `just chronos-coldstart 3` (spike-007 harness) |
| Demo page performs initialize → tools/list → tools/call and charts the result | D-06 (demo page copied verbatim) | Browser interaction | `cargo run -p aprender-mcp-forecast -- --http 8787`, open `http://127.0.0.1:8787/`, load Peyton, press Forecast; repeat for `aprender-mcp-chronos --http 8788` |
| Speed-up from the router pool (3.9× measured in spike 010) | SC5 | Wall-clock ratio needs cores and release profile; CI asserts equality only | `cargo test -p aprender-mcp-forecast --lib pool_equality --release -- --nocapture` on aarch64 and read the printed ratio |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60 s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
