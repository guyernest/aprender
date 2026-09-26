---
phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
plan: 08
subsystem: training
tags: [laya, modernbert, fine-tune, calibration, temperature-scaling, ece, gate, uv, torch, mps, tweeteval-stance]

requires:
  - phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
    provides: "08-01 laya-finetune-gate-v1 (constants, recipe, base pin, run_dir_layout, schemas); 08-02 pinned scripts/laya_train uv project, metrics.py, laya_tiny fixture"
provides:
  - "scripts/laya_train/train.py: the fine-tune -> F16 save -> reload -> calibrate -> zero-shot/eval/probes -> gate pipeline CLI (exit 0 pass / 3 fail / 2 refused)"
  - "scripts/laya_train/contract.py: every recipe/calibration/gate/seed/base value read from laya-finetune-gate-v1 at run time, plus resolve_epochs and recipe_json"
  - "scripts/laya_train/data.py: D-05 task/row validation, nfc-trim-ws-v1 normalized hash (proven equal to the Rust hash on 48 manifest rows), overlap refusal, text-group calibration split with slice_ids"
  - "scripts/laya_train/gate.py: bounded NLL temperature fit (fit_temperature) and evaluate_gate"
  - "scripts/laya_train/prepare_stance.py + just laya-prepare-stance: data/decide/tweet-stance-16 verified against the s16-seed13 manifest"
  - "scripts/laya_train/lifecycle.py + just laya-train-lifecycle: real tiny-fixture lifecycle in ~5 s, LIFECYCLE OK"
  - "models/decide/tweet-stance-16/ (gitignored): a complete production run dir whose gate report FAILS (ece_post 0.377 > 0.10)"
affects: [08-09, 08-10, 08-11, 08-12]

actuals:
  tokens: 15875    # chars/4 over the realized diff (63501 chars)
  tasks: 0         # Task 1 (tracer) code committed but its verify FAILS (GATE FAIL); Task 2 not started (tracer gate halts expansion)
  commits: 1
plan_head_before: 06802c0ad0bbddc7b5d9cf33da7988a0a1f45b5c

tech-stack:
  added: []
  patterns:
    - "Every threshold/recipe value read from the contract through one module (contract.py); nothing downstream holds a literal"
    - "Unrounded probabilities captured from Laya's own predict path (answer_confidence spy + _forward wrapper), asserted equal to predict's round(p, 4) and to softmax(z / T_applied)"
    - "Checkpoint written COMPLETE, sha256'd, then reloaded; every reload re-hashes and refuses a changed file"
    - "Temperature fit by bisection on dNLL/dbeta (convex in beta = 1/T), bound hit detected from the derivative sign at the interval ends"

key-files:
  created:
    - scripts/laya_train/contract.py
    - scripts/laya_train/data.py
    - scripts/laya_train/prepare_stance.py
    - scripts/laya_train/train.py
    - scripts/laya_train/gate.py
    - scripts/laya_train/lifecycle.py
  modified:
    - justfile

key-decisions:
  - "HALTED at the tracer gate: the D-19 stance demo FAILS the pre-declared gate on ECE (ece_post 0.3773 > max_ece 0.10; margin 0.1299 passes). No threshold, recipe value, seed or data was changed; Task 2 (expansion) was not started"
  - "Root cause measured, not assumed: the 12-epoch recipe memorises the shots (ce_last 0.0); at Laya's maximum served temperature 5.0 half the eval rows still carry confidence > 0.99 at 0.50 accuracy. Temperature scaling would need T ~ 20-50 (eval-oracle diagnostic, not usable) — outside Laya's [0.5, 5.0] loader clamp (RESEARCH Pitfall 6 / assumption A7)"
  - "ece_pre is computed with the checkpoint's pre-calibration served temperature (base bucket T 1.760), not T = 1 as the tiny fixture did — it is the ECE a user would get without calibration"
  - "normalized hash = NFC, trim, collapse over the Unicode White_Space set (Rust split_whitespace), NOT Python str.split (which also splits U+001C-U+001F); proven equal to the manifest's Rust-computed normalized_hash on all 48 shots"
  - "Synthetic-fixture base block: {family laya, repo synthetic, revision local, checkpoint tiny-synthetic, sha256 <tiny model sha>}; --base requires --base-sha256 (the tiny recipe.json's recorded digest)"

patterns-established:
  - "Back-office CLIs exit 0 = gate pass, 3 = gate fail, 2 = input refused; refusals print `REFUSED <rule>: ...`"
  - "A run dir is written once: train.py refuses a non-empty --out"

requirements-completed: []   # plan requirements D-01..D-08, D-19 NOT marked: the plan halted at a failing gate

coverage:
  - id: D1
    description: "Tiny-fixture lifecycle: real train -> F16 save -> complete dir -> reload -> calibrate -> gate on CPU, digest mapping proven (right sha loads, wrong sha refused by Laya's ValueError), CHECKPOINT COMPLETE before SCORING START"
    requirement: D-01
    verification:
      - kind: integration
        ref: "just laya-train-lifecycle > /tmp/p08-08-t1l.log -> rc 0, LIFECYCLE OK, CHECKPOINT COMPLETE line < SCORING START line (plan verify 1)"
        status: pass
    human_judgment: false
  - id: D2
    description: "TweetEval stance demo data prepared and verified against the s16-seed13 manifest (48 shots exact_hash + normalized_hash + label, 280 eval rows)"
    requirement: D-19
    verification:
      - kind: other
        ref: "just laya-prepare-stance -> PREPARE OK; wc -l eval.jsonl 280, train.jsonl 48 (plan verify 2)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Production demo run dir with recipe first, device read back (mps:0), complete F16 checkpoint, calibrated reload, zero-shot/eval/probe files and a schema-conformant gate report"
    requirement: D-07
    verification:
      - kind: other
        ref: "Task 1 acceptance checks AC1, AC3, AC4, AC5, AC7 and recipe.json -ot eval-probs.json: all PASS"
        status: pass
      - kind: other
        ref: "plan verify 3: just laya-train data/decide/tweet-stance-16 models/decide/tweet-stance-16 -> exit 3, GATE FAIL"
        status: fail
    human_judgment: true
    rationale: "The pre-declared gate fails on ECE; how to proceed (recipe change as a new declared recipe, a calibration method Laya can serve, or re-scoping D-07/D-18) is a human decision — moving a threshold after reading the result would void D-07"

duration: 14min
completed: 2026-09-26
status: halted
---

# Phase 8 Plan 08: Laya Fine-tune, Calibration and Gate Summary

**The local Laya fine-tune pipeline (`just laya-train`) works end to end on Laya's own code: recipe first, device read back, a complete F16 checkpoint before any reload, a bounded NLL temperature fit, and zero-shot, eval and probe files from Laya's own predict path. The TweetEval stance demo fails the pre-declared gate on calibration: fine-tuned macro-F1 beats zero-shot by 0.130 (needs 0.05), but post-calibration ECE is 0.377 (needs <= 0.10), because the memorised model would need T ~ 20-50 and Laya serves at most T = 5. The plan halted at the tracer gate without adjusting anything.**

## Performance

- **Duration:** 14 min
- **Started:** 2026-09-26T01:25:57Z
- **Completed:** 2026-09-26T01:39:32Z
- **Tasks:** Task 1 (tracer) implemented and committed, verify 3 FAILED (GATE FAIL); Task 2 not started
- **Files modified:** 7 (6 created, 1 modified)
- **Local compute:** demo total 113.2 s (training 65.3 s on mps:0), lifecycle ~5 s — far under the 1-hour check-in line

## Run log order (acceptance: lifecycle before demo)

1. `just laya-train-lifecycle` -> `LIFECYCLE OK` (exit 0, gate on the tiny model FAIL as expected for 1 epoch, 5 checkpoint files unchanged across reload, `device_used cpu` forced)
2. `just laya-prepare-stance` -> `PREPARE OK` (48 / 280 rows)
3. `just laya-train data/decide/tweet-stance-16 models/decide/tweet-stance-16` -> `RECIPE WRITTEN d0f4e40d...` (line 2) -> `DEVICE ... used=mps:0` (line 6) -> `TRAIN` (line 7) -> `CHECKPOINT COMPLETE` (line 8) -> `SCORING START` (line 9) -> `GATE FAIL`, exit 3

## Demo result (the gate report, verbatim numbers)

| Metric | Zero-shot Laya | Fine-tuned (F16 reload, calibrated) | Gate |
|--------|---------------|--------------------------------------|------|
| macro-F1 | 0.3402 | 0.4701 | margin 0.1299 >= 0.05: **pass** |
| F_avg (against/favor) | 0.3471 | 0.5151 | information only: spike 024 baseline 0.538 +- 0.017 @16 (this run fits on 12 shots/class, A9) |
| ECE (15-bin top-label) | 0.2689 | pre 0.4327, post 0.3773 | ece_post 0.3773 <= 0.10: **FAIL** |
| NLL | — | 1.9991 | — |
| accuracy (diagnostic) | 0.3214 | 0.5036 | — |

- Calibration: bucket `choice:3-5`, t_pre 1.760152, **t_fitted 5.0, t_applied 5.0, clamp_hit true**, slice 12 rows `[1, 8, 11, 12, 23, 24, 25, 26, 41, 43, 45, 47]` (sha256 `0640137d...`)
- device_used `mps:0` (read from the parameters after the move), torch 2.14.0, training 65.3 s, 60 steps, ce_first 0.5802, ce_last 0.0
- recipe_id `d0f4e40d39425e68d503f557f4f660eb9a73a4fb258da01f777b6f49362bcf20` = `shasum -a 256 recipe.json`; recipe `variant: production`, epochs 12, seed 13
- seeds label `single seed`; pass `false`
- Every reported metric recomputes exactly (to the printed digits) from `eval-probs.json` / `zero-shot-probs.json`.

### Why it fails (measured, from the written files, nothing re-run)

- At the applied T = 5.0, 50.4 % of eval rows have top probability > 0.99, while accuracy is 0.50. The model memorised its 36 fit shots (ce_last 0.0).
- An eval-ORACLE diagnostic (logits reconstructed from the T = 5 probabilities, temperature swept, **not usable** because it reads eval) gives ECE 0.377 at T = 5, 0.263 at T = 10, 0.095 at T = 20 and 0.072 at T = 50. Temperature scaling can only reach the ceiling far above Laya's `TEMP_MAX = 5.0`, which the loader applies at every load (RESEARCH Pitfall 6).
- RESEARCH assumption A7 flagged exactly this risk ("unmeasured post-calibration ECE; the demo may fail the gate"). The spike only measured ECE before calibration (0.17-0.38).

## Accomplishments

- `train.py` runs the whole pipeline on Laya's own code, and every value comes from the contract.
- The two sequencing bugs the review found cannot recur unobserved:
  - The digest is passed as a mapping, and a wrong sha is refused by Laya itself.
  - `rl_agent_config.json` exists before the first reload, and the log order is asserted.
- `tokenizer_config.json` is written in Laya's fixed form. Every reload re-hashes the checkpoint and refuses a changed file (T-08-08-06).
- The calibration split is text-group-disjoint, seeded and stratified. `slice_ids` is recorded for the Rust re-check (08-09).
- The normalization matches the Rust `nfc-trim-ws-v1` hash on every manifest row.
- A fail-closed gate report was produced on the model that would actually be served (F16 reload).

## Task Commits

1. **Task 1: Tracer — stance demo end to end** - `6afdc73f1` (feat). Code complete, but verify 3 FAILED on the gate (a designed stop)
2. **Task 2: Expansion — refusals, `--seeds N`, gate self-test** - NOT STARTED. The tracer feedback gate halts expansion on a failing tracer verify.

## Files Created/Modified

- `scripts/laya_train/contract.py` - Contract reader, plus `resolve_epochs` / `recipe_json` / `seeds_label`
- `scripts/laya_train/data.py` - Task/row validation, normalized hash, overlap refusal, group split
- `scripts/laya_train/prepare_stance.py` - Demo data from the s16-seed13 manifest, hash-verified
- `scripts/laya_train/train.py` - The pipeline CLI
- `scripts/laya_train/gate.py` - `fit_temperature`, `evaluate_gate`
- `scripts/laya_train/lifecycle.py` - Tiny-fixture lifecycle assertions
- `justfile` - `laya-prepare-stance`, `laya-train`, `laya-train-lifecycle`

## Decisions Made

See `key-decisions` in the frontmatter. The load-bearing one: halt, adjust nothing, and escalate.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Lifecycle assertions live in `scripts/laya_train/lifecycle.py`, not inline in the justfile**
- **Found during:** Task 1
- **Issue:** The plan says the lifecycle "asserts in Python". About 100 lines of assertions inline in a just recipe cannot be linted or read.
- **Fix:** A separate `lifecycle.py`, called by `just laya-train-lifecycle`.
- **Files modified:** scripts/laya_train/lifecycle.py, justfile
- **Committed in:** 6afdc73f1

**2. [Rule 2 - Missing critical] Synthetic variant takes `--base-sha256` explicitly**
- **Found during:** Task 1
- **Issue:** The plan's lifecycle command line has no way to pass "the tiny checkpoint's recorded sha256" for the digest mapping.
- **Fix:** Added `--base-sha256`, required with `--base`. The lifecycle reads it from the tiny fixture's `recipe.json`.
- **Committed in:** 6afdc73f1

**3. [Rule 2 - Missing critical] Epoch rule and wrong-sha refusal implemented in Task 1**
- **Found during:** Task 1
- **Issue:** The Task 1 action already requires the epoch rule. Ignoring a stray `--epochs` silently would have been wrong.
- **Fix:** `contract.resolve_epochs` refuses rather than ignores.
- **Committed in:** 6afdc73f1

---

**Total deviations:** 3 auto-fixed (1 blocking, 2 missing critical)
**Impact on plan:** None on scope. The plan's outcome is the halt below.

## Issues Encountered

**GATE FAIL on the D-19 demo (designed stop, not a defect).** As the plan instructs, this is surfaced as a checkpoint. The live deploy of D-18 cannot proceed on a failing model, and moving `gate_max_ece` after reading 0.377 would void D-07. Options for the human decision:

1. **Declare a less-memorising recipe as a NEW recipe (new recipe_id), in the contract, before re-running.** For example, the spike's r1 recipe of 4 epochs, or early stopping on the calibration slice. The risk: this is still informed by this result, so record it as a new declared recipe, not a tweak.
2. **Serve a calibration Laya can actually apply.** A single T is capped at 5 by Laya's loader, and the Rust port mirrors that clamp. Raising it means diverging from Laya (a laya-parity-v1 change).
3. **Re-scope D-07 / D-18 for the demo.** Keep the failing report as the fail-closed test vector (deploy refuses it), and demonstrate deploy only after option 1 or 2.

Task 2 and plans that depend on a PASSING demo run (08-09's gate PASS path and the D-18 live deploy) are blocked until this is decided.

## Known Stubs

None. No placeholder code. `--seeds` is not implemented because Task 2 was not started.

## Threat Flags

None. No new network endpoint or trust boundary beyond the plan's threat model. The base download goes through Laya's own `snapshot_download` with the pinned revision and a digest mapping (T-08-08-01).

## User Setup Required

None.

## Next Phase Readiness

- **BLOCKED:** a human decision on the failing demo gate (see Issues Encountered).
- Ready for a re-run: `just laya-train-lifecycle` (~5 s), then `just laya-train data/decide/tweet-stance-16 <new run dir>` (~2 min on this M4).
- Task 2 remains to execute after the decision: `--seeds N` variance report, `data.py` / `gate.py` `--selftest`, `just laya-train-selftest`, contract `test_harness` lines, and the README.

---
*Phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server*
*Completed: 2026-09-26 (halted)*

## Self-Check: PASSED

All 6 created scripts exist; commit 6afdc73f1 is in history; every Task 1 acceptance check passes except the gate outcome itself (GATE FAIL, the designed stop).
