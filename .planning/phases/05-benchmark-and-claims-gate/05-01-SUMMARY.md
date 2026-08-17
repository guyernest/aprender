---
phase: 05-benchmark-and-claims-gate
plan: 01
subsystem: testing
tags: [setfit, minilm, calibration, evidence-gate, contract-gate, compute-budget]

requires:
  - phase: 03-faithful-two-stage-trainer-and-head
    provides: "UpdateEvidence / ParameterClass / rounding_noise_floor and the fixture calibration matrix whose mechanics this harness replicates at production scale"
  - phase: 01-differentiable-minilm-conformance
    provides: "SetFitMiniLm::from_pretrained_dir and the pinned 86.7 MB all-MiniLM-L6-v2 checkout (revision 1110a243), plus the APRENDER_MINILM_DIR env-gate pattern"
provides:
  - "production_calibration_matrix: an #[ignore]d, env-gated in-crate harness that measures the production encoder's per-class relative-delta distributions with probe / prospective / boundary-matrix modes"
  - "Frozen production hyperparameters epochs=1, batch=16, read from the pinned setfit 1.1.3 uv environment with command and stdout recorded"
  - "The production calibration regime id, RENDERED by production code and quoted verbatim: minilm-slice-h384-l6-a12-i1536-v30522@1110a243|seeds=13|cells=s8e1b16"
  - "A measured compute projection for the 18-pass boundary matrix, and the corrected s64/s8 weighting (64x, not the plan's ~8x)"
  - "Proof that debug and release profiles produce BIT-IDENTICAL calibration measurements"
affects: [05-02, 05-03, 05-07, 05-12]

actuals:
  tokens: 12000
  tasks: 1
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Env-gated, #[ignore]d in-crate measurement harness that skips with a named remedy when the fetched checkout is absent"
    - "Mode selection by environment variable (probe / prospective / full matrix) so an expensive matrix can be projected before it is committed to"

key-files:
  created:
    - .planning/phases/05-benchmark-and-claims-gate/05-01-calibration-measurements.md
  modified:
    - crates/aprender-train/src/train/setfit/evidence.rs

key-decisions:
  - "The production regime id is rendered by calibration_regime_id from the run's own coordinates and printed, never composed in the harness (T-05-01-01)"
  - "The production corpus is regenerated at 64 rows per class rather than reusing the 16-row fixture pool, because an s64 selection cannot be drawn from a 16-row pool"
  - "The pair budget is left to the contracted closed form (PairConfig::budget = None) because that is what a production benchmark cell will run"
  - "Halted at the CLAUDE.md 60-minute compute check-in rather than starting an 8.3-hour matrix unilaterally"

patterns-established:
  - "Measure the profile question instead of assuming it: a release cross-check proved debug/release measurement equivalence, converting 'release would be faster' from an assumption into a fact that also licenses release for the real run"
  - "Refute a plan's own projection arithmetic with the measured step count rather than inheriting it"

requirements-completed: []

coverage:
  - id: D1
    description: "production_calibration_matrix harness exists, is #[ignore]d, env-gated on APRENDER_MINILM_DIR, and runs green in probe mode against the pinned production checkout"
    requirement: EVAL-02
    verification:
      - kind: integration
        ref: "CARGO_INCREMENTAL=0 APRENDER_CALIBRATION_PROBE=1 cargo test -p aprender-train --lib --features setfit production_calibration -- --ignored --nocapture"
        status: pass
    human_judgment: false
  - id: D2
    description: "Production epochs/batch frozen from the pinned setfit 1.1.3 environment before any calibration pass, with the literal command and stdout recorded"
    requirement: EVAL-02
    verification:
      - kind: integration
        ref: "cd scripts/setfit_fixtures && uv run python -c \"from setfit import TrainingArguments; a = TrainingArguments(); print(a.num_epochs, a.batch_size, a.body_learning_rate)\""
        status: pass
      - kind: unit
        ref: "production_config asserts reference.epochs()==1 and reference.batch_size()==16"
        status: pass
    human_judgment: false
  - id: D3
    description: "No gate, threshold, tune or contract file modified by this plan"
    requirement: EVAL-02
    verification:
      - kind: unit
        ref: "CARGO_INCREMENTAL=0 cargo test -p aprender-train --lib --features setfit thresholds (27 passed)"
        status: pass
      - kind: other
        ref: "git diff --name-only -> crates/aprender-train/src/train/setfit/evidence.rs only"
        status: pass
    human_judgment: false
  - id: D4
    description: "Compute go/no-go for the 18-pass boundary matrix (8.29 h release / 285.8 h debug, both over the CLAUDE.md 60-minute check-in threshold)"
    verification: []
    human_judgment: true
    rationale: "CLAUDE.md requires a human check-in BEFORE compute spend over 1 hour on non-lambda-vector hosts. The alternatives (run local, move to lambda-vector, trim the cell/seed subset) trade wall-clock against the strength of the D-02 margin argument, which is a judgement the executor cannot make."

duration: 95min
completed: 2026-08-17
status: checkpoint
---

# Phase 5 Plan 01: Production Calibration Measurements Summary

**The production calibration harness is built and green, E/B are frozen from the pinned
environment, and the timed probe turned the boundary matrix from an unpriced assumption into a
measured 8.3-hour job — which trips the compute check-in, so the matrix was not run.**

## Performance

- **Duration:** ~95 min (of which ~35 min was the two timed probe runs)
- **Tasks completed:** 1 of 3
- **Commits:** 2 (`0763656a8` implementation, plus this metadata commit)

## Status: HALTED AT CHECKPOINT (compute gate)

Plan 05-01 Task 1 step (4) is an explicit conditional stop:

> if the projected boundary-matrix wall-clock exceeds 60 minutes, STOP after this task, report
> the projection table as a checkpoint, and wait for the human's go/no-go.

The projection is **8.29 h** (release profile) / **285.8 h** (debug profile). Both exceed the
threshold by more than 8x, so execution stopped. **Tasks 2 and 3 were not started, the boundary
matrix was not run, and no ε has been derived.** Nothing in this plan edits a contract, a
threshold table, `tune.rs`, or any gate.

## Accomplishments

### 1. `production_calibration_matrix` — the harness (Task 1, step 2)

Added to `crates/aprender-train/src/train/setfit/evidence.rs` beside
`calibration_matrix_epsilon_basis`, replicating its mechanics verbatim on the production
encoder: real (2e-5) / control (1e-30) / near-null (1e-8) conditions, per-class real
min/median/max, ctrl max, near-null max + moved flags, `rounding_noise_floor`, support fraction,
binding parameter name, and the per-cell `control_max < real_min` separation assertion.

- `#[ignore]`d, and env-gated on `APRENDER_MINILM_DIR` (default
  `~/.cache/aprender/minilm-l6-v2-1110a243`), printing a SKIP that names the env var and the
  fetch script when the checkout is absent — the `full_weight_parity.rs` pattern.
- Three modes: `APRENDER_CALIBRATION_PROBE=1` (one cell, REAL only, for timing),
  `APRENDER_CALIBRATION_PROSPECTIVE="s16:41,s32:29"` (named cells, REAL only, for Task 3's
  post-freeze validation), and the default full boundary matrix.
- Measurement needs **no gate widening**: the `UncalibratedRegime` refusal lives in
  `validate_evidence` at judgement time, not in `run_tuning` / `from_tune_output`.
- The regime id is produced by calling the production `calibration_regime_id` on the run's own
  encoder / selection / config, then asserted to contain the expected cell label. The harness
  never composes the string (T-05-01-01), and it prints every rendered id verbatim while
  asserting all passes share one architecture component.

### 2. Frozen production hyperparameters (Task 1, step 1)

From the hash-locked `uv` environment, not from documentation:

```text
$ cd scripts/setfit_fixtures && uv run python -c \
    "from setfit import TrainingArguments; a = TrainingArguments(); \
     print(a.num_epochs, a.batch_size, a.body_learning_rate)"
(1, 16) (16, 2) (2e-05, 1e-05)
```

Each tuple is `(body, head)`; the contrastive body member is the one `SetFitTrainConfig`
configures. **epochs = 1, batch = 16**, encoder lr 2e-5. Frozen before any calibration pass, as
the plan's must-have truth requires. `production_config` asserts the in-repo
`REFERENCE_EPOCHS` / `REFERENCE_BATCH_SIZE` agree, so a future divergence between the pinned
Python env and the Rust recipe turns the harness red instead of quietly measuring cells that
production runs never enter.

Consequence: every cell label this milestone can enumerate is `s{shots}e1b16`, so D-02's
`cells=` component is `s8e1b16,s16e1b16,s32e1b16,s64e1b16`.

### 3. The timed probe and the rendered regime id (Task 1, step 3)

Green, `rc=0`, 1811 s wall clock, 24 optimizer steps. Quoted verbatim from `/tmp/probe.log`:

```text
CALIBRATION REGIME: minilm-slice-h384-l6-a12-i1536-v30522@1110a243
minilm-slice-h384-l6-a12-i1536-v30522@1110a243|seeds=13|cells=s8e1b16
```

The `h384-l6-a12-i1536-v30522` dimensions prove the **production** encoder was loaded, not the
97-token slice; the `minilm-slice-` prefix is hardcoded in `architecture_fingerprint` and
renders for the full model too (F-R2, as researched).

### 4. Two findings that change the plan's own arithmetic

**(a) The plan's `~8x` s64 weighting is wrong; it is `64x`.** The plan projected s64 at ~8x s8
because rows scale 8x. But the contracted default pair budget is `2·max(pos_cap, neg_cap)` and
`neg_cap = 3n²` dominates, so the budget — and the step count — is **quadratic** in shots:

| cell | budget | steps @ b16, e1 | vs s8 |
|---|---|---|---|
| `s8e1b16`  | 384    | 24 (measured)   | 1x  |
| `s16e1b16` | 1536   | 96              | 4x  |
| `s32e1b16` | 6144   | 384             | 16x |
| `s64e1b16` | 24576  | 1536            | 64x |

This is the difference between a ~6-hour projection and a ~12-day one, which is why it was
surfaced rather than silently applied.

**(b) Debug and release produce BIT-IDENTICAL measurements.** The release cross-check was run
because "release would fix the compute problem" is an assumption, and CLAUDE.md verification
rule 2 forbids labelling a run by intent. Release is 34.5x faster (2.125 s/step vs 73.3 s/step)
and **every printed relative delta, binding parameter, `delta_norm`, `init_norm`, `grad_norm_max`
and noise floor agrees to the last printed digit**. So the profile is not a degree of freedom in
the calibration: choosing release for the compute budget does not change what is measured. This
matters for 05-03 — and release is arguably the *more* faithful profile, since a user's
`apr setfit train` is a release binary.

### 5. A preliminary observation carried into Task 2 (not a conclusion)

`attention_key_bias`, the class the fixture leaves **ungated** on the gradient-free argument
(`dL/db_k = 0` by softmax shift-invariance), is not bit-frozen at production scale:
`grad_norm_max = 8.084e-10`, `real_min relative_delta = 1.714e-7`, ~150x its own
`rounding_noise_floor` of `1.133e-9`. Whether that is f32 reduction residue or a real gradient
cannot be decided without the 1e-30 and 1e-8 controls — which is exactly what Task 2's boundary
matrix runs. Recorded now so the Task 2 re-derivation answers a question that was already open
rather than one invented after seeing its own result.

## The decision awaiting the human

Full table with costs and evidence trade-offs is in the "COMPUTE GATE" section of
`.planning/phases/05-benchmark-and-claims-gate/05-01-calibration-measurements.md`. In brief:

| # | Option | Cost | Evidence cost |
|---|---|---|---|
| A | Full matrix locally, **release** | 8.29 h | None — the plan as written, at the measured price |
| B | Move to **lambda-vector** (pre-authorized) | 8.29 h scaled by that host | None; also removes the check-in requirement |
| C1 | Trim boundary to `{s8, s16}` | ~38 min (under gate) | Weakest — s32/s64 covered by an unobserved 16x/64x extrapolation |
| C2 | Trim boundary to `{s8, s32}` | ~2.17 h | Moderate; still over the gate |
| C3 | Keep `{s8, s64}`, one seed | ~2.76 h | Loses cross-seed spread, which is what makes the window a window |
| D | Pin a small explicit pair budget | ~38 min | **Flagged, not offered as equal** — the cell label does not record the budget, so cells would be labelled `s64e1b16` while having trained 64x less |

Recommendation (non-binding): **B, else A.** Options C1–C3 buy time by weakening the margin
argument in exactly the dimension the cross-AI review already flagged as this plan's soft spot
(six measured cells as an engineering margin for the other 34).

## Deviations from Plan

### Auto-fixed issues

**1. [Rule 3 - Blocking] `full_weight_parity.rs` is not where the plan says it is**
- **Found during:** Task 1, `read_first`
- **Issue:** The plan cites `crates/aprender-train/tests/full_weight_parity.rs` for the
  `APRENDER_MINILM_DIR` env-gate pattern. No such file exists.
- **Fix:** Read the real one at
  `crates/aprender-core/tests/setfit_conformance/full_weight_parity.rs:54` and replicated its
  resolver exactly. A doc comment on `production_checkout_dir` records the correct path so the
  next reader is not sent to the same dead end.
- **Files modified:** `crates/aprender-train/src/train/setfit/evidence.rs`
- **Commit:** `0763656a8`

**2. [Rule 3 - Blocking] The fixture corpus cannot express an `s64` cell**
- **Found during:** Task 1, harness construction
- **Issue:** `fx::TRAIN_PER_CLASS` is 16 — "exactly the largest shot count the FIXTURE matrix
  asks for". `FewShotSelector` cannot draw 64 rows per class from a 16-row pool, so reusing
  `fx::synthetic_dataset` would have made the s64 boundary cell unrunnable.
- **Fix:** Added a production corpus generator (64 distinct rows per class from an 8x8
  modifier/object grid, with disjoint held-out validation/test material) built through the same
  `from_labeled_rows` ingest ladder. Text stays entirely synthetic (T-3-18); the production
  encoder's full 30522-token vocabulary removes the slice's vocabulary constraint.
- **Files modified:** `crates/aprender-train/src/train/setfit/evidence.rs`
- **Commit:** `0763656a8`

**3. [Scope boundary] `cargo fmt -p aprender-train` reformatted an untouched file**
- **Found during:** Task 1 verification
- **Issue:** `cargo fmt` also rewrote `crates/aprender-train/src/train/setfit/apr_reload.rs`
  (4 insertions, 5 deletions), which this plan does not touch — a pre-existing formatting drift.
- **Fix:** Reverted it (`git checkout -- apr_reload.rs`) rather than absorbing an unrelated
  change into this commit. It is logged below as a deferred item, not fixed here.
- **Commit:** n/a (reverted)

### Additions beyond the plan

**4. Release-profile cross-check probe**
- **Why:** The debug projection (285.8 h) was far enough over the gate that the checkpoint's
  options depended on whether release closes the gap. "Release would fix it" is an assumption;
  CLAUDE.md verification rule 2 says prove the mechanism. The measurement cost ~4 minutes and
  produced two results the checkpoint needs: the real 8.29 h figure, and the debug/release
  bit-identity that licenses running the matrix in release at all.

## Deferred Issues

- `crates/aprender-train/src/train/setfit/apr_reload.rs` is not `cargo fmt`-clean at HEAD
  (4 insertions / 5 deletions). Pre-existing, unrelated to this plan, left untouched.

## Known Stubs

None. The harness is a complete, running measurement; the boundary-matrix and ε-derivation
sections of the measurements file are *absent* rather than stubbed, and the file states
explicitly that they were not run and why.

## Threat Flags

None. This plan installs nothing, reads only the already-materialized pinned checkout through
`SetFitMiniLm::from_pretrained_dir` (which verifies the tokenizer SHA-256 against the pin), and
writes only a test harness and a planning document.

## Verification

| Check | Command | Result |
|---|---|---|
| Probe green (debug) | `CARGO_INCREMENTAL=0 APRENDER_CALIBRATION_PROBE=1 cargo test -p aprender-train --lib --features setfit production_calibration -- --ignored --nocapture` | `rc=0`, 1 passed, 1811 s |
| Probe green (release) | same with `--release` | `rc=0`, 1 passed, 220 s |
| Gate files untouched | `cargo test -p aprender-train --lib --features setfit thresholds` | `rc=0`, 27 passed |
| Diff scope | `git diff --name-only` | `crates/aprender-train/src/train/setfit/evidence.rs` only |
| Formatting | `cargo fmt -p aprender-train -- --check` | `rc=0` |
| Lints | `cargo clippy -p aprender-train --lib --features setfit --all-targets` | `rc=0` |

## Next Steps

1. **Human go/no-go on the compute option** (A / B / C1 / C2 / C3).
2. On approval, resume at **Task 2** — the boundary matrix. Nothing from Task 1 is re-run.
3. Task 3 then derives ε, composes the proposed regime entry from a Task 2 rendered id, writes
   the MEASURED-vs-COVERED table, and runs the two-cell prospective validation.
4. Plan 05-03 carries the deliberate three-place edit to the D-04 checkpoint.

**Note for 05-03:** the `cells=` component of the proposed entry is already determined by the
frozen E/B — `s8e1b16,s16e1b16,s32e1b16,s64e1b16` — and the architecture component is already
observed as `minilm-slice-h384-l6-a12-i1536-v30522@1110a243`. Only the ε table is missing.

## Self-Check: PASSED

- `crates/aprender-train/src/train/setfit/evidence.rs` — FOUND (modified, contains
  `fn production_calibration_matrix`, `#[ignore`, `APRENDER_MINILM_DIR`)
- `.planning/phases/05-benchmark-and-claims-gate/05-01-calibration-measurements.md` — FOUND
- Commit `0763656a8` — FOUND in `git log`
