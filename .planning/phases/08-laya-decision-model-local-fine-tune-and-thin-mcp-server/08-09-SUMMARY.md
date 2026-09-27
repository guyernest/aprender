---
phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
plan: 09
subsystem: decision-model-verification
tags: [laya, aprender-decide, verify, gate, fail-closed, rescore, parity, ece, macro-f1, pack, halted]

requires:
  - phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
    provides: "08-05 packer (PackInputs::from_run_dir, write_decide_apr, Decider::load_bytes/load_path); 08-08 the two fail-closed demo vectors and laya-finetune-gate-v1 1.2.0 demo block"
provides:
  - "aprender_decide::verify (VerifyPolicy, verify_run, pack_for_serving, verify_path, fixture_bytes, pack_fixture, check_variant, check_base, check_inputs, check_split, validate_probs, rescore, recompute_metrics, check_gate, VerifyError with exit_code)"
  - "aprender_decide::pack::load_checkpoint_for_scoring (back-office zero-shot scorer, never a Decider)"
  - "examples/pack_laya.rs `pack` subcommand and `just laya-pack` (policy read from the contracts at run time)"
  - "MEASURED finding: the Rust port re-scores real Laya checkpoints outside laya-parity-v1 pack_rescore_probs_abs (1e-5) on some of the 280 eval rows"
affects: [08-09 continuation, 08-10, 08-11, 08-12, laya-parity-v1]

actuals:
  tokens: 18324    # chars/4 over the realized diff aad9a7951..d7031c319 (73297 chars)
  tasks: 0         # Task 1 (tracer) code landed but its real-vector verify failed the stop rule; Tasks 2-3 not started
  commits: 1       # MEASURED: git rev-list --count aad9a7951..HEAD before this SUMMARY commit
plan_head_before: aad9a795107b88c1c5375fc1f953ae3a8eb58f5d

tech-stack:
  added: []
  patterns:
    - "The gate is decided on metrics RECOMPUTED in Rust from probability files that were first validated row by row and re-scored in Rust"
    - "pack_for_serving writes only after verification, atomically (temp file in the target dir + rename)"
    - "VerifyError::exit_code(): 3 for GateFailed, 2 for every other refusal (the scripts/laya_train convention)"

key-files:
  created:
    - crates/aprender-decide/src/verify.rs
    - crates/aprender-decide/src/verify/tests.rs
    - crates/aprender-decide/examples/pack_laya.rs
  modified:
    - crates/aprender-decide/src/pack.rs
    - crates/aprender-decide/src/lib.rs
    - crates/aprender-decide/Cargo.toml
    - Cargo.lock
    - justfile

key-decisions:
  - "USER APPROVAL 1 (2026-09-26): the stdio server real-model leg is DEFERRED (D-ITEM-08-09-A); there is no honest real-weights artifact to serve. Not yet written to deferred-items.md: that is Task 3, which did not start."
  - "USER APPROVAL 2 (2026-09-26): adding FALSIFY-LAYA-GATE-010 to laya-finetune-gate-v1, with the version bump pv diff suggests. Not yet applied (Task 3)."
  - "USER APPROVAL 3 (2026-09-26): stop rule. A vector refused for any reason other than ece_post (for example RescoreDrift) means STOP and return a checkpoint. Never widen a tolerance. APPLIED: the early_stopping vector was refused with RescoreDrift on the zero-shot side, so this plan halted at its tracer. pack_rescore_probs_abs is unchanged at 1e-5."
  - "The fixed_epochs vector was NOT run through just laya-pack (that is Task 2's CLI control). Its drift was measured read-only by a throwaway diagnostic: it would be refused at the FINE-TUNED re-score (17 rows over 1e-5, max 5.48e-5), before the zero-shot one."

requirements-completed: []   # Halted: none of D-06, D-07, D-11, D-12, D-17, D-19 is completed by this plan yet

coverage:
  - id: D1
    description: "Rust verifier accept path on the tiny fixture: production-variant copy under a TEST-ONLY permissive policy verifies, both re-scores within 1e-5, argmax 9/9, the recomputed ece_post matches the report"
    requirement: D-07
    verification:
      - kind: unit
        ref: "cargo test -p aprender-decide --lib verify::tests::tiny_verify_roundtrip -> 1 passed"
        status: pass
    human_judgment: false
  - id: D2
    description: "Early_stopping fail-closed vector refused by just laya-pack on the recomputed ece_post clause (the plan's tracer verify 2)"
    requirement: D-19
    verification:
      - kind: integration
        ref: "just laya-pack models/decide/tweet-stance-16 data/decide/tweet-stance-16 <base> models/decide/fail-closed-check.apr -> rc 2, REFUSED RescoreDrift which=zero_shot row=49 max_abs=1.028e-5"
        status: fail
    human_judgment: true
    rationale: "The stop rule fired. The vector IS refused and nothing was written, so fail-closed holds, but the refusal is RescoreDrift, not ece_post. Resolving it needs a user decision (see Next Phase Readiness)."

duration: 21min
completed: 2026-09-27
status: halted
---

# Phase 8 Plan 09: Rust Gate Verifier and Fail-Closed Pack Summary (HALTED at the tracer)

**The Rust verifier and `just laya-pack` exist and refuse the early_stopping demo vector, writing nothing. The refusal is a re-score drift, not the ece_post clause: the Rust port re-scores one of the base's 280 eval rows at 1.028e-5 against the 1e-5 bar. The fixed_epochs checkpoint drifts much further (up to 5.48e-5). The stop rule the user set applies, so the plan halted after its tracer. No tolerance was widened.**

## Outcome: stop rule fired (user approval 3)

`just laya-pack models/decide/tweet-stance-16 data/decide/tweet-stance-16 <base> models/decide/fail-closed-check.apr` (the early_stopping vector, recipe_id `3d4b91da…`):

```
REFUSED RescoreDrift which=zero_shot row=49 max_abs=0.000010281801223754883 (nothing written)
error: Recipe `laya-pack` failed with exit code 2
rc=2 wall=184s
```

- `models/decide/fail-closed-check.apr` does not exist, and the `models/decide` listing is byte-identical before and after (`cmp` of `ls -a`). Nothing was written.
- The fine-tuned re-score, from the PACKED bytes through the full decide-apr-v1 ladder, passed first. Then the zero-shot re-score of the declared base refused row 49.
- The plan expected exit 3 with `clauses=[ece_post]`. That did not happen, so the plan's `<verify>` 2 fails. Per the plan's stop rule and user approval 3, execution stopped. `pack_rescore_probs_abs`, the thresholds and the `demo` block are unchanged.

## Measurements (read-only, throwaway diagnostic, not committed)

A scratch example (`examples/zz_diag_0809.rs`, deleted before commit) used the committed public API only: `PackInputs::from_run_dir`, `write_decide_apr`, `Decider::load_bytes`, `pack::load_checkpoint_for_scoring`, `verify::validate_probs`, `verify::recompute_metrics`. It re-scored all 280 eval rows of both vectors. Wall time 261 s, aarch64 (Apple M4), release build.

| re-score | max \|dp\| | p99 | p50 | rows > 1e-5 | rows > 5e-6 | argmax |
|---|---|---|---|---|---|---|
| early_stopping fine-tuned (packed bytes) | 6.26e-6 | 5.60e-6 | 5.96e-7 | **0** | 5 | 280/280 |
| zero-shot base (T 1.7601519), shared by both vectors | **1.028e-5** | 4.41e-6 | 7.15e-7 | **1** (row 49) | 2 | 280/280 |
| fixed_epochs fine-tuned (packed bytes, T 5.0) | **5.48e-5** | 2.78e-5 | 5.96e-8 | **17** | 29 | 280/280 |

Worst rows:
- zero-shot row 49 (96 tokens): Rust `[0.3628886, 0.33185735, 0.30525407]` vs file `[0.3628963, 0.33184707, 0.30525666]`.
- fixed_epochs row 147 (80 tokens): Rust `[0.013181239, 0.79504085, 0.19177793]` vs file `[0.013177763, 0.7950957, 0.19172655]`.

Facts that bound the cause:
- **The Rust side is deterministic.** Two zero-shot re-scores in one process are bitwise identical.
- **It is not MPS vs CPU.** Training ran on mps:0, but `train.py load_for_scoring` scores every file on CPU in fp32 over the F16 reload (`agent.model.float()`, device asserted `cpu`). Both sides of the comparison are CPU fp32 over the same F16 weights.
- **The two probability files are identical across vectors.** `zero-shot-probs.json` is byte-identical in both run dirs (sha256 `3c55bdd7…`). The fixed_epochs vector would therefore also fail at the zero-shot row 49, but its fine-tuned re-score refuses first.
- **The drift is a tail, not a bias.** The median |dp| is 6e-7 (early_stopping, zero-shot) and 6e-8 (fixed_epochs), but a few rows reach 5.5x the bar. Spike 025's 3.8e-6 was measured on 14 rows. On real, fine-tuned 280-row sets the tail exceeds 1e-5.
- **Converted to logits** (dp ≈ p(1-p) dz / T):
  - zero-shot row 49 is about 7.7e-5, inside laya-parity-v1 `logits_abs` 1e-4;
  - fixed_epochs row 147 is about 1.8e-3, well outside it.
  - So the fixed_epochs drift is not just probability-space amplification.
- **If the re-score bar had held, the gate verdict would be exactly the one the contract predicts.** Metrics recomputed in Rust from the files match the reports within 1e-5:
  - early_stopping: zs macro-F1 0.3402173 (reported 0.34021731), ft 0.4458313 (reported 0.44583129), margin 0.1056140 (pass), ece_post 0.2224078 (reported 0.22240784; FAIL > 0.10);
  - fixed_epochs: ft 0.4701017, margin 0.1298844 (pass), ece_post 0.3773223 (reported 0.37732241; FAIL).
  - Both would be `clauses=[ece_post]` alone. The verdict logic is not what blocks the rule's demonstration; the re-score bar is.

**Consequence beyond the vectors:** a 1e-5 full-eval re-score bar that a 12-epoch real checkpoint misses by 5.5x would also block any FUTURE gate-passing run at `pack`. That makes it a D-17 / D-18 blocker, not only a demo-vector detail.

## Accomplishments (Task 1 code, committed)

- `aprender_decide::verify`. The whole design in the plan, on the current API:
  - variant, base (contract and on disk), input hashes, the `check_split` mirror of `data.py` (NFC, White_Space collapse, groups, conflicting labels, slice ids / size / sha256 / per-class minimum / group integrity), and `validate_probs` (coverage, unique indices, text sha256, K finite values in [0, 1] summing to 1 within 1e-5);
  - `rescore` (NaN-visible, first drifting row + overall max, exact argmax);
  - `recompute_metrics` (aprender-core `f1_score(Average::Macro)` and `expected_calibration_error_top_label`, OPS-03);
  - `check_gate` (thresholds, then reported-vs-recomputed metrics and row counts, then pass agreement; returns the failed clauses);
  - `verify_run`, `pack_for_serving` (packs from the already-read inputs, verifies, then writes atomically), `verify_path` (`Decider::load_path`, manifest bound to the run and data dirs), `fixture_bytes` / `pack_fixture`.
- `pack::load_checkpoint_for_scoring`: base checkpoint dir -> in-memory `.apr` -> `Laya` (never a `Decider`).
- `examples/pack_laya.rs pack`:
  - The policy comes from laya-finetune-gate-v1 and laya-parity-v1 at run time.
  - The only arguments are `--run/--data/--base/--out`, and no environment variable is read.
  - Output is `PACKED …` or one `REFUSED <Variant> … (nothing written)` line, and the exit code is `exit_code()`.
- `just laya-pack`: exec passes the example's exit status through unchanged.

## Task Commits

1. **Task 1 (tracer): verifier + `pack` + `just laya-pack`** - `d7031c319` (feat). Its real-vector verify failed the stop rule (above).
2. **Task 2:** not started.
3. **Task 3:** not started.

## Verification run

| check | result |
|---|---|
| Task 1 verify 1: `cargo test -p aprender-decide --lib verify::tests::tiny_verify_roundtrip` (via `rtk proxy`) | PASS, `test result: ok. 1 passed; 0 failed` |
| Task 1 verify 2: `just laya-pack` on the early_stopping vector | **FAIL (stop rule)**: rc 2, `REFUSED RescoreDrift which=zero_shot row=49`, nothing written |
| `cargo test -p aprender-decide --lib` | 68 passed, 0 failed |
| `cargo clippy -p aprender-decide --all-targets --no-deps -- -D warnings` | rc 0 |
| `cargo fmt -p aprender-decide -- --check` | rc 0 |
| acceptance grep: `expected_calibration_error_top_label` / `Average::Macro` in verify.rs; `pack_rescore_probs_abs` / `model_safetensors_sha256` in pack_laya.rs | all >= 1 |

## Decisions Made

- The three user approvals are recorded in `key-decisions`. Approval 3 (the stop rule) is the one exercised.
- **`VerifyPolicy` gained `calibration_slice_min_per_class`.** `check_split`'s per-class slice minimum needs it, and it is read from laya-finetune-gate-v1 `constants` like every other policy value (not a literal).
- **`PackInputs` now carries `eval_probs_json` / `zero_shot_probs_json` bytes.** The plan's `verify_run(inputs, packed, data, base, policy)` has no run-dir argument, so the probability files travel in the inputs that `from_run_dir` already hash-checks.
- **`check_gate` returns the failed clauses.** The caller builds `GateFailed` with the re-score evidence, so the evidence is never defaulted.
- **`rescore` takes the model's classify closure.** `Decider` and `Laya` share no trait, so it takes `|t| decider.classify(t)` or `|t| base.classify(t)` rather than a model value.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `PackError::ReportHashMismatch` fires before `check_inputs`**
- **Found during:** Task 1 design.
- **Issue:** `PackInputs::from_run_dir` already refuses a data file whose hash differs from the report. So the plan's `InputHashMismatch { file: eval_jsonl }` would surface as a `PackError`.
- **Fix:** `From<PackError> for VerifyError` maps `ReportHashMismatch` to `InputHashMismatch` with the file named (`eval_jsonl`, `eval_probs_json`, …). Other pack errors stay `VerifyError::Pack`.
- **Committed in:** `d7031c319`.

**2. [Rule 2 - Missing critical] The slice check needs the contract's per-class minimum and "leave a fit row"**
- **Fix:** `calibration_slice_min_per_class` in `VerifyPolicy`, read from the contract. `check_slice_classes` also refuses a class whose every row is in the slice (the Python split refuses the same).
- **Committed in:** `d7031c319`.

**3. [Stop rule, user approval 3] Tracer halted**
- **Found during:** Task 1 verify 2.
- **Issue:** refused with RescoreDrift, not ece_post.
- **Action:** stopped. No tolerance widened, no threshold or `demo` value moved, no re-run or re-train of either vector.

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 missing critical) and 1 halt. **Impact:** the halt blocks Tasks 2 and 3 until the user decides how the re-score bar and the Rust port relate on real checkpoints.

## Issues Encountered

- The rtk hook rewrites `cargo test` output ("cargo test: 1 passed"). All recorded test results come from `rtk proxy cargo test`, whose output carries the literal `test result:` line.

## Known Stubs

None. The code is complete for Task 1's scope. Tasks 2-3 (verify / inspect / pack-fixture subcommands, the 22 refusal tests, the contract bindings, the parity and vector tests, D-ITEM-08-09-A) are unstarted, not stubbed.

## User Setup Required

None.

## Next Phase Readiness (DECISION NEEDED)

The user must choose how to treat the Rust-vs-torch re-score tail on real checkpoints. Options, with the measured facts above:

1. **Investigate the port first (recommended).**
   - Run the laya-parity-v1 ladder (embeddings, per-layer relative rms, final norm, head, logits) with a torch oracle dump for the outlier rows: fixed_epochs row 147 (|dp| 5.48e-5, about 1.8e-3 in logits, outside `logits_abs`) and zero-shot row 49.
   - The ladder localizes which op drifts. Candidates: GELU erf precision (RESEARCH A11's erfc_precise swap), attention softmax / accumulation order, LayerNorm, the local window on longer rows.
   - Fix the port so every eval row fits 1e-5. This keeps the D-17 literal, and the rest of 08-09 then runs unchanged.
2. **Re-declare `pack_rescore_probs_abs`.** This is a laya-parity-v1 claims change (`pv diff`-visible) to a bar justified by a measured full-eval tail. It is the "widen a tolerance" the user forbade without a decision, and it changes what D-17 promises.
3. **Accept RescoreDrift as the vectors' refusal.** Amend `demo.fail_closed_rule` to "refused, with the drift or the gate named", and bind FALSIFY-LAYA-GATE-010 to that. Fail-closed still holds. But this leaves the 1e-5 bar unreachable for any real run (fixed_epochs misses it by 5.5x), so D-18 stays blocked until 1 or 2 happens anyway.

Nothing downstream should assume `just laya-pack` can accept a real checkpoint until this is resolved. 08-10's selftest artifact (pack-fixture, Task 2) is unaffected in principle, but Task 2 has not run.

---
*Phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server*
*Halted: 2026-09-27 (tracer stop rule; user approval 3)*

## Self-Check: PASSED

- Created files exist: `crates/aprender-decide/src/verify.rs`, `crates/aprender-decide/src/verify/tests.rs`, `crates/aprender-decide/examples/pack_laya.rs`.
- Commit `d7031c319` is in history. It deletes no tracked file (`git diff --diff-filter=D HEAD~1 HEAD` empty).
- `models/decide/fail-closed-check.apr` is absent. `models/decide/tweet-stance-16*` run dirs were not modified: nothing writes to them, and `ls -a` is unchanged.
- The throwaway diagnostic example is deleted and not in any commit.
