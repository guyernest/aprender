---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 10
subsystem: infra
tags: [laya, gap-closure, sc4, owner-decision, mean-rule, disposition]
requires:
  - phase: 15-09
    provides: 15-GAP-DECISION-BRIEF.md, 15-GAP-CLASS-DIAGNOSIS.json, check-reply / write-disposition / check-disposition
provides:
  - 15-GAP-OWNER-REPLY.json (option mean-rule, run_go true; written by the orchestrator, commit 9c452408d)
  - 15-GAP-DISPOSITION.json (machine-derived; next_trusted false, trust_basis none)
affects: [15-11, 15-12]
key-files:
  created:
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GAP-OWNER-REPLY.json
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GAP-DISPOSITION.json
key-decisions:
  - "Owner chose mean-rule with run-go: the margin clause of a NEW run is judged on the mean of the three seed margins in [0.30, 0.45]; supersedes D-11 only. Not a waiver and not an override (D-02)."
  - "The disposition keeps next_trusted false and trust_basis none: nothing is trusted until a new run under a declaration committed first meets its declared band."
requirements-completed: [D-02, D-03, D-11, D-12, D-13, D-14]
duration: short
completed: 2026-11-01
status: complete
actuals:
  tokens: 4000
  tasks: 2
  commits: 2
plan_head_before: 78eb5586ac020c2769ce1dfb15a7ca8045ea27e5
plan_head_after: 0ffbe59c6abf6fb8ade4032eafee8f5016900319
---

# Phase 15 Plan 10: The owner's decision on the SC4 gap Summary

**The owner chose `mean-rule` and `run-go`: one new owner-run attempt in which the margin clause is judged on the mean of the three seed margins against the unchanged band [0.30, 0.45], superseding D-11 only; the gap disposition is machine-derived with next_trusted false, and plan 15-11 commits the declaration next.**

## Owner reply (verbatim)

Task 1 (the owner checkpoint) was completed by the orchestrator, which recorded the reply in `15-GAP-OWNER-REPLY.json` in commit `9c452408d`, touching only that file. The executor did not write or edit it. The owner's reply, as recorded in `owner_reply_verbatim`:

> mean-rule; run flag asked separately, answered: run-go

That is, the owner replied `"mean-rule"` and then, asked separately for the run flag, `"run-go"`. The fields read by the checks are `option: "mean-rule"`, `run_go: true`, `supersedes: ["D-11"]`, `brief_sha256: 3cba575347e3b08a5bac192e7227b56f14c0b2715bf8de5fcf607f0ef1ccf863`.

## What was done (Task 2)

- Precondition: the reply was added in exactly one commit (`9c452408d`) touching only the reply file, `recorded_by` orchestrator. `REPLY COMMITTED ALONE`.
- `check-reply` printed `REPLY OK option=mean-rule run_go=true` (brief hash matches the brief on disk, supersedes equals the option's list in the diagnosis).
- `write-disposition` produced `15-GAP-DISPOSITION.json` (`DISPOSITION WRITTEN option=mean-rule next_trusted=false`); `check-disposition` printed `DISPOSITION OK option=mean-rule next_trusted=false`. The file was not hand-edited.
- Structure check: `DISPOSITION STRUCTURE OK option=mean-rule paid_run_planned=True`; no `waiver`/`overrides` key at any depth; both consumed attempts (Phase 14 attempt 1, shipped margin 0.2860; Phase 15 attempt 1, shipped margin 0.2795) carry `rejudged_for_trust: false`.
- `NOTHING ELSE EDITED`: contracts, .github, crates, the evaluator, the consumed declaration and the gate-run evidence are identical to HEAD.
- The disposition was committed alone as `0ffbe59c6`.
- No AWS, SageMaker, Fargate, pmcp or train call was made (D-03).

## Paid-option branch: what plan 15-11 will declare

- **Option:** mean-rule. **run_go:** true. **supersedes:** `["D-11"]` (D-12 stands: attempt cap 1). The change applies to a NEW run only; the two consumed attempts are not re-judged for trust.
- **Rule values (from the disposition's `rule` block):**
  - judging `seed_mean`: margin clause on the mean of the three seed margins in [0.30, 0.45] (numbers unchanged)
  - ECE clause band [0.07, 0.12] and the gate (shipped ECE at most 0.10) stay on the shipped seed
  - attempt_cap 1, retry_classes none, epochs 12, dataset prefix `datasets/motivation-v1/`, run name `motivation-v1-p15b`
  - recipe_id `f88f5b71ef925fdabb30d26a15c2eb5195a151dadfe0882c858b73ca7bf54d7e`
  - p_pass 0.6343 (resampling estimate over six seed-runs, not a confidence interval; leave-one-out range 0.464 to 0.736)
  - cost, report only: about 1346 s of ml.g5.xlarge plus about 150 s of Fargate pack
  - claim scope: gate outcome and band only; not an identical reproduction of attempt 1 or of Phase 12
- **Next:** plan 15-11 commits the declaration (copying supersedes and the rule block, not retyping them) before any paid run; plan 15-12 is the single owner-run attempt that run_go true authorizes. The executor of this plan launched nothing.
- **State of the claim:** SC4 stays unmet until a new run under that declaration meets its declared band; the Phase 14 training server stays untrusted (`next_trusted false`, `trust_basis none`).

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- State tooling: the `state.*` verbs fail on Phase 4's `04-VERIFICATION.md` status `gaps_acknowledged` (outside the tool's closed set), as already known. `04-VERIFICATION.md` was not re-keyed and STATE.md was not updated by the verbs; its position, metrics and session fields do not yet reflect plans 15-09 and 15-10 and need the orchestrator's attention. See the return message for the exact outcomes of each verb.
- `.planning/config.json` and `.planning/state.json` carried pre-existing uncommitted edits and were not touched.

## Known Stubs

None.

## Threat Flags

None. No new network, auth or file-access surface; only two JSON records were added.

## Self-Check: PASSED

`15-GAP-OWNER-REPLY.json`, `15-GAP-DISPOSITION.json` and this summary exist; commits `9c452408d` and `0ffbe59c6` are ancestors of HEAD; `commits: 2` was measured from base `78eb5586a` to `0ffbe59c6`.
