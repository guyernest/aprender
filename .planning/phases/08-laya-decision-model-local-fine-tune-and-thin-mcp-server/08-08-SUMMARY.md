---
phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
plan: 08
subsystem: training
tags: [laya, modernbert, fine-tune, calibration, temperature-scaling, early-stopping, ece, gate, uv, torch, mps, tweeteval-stance]

requires:
  - phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
    provides: "08-01 laya-finetune-gate-v1 (constants, recipe, base pin, run_dir_layout, schemas); 08-02 pinned scripts/laya_train uv project, metrics.py, laya_tiny fixture; 08-05 packer (Recipe / GateReport readers, deny_unknown_fields)"
provides:
  - "scripts/laya_train/train.py: the fine-tune -> F16 save -> reload -> calibrate -> zero-shot/eval/probes -> gate pipeline CLI (exit 0 pass / 3 fail / 2 refused), now with --stopping early_stopping|fixed_epochs"
  - "laya-finetune-gate-v1 1.1.0: a second declared stopping rule (early_stopping, calibration-slice NLL at the bounded fitted T, patience 3, restore best), equation early_stopping_train_side, FALSIFY-LAYA-GATE-009"
  - "gate.EarlyStopper / gate.calibration_monitor: the torch-free stopping rule"
  - "aprender-decide Recipe accepts the one optional, strictly typed early_stopping object"
  - "scripts/laya_train/lifecycle.py + just laya-train-lifecycle: both stopping rules on the tiny fixture, LIFECYCLE OK"
  - "models/decide/tweet-stance-16/ (gitignored): the early_stopping demo run dir, gate FAILS (ece_post 0.2224 > 0.10)"
  - "models/decide/tweet-stance-16-fixed-epochs/ (gitignored): the first demo run dir, gate FAILS (ece_post 0.3773), kept as evidence"
affects: [08-09, 08-10, 08-11, 08-12]

actuals:
  tokens: 28480    # chars/4 over the realized code diffs of 6afdc73f1 + ed19f783a + d787f0e81 (113919 chars)
  tasks: 0         # Task 1 (tracer) verify 3 still FAILS (GATE FAIL under both declared recipes); Task 2 not started
  commits: 3       # 08-08 code/contract commits: 6afdc73f1, ed19f783a, d787f0e81 (docs commits excluded)
plan_head_before: 06802c0ad0bbddc7b5d9cf33da7988a0a1f45b5c
continuation_head_before: 469988b14

tech-stack:
  added: []
  patterns:
    - "Every threshold/recipe/stopping value read from the contract through one module (contract.py); nothing downstream holds a literal"
    - "A recipe informed by a failed result is declared and committed as a NEW recipe (new recipe_id) before the re-run is read; the old recipe and its run stay as evidence"
    - "Selection signals (early-stopping monitor) may run on fp32 in memory; every REPORTED score still comes from the F16 reload"
    - "Eval probabilities only after the checkpoint is fixed: assert_checkpoint_fixed before scoring, mtime order recipe.json <= model.safetensors <= eval-probs.json after"

key-files:
  created:
    - scripts/laya_train/contract.py
    - scripts/laya_train/data.py
    - scripts/laya_train/prepare_stance.py
    - scripts/laya_train/train.py
    - scripts/laya_train/gate.py
    - scripts/laya_train/lifecycle.py
  modified:
    - contracts/laya-finetune-gate-v1.yaml
    - contracts/aprender/binding.yaml
    - crates/aprender-decide/src/pack.rs
    - justfile

key-decisions:
  - "HALTED AGAIN at the tracer gate (one declared attempt, per the 2026-09-25 decision): the early_stopping recipe (recipe_id 3d4b91da…) FAILS on ECE: ece_post 0.2224 > max_ece 0.10. The margin passes (0.1056 >= 0.05), and T is 3.144 with no clamp. No threshold, seed, data or recipe was changed after the run was read, and no further recipe was tried."
  - "Both declared recipes fail the same way. fixed_epochs (d0f4e40d…): ece_post 0.3773, T clamped at 5.0. early_stopping (3d4b91da…): ece_post 0.2224, T 3.144. The gate thresholds are unchanged."
  - "Measured cause 1, the servable ceiling. On the early_stopping checkpoint, the eval-ORACLE minimum ECE over Laya's servable T in [0.5, 5] is 0.1126 (at T = 5.0), so no servable temperature passes. ECE 0.0515 at T = 7.5 and 0.0389 at T = 10 are diagnostic only and not usable."
  - "Measured cause 2, the 12-row calibration slice is too small. The monitor's standard error is 0.22 (per-row NLL sd 0.76 / sqrt 12), but epochs 3/4/5 differ by 0.07 (0.861 / 0.787 / 0.853), so the stopping choice is inside the noise. The slice's in-sample accuracy is 0.667 against 0.471 on eval, and it fits T = 3.14 where the eval-oracle NLL optimum is about 7.5. A higher T ceiling alone would not have fixed this run: the 12-row fit still picks 3.14."
  - "The early_stopping recipe is declared in contract 1.1.0 as an additive amendment committed (ed19f783a) BEFORE the run. The fixed_epochs recipe stays declared; its recipe.json bytes, and so recipe_id d0f4e40d…, are reproduced exactly."
  - "The calibration slice is used twice, for stopping and for T, and both uses are train-side. The eval set never reaches the training function, the monitor, the restored epoch or the T fit. This is enforced in code and recorded in the contract."

patterns-established:
  - "Back-office CLIs exit 0 = gate pass, 3 = gate fail, 2 = input refused; refusals print `REFUSED <rule>: ...`"
  - "A run dir is written once: train.py refuses a non-empty --out; superseded runs are renamed, never overwritten"

requirements-completed: []   # plan requirements D-01..D-08, D-19 NOT marked: the plan halted at a failing gate (twice)

coverage:
  - id: D1
    description: "Tiny-fixture lifecycle, both stopping rules: real train -> F16 save -> complete dir -> reload -> calibrate -> gate on CPU, digest mapping proven, RECIPE WRITTEN < EPOCH < STOP < CHECKPOINT COMPLETE < SCORING START, mtime recipe <= checkpoint <= eval-probs; restore-best exact (early_stopping best epoch 1 checkpoint sha cbe2a8d4… == fixed_epochs 1-epoch checkpoint sha)"
    requirement: D-01
    verification:
      - kind: integration
        ref: "just laya-train-lifecycle -> rc 0, LIFECYCLE OK (both rules)"
        status: pass
    human_judgment: false
  - id: D2
    description: "TweetEval stance demo data prepared and verified against the s16-seed13 manifest (48 shots, 280 eval rows); re-run used byte-identical files (task/train/eval sha256 equal the first run's inputs_sha256)"
    requirement: D-19
    verification:
      - kind: other
        ref: "sha256 of data/decide/tweet-stance-16/{task.json,train.jsonl,eval.jsonl} == tweet-stance-16-fixed-epochs gate-report inputs_sha256 (all True)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Early-stopping recipe declared and committed in the contract before the run (pv validate 0 errors, make contract-audit-phase8 rc 0); Rust packer accepts it (aprender-decide 67 lib tests)"
    requirement: D-04
    verification:
      - kind: other
        ref: "pv validate contracts/laya-finetune-gate-v1.yaml -> 0 error(s); make contract-audit-phase8 -> rc 0"
        status: pass
      - kind: unit
        ref: "crates/aprender-decide/src/pack.rs#recipe_early_stopping_is_optional_and_strict"
        status: pass
    human_judgment: false
  - id: D4
    description: "Production demo re-run under early_stopping: recipe first, device read back (mps:0), best epoch 4 restored, complete F16 checkpoint, calibrated reload, schema-conformant gate report"
    requirement: D-07
    verification:
      - kind: other
        ref: "plan verify 3: just laya-train data/decide/tweet-stance-16 models/decide/tweet-stance-16 -> exit 3, GATE FAIL"
        status: fail
    human_judgment: true
    rationale: "The pre-declared gate fails under both declared recipes. How to proceed (a larger calibration slice via a new declared demo cell, a laya-parity change to the served temperature range, or re-scoping D-07/D-18) is a human decision. Moving a threshold after reading the result would void D-07."

duration: 24min   # 14 min first session + 10 min this continuation
completed: 2026-09-26
status: halted
---

# Phase 8 Plan 08: Laya Fine-tune, Calibration and Gate Summary

**The local Laya fine-tune pipeline (`just laya-train`) works end to end, and now supports a second declared stopping rule. The TweetEval stance demo still FAILS the pre-declared gate on calibration under that rule.**

The user chose (2026-09-25) to declare a less-memorising recipe: early stopping on the calibration slice, as a new recipe_id. It was declared in the contract and committed before any run. The demo was then re-run exactly once:

- **The early-stopping run** restored epoch 4 of 12 and fitted T = 3.14 with no clamp. Macro-F1 beats zero-shot by 0.106, which passes. Post-calibration ECE is 0.222, which fails the 0.10 ceiling.
- **The first run** (fixed epochs) had ECE 0.377. It is kept as evidence.

Two causes are measured:

1. No temperature Laya can serve (at most 5.0) brings this checkpoint under 0.10, even when chosen with eval labels: the best is 0.113.
2. A 12-row calibration slice is too small to estimate T or pick an epoch. The monitor's standard error is 0.22, against epoch differences of about 0.07.

The plan halts again for a human decision. Nothing was tuned.

## Performance

- **Duration:** 24 min total (14 min first session, 2026-09-26T01:25:57Z to 01:39:32Z; 10 min this continuation, 2026-09-26T05:30:57Z to 05:41:03Z)
- **Tasks:** Task 1 (tracer) code complete and committed, but verify 3 FAILED (GATE FAIL) under both declared recipes. Task 2 was not started.
- **Local compute (this continuation):** lifecycle about 10 s. Demo 88 s wall, of which training was 41.1 s on mps:0 over 7 of 12 epochs. Diagnostics about 15 s. Far under the 1-hour check-in line.

## The two demo runs (both gate reports kept)

| | fixed_epochs (first run) | early_stopping (this continuation) |
|---|---|---|
| run dir (gitignored) | `models/decide/tweet-stance-16-fixed-epochs/` (renamed, unmodified) | `models/decide/tweet-stance-16/` |
| recipe_id (= `shasum -a 256 recipe.json`) | `d0f4e40d39425e68d503f557f4f660eb9a73a4fb258da01f777b6f49362bcf20` | `3d4b91daf86772bcb23e5342c2dff4bb6467f5d9f254f7833ac7f61a8f2f5375` |
| epochs | 12 of 12 | best 4, run 7 of max 12 (stopped on patience) |
| train steps / seconds | 60 / 65.3 s | 35 of 60 / 41.1 s |
| zero-shot macro-F1 / F_avg / ECE | 0.3402 / 0.3471 / 0.2689 | 0.3402 / 0.3471 / 0.2689 (same base, same rows) |
| fine-tuned macro-F1 | 0.4701 | 0.4458 |
| margin (need >= 0.05) | 0.1299 **pass** | 0.1056 **pass** |
| F_avg (information only; spike 024 was 0.538 +- 0.017 @16) | 0.5151 | 0.4749 |
| ECE pre (served T 1.760) | 0.4327 | 0.3546 |
| **ECE post (need <= 0.10)** | **0.3773 FAIL** | **0.2224 FAIL** |
| NLL | 1.9991 | 1.1448 |
| t_fitted / t_applied / clamp_hit | 5.0 / 5.0 / true | 3.1436 / 3.1436 / false |
| calibration slice | 12 rows `[1, 8, 11, 12, 23, 24, 25, 26, 41, 43, 45, 47]`, sha256 `0640137d…` | identical |
| device / torch / seeds label | mps:0 / 2.14.0 / single seed | mps:0 / 2.14.0 / single seed |
| gate `pass` | false | false |

**Early-stopping trace.** This is the calibration-slice NLL at the bounded fitted T*, from `checkpoint/rl_agent_config.json` `training.stopping`:

| epoch | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|
| monitor | 0.9352 | 0.9834 | 0.8614 | **0.7866** | 0.8529 | 1.0758 | 1.1330 |
| T* | 0.94 | 2.08 | 1.87 | 3.14 | 5.0 | 5.0 | 5.0 |
| training ce (last batch) | 0.726 | 0.753 | 0.261 | 0.0002 | 0.0 | 0.0 | 0.0 |

The best epoch is 4. Patience 3 stopped the run at epoch 7.

## Run log order (acceptance and leakage evidence)

1. `just laya-train-lifecycle` printed `LIFECYCLE OK` with rc 0, for both stopping rules, before the demo.
   - early_stopping (max 3) restored epoch 1.
   - Its checkpoint sha `cbe2a8d4…` equals the 1-epoch fixed_epochs checkpoint sha, so the restore is exact.
2. The demo log order was:
   - `RECIPE WRITTEN 3d4b91da… stopping=early_stopping epochs_max=12` (before any model load)
   - `DEVICE … used=mps:0`
   - `EPOCH` × 7
   - `STOP … best_epoch=4 … reason=patience`
   - `TRAIN`
   - `CHECKPOINT COMPLETE`
   - `SCORING START`
   - `CALIBRATION`
   - `RESULT`
   - `GATE FAIL`, exit 3
3. The mtime order was recipe.json 1790401090, then checkpoint/model.safetensors 1790401134, then eval-probs.json 1790401156. `assert_eval_after_checkpoint` also enforces this in code.

## Why it fails: measured, not re-tuned

These diagnostics come from a scratch script. They read the written files plus the 12 calibration rows on the shipped F16 reload, and they changed nothing: `rl_agent_config.json` has the same sha256 before and after.

- **The ceiling (eval-ORACLE, not usable).** ECE for the early_stopping checkpoint as a function of T:

  | T | 3.14 (applied) | 4 | 5 (Laya max) | 7.5 | 10 | 20 |
  |---|---|---|---|---|---|---|
  | ECE | 0.2224 | 0.160 | 0.1126 | 0.0515 | 0.0389 | 0.0764 |

  - The minimum over the servable range [0.5, 5] is 0.1126 at T = 5.0. It fails, even with the temperature chosen using eval labels.
  - For the fixed_epochs checkpoint, the same minimum is 0.3722.
  - So early stopping cut the required T from about 20–50 down to about 7.5, but not below 5.
- **The 12-row slice.**
  - Per-row calibration NLL at T_applied has sd 0.7625, so the monitor's standard error is 0.2201. The epoch-to-epoch differences that chose epoch 4 are about 0.07, or about 1/3 of one standard error. The stopping signal is not statistically stable.
  - The slice is easier than eval: accuracy 0.667 against 0.471. It fits T = 3.14, while the eval-oracle NLL optimum is about 7.5.
  - Refitting T on the F16 reload reproduces the reported 3.143625 exactly (no clamp).
- **Both causes bind.** Raising Laya's T ceiling alone would not have rescued this run, because the 12-row fit still picks 3.14, giving ECE 0.222. A better T estimate alone would not either, because the best servable T gives 0.113.

## Accomplishments

- **A second declared stopping rule** (contract 1.1.0, additive). The following are all pinned before the run:
  - max epochs;
  - the monitor (calibration-slice NLL at the bounded fitted T) and its direction;
  - evaluation cadence, first candidate epoch, patience 3 and min_delta 0.001;
  - restore-best, with ties going to the earliest epoch;
  - leakage rules, a new equation and FALSIFY-LAYA-GATE-009.
- **train.py implements the rule** with an exact best-epoch restore.
  - The training function is never given eval rows.
  - No eval probability is computed before the checkpoint and the stopping record are on disk.
- **The fixed_epochs recipe still reproduces** recipe_id `d0f4e40d…` byte-for-byte, so the failing first run stays an identifiable fail-closed test vector.
- **The Rust packer reads both recipe shapes.** The one optional `early_stopping` object is strictly typed, and an unknown nested key is refused. 08-09's `from_run_dir` can therefore consume either run dir (both will be refused by its gate check, as designed).
- **Fixtures unchanged.** `just laya-fixtures` is byte-identical: FIXTURES OK, and git shows no fixture change.

## Task Commits

1. **Task 1: Tracer — stance demo end to end** (first session): `6afdc73f1` (feat).
2. **Continuation, early-stopping declaration** (contract only, before any run): `ed19f783a` (feat).
3. **Continuation, early-stopping implementation** (train.py, gate.py, contract.py, lifecycle.py, pack.rs, justfile): `d787f0e81` (feat).
4. **Task 2: Expansion** (`--seeds N`, `data.py` / `gate.py` `--selftest`, `just laya-train-selftest`, `test_harness` lines, README): NOT STARTED. The tracer feedback gate halts expansion on a failing tracer verify, and rule 5 of the resolution says to halt rather than tune.

Plan docs from the first session: `04a4fda8c`, `b60927862`.

## Files Created/Modified

- `contracts/laya-finetune-gate-v1.yaml`: 1.1.0 adds `recipe.stopping_rules` / `stopping_default`, the `early_stopping` block, `demo.stopping` / `demo.superseded_run`, the optional recipe.json `early_stopping` field, equation `early_stopping_train_side`, an independence obligation and FALSIFY-LAYA-GATE-009.
- `contracts/aprender/binding.yaml`: binds `early_stopping_train_side` to `just laya-train-selftest` (pending).
- `crates/aprender-decide/src/pack.rs`: `Recipe.early_stopping: Option<EarlyStoppingDecl>` (`deny_unknown_fields`), plus the test `recipe_early_stopping_is_optional_and_strict`.
- `scripts/laya_train/train.py`: `--stopping`, `calibration_logits`, the stopping loop with best-epoch restore, the `training.stopping` record, `assert_checkpoint_fixed` and `assert_eval_after_checkpoint`.
- `scripts/laya_train/gate.py`: `EarlyStopper` and `calibration_monitor` (torch-free).
- `scripts/laya_train/contract.py`: `early_stopping_decl`, `resolve_stopping`, `stopping_default`, and `recipe_json(..., stopping)`.
- `scripts/laya_train/lifecycle.py`: runs both rules and asserts the STOP ordering, the mtime order and the recipe shape.
- `justfile`: help text for `--stopping` and the two-rule lifecycle.

## Decisions Made

See `key-decisions` in the frontmatter. The load-bearing ones:

- One declared attempt: halt, change nothing after reading, and escalate with the measured causes.
- Declare the new recipe as a new recipe_id rather than editing the old one.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The Rust packer's `Recipe` gained an optional `early_stopping` field**
- **Found during:** continuation, before implementing the new recipe.
- **Issue:** 08-05's `Recipe` is `deny_unknown_fields`, and `unknown_recipe_key_is_refused` proves it. A recipe.json naming its stopping rule would have been unpackable by 08-09. Encoding the rule elsewhere would have kept the recipe_id equal to `d0f4e40d…`, violating the "new recipe_id" requirement.
- **Fix:** `#[serde(default)] early_stopping: Option<EarlyStoppingDecl>`, strictly typed. Absent means fixed_epochs, so every existing run dir and the tiny fixture still parse.
- **Files modified:** `crates/aprender-decide/src/pack.rs`, `contracts/laya-finetune-gate-v1.yaml` (the `recipe_json_schema` entry).
- **Verification:** `cargo test -p aprender-decide --lib` passes 67 tests, including the new one, which was RED first on "no field early_stopping". `cargo clippy -p aprender-decide --all-targets --no-deps -- -D warnings` returns rc 0, and `cargo fmt --check` returns rc 0.
- **Committed in:** `d787f0e81`.

**2. [Rule 2 - Missing critical] Eval-ordering enforced in code, not only by the log**
- **Found during:** continuation, as required by the resolution's leakage rule.
- **Fix:**
  - `assert_checkpoint_fixed` runs before any eval score and requires model.safetensors plus, under early_stopping, the `training.stopping` record.
  - `assert_eval_after_checkpoint` checks the mtime order.
  - The lifecycle asserts both, together with the log order.
- **Committed in:** `d787f0e81`.

**3. [Rule 3 - Blocking] The first run dir was renamed to `models/decide/tweet-stance-16-fixed-epochs/`**
- **Issue:** train.py writes a run dir once. 08-09 expects the demo at `models/decide/tweet-stance-16`.
- **Fix:** the first run dir was renamed with its contents unmodified. Its gate-report.json and recipe_id are cited above.

First-session deviations (unchanged): `lifecycle.py` as a separate file; `--base-sha256` for the synthetic variant; the epoch rule refusing rather than ignoring `--epochs`.

---

**Total deviations:** 3 this continuation (2 blocking, 1 missing critical), plus 3 from the first session.
**Impact on plan:** None on scope. The plan's outcome is the halt below.

## Issues Encountered

**GATE FAIL on the D-19 demo under the second declared recipe.** This is the designed stop. Per the resolution ("one declared attempt only"), no further recipe was tried. Options for the human decision, ranked by what the measurements support:

1. **Declare a new demo cell with a calibration slice big enough to estimate T.**
   - For example, the committed `s64-seed13` selection (64 shots/class). That gives a 48-row slice at the 25 % fraction, with `--epochs` declared in [4, 12] per the epoch rule. Early stopping could stay on.
   - This addresses cause 2 directly: the monitor's standard error shrinks by about 2x.
   - Risk: it changes the data, so D-19 is amended as a new declared cell. The ceiling (cause 1) may still bind, and it is unmeasured at 64 shots.
2. **Change the servable calibration range.** Raise Laya's `TEMP_MAX` in the served path.
   - This diverges from Laya's loader. It needs a laya-parity-v1 amendment and changes the Rust clamp and `calibration_temp_max`.
   - The oracle says T of about 7.5 would reach ECE 0.05 on this checkpoint. But it does not fix cause 2 (the 12-row fit picked 3.14), so on its own it would not have passed this run.
   - Likely needs pairing with option 1.
3. **Re-scope D-07 / D-18 for the demo.**
   - Keep both failing reports as fail-closed test vectors: 08-09's pack and verify must refuse them.
   - Demonstrate the deploy path only after option 1 and/or 2 produce a passing run, or deploy with an explicit "gate not met" status that D-18 refuses.
4. (Not recommended) **Move `gate_max_ece`.** This voids D-07 for the recorded evidence.

Task 2, and the plans that depend on a PASSING demo run (08-09's gate PASS path and the D-18 live deploy), remain blocked until this is decided.

## Known Stubs

None. `--seeds` is not implemented because Task 2 was not started.

## Threat Flags

None. There is no new network endpoint or trust boundary. The stopping record is provenance in the checkpoint config. The eval set is structurally excluded from training and stopping (T-08-08-02 / T-08-08-03 strengthened, not widened).

## User Setup Required

None.

## Next Phase Readiness

- **BLOCKED:** a human decision on the failing demo gate (see Issues Encountered).
- Ready to re-run in about 2 minutes once a new cell or range is declared:
  - `just laya-train-lifecycle` (about 10 s);
  - then `just laya-train <data> <new run dir>`.
- The `s64-seed13` selection exists under `benchmarks/tweeteval-stance/selections/`. `prepare_stance.py` currently reads s16 only.
- Task 2 remains to execute: the `--seeds N` variance report, `data.py` / `gate.py` `--selftest` (drafts exist in the session scratchpad), `just laya-train-selftest`, the contract `test_harness` lines, and the README.

---
*Phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server*
*Completed: 2026-09-26 (halted, second declared recipe)*

## Self-Check: PASSED

- The 6 created scripts and the 4 modified files exist.
- Commits `6afdc73f1`, `ed19f783a` and `d787f0e81` are in history.
- The two gate reports are on disk:
  - `models/decide/tweet-stance-16/gate-report.json`, recipe_id `3d4b91da…`, which equals `shasum -a 256 recipe.json`;
  - `models/decide/tweet-stance-16-fixed-epochs/gate-report.json`, recipe_id `d0f4e40d…`.
- Both are gitignored.
- Every Task 1 acceptance check passes except the gate outcome itself (GATE FAIL, the designed stop).
