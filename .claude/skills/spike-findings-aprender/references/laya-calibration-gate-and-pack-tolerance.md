# Laya Calibration Gate and Pack Tolerance (what the fine-tune gate can certify, and what pack can accept)

Why the D-19 stance demo failed its ECE clause twice, which lever actually moves it, and why the
only gate-passing checkpoint cannot be packed under the 1e-5 re-score bar. Read this before touching
`contracts/laya-finetune-gate-v1.yaml` or `contracts/laya-parity-v1.yaml`, and before reading any
single-seed gate run on MPS as a verdict.

## Requirements

From idea `llm-decision-classifier`:
- **Training may stay in Python; inference must be Rust on aprender.** The served path is Rust.
- **The Python-to-Rust weight handoff is part of the contract**: a checkpoint must export to an
  artifact the Rust path loads, **with probability parity to Python fp32**. (028 measures what
  "parity to fp32" can mean when fp32 itself is noisy.)
- **Laya is evaluated as an alternative base to Kev**: English root and `typed-decisions`
  checkpoints; `choice`/`score`/`noul` types; `/v1/systemone` wire.
- **Laya's calibration gate fails on split shift, not on the cap** (027, 2026-09-26): on TweetEval
  stance the slice-fitted T calibrates in-distribution held-out rows (ECE 0.05–0.07) but not the
  SemEval test split, which wants T 8–30; raising `TEMP_MAX` 5 → 10 rescues 1/3 runs at best.
  **Choosing the demo's eval distribution is a user decision.**

## How to Build It

### 1. Diagnose a calibration failure in this order (027)
1. **Reproduce the failing run read-only** (`sources/027-*/demo_oracle.py`): which checkpoint,
   which T did the slice fit, which T would the eval set want (eval-oracle T reads eval labels; it is
   a diagnostic, never a selection rule).
2. **Run the grid through the harness's own path**, never a reimplementation: `run_cell.py` drives
   `train.SeedRun.train_and_score` (MPS training → complete F16 checkpoint → fp32 CPU reload → bounded
   T fit on the slice → second reload → eval probabilities) with `data.calibration_split` at the
   declared seed. Mark every run dir `NOT-A-GATE-RUN.txt`; nothing here is a declared gate run (D-07).
3. **Separate the three suspects** — slice size, the T cap, the data distribution — with cheap
   diagnostics: prior reweighting (`diagnose_prior.py`), the untouched TweetEval validation split
   (`diagnose_valsplit.py`), and an **in-distribution held-out set built by rule**
   (`diagnose_indist.py`: validation + every train-pool row in no shot, deduplicated by text; 459
   rows here). If the same checkpoints are calibrated on the held-out set at their own slice T, the
   failure is shift, not slice or cap.
4. **Replicate near the bar.** Re-run the declared recipe on the same seeds: MPS replicates moved
   margin by up to 0.074 and ECE by up to 0.048 and flipped one FAIL to a PASS. A single-seed gate run
   on MPS at this margin is a coin flip; report pass rates with a CI, not a verdict.

### 2. What the measurements say about the levers (TweetEval stance, 3 seeds, 24 runs + 4 replicates)
| lever | effect |
|---|---|
| cap 5 → 10 | rescues only fixed-epoch recipes, only at s64, 1/3 of runs; the declared early-stopping rule fits T < 5 in 8/9 runs, so the cap rarely binds |
| slice 12 → 48 rows | halves the spread of the fitted T, lifts margin 0.12 → 0.19–0.22, does not close the gap: the slice measures train-distribution confidence |
| 4 epochs (r1) | soft logits, best margin at s64 (0.221 ± 0.012), wants T ≈ 9: 1/3 pass at T ≤ 10, 0/3 at T ≤ 5 |
| monitor T bound 10 | lets over-fit epochs look good (best epoch 9 instead of 4); keep the monitor bound equal to the served cap |
| **eval set** | the same s64 early-stopping checkpoints: ECE 0.14 ± 0.03 on the SemEval test split, **0.05–0.07 on 459 in-distribution rows**, 3/3 replicates |

The amendment worth bringing to the operator is an **eval-set decision**: keep `gate_max_ece` 0.10,
T ∈ [0.5, 5] and early stopping as declared; move D-19 to `s64-seed13` with `--epochs 12`; declare
the gate's eval set as the in-distribution held-out set by rule; keep the SemEval test split as a
**reported shift probe, not a gate clause**. Expected margin ≈ 0.27 ± 0.04, ECE ≈ 0.057 ± 0.010. The
cost is a claims change: the gate certifies calibration on data drawn like the tenant's shots, not
robustness to a shifted population. (This is what Phase 8 later declared — see the CLAUDE.md
Decision-model row.)

### 3. Set a pack tolerance from the noise floor, not from fixtures (028)
`just laya-pack` re-scores every eval row in Rust and refuses when max |p_rust − p_torch32| > 1e-5
(`pack_rescore_probs_abs`). That bar came from spike 025's worst of 14 base-model rows. On real
fine-tuned checkpoints:
- The only gate-passing checkpoint (s64-es12-seed17-rep) is **refused** at 1.445e-5 (row 189) — where
  **torch fp32 is 6.27e-5 from float64 and Rust is 4.83e-5**: Rust is the more accurate side.
- ECE, macro-F1 and argmax are identical across Rust / torch32 / float64 to 1e-7, 280/280 and 459/459.
  The bar decides whether two fp32 rounding walks landed within 1e-5 of each other, nothing the gate
  can see.
- A larger eval set makes the bar **harder** (max of a heavy tail): 2.98e-5 on 459 rows.

**Recipe to measure a tolerance** (`sources/028-*/`): dump Rust logits from the Rust builder's own ids
(`examples/zz_triad_dump.rs`, copied into `crates/aprender-decide/examples/` only while measuring),
re-score with torch fp32 through the harness's `Scorer` (must be bitwise equal to the stored file —
it was, 280/280), and **the same model in float64** (`torch_triad.py`). Report the triad per row:
rust−torch32, rust−f64, torch32−f64. Only f64 tells "Rust is wrong" from "torch is noisy".

**Proposed bars** (declare in `laya-parity-v1` before the next gate run is read):
- **A (recommended):** `max(1e-5, 4 × max_r |p_torch32(r) − p_f64(r)|)` per checkpoint, the f64 term
  measured by `train.py` at scoring time (+40–70 s per 280 rows) and hash-bound into the gate report.
  Minimum margin on every measured checkpoint: 3.2× (k = 4; Rust's own error is at most 1.6× torch's).
- **B:** fixed 1e-4. Margins 2.1–6.9×; does not scale with conditioning.
- Either way keep **argmax exact** and keep the **1e-5 literal for the tiny and spike-025 fixtures**,
  where it has headroom.

## What to Avoid
- **Reading a single MPS gate run as a verdict** near the bar. Replicate, or report a pass rate.
- **Raising `TEMP_MAX`** as the fix: it changes `laya-parity-v1` and the Rust clamp, diverges from
  upstream (which gives no rationale for 5.0 but enforces it in `agent.py:425`), and rescues ≤ 1/3.
- **Widening the early-stopping monitor's T bound** beyond the served cap: it selects over-fit epochs.
- **Extending `final_norm_abs` / `logits_abs` 1e-4 to real rows** as absolute bars: torch's own fp32
  error exceeds them on 26–90 % of rows (outlier channels 195 and 963, values 4–12 at token rms ≈ 1).
  Make them relative (`per_layer_rel_rms` 1e-3 holds: 1.8e-4 measured) or noise-referenced.
- **Trusting |Rust − torch| as an upper bound on error**: the two fp32 walks share rounding (F16-widened
  weights, the f32 RoPE `inv_freq` buffer); on one row they agreed to 3.7e-6 while both were 3.7e-5
  from f64.
- **Comparing an x86_64 Rust re-score against an aarch64 torch file under 1e-5**: a coin flip on every
  checkpoint. Compare against the **float64 logits committed in `results/triad/*.json`** instead.

## Constraints
- Everything is **aarch64** (M4 Pro: NEON 8×6 BLIS, Apple libm; torch 2.14 CPU). x86_64 is unmeasured;
  the first x86 value belongs in `laya-parity-v1` (`qa_gate`).
- Laya @ `4066d5d`: `TEMP_MIN 0.5`, `TEMP_MAX 5.0` (`laya/common.py:376-377`), loader clamps stored T.
- Calibration split: per class `max(2, ceil(0.25·n))` rows at the declared seed 13 → 12 rows at s16,
  48 at s64. TweetEval stance test = SemEval-2016 test; every model is 0.1–0.2 accuracy worse there
  than on any train-distribution set.
- Grid cost: 24 runs + 4 replicates ≈ 57 min of MPS on the M4 Pro; `just laya-pack` ≈ 3 min each.
- Pack refuses before writing: no `.apr` was produced in 028; exit 2 = `RescoreDrift`, exit 3 =
  `GateFailed`.

## Origin
Synthesized from spikes 027 (PARTIAL) and 028 (INVALIDATED).
Source files: `sources/027-laya-calibration-slice-and-tcap/` (grid driver, diagnostics, RESULTS.md,
results/*.json), `sources/028-laya-packability-noise-floor/` (pack + triad drivers, `zz_triad_dump.rs`,
RESULTS.md, summaries). The run dirs (`models/decide/spike-027/`), `report.html`, the 2.3 MB triad
JSONs with full per-row probabilities and the held-out data stay in `.planning/spikes/027-*/` and
`.planning/spikes/028-*/`.
