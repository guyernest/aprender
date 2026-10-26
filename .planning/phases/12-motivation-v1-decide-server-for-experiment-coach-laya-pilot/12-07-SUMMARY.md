---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 07
subsystem: laya-finetune-calibration
tags: [laya, probe, spike, calibration-slice, temperature-cap, early-stopping, mps, measurement]
status: complete

requires:
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: plan 12-01 approved task.json and data/decide/motivation-v1 (12-TASK-EVIDENCE.json)
provides:
  - "probe_epoch.py --rule table (antisemitism-v1 default unchanged, motivation-v1 added); evidence key rule"
  - "12-PROBE-EVIDENCE.json: motivation-v1 rule, mps:0, 1.468 s/step, epochs_declared 12"
  - "spike 032 (README with the SLICE and CLAMP-BINDING rules committed before any run, run_spike.py, MANIFEST row)"
  - "12-SPIKE-EVIDENCE.json (completed): slice_fraction_selected 0.25, clamp_binding false"
  - "12-SPIKE-OWNER-REPLY.json: skip record (no trigger)"
affects: [12-08, 12-11]

actuals:
  tokens: 6800
  tasks: 3
  commits: 4
plan_head_before: e928cbe3fb5cc3cd20e83bcd5644387a6c458ec6
plan_head_after: bcb3763ff0807c00f44d7aa1794dd391769f0b8a

tech-stack:
  added: []
  patterns:
    - "rules committed before the first run; the verify compares the commit time with every result"
    - "python audit hook refusing the gate's eval.jsonl, proven by a selftest and a per-process attempt counter"
    - "cache bound to an identity (data, E, fraction, seed, harness bytes, AST of the training path), never to file existence"

key-files:
  created:
    - .planning/spikes/032-motivation-slice-and-tcap/README.md
    - .planning/spikes/032-motivation-slice-and-tcap/run_spike.py
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-PROBE-EVIDENCE.json
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-SPIKE-EVIDENCE.json
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-SPIKE-OWNER-REPLY.json
  modified:
    - scripts/laya_train/probe_epoch.py
    - .planning/spikes/MANIFEST.md

key-decisions:
  - "E = 12: the probe measured 1.468 s/step on mps:0 (monitor 2.15 s/epoch); three-seed hours at E = 12 are 0.43 h, far under the 6 h budget, so the declared rule gives the contract maximum"
  - "Calibration slice stays 0.25 (the contract value): 0.35 lowers median holdout ECE by 0.037 but costs 0.132 of median margin"
  - "The T <= 5 clamp does not bind at 0.25: slice-fitted T 4.54 / 1.64 / 3.35, none above 5.0, no oracle rescue; no owner checkpoint"

requirements-completed: [D-17, D-18, D-21, D-13]

metrics:
  duration: "about 1 h 15 min wall (probe 3 min, nine spike cells about 25 min of MPS)"
  completed: 2026-10-26
---

# Phase 12 Plan 07: Epochs rule, probe and calibration-slice spike Summary

**E = 12 from a timed probe and the 0.25 calibration slice kept by a nine-cell spike whose rules were committed before the first run; the T <= 5 clamp does not bind, so no owner checkpoint fired.**

## Probe line

`PROBE OK device_used=mps:0 s_per_step=1.4679 steps=28 P=97` (rc 0). Rule `motivation-v1`, fit rows 220, calibration rows 75, host not contended (5 samples, no competing builds), no row truncated. Projected three-seed hours: 0.14 (E = 4) to 0.43 (E = 12). **E = 12** (`epochs_declared`), as the rule gives when the E = 12 hours are at most 6.

## State: completed

The nine cells (fraction x seed, early stopping, E = 12, all on mps:0, every result carrying the current cache identity):

| fraction | seed | T unbounded | T oracle | ECE at clamped T | ECE at oracle | margin | best / run epochs |
|---|---|---|---|---|---|---|---|
| 0.25 | 13 | 4.544 | 5.26 | 0.0798 | 0.0752 | 0.2306 | 2 / 5 |
| 0.25 | 17 | 1.643 | 2.46 | 0.1176 | 0.0649 | 0.0730 | 1 / 4 |
| 0.25 | 23 | 3.349 | 4.01 | 0.1169 | 0.0845 | 0.2387 | 2 / 5 |
| 0.35 | 13 | 4.354 | 4.18 | 0.0437 | 0.0407 | 0.2513 | 2 / 5 |
| 0.35 | 17 | 1.852 | 2.16 | 0.0802 | 0.0511 | 0.0532 | 1 / 4 |
| 0.35 | 23 | 1.292 | 0.98 | 0.1045 | 0.0793 | 0.0988 | 1 / 4 |
| 0.5 | 13 | 1.299 | 1.23 | 0.0984 | 0.0782 | -0.0146 | 1 / 4 |
| 0.5 | 17 | 2.035 | 2.48 | 0.1499 | 0.1008 | 0.1694 | 2 / 5 |
| 0.5 | 23 | 2.284 | 2.66 | 0.1137 | 0.0670 | 0.2088 | 2 / 5 |

Medians (ECE at clamped T / margin): 0.25: 0.1169 / 0.2306; 0.35: 0.0802 / 0.0988; 0.5: 0.1137 / 0.1694.

- **Selected fraction: 0.25.** 0.35 meets the ECE leg (drop 0.0367) but fails the margin leg (cost 0.1317 > 0.02); 0.5 drops ECE by only 0.0032.
- **Clamp verdict: not binding** (`clamp_not_binding`): 0 of 3 seeds with an unbounded T above 5.0 and 0 of 3 oracle-rescue seeds at 0.25.
- `eval_jsonl_opened` false (guard selftest refused 4 of 4 open paths; every cell recorded 0 attempts).

Plan 12-08 needs no fraction override (0.25 is the contract value); plan 12-11 declares `slice_fraction_selected = 0.25` and E = 12.

## Owner record

`12-SPIKE-OWNER-REPLY.json` is a skip record (schema v2, state completed, history empty): clamp not binding, probe on a non-cpu device and not contended.

## Informational for later plans (not a rule input)

At 0.25 the median holdout ECE at the clamped T is 0.117, above the gate's 0.10. It rests on about 11 conspiratorial holdout rows (spike 027 saw ECE move 0.048 between MPS replicates), and early stopping picks epoch 1 or 2 on every cell. An ECE-only gate failure is plausible; D-19 covers that case (never margin, D-20).

## Deviations from Plan

None - plan executed exactly as written, with these choices inside the plan's latitude:
- `ece_at.fitted5` and `ece_at.t5` are both the holdout ECE at the slice-fitted T clamped to [0.5, 5] (the rules' "at the clamped T"); the README states they are equal by construction. An extra `ece_at.exact5` (ECE at T = 5.0 exactly) is recorded for information.
- A crashed cell's directory would be moved to `models/decide/spike-032/stale/` on retry, never deleted; no cell crashed.

## Authentication gates

None.

## Known Stubs

None.

## Tooling note

`gsd_run` state and roadmap updates were not run: the known `status: gaps_acknowledged` in the Phase 4 verification file (outside the tool's closed set) makes `state.*` and `roadmap.*` queries error, and the orchestrator owns tracking. STATE.md, ROADMAP.md and REQUIREMENTS.md were not touched by this plan.

## Self-Check: PASSED

- Files exist: probe_epoch.py, README.md, run_spike.py, 12-PROBE-EVIDENCE.json, 12-SPIKE-EVIDENCE.json, 12-SPIKE-OWNER-REPLY.json.
- Commits in HEAD history: 772af7ab9, 66347039c (rules, before the first result), df1356767, bcb3763ff.
- Task 1, 2 and 3 verifies re-run and green; no `models/decide/laya-motivation-v1` run dir; no contract changed; no spike checkpoint kept.
