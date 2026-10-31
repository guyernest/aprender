# Phase 15 seed: what Phase 15 inherits from Phase 14

This file is the hand-off from Phase 14 (Laya training MCP server) to Phase 15 (pack-bound amendment per study, D-15, and the motivation-v1 re-reproduction, D-16). It is built from `14-VERIFICATION.md` (verified 2026-10-30T23:59:00Z, gaps_found, 13/16), `14-GAP-ROUND-2-OWNER-DECISIONS.md`, `14-DRIFT-EVIDENCE.json`, `14-GATE-RUN-EVIDENCE.json`, `14-REPRODUCTION-DECLARATION.json` and `14-REVIEW-DISPOSITION.md`, all in `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/`. It states facts and consequences and recommends no bound parameter: the Phase 15 planner argues the bound and the owner signs off at a `checkpoint:decision` before any contract edit (owner answer 3). Phase 14 truth 9 (D-22, D-05) stays true because no Phase 14 plan edits `contracts/`, a decide crate or a decide server. It is not named CONTEXT, RESEARCH or PLAN on purpose, so Phase 15's discuss and plan steps do not mistake it for their own artifacts.

## Inherited truths

### Truth 12 (gaps[0], failed): covered by Phase 15 success criterion 4

D-16 trust bar: the server's first real gate run reproduces motivation-v1 (passes the gate and lands in the owner-approved band margin [0.30, 0.45], ECE [0.07, 0.12], with an accepted deploy-eligible .apr) before the server is trusted

Missing, verbatim from 14-VERIFICATION.md:

A reproduction that lands in band with an accepted, deploy-eligible .apr (needs the D-15 amendment first or a new declaration), OR an explicit owner waiver of D-16 recorded as an overrides: entry in this file

Its waiver branch is closed by owner answer 2: no waiver and no override entry. Covered by Phase 15 success criterion 4.

### Truth 13 (gaps[1], partial): covered by Phase 15 success criteria 1, 2 and 3

A gate-passing CUDA-trained checkpoint yields a deploy-eligible .apr from the arm64 pack step

Missing, verbatim from 14-VERIFICATION.md:

The planned amend-per-study change set: a per-set (fine_tuned, zero_shot) parity-bound amendment argued from 14-DRIFT-EVIDENCE.json, plus a stated role for the local laya-verify (owner-only, D-15)

A roadmap entry (new phase or inserted decimal phase) that owns it, because no later phase exists to defer to

The roadmap-entry item is met by the Phase 15 entry in ROADMAP.md. The amendment item is Phase 15 success criteria 1 and 2; the deploy-eligible `.apr` is criterion 3.

### Truth 8 (behavior_unverified_items[0]): a candidate for the owner, not a Phase 15 criterion

Cancelling a task while its arm64 pack/verify task is really packing stops the pack task, releases the single-flight lock and ends the run cancelled

This stays open in 14-VERIFICATION.md human_verification. It is not a Phase 15 success criterion: see the Candidates for the owner section for why a cancel on the D-16 run's own pack would end that run with no `.apr`.
