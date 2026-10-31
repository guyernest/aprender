---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 04
subsystem: decide
tags: [laya, amendment-a2, offline-gate, gap-regression, claims-ledger, contract-hygiene]
requires:
  - phase: 15-01
    provides: 15-BOUND-DECISION.json and the SC1 anchor 91969e11e
  - phase: 15-02
    provides: the per-set A2 verifier and laya-parity-v1 3.0.0
  - phase: 15-03
    provides: the pack-record eligibility helper and the repaired Phase 8 binding audit
provides:
  - 15-OFFLINE-GATE-EVIDENCE.json (verify-suite, claims, ratchet, graph, early gates, gap-regression, SC1 list, image_head)
  - CLAUDE.md states amendment A2 and the role of the local laya-verify
  - three claims rows binding that sentence, FALSIFY-LAYA-PARITY-007 and FALSIFY-DECIDE-APR-015
  - a green corpus-hygiene gate (pre-existing defect repaired inline)
affects: [15-05, 15-06]
tech-stack:
  added: []
  patterns: [evidence JSON carries the first failed sweep and the repaired per-class runs side by side]
key-files:
  created:
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-OFFLINE-GATE-EVIDENCE.json
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/deferred-items.md
  modified:
    - CLAUDE.md
    - scripts/laya_claims.tsv
    - contracts/contracts.nt
    - contracts/llm-endpoint-openai-conformance-v1.yaml
    - contracts/motivation-enrichment-v1.yaml
key-decisions:
  - "The contract graph was stale (the 15-03 binding row added a Symbol): regenerated from a clean worktree, contracts.nt +15 triples, shapes.ttl unchanged"
  - "The pre-existing corpus-hygiene failure was repaired inline on the user's choice (option a), in two separate commits, without touching the data-integrity ceiling or any baseline"
requirements-completed: [D-01, D-05, D-08, D-10, D-17]
duration: n/a
completed: 2026-10-31
status: complete
actuals:
  tokens: 30000
  tasks: 2
  commits: 7
plan_head_before: 8a5e8642825daa7e010edfd0c197fb9b8e8c2638
plan_head_after: fb3454c3b7ec77657c390303269915ff3bc2db47
---

# Phase 15 Plan 04: Offline gate on real weights, A2 docs, clean closure Summary

**The untouched trainer's records verify under the amended per-set bound on real weights (enforced 5e-5 fine_tuned, 3e-5 zero_shot against stamps of 1e-5), the whole Laya regression is green, CLAUDE.md and the claims ledger say what the code enforces, and HEAD 9b5e17101 is the committed, clean tree the pack image is built from.**

## Commits

| Task | Commit | What |
|------|--------|------|
| 1 (tracer) | c9f735278 | evidence with the verify_suite block |
| 2 | ce6e91a8a | contracts.nt regenerated (+15 triples, `decide_pilot_waiver`) |
| 2 | 364f12a6a | CLAUDE.md A2 sentence; three claims rows |
| 2 | 0bab85a5f | evidence (blocked state) and deferred-items.md |
| repair | 49a8df031 | pre-existing-defect repair 1: `prose: true` on two U+2717 obligations |
| repair | 9b5e17101 | pre-existing-defect repair 2: one equation in motivation-enrichment-v1 |
| 2 | fb3454c3b | evidence complete |

All descend from the sign-off commit 91969e11e.

## Verification results

- `just laya-verify-suite` rc 0 `LAYA VERIFY SUITE OK`; legs laya_parity, fail_closed_vectors, demo_run, python_records; `FAIL-CLOSED VECTORS REFUSED 2/2 (387 s, ARCH aarch64)`.
- `A2 ON PYTHON RECORDS OK 2 lines`: fine_tuned rust=4.299e-8 stamp=1e-5 bound=5e-5; zero_shot rust=3.106e-8 stamp=1e-5 bound=3e-5.
- `LAYA CLAIMS OK 147 rows`; readme_contract 16 passed; ratchet PASS (also after the repair); EARLY GATES OK (`LAYA GATES ROWS OK 1 of 56`, role `role-drift-advisory`).
- Gap regression: first full sweep (364f12a6a) passed A, B, C (including `contract-audit-phase8`), D, E and failed HYGIENE `pv-lint`. After the repair, own runs at 9b5e17101: HYGIENE PASS (pv-lint PASS, graph-fresh, contracts-lib 1671, contracts-corpus 10), E PASS, D PASS, REGRESSION PASS (contracts-cli 394 passed, verify-suite, `laya-verify` of the deployed artifact deploy_eligible true). Evidence: `gap_regression.classes` all seven rc 0, `pre_existing` empty.
- `OFFLINE GATE OK; SC1 PHASE-WIDE OK 10 commits`; image context clean; `image_head` 9b5e17101.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocker, user-authorised repair] Pre-existing corpus-hygiene failure**
- **Found during:** Task 2 (gap regression HYGIENE `pv-lint`, then `contracts-lib`, `contracts-corpus`, and REGRESSION `contracts-cli`)
- **Issue:** red identically at dd3c96937. (a) two `PV-ONT-003` errors: `proof_obligations[2]` and `[3]` of `contracts/llm-endpoint-openai-conformance-v1.yaml` carry U+2717, which Sigma does not declare (Phase 9, noted in 12-02 and 13-02 summaries); (b) `contract_data_integrity` 445 against a ceiling of 444: the Phase 13 schema contract `motivation-enrichment-v1` had no equation. Later stages of the same classes also failed, so the plan's exemption did not apply; the owner chose option (a), repair inline.
- **Fix:** (a) `prose: true` on the two obligations, the pattern `contracts/ontology.yaml` prescribes for undeclared glyphs; no obligation's wording changed. (b) one equation in motivation-enrichment-v1 restating its own `no_text_rule`; no rule, bound or value of either format changed. The ceiling in `validate_contracts.rs` and every baseline file are untouched.
- **Files modified:** contracts/llm-endpoint-openai-conformance-v1.yaml, contracts/motivation-enrichment-v1.yaml (both outside this plan's files_modified list, authorised by the owner)
- **Verification:** `pv lint contracts` 0 errors, `Result: PASS`, `armed meet: Pass`; `validate_contracts` 10 passed; `pv extract contracts --check` clean on a worktree of 9b5e17101 (the edits do not change the graph); Class E (reads motivation-enrichment-v1) PASS.
- **Commits:** 49a8df031, 9b5e17101

**2. [Plan wording] Python-records NOISE filter** — the check applies `kv['rust']` to every `NOISE which=` line; the torch-lifecycle legs print another format without `rust=`. `noise_lines` holds the python_records lines (` rust=`), the others are kept as `lifecycle_noise_lines`.

**3. [Plan wording] Class runs** — after the failed first sweep, classes A, B, C (which ran and printed PASS before the first failure) are recorded from that sweep with a `source` note, not re-run alone; HYGIENE, REGRESSION, D and E were run on their own after the repair.

**4. [Evidence ordering]** `image_head` is the last commit that touches the image context (9b5e17101); the evidence and this SUMMARY commit only `.planning/`.

## Authentication gates

None.

## Known Stubs

None.

## Threat Flags

None. The only contract edits are the two repairs above (no new surface).

## Self-Check: PASSED

Evidence JSON, deferred-items.md, CLAUDE.md sentence (`amendment A2`), the three claims rows and the commits c9f735278, ce6e91a8a, 364f12a6a, 0bab85a5f, 49a8df031, 9b5e17101, fb3454c3b all exist on HEAD's ancestry.
