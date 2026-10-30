---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 14
subsystem: infra
tags: [laya, pmcp-run, sagemaker, fargate, single-flight, cancel, retention, smoke, ze-kasher-dev, live]
status: complete

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-13 live, authenticated and granted request function; 14-12 stacks, images and staged datasets; 14-11 fetch and the pure reproduction evaluator
provides:
  - 14-SMOKE-EVIDENCE.json (laya-trainer-smoke-evidence-v1): the first real end-to-end run on AWS, the live single-flight refusals, the fetch, retention, the training cancel, the (inconclusive) packing cancel, measured spend
  - 14-REPRODUCTION-DECLARATION.json (laya-reproduction-declaration-v1): the owner-approved D-16 band and retry policy, committed before the paid run
  - 14-14 and 14-15 plan text amended to the owner's decisions (pack verify authoritative, packing cancel may be inconclusive)
affects: [14-15]

plan_head_before: 0a10327b767584faba2a415b5f6f4aeb8d03ea58
plan_head_after: ecc3232f196463c1b3c12ff7b358493045f5fd1d

# commits is MEASURED (git rev-list --count plan_head_before..plan_head_after = 7): five are this plan's, two
# (c649f36fd, d83e4ee35) are the orchestrator recording the owner's decisions.
actuals:
  tokens: 6400     # chars/4 over the realized diff of the phase directory
  tasks: 3
  commits: 7

tech-stack:
  added: []
  patterns:
    - "A tracer that fails its last assertion for a reason outside the pipeline is recorded as a finding and decided by the owner, not forced green and not silently expanded"
    - "A cancel test is only conclusive if the thing being cancelled was really doing the work: the pack task of a gate-failed run is a skip, so cancelling it proves nothing about the .sync abort"

key-files:
  created:
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-SMOKE-EVIDENCE.json
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-REPRODUCTION-DECLARATION.json
  modified:
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-14-PLAN.md
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-15-PLAN.md

key-decisions:
  - "The pack task's verify (pack-outcome.json deploy_eligible) is the authoritative eligibility; the local Mac laya-verify refusal is a known cross-CPU finding (owner, Task 3 checkpoint). No contract amended (D-15); a parity-bound amendment can follow as its own measured work"
  - "The packing cancel is accepted as inconclusive, with no extra spend (owner, decision C)"
  - "D-16 declaration is the plan's proposal: margin [0.30, 0.45], ece_post [0.07, 0.12], attempt_cap 2, retry_classes [in-band-gate-fail, waived], a waived acceptance is inconclusive; repro-go authorizes 14-15 up to the cap"
  - "The report-only wall basis is scored rows, not 3 x 752 s; 2256 s stays only as an upper report line"

requirements-completed: [D-11, D-18, D-19, D-20, D-04, D-14, D-12, D-16, D-17, D-15]

coverage:
  - id: D1
    description: "End to end on AWS: tweet-stance-64 epochs 4 ran Train, PackVerify, Finalize on CUDA to completed with the full verdict, shift probe, 30 linked manifest entries, a downloaded link that matches its sha256, and tasks/result with the same verdict and no urls"
    requirement: "D-18"
    verification:
      - kind: other
        ref: "Task 1 verify -> SMOKE completed billable 3058 s (with the amended local-verify assertion: KNOWN CROSS-CPU FINDING)"
        status: pass
    human_judgment: false
  - id: D2
    description: "D-11 live: a second train during the smoke and replacements during both cancels were refused REFUSED single-flight naming the running task; no second execution existed"
    requirement: "D-11"
    verification:
      - kind: other
        ref: "Task 1 and Task 2 verifies; executions 1, then 2 and 3 only for the two cancel probes"
        status: pass
    human_judgment: false
  - id: D3
    description: "D-19 live: a completed passing run's objects are tagged keep; both cancelled runs' verdict.json and manifest.json are tagged expire"
    requirement: "D-19"
    verification:
      - kind: other
        ref: "Task 1 tags (gate-report.json, the .apr, verdict.json, manifest.json all keep); Task 2 verdict_tag expire x2"
        status: pass
    human_judgment: false
  - id: D4
    description: "Cancel during TRAINING (A1): ABORTED, job Stopped in 79 s by the .sync abort, replacement refused while Stopping, lock released only after the job stopped, stored cancelled payload, no urls, expire tag, nothing InProgress"
    requirement: "D-12"
    verification:
      - kind: other
        ref: "Task 2 verify -> CANCEL OK by sync-abort 79 s"
        status: pass
    human_judgment: false
  - id: D5
    description: "Cancel during PACKING is INCONCLUSIVE: the cancelled run's pack task was a gate_fail skip that exited by itself as the cancel was sent, so the stop cannot be attributed to the .sync abort or the finalizer"
    requirement: "D-12"
    verification:
      - kind: other
        ref: "Task 2 verify -> PACK CANCEL INCONCLUSIVE (recorded limitation); owner decision C"
        status: partial
    human_judgment: true
    rationale: "A real-pack cancel needs another gate-passing run (about 51 min of GPU plus 11 min of Fargate); the owner declined the extra spend"
  - id: D6
    description: "D-16 declaration approved by the owner and committed byte for byte before the paid run, accepted by the evaluator"
    requirement: "D-16"
    verification:
      - kind: other
        ref: "Task 3 verify -> DECLARATION OK; DECLARED band, cap 2, retry [in-band-gate-fail, waived], authorized True; tracked and equal to the committed blob"
        status: pass
    human_judgment: false
---

# Phase 14 Plan 14: Live smoke, single-flight, cancel and the D-16 declaration Summary

**The Laya training server ran a real tweet-stance-64 run end to end on SageMaker and Fargate, refused concurrent runs live, observed a training cancel stopped by the Step Functions abort in 79 s, and has the owner's D-16 reproduction band committed before the trust-bar run; the packing cancel stayed inconclusive and the owner's Mac verify disagrees with the Graviton pack verify.**

## Accomplishments

- **Smoke (Task 1, tracer):** `smoke-stance-64` (tweet-stance-64, epochs 4) ran Train, PackVerify and Finalize on `cuda:0` (torch 2.14.0+cu130) to `completed`, pass true, margin 0.3158, ece_post 0.0631, shipped seed 23, seeds 13/17/23 all pass, shift probe present (fine-tuned ECE 0.200 against zero-shot 0.269, margin 0.206, not a gate clause). The .apr is sha256 `31ace8ea...`. The status payload listed 30 manifest entries, every one linked; `run/gate-report.json` downloaded with HTTP 200 and matched its manifest sha256 and size; `tasks/result` carried the identical verdict and manifest with no url. Retention: gate-report.json, the .apr, verdict.json and manifest.json are all `keep`. The fetched manifest's train and pack image digests equal 14-INFRA-EVIDENCE.json's. `just laya-fetch` printed `FETCH OK phase completed apr 31ace8ea...`.
- **Single-flight live (D-11):** the second train during the smoke was refused `REFUSED single-flight: task 3e964ab5-... is running on this deployment; one job at a time (D-11)`; exactly one execution existed.
- **Training cancel (Task 2, observed):** cancelled as soon as the job was InProgress (SecondaryStatus Pending, no train.log). Execution ABORTED; the job Stopped 79 s after the cancel by the `.sync` abort (the job was already Stopping when the event-driven finalizer ran with `stopped_job=false`, so RESEARCH A1 holds). A replacement train sent while the job was Stopping was refused naming the cancelled task. train_status and tasks/result both returned the stored `laya-train-status-v1` payload, phase cancelled, no url; verdict.json is `expire`; no training job was left InProgress; the job billed nothing.
- **D-16 declaration (Task 3):** owner replies `proposal` and `repro-go` (recorded by the orchestrator in the evidence `owner_decisions`). 14-REPRODUCTION-DECLARATION.json carries band margin [0.30, 0.45] and ece_post [0.07, 0.12], attempt_cap 2, retry_classes [in-band-gate-fail, waived], waiver_rule `inconclusive`, must keys including seeds 13/17/23 and Phase 12's three input hashes and recipe id, `run_authorized` true. `laya_repro_eval.py check-declaration` printed `DECLARATION OK`; the committed blob equals the working file.
- **Memory headroom (14-12 open item):** the production recipe is batch 8 with Laya max_len 512, below the probe's 8 x 1024. The real run's GPU peak was 9,503 MiB (about 40 percent of the card) against the probe's 96.7 percent. Closed.

## Findings

- **Mac versus Graviton verify gap (known cross-CPU finding, owner-decided).** The same artifact re-scores 25 percent further from the recorded probabilities on the owner's Mac than on the Graviton pack task. Local `just laya-verify` on the Apple M4 Pro refused three times with an identical line: `REFUSED RescoreDrift which=fine_tuned row=199 max_abs=0.00003439188003540039 bound=0.00003259039634251337 (nothing written)`. The ECS Fargate ARM64 pack task accepted the same bytes at fine-tuned max_abs 2.7478e-5 (84 percent of the bound) and zero-shot 1.508e-5 against 1.881e-5. The Mac figure is 105 percent of the bound. The bound is 4 times the noise, and that noise (8.15e-6 fine-tuned, 4.70e-6 zero-shot) was measured on the A10G, not on either aarch64 CPU. Decision: the pack task's verify is authoritative (the evaluator already reads `deploy_eligible` from `pack-outcome.json`); the Mac refusal is a recorded finding; no contract changed; 14-14 and 14-15 amended. A parity-bound or noise-reference amendment, if wanted, is its own measured work, because it needs a cross-CPU drift measurement.
- **Lock-release timing after a cancel.** The event-driven finalizer ran while compute was still stopping (`awaiting_compute=true`) and nothing re-triggers it, so the lock waited for the next 10-minute reconcile sweep: released 8 min 31 s after the training job stopped (job 23:27:46Z, release 23:36:17Z), and 7 min 31 s after the pack task stopped (00:08:48Z to 00:16:19Z). A replacement train is therefore refused for up to about 10 minutes after the compute is gone. Safe (no second run can start beside a live one) but slower than the compute.
- **The packing cancel is inconclusive.** tweet-stance-16 (epochs fixed 12) ends GATE FAIL in this build (ece_post 0.2944 against the 0.10 bar, margin 0.105). Its pack task wrote `skipped: gate_fail` and exited 0 by itself in about 13 s, the same second the cancel was sent, so a task that was RUNNING when polled was already exiting. The replacement was still refused (single-flight held), the phase ended cancelled with an `expire` verdict and no .apr, and the lock was released after the task stopped, but whether the `.sync` abort or the finalizer's reconcile stops a REAL pack is not observed. `stopped_by` is recorded null. A valid observation needs a run that passes its gate (tweet-stance-64 epochs 4: about 51 min of ml.g5.xlarge plus about 11 min of Fargate). The owner accepted the limitation (decision C); the offline tests and 14-15's real runs are the remaining evidence. The container-SIGTERM path of a running training job is also unobserved (the training cancel hit the job in Pending).
- **Wall-time correction (report-only band).** The draft report band 3 x 752 s = 2256 s was derived from Phase 12's MPS run and is the wrong basis. The smoke's `train.py` took 2882.9 s on the A10G: the three seeds about 1538 s (training itself only 18 to 22 s per seed), the fine-tuned rescore-noise set about 606 s, the zero-shot set about 610 s, the shift probe and finish about 130 s. Scoring (about 1 s per row per seed) and noise (about 1.3 s per row per set) dominate, not training. The second measured point (tweet-stance-16, 280 eval rows) took 1509.1 s. For motivation-v1 (100 eval rows, 295 train rows, 12 epochs) that extrapolates to about 1000 to 1100 s [ASSUMED, extrapolated from two points], below 2256 s, not above it; the declaration now states the scored-rows basis, keeps 2256 s only as an upper report line and reports Phase 12's 752 s beside it.
- **Status-poll parser bug (own tooling).** The background poll's display scrub rewrote 12-digit runs inside floats and broke the JSON, so it logged ERR after the phase was completed and never printed `POLL DONE`. I stopped it by hand and read the terminal state with an unscrubbed call; the execution ended SUCCEEDED well inside the 36000 s plus 15 minute cap.

## Measured spend

SageMaker billable seconds on ml.g5.xlarge: 4734 in total (smoke 3058, packing-cancel run 1676, training-cancel run 0). Fargate: the smoke pack task about 640 s of pack plus verify work, the cancel-pack task about 13 s. CodeBuild: 0 builds. Three executions in total. At the owner-recorded $20/h that is about $26; at the orchestrator's $1.41/h about $1.85. Both prices are unverified: the owner's $20/h has not been checked against the AWS price list, and my earlier $1.41 figure was an unverified third-party assumption. Billable seconds are the measured quantity; the dollar figures are conversions.

Hygiene after Task 2: no execution, training job or ECS task left running; the single-flight lock record reads released (status completed).

## Task Commits

1. **Task 1 (tracer):** `90882719b` (the smoke evidence; local verify refused), `b8342a48b` (plan: a refused local verify becomes a recorded finding), `ecc3232f1` (plan: pack verify authoritative, both plans)
2. **Task 2:** `5c8a4f520` (both cancel tests recorded)
3. **Task 3:** `41d8b5f9f` (the committed declaration); `ecc3232f1` also amends 14-14 Task 2 and 14-15 per the owner's decisions
4. Orchestrator records: `c649f36fd` (run Task 2, decide the finding at Task 3), `d83e4ee35` (the Task 3 answers)

## Deviations from Plan

### Owner-approved scope changes

**1. [Owner decision - tracer gate] Task 2 ran despite the Task 1 local verify refusal**
- **Found during:** Task 1 (local laya-verify refused RescoreDrift)
- **Action:** halted at the tracer gate, reported; the owner chose to run Task 2 and decide the finding at Task 3.

**2. [Owner decision B] Pack verify authoritative; local Mac refusal is a known finding**
- **Fix:** Task 1 verify and truth, and 14-15's local-verify step, verify and truth, amended; no contract file touched (D-15).
- **Commits:** `b8342a48b`, `ecc3232f1`

**3. [Owner decision C] The packing cancel recorded as inconclusive**
- **Fix:** the Task 2 verify and acceptance accept `INCONCLUSIVE` (still requiring the replacement refusal, a lock released after the task stopped, a cancelled phase, no .apr and an expire tag); the training-cancel assertions are unchanged.
- **Commit:** `ecc3232f1`

### Auto-fixed Issues

**4. [Rule 1 - Bug] Task 2 verify key mismatch in the evidence**
- **Found during:** re-running the verifies after the amendments
- **Issue:** the evidence nested `pack_task_stopped_at` under `pack_task`, while the verify reads it at the top level of `cancel_pack`.
- **Fix:** added the top-level key with the same value.
- **Commit:** `ecc3232f1`

## Authentication Gates

None. The M2M file path was read into the environment only and never printed; no token or account id is in any committed file.

## Known Stubs

None.

## Threat Flags

None. The surfaces touched are the plan's own T-14-50 (single-flight, cancel, runaway spend), T-14-51 (the band and retry declared and committed before the run) and T-14-52 (fetched artifact: laya-fetch sha256 checks and the local verify, nothing deployed). Each was observed live.

## Next Phase Readiness

The server is proven end to end on AWS and the D-16 trust bar is declared and committed. Plan 14-15 (the paid reproduction of motivation-v1, up to attempt_cap 2, stop if inconclusive) is authorized by the owner's `repro-go` and is a separate dispatch; it must judge acceptance by the pack task's verify and record the local Mac verify as `local_verify`. Carry forward: the real-pack cancel stays unobserved, and the 10-minute lock-release latency after a cancel.

## Self-Check: PASSED

- Files present: both evidence files, the declaration, 14-14-PLAN.md and 14-15-PLAN.md; `check-declaration` prints `DECLARATION OK` and the committed blob equals the working file
- Commits `90882719b`, `b8342a48b`, `5c8a4f520`, `41d8b5f9f`, `ecc3232f1` (and the orchestrator's `c649f36fd`, `d83e4ee35`) are ancestors of HEAD
- Task 1 verify prints `SMOKE completed billable 3058 s` with the cross-CPU finding; Task 2 verify prints `CANCEL OK by sync-abort 79 s; PACK CANCEL INCONCLUSIVE (recorded limitation)`; Task 3 verify prints `DECLARED ... authorized True`
- No 12-digit account id, JWT or presigned URL in any committed evidence file; no execution, training job or ECS task left running
