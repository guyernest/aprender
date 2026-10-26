---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 10
subsystem: setfit-reference-arm
tags: [setfit, s32e1b16, apr-eval, dump-probabilities, memory-rehearsal, d-25, laya-setfit-cell]
status: complete

requires:
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: plan 12-01 data/decide/motivation-v1, plan 12-06 K-label adapter and apr eval --dump-probabilities
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: the laya-setfit-cell recipe, the s8 rehearsal method, the apr eval memory defect record
provides:
  - "laya-setfit-cell with a shots parameter (default 64) that writes test-probabilities.json from the same eval pass"
  - "12-SETFIT-CELL-EVIDENCE.json: outcome completed, s32e1b16 seed 13, 100-row same-pass probability dump (gitignored under models/setfit/)"
affects: [12-11, 12-16]

actuals:
  tokens: 4500
  tasks: 2
  commits: 3
plan_head_before: 21603f53183c7b36b9f41bd51c41ef4f86d6605b
plan_head_after: 540b3f4cb9fe1a992f4c952176f0cdf44e8833e3

tech-stack:
  added: []
  patterns:
    - "decision rule written into the evidence before the measurement it governs; the decision re-derived from rc and max RSS only"
    - "wrapped /usr/bin/time -l validated by a known-size control through the same time -> just -> shell -> process nesting"

key-files:
  created:
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-SETFIT-CELL-EVIDENCE.json
  modified:
    - justfile

key-decisions:
  - "the s32 cell ran because the s8 rehearsal exited 0 at 12509954048 bytes (11.65 GiB), under the 32 GiB rule, with the 36 GiB watchdog untouched"
  - "SetFit arm recorded at s32e1b16, a declared deviation from Phase 10's s64: after the 0.1 validation carve the pools are conspiratorial 36 and social_affiliative 59, so s32 is the largest legal shot count"

requirements-completed: [D-08, D-25, D-23, D-06]

coverage:
  - id: D1
    description: "laya-setfit-cell takes shots (default 64, validated against 8 16 32 64) and writes the probability dump from the test eval; only this plan's justfile hunks are committed"
    requirement: "D-23"
    verification:
      - kind: other
        ref: "Task 1 justfile verify: committed range start..HEAD equals the own patch as a +/- multiset; git diff HEAD -- justfile equals the owner's foreign patch; committed recipe has shots=\"64\", --shots {{shots}}, --dump-probabilities. Refusal `just laya-setfit-cell ... 7` printed `SETFIT CELL REFUSED select: shots 7 is not one of 8 16 32 64`"
        status: pass
    human_judgment: false
  - id: D2
    description: "s8 rehearsal memory measurement under /usr/bin/time -l, validated by a 4 GiB control, with a 36 GiB watchdog, against a rule written before the run"
    requirement: "D-08"
    verification:
      - kind: other
        ref: "Task 1 evidence verify (REHEARSAL OK rc 0 max_rss_GiB 11.7 -> proceed-s32); control 4320854016 >= 4294967296"
        status: pass
    human_judgment: false
  - id: D3
    description: "s32e1b16 cell completed on the shared split; accuracy recomputed from the dump is bit-equal to apr eval's value; row identity and dataset provenance match eval.jsonl and eval-test.json"
    requirement: "D-25"
    verification:
      - kind: other
        ref: "Task 2 evidence verify (SETFIT EVIDENCE OK completed); git ls-files for models/setfit and data/decide/motivation-v1-setfit is empty"
        status: pass
    human_judgment: false
---

# Phase 12 Plan 10: SetFit reference arm on the shared motivation split Summary

The SetFit reference arm (s32e1b16, seed 13, CPU) was measured on the shared 100-row eval split after an s8 rehearsal proved `apr eval` fits in memory, and its per-row probabilities are kept for the bake-off.

## What was done

**Task 1 (tracer).**
- Step 0: `CARGO_INCREMENTAL=0 cargo build --release --bin apr --features setfit` (rc 0), pinned with `scripts/apr_bin.sh`; `apr eval --help` lists `--dump-probabilities`. The binary was rebuilt after each commit that moved HEAD (apr_bin.sh checks HEAD), so the rehearsal ran on `apr 0.69.3 (bc52bf3d6)` and the s32 cell on `apr 0.69.3 (8be3707cf)`; neither commit touched Rust code.
- Adapter: `ADAPTED data/decide/motivation-v1-setfit labels=social_affiliative,conspiratorial,other train=[59,36,169] validation=[7,5,19] test=[23,14,63] dataset_fingerprint=e18595a2...` (pools after the carve match the plan: conspiratorial 36, social_affiliative 59, other 169).
- justfile (`bc52bf3d6`): `laya-setfit-cell` gains `shots="64"` validated against 8 16 32 64, the selection dir and `apr data select --shots` follow it, the test eval writes `--dump-probabilities "$OUT/test-probabilities.json"`, and the OK line names `cell=s<shots>e1b16`. Committed as own hunks only (+14/-9 lines); the owner's uncommitted region-parameter diff is still exactly as it was.
- Evidence skeleton with the memory rule, the 36 GiB stop rule and the quality exclusion was written before the rehearsal.
- RSS control: a 4 GiB allocation through `/usr/bin/time -l just --justfile /tmp/p12-10-ctl.just alloc` reported 4320854016 bytes, so the wrapped number includes grandchildren.
- Rehearsal (s8, under the host lock, watchdog beside it): rc 0, `maximum resident set size` 12509954048 bytes (11.65 GiB) against the 32 GiB rule, watchdog peak 5290573824 bytes (5 s sampling, so a lower bound) and not triggered, 100-row dump. Decision: proceed-s32. The rehearsal's accuracy is redacted in the evidence and excluded from every decision.

**Task 2.**
- s32 cell, detached under the host lock and timed: `SETFIT CELL OK cell=s32e1b16 seed=13 artifact_sha256=52227695fdb9f47c96dff1f5e32da364699ff8b12cc997f0470ccea4312abbe9 metric_tag=accuracy value=0.66 n_rows=100 train_seconds=748`. Whole-cell maximum resident set size 12519899136 bytes (11.66 GiB), wall 754 s, watchdog peak 8114634752 bytes, not triggered.
- Same-pass control: accuracy recomputed from the dump (66 of 100) is bit-equal (`float.hex`) to `eval-test.json`'s value; `predicted` is the argmax of every probability row; `row_ids` are `test:0`..`test:99`; per-row `text_sha256` equals the sha256 of the eval texts of `data/decide/motivation-v1/eval.jsonl` in order; the dump's `dataset_fingerprint` (`e18595a2...`) and `artifact_sha256` equal `eval-test.json`'s.
- The D-25 deviation is recorded: s64 infeasible (pools 36 and 59), s32 the largest legal cell, subsampled train (96 of 295 rows).
- Dump: `models/setfit/motivation-v1-s32e1b16-seed13/test-probabilities.json`, sha256 in the evidence; models and data outputs are gitignored and untracked.

Macro-F1, false-assignment and length bands are not computed here; plan 12-16's bakeoff.py does that from the dump. SetFit is a reference arm only (D-06): nothing served changes.

## Deviations from Plan

None - plan executed as written. Judgment calls inside the plan's latitude:

**1. [Clarification] A watchdog also ran beside the s32 cell.** The plan requires one only for the rehearsal; the same script guarded the host for the cell and recorded a 7.6 GiB peak (`cell_watchdog` in the evidence). It never triggered.

**2. [Clarification] The binary was rebuilt twice.** `scripts/apr_bin.sh` checks and never builds, and each justfile or evidence commit moves HEAD; the plan anticipates this ("Rebuild the same way after this task's justfile commit"). Rebuilds took about 80 s each.

**3. [Clarification] verify greps.** The Bash tool's rtk hook strips the `--` argument separator from `grep -q -- '--dump-probabilities'`, which then errors; the same assertion was run in python (`"--dump-probabilities" in help`) instead. Not a plan defect.

## Authentication Gates

None.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register (T-12-32 mitigated by the control, the 32 GiB rule, the 36 GiB watchdog and the host lock; T-12-33 by the same-pass and provenance controls; T-12-59 by the step 0 build and pin).

## Tracking updates

`gsd_run query state.*` / roadmap / requirements updates were not run: the known tooling issue (`04-VERIFICATION.md` with `status: gaps_acknowledged`) makes them error, and the instruction was to leave tracking to the orchestrator. STATE.md, ROADMAP.md and REQUIREMENTS.md are untouched; the pre-existing uncommitted `.planning` and foreign justfile changes were left alone.

## Commits

- `bc52bf3d6` feat(12-10): laya-setfit-cell takes a shots parameter and writes the probability dump
- `8be3707cf` docs(12-10): s8 rehearsal memory measurement and the s32 decision rule
- `540b3f4cb` feat(12-10): SetFit s32e1b16 cell of record with the same-pass probability dump

## Self-Check: PASSED

- FOUND: 12-SETFIT-CELL-EVIDENCE.json, justfile (recipe in HEAD), models/setfit/motivation-v1-s32e1b16-seed13/{cell.apr, eval-test.json, test-probabilities.json}
- FOUND commits: bc52bf3d6, 8be3707cf, 540b3f4cb (ancestors of HEAD)
