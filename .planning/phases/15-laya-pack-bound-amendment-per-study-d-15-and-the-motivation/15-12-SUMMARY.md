---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 12
subsystem: infra
tags: [laya, gap-closure, sc4, mean-rule, paid-run, evaluator, inconclusive, pilot-waiver]
requires:
  - phase: 15-11
    provides: committed mean-rule declaration (e06b59ade, sha256 a57165fc867e...), judging-aware evaluator, classify-run and scan-identifiers
provides:
  - check-run-go readiness gate in laya_margin_diagnosis.py (field-based authorization check)
  - 15-GAP-RUN-EVIDENCE.json status complete for the owner-run attempt motivation-v1-p15b (consumed, class waived, verdict inconclusive, next_trusted false)
  - the owner's replies V R S F H recorded verbatim by the orchestrator
affects: [phase-15-verification, laya-training-server-trust]
key-files:
  created: []
  modified:
    - scripts/laya_deploy/laya_margin_diagnosis.py
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GAP-RUN-EVIDENCE.json
key-decisions:
  - "The attempt is consumed (exit_class gate_fail, gate report and GATE FAIL line present) and judged by the one evaluator over the declaration materialized from its first-add commit: class waived, verdict_if_final inconclusive. The verdict was not decided by hand and no near miss was converted to a pass."
  - "next_trusted is false: the class is not pass, the verdict is not reproduced, and a pilot waiver is non-null. The Phase 14 training server stays untrusted (SC4 not met)."
  - "Status is complete, not retry-pending: retry_allowed is false, attempt cap 1, no retry class. Any further attempt, waiver ruling or new declaration is the owner's decision; nothing is pre-authorized."
requirements-completed: [D-02, D-03, D-09, D-11, D-12, D-13, D-14, D-16, D-17]
duration: owner-run attempt plus short executor work
completed: 2026-11-01
status: complete
actuals:
  tokens: 14000
  tasks: 3
  commits: 3
plan_head_before: d5de94586cfc865fc7c88d574a959b22115845bd
plan_head_after: 128e01a0ab5b22ed83601107055f65f34586b3d3
---

# Phase 15 Plan 12: The owner-run mean-rule attempt, judged Summary

The paid run `motivation-v1-p15b` (12 epochs, recipe f88f5b71...) was run by the owner under the declaration committed first, fetched, classified consumed and judged: **inconclusive, next_trusted false**. The pack-bound refusal is gone, but the gate itself failed on the shipped seed and the run was accepted only through the Phase 12 pilot waiver, which the declaration counts as inconclusive, never reproduced.

## Tasks

| Task | Name | Commit |
| ---- | ---- | ------ |
| 1 | Readiness gate (check-run-go), evidence ready-for-owner-run | 300467765 |
| 2 | Owner-run V, R, S, F, H; replies recorded verbatim by the orchestrator | e52cfbdfb |
| 3 | classify, evaluate and record the attempt with an honest next_trusted | 128e01a0a |

## The result

| Item | Value |
| ---- | ----- |
| classify-run | consumed (exit_class gate_fail: the gate executed) |
| Evaluator (first-add blob e06b59ade, sha256 a57165fc867e...) | class waived, verdict_if_final inconclusive, retry_allowed false |
| Judged margin (seed_mean, band [0.30, 0.45]) | 0.3393 (13: 0.3283, 17: 0.2860, 23: 0.4035), in band |
| Shipped-seed margin (seed 23, median-ECE rule) | 0.4035 |
| Shipped ece_post (declared band [0.07, 0.12]) | 0.1150, in band, but above the gate's 0.10 bar |
| Gate | FAIL (gate report pass false, train_rc 3, train.log GATE FAIL); seed 23 hit the calibration clamp (t 5.0); seed 13 also above 0.10 (0.1206), only seed 17 (0.0964) passed |
| Pilot waiver | motivation-v1-pilot-ece applied by the trainer (train.log: ece_post failed alone; pack and verify decide) |
| Pack | accepted, deploy_eligible true only by that route; image sha256:dd38865c... and parity contract 466891fa... equal expected; rescore_max_abs 9.54e-06 against 5.0e-05 (fine-tuned), 1.73e-05 against 3.0e-05 (zero-shot) |
| Artifact | sha256 dcc123ae...afac, 846196868 bytes |
| Offline check-record | PACK RECORD OK for the real sha256; REFUSED for the all-zero sha256; aws_calls 0 |
| train_seconds | 1203.0 (report-only ceiling 2256; Phase 14 attempt 1: 1176.4); billable seconds not obtained (no read-back in the owner replies) |
| Hygiene | no training job or ECS task running, state machine SUCCEEDED, lock status completed |

Comparison (per-seed margins; three-seed mean; shipped-seed margin; shipped ECE), all recorded in the evidence:

| Run | 13 | 17 | 23 | mean | shipped (seed) | shipped ece_post |
| --- | -- | -- | -- | ---- | -------------- | ---------------- |
| Phase 12 | 0.3780 | 0.3502 | 0.3709 | 0.3664 | 0.3780 (13) | 0.0936 |
| Phase 14 attempt 1 | 0.3400 | 0.2860 | 0.3541 | 0.3267 | 0.2860 (17) | 0.0964 |
| Phase 15 attempt 1 (4 epochs) | 0.2795 | 0.3122 | 0.3955 | 0.3291 | 0.2795 (13) | 0.0973 |
| This run (12 epochs) | 0.3283 | 0.2860 | 0.4035 | 0.3393 | 0.4035 (23) | 0.1150 |

The mean and the shipped-seed margins are different instruments (the claim scope says so); a mean-rule pass would not have been comparable with the earlier shipped-seed columns. Here the margin clause did not fail; the gate did.

## Verification results

- EVIDENCE RECORDED status=complete class=waived verdict=inconclusive next_trusted=False (the plan's automated block: declaration digest and first-add blob equal, one declaration commit, no overrides key, recorded evaluation equals a fresh evaluator run, next_trusted equals the recomputed conjunction, claim_scope equal to the declaration's).
- FINAL SCAN OK: `NO IDENTIFIERS files=1`; contracts, .github and crates equal HEAD; 15-REPRODUCTION-DECLARATION.json and 15-GATE-RUN-EVIDENCE.json untouched.

## Deviations from Plan

None - plan executed exactly as written. Observations that are not deviations:

- The trainer applied Phase 12's pilot waiver automatically (same inputs and recipe id), as the declaration's `waiver_note` anticipated; the evaluator therefore reports class `waived`, and `waiver_rule: inconclusive` makes it inconclusive. The evidence's top-level `pilot_waiver` and `waived` are therefore non-null, which by itself keeps next_trusted false.
- Seed 17 in this run equals Phase 14 attempt 1's seed 17 to the last digit (macro_f1, ece_post, margin), while seeds 13 and 23 differ from that run; the shipped seed moved from 17 to 23 and the median-ECE rule shipped a seed that fails the gate. Recorded as an observation in the evidence; the mechanism was not investigated.

## Authentication gates

None for the executor: the paid run, fetch and hygiene read-backs were the owner's (bang-prefixed) with replies recorded by the orchestrator. No AWS call was made by the executor; laya-verify and laya-deploy were not run.

## Issues Encountered

- `state.advance-plan` failed as predicted: Phase 4's `04-VERIFICATION.md` has status `gaps_acknowledged`, outside the tool's closed set (passed | gaps_found | human_needed). STATE.md position, metrics and session were therefore not updated by the verbs; the file was not re-keyed. `roadmap.update-plan-progress 15` worked (12/12 plans, phase still In Progress). `requirements.mark-complete` applied nothing (the D-NN ids are phase decisions, not REQUIREMENTS.md entries).

## Next steps (owner decisions, nothing pre-authorized)

- SC4 is not met: the training server remains untrusted. Options for the owner: accept the inconclusive result; rule on whether Phase 12's pilot waiver should apply to future training-server runs (this run shows it silently makes a failing gate pack-eligible); or declare a new attempt (new run name, new declaration committed first).
- Local `just laya-verify` and `laya-deploy` for the artifact were not run and are owner-run; even a clean local verify would not make this run trusted.

## Known Stubs

None.

## Threat Flags

None. No new network surface; the executor made no network call.

## Self-Check: PASSED

- Files: 15-GAP-RUN-EVIDENCE.json, 15-12-SUMMARY.md present; models/decide/motivation-v1-p15b present (gitignored).
- Commits 300467765, e52cfbdfb, 128e01a0a are ancestors of HEAD; `git rev-list --count d5de94586..HEAD` was 3 before this SUMMARY.
