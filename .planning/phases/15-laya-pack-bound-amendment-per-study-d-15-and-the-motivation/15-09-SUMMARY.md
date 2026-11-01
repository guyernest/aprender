---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 09
subsystem: infra
tags: [laya, gap-closure, sc4, margin-miss, class-diagnosis, resampling, decision-brief]
requires:
  - phase: 15-08
    provides: 15-GATE-RUN-EVIDENCE.json (the consumed epochs-4 attempt, not-reproduced, next_trusted false)
  - phase: 14
    provides: 14-GATE-RUN-EVIDENCE.json (attempt 1, CUDA, 12 epochs)
  - phase: 12
    provides: 12-GATE-RUN-EVIDENCE.json (MPS, 12 epochs, the draw the band was set around)
provides:
  - scripts/laya_deploy/laya_margin_diagnosis.py with a justfile wrapper (snapshot, verify-inputs, analyze, check-diagnosis, check-brief, check-reply, write-disposition, check-disposition, selftest)
  - 15-GAP-MARGIN-INPUTS.json (text-free fixture; all nine margins and ECEs recompute bit for bit)
  - 15-GAP-CLASS-DIAGNOSIS.json (class single-draw-acceptance, five options, rule-derived recommendation mean-rule)
  - 15-GAP-DECISION-BRIEF.md (the owner's brief for plan 15-10)
affects: [15-10, 15-11, 15-12]
tech-stack:
  added: []
  patterns: [rule pre-registered in a commit before the analysis ran, rule_sha256 binds the diagnosis to it, brief_facts verbatim check, replies judged on fields not on verbatim text]
key-files:
  created:
    - scripts/laya_deploy/laya_margin_diagnosis.py
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GAP-MARGIN-INPUTS.json
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GAP-CLASS-DIAGNOSIS.json
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GAP-DECISION-BRIEF.md
  modified:
    - justfile
key-decisions:
  - "The SC4 miss is class single-draw-acceptance (all five predicates hold): shortfall 0.0205 is inside the CUDA between-seed sd 0.0441 and the median row-resampling se 0.0642, and the declared criterion passes in 0.259 of resampled triples. Recipe deficiency is refuted on the full set but only narrowly (3 of 6 leave-one-out sets are undetermined); device anchoring is unproven (n=3, p 0.1012)."
  - "Recommendation is the rule's output, mean-rule (p_pass 0.634 against 0.259 for redraw); it supersedes D-11 and needs the owner's recorded reply. stop-accept is named as the alternative if the owner will not change the judging rule after two misses. Nothing was decided or authorized here."
  - "No trust is set: next_trusted_effect none, rejudge exploratory, both consumed attempts rejudged_for_trust false."
requirements-completed: [D-02, D-03, D-11, D-12, D-13, D-14]
duration: about 1 session
completed: 2026-11-01
status: complete
actuals:
  tokens: 43900
  tasks: 3
  commits: 4
plan_head_before: cab8f0956e588e52ea24b04a64531595ffb28581
plan_head_after: b34768a2d85d67dbdcb77177763814c43020e5c5
---

# Phase 15 Plan 09: The class of the SC4 margin miss Summary

**The two margin misses (0.2860, then 0.2795 against the 0.30 floor) are classed single-draw-acceptance by a rule committed before the analysis ran: the shipped-seed margin is one draw from a distribution (CUDA mean 0.3279, sd 0.0441, six seed-runs) whose spread and 100-row resampling error exceed the 0.0205 shortfall, so the declared criterion passes in only 0.259 of resampled triples and a further attempt under the same rule re-draws it. The owner now holds a machine-checked brief with five concrete options; nothing was spent, decided or trusted.**

## What was built

- **Task 1 (tracer, 8ed43914c).** `laya_margin_diagnosis.py` `snapshot` (the only step that reads gitignored files) wrote `15-GAP-MARGIN-INPUTS.json`: class indices and floats only, row alignment proven 100 of 100 by text hash, the Phase 12 per-seed `eval_probs_sha256` matched. `verify-inputs` recomputes all nine seed-runs from first principles and matched the tracked evidence on the first run: `INPUTS OK seed_runs=9 margins_exact=9 ece_exact=9 zero_shot_f1=0.3695652173913044`. A second snapshot is byte-identical. The red side (flipped y, flipped pred, a 1e-6 confidence move, a dropped seed-run, a text key, a 300-character string, an edited evidence digest, a duplicate key, a zero-shot mismatch) is refused in the selftest.
- **Task 2 (57483042e, then 16ca6f9b2).** The first commit holds RULE_SPEC, the pure statistics and the selftest cases, before `analyze` existed. The second adds `analyze` and `check-diagnosis` and the diagnosis. `rule_sha256` of the committed RULE_SPEC (7f94f41789ce) equals the hash in the diagnosis (verified by loading the first commit's file). All planner scratch values matched within the 1e-9 tolerances: CUDA mean 0.32788327435520637 and sd 0.04412368663280948, declared p_pass 0.25925925925925924 with leave-one-out 0.104 to 0.352, mean-rule 0.6342592592592593, floor-rule 0.7407407407407407 (floor 0.23), device-gap mean 0.0397 and p 0.1012, epochs difference +0.0023, median row-resampling se 0.0642 (range 0.0568 to 0.0765). `SCRATCH CROSS-CHECK OK`, second analyze byte-identical, `DIAGNOSIS OK class=single-draw-acceptance recommended=mean-rule`.
- **Task 3 (b34768a2d).** `check-brief`, `check-reply`, `write-disposition` and `check-disposition` with a refusal case for every listed defect, and `15-GAP-DECISION-BRIEF.md` (ten headings in order; `BRIEF OK facts=35 options=5`; `BRIEF STRUCTURE OK`). Replies and dispositions are judged on fields: the literal reply `go`, a verbatim text containing `run-go` with `run_go` false, and a verbatim text naming another option each behave correctly in the selftest. A CLI smoke test with a throwaway reply (not committed) ran check-reply, write-disposition and check-disposition green.

Selftest: `MARGIN DIAGNOSIS SELFTEST OK cases=103`, zero FAIL lines. `just --summary` lists `laya-margin-diagnosis` once. `contracts`, `.github` and `crates` are identical to HEAD; the consumed declaration and gate-run evidence are untouched.

## What the numbers say (for the owner, via the brief)

- Gate (shipped ECE at most 0.10) passes with probability 0.741 under every rule, so no judging rule gets above 0.741 on this recipe.
- The declared rule's failures are 0.481 defect (margin outside its band) and 0.259 in-band gate fail. Two misses in a row under it are not unlikely (0.549 if independent).
- The 12 -> 4 epochs lever did nothing detectable (+0.0023, se 0.0403); one more 3-seed run could only detect a difference of about 0.1127, so a new lever cannot be shown by one more attempt.
- In both CUDA runs the shipped seed (median ECE) was the lowest-margin seed of its run; in Phase 12 it was not.
- Caveats the brief states: the pass probabilities are resampling estimates over six seed-runs, not confidence intervals; the floor-rule's 0.741 is in-sample because its floor (0.23) is derived from the same six draws; the leave-one-out range is a sensitivity check.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical functionality] brief_facts extended after Task 2's diagnosis commit**
- **Found during:** Task 3, writing the brief.
- **Issue:** the plan's brief_facts list left the per-option leave-one-out ranges, claim scopes, the shared run parameters, the gate probability and the nine table rows without a machine check, so those figures in the brief could have drifted from the diagnosis.
- **Fix:** `brief_facts()` in `laya_margin_diagnosis.py` now also emits the nine table rows, one line per option (judging, bands, cap, retry classes, supersedes, p_pass, within cap, leave-one-out range), each paid option's claim scope, and the shared run line. This is presentation only: `RULE_SPEC` and `rule_sha256` are unchanged, no statistic changed, the diagnosis was regenerated and `check-diagnosis` passes. The brief then has 35 verbatim-checked facts instead of about 12.
- **Files modified:** `scripts/laya_deploy/laya_margin_diagnosis.py`, `15-GAP-CLASS-DIAGNOSIS.json`
- **Commit:** b34768a2d

**2. [Interpretation] options carry exactly the 17 listed keys; shared run parameters live in `options_common`**
- The plan lists seeds, device and the pack image for each paid option but also says each option object has exactly 17 keys that do not include them. They are recorded once in a top-level `options_common` block (seeds [13, 17, 23], device cuda, pack image dd38865c...) and in the shared brief_facts line. Plans 15-10 to 15-12 read option key names, which are as listed.

**3. [Interpretation] definitions the plan left open**
- `median_se` is the median of the nine seed-runs' row-resampling standard errors. The bootstrap uses one fixed index set family (seed 20261031, B 2000) for all nine seed-runs. For `mean-rule-retry`, `within_cap` is p_pass + p_in_band_gate_fail x p_pass and `loo_min`/`loo_max` are the leave-one-out range of `within_cap` (identical to p_pass's range for the cap-1 options). `loo` entries also carry `within_cap2`.

None of these changed a number the plan's cross-check tests.

## Issues Encountered

- State tooling, as warned: `state.advance-plan`, `state.record-metric` and `state.record-session` each exited with the known Phase 4 error (`04-VERIFICATION.md` status `gaps_acknowledged` is outside the tool's closed set). `04-VERIFICATION.md` was not re-keyed and STATE.md was left untouched, so its position, metrics and session fields do not yet reflect plan 15-09 and need the orchestrator's attention. `state.update-progress` skipped itself ("phase scope is unscoped"). `roadmap.update-plan-progress 15` worked (9/12 plans, 15-09 checked). `requirements.mark-complete` found none of D-02, D-03, D-11, D-12, D-13, D-14 in REQUIREMENTS.md (they are phase-context decision ids, not requirement rows) and wrote nothing.
- No checkpoint, owner decision or AWS call was needed. Nothing was paid for, launched, deployed or uploaded (D-03).

## Known Stubs

None. The only deferred items are those the plan names (MPS-versus-CUDA explanation, dataset growth, a pack_laya change) and they are untouched.

## Threat Flags

None. The script is stdlib-only and offline; its only gitignored reads are in `snapshot`, which keeps class indices and floats (T-15-45 mitigated structurally by verify-inputs: no key named text or texts, no string over 200 characters).

## Next

Plan 15-10 presents `15-GAP-DECISION-BRIEF.md` to the owner and the orchestrator records the reply in `laya-gap-owner-reply-v1` form (`check-reply`, then `write-disposition`, `check-disposition`). No executor relays an approval. `scan-identifiers` and `check-run-go` are added by plans 15-11 and 15-12.

## Self-Check: PASSED

All five created files exist (script, fixture, diagnosis, brief, this summary) and the four task commits (8ed43914c, 57483042e, 16ca6f9b2, b34768a2d) are ancestors of HEAD. `commits: 4` was measured from the ledger base cab8f0956 to b34768a2d.
