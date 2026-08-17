---
phase: 5
slug: benchmark-and-claims-gate
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-16
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (libtest) + trybuild + pv contract validation |
| **Config file** | Cargo.toml (workspace), Makefile tier targets |
| **Quick run command** | `cargo test -p aprender-train --lib --features setfit setfit::` |
| **Full suite command** | `make setfit-all-tests && pv validate contracts/setfit-benchmark-claims-v1.yaml` |
| **Estimated runtime** | ~120 seconds (quick), ~5 min (full) |

---

## Sampling Rate

- **After every task commit:** Run the quick run command
- **After every plan wave:** Run the full suite command
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 300 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| (filled by planner from PLAN.md tasks) | | | | | | | | | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*(filled by planner — see RESEARCH.md "## Validation Architecture" for the per-criterion validation plan: calibration-probe harness, in-band negative row sets, scipy/sklearn reference fixtures for t/ECE/Brier, spawned CLI ladder extension)*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Contract-edit approval (D-04) | EVAL-02..05 unblock | Human checkpoint by explicit ruling | Executor presents pv diff + measured thresholds; approve before commit |
| lambda-vector GPU runs | EVAL-02/03/05 | Remote host access + compute authorization | Confirm SSH access and 9B base-weight location before the LoRA wave |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 300s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
