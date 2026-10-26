---
spike: 032
idea: llm-decision-classifier
name: motivation-slice-and-tcap
type: standard
validates: "Given the approved motivation-v1 task (about 220 spike-train rows after a 25 % holdout is carved from train.jsonl), when Laya en-root is fully fine-tuned with early stopping at the probe-declared epochs over calibration-slice fractions {0.25, 0.35, 0.5} x training seeds {13, 17, 23}, then the holdout ECE at the slice-fitted T (unbounded, [0.5, 5], [0.5, 10]), the holdout-oracle T and the margin over zero-shot show which slice fraction to declare for the one gate run and whether the T <= 5 clamp binds"
verdict: pending
related: [027, 028]
tags: [laya, calibration, temperature-scaling, ece, gate, motivation-v1, thin-data, mps]
---

# Spike 032: motivation-v1 calibration slice x temperature cap

> **This is a MEASUREMENT, not a gate run.** No run here is a declared `laya-finetune-gate-v1` run and none may be
> read as one. Every run dir carries `NOT-A-GATE-RUN.txt`; no `models/decide/laya-motivation-v1` run dir exists
> afterwards; no contract value changes in plan 12-07. The rules below are committed BEFORE the first spike run.

## The question

motivation-v1 has the stance demo's thin-data shape: about 41 conspiratorial train rows before the calibration slice
is drawn. In Phase 8 a 12-row slice and the `calibration_temp_max` 5.0 cap made ECE fail twice (spike 027). Two
numbers must be fixed before the one gate run is declared (D-17):

1. the calibration-slice fraction (the contract's `calibration_slice_fraction` is 0.25), and
2. whether the T <= 5 clamp binds on this dataset (D-18). If it does, the owner decides; nothing in this phase
   raises the cap.

Epochs are not in question here: they come from `12-PROBE-EVIDENCE.json` (`epochs_declared`), fixed by the rule
table in `scripts/laya_train/probe_epoch.py` (`--rule motivation-v1`) written before the probe.

## Data rule (D-13 / RESEARCH Q3: no second look at the test set)

Only `task.json` and `train.jsonl` of `data/decide/motivation-v1` are read. The gate's `eval.jsonl` is NEVER
opened: `run_spike.py` installs a Python audit hook (`sys.addaudithook`) at import that refuses any `open` of its
absolute path with `REFUSED spike-eval-guard`, `--guard-selftest` proves the refusal, and every result records its
process's attempt counter (the evidence field `eval_jsonl_opened` is false only when all counters are 0).

The holdout is carved from train by re-applying the eval rule (seed 13, stratified, `ceil(0.25 x class)` per class,
the same RandomState permutation `prepare_motivation.py` uses) to the 295 train rows: holdout 75 rows
(social_affiliative 17, conspiratorial 11, other 47), **spike-train 220 rows** (49, 30, 141). Holdout and
spike-train are written write-once to `data/decide/motivation-v1-spike/` (gitignored) with their sha256s.

## The grid

Calibration-slice fractions **0.25, 0.35, 0.5** x training seeds **13, 17, 23** (nine cells), slice seed 13
(`data.calibration_split(spike_train, F, 2, 13, 3)`; the contract value is never edited), early stopping
(`stopping = early_stopping`, the contract's block), epochs E from `12-PROBE-EVIDENCE.json`, D-21: the file's prior,
the unchanged recipe, no class reweighting. Cells are trained by `train.SeedRun.train_and_score` unchanged.
At fraction 0.25 the slice is `max(2, ceil(0.25 n))` per class = [13, 8, 36] and the fit set has 163 rows.
`KEEP 0`: every checkpoint is deleted after scoring.

## Recorded quantities (per cell, `result.json`)

`identity`, fraction, seed, `slice_per_class` (label order), `fit_rows`, `best_epoch`, `epochs_run`;
`t_unbounded` (slice-fitted T on [0.05, 1000]), `t_clamped5` / `t_clamped10` (the fit clamped to [0.5, 5] /
[0.5, 10]), `clamp_hit5`, `t_oracle` (holdout-ECE-minimising T on a 461-point log grid 0.5..50: a diagnostic that
reads the holdout labels, never a selection rule for the gate), `ece_at` = `{fitted5, t5, fitted10, oracle}`
(`fitted5` and `t5` are both the holdout ECE at the slice-fitted T clamped to [0.5, 5], i.e. "at the clamped T" of
the rules; `t5` is the key the clamp rule reads, `fitted5` the key the slice rule reads, they are equal by
construction), `ft_macro_f1`, `zs_macro_f1`, `margin = ft - zs`, `device_used` (read from the `DEVICE ... used=`
trace line), `started_at` / `ended_at` (integer unix seconds), `seconds`.

## SLICE RULE (D-17)

Select **0.25** unless a larger fraction lowers the median holdout ECE at its clamped T (median over the three seeds
of `ece_at.fitted5`) by at least 0.02 relative to the 0.25 median, without costing more than 0.02 of median margin
(median over the three seeds of `margin`, relative to the 0.25 median); among the larger fractions that satisfy both,
the smallest wins. `slice_fraction_selected` is the one number plan 12-08 (only when it is not 0.25) and the gate
contract declaration of plan 12-11 take.

## CLAMP-BINDING RULE (D-18)

At the selected fraction, the T <= 5 clamp **BINDS** if at least 2 of the 3 seeds have an unbounded slice-fitted T
above 5.0, or at least 2 of the 3 seeds have holdout ECE above 0.10 at the clamped T (`ece_at.t5`) while their
holdout-oracle T is above 5.0 with ECE at the oracle (`ece_at.oracle`) at most 0.10. A binding clamp STOPS at a
blocking owner checkpoint (an architectural decision: `calibration_temp_max` lives in `laya-finetune-gate-v1` and
`laya/temperature.rs` and diverges from upstream Laya); nothing in this phase raises the cap.

## The three states

* `probe_failed`: the probe outcome is `oom` or `failed`. No spike cell runs, `12-SPIKE-EVIDENCE.json` is never
  written; the owner checkpoint decides.
* `grid_incomplete`: fewer than nine identity-valid cells (a crashed cell, an OOM, a refused stale cell).
  `12-SPIKE-EVIDENCE.json` records the completed rows and the missing cells with their failing line; neither rule
  is applied to a partial grid and `--aggregate` refuses (`REFUSED spike-grid-incomplete`, exit 3).
* `completed`: nine identity-valid cells on one device; both rules applied.

## Cache identity

A cell's `result.json` carries `identity = {source_task_json_sha256, source_train_jsonl_sha256, holdout_sha256,
spike_train_sha256, epochs, fraction, seed, harness_sha256, training_path_sha256}`. `--grid` reuses a cell only when
the stored identity equals the current one (`--identity`) and refuses a mismatch as `REFUSED spike-cache-identity`;
the existence of a result file is never enough.

## Two honest caveats (declared before any run)

1. **Noise.** The clamp rule's ECE comparisons rest on about 11 conspiratorial holdout rows. Spike 027 saw ECE move
   by 0.048 between MPS replicates of the same recipe and seed, so expect noise: a verdict at one seed is an
   anecdote, which is why the rules count seeds.
2. **Partition.** The holdout is carved by re-applying the eval rule to train, so the spike-train (220 rows) and its
   slices are NOT the gate's fit/slice partition (fit 220 / slice 75 of the full 295 train rows). The spike measures
   the behaviour of the recipe on a thin dataset of this shape, not the gate run.

## How to run

```bash
uv run --project scripts/laya_train --frozen python -I .planning/spikes/032-motivation-slice-and-tcap/run_spike.py --guard-selftest
uv run --project scripts/laya_train --frozen python -I .planning/spikes/032-motivation-slice-and-tcap/run_spike.py --identity --fraction 0.25 --seeds 13 --epochs E
lockf -k -t 3600 /tmp/aprender-laya-real-weights.lock uv run --project scripts/laya_train --frozen python -I \
  .planning/spikes/032-motivation-slice-and-tcap/run_spike.py --grid --epochs E     # nine cells, about 1.5 h of MPS
uv run --project scripts/laya_train --frozen python -I .planning/spikes/032-motivation-slice-and-tcap/run_spike.py --aggregate --epochs E
```

`scripts/laya_train` is imported and not modified.
