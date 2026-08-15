---
phase: 4
slug: apr-artifact-and-production-parity
status: ready
nyquist_compliant: true
wave_0_complete: false
created: 2026-08-14
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Populated from 04-RESEARCH.md § Validation Architecture + the 04-01..04-11 plan set.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (libtest) + trybuild + proptest; cargo-nextest 0.9.102 in CI; cargo-mutants 25.3.1 (scoped, 04-11 only) |
| **Config file** | Makefile tiers (tier1–tier4) + `.github/workflows/ci.yml` (setfit steps at ci.yml:274–309) |
| **Quick run command** | `cargo test -p <touched-crate> --features setfit --lib setfit::` (aprender-core / aprender-train; apr-cli and aprender-serve use the per-task scoped filters below) |
| **Full suite command** | `make tier3` (setfit targets + contracts via `$(PV_BIN)` + feature matrix; parity harness runs in default `cargo test` after 04-09) |
| **Estimated runtime** | scoped filter ~30–120s incl. incremental compile; `make tier3` 1–5 min |

---

## Sampling Rate

- **After every task commit:** Run the task's scoped `<automated>` filter for the touched crate(s) — every filtered run carries a nonzero-ran guard (CR-02: zero-match exiting 0 is vacuous)
- **After every plan wave:** Run `make tier2` (arm64 scoped-clippy caveat per RESEARCH Pitfall 6) + all scoped setfit suites landed so far
- **Before `/gsd:verify-work`:** `make tier3` must be green (setfit targets, `$(PV_BIN) validate contracts/setfit-apr-v1.yaml`, feature matrix, parity harness + in-band negative)
- **Max feedback latency:** 120 seconds (scoped filter including incremental compile)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 4-01-01 | 01 | 1 | SAFE-01 | T-04-01/02 | contract carries falsification gates; tolerances cite frozen provenance | contract | `cargo run --release -p aprender-contracts-cli --bin pv -- validate contracts/setfit-apr-v1.yaml` | ❌ W0 | ⬜ pending |
| 4-01-02 | 01 | 1 | SAFE-01 | T-04-01 | contract wired into `$(CONTRACTS)` + blocking phase audit | gate | `grep -v '^#' Makefile \| grep -c "setfit-apr-v1.yaml" && make contract-audit-phase4` | ❌ W0 | ⬜ pending |
| 4-01-03 | 01 | 1 | SAFE-01 | — | N/A (docs — D-09 exception row) | docs | `grep -c "SetFit" CLAUDE.md` | ✅ | ⬜ pending |
| 4-02-01 | 02 | 2 | APR-01 | T-04-03/04/05 | deterministic metadata; probes contain no dataset text; non-finite rejected before write | unit | `cargo test -p aprender-core --features setfit --lib setfit::artifact::` | ❌ W0 | ⬜ pending |
| 4-02-02 | 02 | 2 | APR-01 | T-04-03 | cross-process byte equality + hash stability | unit | `cargo test -p aprender-core --features setfit --lib setfit::artifact::determinism` | ❌ W0 | ⬜ pending |
| 4-03-01 | 03 | 3 | APR-02 | T-04-06/07/08/09 | every corruption class dies typed before a model exists; length cap before parse | unit (induced corruption) | `cargo test -p aprender-core --features setfit --lib setfit::artifact::ladder` | ❌ W0 | ⬜ pending |
| 4-03-02 | 03 | 3 | APR-02, APR-05 | T-04-07/10 | probe replay is the last rung; typestate mint-on-success; embed accessor typed-fallible | unit | `cargo test -p aprender-core --features setfit --lib setfit::artifact::probe` | ❌ W0 | ⬜ pending |
| 4-03-03 | 03 | 3 | APR-04 | T-04-10 | out-of-crate code cannot mint the witness type | trybuild (compile-fail) | `cargo test -p aprender-train --features setfit --test ui` | ❌ W0 | ⬜ pending |
| 4-04-01 | 04 | 4 | OPS-04 | T-04-13 | non-finite values unrepresentable in serialized output | unit + golden | `cargo test -p aprender-core --features setfit --lib setfit::classify::envelope` | ❌ W0 | ⬜ pending |
| 4-04-02 | 04 | 4 | OPS-04, OPS-06 | T-04-11/12 | batch cap typed (256); backend identity from execution only, no setter | unit | `cargo test -p aprender-core --features setfit --lib setfit::classify::` | ❌ W0 | ⬜ pending |
| 4-05-01 | 05 | 4 | APR-03 | T-04-14 | foreign format refused twice (trusted + codec self-check); closure holds twice | unit | `cargo test -p aprender-train --features setfit --lib setfit::apr_codec::` | ❌ W0 | ⬜ pending |
| 4-05-02 | 05 | 4 | APR-03 | T-04-15/16 | trusted policy closure + EXACT + production-loader cross-check; mutated byte fails typed | integration (in-lib) | `cargo test -p aprender-train --features setfit --lib setfit::apr_codec::round_trip` | ❌ W0 | ⬜ pending |
| 4-05-03 | 05 | 4 | OPS-01 | — | zero apr-cli deps; train→save→load→embed→classify→inspect via public API only | integration | `cargo test -p aprender-train --features setfit --test setfit_apr_lifecycle` | ❌ W0 | ⬜ pending |
| 4-06-01 | 06 | 5 | OPS-02 | T-04-17 | feature-gated namespace compiles with AND without setfit | build | `cargo check -p apr-cli --features setfit && cargo check -p apr-cli` | ✅ (checks existing crates) | ⬜ pending |
| 4-06-02 | 06 | 5 | OPS-02, OPS-06 | T-04-17/18/19 | deny_unknown_fields config; typed CudaNotAvailable before data load; atomic temp+rename write | unit | `cargo test -p apr-cli --features setfit --lib setfit_train` | ❌ W0 | ⬜ pending |
| 4-06-03 | 06 | 5 | OPS-02 | T-04-19 | end-to-end tiny-fixture train leg (tier3-weight) | integration (ignored) | `cargo test -p apr-cli --features setfit --lib setfit_train -- --ignored --include-ignored` | ❌ W0 | ⬜ pending |
| 4-07-01 | 07 | 6 | OPS-03 | T-04-20/22 | explicit-tag-only auto-detect; untagged APR stays plain (negative test) | unit | `cargo test -p apr-cli --features setfit --lib predict` | ❌ W0 | ⬜ pending |
| 4-07-02 | 07 | 6 | APR-05 | T-04-22 | inspect recovers every identity field via the full ladder | unit | `cargo test -p apr-cli --features setfit --lib inspect` | ❌ W0 | ⬜ pending |
| 4-07-03 | 07 | 6 | OPS-02 | T-04-21 | test split reachable only via lock→token→grant; typed refusal without lock | unit | `cargo test -p apr-cli --features setfit --lib eval::setfit` | ❌ W0 | ⬜ pending |
| 4-08-01 | 08 | 6 | OPS-05 | T-04-23/24/25 | slot holds only VerifiedSetFitModel; batch + 1 MiB body bounds | build | `cargo check -p aprender-serve --features setfit && cargo check -p aprender-serve` | ✅ (checks existing crates) | ⬜ pending |
| 4-08-02 | 08 | 6 | OPS-03, OPS-05 | T-04-24 | startup branches inside the APR arm after the typed tag; typed fail on ladder errors | unit | `cargo check -p apr-cli --features setfit && cargo test -p apr-cli --features setfit --lib serve` | ❌ W0 | ⬜ pending |
| 4-08-03 | 08 | 6 | OPS-05 | T-04-23/25 | oneshot suite in every `cargo test`; readiness hash pinned to the loaded model | in-process HTTP | `cargo test -p aprender-serve --features setfit --lib setfit` | ❌ W0 | ⬜ pending |
| 4-09-01 | 09 | 7 | SAFE-01, OPS-04, OPS-01 | T-04-29 | three-surface value parity; CLI leg pinned via CARGO_BIN_EXE_apr, never PATH | integration | `cargo test -p apr-cli --features setfit,inference --test setfit_parity` | ❌ W0 | ⬜ pending |
| 4-09-02 | 09 | 7 | SAFE-01 | T-04-27/28 | frozen goldens + SHA-256 manifest; in-band skewed negative FAILS the gate every run | golden + negative | `cargo test -p apr-cli --features setfit,inference --test setfit_parity golden` | ❌ W0 | ⬜ pending |
| 4-09-03 | 09 | 7 | SAFE-01 | T-04-29 | ONE spawned-serve smoke; Drop-guarded child; status never read through a pipe | spawned smoke (ignored, tier3) | `cargo test -p apr-cli --features setfit,inference --test setfit_parity -- --ignored spawned_serve_smoke` | ❌ W0 | ⬜ pending |
| 4-10-01 | 10 | 8 | SAFE-01 | T-04-30/31 | ran-something guards on every filtered target; `rc=$?` never through a pipe | gate (Make) | `make setfit-apr-tests && make setfit-codec-tests && make setfit-cli-tests && make setfit-serve-tests && make setfit-parity && make setfit-api-boundary` | ❌ W0 | ⬜ pending |
| 4-10-02 | 10 | 8 | SAFE-02 | T-04-32 | four-crate feature matrix; boundary greps carry a must-match case table | build matrix | `make setfit-feature-matrix && make contract-audit-phase4` | ❌ W0 | ⬜ pending |
| 4-11-01 | 11 | 9 | SAFE-02 | T-04-33 | prepared-diff-only; no ci.yml commit before approval | manual-prep evidence | `git diff --stat .github/workflows/ci.yml` | ✅ | ⬜ pending |
| 4-11-02 | 11 | 9 | SAFE-02 | T-04-33 | blocking human checkpoint gates the CI edit | checkpoint | `rtk proxy git status --porcelain -- .github/workflows/ci.yml` | ✅ | ⬜ pending |
| 4-11-03 | 11 | 9 | SAFE-02 | T-04-34/35 | evidence-cited requirement audit; mutation gate reports total mutant count | gate | `grep -c "apr-cli --features setfit" .github/workflows/ci.yml && grep -A 2 "TRN-07" .planning/REQUIREMENTS.md \| grep -c "\[x\]"` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

No standalone Wave 0 scaffold plan: every code-producing task is `tdd="true"` and creates its own
test surface RED-first inside the task, so each ❌ W0 marker above is satisfied by its own task's
RED step before implementation. The new test surfaces (mapping RESEARCH § Wave 0 Gaps → plans):

- [ ] `crates/aprender-core/src/setfit/artifact.rs` tests (ladder/determinism/probe) — 04-02, 04-03
- [ ] `crates/aprender-core/src/setfit/classify.rs` tests + envelope goldens — 04-04
- [ ] `crates/aprender-train/src/train/setfit/apr_codec.rs` closure/determinism tests — 04-05
- [ ] `crates/aprender-train/tests/ui/setfit_verified_model_constructed.{rs,stderr}` trybuild case — 04-03
- [ ] `crates/aprender-train/tests/setfit_apr_lifecycle.rs` (OPS-01) — 04-05
- [ ] apr-cli setfit_train / predict / inspect / eval / serve test modules — 04-06, 04-07, 04-08
- [ ] aprender-serve oneshot suite — 04-08
- [ ] `contracts/setfit-apr-v1.yaml` + `$(CONTRACTS)` append + pv validation — 04-01
- [ ] Parity harness + goldens + SHA-256 manifest + in-band negative — 04-09
- [ ] Make targets with ran-something guards; feature matrix growth — 04-10

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| ci.yml matrix extension approval | SAFE-02 | CLAUDE.md forbids autonomous CI workflow edits; 04-11 Task 2 is a blocking human checkpoint | Review the prepared diff from 04-11 Task 1 (`git diff .github/workflows/ci.yml` staged as a prepared patch, uncommitted); approve or reject; only then does Task 3 apply it |

All other phase behaviors have automated verification.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (verified against 04-01..04-11 task blocks — 30/30 tasks carry `<automated>`)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify (every task has one; 04-11 Task 2's checkpoint is bracketed by automated evidence commands)
- [x] Wave 0 covers all MISSING references (tdd tasks create their own test files RED-first; no orphan MISSING markers exist in any plan)
- [x] No watch-mode flags (audited: no `--watch`/watch-mode invocation in any `<automated>` command)
- [x] Feedback latency < 120s (scoped per-crate filters; tier3 reserved for wave/phase gates)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-08-14
