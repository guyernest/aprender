---
phase: 16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak
plan: 02
subsystem: laya-training-control
tags: [laya, pipeline-equivalence, control, dataset-staging, validate-dataset, python, stdlib]

requires:
  - phase: 16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak
    provides: 16-CONTROL-DECLARATION.json (first-add a3d4d2baa), the preflight record, laya_control.py
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: the data_sha256 the stager is pinned to
provides:
  - "stage dataset --expect antisemitism-v1: refuses any file that is not Phase 10's before any AWS call"
  - "laya_control.py check-stage and check-validate, each with a case table, pinned in the evidence (tool_pins)"
  - "16-CONTROL-RUN-EVIDENCE.json status staged-validated: datasets/goldstandard-2024/ holds Phase 10's exact bytes and the live server accepted them"
affects: [16-03-paid-run]

key-files:
  created:
    - .planning/phases/16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak/16-CONTROL-RUN-EVIDENCE.json
  modified:
    - scripts/laya_deploy/laya_trainer_ops.py
    - scripts/laya_deploy/laya_control.py
    - justfile

key-decisions:
  - "The immutable prefix is protected by a byte pin in the stager, not by review: a wrong file is refused before any call, and the rehearsal proves it with zero AWS calls"
  - "Both checkers print only a fixed set of derived fields and never a line of their input, so the owner's raw output (target, token and bucket lines) never reaches a recorded line"
  - "check-validate binds --prefix to the recorded owner command and to staging.prefix_used, and applies the prefix rule before reading the reply"
  - "The declared prefix goldstandard-2024 was used; no fallback prefix, no deviation"

patterns-established:
  - "Stage once, prove twice: the stager's own record and the server's validate echo both re-prove the three digests"
  - "Re-run the checkers over the owner's local files rather than trusting the pasted reply lines"

requirements-completed: [D-01, D-07, D-09]

duration: about 1 h
completed: 2026-11-01
status: complete
commits: 5
plan_head_before: 8ff12359e7201de650ae8ba5acd6c9d916cedfbb
plan_head_after: 89b43dd399dec350d4e59426ba8ff1ecc38c4055
actuals:
  tokens: 7300
  tasks: 3
  commits: 5
---

# Phase 16 Plan 02: Stage the antisemitism-v1 dataset and validate it Summary

**antisemitism-v1 is staged once with Phase 10's exact bytes (task 33fd8d7c, train 81a923d4, eval f4a0cf7c) at datasets/goldstandard-2024/, and the live server's validate_dataset answer matches the preflight record and the declaration. No executor made an AWS call, nothing paid ran, nothing is trusted.**

## Performance

- **Duration:** about 1 h (executor work; the owner's S0 and V ran in between)
- **Tasks:** 3 of 3
- **Commits:** 5 measured from the ledger `8ff12359e..89b43dd39`, before this SUMMARY's own commit: `83e48caef`, `97a13be14`, `6f469fade`, `9d95185bb` (recorded by the orchestrator: the owner's replies), `89b43dd39`. The `actuals.tokens` figure is chars/4 over the realized diff of the four files changed (29,042 bytes).

## Accomplishments

- **Byte pin (T-16-06).** `EXPECT["antisemitism-v1"]` holds the three Phase 10 digests; the refusal reads `REFUSED stage: <file> is not Phase 10's`, and the motivation-v1 pin and its `is not Phase 12's` message are byte-identical. The stager selftest gained `stage-expect-antisemitism` and ends `LAYA TRAINER OPS SELFTEST OK` (re-run at the end of this plan). The justfile change is comment lines only. The exact owner stage command was rehearsed with `DRY_RUN=1` and a recording aws shim first on PATH: three planned `+ aws s3 cp` lines, `AWS CALLS: 0`, an empty shim log, and an altered copy refused with exit 2.
- **Two checkers (T-16-06, T-16-07).** `check-stage` re-proves the three digests from the stager's own output and the `datasets-dev.json` record; `check-validate` re-proves them from the server's echo (`inputs_sha256`) and checks labels in order, class counts, rows, shots_per_class, the epochs range, no shift file. Neither echoes a line of its input (selftest-enforced with fake identifiers). The tool's selftest now ends `LAYA CONTROL SELFTEST OK cases=138` (re-run at the end of this plan). `laya_control.py` was committed alone (`97a13be14`) and pinned in the evidence (`tool_pins`, sha256 `d35f9b12...`).
- **Owner checkpoint closed.** The owner ran S0 and V; the orchestrator recorded both replies verbatim in `owner_replies` (`9d95185bb`, unedited here). This plan then re-ran both checkers over the local `ctl-stage.out`, `datasets-dev.json` and `ctl-validate.out` instead of trusting the pasted lines: `STAGE CHECK OK prefix=goldstandard-2024 task=33fd8d7c train=81a923d4 eval=f4a0cf7c` and `VALIDATE CHECK OK rows=8510/2129 shots_per_class=7016 labels=not_antisemitic,antisemitic epochs_range=4-12 inputs=match deferred_rules=0 prefix=goldstandard-2024`, both exit 0. The `staging` block was then written and the status set to `staged-validated` (`89b43dd39`). The Task 3 verify printed `STAGED AND VALIDATED prefix=goldstandard-2024`, and `scan-identifiers` over the evidence printed `NO IDENTIFIERS files=1`.

## Facts for plan 16-03

- **Validate accepted 8,510 train and 2,129 eval rows**, so the server's file, row and size limits are not an obstacle to this dataset; the server reported an epochs range of 4 to 12 (the declared run uses 4) and no deferred rules.
- **Paired-report columns will be null on a real run.** Carried from the 16-01 note: the `paired-report` columns `best_epoch`, `train_seconds`, `epochs_run` and `clamp_hit` will be null on a real run, so the report is descriptive on the remaining columns. This is a known property noted in 16-01, not something found or changed here.
- Plan 16-03 must compare `scripts/laya_deploy/laya_control.py` to the pinned commit `97a13be149b50ffc4d66140be5decd49073a2d50`, not to the declaration's commit, and must re-run `check-stage` and `check-validate` before spending anything.

## Task Commits

1. **Task 1 (tracer): the antisemitism-v1 byte pin for the stager and a zero-AWS rehearsal** - `83e48caef` (feat)
2. **Task 2: check-stage and check-validate with case tables** - `97a13be14` (feat)
2b. **Task 2: evidence skeleton and owner command pack** - `6f469fade` (docs)
3. **Task 3 (owner checkpoint): replies recorded by the orchestrator** - `9d95185bb` (docs); **staging block and status staged-validated** - `89b43dd39` (docs)

## Deviations from Plan

None - plan executed as written. The declared prefix was used, so no S0b/Vb, no fallback prefix and no recorded prefix deviation. The `state.*` verbs were not run (see below).

## Issues Encountered

- Phase 4's `gaps_acknowledged` verification status is outside the SDK's closed set, so the `state.*` verbs and `roadmap.analyze` fail for every phase (known, in memory). Per the dispatch it was not re-keyed. STATE.md updates through those verbs were skipped and are reported to the orchestrator; ROADMAP progress was attempted through `roadmap.update-plan-progress` (outcome in the orchestrator report).
- `.planning/config.json` and `.planning/state.json` were already modified before this plan and were left alone.

## Authentication Gates

None for the executor: S0 and V were owner-run by design (D-07), with the owner's credentials; no executor made an AWS, pmcp or SageMaker call.

## Known Stubs

None.

## Threat Flags

None. The new surface (stager pin, two read-only checkers) is the one the plan's threat register covers (T-16-06 to T-16-09).

## Self-Check: PASSED

- Files: laya_control.py, laya_trainer_ops.py, justfile, 16-CONTROL-RUN-EVIDENCE.json present.
- Commits `83e48caef`, `97a13be14`, `6f469fade`, `9d95185bb`, `89b43dd39` are ancestors of HEAD.
- Both checkers accept the owner's local files; both selftests pass.
