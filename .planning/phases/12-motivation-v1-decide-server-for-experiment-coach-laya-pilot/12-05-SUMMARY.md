---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 05
subsystem: laya-back-office-bakeoff
tags: [laya, bakeoff, metrics, false-assignment, ece-bootstrap, length-bands, row-identity, counts-only]
status: complete

requires:
  - phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
    provides: scripts/laya_train/metrics.py (macro_f1, ece_top_label, numeric_cases.json), data.py (exact_sha256, load_rows, class_counts), contract.py thresholds
provides:
  - "metrics.py: precision_recall_per_class, false_assignment_rate(strict=True|False), recall_at_threshold, ece_bootstrap_ci, check_distribution, length_tertile_cuts, length_band (+ 21 selftest behaviour cases; numeric_cases.json untouched)"
  - "bakeoff.py: --data/--laya/--setfit/--kev/--out report (schema motivation-bakeoff-v1), --render-tables EVIDENCE, --selftest (35 cases)"
  - "refusals bakeoff-alignment (incl. `setfit row identity`, `setfit dump lacks row identity`, `setfit truth`, `<arm> distribution row <i>`) and bakeoff-laya-recompute"
affects: [12-10, 12-11, 12-15, 12-16]

actuals:
  tokens: 16300
  tasks: 2
  commits: 3
plan_head_before: 2faeeef6ecb5521933d61733b4685d2beb8db0c5
plan_head_after: 8437da91711f1788b03d0697265bd002c3e7d569

tech-stack:
  added: []
  patterns:
    - "one metrics module (OPS-03): the report script calls metrics.* and computes nothing itself; the selftest checks it against an independent plain-Python count"
    - "alignment by row identity (index + text_sha256; SetFit row_ids), truth labels only as a secondary check"
    - "rules written in the docstring before any arm exists; a module constant (0.05) is asserted equal to the contract's gate margin by the selftest"
    - "hand-off tables are generated from the evidence file (--render-tables), deterministic bytes"

key-files:
  created:
    - scripts/laya_train/bakeoff.py
  modified:
    - scripts/laya_train/metrics.py

key-decisions:
  - "laya_zero_shot is reported beside the arms as a baseline and is NOT ranked; the ranking and the clearly-beats finding cover laya, setfit, kev only"
  - "recall_at_threshold is p_k strictly > threshold (for any threshold >= 0.5 that already means k is the top label, so no separate argmax condition)"
  - "Laya's gate-report seeds.per_seed rows are cross-checked against seeds/seed-<s>/eval-probs.json with exact equality, beyond the three fields the plan names; absent per_seed (single-seed run) skips only this cross-check"
  - "Laya and Kev probabilities are stored f32 values widened to float64, so a value printing as 0.6 is above 0.6 under strict >; this is written into threshold_note and the docstring"
  - "a blocked arm's evidence file is hashed when it exists and recorded with evidence_sha256 null when it does not (reported, not refused)"

requirements-completed: [D-04, D-05, D-06, D-13, D-14]

coverage:
  - id: D1
    description: "metrics.py carries the six new bake-off functions plus check_distribution, with hand-computed selftest cases, and numeric_cases.json is unchanged"
    requirement: "D-05"
    verification:
      - kind: unit
        ref: "uv run --project scripts/laya_train --frozen python -I scripts/laya_train/metrics.py --selftest (METRICS SELFTEST OK, 21 bake-off function cases)"
        status: pass
    human_judgment: false
  - id: D2
    description: "bakeoff.py produces the full counts-only report for three arms plus the zero-shot baseline: false assignment (strict and client_ge) per arm and per length band at 0.5/0.6/0.7/0.85, recall at 0.6/0.85, per-class precision/recall with counts, ECE with class-stratified bootstrap interval (per band only at 30+ rows, else not_reported)"
    requirement: "D-14"
    verification:
      - kind: unit
        ref: "scripts/laya_train/bakeoff.py --selftest (end to end cases, independent plain-Python count of every false-assignment numerator)"
        status: pass
    human_judgment: false
  - id: D3
    description: "bakeoff.py refuses misaligned or inconsistent input (SetFit row identity incl. two swapped same-label rows, Kev index/hash, bad distributions, Laya gate-report recompute) and reports blocked arms, the tie rule and the clearly-beats finding exactly as declared"
    requirement: "D-06"
    verification:
      - kind: unit
        ref: "scripts/laya_train/bakeoff.py --selftest (BAKEOFF SELFTEST OK (35 cases))"
        status: pass
    human_judgment: false
  - id: D4
    description: "--render-tables prints the lead (false assignment per arm and band at 0.6/0.85 with numerators, denominators and client_ge), recall, ranking, per-class and ECE tables deterministically"
    requirement: "D-05"
    verification:
      - kind: unit
        ref: "scripts/laya_train/bakeoff.py --selftest (same bytes twice; blocked (compute) rendering)"
        status: pass
    human_judgment: false
  - id: D5
    description: "The report is exercised on synthetic arm files only; no real arm's number exists yet, so the rules are untested against real data and the Laya recompute is untested against a real run dir"
    verification: []
    human_judgment: true
    rationale: "Plans 12-10/12-11/12-15 produce the real inputs and 12-16 runs the script; the first real run is the check that the synthetic gate-report layout matches train.py's real one"

duration: ~20min
completed: 2026-10-26
---

# Phase 12 Plan 05: Bake-off measurement Summary

**One metrics module (six new functions plus a distribution check) and a counts-only `bakeoff.py` that ranks Laya, SetFit and Kev by macro-F1 and leads with the false-assignment rate per arm and per length tertile, aligned by row identity, with the rules written before any arm exists.**

## Performance

- **Duration:** ~20 min (start time not recorded; estimated from the previous plan's last commit at 11:40 to the last task commit at 11:50, -07:00)
- **Tasks:** 2 of 2 (Task 1 tracer, Task 2 TDD)
- **Files:** 2 (1 created, 1 modified)

## Accomplishments

- `metrics.py` gains `precision_recall_per_class`, `false_assignment_rate` (strict `>` by default, `strict=False` for the client-equivalent `>=`), `recall_at_threshold`, `ece_bootstrap_ci` (class-stratified, `RandomState(13)`, calls `ece_top_label`), `check_distribution`, `length_tertile_cuts`, `length_band`. 21 hand-computed selftest cases run before the unchanged `METRICS SELFTEST OK (5 frozen cases ...; 23 numeric cases bit for bit)` line. `numeric_cases.json` is untouched.
- `bakeoff.py` builds the full report (`motivation-bakeoff-v1`): eval counts and length cuts, `pi_train`, four arms (laya, laya_zero_shot, setfit, kev), `laya_seeds`, ranking, findings, `source_batch: unmeasured`, `threshold_note`, `not_a_gate: true`, input hashes. A production-size run (10,000 resamples, 4 arms, 99 synthetic rows) takes about 6 s.
- `--render-tables EVIDENCE` prints the LEAD, RECALL, RANKING, PER-CLASS and ECE markdown tables from an evidence file, same input same bytes, `blocked (<class>)` for a blocked arm.

## Declared rules (verbatim from the bakeoff.py docstring, written before any arm was read)

- **arms:** laya (eval-probs.json of the Laya run dir: the shipped median-ECE seed, calibrated), setfit and kev (reference arms, D-06). laya_zero_shot (zero-shot-probs.json, the declared base on the same rows) is reported beside them as a baseline and is not ranked. A blocked arm is reported with its failure_class and evidence path and is absent from the ranking.
- **ranking:** by metrics.macro_f1 on the shared eval split, descending (D-05).
- **tiebreak:** two arms are TIED when their macro-F1 is equal rounded to 4 decimals; the tie is broken by the LOWER false-assignment rate at 0.6 (experiment-coach's motivation_min_prob), then by the unrounded macro-F1. The false-assignment rate is never used any other way: there is no false-assignment ceiling.
- **clearly beats:** a reference arm whose macro-F1 exceeds Laya's by at least 0.05 (the gate's own margin threshold, laya-finetune-gate-v1 gate_min_macro_f1_margin, checked against the contract by --selftest) is written as the finding `reference_arm_clearly_beats_laya`. It is a finding, never a switch: Laya stays the served arm (D-06).
- **thresholds:** 0.5, 0.6, 0.7 and 0.85. The false-assignment rate of an arm is the share of its eval `other` rows whose largest non-`other` probability is STRICTLY greater than the threshold (D-05, "exceeds"), over all its eval `other` rows. Beside it, `client_ge` is the same count with `>=` (experiment-coach assigns at p >= min_prob, crates/core/src/treatment.rs:59): reported only, never ranked. Probabilities are compared as float64; Laya's and Kev's are stored f32 values widened, so a value that prints as 0.6 is the f32 0.60000002 and lies above 0.6.
- **length:** len(text) in code points of the eval.jsonl text. Bands are the tertiles of the eval lengths: cut points c1, c2 = numpy.percentile at 100/3 and 200/3 (recorded), band 0 if len <= c1, 1 if len <= c2, else 2 (D-14). Per-class precision and recall, per-class counts and the false-assignment rates are reported inside each band for every arm, the counts beside every rate.
- **ECE:** the house top-label ECE (metrics.ece_top_label, 15 bins from the contract) with a class-stratified percentile bootstrap 95 % interval, 10,000 resamples, numpy RandomState(13) (D-13). Reported, not a gate. The interval is checked only for 0 <= lo <= hi <= 1 and for determinism, NEVER for containing the point. Inside a length band the interval is reported only for a band of at least 30 rows; a smaller band carries its point and the string `not_reported`.
- **source batch:** UNMEASURED: the mapping is not available (D-04, D-15). Nothing is computed by source.
- **counts only:** no post text is read into the report, printed or written; row identity is by index and sha256.

## Selftest case count

- `metrics.py --selftest`: 21 bake-off function cases, then the unchanged frozen (5) and numeric (23) replays; final line `METRICS SELFTEST OK (5 frozen cases replayed within 1e-06; 23 numeric cases bit for bit)`.
- `bakeoff.py --selftest`: **`BAKEOFF SELFTEST OK (35 cases)`** (plan minimum 17): 10 from the tracer (full report, independent count, bands, ECE bounds and determinism, 60-row `not_reported` band, render determinism) plus 25 added by Task 2.

## Task Commits

1. **Task 1 (tracer): metric functions and a full report end to end** - `f6c501bfd` (feat)
2. **Task 2 RED: failing selftest cases for refusals and declared rules** - `be6feb4d1` (test)
3. **Task 2 GREEN: refusals, blocked arms, tie and clearly-beats rules** - `8437da917` (feat)

## TDD Gate Compliance

- **RED (`be6feb4d1`):** 25 cases added, 17 failed on their planned assertions at that commit: the seven alignment cases (exit 0 where a `REFUSED bakeoff-alignment: ...` exit 2 was expected), the four Laya recompute cases (exit 0), four blocked-arm cases (`--kev blocked:...` was read as a path: `REFUSED ... kev probability file is missing`; the malformed form likewise), the tie rule (`tiebreak_applied` all False) and the findings case (`[]`). The other 8 new cases (two distribution refusals, the tie negative, the margin constant, the no-finding case, threshold equality, both sentinel cases) already held from the tracer commit; they are regression guards, not RED.
- **Semantic assessment:** each failing case reached its assertion with the target code running (no import, syntax, fixture or zero-test fault); the fixture builder and the unchanged happy path were green in the same run. `gsd_run check tdd-red-evidence` was not run: the selftest's text format is not one of the classifier's supported report formats, so the evidence above is the unmodified FAIL lines of the RED run, not a classifier verdict.
- **GREEN (`8437da917`):** 35/35 cases pass. No REFACTOR commit (no change warranted beyond one `data.class_counts` swap folded into GREEN).

## Files Created/Modified

- `scripts/laya_train/bakeoff.py` - the three-arm counts-only report, refusals, table renderer and selftest (new)
- `scripts/laya_train/metrics.py` - the bake-off functions and their selftest cases (modified)

## Decisions Made

See `key-decisions`. The one worth restating: the strict-`>` rate is the declared metric and `client_ge` rides beside it unranked, so plan 12-16 can disclose the difference at exact equality; the f32-widening nuance means that difference is mostly invisible for Laya and Kev (their stored values are rarely exactly a decimal threshold) but real for the SetFit f64 dump.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] Laya `seeds.per_seed` cross-check**
- **Found during:** Task 2
- **Issue:** the plan names three recomputed gate-report fields; the gate report also carries per-seed `macro_f1`/`ece_post` for the seeds the report lists, and `laya_seeds` is computed from the seed files. An unchecked per_seed table could disagree with the files the report is built from.
- **Fix:** `check_laya_recompute` also compares each `seeds.per_seed` row (and the seed set) with `seeds/seed-<s>/eval-probs.json`, exact equality, refusal `bakeoff-laya-recompute: seeds.per_seed[<s>].<field>`. Skipped only when the gate report has no `per_seed`.
- **Files modified:** scripts/laya_train/bakeoff.py
- **Commit:** `8437da917`

**2. [Rule 1 - Interpretation] laya_zero_shot not ranked**
- The plan says bakeoff "ranks arms" and lists `laya_zero_shot` as a second Laya arm; ranking a baseline beside the three arms would make the clearly-beats and tiebreak rules ambiguous. It is reported in `arms` and the tables as a baseline and excluded from `ranking`/`findings`. No plan text was contradicted.

### Staging note

Task 1 shipped the loaders with count/label checks and distribution validation only; row-identity alignment, the Laya recompute, blocked arms, the tie rule and the findings were added in Task 2 so the RED run was meaningful. The tracer commit is therefore not safe to use alone on real data; the plan's two tasks are one unit.

**Total deviations:** 2 (1 Rule 2 addition, 1 interpretation). **Impact:** none on plan goals; both are stricter or clearer, not looser.

## Issues Encountered

- The synthetic `gate-report.json` in the selftest mirrors `train.py`'s report layout (`fine_tuned`, `zero_shot`, `seeds.per_seed`) as read from `train.py`, not from a real run directory. The first real run (plan 12-16) is the check; a layout drift would surface as a `bakeoff-laya-recompute` refusal naming the missing field, never a silent pass.
- Tracking updates (`state.*`, `roadmap.update-plan-progress`, `requirements.mark-complete`) were not attempted: `gsd_run` queries error on `04-VERIFICATION.md` `status: gaps_acknowledged` (known, not edited). Left to the orchestrator. Requirement ids in the plan are CONTEXT decision ids (D-04, D-05, D-06, D-13, D-14), not REQUIREMENTS.md rows.

## Known Stubs

None. Nothing in the two files is a placeholder; the only empty-looking value is `evidence_sha256: null` for a blocked arm whose evidence path does not exist, which is a reported state, not a stub.

## Threat Flags

None. The scripts read local files and write one JSON; no network, auth or schema surface. T-12-18 (alignment/recompute) and T-12-19 (no text) are mitigated as planned and exercised by the selftest, including the sentinel with a capture control over 38 captured streams.

## Next Phase Readiness

Plans 12-10 (SetFit dump), 12-11 (Laya run dir) and 12-15 (Kev eval-probs) feed `--setfit`, `--laya`, `--kev`; plan 12-16 runs `bakeoff.py ... --out 12-BAKEOFF-EVIDENCE.json` and pastes `--render-tables`. Inputs must be: SetFit = the nine-key dump of plan 12-06; Kev = `eval_probs_schema`; Laya = a run dir with `eval-probs.json`, `zero-shot-probs.json`, `gate-report.json`, `seeds/seed-<s>/eval-probs.json`.

## Self-Check: PASSED

- scripts/laya_train/bakeoff.py and scripts/laya_train/metrics.py exist; commits f6c501bfd, be6feb4d1, 8437da917 are on the branch; `metrics.py --selftest` and `bakeoff.py --selftest` both exit 0 on the final tree; `numeric_cases.json` has no diff.
