---
phase: 16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak
plan: 03
subsystem: laya-training-control
tags: [laya, pipeline-equivalence, control, sagemaker, cuda, evaluation, python]

requires:
  - phase: 16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak
    provides: the declaration (first-add a3d4d2baa), the Mac reference fixture, the staged dataset, laya_control.py pinned at 97a13be14
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: the Mac (mps:0) three-seed reference the control is compared with
provides:
  - "16-CONTROL-RUN-EVIDENCE.json status complete: the one declared attempt classified consumed, judged class pass, verdict reproduced, next_trusted false, sets_next_trusted false"
  - "the measured CUDA wall time (11,485 s for three seeds on ml.g5.4xlarge, per-seed training about 1,195 s) replacing the unmeasured estimate"
affects: [phase-16-closure, phase-14-trust-ruling]

key-files:
  created:
    - .planning/phases/16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak/16-03-SUMMARY.md
  modified:
    - .planning/phases/16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak/16-CONTROL-RUN-EVIDENCE.json

key-decisions:
  - "The verdict is the committed evaluator's over the declaration blob materialized from a3d4d2baa (sha256 600b6494...), with the three-seed mean margin recomputed from the fetched gate report (0.25638519753336625) and equal to the evaluator's"
  - "The declared attempt is numbered 1: physical run 1 (runtime-cap stop) was ruled not-an-attempt by the owner, so physical run 2 is the one attempt; the evaluator was run with --attempt 1 (--attempt 2 prints the same class and verdict)"
  - "The paired-report block is stored unmodified; the columns it prints null for the run side are supplied in a separate paired_supplement from run/variance-report.json, outside the instrument's block"
  - "Closure of Phase 16, and whether a pass redefines trust, are the owner's; nothing here sets trust"

patterns-established:
  - "Recompute the judged statistic from the fetched files and compare it with the evaluator before recording"

requirements-completed: [D-01, D-02, D-03, D-04, D-05, D-06, D-07, D-09]

duration: about 1 h (executor work after the owner's run)
completed: 2026-11-02
status: complete
commits: 7
plan_head_before: 946df1793661b82110aa18a3d8d2634c5dd20efb
plan_head_after: 9d2f632abb3e242f3f420304e180ac484e4bcd2a
actuals:
  tokens: 6500
  tasks: 3
  commits: 7
---

# Phase 16 Plan 03: Run the control, judge it, record it Summary

**On antisemitism-v1 the SageMaker CUDA pipeline's three-seed mean margin is 0.2564, inside the declared band [0.209, 0.269]; the gate passed unwaived, the Graviton pack accepted a deploy-eligible .apr, and the committed evaluator says class pass, verdict reproduced. next_trusted and sets_next_trusted stay false; the claim is pipeline equivalence on this task only.**

## Performance

- **Duration:** about 1 h of executor work (the owner-run training, about 3.2 h, happened before this step)
- **Tasks:** 3 of 3 (Task 1 readiness and Task 2 owner run were completed earlier by the orchestrator and the owner; this step is Task 3)
- **Commits:** 7 measured from the ledger `946df1793..9d2f632ab`, before this SUMMARY's own commit: `0a9cea23e`, `b319fc95d`, `73b6d5c94`, `5eb4c90c6`, `cf6f933aa`, `a7928d54a` (all orchestrator or earlier steps) and `9d2f632ab` (Task 3, this step). `actuals.tokens` is chars/4 over the 26,012-byte realized diff of the Task 3 commit.

## Accomplishments

- **Classification (the tool).** `classify-run` on `models/decide/antisemitism-v1-cuda-ctl` printed `{"class":"consumed","reason":"exit_class is pass: the gate executed"}`.
- **Verdict (the committed evaluator, over the blob from a3d4d2baa, hash re-checked, working file equal, one commit).** Class pass, verdict_if_final reproduced, retry_allowed false. Band margin true (mean 0.25638519753336625), band ece_post true (shipped seed 17: 0.013291716967574172), every must clause true (deploy_eligible, device_prefix, recipe_id, inputs_sha256, seeds). The mean was recomputed from the fetched gate report's three seed margins and equals the evaluator's value exactly. The shipped-seed margin is 0.2613 (seed 17), not the judged statistic.
- **Paired report (descriptive only).** Per-seed CUDA minus MPS margin: seed 13 +0.0139, seed 17 +0.0155, seed 23 +0.0221; mean +0.0172 (CUDA 0.2564 against Mac 0.2392), max 0.0221. The shipped seed differs (17 here, 23 on the Mac).
- **Pack.** Accepted, deploy_eligible true, waived null, argmax 2129/2129 on the expected pack image and parity contract. `check-record` printed PACK RECORD OK for the real sha256 and REFUSED for an all-zero one.
- **Measured compute.** Training job 11,485.1 s (3.19 h) on ml.g5.4xlarge; per-seed training 1,197.2 / 1,194.3 / 1,193.8 s against the Mac's 4,944.4 / 4,862.3 / 4,864.2 s; Graviton pack 2,557.85 s and verify 2,563.05 s.

## Task Commits

1. **Task 3: classify, evaluate, paired report and evidence** - `9d2f632ab` (docs)

## Deviations from Plan

### Disagreements the tools and prose surfaced (reported, not fixed)

**1. [Plan check] FINAL SCAN AND SCOPE cannot print OK as written.** Its clause that `crates` and the Phase 14 records equal the declaration's first-add commit is false, solely because of the owner-approved amendment commit `b319fc95d` (the 36,000 s cap and ml.g5.4xlarge: `layout.rs`, the failure-surface fixture, `14-FAILURE-SURFACE.md`). Every other clause holds: scan-identifiers printed `NO IDENTIFIERS files=1`; laya_control.py equals its pin (97a13be14, sha256 d35f9b12...); the evaluator, diagnosis tool, scripts/laya_train, contracts, .github, the declaration, the fixture, the draft and the Phase 10 and 15 records equal a3d4d2baa. Recorded in the evidence's deviations; the check was not edited.

**2. [Plan check] The Task 2 verify fails on the recorded step names.** It requires owner_replies steps named exactly R, S, F and H; the orchestrator recorded `R-attempt2`, `S-attempt2`, `F-attempt1`, `F-attempt2`, `H-attempt2`, so the assertion stops at F. The replies were left unedited.

**3. [Tool vs prose] Attempt 1's label.** `classify-run` maps only exit_class refused or crash to pre-verdict; a runtime-cap stop is `unresolved` by the tool (re-run on `models/decide/antisemitism-v1-cuda-ctl-attempt1` it still prints unresolved because outcome.json was not fetched into that directory), while the declaration's prose calls it a timeout that is not an attempt. The owner's ruling of 2026-11-02 settles it; the classifier was not changed after the result.

**4. [Instrument gap] paired-report prints null for the run side of best_epoch, clamp_hit and train_seconds** (it reads the gate report's per_seed, which has none of them; plan 16-02 predicted this). They are supplied from `run/variance-report.json` in `paired_supplement`, outside the instrument's block, which is stored unmodified and equals a fresh run.

**5. [Rule 3] Attempt numbering.** The evidence carried attempt_number 2 and attempts[0].attempt 2 (physical run numbering). The plan keeps the declared attempt at 1 because a pre-verdict stop is not the attempt, so attempt_number is 1 and attempts[0] carries physical_run 2 and the earlier label.

### Auto-fixed Issues

None.

## Findings worth the owner's attention

- CUDA seeds 13 and 17 hit the calibration clamp (T 5.0); the Mac hit it on seed 17 only. The shipped seed 17 hit the clamp and still passed the gate (ece_post 0.0133). All three per-seed margin differences are positive.
- The fine-tuned rescore reaches 78 percent of its bound (3.88e-05 against 5e-05); the zero-shot rescore 1.57e-05 against 3.93e-05.
- Same seeds on attempt 1's partial variance report (amendment record, never a result) gave margins 0.2522 / 0.2519 / 0.2403 against 0.2497 / 0.2613 / 0.2582 here, on a different instance size. Observation only; no determinism claim is made and nothing earlier is re-judged.
- The training infrastructure differs from the declaration's changes_from_reference as amended: ml.g5.4xlarge, 36,000 s cap. The band, inputs, recipe, epochs, seeds and statistic are unchanged.

## Authentication Gates

None for the executor: every AWS step was owner-run; this step made no AWS, pmcp or train call and ran no owner command.

## Known Stubs

None.

## Threat Flags

None. Only an evidence JSON and this SUMMARY were written; the threat register (T-16-10 to T-16-14) is covered by the verify that passed.

## Open for the owner (not decided here)

- Whether Phase 16 closes on this result.
- Whether a pass redefines trust for the Phase 14 training server (the claim scope says it does not).
- Whether to deploy or upload the fetched artifact (not done, not authorized).

## Self-Check: PASSED

- Task 3 verify printed `EVIDENCE RECORDED status=complete class=pass verdict=reproduced next_trusted=False sets_next_trusted=False`.
- scan-identifiers printed `NO IDENTIFIERS files=1`.
- Commit `9d2f632ab` is an ancestor of HEAD; the evidence file exists.
