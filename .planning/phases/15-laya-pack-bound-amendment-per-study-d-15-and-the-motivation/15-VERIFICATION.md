---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
verified: 2026-11-01T19:00:00Z
status: gaps_found
score: 4/5 must-haves verified
covered_files:
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-01-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-01-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-02-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-02-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-03-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-03-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-04-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-04-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-05-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-05-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-06-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-06-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-07-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-07-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-08-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-08-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-09-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-09-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-10-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-10-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-11-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-11-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-12-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-12-SUMMARY.md
  - contracts/decide-apr-v1.yaml
  - contracts/laya-parity-v1.yaml
  - crates/aprender-decide/src/verify.rs
  - justfile
  - scripts/laya_deploy/laya_eligibility.py
  - scripts/laya_deploy/laya_margin_diagnosis.py
  - scripts/laya_deploy/laya_repro_eval.py
covered_digest: "v4:sha256:998ebae6574cae508be317143aaa3837e93b1d182a4ed17438d4b1bbc12bcd7f"
behavior_unverified: 0
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 4/5
  gaps_closed: []
  gaps_remaining:
    - "SC4 / Phase 14 truth 12 (D-16): trust bar not met"
  regressions: []
gaps:
  - truth: "SC4 / Phase 14 truth 12 (D-16): a fresh owner-run SageMaker reproduction of motivation-v1 under a new committed declaration passes the gate and lands in the owner-approved band with an accepted, deploy-eligible .apr from the Graviton pack task; the Phase 15 gate-run evidence records next_trusted true; no waiver and no override stands in for it"
    status: failed
    reason: "Two owner-run attempts were consumed and neither reproduced. Attempt 1 (15-GATE-RUN-EVIDENCE.json, motivation-v1-p15, shipped-seed rule): verdict not-reproduced, margin 0.2795 below the 0.30 floor, next_trusted false. Attempt 2 (15-GAP-RUN-EVIDENCE.json, motivation-v1-p15b, owner-chosen mean-rule that supersedes D-11): the judged mean margin 0.3393 is in band, but the gate itself FAILED on the shipped seed (seed 23 ece_post 0.1150 above the gate's 0.10 bar, GATE FAIL, train_rc 3) and the pack was accepted only through the Phase 12 pilot waiver motivation-v1-pilot-ece. The declaration's waiver_rule counts that as inconclusive, so next_trusted is false. The owner then recorded accept-inconclusive (15-GAP-OWNER-ACCEPT.json, commit 9345e1984): SC4 is accepted as unmet. That is neither a waiver nor an override (D-02). SC4 stays unmet and the Phase 14 training server stays untrusted."
    artifacts:
      - path: ".planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GAP-RUN-EVIDENCE.json"
        issue: "verdict inconclusive; class waived; next_trusted false; gate.pass false; pilot_waiver motivation-v1-pilot-ece applied"
      - path: ".planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GATE-RUN-EVIDENCE.json"
        issue: "verdict not-reproduced; next_trusted false (margin 0.2795 outside [0.30, 0.45])"
    missing:
      - "A new run under a new committed, owner-approved declaration whose gate passes UNWAIVED on the shipped seed, whose judged margin and shipped ece_post are in the declared band, and whose Graviton pack record is deploy_eligible without any waiver, giving next_trusted true. Nobody has asked for one: the owner chose accept-inconclusive, declared no new attempt and made no waiver ruling."
      - "Or the owner's explicit, separately recorded ruling on the waiver route or an override entry (not made; D-02 forbids treating the pilot-waived acceptance as a pass)."
deferred: []
human_verification: []
---

# Phase 15: Laya pack-bound amendment per study (D-15) and the motivation-v1 re-reproduction (D-16) Verification Report

**Phase Goal:** Carry out the owner's D-15 decision (amend-per-study) and then close D-16: amend the pack/verify re-score bound in `contracts/laya-parity-v1.yaml` per set (fine_tuned, zero_shot), argued from `14-DRIFT-EVIDENCE.json`, with the owner's sign-off before any contract edit and a stated role for the local `laya-verify`; then run a fresh owner-run SageMaker reproduction of motivation-v1 that lands in the D-16 band with an accepted, deploy-eligible `.apr` from the Graviton pack task before the Phase 14 training server is trusted.
**Verified:** 2026-11-01
**Status:** gaps_found
**Re-verification:** Yes, after gap closure (plans 15-09..15-12). The prior report was gaps_found 4/5 on SC4.

## Verdict in one paragraph

The amendment half of the goal remains achieved (SC1, SC2, SC3, SC5) and was regression-checked. The D-16 half is still not achieved, and the gap-closure round did not close it. The gap round diagnosed the first miss as one draw of a noisy margin, the owner chose the mean-rule (margin clause on the mean of the three seed margins, superseding D-11), and the second owner-run attempt `motivation-v1-p15b` was run and judged. On the owner's own new instrument the margin clause passed (mean 0.3393 in [0.30, 0.45]), but the gate failed on the shipped seed (ece_post 0.1150 above 0.10) and the artifact is deploy-eligible only through the Phase 12 pilot waiver. The declaration pre-committed that a waived acceptance is `inconclusive`, never reproduced. Verdict inconclusive, `next_trusted` false. The owner then recorded `accept-inconclusive`, which accepts SC4 as unmet; it does not make it met. Status is `gaps_found` because SC4 is an unmet roadmap criterion and the closed status set has no honest alternative: `passed` would be false, `human_needed` would imply a pending human check of existing evidence (there is none; the remaining step is a new owner decision and run, which the owner declined for now).

## What the owner accepted, and what remains

Read from `15-GAP-OWNER-ACCEPT.json` (recorded_by orchestrator, 2026-11-01T18:09:39Z, commit `9345e1984`), reply verbatim `accept-inconclusive`, in answer to "accept-inconclusive / new-attempt / waiver-ruling":

- **Accepted:** SC4 is recorded unmet by the owner's decision. `next_trusted` stays false. The Phase 14 training server stays untrusted.
- **Not done by that reply:** no new attempt is declared, no waiver ruling is made, no override entry exists (`overrides_applied: 0`, no `overrides:` key, `waived` is `motivation-v1-pilot-ece` in the evidence as a fact of the run, not as an acceptance of the trust bar). D-02 holds: nothing stands in for the D-16 pass.
- **Remains:** the trust bar. To trust the training server, a new owner-approved declaration and paid run must pass the gate unwaived and land in band with an unwaived deploy-eligible pack; or the owner must make a separate explicit ruling. Until then Phase 15's own goal sentence ("before the Phase 14 training server is trusted") is satisfied only in the negative: the server is correctly untrusted.

This is an owner-accepted unmet criterion, not a pass.

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| SC1 | Per-set (fine_tuned, zero_shot) amendment of the re-score bound, argued from `14-DRIFT-EVIDENCE.json`, signed off by the owner at a `checkpoint:decision` before the contract is edited | VERIFIED (regression-checked) | `15-BOUND-DECISION.json` (orchestrator-recorded, `sb-recommended role-drift-advisory`; fine_tuned k4/5e-5/1e-3, zero_shot k4/3e-5/1e-3). `contracts/laya-parity-v1.yaml` lines 138-157 hold the six per-set constants equal to the signed values. `pv validate` on `laya-parity-v1.yaml` and `decide-apr-v1.yaml` re-run by me: 0 errors, 0 warnings. Sign-off commit `91969e11e` precedes the contract edits (shown in the prior report; no contract, crate or justfile change since `e06b59ade`, per `git diff --stat e06b59ade..HEAD`, only `scripts/laya_deploy/laya_margin_diagnosis.py` changed). |
| SC2 | The amendment states the role of the local `just laya-verify` | VERIFIED (regression-checked) | Signed `role-drift-advisory` in the contract text; `laya_eligibility.py --selftest` re-run: `ELIGIBILITY SELFTEST OK 65 cases`. On the new run's record the offline `check-record` accepts the true sha256 and refuses the all-zero one (evidence `local_verify`, 0 AWS calls); the `.apr` on disk `dcc123ae...afac` equals the pack record's `apr_sha256` (re-hashed by me). Phase 15 run 1 `.apr` still hashes `42efa308...feab`. |
| SC3 | A gate-passing CUDA-trained checkpoint yields an accepted, deploy-eligible `.apr` from the Graviton (arm64 Fargate) pack task under the amended bound | VERIFIED (regression-checked) | Unchanged from the prior report: the Phase 14 checkpoint re-pack (`15-REPACK-EVIDENCE.json`) and Phase 15 run 1's own pack record (`accepted`, `deploy_eligible`). Third datum from this round: `models/decide/motivation-v1-p15b/logs/pack-outcome.json` read by me, `ran true, accepted true, deploy_eligible true`, image `sha256:dd38865c...`, parity contract `466891fa...` equal to `shasum` of the on-disk contract (re-hashed by me), `rescore_max_abs 9.54e-06 < 5e-05`, `zs_rescore_max_abs 1.7285e-05 < 3e-05`. Caveat: this third artifact is deploy-eligible only via the pilot waiver (the gate failed), so it is NOT used as SC3 evidence; SC3 rests on the two gate-passing artifacts. |
| SC4 | After SC1 lands, a fresh owner-run reproduction under a new committed declaration passes the gate and lands in the owner-approved band with an accepted, deploy-eligible `.apr`; Phase 15 gate-run evidence records `next_trusted` true; no waiver or override stands in for it | FAILED (BLOCKER), owner-accepted as unmet | See "D-16 outcome" below. `15-GAP-RUN-EVIDENCE.json`: `verdict inconclusive`, `class waived`, `next_trusted false`, `gate.pass false`. `15-GATE-RUN-EVIDENCE.json`: `not-reproduced`, `next_trusted false`. `15-GAP-OWNER-ACCEPT.json`: `accept-inconclusive`. |
| SC5 | Every AWS command is owner-run with the bang prefix; each paid run is owner-approved | VERIFIED | `owner_commands` V, R, S, F, H in `15-GAP-RUN-EVIDENCE.json` all begin with `! `; the prep entry P0 contains no AWS call and no `assume`. Owner replies V, R, S, F, H are `recorded_by: orchestrator`. Paid-run approval for the second attempt: `15-GAP-OWNER-REPLY.json` (`mean-rule; run flag asked separately, answered: run-go`, `run_go true`, orchestrator-recorded) and the declaration's `run_authorized true`, committed in `e06b59ade` at 15:42:26Z, before the task was created at 16:25:41Z. No AWS, CDK, SageMaker or train call was made by this verification. |

**Score:** 4/5 roadmap success criteria verified; 0 present-but-behavior-unverified.

### D-16 outcome (re-verified from the fetched artifacts, not from the evidence JSON alone)

Run `motivation-v1-p15b`, task `42d8eb1e-ee3b-43ca-b6d9-0235a66e1f7f`, 12 epochs, recipe `f88f5b71...`.

| Quantity | Declared | Observed (source read by me) | In band / passes |
| --- | --- | --- | --- |
| judged margin (`seed_mean`) | [0.30, 0.45] | 0.3392777459279122, the mean of per-seed margins 0.3283 / 0.2860 / 0.4035 in `run/gate-report.json` (recomputed by me) | yes |
| shipped-seed margin (informational under the mean-rule) | n/a | 0.4034762572630734 (seed 23) | n/a |
| shipped-seed ece_post | [0.07, 0.12] | 0.1150366187095642 (`logs/verify.log`, gate-report seed 23) | yes |
| the gate (`gate_max_ece` 0.10, margin >= 0.05) | pass | `run/gate-report.json` `pass: false`; `logs/train.log` last line `GATE FAIL`; `logs/outcome.json` `exit_class gate_fail, train_rc 3` | **NO** |
| pilot waiver | none stands in (D-02) | `PILOT WAIVER motivation-v1-pilot-ece ... ece_post failed alone` in train.log; `waived` in pack-outcome.json | applied, so pack acceptance is by waiver |
| deploy_eligible | true, unwaived for trust | true, but only through that waiver | not a trust basis |
| device / recipe / inputs / seeds | cuda / `f88f5b71...` / declared sha256s / [13,17,23] | all match (`cuda:0`, evaluator `must` all true) | yes |
| verdict | reproduced to set next_trusted | evaluator: class `waived`, `verdict_if_final inconclusive`, `retry_allowed false` | **inconclusive** |
| next_trusted | true required | **false** | NO |

Independent re-evaluation: I materialized the declaration from its first-add commit `e06b59ade` (byte-identical to the working file, sha256 `a57165fc...`, committed 15:42:26Z, before the task's `createdAt 16:25:41Z`) and ran `laya_repro_eval.py evaluate --attempt 1` against `models/decide/motivation-v1-p15b`. Output: class `waived`, `verdict_if_final inconclusive`, `retry_allowed false`, `metrics.margin 0.3392777459279122` with `margin_statistic seed_mean`, `gate_pass false`, all `must` clauses true. This matches the recorded evidence exactly. Evaluator selftest re-run: `LAYA REPRO EVAL SELFTEST OK`.

Honest reading of the numbers: the mean-rule instrument was chosen after two shipped-seed margin misses (0.2860 and 0.2795). It would have passed the margin clause here, but it did not rescue the criterion, because a different clause failed. The gate fails whenever the median-ECE seed ships with ece_post above 0.10: seed 23 hit the temperature clamp (t_applied 5.0, `clamp_hit true`), seed 13 was also above the bar (0.1206), and only seed 17 (0.0964) passed. The plan-15-09 diagnosis had put the gate's pass probability at about 0.741 under any rule; this draw landed in the other 0.259. The evidence file records that the difference between this run and Phase 14 attempt 1 on the same recipe was not investigated (seed 17 reproduces attempt 1 to the last digit; seeds 13 and 23 differ). That open question is recorded as an observation, not a gap.

Not rounded into a pass: (a) the mean margin in band is not a reproduction, (b) ece_post 0.1150 being inside the declared band [0.07, 0.12] does not offset the gate's stricter 0.10 bar, and (c) the pack's `deploy_eligible true` rests on the pilot waiver, which D-02 and the declaration's `waiver_rule: inconclusive` bar from standing in for the trust bar.

### Plan 15-09..15-12 deliverables (checked, not taken from the SUMMARYs)

| Plan | Claim | Status | Evidence |
| --- | --- | --- | --- |
| 15-09 | Class diagnosis `single-draw-acceptance`, five-option brief | VERIFIED as analysis, did not close SC4 | Files present (`15-GAP-CLASS-DIAGNOSIS.json`, `15-GAP-MARGIN-INPUTS.json`, `15-GAP-DECISION-BRIEF.md`); the tool exists (`laya_margin_diagnosis.py` lists `analyze, check-diagnosis, check-brief, ...`). Its own predicted gate-pass probability 0.741 is consistent with the failure mode that occurred. |
| 15-10 | Owner reply `mean-rule` plus `run-go` recorded by the orchestrator; disposition machine-derived, `next_trusted false` | VERIFIED | `15-GAP-OWNER-REPLY.json`, `15-GAP-DISPOSITION.json` (`trust_basis none`, `declaration_pending true`, `supersedes [D-11]`). |
| 15-11 | Evaluator reads `judging.margin_statistic` without moving verdicts; declaration committed alone first | VERIFIED | Only `laya_margin_diagnosis.py` differs between `e06b59ade` and HEAD for scripts/contracts/crates/justfile; the evaluator selftest passes; the declaration blob equals the committed first-add. |
| 15-12 | Owner-run attempt consumed and judged honestly (`inconclusive`, `next_trusted false`) | VERIFIED | Re-evaluated above; hygiene read-backs recorded (no training job or ECS task running, state machine SUCCEEDED, lock completed). |
| Owner decision | `accept-inconclusive` recorded | VERIFIED as a record | `15-GAP-OWNER-ACCEPT.json`, commit `9345e1984` touches only that file. Effect field states what it does and does not do. |

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `contracts/laya-parity-v1.yaml` | A2 per-set constants and role text | VERIFIED | Six constants correct; `pv validate` clean; sha256 `466891fa...` equals the value both pack records stamped. |
| `contracts/decide-apr-v1.yaml` | Pack record as eligibility authority | VERIFIED | `pv validate` clean. |
| `crates/aprender-decide/src/verify.rs` | Per-set bound from contract constants | VERIFIED (wired) | Production image printed `rescore_bound=0.00005 zs_rescore_bound=0.00003` on both runs. |
| `justfile` / `scripts/laya_deploy/laya_eligibility.py` | sha256-bound eligibility | VERIFIED | Selftest 65 cases; accept/refuse exercised on the real records. |
| `scripts/laya_deploy/laya_repro_eval.py` | Single evaluator, `seed_mean` support | VERIFIED | Selftest OK; re-run on the real run reproduces the recorded verdict. |
| `15-GAP-REPRODUCTION-DECLARATION.json` | Committed before the paid run | VERIFIED | Commit `e06b59ade` 15:42:26Z precedes the task (16:25:41Z). |
| `15-GAP-RUN-EVIDENCE.json` | Run evidence incl. `next_trusted` | VERIFIED as a record; records an unmet trust bar | Consistent with gate-report, outcome, pack-outcome, verify.log and the `.apr` hash. |
| `15-GAP-OWNER-ACCEPT.json` | Owner decision | VERIFIED as a record | `accept-inconclusive`; not an override or waiver. |

### Key Link Verification

| From | To | Via | Status |
| --- | --- | --- | --- |
| Contract per-set constants | `aprender-decide` verify | `parity.constants.pack_rescore_*_{fine_tuned,zero_shot}` | WIRED |
| Contract sha256 | pack task records (both runs) | `parity_contract_sha256 466891fa...` equals on-disk hash | WIRED |
| Pack record `apr_sha256` | deploy/upload eligibility | `laya_eligibility.py check-record` (accept and refuse both seen) | WIRED |
| Committed declaration | evaluator | first-add blob equals file; evaluator reproduces the recorded verdict | WIRED |
| Gate verdict to `next_trusted` | evidence | waived or gate-failed runs cannot set it (class `waived` gives false) | WIRED, and it correctly withheld trust |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| --- | --- | --- | --- |
| Evaluator judges the real run | `laya_repro_eval.py evaluate --declaration <e06b59ade blob> --run-dir models/decide/motivation-v1-p15b --attempt 1` | class waived, inconclusive, mean margin 0.3393, gate_pass false | PASS (matches evidence) |
| Evaluator selftest | `laya_repro_eval.py --selftest` | `LAYA REPRO EVAL SELFTEST OK` | PASS |
| Eligibility selftest | `laya_eligibility.py --selftest` | `OK 65 cases` | PASS |
| Per-seed mean margin | recompute from gate-report | 0.3392777459279122 | PASS |
| `.apr` hashes | `shasum -a 256` p15b and p15 | `dcc123ae...afac` and `42efa308...feab`, equal to their pack records | PASS |
| Contracts | `pv validate` x2 | 0 errors, 0 warnings | PASS |
| Full cargo suites | not re-run | cited from `15-OFFLINE-GATE-EVIDENCE.json`; no crate or contract change since | CITED |

### Probe Execution

SKIPPED: the phase declares no `probe-*.sh` scripts.

### Requirements Coverage

`REQUIREMENTS.md` has no Phase 15 rows (ROADMAP: `Requirements: TBD`); the plans' `requirements` fields hold CONTEXT decisions. Accounted against `15-CONTEXT.md`:

| Decision | Status | Evidence |
| --- | --- | --- |
| D-01 amend per study, sign-off before edit | SATISFIED | `15-BOUND-DECISION.json`; commit order as in the prior report. |
| D-02 no waiver, no override for D-16 | SATISFIED, and it is why SC4 is unmet | `overrides_applied 0`; the pilot-waived acceptance is classed inconclusive; `accept-inconclusive` is expressly not an override. |
| D-03 AWS owner-run, replies recorded by the orchestrator | SATISFIED | Both gate-run evidence files. |
| D-04 Graviton is the authority | SATISFIED | Both bounds are judged from the Graviton pack records. |
| D-05 per-set parameters, A1 formula kept | SATISFIED | Contract lines 138-157. |
| D-06 refuses a real regression (ceiling 1e-3) | SATISFIED | Ceilings 1e-3 per set; selftests. |
| D-07 evidence-only argument, four unmeasured limits stated | SATISFIED | Contract text; no new measurement before sign-off. |
| D-08..D-10 verify advisory, sha256-bound, justfile edited | SATISFIED | Helper and recipe; accept/refuse on real records. |
| D-11 band unchanged | SUPERSEDED by the owner for the margin statistic only | Owner reply `mean-rule` (`supersedes [D-11]`); the numeric band [0.30, 0.45] / [0.07, 0.12] is unchanged; decided and committed before the paid run. |
| D-12 attempt cap 1 | SATISFIED per declaration | Each declaration carries cap 1 and no retry class; the second attempt ran under a new owner-approved declaration after the first miss (D-13: the owner decides). |
| D-13 nothing pre-authorized after a miss | SATISFIED | Second run required the owner's own `run-go`; after the second miss the owner declared nothing further. |
| D-14 listed changes, claim scope reworded | SATISFIED | `changes_from_attempt_1` and `claim_scope` state it is not an identical reproduction. |
| D-15 re-pack of motivation-v1-sm | SATISFIED | `15-REPACK-EVIDENCE.json`, accepted. |
| D-16 truth 8 out of scope | N/A | Not attempted, still open in 14-VERIFICATION. |
| D-17 order | SATISFIED | Declaration committed before each paid run (second: 15:42Z before 16:25Z). |

The Phase 14 truths inherited: truth 13 VERIFIED; truth 12 FAILED (SC4).

### Anti-Patterns Found

None blocking. No `TBD/FIXME/XXX` check was applicable to evidence JSON. `scripts/laya_deploy/laya_margin_diagnosis.py` gained 240 lines in 15-12 (`check-run-go`), within the plan; selftests of the neighbouring tools pass.

### Observations (non-blocking)

1. The training recipe's median-ECE shipping rule can ship a seed that fails the gate when two of three seeds exceed 0.10 (this run). The pilot waiver then lets the trainer pack it anyway. That is by Phase 12 design, and the declaration pre-committed to count it inconclusive; flagged only so a future declaration anticipates it.
2. `billable_seconds` was not obtained for this run (report-only).
3. The `04-VERIFICATION.md` out-of-set status (`gaps_acknowledged`) did not block this verification: `gsd-tools query verification.fingerprint` ran fine. I did not call `state.*` verbs and did not re-key it. Do not use `gaps_acknowledged` for this phase either; the owner's acceptance is recorded in `15-GAP-OWNER-ACCEPT.json` and in this report's text, not as a status.

### Human Verification Required

None. The outstanding item is an owner decision (new attempt, waiver ruling, or leaving the server untrusted), not a check of existing evidence.

### Gaps Summary

One gap, SC4 / Phase 14 truth 12 (D-16), unchanged in substance from the prior report but now with a second consumed attempt behind it. The amendment half of the phase is done and holds. The D-16 half is unmet: neither attempt produced a gate-passing, in-band, unwaived, deploy-eligible artifact, `next_trusted` is false, and the Phase 14 training server must not be treated as trusted. The owner accepted this outcome (`accept-inconclusive`, commit `9345e1984`) without a waiver, an override or a new attempt. The orchestrator may proceed to phase close with SC4 carried as an explicitly owner-accepted unmet criterion, but it must not report Phase 15 as fully passed, and no closure plan is requested because the owner declined further attempts for now.

---

_Verified: 2026-11-01_
_Verifier: Claude (gsd-verifier)_
