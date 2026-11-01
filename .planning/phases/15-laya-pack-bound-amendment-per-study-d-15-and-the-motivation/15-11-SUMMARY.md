---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 11
subsystem: infra
tags: [laya, gap-closure, sc4, mean-rule, declaration, evaluator, owner-command-pack]
requires:
  - phase: 15-10
    provides: 15-GAP-OWNER-REPLY.json (mean-rule, run_go true), 15-GAP-DISPOSITION.json (paid_run_planned true)
provides:
  - laya_repro_eval.py judges an optional, validated judging.margin_statistic (shipped_seed default, seed_mean), no existing verdict moved
  - 15-GAP-REPRODUCTION-DECLARATION.json, owner-signed, committed alone before any paid run (blob sha256 a57165fc867e..., commit e06b59ade)
  - 15-GAP-RUN-EVIDENCE.json, status awaiting-owner-run, owner command pack V R S F H, control evaluation, failure-path controls
  - laya_margin_diagnosis.py scan-identifiers and classify-run subcommands
affects: [15-12]
key-files:
  created:
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GAP-REPRODUCTION-DECLARATION.json
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GAP-RUN-EVIDENCE.json
  modified:
    - scripts/laya_deploy/laya_repro_eval.py
    - scripts/laya_deploy/laya_margin_diagnosis.py
key-decisions:
  - "The judged statistic is the mean of the three seed margins in [0.30, 0.45] (D-11 superseded, the owner's mean-rule); the ECE clause, the gate and the pack stay on the shipped seed. Attempt cap 1, no retry class, run motivation-v1-p15b, 12 epochs, recipe f88f5b71...."
  - "An unknown judging key or statistic is a DeclarationRefused: a declared clause can never be silently ignored (the current evaluator ignores every unknown top-level key)."
  - "next_trusted stays false and authorization is null; plan 15-12 fills authorization. Nothing here is trust, a waiver or an override (D-02)."
requirements-completed: [D-02, D-03, D-11, D-12, D-13, D-14, D-17]
duration: short
completed: 2026-11-01
status: complete
actuals:
  tokens: 8000
  tasks: 3
  commits: 4
plan_head_before: f669fca3f0797ccd1a0a6f86fa5e92208f13091e
plan_head_after: bdcd669a0c6f27e6e3f96faa856e787b7d858e52
---

# Phase 15 Plan 11: Declare the owner's mean-rule attempt before any spend Summary

The one evaluator now judges a declared margin statistic (`seed_mean`) without moving any existing verdict; the mean-rule declaration is committed alone and first; the owner's bang-prefixed command pack, a control and four failure-path controls are on disk. No AWS call, no owner command run, nothing trusted.

## Tasks

| Task | Name | Commit |
| ---- | ---- | ------ |
| 1 (tracer) | Evaluator reads `judging.margin_statistic` | a09bd09d9 |
| 2 | Owner-signed declaration, committed alone | e06b59ade |
| 3 | scan-identifiers / classify-run (TDD), evidence skeleton | 4df0e1587, bdcd669a0 |

## Verification results

- EVALUATOR SELFTEST OK: 126 ok lines (112 prior + 14 `judging-`), no FAIL.
- EXISTING VERDICTS UNMOVED: the evaluator at c259f972e and the new one print byte-identical lines on the Phase 15 pair (declaration at c078910ba, `models/decide/motivation-v1-p15`) and the Phase 14 pair (declaration at 41d8b5f9f, `models/decide/motivation-v1-sm`).
- DECLARATION CHECK OK, DECLARATION MATCHES OPTION mean-rule run_authorized=True, DECLARATION COMMITTED ALONE (one commit, only that file, equals its first-add blob).
- GAP TOOLS SELFTEST OK: 129 cases (19 new for scan-identifiers and classify-run, written before the functions: RED was a NameError, then GREEN).
- IDENTIFIER SCAN OK: `NO IDENTIFIERS files=4` over the declaration, the evidence skeleton, the reply and the disposition.
- RUN EVIDENCE SKELETON OK (see Deviations for the one adjusted assertion).
- NO AWS NO CONTRACT NO EDIT: contracts, .github, crates, 15-REPRODUCTION-DECLARATION.json and 15-GATE-RUN-EVIDENCE.json untouched.

Control: the consumed Phase 14 attempt 1 run under the new declaration is class `defect` (pack refused, phase failed) with `metrics.margin` 0.3267183187404668 (the three-seed mean; `shipped_seed_margin` 0.2860488947445468), recorded `control: true`, `sets_next_trusted: false`. This proves the declared statistic is the one read; it is not trust.

Failure-path controls (copies of the Phase 14 run): no-report consumed + defect (missing-file), no-pack consumed + defect (missing-file), crash pre-verdict, no-fetch unresolved.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Plan inconsistency] The skeleton check required the run name in command S**
- **Found during:** Task 3 verify
- **Issue:** The plan's skeleton assertion demands `motivation-v1-p15b` in V/R/S/F/H except V and H, but the plan also says to derive the commands by exactly four substitutions and nothing else. Command S (poll) never names the run in 15-08 either: it polls by task id and only carries `p15-08` file names, which become `p15b`. Both statements cannot hold.
- **Fix:** Kept the exact-substitution rule (S polls `models/laya-trainer/p15b-task-id` and logs to `p15b-poll.log`), and exempted S from the run-name assertion in the check I ran. No command text was changed beyond the four substitutions.
- **Files modified:** none (the check script lives in the scratchpad)
- **Commit:** n/a

**2. [Rule 3 - Blocking] Task text called `check-disposition` / `check-reply` with a bare file argument**
- **Found during:** precondition check
- **Issue:** These subcommands take `--disposition/--reply/--brief/--diagnosis` options, not positionals.
- **Fix:** Ran them with the options (REPLY OK option=mean-rule run_go=true; DISPOSITION OK option=mean-rule next_trusted=false). No code change.

Otherwise the plan was executed as written. Pre-existing, left alone: uncommitted `.planning/config.json` and `.planning/state.json`.

## Notes for plan 15-12 (the owner-run attempt)

- `authorization` in 15-GAP-RUN-EVIDENCE.json is null by design; 15-12 fills it from the run-go record.
- `declaration_commit.sha` is e06b59adea966c1d4fedc01f659a3fdcd9ba97e6; `declaration_sha256` is stored in full in the evidence file. 15-12 must hash the first-add blob, not the working file.
- Owner commands V, R, S, F, H start with the bang prefix, take every identifier from the owner's gitignored state, and use run name `motivation-v1-p15b`, 12 epochs, and the corrected lock key `owner_id`. `models/decide/motivation-v1-p15b` does not exist yet (the fetch refuses a non-empty destination).
- Evaluation of the real run must use the new `judging`-aware evaluator against the first-add blob; the claim scope forbids comparing its seed-mean margin with the earlier shipped-seed margins.

## Known Stubs

None.

## Threat Flags

None. No new network surface; the evaluator and the diagnosis script stay free of network code (the evaluator purity case is ok).

## Self-Check: PASSED

- Files: laya_repro_eval.py, laya_margin_diagnosis.py, 15-GAP-REPRODUCTION-DECLARATION.json, 15-GAP-RUN-EVIDENCE.json all present.
- Commits a09bd09d9, e06b59ade, 4df0e1587, bdcd669a0 are ancestors of HEAD; `git rev-list --count f669fca3f..HEAD` was 4 before this SUMMARY.
