---
status: partial
phase: 03-faithful-two-stage-trainer-and-head
source: [03-VERIFICATION.md, 03-REVIEW.md]
started: 2026-08-12T01:34:04Z
updated: 2026-08-12T01:34:04Z
---

## Current Test

[awaiting human decision]

## Tests

### 1. Authorize (or decline) a dedicated compute budget for the scoped cargo-mutants run
expected: Adjusted mutation score >= 85% after excluding only proven-equivalent re-run survivors
  (plan 03-10 must-have 5). Requires `--timeout >= 120` for aprender-core (its 14285-test binary
  has not finished LINKING at 20s) and cargo-mutants tree-copy mode instead of `--in-place`,
  because 25.3.1 refuses `--in-place` together with `--jobs`.
why_human: Projected ~44.6h single-job. CLAUDE.md places any non-lambda-vector compute spend
  >1hr behind explicit human authorization. The criterion is UNMEASURED, not failed by
  implementation — no score exists to judge, and none was claimed.
evidence_measured:
  - "`error: the argument '--in-place' cannot be used with '--jobs <JOBS>'` — cargo-mutants 25.3.1,
    reproduced independently by the executor, the verifier and the orchestrator."
  - "`--timeout 20` kills the BASELINE: `elapsed=20.050001083s -> outcome=Timeout`, then
    `ERROR cargo test failed in an unmutated tree, so no mutants were tested`."
  - "Mutant inventory re-measured by the orchestrator at 2026-08-12: setfit dir 937,
    multinomial.rs 186, dropout_rng.rs 79 = 1202. The executor's 1181 was taken BEFORE 03-10's
    own refactor commit c04674039; dropout_rng.rs, the one file that refactor did not touch,
    matches at exactly 79 in both measurements."
  - "SCOPING FOOTGUN (found while reconciling): `cargo mutants --list -f <form>` exits 0 with
    EMPTY output when the form does not match, rather than erroring. `-f 'src/train/setfit/**'`
    and `-f '*/setfit/*'` both yield 0 mutants silently; the forms that work are
    `-f '**/setfit/**'` and the full repo-relative path `-f 'crates/aprender-train/src/train/setfit/*.rs'`.
    Any future mutation run MUST assert a non-zero inventory before trusting a score."
result: [pending]

### 2. Authorize (or decline) the `make coverage` run on an uncontended target dir
expected: Line coverage >= COV_FLOOR (88%) with the Phase 3 surface included; plan 03-10
  must-have 6 lists coverage in the closing audit (VALIDATION.md task 3-10-03).
why_human: `cargo llvm-cov` across the workspace with the mold linker disabled is the phase's
  second-heaviest command and needs the same cargo lock the mutation attempts held. Same >1hr
  compute authorization gate. Schedule it in the same window as item 1 — both need an
  uncontended target dir.
result: AUTHORIZED and ATTEMPTED 2026-08-12 — **coverage is NOT MEASURABLE against the floor on
  this host.** Four attempts, ~90 min, three distinct blocking modes:
    1. `make coverage` as written -> rc=2. Two aprender-cgp tests fail on Darwin:
       `test_read_system_memory_total_mb` reads `/proc/meminfo` (absent on macOS) and
       `test_empirical_flops_positive` asserts a FLOPS floor this host misses. CONTROL RUN:
       both also fail UNINSTRUMENTED (0.7 GFLOP/s bare vs 0.1 instrumented), so instrumentation
       is NOT the cause — an earlier hypothesis of mine that the control refuted.
    2. skipping those two -> rc=1 on aprender-compute's `test_gemm_parallel_shared_b_256`, the
       timing flake 03-VERIFICATION.md already characterised (pass/FAIL/pass/pass).
    3. `--no-fail-fast` + two-phase -> HUNG on `h0_mon_90/91_*` (GPU device enumeration,
       `aprender-compute/src/monitor/tests/integration.rs`; the recipe's `--skip gpu_` does not
       match the `h0_mon_` naming).
    4. `--skip h0_mon_` + 30-min wall -> wall tripped at 57 of 58 binaries. The remaining time
       goes to DOZENS of genuine multi-minute training tests (bug_hunter::hunt_*,
       finetune::classify_pipeline convergence, instruct_pipeline LoRA overfit, code_gan,
       transformer_trainer seed reproducibility). Not hangs — real workloads. The recipe's
       "warm: ~3min" comment is off by more than an order of magnitude here.

  BEST DATA (57/58 binaries, disclosed as incomplete): LH=779558 LF=1063825 -> 73%.
  **NOT COMPARABLE to CLAUDE.md's 88.78% (786448/885829) and NOT checked against COV_FLOOR.**
  Covered lines are FLAT (779558 vs 786448, within 0.9%); the whole gap is the DENOMINATOR,
  +177996 coverable lines, 1.20x. Two explanations cannot be separated without a completed run:
  ~178k lines of largely untested new code since July, or scope/exclusion differences plus the
  one unfinished binary. No coverage regression is claimed, and no floor breach is claimed.

  GATE DEFECT FOUND (worth its own ticket, independent of Phase 3): COV_FLOOR compares a
  percentage whose DENOMINATOR IS NOT PINNED. Identical covered-line counts read 73% or 88.78%
  depending on what is in scope, so the floor cannot detect a regression — the same code crosses
  it in either direction purely from the workspace growing. Separately, `make coverage` omits
  `--no-fail-fast`, so cargo stops at the first failing binary and every later crate contributes
  lines-found with zero lines-hit; only the recipe's `|| exit 1` stops it from parsing that
  partial lcov and printing a confident wrong number (measured: 4%, LH=53125/LF=1063825).
  COV_FLOOR was NOT lowered — a floor that moves to meet the measurement is not a floor.

### 3. Decide the feature-closure substitution (plan 03-03 must-have 1)
expected: Either (a) accept `make setfit-feature-matrix` leg (a)'s two-sided diagnostic diff as
  satisfying the must-have, recording an override; or (b) require the D-ITEM-05 fix — gate
  `crates/aprender-train/src/monitor/mod.rs:45`'s unconditional `pub mod tui;` on
  `feature = "tui"` plus its re-exports — so the plain
  `cargo check -p aprender-train --no-default-features --features setfit` can exit 0 as the plan
  literally asked.
why_human: The literal must-have is measured RED (rc=101, 8 errors) and the cause is a
  pre-existing defect in a module outside Phase 3's declared file set. The property Phase 3 owns
  (setfit does not leak into the minimal build) IS verified. Whether to accept the substitution
  or pull in the out-of-scope fix is a scope decision, not a measurement.
result: [pending]

### 4. Decide how Phase 3's test surface gets guarded in tier3 and CI (REVIEW CR-01)
expected: The ~2900 lines of Phase 3 unit tests and all seven trybuild compile-fail cases run in
  at least one tier target AND one CI job. Options: (a) add a `--features setfit` test leg to
  tier3 and to `.github/workflows/ci.yml`; (b) enable `aprender-train/setfit` from a workspace
  member so `cargo test --all` picks it up; (c) accept the gap and record it as debt.
why_human: The fix touches `.github/workflows/*.yml`, which CLAUDE.md reserves for explicit human
  approval. Not an implementation defect — the tests PASS when run (verifier: 837 scoped
  aprender-train tests rc=0; orchestrator: all seven trybuild cases rc=0 under
  `--features setfit`). The defect is that nothing runs them automatically.
evidence_measured:
  - "`setfit` is declared at crates/aprender-train/Cargo.toml:79 but `default = [\"tui\"]` (line 54)."
  - "`train/mod.rs:51` gates the module on `#[cfg(feature = \"setfit\")]`; no workspace member
    enables `aprender-train/setfit`, and resolver 2 does not unify it in."
  - "`make tier3` runs `cargo test --all` and CI runs `cargo nextest run --workspace --lib` —
    both compile the module out."
  - "What DOES run in tier3: `setfit-feature-matrix` (Makefile:338) does `cargo check` — not
    `test` — and the two repro targets (Makefile:336-337) run 2 tests. Never executed anywhere:
    bundle_tests.rs (1045 lines), lock_tests.rs (753), verify_tests.rs (694),
    evaluate_tests.rs (378), and the seven trybuild cases."
result: [pending]

### 5. Decide whether REVIEW CR-02/CR-03/CR-04 are fixed inside Phase 3 or become a Phase 3.1
expected: A decision on scope. These three are self-contained (no CI edit, no cross-phase
  dependency) and can be fixed autonomously on this branch:
  - CR-02: `setfit_repro_recorded_matches_expected_replay` (setfit_repro.rs:513) matches neither
    Makefile filter (`in_process`, `setfit_repro_cross_process`) so the one test separating
    "reproducible" from "correct" never runs; and libtest exits 0 on a zero-match filter, so both
    repro gates go silently vacuous on a rename while printing their success banner.
  - CR-03: `serde_json` renders every non-finite f64 as `null`, so `UpdateEvidence::table_hash`
    is not injective over +inf/-inf/NaN and the bundle fails its own reload; the contract
    precondition at setfit-train-lifecycle-v1.yaml:491-492 has no implementation.
  - CR-04: `thresholds_match_the_contract` checks entry COUNT then a SUBSET test, so a Rust-side
    widening of CALIBRATED_REGIMES keeps it green while admitting uncalibrated runs.
  Also WR-01: the three new Make targets omit `set +e` under `.SHELLFLAGS := -e -c` (Makefile:39),
  so `tail -3`, every `FAIL:` message and `exit $$rc` are unreachable. The gates still fail
  CLOSED (make gets a non-zero status) — the loss is diagnostics, not correctness.
why_human: Scope call only. Fixing inside Phase 3 keeps the phase's evidence honest before it is
  marked complete; deferring to a 3.1 gap-closure phase lets Phase 4 start sooner.
result: RESOLVED 2026-08-12 — human chose "fix all three now". All three landed on
  `gsd/phase-2-contract-gate`, each RED-proven before being trusted:
    - CR-04 `51db85ace` — regime ids compared for EQUALITY. The widening mutation left the OLD
      test at rc=0 and is rc=101 against the new assertion.
    - CR-03 `0158a758d` — `NonFiniteLoss` in `run_batch` before `backward()`, plus a null-scan in
      `to_canonical_bytes`. Defeating either check turns the tests red (rc=101), and the returned
      bytes contain `"relative_delta":null` verbatim.
    - CR-02 + WR-01 `1038f6414` — `setfit-repro-replay` wired into tier3, and `assert_tests_ran`
      refuses a gate that ran nothing (zero-match filter: libtest `ok. 0 passed` → gate rc=2).
      `set +e` makes the FAIL diagnostics reachable.
    - `c992831b2` style commit for rustfmt on the CR-03 helpers.
  Verified after: 234 setfit lib tests pass, all four gates green with asserted counts
  (1/1/1/2), clippy `--no-deps` rc=0, `cargo fmt --check` rc=0.
  Caveats recorded in 03-REVIEW.md: workspace-wide clippy cannot reach this crate (D-ITEM-02
  fails first in aprender-compute), and `bashrs` is absent on this host so the mandated
  Makefile lint did not run.

## Summary

total: 5
passed: 1
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps
