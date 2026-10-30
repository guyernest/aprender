---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
verified: 2026-10-30T00:00:00Z
status: gaps_found
score: 8/12 must-haves verified
covered_files:
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-01-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-01-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-02-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-02-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-03-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-03-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-04-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-04-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-05-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-05-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-06-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-06-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-07-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-07-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-08-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-08-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-09-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-09-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-10-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-10-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-11-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-11-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-12-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-12-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-13-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-13-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-14-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-14-SUMMARY.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-15-PLAN.md
  - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-15-SUMMARY.md
  - crates/aprender-core/tests/monorepo_invariants.rs
  - crates/aprender-mcp-laya-train-lambda/src/bin/aprender-laya-train-finalizer.rs
  - crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs
  - crates/aprender-mcp-laya-train-lambda/src/compute.rs
  - crates/aprender-mcp-laya-train-lambda/src/dataset.rs
  - crates/aprender-mcp-laya-train-lambda/src/finalizer.rs
  - crates/aprender-mcp-laya-train-lambda/src/objects.rs
  - crates/aprender-mcp-laya-train-lambda/src/sfn.rs
  - crates/aprender-mcp-laya-train/src/finalize.rs
  - crates/aprender-mcp-laya-train/src/layout.rs
  - crates/aprender-mcp-laya-train/src/lib.rs
  - crates/aprender-mcp-laya-train/src/lock.rs
  - crates/aprender-mcp-laya-train/src/validate.rs
  - deploy-extensions/lib/laya-trainer-images-stack.ts
  - deploy-extensions/lib/laya-trainer-stack.ts
  - scripts/laya_deploy/laya_fetch.py
  - scripts/laya_deploy/laya_repro_eval.py
  - scripts/laya_deploy/laya_trainer_ops.py
  - scripts/laya_train/container/run_step.py
  - scripts/laya_train/data.py
covered_digest: "v4:sha256:2a43a0851fdd9dfb8dec94c38279a8186a1c227f86d494aa61a6a9aecfb06916"
behavior_unverified: 1
overrides_applied: 0
gaps:
  - truth: "D-16 trust bar: the server's first real gate run reproduces motivation-v1 (passes the gate and lands in the owner-approved band margin [0.30, 0.45], ECE [0.07, 0.12], with an accepted deploy-eligible .apr) before the server is trusted"
    status: failed
    reason: "NOT REPRODUCED. Shipped seed 17 margin 0.2860 is below the declared 0.30 floor (ECE 0.0964 is inside its band). The arm64 pack task refused: RescoreDrift which=zero_shot row=17 max_abs=1.7285e-05 bound=1e-05, so there is no .apr and deploy_eligible is false. verdict not-reproduced, next_trusted false. The owner accepted the verdict and authorised no further attempt. The MPS-versus-CUDA accuracy gap was not explained by measurement."
    artifacts:
      - path: ".planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-GATE-RUN-EVIDENCE.json"
        issue: "verdict not-reproduced, next_trusted false, evaluation class defect (out-of-band margin, phase failed, pack-refused), retry_allowed false"
    missing:
      - "A reproduction that lands in band with an accepted, deploy-eligible .apr, OR an explicit owner waiver of D-16 recorded as a verification override (an acceptance of the not-reproduced verdict is not a waiver of the trust bar)"
      - "Measured cross-device / cross-CPU re-score drift (A10G x86, Graviton, M4 Pro; MPS vs CUDA) so the pack noise reference and bound can be decided under D-15 (owner-only contract decision; none made)"
  - truth: "A CUDA-trained checkpoint that passes the gate reliably yields a deploy-eligible .apr from the arm64 pack step"
    status: partial
    reason: "Observed 1 accepted of 2 gate-passing CUDA runs. The smoke artifact was accepted on Graviton (fine-tuned re-score 2.748e-5 against bound 3.259e-5) but the identical bytes were REFUSED by the local Mac laya-verify (RescoreDrift 3.439e-5 against 3.259e-5, deterministic over 3 runs). The motivation-v1 run was refused on Graviton (zero_shot 1.729e-5 against the 1e-5 floor). Both bounds derive from A10G noise (8.1e-6 and 1.7e-6), which is not the CPU the re-score runs on. The owner decided the pack task's verify is authoritative and that the Mac refusal is a recorded cross-CPU finding; no contract was amended (D-15). The failure is fail-closed by design, not a silent pass."
    artifacts:
      - path: ".planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-SMOKE-EVIDENCE.json"
        issue: "local_verify.deploy_eligible false for the artifact the pack task accepted"
      - path: "contracts/laya-parity-v1.yaml"
        issue: "pack bound max(1e-5, 4 x noise) was calibrated on aarch64 (A1); not amended for CUDA-trained checkpoints"
    missing:
      - "Owner decision on the pack bound for CUDA-trained checkpoints, informed by a measured cross-CPU drift study"
  - truth: "A trainer refusal the request pre-filter accepted, a trainer crash, or a runtime-cap timeout is classified by the finalizer from outcome.json, and a pre-filter disagreement (D-08) is surfaced to the caller with the trainer's REFUSED line"
    status: partial
    reason: "WR-01 confirmed in code. run_step.py exits 1 for refused, crash and timeout, so the Train state fails; the CDK train.addCatch(catchFinalize, ALL, resultPath $.error) always hands the finalizer a populated stage_error; finalize_run returns b.failed(error, None, false) at finalize.rs:430 before it reads outcome.json. The Refused / Crash / Timeout arms and prefilter_disagreement: true are reachable only with stage_error None (Event and reconcile paths), which the always-catching state machine does not feed. Unit tests pass stage_error None for these rows. No live run exercised this; the live runs covered only the pack-refusal path (exit 0)."
    artifacts:
      - path: "crates/aprender-mcp-laya-train/src/finalize.rs"
        issue: "line 430 early return on stage_error precedes the outcome.json classification"
      - path: "deploy-extensions/lib/laya-trainer-stack.ts"
        issue: "lines 508-509 catch ALL errors into the finalizer with $.error set"
    missing:
      - "Let a parsed outcome.json decide the class; use stage_error only as the fallback; add a test with stage_error Some and a refused outcome asserting prefilter_disagreement true (14-REVIEW.md WR-01 fix)"
  - truth: "Finalization cannot wedge the single-flight lock"
    status: partial
    reason: "WR-02 confirmed structurally. finalizer.rs:874-905 builds tag targets from every manifest entry and aborts with a store error on a missing key; run_step lists outcome.json and pack-outcome.json in the manifest before uploading them. A failed last upload leaves a listed-but-absent object, so tagging errors on every 10-minute sweep, the lock stays held, and the sweep keeps retrying past the 11 h lease. Trigger probability is low (three failed attempts at the final upload) and it never fired live."
    artifacts:
      - path: "crates/aprender-mcp-laya-train-lambda/src/finalizer.rs"
        issue: "tagging pass is not tolerant of a listed-but-absent object"
    missing:
      - "Intersect tag targets with the listed keys, report the dropped paths, do not fail (14-REVIEW.md WR-02 fix)"
behavior_unverified_items:
  - truth: "Cancelling a task while its arm64 pack/verify task is really packing stops the pack task, releases the single-flight lock and ends the run cancelled"
    test: "Run a dataset that PASSES its gate (for example tweet-stance-64 epochs 4, about 51 min of ml.g5.xlarge plus about 11 min of Fargate), call tasks/cancel once the pack task has been running a minute or two, then check ECS for a stopped pack task, the lock record, and the task phase"
    expected: "The ECS pack task reaches STOPPED, the lock is released by the finalizer, a replacement train is refused until then, and the task ends cancelled with no .apr"
    why_human: "The one packing-cancel attempt used tweet-stance-16, which ends GATE FAIL, so the pack task was a skip already exiting (skipped gate_fail, exit 0 within 13 s). stopped_by is unattributable. The owner accepted this as an inconclusive limitation with no extra spend. Presence checks cannot see whether StopExecution or the reconcile sweep stops a real pack."
human_verification:
  - test: "Decide whether the server is acceptable to use untrusted, or order a second reproduction under an amended declaration"
    expected: "Either an explicit owner waiver of D-16 (recorded as an override) or a new reproduction landing in band with a deploy-eligible .apr"
    why_human: "D-16 is the owner's trust bar and the owner accepted not-reproduced without waiving it; the phase cannot certify trust on its own"
  - test: "Decide the pack bound for CUDA-trained checkpoints (D-15) after a cross-device / cross-CPU drift measurement"
    expected: "A measured amendment of contracts/laya-parity-v1.yaml, or a documented decision to keep the bound and accept refusals"
    why_human: "Contract amendments are owner-only (D-15); the Mac-versus-Graviton and GPU-noise-derived bound findings are unresolved"
  - test: "Run the packing cancel on a run that passes its gate (see behavior_unverified_items)"
    expected: "ECS pack task stopped, lock released, task cancelled"
    why_human: "Costs about 62 min of paid compute; owner accepted the limitation without spend"
---

# Phase 14: Laya training MCP server, S3 dataset in, .apr and reports out: Verification Report

**Phase Goal:** Stand up a thin training MCP server (`aprender-mcp-laya-train` plus its Lambda request function) that takes a dataset by S3 URI (a prefix in one server-owned bucket, read by the server's role), runs the full `laya-finetune-gate-v1` lifecycle as a SageMaker Training Job on a container holding the pinned `scripts/laya_train` uv project plus a prebuilt Rust packer, packs and Rust-verifies the `.apr`, and returns the FULL output as an MCP Task (the `.apr` only on GATE PASS plus every report, readable on GATE FAIL too), deployed to ze-kasher-dev.
**Verified:** 2026-10-30
**Status:** gaps_found
**Re-verification:** No, initial verification

## Headline

The server exists, is deployed, authenticated and works end to end on a real gate-passing dataset (smoke run: SageMaker CUDA training, three seeds, shift probe, arm64 pack and verify accepted, 30-entry manifest with sha-matched presigned links, fetch recipe verified). The D-16 trust bar was NOT met: the motivation-v1 reproduction came out NOT REPRODUCED. The shipped-seed margin 0.2860 is below the declared 0.30 floor, the arm64 pack refused with RescoreDrift (zero_shot), so no `.apr` was produced, and `next_trusted` is false. The owner accepted the verdict and authorised no further attempt; that is acceptance of a recorded outcome, not a waiver of the trust bar, so this report does not convert it into a pass. The packing-stage cancel is inconclusive (owner-accepted). The local Mac `laya-verify` refused the smoke artifact that the pack task accepted (cross-CPU finding; owner decided the pack verify is authoritative; no contract amended per D-15).

## Success criteria source

ROADMAP.md Phase 14 carries a Goal, owner decisions and known constraints but no `Success Criteria` list, and no requirement ids map to the phase (REQUIREMENTS.md holds none; the plans implement CONTEXT decisions D-01..D-22). The must-haves below are therefore derived from the Goal text and the CONTEXT decisions (derivation fallback), cross-checked against the plan frontmatter, with D-16 included because CONTEXT makes it the acceptance test of the whole server.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A thin server exposes `validate_dataset`, `train`, `train_status`, speaks MCP Tasks, requires pmcp.run OAuth, and is deployed to ze-kasher-dev | VERIFIED | Live: tools list is exactly the three (14-DEPLOY-EVIDENCE.json); OAuth-on 401 for `tools/list` and `tools/call` observed before the grant; function role held no `StartExecution`/`PutItem`/`GetObject` pre-grant (simulation implicitDeny); grant run by the owner. `cargo test -p aprender-mcp-laya-train --lib`: 93 passed. |
| 2 | Dataset in by S3 URI under one server-owned bucket read by the role; unknown args, bad names, bad URIs, bad `epochs` refused before any job; the request-side check equals the trainer's `data.py` on one shared table | VERIFIED | Live refusals: `unknown field seeds`, `REFUSED name`, `REFUSED dataset-uri` (other bucket), `REFUSED epochs` (3, and absent above 16 shots), absent prefix answers `task-missing` (404 not 403 after the ListBucket fix). `data.py --selftest` re-run: `DATA SELFTEST OK`. Shared table lib tests green; 14-SHARED-TABLE-EVIDENCE.json records 6 mutations RED. |
| 3 | The full lifecycle (seeds 13/17/23, median-ECE selection, float64 noise record, optional shift probe) runs as a SageMaker Training Job in the pinned container on CUDA | VERIFIED | Smoke task `3e964ab5-...`: `device_used cuda:0`, torch 2.14.0+cu130, three per-seed rows, `shipped_seed 23`, shift probe present (n 280), noise recorded, `ml.g5.xlarge`, 3058 billable s, GPU peak 9,503 MiB. Also the motivation-v1 run (cuda:0, recipe id equal to the declaration). CUDA/SIGTERM probes recorded in 14-PROBE-EVIDENCE.json (first CUDA probe failed for a missing C compiler, image rebuilt, re-probe passed under an owner-amended fixture rule; SIGTERM grace 118 s). |
| 4 | Pack and verify run on arm64 after training; a failed verify is a failed run with every report attached and no `.apr` (D-14, D-15) | VERIFIED (mechanism) | Smoke: pack task on Fargate ARM64 accepted (`deploy_eligible true`, argmax 459/459, `.apr` sha256 31ace8ea...). motivation-v1: pack refused (`REFUSED RescoreDrift ... (nothing written)`), run phase `failed`, `FETCH OK phase failed apr none`, reports fetched. Fail-closed behaviour observed live. Whether a CUDA-trained checkpoint reliably packs is a separate gap (see gaps). |
| 5 | The full output is returned as an MCP Task: `.apr` only on GATE PASS plus every report; `tasks/result` carries no links; links are minted at status read | VERIFIED | Smoke: `manifest_entries 30`, `all_entries_linked true`, presigned link check http 200 with sha and size match, `result_has_no_urls true`, `result_same_verdict_and_manifest true`. GATE FAIL path: `finalize_gate_fail` and `finalize_gate_fail_no_claim` unit tests (phase `gate_failed`, reports, no pack); a live terminal `gate_failed` task was not observed un-cancelled (the live failed-run observation is the pack-refused run). |
| 6 | Retention and hand-off: GATE PASS runs kept, GATE FAIL / failed / cancelled expire in 30 days (tags + lifecycle rule); a `just` fetch recipe verifies every sha256 into `models/decide/<name>/` | VERIFIED | Live tags: pass run `keep` on `.apr`, `verdict.json`, `manifest.json`, `run/gate-report.json`; cancelled and refused runs `expire`. `FETCH OK phase completed apr 31ace8ea...` and `FETCH OK phase failed apr none`. Fetch selftest has a tampered-file negative control (14-11). Lifecycle rules asserted in `laya-trainer-synth` (205 + 105 assertions). Note IN-01: three self-listing manifests keep the `expire` tag even on kept runs (info). |
| 7 | One job at a time (single-flight, D-11) and `tasks/cancel` of a training run stops the SageMaker job and releases the lock | VERIFIED | Live: second `train` refused naming the running task, `second_execution_created false` (twice); training cancel: execution ABORTED, training job Stopped in 79 s, `stopped_by sync-abort`, replacement refused while Stopping, lock released at 23:36:17Z. Caveat recorded in evidence: the job was Pending (container never started), so the run_step SIGTERM path was not exercised in a real run (covered separately by the SIGTERM probe); lock release took 8 min 31 s after the job stopped (waits for the 10-minute sweep; safe). |
| 8 | Cancelling a run while its pack/verify task is really packing stops it and releases the lock | PRESENT_BEHAVIOR_UNVERIFIED | INCONCLUSIVE per 14-SMOKE-EVIDENCE.json `cancel_pack`: the run used tweet-stance-16 which ends GATE FAIL, so the pack task was a skip already exiting (exit 0 within about 13 s); `stopped_by` null. Lock was released at 00:16:19Z and the replacement was refused meanwhile, but who stopped a real pack is unobserved. Owner accepted as an inconclusive limitation, no extra spend. |
| 9 | Deployment-unit register 10 to 11, duplicate-bin allowlist updated, no decide server or contract changed (D-22, D-05) | VERIFIED | `DEPLOYMENT_UNIT_BASELINE = 11`; `aprender-mcp-laya-train-lambda` in `deployment_unit_bins` and the duplicate-bin allowlist; `cargo test -p aprender-core --test monorepo_invariants`: 15 passed. `git diff` from the first Phase 14 commit to HEAD touches no `contracts/` file and no decide crate. |
| 10 | The finalizer classifies a refused / crashed / timed-out trainer from `outcome.json` and surfaces a D-08 pre-filter disagreement with the trainer's `REFUSED` line | FAILED (WR-01) | `finalize.rs:430` returns on `stage_error` before the outcome is read; the state machine's Catch always supplies `stage_error` (`laya-trainer-stack.ts:508-509`). Unit tests exercise the arms only with `stage_error None`. Never exercised live. The failed run still attaches its manifest entries, so the reports stay readable; what is lost is the class, the `prefilter_disagreement` flag and the trainer's own refusal line. |
| 11 | Finalization cannot wedge the lock on a listed-but-absent manifest object | FAILED (WR-02, latent) | `finalizer.rs:874-905` errors on any manifest target whose tag read fails; `run_step.py` lists `outcome.json`/`pack-outcome.json` before uploading them. Low trigger probability, unbounded consequence (alarm storm, lockout beyond the 11 h lease). Never fired live. |
| 12 | D-16 trust bar: reproducing motivation-v1 through the server, in band, with an accepted deploy-eligible `.apr`, so the first real gate run can be trusted | FAILED (BLOCKER) | `14-GATE-RUN-EVIDENCE.json`: verdict `not-reproduced`, `next_trusted false`. Shipped seed 17: margin 0.2860 versus band [0.30, 0.45] (out of band), ECE 0.0964 inside [0.07, 0.12]; per seed 13 FAIL (ECE 0.1124), 17 pass, 23 pass; Phase 12 shipped seed 13 margin 0.3780 / ECE 0.0936. Pack refused `RescoreDrift which=zero_shot row=17 max_abs=1.7285e-05 bound=1e-05`; no `.apr`. The evaluator (`laya_repro_eval.py`, run over the fetched run's own files) classed the attempt `defect`, `retry_allowed false`. Owner reply `not-reproduced` accepted the verdict, authorised no further attempt, and recorded that the MPS-versus-CUDA accuracy difference was not explained by measurement. |

**Score:** 8/12 truths verified (1 present and wired but behavior unexercised: truth 8; 3 failed: truths 10, 11, 12)

### Judgement of the phase against the Goal, plainly

| Goal element | Verdict |
|---|---|
| Thin server, three tools, MCP Task, OAuth, ze-kasher-dev | Met by live evidence |
| S3 dataset in, one server-owned bucket, shared validation | Met by live evidence and tests |
| Full gate lifecycle on SageMaker in the pinned container | Met by live evidence (smoke and motivation-v1) |
| Packs and Rust-verifies the `.apr` | Met only partly: the mechanism works and fails closed, an accepted `.apr` was produced once (smoke), but the one gate-passing run that mattered for trust (motivation-v1) was refused at pack, and the local Mac verify refused the smoke artifact |
| Returns the full output (`.apr` only on pass, every report, readable on GATE FAIL) | Met on the pass path (live) and the failed-with-reports path (live); the terminal `gate_failed` path is unit-tested only |
| The server is trusted by reproduction (D-16) | NOT met |
| Cancel semantics | Training cancel met live (pre-container); packing cancel inconclusive |

## Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/aprender-mcp-laya-train/src/{lib,validate,lock,layout,finalize}.rs` | tools, shared validation, lease, layout vocabulary, finalize decision | VERIFIED | 5,000+ lines, 93 lib tests pass, wired into the Lambda crate |
| `crates/aprender-mcp-laya-train-lambda/src/{sfn,dataset,compute,objects,finalizer}.rs`, `src/bin/{bootstrap,aprender-laya-train-finalizer}.rs` | dispatcher and cancel sink, S3 source, finalizer | VERIFIED (with WR-01, WR-02) | 60 lib tests pass; running live (execution SUCCEEDED, lock released) |
| `deploy-extensions/lib/laya-trainer-stack.ts`, `laya-trainer-images-stack.ts` | CDK stacks, least privilege, lifecycle | VERIFIED | synth assertions green per 14-IAM-FIX-NOTE.md; stack deployed; stack-check selftest re-run: 41 mutation cases, 0 failures |
| `scripts/laya_train/container/run_step.py`, `Dockerfile.train`, pack-arm64 image | train / probe / pack modes | VERIFIED | live runs on SageMaker and Fargate; images pinned by digest in the evidence |
| `scripts/laya_deploy/{laya_fetch,laya_repro_eval,laya_trainer_ops,train_call}.py` | fetch with sha checks, pure evaluator, ops recipes | VERIFIED | fetch run live twice; evaluator produced the D-16 verdict |
| Evidence JSONs (INFRA, PROBE, DEPLOY, SMOKE, GATE-RUN, REPRODUCTION-DECLARATION, LOCK, SHARED-TABLE) | live evidence | VERIFIED present | read and cross-checked above; the declaration (commit 41d8b5f9f) predates the paid reproduction |

## Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| request Lambda `train` | Step Functions execution | `SfnExecutions::start` | WIRED | live execution created; single-flight refusal prevents a second |
| Step Functions Train | SageMaker job (`.sync`) | ASL with pinned digests | WIRED | live job on `ml.g5.xlarge`; cancel aborts the job |
| Step Functions PackVerify | Fargate arm64 pack task | ECS RunTask | WIRED | live pack task accepted and refused runs |
| SFN Catch / Event / sweep | finalizer | `LambdaInvoke` | PARTIAL | wired and live, but the Catch path bypasses the outcome classification (WR-01) |
| finalizer | S3 verdict, manifest, tags, lock release | `finalize_task` | WIRED | live: tags `keep`/`expire`, lock released on every observed path |
| `train_status` | presigned links | `guard_link_key`, minted at read | WIRED | live link http 200, sha match |
| `laya-fetch` | `models/decide/<name>/` | manifest sha256 re-hash | WIRED | `FETCH OK` on a pass run and a failed run |

## Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `train_status` verdict | verdict, per-seed rows | `verdict.json` written by the finalizer from the trainer's gate report | Yes (live, smoke and motivation-v1) | FLOWING |
| manifest entries | 30 entries, sha256, size | S3 listing and training/pack manifests | Yes (live links resolved with matching sha) | FLOWING |
| failure class on the Catch path | stage_error text | SFN Catch cause | Real but unclassified (WR-01) | PARTIAL |

## Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Server library | `cargo test -p aprender-mcp-laya-train --lib` | 93 passed, 0 failed | PASS |
| Lambda unit | `cargo test -p aprender-mcp-laya-train-lambda --lib` | 60 passed | PASS |
| D-22 register and invariants | `cargo test -p aprender-core --test monorepo_invariants` | 15 passed | PASS |
| D-08 trainer replay | `uv run --project scripts/laya_train --frozen python scripts/laya_train/data.py --selftest` | `DATA SELFTEST OK` | PASS |
| Stack template mutation check | `python3 -I scripts/laya_deploy/laya_trainer_stack_check.py --selftest` | 41 mutation cases, 0 failure(s) | PASS |
| WR-01 reachability | read `finalize.rs:420-475` and `laya-trainer-stack.ts:501-509` | stage_error early return precedes outcome classification | CONFIRMED |
| Contracts untouched | `git diff --quiet HEAD -- contracts` and phase-range diff | clean | PASS |

No AWS call was made by this verification.

### Probe Execution

SKIPPED: the phase declares no `scripts/*/tests/probe-*.sh` probe; its probes are the owner-run CUDA and SIGTERM SageMaker jobs recorded in 14-PROBE-EVIDENCE.json (cited, not re-run; AWS calls need the owner).

## Requirements Coverage

No requirement ids are mapped to Phase 14 in REQUIREMENTS.md and no PLAN frontmatter lists any; none is orphaned. The plans implement CONTEXT decisions:

| Decision | Status | Evidence |
|----------|--------|----------|
| D-01 ze-kasher-dev first | SATISFIED | all live evidence on that profile |
| D-02 / D-13 SageMaker, `ml.g5.xlarge` | SATISFIED | live jobs; no ECS fallback triggered |
| D-03 one server-owned bucket by role | SATISFIED | live refusals and 404-not-403 observation |
| D-04 full output | SATISFIED on the pass path (30 entries); reports readable on a failed run |
| D-05 Python/torch training unchanged | SATISFIED | trainer reused, no Rust trainer |
| D-06 / D-21 args | SATISFIED | live unknown-arg and epochs refusals |
| D-07 prepared data dir | SATISFIED | live |
| D-08 pre-filter shared table | SATISFIED for the table; the disagreement-surfacing leg is not (WR-01) |
| D-09 naming | SATISFIED | live name refusal; runs under `runs/<name>/<task_id>/` |
| D-10 OAuth required | SATISFIED | live 401 before grant |
| D-11 single-flight | SATISFIED | live, twice |
| D-12 6 h ceiling | SATISFIED in the synth assertions; never hit |
| D-14 pack and verify on arm64 | SATISFIED mechanism |
| D-15 failed verify is a failed run, no contract amendment by an executor | SATISFIED (refusal observed live; no contract changed) |
| D-16 trust bar reproduction | NOT SATISFIED (not-reproduced) |
| D-17 gate-outcome-level reproducibility claim | SATISFIED as a claim scope; evidence states gate outcome only |
| D-18 verdict inline plus manifest | SATISFIED live |
| D-19 retention | SATISFIED live (IN-01 note) |
| D-20 fetch recipe | SATISFIED live |
| D-22 register 10 to 11 | SATISFIED |

## Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/aprender-mcp-laya-train/src/finalize.rs` | 430 | stage_error short-circuit hides the outcome classification | Warning | WR-01: D-08 disagreement and the trainer's refusal line never reach the caller |
| `crates/aprender-mcp-laya-train-lambda/src/finalizer.rs` | 874-905 | tag pass fails on a listed-but-absent object | Warning | WR-02: latent lock wedge |
| `crates/aprender-mcp-laya-train-lambda/src/sfn.rs`, `finalizer.rs` | 142-145, 263-269 | raw AWS error text (account ids, ARNs, ECR URIs) forwarded to callers | Warning | WR-03: information exposure through the authenticated API; not mitigated |
| `scripts/laya_deploy/laya_trainer_ops.py` | 670-675 | finalizer asset deployed with no provenance check | Warning | WR-04 (and 14-IAM-FIX-NOTE.md: the asset is not byte-reproducible across synths, and an unproven rebuild was accepted by the orchestrator for the IAM redeploy) |
| `.planning/phases/.../14-VALIDATION.md` | frontmatter | `nyquist_compliant: false` while the sign-off block ticks `nyquist_compliant: true set` | Info | self-inconsistent record; the audit table says 40/40 tasks automated |

Debt-marker scan (`TBD`/`FIXME`/`XXX`) over the phase's implementation files: none found. 14-REVIEW.md and 14-REVIEW-DISPOSITION.md: 0 critical, 4 warning, 3 info, all disposition `open` (nothing fixed). 14-SECURITY.md: 82/82 threats closed, `threats_open: 0`, auditor had no AWS credentials and rested live claims on the committed evidence; cross-checked, with the caveat that WR-03 (raw AWS error text to API callers) is a real information-exposure behaviour the register did not capture as open.

## Human Verification Required

### 1. Trust decision on D-16

**Test:** Decide whether to run a second reproduction under a new or amended declaration (the current declaration allows no retry of a defect-class result), or to waive D-16 explicitly.
**Expected:** A recorded in-band reproduction with an accepted `.apr`, or an owner waiver written as a verification override.
**Why human:** D-16 is the owner's bar; acceptance of the `not-reproduced` verdict is not a waiver.

### 2. Pack bound for CUDA-trained checkpoints (D-15)

**Test:** Measure the cross-device and cross-CPU re-score drift (A10G x86, Graviton, M4 Pro; MPS versus CUDA) and decide the bound.
**Expected:** A decision on `contracts/laya-parity-v1.yaml` or a documented choice to keep the bound.
**Why human:** Contract amendment is owner-only; two live observations (Mac 3.44e-5 and Graviton 2.75e-5 against 3.26e-5; Graviton 1.73e-5 against 1e-5) show a bound derived from A10G noise does not hold across CPUs.

### 3. Packing-stage cancel

**Test:** Cancel a gate-passing run while its pack task is running (about 62 min of paid compute).
**Expected:** Pack task STOPPED, lock released, task cancelled with no `.apr`.
**Why human:** Paid run; the single attempt was inconclusive and the owner accepted it.

## Gaps Summary

The build is real and substantially live-proven: the server, dispatcher, finalizer, stacks, containers, fetch recipe and retention all exist, are wired, are covered by passing tests (93 + 60 + 15 re-run here, plus the data and stack selftests), and ran end to end on AWS. What the phase does not deliver is trust in the server. The D-16 reproduction of motivation-v1 is NOT REPRODUCED: the shipped-seed margin 0.2860 is below the 0.30 floor and the arm64 pack refused (RescoreDrift zero_shot, no `.apr`, `next_trusted` false). The owner accepted that outcome and authorised no further attempt, which closes the plan but leaves the trust bar open; there is no later roadmap phase that picks it up, so it is not deferred. Two further live limitations were accepted by the owner and are recorded rather than hidden: the packing-stage cancel is inconclusive, and the local Mac `laya-verify` refused an artifact the Graviton pack task accepted (a cross-CPU finding; no contract was amended). The pack step's refusal of CUDA-trained checkpoints is the shared root cause of the two pack-related findings: the bound derives from GPU noise but the re-score runs on a different CPU. Independently of the live evidence, the code review's WR-01 and WR-02 are confirmed in the source: the finalizer's refused/crash/timeout classification and the D-08 disagreement flag are unreachable behind the state machine's catch-all, and a missing listed object can wedge finalization. WR-03 and WR-04 remain open.

If the owner wants to accept the untrusted-server state as final, that needs an explicit override entry, for example:

```yaml
overrides:
  - must_have: "D-16 trust bar: the server's first real gate run reproduces motivation-v1 in band with an accepted deploy-eligible .apr"
    reason: "<owner's own words>"
    accepted_by: "<owner>"
    accepted_at: "<ISO timestamp>"
```

That is not applied here: the owner accepted the verdict, not a deviation from the bar.

---

_Verified: 2026-10-30_
_Verifier: Claude (gsd-verifier)_
