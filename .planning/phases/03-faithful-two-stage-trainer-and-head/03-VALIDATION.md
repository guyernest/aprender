---
phase: 3
slug: faithful-two-stage-trainer-and-head
status: approved
nyquist_compliant: true
wave_0_complete: true
created: 2026-08-09
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Populated from 03-RESEARCH.md "Validation Architecture" + the 26 tasks in plans 03-01…03-09.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (libtest) + proptest 1 + trybuild 1 + cargo-mutants (scoped) — all pre-existing (Phase 2 used each) |
| **Config file** | per-crate `Cargo.toml`; `.clippy.toml` (unwrap ban); `.pmat-gates.toml` |
| **Quick run command** | `cargo test -p aprender-train --lib --features setfit` AND `cargo test -p aprender-core --lib --features setfit` |
| **Full suite command** | `cargo test --workspace --lib --exclude aprender-profile` (Darwin form, STATE.md) |
| **Estimated runtime** | test execution seconds-scale per scoped filter; end-to-end is Rust-compile-bound (~1–3 min scoped, `CARGO_INCREMENTAL=0` per STATE.md ENOSPC mitigation) |

**Command form (contracted — CLAUDE.md Verification Discipline rule 1, shipped broken twice: #2336, #2360):**
Every `<automated>` verify command in every plan runs with DIRECT rc capture, never through a pipe:

```
cmd > /tmp/p3-NNtM.log 2>&1; rc=$?; tail -3 /tmp/p3-NNtM.log; exit $rc
```

Multi-leg commands chain with `[ $rc -eq 0 ] || exit $rc` between legs. The table below shows the
underlying invocations; each executes in this rc-capture form (log names `/tmp/p3-<plan>t<task>[a|b|c].log`
are unique per plan/task so parallel-wave runs cannot collide). Phase 2 lesson 02-06 also applies
verbatim: every contract falsification command carries `--lib` (or a concrete `--test` target) —
bare filter forms pass vacuously.

---

## Sampling Rate

- **After every task commit:** Run that task's scoped `<automated>` command (rc-capture form)
- **After every plan wave:** Run both quick run commands + `cargo check -p aprender-train --no-default-features --features setfit` (feature-closure leg; aprender-core leg for waves touching it)
- **Before `/gsd:verify-work`:** Full suite green + the four Phase 3 tier3 targets standalone with rc recorded: `contract-audit-phase3`, `setfit-repro-crossproc`, `gemm-thread-determinism`, `setfit-feature-matrix`
- **Max feedback latency:** seconds once compiled; no watch modes anywhere (compile time is the floor, accepted for a Rust workspace)

---

## Per-Task Verification Map

*Commands shown are the underlying invocations; all run in the rc-capture form defined above.*

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 3-01-01 | 01 | 1 | TRN-04 | T-3-02 | non-finite inputs keep NumericalError path in both float widths | unit | `cargo test -p aprender-core --lib optim::` + `cargo check -p aprender-core` | ✅ existing optim suite (extended in-task) | ⬜ pending |
| 3-01-02 | 01 | 1 | TRN-04 | T-3-01 | contract drift visible: pv diff vs materialized old revision + version bump | unit (falsify f64 twins) + pv | `cargo test -p aprender-core --lib lbfgs` + `pv validate contracts/lbfgs-kernel-v1.yaml` | ✅ tests_lbfgs_contract.rs (twins authored in-task, tests-first) | ⬜ pending |
| 3-02-01 | 02 | 1 | TRN-06 | T-3-04 | StdRng path removed from SetFit route; feature closure holds | unit | `cargo test -p aprender-core --lib --features setfit setfit::` + `cargo check -p aprender-core --no-default-features` | ✅ Ph1 setfit suite (mode tests must survive) | ⬜ pending |
| 3-02-02 | 02 | 1 | TRN-06 | T-3-06 / T-3-07 | frozen tag `apr-setfit-dropout-v1` collision-free (golden); p>=1.0 typed error | unit + golden (tdd) | `cargo test -p aprender-core --lib --features setfit dropout_rng` | in-task (tests-first) | ⬜ pending |
| 3-02-03 | 02 | 1 | TRN-06 | T-3-05 | thread-count independence MEASURED (3 pool sizes, mechanism-engaged assert) | integration (subprocess) | `cargo test -p aprender-core --test gemm_thread_determinism` | in-task | ⬜ pending |
| 3-03-01 | 03 | 1 | TRN-01 | T-3-09 | states unforgeable: sealed trait, private fields, PhantomData | feature-closure check | `cargo check -p aprender-train --no-default-features --features setfit` + default-features leg | ✅ compiler | ⬜ pending |
| 3-03-02 | 03 | 1 | TRN-02 | T-3-08 | all 12 knobs fail closed with typed knob-naming errors; canonical serde form defined | unit (FALSIFY case tables, tdd) | `cargo test -p aprender-train --lib --features setfit config_` | in-task (tests-first) | ⬜ pending |
| 3-03-03 | 03 | 1 | TRN-02 / TRN-06 | T-3-10 / T-3-11 | epoch order pure fn of (seed, epoch) under own tag; scheduler zero-division guards | unit + independent golden | `cargo test -p aprender-train --lib --features setfit reduce_` / `epoch_` / `--lib warmup_linear` | in-task | ⬜ pending |
| 3-04-01 | 04 | 2 | TRN-04 | T-3-12 / T-3-15 | invalid inputs typed-rejected pre-solve; K=2 and K=3 fit/predict; Duration-free report | unit (tdd) | `cargo test -p aprender-core --lib multinomial` | in-task (tests-first) | ⬜ pending |
| 3-04-02 | 04 | 2 | TRN-04 | T-3-13 / T-3-14 | factor-2 lambda drift caught (wrong-lambda control); fixture provably converged | fixture falsification pair (tdd) | `cargo test -p aprender-core --lib multinomial_contract` | in-task | ⬜ pending |
| 3-04-03 | 04 | 2 | TRN-04 | T-3-13 | blocking contract audit proven falsifiable (induced red recorded) | pv + make gate | `pv validate contracts/multinomial-head-v1.yaml` + `make contract-audit-phase3` | in-task (contract authored here) | ⬜ pending |
| 3-05-01 | 05 | 2 | TRN-03 / TRN-06 | T-3-18 | synthetic-text-only fixture (grep gate: no TweetEval content) | unit | `cargo test -p aprender-train --lib --features setfit fixture_` | in-task | ⬜ pending |
| 3-05-02 | 05 | 2 | TRN-03 / TRN-06 | T-3-19 / T-3-17 | zero-match freeze typed error engaged pre-capture; two-run bitwise trace equality | unit | `cargo test -p aprender-train --lib --features setfit tune_` | in-task | ⬜ pending |
| 3-05-03 | 05 | 2 | TRN-03 | T-3-16 / T-3-17 | evidence canonical + hash-bound (mutation test); Duration-free structs | unit (hash-binding) | `cargo test -p aprender-train --lib --features setfit evidence_` | in-task | ⬜ pending |
| 3-06-01 | 06 | 3 | TRN-03 | T-3-21 | epsilon/k/margin frozen in pv-validated contract BEFORE any judgment | pv | `pv validate contracts/setfit-train-lifecycle-v1.yaml` | in-task (contract authored here) | ⬜ pending |
| 3-06-02 | 06 | 3 | TRN-03 / SAFE-03 | T-3-20 / T-3-22 | gate inside the transition; frozen + 1e-30-LR negatives red in every cargo test | unit (negative/control/mirror, tdd) | `cargo test -p aprender-train --lib --features setfit evidence_gate` + `negative_` | in-task (negatives RED-observed first) | ⬜ pending |
| 3-06-03 | 06 | 3 | SAFE-03 | T-3-20 | FrozenProbeRun never claims SetFit (kind string + no-conversion grep gate) | unit + make gate | `cargo test -p aprender-train --lib --features setfit baseline_` + `make contract-audit-phase3` | in-task | ⬜ pending |
| 3-07-01 | 07 | 4 | TRN-05 | T-3-24 | eval-mode assertion at encode time; encode-once bitwise re-encode equality | unit (tdd) | `cargo test -p aprender-train --lib --features setfit head_input_` | in-task (tests-first) | ⬜ pending |
| 3-07-02 | 07 | 4 | TRN-05 | T-3-25 | lambda resolves against unique rows (exact 1/48 pin), never pair count | unit (tdd) | `cargo test -p aprender-train --lib --features setfit fit_head` | in-task | ⬜ pending |
| 3-07-03 | 07 | 4 | TRN-05 | T-3-23 | pair-multiplicity adversary red in every cargo test; inexpressible via fit_head | in-band negative quartet | `cargo test -p aprender-train --lib --features setfit pair_weight` | in-task | ⬜ pending |
| 3-08-01 | 08 | 5 | TRN-01 | T-3-26 | corrupted/truncated/version-bumped bytes typed-rejected; f32 exactness (subnormal, -0.0) | unit | `cargo test -p aprender-train --lib --features setfit verify_` | in-task | ⬜ pending |
| 3-08-02 | 08 | 5 | TRN-01 | T-3-26 | comparison object built from reload() output only; typed divergence errors | unit (tdd) | `cargo test -p aprender-train --lib --features setfit verify_artifact` | in-task | ⬜ pending |
| 3-08-03 | 08 | 5 | TRN-07 | T-3-27 / T-3-28 / T-3-29 | stale lock invalidates (both hashes named); forged record fails hash; ValidationMetric requires Split<Validation> | unit (tdd) + pv + make gate | `cargo test -p aprender-train --lib --features setfit lock_` + `pv validate contracts/setfit-train-lifecycle-v1.yaml` + `make contract-audit-phase3` | in-task | ⬜ pending |
| 3-09-01 | 09 | 6 | TRN-01 | T-3-31 | five compile-fail proofs; each .stderr names a real type/method (non-vacuous) | trybuild compile-fail | `cargo test -p aprender-train --test ui --features setfit` | in-task | ⬜ pending |
| 3-09-02 | 09 | 6 | TRN-06 | T-3-30 / T-3-32 | cross-process, cross-thread-count hash equality; THREADS-differ mechanism assert; rc captured directly in both Make targets | integration (subprocess) + make gates | `cargo test -p aprender-train --test setfit_repro --features setfit` + `make setfit-repro-crossproc` | in-task (public accessors from 03-08 only) | ⬜ pending |
| 3-09-03 | 09 | 6 | TRN-01…07 + SAFE-03 (closing audit) | T-3-33 | mutation survivors individually justified + re-run (02-08 discipline); honest requirement booking | scoped cargo-mutants + audit commands | `cargo mutants --no-times --timeout 20 --in-place -f <scope>` (3 scopes) + full closing-audit command list in 03-09 T3 | ✅ tooling exists | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*
*"in-task" = the test file/module is authored inside the same task that implements the behavior (TDD tasks write tests first); no task depends on a test file another plan was supposed to scaffold.*

**Sampling continuity:** every one of the 26 tasks carries an `<automated>` verify — there is no
run of even two consecutive tasks without automated feedback.

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. No Wave 0 scaffold plan is needed:

- **Framework present:** libtest (cargo test), proptest 1 and trybuild 1 are workspace deps, cargo-mutants is installed — Phase 2 exercised all of them.
- **Tests authored inline:** every task writes its own tests in the same task (tdd tasks write them first); no plan references a test file that some earlier scaffold was supposed to create, so there are no `MISSING` `<automated>` entries.
- **The one genuinely missing surface** — the `crates/aprender-train/src/train/setfit/` module tree and the aprender-train `setfit` feature — is created by plan 03-03 Task 1 in wave 1, before any command that depends on it runs; wave ordering enforces the dependency.
- RESEARCH's "Wave 0 Gaps" list maps onto scheduled plans, not a Wave 0: module tree + feature → 03-03 T1; f64 L-BFGS widening → 03-01; GEMM falsification harness → 03-02 T3; contracts + `PHASE3_CONTRACTS` + blocking audit target → 03-04 T3 (grown in 03-06/03-08); sklearn pinned fixture → 03-04 T2; trybuild ui cases → 03-09 T1.

---

## Manual-Only Verifications

All phase behaviors have automated verification. (SUMMARY evidence recording — e.g. the epsilon
basis numbers, induced-red observations, mutation table — is documentation of automated runs,
not manual testing.)

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (26/26 have `<automated>`; zero Wave 0 dependencies)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify (density is 26/26)
- [x] Wave 0 covers all MISSING references (none exist — see Wave 0 section)
- [x] No watch-mode flags
- [x] Feedback latency: seconds-scale test execution once compiled; compile-bound end-to-end accepted for Rust (no faster path exists; no watch modes)
- [x] `nyquist_compliant: true` set in frontmatter
- [x] All `<automated>` commands use direct rc capture — no `| tail` pipe forms remain (revision 1 fix; CLAUDE.md Verification Discipline rule 1)

**Approval:** approved 2026-08-09 (revision 1 — populated alongside the rc-capture rewrite so the contract records the corrected command forms)
