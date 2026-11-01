---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 07
subsystem: infra
tags: [laya, trust-bar, declaration, epochs-4, run-hold, owner-decision]
requires:
  - phase: 15-06
    provides: the Graviton re-pack accepted under the amended bound (SC3); the amended pack image dd38865c...
provides:
  - 15-DECLARATION-DRAFTS.json (laya-declaration-drafts-v1): epochs-4 and epochs-12-redraw, every MUST value computed offline
  - 15-DECLARATION-BRIEF.md: the owner's brief (sha256 eb6628fdeed47de371de9b5b23695ade96e79306ba4c4e90872640213592ef0d)
  - 15-REPRODUCTION-DECLARATION.json: the committed epochs-4 declaration, run_authorized false (run-hold), exactly one commit
affects: [15-08]
tech-stack:
  added: []
  patterns: [recipe id from the trainer's own builder with an attempt-1 positive control; declaration committed once and never edited]
key-files:
  created:
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-DECLARATION-DRAFTS.json
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-DECLARATION-BRIEF.md
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-REPRODUCTION-DECLARATION.json
  modified: []
key-decisions:
  - "Owner chose epochs-4 with run-hold (verbatim `epochs-4 run-hold`, recorded by the orchestrator in 15-DECLARATION-OWNER-REPLY.json, cbd156982): max epochs 12 -> 4, same data and seeds, recipe id a9f1c4a7176655664f391e3237a3205d1c9989155160edb2d78ef46808ba5087"
  - "Band [0.30, 0.45] margin and [0.07, 0.12] ECE, attempt cap 1, retry_classes [], waiver_rule inconclusive: unchanged (D-11, D-12, D-02); effective ECE window [0.07, 0.10]"
requirements-completed: [D-02, D-11, D-12, D-13, D-14, D-03, D-17]
duration: n/a
completed: 2026-11-01
status: complete
actuals:
  tokens: 14000
  tasks: 3
  commits: 3
plan_head_before: 7db4597ffa2f4cfc9ea0e60f3cdd056d8aeffec9
plan_head_after: c078910ba3bdccd65246019f797574e237b3cef5
---

# Phase 15 Plan 07: The trust-bar declaration Summary

**The single trust-bar attempt is declared and committed before any spend: epochs-4 (max epochs 12 -> 4, recipe id a9f1c4a7...), band and cap unchanged, and the owner replied `epochs-4 run-hold`, so `run_authorized` is false and no paid run is authorized yet.**

## Run-hold consequence for plan 15-08

Plan 15-08 must NOT run the paid attempt on this declaration alone. A later go exists only as a separate `15-RUN-GO.json` that the orchestrator records, bound to the sha256 of the declaration blob. The declaration has exactly one commit (c078910ba3bdccd65246019f797574e237b3cef5) and must never be edited: blob sha256 `1471c10638e78e924229f0038d50536b813190b89b81807a9b499452cd0e5f86`.

## Accomplishments

- Task 1 (tracer, offline): `contract.recipe_id_for` with the attempt-1 arguments reproduced f88f5b71... byte-for-byte (positive control); epochs 4 gives a9f1c4a7...; the three input hashes equal Phase 12's and Phase 14's. Both drafts passed `check-declaration`; the verify printed `DRAFTS OK epochs-4 a9f1c4a71766`. The brief carries the effective ECE window [0.07, 0.10], the `median_ece` shipped-seed framing (attempt 1 shipped its lowest-margin seed 17), the [ASSUMED] label on the epochs effect, the report-only note and the run-hold path.
- Task 2 (checkpoint, owner): `epochs-4 run-hold`, recorded alone by the orchestrator (cbd156982, brief_sha256 matches the brief on disk).
- Task 3: the chosen draft plus `owner_declaration_reply` (run_go false, recorded_commit cbd156982...) and `run_authorized` false; `DECLARATION OK`; tracked, working file equals HEAD blob; `DECLARATION COMMITTED epochs-4 run_authorized=False`, one commit touching the file.

## Task Commits

1. ff4adb7b8 - drafts and brief (Task 1)
2. cbd156982 - owner's reply (orchestrator-written, counted in `commits`)
3. c078910ba - the committed declaration (Task 3)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Task 3's "no overrides" check is a bare substring test**
- **Found during:** Task 3 verify
- **Issue:** `assert 'overrides' not in json.dumps(x)` trips on the waiver_note sentence "no waiver and no overrides entry stands in for the trust bar", which the plan itself requires ("D-02 stated"). The intent (T-15-29) is that no `overrides` KEY exists.
- **Fix:** ran the same verify with the check made structural (no `overrides` key at any depth). It passed. The declaration was not edited, since it must keep exactly one commit.
- **Files modified:** none (scratch verify script only)

Otherwise the plan executed as written.

## Issues Encountered

- `gsd_run query state.*` verbs may fail on 04-VERIFICATION.md status `gaps_acknowledged`; that file was not touched.

## Next Phase Readiness

Plan 15-08 is blocked on the owner: it needs an orchestrator-recorded `15-RUN-GO.json` bound to the declaration sha256 above before the single paid GPU run.

## Known Stubs

None.

## Threat Flags

None. No network, auth or schema surface was added; nothing was run on AWS, cdk, pmcp or GPU.

## Self-Check: PASSED

Files found: 15-DECLARATION-DRAFTS.json, 15-DECLARATION-BRIEF.md, 15-REPRODUCTION-DECLARATION.json. Commits ff4adb7b8, cbd156982 and c078910ba are ancestors of HEAD.
