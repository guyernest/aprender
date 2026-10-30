---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 15
subsystem: infra
tags: [laya, pmcp-run, sagemaker, fargate, reproduction, d-16, trust-bar, not-reproduced, ze-kasher-dev, live]
status: complete

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-14 smoke and the owner-committed D-16 declaration; 14-13 live authenticated server; 14-11 fetch and the pure evaluator
provides:
  - 14-GATE-RUN-EVIDENCE.json (laya-gate-run-evidence-v3): the D-16 reproduction record, verdict not-reproduced, next_trusted false, comparable with 10- and 12-GATE-RUN-EVIDENCE.json
  - the first x86_64 / CUDA noise record for motivation-v1 and the second observation of the arm64 pack re-score exceeding a GPU-noise-derived bound
affects: [phase-14-verification, laya-training-server-trust]

plan_head_before: a678223c266819e1fb3a97db3c274f99f95b1e4c
plan_head_after: e2a325af1fc7546613f0eaaa7d109a7508754556

# commits is MEASURED (git rev-list --count plan_head_before..plan_head_after = 3): two are this plan's (5ff900c49 evidence,
# e2a325af1 verify amendment), one (213e9d661) is the orchestrator recording the owner's reply. The SUMMARY commit follows this measurement.
actuals:
  tokens: 7000     # chars/4 over the realized diff (evidence JSON, plan amendment, this summary)
  tasks: 2
  commits: 3

tech-stack:
  added: []
  patterns:
    - "A verdict is the evaluator's line over the fetched run's own files, never the status payload; the pack refusal and the out-of-band margin were each sufficient on their own"
    - "A verify that assumes the happy path (an .apr exists) is amended, strictly and only for the refused-pack case, when the plan's own checkpoint rule routes that case to the owner"

key-files:
  created:
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-GATE-RUN-EVIDENCE.json
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-15-SUMMARY.md
  modified:
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-15-PLAN.md

key-decisions:
  - "D-16 verdict is not-reproduced under the committed declaration; the owner accepted it and authorized no further attempt"
  - "No retry: attempt 1 is a defect-class result, which the declaration never retries; the cap of 2 was not exhausted"
  - "No contract amended (D-15): the pack bound and noise reference are an owner decision, left open with a measured work item"

requirements-completed: [D-17, D-15, D-14, D-20, D-21, D-05, D-04]
requirements-not-completed: "D-16 -- the plan ran the reproduction exactly as declared, but the bar was not met (verdict not-reproduced), so D-16 stays open for the owner"

coverage:
  - id: D1
    description: "Attempt 1 ran through the live server on SageMaker (ml.g5.xlarge, cuda:0) under the committed declaration and is recorded from the fetched run's own files in the Phase 12 evidence shape"
    requirement: "D-16"
    verification:
      - kind: other
        ref: "Task 1 verify (amended for a refused pack) -> ATTEMPT 1 pack_refused recipe f88f5b71ef92 device cuda:0"
        status: pass
    human_judgment: false
  - id: D2
    description: "The verdict follows mechanically from the committed declaration through laya_repro_eval.py: class defect, retry_allowed false, verdict not-reproduced, re-computed by the verify from the fetched run"
    requirement: "D-17"
    verification:
      - kind: other
        ref: "Task 2 verify -> D-16 not-reproduced attempts 1; both Laya contracts equal HEAD"
        status: pass
    human_judgment: false
  - id: D3
    description: "D-15: the refused pack ended the run failed with every report fetched and no .apr; no contract was amended and nothing was deployed"
    requirement: "D-15"
    verification:
      - kind: other
        ref: "git diff --quiet HEAD -- contracts/laya-parity-v1.yaml contracts/laya-finetune-gate-v1.yaml; FETCH OK phase failed apr none"
        status: pass
    human_judgment: false
  - id: D4
    description: "Why the accuracy of this CUDA run differs from Phase 12's MPS run (margin 0.286 vs 0.378 on the shipped seed)"
    requirement: "D-16"
    verification: []
    human_judgment: true
    rationale: "Not measured. Candidate causes (device numerics MPS vs CUDA, seed variance on a 100-row eval set) are untested; the record states the gap, not a cause"
---

# Phase 14 Plan 15: Trust-bar reproduction of motivation-v1 through the live server Summary

**Verdict: NOT REPRODUCED. `next_trusted` is false, so the Laya training server is not trusted by the D-16 bar.** Retraining motivation-v1 through the live server on SageMaker under the owner's committed declaration produced a gate pass on the shipped seed, but with two independent defect reasons, so the declaration allows no retry and the owner accepted the verdict with no further attempt.

## Verdict, plainly

Attempt 1 (task `a2cbacae-529c-4703-a796-b6eda6742446`, name `motivation-v1-sm`, epochs 12, `cuda:0`, torch 2.14.0+cu130) ran the declared recipe (recipe id `f88f5b71...` equal to the declaration) on Phase 12's three input hashes with seeds 13, 17 and 23. `laya_repro_eval.py evaluate` classed it `defect`, `retry_allowed` false, verdict `not-reproduced`, for two independent reasons, either of which is sufficient:

1. **Shipped-seed margin below the band floor.** The median-ECE seed 17 shipped with margin **0.2860** against the declared band [0.30, 0.45]. Its ece_post was 0.0964, inside [0.07, 0.12], and the gate itself passed (pass true). Per seed: 13 margin 0.3400, ece 0.1124 FAIL; 17 margin 0.2860, ece 0.0964 pass; 23 margin 0.3541, ece 0.0767 pass. Phase 12's shipped seed 13 had margin 0.3780 and ece 0.0936.
2. **The arm64 pack task refused the re-score (D-15).** `REFUSED RescoreDrift which=zero_shot row=17 max_abs=0.00001728534698486328 bound=0.00001 (nothing written)`. `deploy_eligible` is false, there is no .apr, and the run phase is `failed`. The zero-shot bound is the 1e-5 floor because the A10G zero-shot noise was only 1.73e-6 (4x = 6.9e-6, below the floor); the fine-tuned noise was 3.26e-6 (bound 1.30e-5), and the pack refused before producing a fine-tuned figure.

This is the second observation, after the 14-14 smoke (Mac 3.44e-5, Graviton 2.75e-5 against 3.26e-5), of a re-score on an arm64 CPU exceeding a bound derived from GPU noise. Phase 12's MPS-derived bounds were roughly four times wider (fine-tuned 7.28e-5, zero-shot 5.01e-5).

**Unexplained accuracy gap.** The margin on the shipped seed (0.286 against Phase 12's 0.378) and the different seed outcomes (seed 13 failed here, seed 17 failed in Phase 12) were NOT explained by measurement. The candidate causes, device numerics (MPS versus CUDA) and seed variance on a 100-row eval set, were NOT measured and are not claimed.

## What the owner decided

The owner replied `not-reproduced`: accept the verdict under the committed declaration, authorize no further attempt, and the MPS-versus-CUDA accuracy difference was not explained by measurement (recorded by the orchestrator in the evidence `owner_replies`, commit `213e9d661`, untouched).

**Owner note:** with only a few examples we get a mediocre model; the way forward is a mechanism to grow the dataset and improve annotation quality, and the training and inference MCP servers can spin the data-quality flywheel.

## Real figures

| Quantity | This run | Phase 12 |
|---|---|---|
| Trainer wall (`train_wall_seconds`, outcome.json) | 1176.4 s (SEEDS total_seconds 1171.1) | 752 s (MPS) |
| SageMaker TrainingTimeInSeconds / BillableTimeInSeconds | 1346 s / 1346 s (ml.g5.xlarge) | n/a |
| Pack task (Fargate arm64) | 124.16 s | n/a |
| GPU peak (run-env.json) | 13,729 MiB | n/a |
| Noise fine-tuned max_abs / bound | 3.255e-6 / 1.302e-5 | 1.821e-5 / 7.283e-5 |
| Noise zero-shot max_abs / bound | 1.727e-6 / 1e-5 (floor) | 1.252e-5 / 5.010e-5 |

The report-only prediction for the trainer wall was about 1000 to 1100 s [extrapolated]; the real figure is 1176.4 s, below the 2256 s upper line. The GPU peak was 9,503 MiB in the smoke and 13,729 MiB here, both below the card. One state-machine execution (SUCCEEDED; the workflow returns the failed phase gracefully). Dollar cost was not computed (no verified price).

## Hygiene

No SageMaker job InProgress, no ECS task running, the single execution SUCCEEDED, the lock record reads status completed (released). `verdict.json`, `manifest.json` and `run/gate-report.json` are tagged `expire`. Both Laya contracts equal HEAD. Nothing was deployed. No secret, token, presigned URL, account id or M2M path is in a committed file.

## Follow-ups (not done here)

- Measure cross-CPU and cross-device re-score drift (A10G x86 versus Graviton versus M4 Pro, and MPS versus CUDA) so the pack's noise reference and bound can be decided under D-15; that decision is the owner's and no contract was changed here.
- Optionally retrain motivation-v1 on MPS with this torch build, to test whether the accuracy gap versus Phase 12 comes from the device or from seed variance. This needs an owner authorization of its own.
- A second reproduction attempt, if wanted, needs a new or amended declaration: a defect class is not retriable under the current one.

## Task Commits

1. **Task 1 (tracer):** `5ff900c49` (attempt 1 recorded in the Phase 12 evidence shape)
2. **Task 2:** satisfied by the evaluator line in the same evidence (no retry was allowed); `e2a325af1` (the Task 1 verify and acceptance amended so a refused pack is a recorded outcome when the evaluator says defect / not-reproduced, evidence status marked complete)
3. Orchestrator record: `213e9d661` (the owner's `not-reproduced` reply and note)

## Deviations from Plan

### Owner-approved scope changes

**1. [Owner decision - verify amendment] Task 1 verify accepts a refused pack**
- **Found during:** Task 1 (the pack refused, so no .apr existed and `local_verify` could not run; the original verify required a local verify line)
- **Fix:** the `local_verify` assertion is now strict-conditional: when the outcome is `pack_refused` it requires the evaluator to class attempt 1 `defect` with verdict `not-reproduced` and `retry_allowed` false, the pack not accepted with a `REFUSED` line, no artifact, and `local_verify` recording that it did not run; every other assertion is unchanged. The acceptance text was amended likewise.
- **Files modified:** 14-15-PLAN.md. **Commit:** `e2a325af1`

### Auto-fixed Issues

None.

**Total deviations:** 1 owner-directed plan amendment. **Impact:** none on the verdict, which follows from the unchanged evaluator and declaration.

## Authentication Gates

None. The M2M token was minted from the existing mode-600 file; no auth step was needed.

## Known Stubs

None.

## Threat Flags

None. No new network surface or contract change; threat T-14-53 (overstated reproduction) mitigated by the evaluator-computed verdict, T-14-54 (contract drift) by the unchanged contracts, T-14-55 (unbounded retries) by running exactly one attempt.

## Self-Check: PASSED

- 14-GATE-RUN-EVIDENCE.json and this summary exist; both plan verifies were re-run green after the amendment (Task 1 `ATTEMPT 1 pack_refused`, Task 2 `D-16 not-reproduced attempts 1`, contracts equal HEAD).
- Commits `5ff900c49`, `213e9d661` and `e2a325af1` are ancestors of HEAD.
