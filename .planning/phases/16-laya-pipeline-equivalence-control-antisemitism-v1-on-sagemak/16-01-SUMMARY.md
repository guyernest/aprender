---
phase: 16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak
plan: 01
subsystem: laya-training-control
tags: [laya, pipeline-equivalence, control, declaration, python, stdlib, reference-fixture, rehearsal]

requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: the Mac (mps:0) three-seed reference run, 10-GATE-RUN-EVIDENCE.json
  - phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
    provides: the unmodified evaluator laya_repro_eval.py with judging.margin_statistic seed_mean, the amended pack image, the declaration shape
provides:
  - 16-REFERENCE-MPS.json, the Phase 10 per-seed numbers as a tracked, text-free fixture re-derivable byte for byte
  - laya_control.py with snapshot-reference, check-reference, paired-report, rehearse and a 63-case selftest
  - laya_control_preflight.py, the offline proof that the trainer will write recipe d70c3f3d... for the local data dir
  - 16-CONTROL-DECLARATION.json, committed alone (a3d4d2baa) and never to be edited
affects: [16-02-staging, 16-03-paid-run, any later trust ruling]

key-files:
  created:
    - scripts/laya_deploy/laya_control.py
    - scripts/laya_deploy/laya_control_preflight.py
    - .planning/phases/16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak/16-REFERENCE-MPS.json
    - .planning/phases/16-laya-pipeline-equivalence-control-antisemitism-v1-on-sagemak/16-CONTROL-DECLARATION.json
  modified: []

key-decisions:
  - "D-08 held: laya_repro_eval.py is byte-identical to the last planning commit and its selftest still prints 126 ok lines; the control is judged through its existing judging.margin_statistic seed_mean"
  - "The draft's must.gate_pass_unwaived is NOT carried: check_declaration_obj refuses an unknown must clause. Its job is carried by class pass (gate pass true, waived null) plus the ece_post ceiling 0.10, the gate's own bar"
  - "The preflight lives under scripts/laya_deploy, not scripts/laya_train, so the train image context stays unchanged since ba35837e9"
  - "run_authorized is false in the declaration; the per-run owner go is a separate 16-RUN-GO.json bound to this declaration's first-add commit and sha256"

patterns-established:
  - "Rehearse a declaration against the committed evaluator on synthetic runs before it is committed: prove it can be passed and failed, including the narrow corner that is not an out-of-band defect"
  - "Pin a reference as a text-free fixture that a check re-derives byte for byte from the tracked evidence, and embed the fixture sha256 in the declaration"
  - "Selftests are written red first and a mutation of the guarded line is shown to turn a case red"

requirements-completed: [D-01, D-02, D-03, D-04, D-05, D-06, D-07, D-08, D-09]

duration: 1h
completed: 2026-11-01
status: complete
commits: 4
plan_head_before: a13a4b1f735346dc8ef4630238d68fa0a4860ad7
plan_head_after: a3d4d2baa59af8b48399409e1b36ae3fdd14da2c
actuals:
  tokens: 21096
  tasks: 3
  commits: 4
---

# Phase 16 Plan 01: Control declaration, reference and rehearsal Summary

**The control's judge, reference and recipe are proven offline and its declaration is committed alone: the three-seed mean margin in [0.209, 0.269] and the shipped-seed ece_post at most 0.10, judged by the unmodified Phase 15 evaluator. No AWS call was made and nothing is trusted.**

## Dropped draft clause (read this first)

The owner-accepted draft carried `must.gate_pass_unwaived`. It is **not** in the committed declaration, because `laya_repro_eval.py` `check_declaration_obj` refuses an unknown `must` clause (it would never be evaluated), and D-08 forbids widening the evaluator. Its job is carried by **class pass** (the gate passed and the pack carries `waived` null) plus the **ece_post ceiling 0.10**, which is the gate's own bar (`max_ece`).

This is a deviation from the owner-accepted draft. The owner was told of it on 2026-11-01 and **did not object**. That is **not an approval** and is not claimed as one. The drop is named in the declaration's `must_note`, in the body of commit `a3d4d2baa`, and here. If the owner wants the clause back, that is a new evaluator change (test first) and a superseding declaration, not an edit.

## Performance

- **Duration:** about 1 h
- **Tasks:** 3 of 3
- **Commits:** 4 task commits (measured from the ledger: `a13a4b1f7..a3d4d2baa`)

## Accomplishments

- **Reference pinned (T-16-02).** `16-REFERENCE-MPS.json` (sha256 `d7f19f52d1dcf1dd41e8ae613b3a72fb5a2bcda17c0777163d6b5ec17afd967d`) holds the Phase 10 per-seed margin, ece_post, macro_f1, t_applied, clamp_hit, best_epoch, epochs_run and train_seconds, copied unrounded, with the evidence sha256 `c9ea039a...`. `check-reference` re-derives it byte for byte and cross-checks the owner-accepted draft (REFERENCE OK). It is text-free (a key and length walk passes).
- **Recipe reproduced offline (D-01, D-09).** Armed on `data/decide/goldstandard-2024` the preflight prints `PREFLIGHT OK recipe_id=d70c3f3da0daae129f3ee1ce928eb21a0ef71a83a6f09dc18e0621f3cc8adcbc shots_per_class=7016 epochs=4 seed=13 stopping=early_stopping rows=8510/2129 inputs=match waiver=none thresholds=match`, using `contract.recipe_id_for`, `data.validate_data_dir` and `gate.matching_pilot_waiver` by import. The record shows class counts 7016/1494, one declared pilot waiver and no match. The train sources are unchanged since the image commit `ba35837e9`.
- **Declaration proven passable and failable (T-16-05).** `rehearse` runs 20 synthetic CUDA runs through the committed evaluator by subprocess: pass (in band, mean-rule where one seed is outside the band, both edges inside), defect (floor, ceiling, ece over the bar with and without a claimed waiver, mps, cpu, wrong recipe, changed input, two seeds, pack refused), the narrow corner (`in-band-gate-fail` and `waived`, both ending inconclusive, never reproduced) and four failure-path copies classified consumed, consumed, pre-verdict, unresolved. A declaration without the seed_mean statistic fails the mean-rule and corner rows, so the table can fail.
- **Paired report is descriptive (D-04).** `paired-report` prints per-seed CUDA-minus-MPS differences with no `pass`, `fail`, `verdict`, `class`, `ok`, `reproduced`, `equivalent`, `within` or `band` key at any depth (selftest-walked) and has no declaration parameter.
- **Declaration committed alone (T-16-01).** `16-CONTROL-DECLARATION.json`, sha256 `600b64947ab8c058e5b52cf4f3227aca2eeb2a887d3d1dbaae2a41932fb5f096`, first-add commit `a3d4d2baa59af8b48399409e1b36ae3fdd14da2c`, one commit touching one file. `run_authorized` false, `sets_next_trusted` false, no overrides, waiver, waived, next_trusted or gate_pass_unwaived key. Every number was generated from the draft, the fixture or the Phase 15 declaration, never retyped.

## Task Commits

1. **Task 1 (tracer): reference fixture and the evaluator judged end to end** - `bdeff65f0` (feat)
2. **Task 2: offline recipe, inputs, waiver and thresholds preflight** - `18a47a7b2` (feat)
3. **Task 3a: paired report and the complete rehearsal table** - `90ca75240` (feat)
4. **Task 3b: the declaration, committed alone** - `a3d4d2baa` (docs)

## Deviations from Plan

### Plan-sanctioned

**1. [Plan-recorded] Dropped `must.gate_pass_unwaived`** - see the section above. The plan itself directs this and requires it to be named; it is restated here because it deviates from the owner-accepted draft and the owner's silence is not approval.

### Auto-fixed Issues

None - the plan was executed as written. Two small implementation choices inside the plan's freedom:

- `check-reference` compares the evidence path by suffix against the fixture's recorded `source.path`, so it works from any cwd and with an absolute `--evidence`; the byte comparison then uses the fixture's own recorded path.
- Preflight check order is data, epochs, recipe, inputs, labels, thresholds, waiver, so a wrong `--epochs` reports `preflight-epochs` rather than the less specific `preflight-recipe`.

## Findings for plans 16-02 and 16-03

- **The real gate report's per-seed rows do not carry `best_epoch`, `train_seconds`, `epochs_run` or `clamp_hit`.** The trainer writes those to `variance-report.json` (`train.py` `variance_row`), and `laya_fetch.py` fetches only `run/gate-report.json` among the run files. The plan's `paired-report` reads only `gate-report.json` and prints null for an absent optional field, so on a real fetched run the `best_epoch`, `train_seconds` and `clamp_hit` columns will be null; `margin`, `ece_post`, `macro_f1` and `t_applied` are real. `train_seconds` may still be read from `logs/outcome.json` (one total) if 16-03 wants a wall-time comparison. Extending `paired-report` to read an optional `run/variance-report.json` is a small, test-first change if wanted; it was not done here because the plan fixes the interface.
- The narrow corner (shipped-seed margin under the gate's 0.05 bar while the three-seed mean sits in the band) is reachable and ends `inconclusive`; plan 16-03 Task 3 stops for the owner on either class, as planned.
- `just laya-gates-selftest` is red at HEAD for `laya-margin-diagnosis` (no row in `scripts/laya_gates.tsv`), a Phase 15 omission left alone as the plan directs; the new tools are invoked directly with `python3 -I` and add no `laya*` justfile recipe.

## Verification

All plan verifies ran and passed: CONTROL TOOL SELFTEST OK (63 cases), REFERENCE FIXTURE OK, FIXTURE TEXT-FREE AND EXACT OK, EVALUATOR AND SCOPE UNTOUCHED (126 evaluator ok lines, clean diff against `0c0150709`), PREFLIGHT SELFTEST OK (21 cases), PREFLIGHT ARMED OK, TRAIN SOURCES UNCHANGED SINCE IMAGE ba35837e9, REHEARSAL TABLE COMPLETE, DECLARATION CHECKS OK (DECLARATION OK, REHEARSAL OK cases=20, PREFLIGHT OK with the declaration, NO IDENTIFIERS files=2), DECLARATION MATCHES DRAFT AND FIXTURE, DECLARATION COMMITTED ALONE, SCOPE UNTOUCHED. Mutations shown: disabling the drift comparison turns three `check-reference` cases red; disabling the waiver digest loop turns the waiver case red.

## Known Stubs

None.

## Threat Flags

None. No network endpoint, auth path or trust-boundary schema was added; no AWS call was made.

## Issues Encountered

`state.*` verbs fail on Phase 4's `04-VERIFICATION.md` (`gaps_acknowledged` is outside the tool's closed set); the file was deliberately not re-keyed. See the hand-off report for the verbs run and their output.

## Self-Check: PASSED

Files found: `scripts/laya_deploy/laya_control.py`, `scripts/laya_deploy/laya_control_preflight.py`, `16-REFERENCE-MPS.json`, `16-CONTROL-DECLARATION.json`. Commits found as ancestors of HEAD: `bdeff65f0`, `18a47a7b2`, `90ca75240`, `a3d4d2baa`.
