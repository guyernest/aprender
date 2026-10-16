---
phase: "09"
slug: "internal-openai-compatible-llm-endpoint-for-pmcp-run"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-10-15"
---

# Phase 9 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test / cargo nextest (Rust); `pv` (contracts); `bashrs` (scripts); stdlib Python probe under `python3 -I` (conformance); aws-cli `lambda invoke` (live) |
| **Config file** | workspace `Cargo.toml` lints; `.pmat-gates.toml`; `scripts/contract_test_binding_baseline.txt` |
| **Quick run command** | `cargo test -p aprender-serve --lib api::llm_endpoint_conformance_tests` and `cargo test -p apr-cli --lib gguf_router_auth_tests` |
| **Full suite command** | `cargo nextest run --profile ci --workspace --lib --exclude aprender-gpu --exclude aprender-cuda-edge --exclude aprender-compute` |
| **Estimated runtime** | quick filters: seconds once compiled; a whole crate suite (`-p aprender-serve --lib`, `-p apr-cli --lib`): minutes; image builds and live AWS calls: minutes |

---

## Sampling Rate

- **After every task commit:** Run the quick filter for the crate the task touched (`api::llm_endpoint_conformance_tests`, `chat_message_content_tests` or `gguf_router_auth_tests`); for script-only tasks, `bashrs lint` on the touched script and `just llm-endpoint-selftest` (AWS CALLS: 0)
- **After every plan wave:** Run the SC5 crate suites (`cargo test -p aprender-serve --lib`, `cargo test -p apr-cli --lib`) and the CI nextest list
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 s for the quick filters (check 8b). **Acknowledged exceptions (plan-check 2026-10-15, nyquist warning on plans 09-01/02/03/04/07/08/10):** whole-crate test suites, the workspace `nextest list`, `docker buildx` image builds and live AWS / curl calls exceed 30 s — inherent to a deploy phase and mirrors Phase 8; every such verify still captures `rc=$?` after a redirect and carries a `<fails_when>` sibling.

---

## Per-Task Verification Map

> The rows below are a template seed. The per-task map for this phase's 13 plans (09-01 .. 09-13, including 09-13) is filled by `/gsd-validate-phase 9` after execution, from each task's `<automated>` command and `<fails_when>` signal in the PLAN.md files; until then the plans' own verify blocks are the authoritative map.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 09-01-01 | 01 | 1 | REQ-{XX} | T-09-01 / — | {expected secure behavior or "N/A"} | unit | `{command}` | ✅ / ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `{tests/test_file.py}` — stubs for REQ-{XX}
- [ ] `{tests/conftest.py}` — shared fixtures
- [ ] `{framework install}` — if no framework detected

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| {behavior} | REQ-{XX} | {reason} | {steps} |

*If none: "All phase behaviors have automated verification."*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < {N}s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** {pending / approved YYYY-MM-DD}
