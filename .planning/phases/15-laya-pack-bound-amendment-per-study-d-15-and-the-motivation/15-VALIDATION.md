---
phase: "15"
slug: "laya-pack-bound-amendment-per-study-d-15-and-the-motivation"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-10-31"
---

# Phase 15 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Source: `15-RESEARCH.md` § Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust libtest (`cargo test`), Python stdlib selftests (`laya_repro_eval.py --selftest`, `laya_fetch.py --selftest`, `laya_drift.py --selftest`), justfile selftests (`just laya-train-selftest`, `just laya-deploy-selftest`, `just laya-gates-selftest`), `pv` |
| **Config file** | `rust-toolchain.toml`, `scripts/laya_gates.tsv`, `scripts/laya_claims.tsv`, `contracts/aprender/binding.yaml` |
| **Quick run command** | `cargo test -p aprender-decide --lib verify::tests::noise` |
| **Full suite command** | `cargo test -p aprender-decide && just laya-train-selftest && just laya-deploy-selftest && python3 -I scripts/laya_deploy/laya_repro_eval.py --selftest && ./target/release/pv validate contracts/laya-parity-v1.yaml && just laya-claims-check` |
| **Estimated runtime** | ~18 s quick (incl. compile); full suite several minutes |

---

## Sampling Rate

- **After every task commit:** `cargo test -p aprender-decide --lib verify::tests::noise` (plus new `noise_per_set_*`); `./target/release/pv validate contracts/laya-parity-v1.yaml` when the contract changes; `cargo fmt --all -- --check`; `cargo clippy -p aprender-decide --lib -- -D warnings`
- **After every plan wave:** `cargo test -p aprender-decide`, `just laya-deploy-selftest`, `just laya-claims-check`, `python3 -I scripts/laya_deploy/laya_repro_eval.py --selftest`, `cargo test -p aprender-core --test readme_contract`
- **Before `/gsd-verify-work`:** the above plus `just laya-train-selftest` and `just laya-gap-regression` (or its documented subset on the main checkout)
- **Max feedback latency:** ~60 seconds per task commit

---

## Per-Task Verification Map

Filled in by the planner/executor per task. Criterion-level map:

| Criterion | Behavior | Test Type | Automated Command / Evidence | File Exists |
|-----------|----------|-----------|------------------------------|-------------|
| SC1 sign-off precedes edit | decision record commit is an ancestor of the first Phase 15 commit touching `contracts/laya-parity-v1.yaml` | git-order | `git merge-base --is-ancestor <decision-commit> <first-contract-commit>` | ❌ W0 |
| SC1 per-set bound valid | `pv validate` 0 errors, `pv lint` file form PASS, version bump matches `pv diff` | contract | `./target/release/pv validate contracts/laya-parity-v1.yaml` | ✅ |
| SC1 per-set enforcement | table-driven drift rows pass/fail per chosen params; v1 record still verifies; absent record still 1e-5 | unit | `cargo test -p aprender-decide --lib verify::tests::` | ❌ W0 |
| SC1 fail-closed vectors | no-record vectors still refused at 1e-5 (SKIP never counts as pass) | integration (armed) | `LAYA_FAIL_CLOSED_VECTORS=1 cargo test -p aprender-decide --release --test fail_closed_vectors` | ✅ |
| SC2 laya-verify role | recipe case table: record ok + Mac RescoreDrift proceeds with finding; other refusal / sha mismatch / accepted false refused; no record unchanged | recipe selftest | `just laya-deploy-selftest` | ❌ W0 |
| SC3 Graviton re-pack | `pack-outcome.json` accepted + deploy_eligible, digest + contract sha match the amended commit | evidence check | read-only check → `15-REPACK-EVIDENCE.json` | ❌ W0 |
| SC4 D-16 in band | `check-declaration` ok; `evaluate` class `pass`; `next_trusted` true; no waiver/override | evaluator | `python3 -I scripts/laya_deploy/laya_repro_eval.py check-declaration ...` / `evaluate ...` | ✅ script / ❌ W0 declaration |
| SC5 owner-run AWS only | owner replies on disk; DRY_RUN prints `AWS CALLS: 0` | process | owner-reply records in phase dir | manual |
| Docs coherence | CLAUDE.md bar sentence updated, cited paths exist | gate | `cargo test -p aprender-core --test readme_contract` | ✅ |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `15-BOUND-DECISION.*` record + git-order check for SC1
- [ ] `noise_per_set_*` Rust tests in `crates/aprender-decide/src/verify/tests.rs` (v1-record-still-valid, per-set ceiling, absent-record floor, swapped-field mutation)
- [ ] recipe case table for the D-08/D-09 gate in `laya-deploy-selftest` and `scripts/laya_gates.tsv`
- [ ] `15-REPACK-EVIDENCE.json` producer/check (new file; not `14-DRIFT-EVIDENCE.json`)
- [ ] `15-REPRODUCTION-DECLARATION.json` + `check-declaration` run
- [ ] baseline note: `pv lint contracts/` (directory form) fails Gate 10 at HEAD — record baseline and compare

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Owner sign-off on bound parameters | SC1 | owner decision (`checkpoint:decision`) | owner reply recorded verbatim on disk by orchestrator |
| SageMaker reproduction + Graviton pack | SC3, SC4, SC5 | paid AWS, owner-run with `!` prefix | owner runs staged commands; executor fetches and evaluates artifacts |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
