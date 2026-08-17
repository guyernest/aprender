---
phase: 5
slug: benchmark-and-claims-gate
status: ready
nyquist_compliant: true
wave_0_complete: false
created: 2026-08-16
updated: 2026-08-16
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (libtest) + pv contract validation + bashrs + spawned-binary tests |
| **Config file** | Cargo.toml (workspace), Makefile tier targets |
| **Quick run command** | `CARGO_INCREMENTAL=0 cargo test -p aprender-train --lib --features setfit setfit::` |
| **Full suite command** | `make setfit-all-tests && make setfit-bench-tests && make contract-audit-phase5 && target/release/pv validate contracts/setfit-benchmark-claims-v1.yaml` |
| **Estimated runtime** | ~120 s (quick), ~5 min (full; excludes the deliberate compute runs in 05-01/05-11/05-12) |

---

## Sampling Rate

- **After every task commit:** Run the task's `<automated>` verify (each < 60 s except the flagged calibration/cell runs)
- **After every plan wave:** Run the full suite command
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 300 seconds (calibration matrix and 40-cell runs are contracted exceptions with their own compute gates)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 05-01.T1 | 05-01 | 1 | EVAL-02 | T-05-01-04 | probe before matrix; frozen E/B recorded | ignored harness (probe mode) | `APRENDER_CALIBRATION_PROBE=1 cargo test -p aprender-train --lib --features setfit production_calibration -- --ignored --nocapture` | ❌ Wave 0 | ⬜ pending |
| 05-01.T2 | 05-01 | 1 | EVAL-02 | T-05-01-02 | ctrl_max < real_min per class per cell | ignored harness (full matrix) | same, probe var unset | ❌ Wave 0 | ⬜ pending |
| 05-01.T3 | 05-01 | 1 | EVAL-02 | T-05-01-01 | entry byte-copied from measured id | doc assertion (grep) | `grep -c 'seeds=13,17,...' 05-01-calibration-measurements.md` | ❌ Wave 0 | ⬜ pending |
| 05-02.T1 | 05-02 | 1 | EVAL-02 | T-05-02-03 | one source for regimes + tables | unit | `cargo test -p aprender-train --lib --features setfit thresholds` | ◐ edits existing | ⬜ pending |
| 05-02.T2 | 05-02 | 1 | EVAL-02 | T-05-02-01 | single table_for lookup; induced-mutation control | unit + mutation control | `cargo test -p aprender-train --lib --features setfit setfit::` | ◐ edits existing | ⬜ pending |
| 05-03.T1 | 05-03 | 2 | EVAL-02 | T-05-03-01/02/03 | additive edit; envelope covered; NO commit | unit + pv | `cargo test ... thresholds` + `pv validate` + `pv diff` | ◐ edits existing | ⬜ pending |
| 05-03.T2 | 05-03 | 2 | EVAL-02 | T-05-03-04 | human approval before commit (D-04) | checkpoint (human) | — (human-check) | — | ⬜ pending |
| 05-03.T3 | 05-03 | 2 | EVAL-02 | T-05-03-01 | one commit; both gate directions | unit + audit | `cargo test ... setfit::` + `make contract-audit-phase4` | ◐ edits existing | ⬜ pending |
| 05-04.T1 | 05-04 | 2 | EVAL-01/04 | T-05-04-01 | manifested fixtures; A1/A3/A4 resolved | generator self-verify | `uv run python gen_claims_fixtures.py` + `shasum -c` | ❌ Wave 0 | ⬜ pending |
| 05-04.T2 | 05-04 | 2 | EVAL-01 | T-05-04-01 | fixture-parity ECE/Brier, contract-bound | unit (tdd) | `cargo test -p aprender-core --lib calibration` | ◐ edits existing | ⬜ pending |
| 05-04.T3 | 05-04 | 2 | EVAL-04 | T-05-04-02/03 | frozen t verified vs scipy; no RNG | unit (tdd) | `cargo test -p aprender-core --lib stats::` | ◐ edits existing | ⬜ pending |
| 05-05.T1 | 05-05 | 2 | EVAL-03/04 | T-05-05-02 | contract pv-valid + gate can fail | pv + induced red | `pv validate contracts/setfit-benchmark-claims-v1.yaml` | ❌ Wave 0 | ⬜ pending |
| 05-05.T2 | 05-05 | 2 | EVAL-03 | T-05-05-01/03/04/05 | 5 distinct refusals; 80-cell parity | unit (tdd) | `cargo test -p aprender-train --lib --features setfit bench_row` | ❌ Wave 0 | ⬜ pending |
| 05-06.T1 | 05-06 | 2 | EVAL-02 | T-05-06-01 | manifest->Selection door; hardcodes controllable | check + unit | `cargo check -p apr-cli --features setfit` | ◐ edits existing | ⬜ pending |
| 05-06.T2 | 05-06 | 2 | EVAL-02 | T-05-06-02 | A6 probed; epochs_completed exposed; refusals | unit (tdd) | `cargo test -p aprender-train --lib classify_trainer` + `cargo test -p apr-cli --lib --features setfit finetune` | ◐ edits existing | ⬜ pending |
| 05-07.T1 | 05-07 | 3 | EVAL-02 | T-05-07-01/03 | spawned production ladder; one sha across surfaces | spawned integration | `cargo test -p apr-cli --test setfit_cli_lifecycle --features setfit` | ◐ extends existing | ⬜ pending |
| 05-07.T2 | 05-07 | 3 | EVAL-02 | T-05-07-02/04 | prose honest; zero assertion changes; floors re-run | suites + grep | both scoped lib suites | ◐ edits existing | ⬜ pending |
| 05-08.T1 | 05-08 | 3 | EVAL-01 | T-05-08-01 | credential-gated door; scalar consistency | unit (tdd) | `cargo test -p aprender-train --lib --features setfit apr_evaluate` | ❌ Wave 0 | ⬜ pending |
| 05-08.T2 | 05-08 | 3 | EVAL-01 | T-05-08-02/03/04 | ordered labels; validation-only calibration | unit (tdd) | `cargo test -p aprender-train --lib --features setfit bench_metrics` | ❌ Wave 0 | ⬜ pending |
| 05-09.T1 | 05-09 | 4 | EVAL-05 | T-05-09-04 | contracted resource protocol helpers | unit | `cargo test -p apr-cli --lib --features setfit setfit_bench` | ❌ Wave 0 | ⬜ pending |
| 05-09.T2 | 05-09 | 4 | EVAL-03 | T-05-09-02/03/05 | library doors only; --record verified | unit | same filter | ❌ Wave 0 | ⬜ pending |
| 05-09.T3 | 05-09 | 4 | EVAL-03/05 | T-05-09-01 | executed identity; lint-clean driver | bashrs + check | `bashrs lint scripts/run_bench_cells.sh` + `cargo check -p apr-cli --features setfit` | ❌ Wave 0 | ⬜ pending |
| 05-10.T1 | 05-10 | 5 | EVAL-04 | T-05-10-01/02/03/04 | 5 negatives refused in every cargo test; deterministic aggregate | unit (tdd) | `cargo test -p aprender-train --lib --features setfit bench_gate` | ❌ Wave 0 | ⬜ pending |
| 05-10.T2 | 05-10 | 5 | EVAL-04 | T-05-10-05 | estimation-first rendering; refusal = no tables | unit | `cargo test -p apr-cli --lib --features setfit setfit_bench` | ◐ extends 05-09 | ⬜ pending |
| 05-10.T3 | 05-10 | 5 | EVAL-04 | T-05-10-01 | floors measured; two induced reds reverted | make gates | `make setfit-bench-tests` + `make contract-audit-phase5` | ❌ Wave 0 | ⬜ pending |
| 05-11.T1 | 05-11 | 5 | EVAL-02 | T-05-11-01 | probe-first; A1 resolved at checkpoint | checkpoint (human) | — (human-check; ssh probes automated) | — | ⬜ pending |
| 05-11.T2 | 05-11 | 5 | EVAL-02/03/05 | T-05-11-02/03/04/05 | same-SHA remote; digest-verified ingest x40 | dispatch + record | `ls rows/lora-*.json | wc -l` == 40 + bashrs lint | ❌ Wave 0 | ⬜ pending |
| 05-12.T1 | 05-12 | 6 | EVAL-02/03/05 | T-05-12-01/04 | pre-authorized compute; reloaded-artifact cells | driver run | `ls rows/setfit-*.json | wc -l` == 40 | ❌ Wave 0 | ⬜ pending |
| 05-12.T2 | 05-12 | 6 | EVAL-02 | T-05-12-02/03 | 80/80 manifest; pairing spot-check; text-free | json count + grep | manifest complete-count == 80 | ❌ Wave 0 | ⬜ pending |
| 05-13.T1 | 05-13 | 7 | EVAL-04/05 | T-05-13-01/02 | gate green on real set; bit-identical re-run | spawned report + cmp | report.json/md exist; no "significant" | ❌ Wave 0 | ⬜ pending |
| 05-13.T2 | 05-13 | 7 | (D-11) | — | refusal names remedy; exit code unchanged | unit | `cargo test -p apr-cli --lib qa` | ◐ edits existing | ⬜ pending |
| 05-13.T3 | 05-13 | 7 | EVAL-01..05 | T-05-13-03 | closing audits; no checkbox flips | make + pv | `make setfit-bench-tests` + `make contract-audit-phase5` | — | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Mapped from RESEARCH.md "Wave 0 Gaps" to owning plans (all covered):

- [ ] Production calibration harness (`production_calibration_matrix`, env-gated, `#[ignore]`d) — **05-01**
- [ ] Per-regime `Thresholds` restructuring + updated `thresholds_match_the_contract` — **05-02** (structure) + **05-03** (the len 1→2 edit)
- [ ] `contracts/setfit-benchmark-claims-v1.yaml` + `$(CONTRACTS)` append + scoped audit target — **05-05** (+ binding rows in **05-10**)
- [ ] Multiclass ECE/Brier in `aprender-core::calibration` + uv-env fixture generator + SHA-256 manifest — **05-04**
- [ ] f64 paired-stats helpers + frozen `t_{0.975,9}` + scipy fixtures — **05-04**
- [ ] Row type + run manifest + doctored-row negatives — **05-05** (types) + **05-10** (five negatives)
- [ ] `SetfitCommands::Bench` + `commands/setfit_bench.rs` + Make/tier wiring (non-vacuous) — **05-09** (run) + **05-10** (report + gates)
- [ ] `--selection-manifest` on finetune + explicit `TrainingConfig` control + refusal tests — **05-06**
- [ ] Per-row-predictions evaluator door beside `evaluate_validation_from_artifact` — **05-08**

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Contract-edit approval (D-04) | EVAL-02..05 unblock | Human checkpoint by explicit ruling | 05-03.T2: executor presents pv diff + measured thresholds + MEASURED/COVERED margins; approve before commit |
| lambda-vector GPU access + 9B base weights (A1) | EVAL-02/03/05 | Remote host credentials + weight provenance | 05-11.T1: automation-first ssh probes presented; human supplies access, confirms weight hash, decides Q6 SetFit host |
| >1hr compute authorizations | CLAUDE.md rule | Compute-budget decisions reserved for the human | 05-01.T1 (matrix projection), 05-03.T2 (05-12 pre-auth), 05-12.T1 (fallback gate) |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (checkpoints carry `<human-check>`)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 300s (compute runs excepted via contracted gates)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending execution
