---
phase: 2
slug: deterministic-pair-and-data-protocol
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-08-08
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) + proptest + trybuild + pv (contract validation) |
| **Config file** | Cargo.toml (workspace) / .pmat-gates.toml |
| **Quick run command** | `cargo test -p <crate-under-change> --lib` |
| **Full suite command** | `cargo test --workspace --lib && pv validate contracts/*.yaml` |
| **Estimated runtime** | ~60 seconds (quick), ~5 min (full) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p <crate-under-change> --lib`
- **After every plan wave:** Run `cargo test --workspace --lib && pv validate contracts/*.yaml`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| (filled by planner) | — | — | DATA-01..DATA-06 | — | — | unit/property/contract | — | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Commit the in-flight D-06 baseline work currently uncommitted on the branch (data_tweeteval.rs, eval diff, contract, docs example)
- [ ] Restructure `contracts/tweet-eval-stance-benchmark-v1.yaml` so `pv validate` passes (PROVABILITY-001: add proof_obligations/kani_harnesses per `setfit-encoder-conformance-v1.yaml` shape precedent)
- [ ] Append phase contracts to the Makefile `$(CONTRACTS)` explicit list so tier3 exercises them

*Existing cargo test infrastructure covers unit/property testing; contract gates need the Wave 0 items above.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Pinned TweetEval source acquisition from network | DATA-01 | Requires network fetch of pinned upstream source | Run acquisition command against pinned revision; verify SHA-256 hashes match manifest |

*All other phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
