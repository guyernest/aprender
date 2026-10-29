---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 12
subsystem: infra
tags: [laya, aws, sagemaker, codebuild, ecr, cdk, cuda, triton, probe, ze-kasher-dev]

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-06 train image + run_step probe; 14-08/14-09 CDK stacks and the owner-run recipes; 14-03 lock
provides:
  - both trainer stacks live in ze-kasher-dev (images stack and trainer stack), both images digest-pinned and deployed
  - the staged base tar and the three datasets (motivation-v1, tweet-stance-64, tweet-stance-16)
  - a measured answer to RESEARCH Open Question 2 and A6 (CUDA 13 / driver, SIGTERM grace)
  - probe_verdict(), the amended CUDA pass rule, in scripts/laya_deploy/laya_trainer_ops.py with a selftest
affects: [14-13, 14-14, 14-15]

plan_head_before: 1801057d2a2b4593a4c8acf356adddeda87180f2
plan_head_after: dd517a84779c9a40a48b883dec581d84229b634d

actuals:
  tokens: 60000
  tasks: 3
  commits: 12

tech-stack:
  added: []
  patterns:
    - "a probe record carries its own verdict, computed by one pure function from the recorded measurements, with a mutation-checked case table"
    - "images are identified by repo name + digest in committed evidence; the ECR URIs (which embed the account id) stay in gitignored state"

key-files:
  created:
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-INFRA-EVIDENCE.json
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-PROBE-EVIDENCE.json
  modified:
    - scripts/laya_train/container/Dockerfile.train
    - scripts/laya_deploy/laya_trainer_ops.py
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-12-PLAN.md

key-decisions:
  - "Owner go (price $20/h stated, quota >= 1) before any AWS call; recorded verbatim in 14-INFRA-EVIDENCE.json"
  - "Owner rebuild-compat: the first CUDA probe failed on a missing C compiler (triton JIT), not on the driver; gcc and libc6-dev added to Dockerfile.train, nothing else"
  - "Owner amend-criterion: the CUDA pass rule is tiny_fixture_rc in {0,3} on a cuda device. The tiny fixture ends GATE FAIL (exit 3, margin 0.0 vs 0.05) on every device; the identical command on CPU also exits 3 with the same numbers"
  - "deploy-go was neither offered nor taken in this plan; the next step belongs to the owner"

patterns-established:
  - "When a probe's acceptance rule fails on a designed outcome, measure the control on another device first, then amend the rule with the owner and mutation-check the new encoding"

requirements-completed: [D-01, D-02, D-13, D-12, D-03, D-07, D-14]

coverage:
  - id: D1
    description: "Offline preflight of the whole live sequence: base tar equals the pin, both image contexts clean, the three datasets present and motivation-v1 equal to Phase 12's inputs_sha256, every live command dry-run with zero AWS calls"
    verification:
      - kind: command
        ref: "Task 1 verify (PREFLIGHT OK 12 dry runs)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Both CDK stacks deployed in ze-kasher-dev with cdk diff run first; both images built in CodeBuild from a clean commit and deployed by digest (train sha256:b49b3fe9... after the compiler rebuild, pack sha256:504084d0...); the base and three datasets staged with sizes and sha256 recorded"
    verification:
      - kind: command
        ref: "Task 3 verify against 14-INFRA-EVIDENCE.json (images, stacks.diff_ran, staged.base.bytes == base-stage.json tar_bytes, three datasets, no 12-digit id)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The pinned torch 2.14.0+cu130 stack runs on ml.g5.xlarge: driver 595.91.07 (no compat libraries), A10G, matmul, offline real-base load, a real-model forward/backward/AdamW step, and the tiny fixture all ran on cuda; verdict pass under the owner-amended rule"
    verification:
      - kind: command
        ref: "Task 3 verify (PROBE pass) plus laya_trainer_ops.py --selftest (59 ok, 0 FAIL; probe-verdict-table 13 rows, probe-verdict-recorded; 3 mutants killed) and LAYA_GATES_ONLY=trainer-probe just laya-gates-selftest (GREEN, AWS CALLS: 0)"
        status: pass
    human_judgment: false
  - id: D4
    description: "SIGTERM probe: job Stopped, grace 118 s (lower bound from 60 heartbeats at 2 s), above run_step's 25 s SHUTDOWN_DEADLINE_S, so RESEARCH A6 holds"
    verification:
      - kind: command
        ref: "Task 3 verify (grace 118)"
        status: pass
    human_judgment: false
  - id: D5
    description: "Production-batch memory headroom on the A10G is NOT established: the probe's real-model step peaked at 96.7% of device memory and survived one allocator OOM retry"
    human_judgment: true
    rationale: "Whether the production batch and sequence length fit with margin is a decision about the real run's settings, which this plan did not measure; recorded as a follow-up"
---

# Phase 14 Plan 12: Live AWS bring-up and the CUDA / SIGTERM probes Summary

**The Laya trainer's AWS side is live in ze-kasher-dev (two stacks, two digest-pinned images, staged base and datasets), and one extra rebuild plus a re-probe proved torch 2.14.0+cu130 runs the real model on an A10G.**

## Performance

- **Duration:** spread over 2026-10-29 across the bring-up session, the compiler rebuild and the criterion amendment (wall time not tracked as one run)
- **Tasks:** 3 (tracer preflight, owner go checkpoint, live bring-up and probes)
- **Files modified:** 3 repo files plus the two evidence files

## Accomplishments

- **Preflight (Task 1, tracer):** 12 dry runs with zero AWS calls, base tar sha256 `15600ec3...` equal to the pin, both image contexts clean, motivation-v1 equal to Phase 12's inputs_sha256.
- **Owner go (Task 2):** reply `go price $20` with quota >= 1 recorded verbatim (the orchestrator's own $1.41/h figure was [ASSUMED]; the owner's figure is the one on record).
- **Bring-up (Task 3):** images stack, then both images in CodeBuild, then the trainer stack with `cdk diff` first, then outputs, then the base tar (846,223,360 bytes, size verified) and the three datasets (every prefix empty before staging).
- **First CUDA probe:** failed on `Failed to find C compiler` raised by triton on the first CUDA forward. It was not a driver or CUDA-13 problem: driver 595.91.07 >= 580, so no compat libraries; `torch.cuda.is_available()` true; the 4096x4096 matmul ran on the A10G.
- **Owner rebuild-compat:** `gcc libc6-dev` added to the apt line of `Dockerfile.train` (commit `ba35837e9`), one CodeBuild build (train digest `sha256:b49b3fe9...`, 6,518,525,500 bytes), one stack redeploy (the cdk diff showed exactly one change: the state machine's train digest; no IAM, network or storage change), one re-probe (job `laya-probe-cuda-20261029181151`, 210 billable seconds).
- **Re-probe measurements:** base load OK (forward 2.49 s, 1.70 GB), real-model step OK at batch 8 x 1024 tokens (2.6 s, host RSS peak 3.54 GB), tiny fixture ran on `cuda:0` and ended exit 3 (GATE FAIL) with numerics matching a CPU control (macro_f1 0.1667, margin 0.0000, ece_post 0.0093 vs 0.0094, nll 1.0969).
- **Owner amend-criterion:** the pass rule became `tiny_fixture_rc in {0,3}` on a cuda device. `probe_verdict()` encodes it; cuda probe records now carry `verdict` and `fail_reasons`. The verdict was recomputed from the recorded measurements (no new probe): pass.
- **SIGTERM probe:** Stopped, grace 118 s against a 25 s deadline.

## Task Commits

1. **Task 1 (tracer): offline preflight** - `a21d51d60` (feat)
2. **Task 2: owner go** - `69af66312` (docs)
3. **Task 3: bring-up, staging, probes** - `e8d8f2b18`, `7ab218bb9` (feat); owner decision `18269c491`; compiler fix `ba35837e9` (fix); re-probe evidence `b79ad687e`; owner decision `86a7b8ae3`; amended rule `a408736b4` (feat); plan text and verdict `dd517a847` (docs)

Commits `e3a3aea73` and `e5482a743` fall inside the measured range but are orchestrator-side phase-14 fix commits, not this plan's tasks.

## Files Created/Modified

- `.planning/phases/14-.../14-INFRA-EVIDENCE.json` - preflight, owner go, images (superseded train record kept), stacks and the rebuild redeploy, staging
- `.planning/phases/14-.../14-PROBE-EVIDENCE.json` - first probe (fail, compiler), `reprobe_1` (pass under the amended rule), SIGTERM probe, control measurement, findings, owner decisions
- `scripts/laya_train/container/Dockerfile.train` - `gcc libc6-dev` in the apt line, with the reason
- `scripts/laya_deploy/laya_trainer_ops.py` - `probe_verdict()`, `FIXTURE_OK_RCS`, verdict recorded in cuda probe records, selftest table
- `.planning/phases/14-.../14-12-PLAN.md` - the criterion amended in the truth, the Task 3 verify script and the acceptance criteria

## Decisions Made

See key-decisions above. The three owner decisions (go, rebuild-compat, amend-criterion) are recorded verbatim by the orchestrator in the evidence files.

## Deviations from Plan

### Owner-approved scope changes

**1. [Owner decision - rebuild-compat] Train image rebuilt with a C toolchain**
- **Found during:** Task 3 (first CUDA probe)
- **Issue:** the `nvidia/cuda:13.0.3-base-ubuntu24.04` base ships no compiler; triton JIT-compiles a CUDA driver stub on the first CUDA forward.
- **Fix:** `gcc libc6-dev` only; one build, one redeploy, one extra probe, all inside the approved scope (MaxRuntime 1800, ze-kasher-dev).
- **Commit:** `ba35837e9`

**2. [Owner decision - amend-criterion] The plan's `tiny_fixture_rc == 0` criterion was wrong**
- **Found during:** Task 3 (re-probe)
- **Issue:** the 12-row random-weight fixture ends GATE FAIL (exit 3) on every device. The plan's criterion, and the selftest's fake `tiny_cmd` returning 0, were never measured on CPU. The identical command on CPU exits 3 with the same numbers.
- **Fix:** rule amended to `{0,3}` on a cuda device; rc 2, other codes and non-cuda devices still fail (table of 13 rows; three mutants, each killed). The plan's verify now reads the deciding probe (`deciding_probe: reprobe_1`); the first attempt stays recorded as a fail.
- **Commits:** `a408736b4`, `dd517a847`

**Total deviations:** 2, both owner-approved. **Impact:** one extra CodeBuild build, stack deploy and 210 s probe job beyond the plan's original estimate; no change to the torch pin or lock.

## Authentication Gates

None. `ze-kasher-dev` credentials were already valid.

## Issues Encountered

- **Memory headroom (follow-up, open):** the real-model step peaked at 22,908,016,640 bytes of 23,686,938,624 (96.7%) at batch 8 x 1024 tokens, and `helper.log` shows a CUDACachingAllocator OOM retry (207,618,048 bytes requested, 93,782,016 free) that the step recovered from. The probe proves one step runs; it does not prove the production batch and sequence length have margin. Check the production batch and length against the 24 GB A10G ceiling before the first real training run.
- **Cold start:** about 105 s pending, 162 s image download, 40 s training for the 210 s billable re-probe.
- **Not exercised:** the role-conditioned `s3:ListBucket` 403-versus-404 question (the owner's IAM user was used); out of scope here and IAM was left alone.
- The SIGTERM probe was not re-run after the rebuild (approved scope was one CUDA probe); its first record stands.

## Spend

CodeBuild: 3 builds (train, pack, train again). SageMaker: 3 probe jobs, 180 + 280 + 210 = 670 billable seconds on ml.g5.xlarge. S3 under 1 GB. Nothing deployed to pmcp.run.

## User Setup Required

None beyond the quota confirmed at the owner go.

## Next Phase Readiness

The AWS side exists and the GPU question is answered. The next step is the owner's decision; this plan does not take it. Open items for later plans: the memory-headroom follow-up, and the S3 ListBucket condition.

## Self-Check: PASSED

- Files: Dockerfile.train, laya_trainer_ops.py, both evidence files, 14-12-PLAN.md present
- Commits `ba35837e9`, `a408736b4`, `dd517a847`, `b79ad687e` are ancestors of HEAD
- Task 3 verify prints `PROBE pass`; `laya_trainer_ops.py --selftest` 59 ok, 0 FAIL; `trainer-probe` gate row GREEN with AWS CALLS: 0
- No 12-digit account id in either evidence file; no probe job InProgress
