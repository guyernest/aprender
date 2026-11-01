---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 06
subsystem: infra
tags: [laya, pack, graviton, repack, stack-deploy, sc3, amended-bound]
requires:
  - phase: 15-05
    provides: the rebuilt pack image dd38865c..., the predicted stack change set, the A2 contract hash 466891fa...
provides:
  - 15-REPACK-EVIDENCE.json (laya-repack-evidence-v1, status measured): the deploy facts, the Graviton pack-outcome, the bound check, the claim scope
  - 15-ROLL-RUNBOOK.md Sections C and D completed
  - an accepted, deploy-eligible .apr for the gate-passing CUDA checkpoint motivation-v1-sm, packed on Graviton under the amended bound
affects: [15-07]
tech-stack:
  added: []
  patterns: [RESULTS-only rehearsal variant against a tag-modelling fake aws so a rehearsal cannot take the paid run's results slot]
key-files:
  created:
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-REPACK-EVIDENCE.json
  modified:
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-ROLL-RUNBOOK.md
key-decisions:
  - "The re-pack is recorded as running the production pack image and task definition via the drift measurement prefix, not the state machine (RESEARCH assumption A6)"
  - "The trust bar is not claimed closed: the motivation-v1-sm margin 0.2860488947445468 is below the 0.30 band floor, so no reproduction claim is made"
requirements-completed: [D-15, D-03, D-04, D-09, D-16, D-17]
duration: n/a
completed: 2026-11-01
status: complete
actuals:
  tokens: 9000
  tasks: 3
  commits: 4
plan_head_before: 1d2e9af7f939862c9c6e84d8ead7ae01d57c72e3
plan_head_after: f6d2dc85d18995ea21693eead3328f21c81d1a4c
---

# Phase 15 Plan 06: Roll the amended image and re-pack on Graviton Summary

**The owner deployed the amended pack image through a stack change that matched the prediction (plus one derived IAM line), then re-packed the retained CUDA run motivation-v1-sm on the Graviton pack task: accepted, deploy-eligible, new image dd38865c..., amended contract 466891fa..., both bounds equal to max(floor, 4 x noise).**

## Accomplishments

- Task 1 (tracer, executor, offline): FINALIZER PROVENANCE OK, DRIFT SELFTEST OK (88 cases), AWS PLAN OK for the production script and a rehearsal variant differing only on the `readonly RESULTS=` line, BASHRS GATE PASS, and a fake-aws rehearsal (25 calls, tag oracle clean, source keys unchanged, production results dir empty, rehearsal tree deleted). Evidence skeleton and runbook Sections C and D committed (e818dad4b).
- Task 2 (checkpoint, owner): C1 `INFRA OK diff`, C2 `INFRA OK deploy`, C3 `INFRA OK outputs`, D0 `AWS PLAN OK` (script now pins `aprender-laya-pack-dev:2`), D1 run once. Replies were recorded by the orchestrator (0b6cb8d48, 0ded67efc). The executor ran nothing live.
- Task 3 (executor, offline): the only `motivation-*` directory is `motivation-1`; its `leg.txt` task_id, `pack-outcome.json` task_id and the uuid in the owner's TAGS OK line are one uuid, with leg number 1 (recorded as `drift_task`). SC3 OK printed: `apr dd7b1aafa3ed image sha256:dd38865c43ab`.

| Measure | Value |
|---|---|
| accepted / deploy_eligible | true / true |
| pack_image_digest | sha256:dd38865c... (the new image; not the pre-roll 504084d0...) |
| parity_contract_sha256 | 466891fa... (the committed amended contract) |
| task definition | aprender-laya-pack-dev:1 to :2 |
| fine_tuned bound (noise 3.255285573489086e-06) | 5e-05 = max(5e-05, 4 x noise): equal |
| zero_shot bound (zs_noise 1.7265105816122173e-06) | 3e-05 = max(3e-05, 4 x zs_noise): equal |
| Graviton fine_tuned rescore_max_abs (first produced) | 6.973743438720703e-06 (0.139 of its bound) |
| Graviton zero_shot rescore_max_abs | 1.728534698486328e-05 (0.576 of its bound; equal to the attempt-1 refusal value, accepted under the amended floor) |
| argmax | 100/100 |

The fine_tuned Graviton figure is a post-sign-off observation, not an input to the bound. The claim scope makes no reproduction claim: the margin 0.2860488947445468 is below the 0.30 band floor.

## Deviations from Plan

**1. [Owner decision] Unpredicted IAM line in the C1 diff**
- **Found during:** Task 2 (owner C1, read by the orchestrator)
- **Issue:** besides the three predicted resources, the diff showed `MachineRoleDefaultPolicy` `ecs:RunTask` Resource going to KNOWN_AFTER_APPLY. The runbook's STOP list says to stop on any IAM change.
- **Resolution:** the orchestrator verified the statement's expression is identical in the pre-roll template and the new synth (a Ref-derived task-definition family wildcard; unknown only because the task definition is replaced, revision 1 to 2). The owner explicitly chose Go, overriding the STOP-on-IAM rule. Recorded in `owner_replies` and `deviations` of the evidence.
- **Commits:** 0b6cb8d48

**2. [Rule 3 - blocked verify] DRY_RUN `infra diff` not run by the executor**
- **Found during:** Task 1
- **Issue:** the dispatch forbade `just ... infra diff|deploy` for the executor, and the auto-mode classifier denied the call. The plan's first Task 1 verify included that DRY_RUN step.
- **Fix:** ran the bashrs-gate half (PASS) and recorded the skipped half under `offline_checks.dry_run_infra_diff`. The owner's real C1 was the diff of record.
- **Commit:** e818dad4b

**3. [Rule 1 - verify strictness] Recorded reply lacks lowercase `done`**
- **Found during:** Task 3
- **Issue:** the verify asserts a recorded reply containing `done`; the owner pasted the script output, which ends `DRIFT GRAVITON DONE motivation:1`.
- **Fix:** applied the check case-insensitively; no reply was invented. Recorded in the evidence `deviations`.
- **Commit:** f6d2dc85d

**4. [Informational] TAGS OK key count is 21, the rehearsal printed 6**
- The plan pins no key count (the checks are the uuid, the leg number and the expire read-back). The fake's fixture source simply holds fewer objects. Recorded in `drift_task.tags_ok_note`. Not a discrepancy.

## Auth gates

Task 2 was a `checkpoint:human-action` (blocking-human): the deploy and the Fargate task run with the owner's credentials. Handled as normal flow.

## Known Stubs

None.

## Threat Flags

None. The committed files carry no account id, ARN, ECR URI or presigned URL (the verify scans them); the task definition is recorded as family:revision only. T-15-21 through T-15-25 and T-15-37 were handled as planned; the T-15-21 IAM line is Deviation 1. D-16 holds: no cancel was sent to the pack task. `laya_drift.py collect` was not run, so 14-DRIFT-EVIDENCE.json is untouched.

## Hand-off to plan 15-07

- D-17 step (3) is complete here; the declaration (15-07) may follow.
- The re-packed artifact is not deployed or uploaded to any decide server.
- The local drift copy under `runs/drift-motivation/<uuid>/` is tagged `laya-retention=expire`.

## Self-Check: PASSED

- 15-REPACK-EVIDENCE.json, 15-ROLL-RUNBOOK.md exist; commits e818dad4b, 0b6cb8d48, 0ded67efc and f6d2dc85d are ancestors of HEAD.
- REPACK READY and SC3 OK printed; `git status` shows no change under crates/, Cargo.toml, Cargo.lock, rust-toolchain.toml or contracts/.
