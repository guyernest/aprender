---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 21
subsystem: laya-pack-parity
tags: [laya, drift, graviton, fargate, pack-bound, laya-parity-v1, d-15]

requires:
  - phase: 14-20
    provides: "laya_drift.py (aws-plan generator, static policy table, tag-modelling fake aws, bashrs-gate, collect) and the local Mac drift legs"
provides:
  - "14-DRIFT-EVIDENCE.json (laya-drift-evidence-v1, status measured): six Graviton pack-task legs (smoke-stance-64 x3, motivation-v1-sm x3) next to the local Mac legs, controls, references, the unmeasured list and the owner's replies"
  - "14-DRIFT-DECISION-BRIEF.md: the D-15 brief, generated from the evidence"
  - "The owner's D-15 decision, recorded: amend-per-study (no parameters named)"
affects: [contracts/laya-parity-v1.yaml amendment (follow-up), 14-VERIFICATION human_verification item 2, laya pack eligibility, decide servers]

plan_head_before: 3f194f60b23ea9335fc7561891e2aebc5bcd6230
plan_head_after: 3b61d11186780ca7ad0c285705be6128b11c0dd0

actuals:
  tokens: 11800
  tasks: 3
  commits: 6

tech-stack:
  added: []
  patterns:
    - "Owner-run measurement script: executor generates and proves (static table, fake aws, bashrs-gate), owner runs with the bang prefix, orchestrator records replies on disk"

key-files:
  created:
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-DRIFT-EVIDENCE.json
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-DRIFT-DECISION-BRIEF.md
  modified:
    - scripts/laya_deploy/laya_drift.py

key-decisions:
  - "D-15: the owner chose amend-per-study. No contract was edited by this plan; contracts/ and crates/aprender-decide equal HEAD and the plan base (git diff --quiet holds)"
  - "The amendment of contracts/laya-parity-v1.yaml is a follow-up with its own planned change set and review. The owner named no parameters (floor, k, per-set terms, the role of the local laya-verify); they must be argued per set (fine_tuned and zero_shot) with the Graviton spread in view"
  - "Timing: motivation-v1-sm is tagged expire and its objects leave S3 about 2026-11-28 (30 days after 2026-10-29); any further measurement against it must happen before then"

patterns-established:
  - "aws-cli v2 prints None, not 0, for KeyCount on an empty prefix: emptiness checks accept 0|None and refuse anything else"

requirements-completed: [D-01, D-14, D-15, D-17]

coverage:
  - id: D1
    description: "Real Graviton script generated, proven against the static policy table, the tag-modelling fake aws and bashrs-gate, with no executor aws call"
    requirement: D-14
    verification:
      - kind: other
        ref: "python3 -I scripts/laya_deploy/laya_drift.py bashrs-gate models/laya-trainer/drift-graviton.sh"
        status: pass
      - kind: other
        ref: "Task 1 tracer verify (all assertions except status, which is now measured by design)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Six owner-run Graviton legs collected into 14-DRIFT-EVIDENCE.json (status measured) and the brief, with no account id or ARN"
    requirement: D-14
    verification:
      - kind: other
        ref: "plan-level verification: status measured, 6 graviton legs, controls/references/unmeasured/owner_replies present, account-id/ARN scan clean"
        status: pass
    human_judgment: false
  - id: D3
    description: "D-15 recorded as amend-per-study with no contract edit"
    requirement: D-15
    verification:
      - kind: other
        ref: "git diff --quiet HEAD -- contracts crates/aprender-decide (rc 0); same against 3f194f60b (rc 0)"
        status: pass
    human_judgment: true
    rationale: "The amendment itself is owner-only and one-way; whether and how it is made is outside this plan"

duration: 1h 40m
completed: 2026-10-30
status: complete
---

# Phase 14 Plan 21: Graviton drift legs and the D-15 decision Summary

**Six Graviton pack tasks on the production image: smoke-stance-64 passes 3 of 3 (fine_tuned 2.7478e-5 against a 3.259e-5 bound) and motivation-v1-sm fails 3 of 3 (zero_shot 1.7285e-5 against the 1e-5 floor, 10.0 times its A10G noise). Every repeat gave the same value bit for bit. The owner chose amend-per-study, and no contract was edited.**

## Performance

- **Duration:** about 1h 40m wall clock, most of it the owner's Fargate runs
- **Started:** 2026-10-30T20:24:44Z (the plan base 3f194f60b)
- **Completed:** 2026-10-30T22:05Z
- **Tasks:** 3 of 3 (1 tracer, 1 owner action, 1 owner decision)
- **Files modified:** 3 committed (evidence, brief, laya_drift.py) plus this summary

## Accomplishments

- The real `models/laya-trainer/drift-graviton.sh` (gitignored) passed the static policy table, a tag-modelling fake-aws run and `bashrs-gate`. `bashrs-gate` still reports BASHRS GATE PASS on the regenerated file, sha256 10b6ed11…7d6f, which matches `owner_script.sha256`. The executor made no real aws call.
- The owner ran stage 1 (smoke:1) and then stage 2 (smoke:2 motivation:3). Every leg has a `pack-outcome.json` under `models/laya-trainer/drift-graviton/` (smoke-1..3, motivation-1..3).
- Measured on Graviton: smoke-stance-64 accepted 3 of 3. Its fine_tuned set reached 0.843 of the bound (3.37 times the noise) and its zero_shot set 0.802 of the bound. motivation-v1-sm was refused 3 of 3: its zero_shot set reached 1.729 of the 1e-5 floor (10.01 times the A10G noise). The spread between repeats was zero: each artifact gave one distinct value per set.
- Mac compared with Graviton on the same bytes: the Mac gave higher drift for the smoke fine_tuned set (3.439e-5, refused) and lower drift for the motivation zero_shot set (1.490e-5, still refused). Rayon thread count was ruled out as the cause on the Mac only: wall time changed 4.97 times from 1 to 8 threads and the re-score did not change.
- The owner's D-15 reply, **amend-per-study**, is recorded in `14-DRIFT-EVIDENCE.json` `owner_replies` (by the orchestrator, 3b61d1118).

## Task Commits

1. **Task 1 (tracer): real Graviton script generated and proven, evidence skeleton**: `752dc884d` (test)
2. **Deviation (Rule 3): collect merges into the evidence and writes the D-15 brief**: `35b6fc10d` (fix)
3. **Deviation (Rule 1, orchestrator): accept aws-cli v2 None for KeyCount on an empty prefix**: `5353b8727` (fix)
4. **Task 2 (owner action): owner Graviton runs recorded**: `d576ad608` (docs)
5. **Task 2 (verification): Graviton legs collected, status measured, brief written; owner_script hash corrected**: `87bc8330b` (docs)
6. **Task 3 (owner decision): D-15 reply recorded**: `3b61d1118` (docs)

**Plan metadata:** this summary's commit (docs)

## Task 3: what amend-per-study means here

The plan's Task 3 has two acceptance criteria. Both hold:
- The orchestrator recorded the reply on disk (`owner_replies[3]`: `"reply": "amend-per-study"`, parameters "not specified by the owner"), and this summary states the chosen option.
- `git diff --quiet HEAD -- contracts crates/aprender-decide` holds (rc 0), and so does the same check against the plan base 3f194f60b. The plan made no amendment, as its reversibility note (one-way) and the D-15 prohibition require.

Nothing else was done for Task 3. Neither the brief nor the evidence was edited after the reply: the brief is generated by `collect`, and the reply belongs in `owner_replies`, where it already is.

## Follow-up: the amendment of laya-parity-v1 (not started)

- It needs its own planned change set and review, for example a `/gsd-plan-phase` gap or decimal phase, before anything under `contracts/` or `crates/aprender-decide` changes.
- **The parameters are unspecified.** The floor (1e-5), the multiplier k (4), whether fine_tuned and zero_shot get separate terms, and what role the local `laya-verify` plays now that the Mac and Graviton disagree on the same bytes must each be argued per set, with the Graviton spread in view. The largest observed ratios of max_abs to the A10G noise are 4.22 for CUDA fine_tuned (Mac) and 10.01 for CUDA zero_shot (Graviton). The brief records these as observations, not as proposed multipliers.
- The pack task's verify stays the authoritative eligibility check until an amendment lands.
- **Deadline:** motivation-v1-sm (task a2cbacae…) is tagged expire, so the lifecycle rule removes its objects about **2026-11-28**. If the amendment study needs any more measurement on that run, it has to happen before then. The drift copies under `runs/drift-*` are also tagged expire.
- Still open: the second set of a refused leg was not measured, because pack_laya reports only the first drifted set. Exposing it would be a decide-crate change, so it belongs to the amendment's change set.

## Files Created/Modified

- `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-DRIFT-EVIDENCE.json`: merged local and Graviton legs, controls, references, unmeasured list, owner commands and replies, owner_script provenance
- `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-DRIFT-DECISION-BRIEF.md`: the D-15 brief
- `scripts/laya_deploy/laya_drift.py`: collect merges into the evidence and writes the brief; the KeyCount emptiness check accepts `None`
- Local, gitignored: `models/laya-trainer/drift-graviton.sh`, `models/laya-trainer/drift-graviton/`

## Decisions Made

- D-15: the owner chose **amend-per-study**. The amendment is deferred to a separate planned change set, and its parameters are unspecified (see the follow-up above).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] collect had to merge into the evidence file and write the brief**
- **Found during:** Task 2 preparation
- **Issue:** The 14-20 `collect` could not produce the file Task 2's verification requires: a merged `14-DRIFT-EVIDENCE.json` with status measured, built on the Task 1 skeleton, plus `14-DRIFT-DECISION-BRIEF.md`.
- **Fix:** `collect` now merges the Graviton legs into the existing evidence, keeps owner_commands and owner_replies, and generates the brief from the evidence.
- **Files modified:** scripts/laya_deploy/laya_drift.py
- **Verification:** the selftest still ends DRIFT SELFTEST OK, and the real collect printed DRIFT COLLECT OK
- **Committed in:** 35b6fc10d

**2. [Rule 1 - Bug, fixed by the orchestrator] aws-cli v2 prints None for KeyCount on an empty prefix**
- **Found during:** Task 2, stage-1 attempt 1
- **Issue:** aws-cli/2.29.0 prints `None` for `KeyCount` when the prefix is empty. The script's emptiness check accepted only `0`, so it refused before any copy (`REFUSED prefix: runs/drift-smoke/… is not empty`). The refusal was false, and nothing was written: no object and no task (`owner_replies[0]`).
- **Fix:** the check accepts `0|None` and refuses anything else. The fake aws now prints `None`. The script was regenerated.
- **Files modified:** scripts/laya_deploy/laya_drift.py
- **Verification:** the selftest covers the `None` shape. The owner's retry of stage 1 completed (`DRIFT GRAVITON DONE smoke:1`).
- **Committed in:** 5353b8727

**3. [Rule 1 - Bug] owner_script.sha256 named the superseded script**
- **Found during:** Task 2 verification
- **Issue:** The evidence still carried the Task 1 hash (e6917dc4…) after the script was regenerated in 5353b8727, so it did not identify the file the owner actually ran.
- **Fix:** `owner_script.sha256` now holds the regenerated file's hash, 10b6ed11…7d6f. The old hash is kept under `owner_script.superseded` together with what happened to it.
- **Files modified:** 14-DRIFT-EVIDENCE.json
- **Verification:** `shasum -a 256 models/laya-trainer/drift-graviton.sh` gives 10b6ed1177165814e235816ff59aa3e7bca9daf528ee150220a9861012637d6f
- **Committed in:** 87bc8330b

---

**Total deviations:** 3 auto-fixed (1 blocking, 2 bugs; one bug was fixed by the orchestrator)
**Impact on plan:** All three were needed to produce or correctly identify the measurement. None touches contracts/ or crates/aprender-decide.

## Issues Encountered

- Task 1's tracer verify asserts `status == 'awaiting-owner-run'`. That no longer holds, because the status is now `measured`, which is the intended state after Task 2. All of its other assertions still pass when re-run. The verify was left unchanged on purpose.
- The `state.*` gsd-tools verbs are expected to fail on the known 04-VERIFICATION `gaps_acknowledged` status (see memory). That file was not edited. The outcome is recorded in the close-out report.

## Verification (plan-level)

- 14-DRIFT-EVIDENCE.json has status measured and 6 Graviton legs (smoke-stance-64 x3, motivation-v1-sm x3); local_controls, references, unmeasured and owner_replies are present; the account-id/ARN scan is clean.
- 14-DRIFT-DECISION-BRIEF.md exists and contains `## Options`.
- owner_replies holds the D-15 reply `amend-per-study`.
- `git diff --quiet HEAD -- contracts crates/aprender-decide` gives rc 0.
- `bashrs-gate` on the real script gives BASHRS GATE PASS.

## User Setup Required

None.

## Next Phase Readiness

- 21 of Phase 14's 22 plans are executed. Ready for 14-22 (owner-run finalizer redeploy with hash check). Still open in 14-VERIFICATION human_verification: D-16 (a second reproduction or a waiver), the packing-cancel live check, and now the planned amendment of laya-parity-v1 that follows from D-15 (parameters unspecified, before about 2026-11-28 if motivation-v1-sm is needed again).

## Self-Check: PASSED

- FOUND: 14-DRIFT-EVIDENCE.json, 14-DRIFT-DECISION-BRIEF.md, models/laya-trainer/drift-graviton.sh, and 6 pack-outcome.json files
- FOUND (ancestors of HEAD): 752dc884d, 35b6fc10d, 5353b8727, d576ad608, 87bc8330b, 3b61d1118
- Commit count measured from the ledger: `git rev-list --count 3f194f60b..HEAD` = 6 before this summary's commit
