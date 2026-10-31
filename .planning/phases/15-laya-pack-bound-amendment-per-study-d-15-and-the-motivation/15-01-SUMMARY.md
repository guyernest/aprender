---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 01
subsystem: decide
tags: [laya, parity-contract, pack-bound, owner-sign-off, evidence]
requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-DRIFT-EVIDENCE.json and 14-GATE-RUN-EVIDENCE.json (Graviton and Mac drift cells, A10G noise records)
provides:
  - 15-BOUND-CANDIDATES.json (12 cells, 4 recomputed option grids, red line, limits, roles, SC1 sign_off anchor)
  - 15-BOUND-DECISION-BRIEF.md (the owner's brief)
  - 15-BOUND-DECISION.json (owner sign-off, recorded by the orchestrator)
affects: [15-02, 15-03, 15-06, 15-07, 15-08]
tech-stack:
  added: []
  patterns: [dotted-key re-resolution of every cited number, grid verdict modelling production acceptance]
key-files:
  created:
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-BOUND-CANDIDATES.json
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-BOUND-DECISION-BRIEF.md
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-BOUND-DECISION.json
  modified: []
key-decisions:
  - "Owner signed sb-recommended (S-B): fine_tuned k 4 / floor 5.0e-5 / ceiling 1.0e-3; zero_shot k 4 / floor 3.0e-5 / ceiling 1.0e-3"
  - "Owner signed role-drift-advisory for the local laya-verify: only RescoreDrift is demoted to a logged finding when a valid pack record exists"
  - "Structure S-B (additive per-set constants beside the A1 stamp scalars); S-A (record v2) rejected"
requirements-completed: [D-01, D-03, D-04, D-05, D-06, D-07, D-08, D-17]
duration: n/a
completed: 2026-10-31
status: complete
actuals:
  tokens: 30000
  tasks: 3
  commits: 3
plan_head_before: f426582a396417b30e972896d0ceb400afb254bd
plan_head_after: 0acac1aa8598bbaca9a79aa49f776d7aa31fa167
---

# Phase 15 Plan 01: Owner's per-set pack bound decision Summary

**The owner signed the S-B per-set re-score bound (fine_tuned floor 5.0e-5, zero_shot floor 3.0e-5, ceiling 1.0e-3, k 4) and the drift-advisory role for the local laya-verify, from a brief whose 12 cells and 48 grid verdicts all re-resolve to Phase 14 evidence; the sign-off commit 91969e11e is pinned as the SC1 anchor.**

## What was done

- **Task 1 (tracer, 480de4a8a):** generated `15-BOUND-CANDIDATES.json` (schema `laya-bound-candidates-v1`) and `15-BOUND-DECISION-BRIEF.md` from `14-DRIFT-EVIDENCE.json` by recorded dotted keys. Four option grids (a1-today, sb-recommended, sb-minimal, sb-uniform) each model policy validity, the A1 stamp ceiling, the per-set enforced ceiling, then drift. The verify printed `BOUND BRIEF OK points=12 grid=48`. a1-today fails Graviton motivation-v1-sm zero_shot (1.7285e-05 against 1e-05); sb-recommended passes all four Graviton cells (headroom 1.74x, 1.82x, 1.99x, at least 3.84x on the upper-bounded motivation fine_tuned cell).
- **Task 2 (checkpoint:decision, blocking-human):** owner reply `sb-recommended role-drift-advisory`, recorded verbatim by the orchestrator in `15-BOUND-DECISION.json` and committed alone as 91969e11e. No executor wrote or relayed it.
- **Task 3 (0acac1aa8):** validated the record (orchestrator-recorded, brief hash equals the brief on disk, params equal the named option, all measured Graviton cells pass) and added `sign_off` to `15-BOUND-CANDIDATES.json`: decision_commit 91969e11e, role-drift-advisory consequences table, empty warnings. Verify printed `SIGN-OFF OK sb-recommended role-drift-advisory 91969e11ee25`.

## Red line (as signed)

Floor headroom of the largest floor (5.0e-5) to the smallest cited porting regression (1.4e-2) is 280x. The 1.0e-3 ceiling the option still permits is only 14x below 1.4e-2. For CUDA-trained sets no measured A10G noise reaches the noise term (engage levels 1.25e-5 fine_tuned, 7.5e-6 zero_shot), so for CUDA runs the bound is in effect the absolute floor. Only MPS-trained Mac artifacts (M-mps-2, -3, -5, -7) reach the noise term. The region between the bound and 1.4e-2 is unmeasured.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None in the plan's scope. `sign_off.warnings` is empty (named option, not custom).

## Authentication gates

None.

## Known Stubs

None.

## Threat Flags

None. No file outside the plan's `files_modified` was touched; `contracts/laya-parity-v1.yaml`, `contracts/decide-apr-v1.yaml`, `contracts/aprender/binding.yaml`, `crates/aprender-decide` and `justfile` are byte-identical to dd3c96937 (asserted by `git diff --quiet`). `commits: 3` counts the orchestrator's sign-off commit 91969e11e, which falls in the measured range.

## Next Phase Readiness

Plans 15-02 and 15-03 may now edit the contract and the recipes; every such commit must descend from 91969e11e (`sign_off.decision_commit`). The signature authorizes only those edits, no paid run.

## Self-Check: PASSED
