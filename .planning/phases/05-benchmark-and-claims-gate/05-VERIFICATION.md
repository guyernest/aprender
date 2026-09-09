---
phase: 05-benchmark-and-claims-gate
verified: 2026-09-08T22:55:00Z
status: gaps_found
score: 3/5 must-haves verified
closing_sha: 4e80e48cca5676e4966db0295e6615a48a7a58ef
branch: gsd/phase-2-contract-gate
covered_files:
  - ".planning/REQUIREMENTS.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-01-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-01-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-01-calibration-measurements.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-02-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-02-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-03-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-03-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-03-epsilon-basis-decision.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-04-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-04-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-05-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-05-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-06-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-06-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-07-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-07-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-08-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-08-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-09-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-09-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-10-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-10-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-11-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-11-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-11-narrowing-inventory.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-12-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-12-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-12-compute-projection.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-13-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-13-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-14-PLAN.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-14-SUMMARY.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-14-controls.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-CONTEXT.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-DISCUSSION-LOG.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-PATTERNS.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-RESEARCH.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-REVIEW.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-REVIEWS.md"
  - ".planning/phases/05-benchmark-and-claims-gate/05-VALIDATION.md"
  - ".planning/phases/05-benchmark-and-claims-gate/deferred-items.md"
  - "Makefile"
  - "benchmarks/tweeteval-stance/report.json"
  - "benchmarks/tweeteval-stance/report.md"
  - "benchmarks/tweeteval-stance/run-manifest.json"
  - "contracts/setfit-benchmark-claims-v1.yaml"
  - "contracts/setfit-train-lifecycle-v1.yaml"
  - "crates/apr-cli/src/commands/finetune.rs"
  - "crates/apr-cli/src/commands/setfit_bench.rs"
  - "crates/aprender-core/src/calibration.rs"
  - "crates/aprender-core/src/stats/hypothesis.rs"
  - "crates/aprender-train/src/train/setfit/bench_gate.rs"
  - "crates/aprender-train/src/train/setfit/bench_gate_tests.rs"
  - "crates/aprender-train/src/train/setfit/bench_metrics.rs"
  - "crates/aprender-train/src/train/setfit/bench_row.rs"
  - "scripts/run_bench_cells.sh"
covered_digest: "v1:sha256:fa24ca2fc6cec93ec29f61b72b05e13d43b7c38d552fa23c209a5b86363eecff"
behavior_unverified: 0
overrides_applied: 0
requirements_disposition:
  EVAL-01: met_in_full
  EVAL-02: met_narrowly       # D-19 amendment; recording verified, BINDING unenforced (see gap 2)
  EVAL-03: met_in_full
  EVAL-04: not_met            # CR-02 reproduced end-to-end through the shipped door
  EVAL-05: met_narrowly       # D-19 amendment; "comparable" reduces to across-shot-level
gaps:
  - truth: "Headline means, dispersion and uncertainty exactly recomputable from stored rows, while any missing, selectively omitted, unmatched or post-test-selected cell invalidates the report (EVAL-04)"
    status: failed
    reason: >-
      The exact-recomputation half is VERIFIED (regenerated report.md is byte-identical to the
      committed one) and the invalidate-on-omission half is VERIFIED behaviourally against three
      escalating attacks, INCLUDING a self-consistent 39-cell manifest with a repaired digest.
      But the provenance half is holed. `verify_provenance` builds
      `bench_dir.join(row.evidence.setfit.lock.lock_record_path)` from a row-supplied string with
      no validation (bench_gate.rs:940 and :980). `Path::join` with an absolute component
      DISCARDS the base, and `..` is never resolved. Reproduced end-to-end through the shipped
      door, not by a compiled probe: with the committed lock file `locks/setfit-s8-seed13.lock.json`
      DELETED and the row pointing at `/tmp/outside/anywhere.json`, `apr setfit bench report`
      exited 0 and printed its own attestation "provenance was recomputed from the committed lock
      bytes rather than read off the rows". That sentence is false on the path I ran. The doc
      comment at bench_gate.rs:601 asserts these paths are relative to the benchmark directory;
      nothing enforces it, and none of the 38 bench_gate tests exercises a path-escaping
      lock_record_path. This is CLAUDE.md Verification Discipline rule 5 — the guard does not scan
      the surface where the decision is made.
    artifacts:
      - path: "crates/aprender-train/src/train/setfit/bench_gate.rs"
        issue: "lines 940, 980 — `bench_dir.join(row_supplied_path)` unvalidated; absolute path replaces the base, `..` unresolved"
      - path: "crates/aprender-train/src/train/setfit/bench_gate_tests.rs"
        issue: "38 tests, none covering a path-escaping lock_record_path or candidate_ledger_path"
    missing:
      - "Reject any lock_record_path / candidate_ledger_path that is absolute, contains a `..` component, or whose canonicalized form escapes bench_dir — as a typed BenchGateError variant, not a panic"
      - "A RED-turning negative test for each: absolute path, `..` traversal, and a symlink out of bench_dir (re-mutated at the ACTIVE 40-cell scope, per the phase's own re-mutation rule)"
      - "Extend the same validation to LEDGER_DIR consumers so the deferred two-method scope inherits the fix rather than re-opening it"
  - truth: "All 40 shot/seed cells runnable for SetFit, each cell recording the selection-manifest hash that would BIND a second method to an identical sampled-ID set (EVAL-02, as amended by D-19)"
    status: partial
    reason: >-
      The recording half is fully verified: 40/40 rows carry a selection_manifest_hash, all 40 are
      distinct, and each equals the `semantic_hash` of the committed selection manifest for its own
      (shots, seed) cell — checked independently against the 40 files on disk, not read from any
      SUMMARY. `apr finetune --task classify --selection-manifest` exists and requires `--seed`, so
      D-10's shared-code-path pairing door is delivered. What is NOT delivered is the BINDING: the
      gate never opens a selection manifest. `apr setfit bench report` returns 0 with the ENTIRE
      `selections/` directory deleted, and returns 0 with a row's selection_manifest_hash doctored
      to 64 zeros (digests repaired). The hash is a free-floating string that nothing recomputes.
      Under D-19 this pairing mechanism is the whole remaining justification for keeping EVAL-02
      open, and the phase goal's word is "bound". The property currently holds in the data by
      construction, not by enforcement — so a future second arm would pair against an unattested
      key. NOTE this is distinct from the deferred cross-method `UnpairedSelection` check: the
      intra-cell row-to-manifest recomputation is a single-method check that is constructible today.
    artifacts:
      - path: "crates/aprender-train/src/train/setfit/bench_gate.rs"
        issue: "verify_provenance recomputes the LOCK but never the SELECTION MANIFEST; selections/ is never read by verify_run"
      - path: "benchmarks/tweeteval-stance/selections/"
        issue: "40 committed manifests are inert — deleting all of them does not change the gate's verdict"
    missing:
      - "In verify_provenance (or a sibling step), resolve selections/s{shots}-seed{seed}/selection-manifest.json, recompute its semantic_hash, and refuse when it disagrees with the row's selection_manifest_hash"
      - "Refuse when the manifest's own payload.shots_per_class / payload.root_seed disagree with the cell key — a transplanted manifest must not satisfy the check"
      - "A RED-turning negative test for each: deleted manifest, doctored row hash, transplanted manifest from another cell"
deferred:
  - truth: "Paired SetFit-versus-LoRA delta, paired-t CIs over per-seed differences, and the two-method 80-cell expectation set"
    addressed_in: "D-ITEM-05-15 (deferred item, not a later milestone phase)"
    evidence: >-
      REQUIREMENTS.md EVAL-02/EVAL-04 amendment 2026-09-07; contracts/setfit-benchmark-claims-v1.yaml
      2.0.0 retains the two-method design verbatim under `expectation_set.deferred_two_method_scope`
      and `status: deferred` clauses. The paired-t machinery (aprender-core stats::hypothesis) is
      retained, contract-bound and fixture-tested; it is unexercised, not removed.
advisory:
  - finding: "Makefile line 29 `.SHELLFLAGS := -o pipefail -c` is dead — overwritten by line 57 `.SHELLFLAGS := -e -c` (CR-01)"
    category: other
    reason: >-
      Reproduced: an `include Makefile` probe target prints `flags=[-e -c]` and `(exit 3) | cat`
      yields `pipeline-rc=0`. But this is NOT phase-5 work. Line 29 was authored by 3181cf9a4
      (2026-08-21, "fix(make): recipes ran without pipefail ... (#2550)"), a non-phase maintenance
      commit; line 57 by b6729b7239 (2026-08-09, Phase 2). `git diff <first-05-commit>^..HEAD -- Makefile`
      shows no phase-5 plan touched either line, and no phase-5 SUMMARY claims the hardening — the
      only phase-5 "pipefail" is `scripts/run_bench_cells.sh`'s own `set -euo pipefail`. Impact on
      THIS phase's goal is nil: every phase-5 Make floor (setfit-bench-gate/-row/-metrics) uses
      `> log 2>&1; rc=$$?` redirection, never a pipe, so it reads cargo's real status. Recorded as
      advisory rather than a phase gap; #2550 remains unfixed and should be re-opened on its own.
    evidence_status: "reproduced; scope is pre-existing, off the phase-goal path"
  - finding: "A fully self-consistent forgery of a quality metric passes the gate"
    category: other
    reason: >-
      Doctoring `quality.f_avg` to 0.99 and repairing the row envelope digest and the manifest's
      row_sha256 + envelope digest yields rc=0 and a published mean of 0.5278. The report DISCLOSES
      exactly this ("a producer holding both the rows and those files could still emit a mutually
      consistent forgery. This report proves consistency, not truth"), so it is within the stated
      threat model and is not a gap. It is nonetheless a cheap hardening miss: the row already
      carries `confusion_matrix` and `ordered_labels`, so f_avg / macro_f1 / mcc / per-class metrics
      are recomputable IN CLOSED FORM from bytes already present, and cross-checking them would have
      refused this exact doctoring without any new evidence file.
    evidence_status: "reproduced; disclosed residual, offered as hardening"
  - finding: "The production calibration regime is PROVISIONAL — 4 of 40 cells measured, epsilons not frozen under the 2.x derivation rule"
    category: other
    reason: >-
      contracts/setfit-train-lifecycle-v1.yaml states this in-band and prominently (lines 21-26,
      315-331): five of six parameter classes have no legal epsilon under the 2.x rule, so the
      production regime's lower bound is the f32 rounding-noise clearance at a factor the edit
      CHOOSES rather than cites; the two prospective-validation cells (s16 seed 41, s32 seed 29)
      were NOT run. Raised as advisory only because the disclosure is exemplary and the empirical
      per-run invariant DID hold on all 40 cells (the driver halts on first evidence failure, and
      40/40 rows carry distinct verified evidence_table_hashes).
    evidence_status: "self-disclosed in contract; no phase-goal impact observed"
---

# Phase 5: Benchmark and Claims Gate — Verification Report

**Phase Goal (AMENDED 2026-09-07, D-19):** Users can audit and recompute a complete,
selection-safe TweetEval evidence set for the verified SetFit APR across every contracted shot and
seed, with each cell bound to a recorded selection manifest so a second method can later be paired
against it without re-running this half.

**Verified:** 2026-09-08 · **Closing SHA:** `4e80e48cc` · **Status:** gaps_found
**Re-verification:** No — initial verification.

## Binary pin (CLAUDE.md Verification Discipline rule 3)

`scripts/apr_bin.sh` REFUSED: `target/release/apr` reports `7c3a6b401`, HEAD is `4e80e48cc`. I did
not run a stale binary blind, and I did not hardcode a path on faith. I proved the gap is empty:

```
git diff --stat 7c3a6b401..HEAD -- crates/ contracts/ Makefile scripts/   # → no output
git log --oneline 7c3a6b401..HEAD   # → 3 commits, all docs(05): *
```

The three intervening commits are documentation only. The binary is therefore code-identical to
HEAD for every surface under test, and every `apr` invocation below is
`/Users/guy/Development/machine-learning/aprender/target/release/apr`. `make test` was not used —
it cannot compile on macOS (`crates/aprender-profile/examples/validate_golden_trace.rs` imports a
linux-gated module), per the orchestrator; targeted `cargo nextest` invocations were used instead.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Ordered predictions evaluable with official `F_avg`, per-class metrics, three-class macro-F1, MCC, confusion matrix, and validation-only calibration diagnostics bound to explicit ordered labels (EVAL-01) | ✓ VERIFIED | `bench_metrics.rs:193 assemble_quality_block` emits all of them; `check_evidence` (`:284`) REFUSES when observed `ordered_labels` disagree with the caller's declaration, and separately gates `TEST_SPLIT` vs `CALIBRATION_SPLIT`. All 40 rows carry `per_class_precision/recall/f1`, `mcc`, `confusion_matrix` (3×3), `ordered_labels: ["none","against","favor"]`, `ece_top_label_validation`, `brier_multiclass_validation`, `calibration_split: "validation"` (40/40). Multiclass calibration is real code, not a stub: `calibration.rs:285 expected_calibration_error_top_label`, `:341 brier_score_multiclass`. Behavioural: 41/41 `bench_metrics`+`bench_row` tests PASS; 95/95 `aprender-core` calibration + claims-stats tests PASS against committed scipy reference fixtures (`scripts/setfit_fixtures/claims_stats/`). |
| 2 | All 40 shot/seed cells runnable for SetFit, each cell recording the selection-manifest hash that would **bind** a second method to an identical sampled-ID set (EVAL-02, amended) | ⚠️ PARTIAL | RECORDING verified: 40 rows on disk, 40 distinct `selection_manifest_hash` values, and each equals the `semantic_hash` of the committed manifest for its own `(shots_per_class, root_seed)` — checked independently against the 40 files, not from a SUMMARY. `apr finetune --selection-manifest` shipped (requires `--seed`). BINDING **not** enforced: `bench report` returns 0 with `selections/` entirely deleted, and returns 0 with a row's hash doctored to 64 zeros. See gap 2. |
| 3 | One machine-readable row per method/shot/seed run carrying dataset/model revisions, selection lock, artifact hash, encoder-update evidence, backend/hardware identity, quality metrics, and consistently bounded resource measurements (EVAL-03) | ✓ VERIFIED | 40 rows, every field present in 40/40: `dataset_revision`, `dataset_fingerprint`, `model_revision`, `selection_manifest_hash`, `backend_identity` (`cpu:setfit-core:autograd-trueno-matmul`), `host{hostname,os,arch}`, full quality block, full resource block, `evidence.setfit{evidence_table_hash, apr_artifact_sha256, lock{lock_hash, role, rule, lock_record_path}}`. `evidence_table_hash` 40/40 distinct; `apr_artifact_sha256` 40/40 distinct. Resource measurements are BOUNDED per figure and per mechanism: inference is uniform `child_max_rss_time_l [exact_kernel_high_water_mark]` 40/40 with `cold_measured_in_child_process: true` 40/40; training is `sysinfo_sampled_{19×30, 18×8, 17×2}`, all one CLASS (`sampled_lower_bound`), and the report prints the class, the interval AND a `LOWER BOUND (sampled; can only understate)` rider on every such figure, then explicitly refuses to add or average the two. |
| 4 | Headline means, dispersion and uncertainty exactly recomputable from stored rows, while any missing, selectively omitted, unmatched or post-test-selected cell invalidates the report (EVAL-04) | ✗ FAILED | Recomputation ✓ (byte-identical regeneration). Fail-closed ✓ against three escalating attacks including a self-consistent 39-cell manifest. **Provenance ✗** — CR-02 reproduced through the shipped door: the gate returned 0, and printed its "recomputed from the committed lock bytes" attestation, with that lock file deleted. See gap 1. |
| 5 | Training time, cold/warm latency, throughput with batch/warmup boundaries, peak memory, artifact size, calibration and classification quality comparable, measured from the same reloaded production artifacts (EVAL-05) | ✓ VERIFIED (narrow) | Every named figure exists in 40/40 rows with its boundary: `train_wall_ms`, `cold_latency_ms` + `cold_measured_in_child_process`, `warm_latency_ms_median` (median of 10 after 3), `throughput_rows_per_sec` + `throughput_batch_size: 32` + `warmup_count: 3`, `train_peak_rss_bytes`/`inference_peak_rss_bytes` each with its own mechanism, `artifact_bytes` AND `deployable_total_bytes` (the report states these answer different questions and that a size claim uses only the latter). Measurement is from the reloaded artifact, not the in-memory trainer: `setfit_bench.rs:1237 reload_verified_run_from_apr`, doc at `:1215` "train, write, RELOAD, measure, lock, evaluate, emit", and `:382` records that a same-process peak would report the TRAINING peak — which is why cold latency is spawned in a fresh child. **NARROW under D-19:** with one method measured, "comparable" reduces to comparability across shot levels; there is no second method to compare against, and the report says so in its own SCOPE block. |

**Score:** 3/5 truths verified (0 present-but-behaviour-unverified). One PARTIAL, one FAILED.

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|------|-------------|----------|
| 1 | Paired SetFit-versus-LoRA delta, paired-t CIs over per-seed differences, and the 80-cell two-method expectation set | D-ITEM-05-15 | REQUIREMENTS.md EVAL-02/EVAL-04 amendment; contract 2.0.0 retains the two-method design verbatim under `deferred_two_method_scope` + `status: deferred` clauses; paired-t machinery retained, contract-bound, fixture-tested, unexercised |

### Advisory (Reproduced, Off the Phase-Goal Path)

| # | Finding | Category | Why Advisory |
|---|---------|----------|--------------|
| 1 | CR-01: `.SHELLFLAGS` pipefail hardening is dead | other | Reproduced (`flags=[-e -c]`, `pipeline-rc=0`), but authored outside phase 5 (line 29 by #2550 on 2026-08-21, line 57 by Phase 2 on 2026-08-09); no phase-5 commit touches either line, no phase-5 SUMMARY claims it, and every phase-5 Make floor uses redirection rather than a pipe |
| 2 | A fully self-consistent metric forgery passes | other | Explicitly disclosed as the report's stated residual; offered as a cheap hardening (recompute `f_avg` from the row's own `confusion_matrix`) |
| 3 | Production calibration regime is PROVISIONAL (4/40 cells measured) | other | Disclosed in-band and in detail in the contract; the empirical per-run invariant held on all 40 cells |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `benchmarks/tweeteval-stance/rows/*.json` | 40 machine-readable rows | ✓ VERIFIED | 40 tracked; all schema fields present; `row_sha256` in the manifest equals each row's own `semantic_hash`, 40/40 distinct |
| `benchmarks/tweeteval-stance/locks/*.lock.json` | 40 selection locks | ✓ VERIFIED | 40 tracked; hashes recomputed by the gate — but see gap 1 for the path it recomputes them at |
| `benchmarks/tweeteval-stance/selections/*/selection-manifest.json` | 40 selection manifests | ⚠️ ORPHANED | 40 tracked and internally correct, but the gate never reads them (gap 2) |
| `benchmarks/tweeteval-stance/run-manifest.json` | 40/40 complete, digest-sealed | ✓ VERIFIED | Envelope digest recomputes; completeness is checked against the CONTRACT-derived set, not the manifest's own list |
| `benchmarks/tweeteval-stance/report.md` | Recomputable claims report | ✓ VERIFIED | Regenerated output is byte-identical (`cmp` clean, md5 `0abdacc72880081af08e4dba3759e1b6`) |
| `contracts/setfit-benchmark-claims-v1.yaml` | 2.0.0, active 40-cell scope, deferred scope retained | ✓ VERIFIED | `pv validate`: 0 errors, 0 warnings. `expectation_set` yields `1 × 4 × 10 = 40`; the narrowing was done by REMOVING an element from `methods` so the arithmetic follows rather than being retyped; deferred scope preserved verbatim |
| `contracts/setfit-train-lifecycle-v1.yaml` | Additive production regime entry (F-10 unblock) | ✓ VERIFIED | Production regime present, covering all 10 seeds × 4 cell labels; MEASURED-vs-COVERED stated in-band; see advisory 3 |
| `crates/aprender-train/src/train/setfit/bench_gate.rs` | The EVAL-04 claims gate | ⚠️ HOLED | Substantive and wired, but the provenance path is row-controlled (gap 1) |
| `crates/apr-cli/src/commands/setfit_bench.rs` | `apr setfit bench run/report/verify-cell` | ✓ VERIFIED | All three subcommands present and exercised below |
| `scripts/run_bench_cells.sh` | Sweep driver, one process per cell | ✓ VERIFIED | Present, executable, `set -euo pipefail` + `. scripts/apr_bin.sh \|\| exit 1` |
| `crates/apr-cli/src/commands/finetune.rs` | `--selection-manifest` pairing door (D-10) | ✓ VERIFIED | Flag present, documented as the EVAL-02 pairing key, requires `--seed` |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| row `selection_manifest_hash` | `selections/*/selection-manifest.json` | recomputation in `verify_run` | ✗ NOT WIRED | Gate returns 0 with `selections/` deleted; the link exists only in the data |
| row `lock.lock_record_path` | `locks/*.lock.json` | `verify_provenance` sha256 | ⚠️ PARTIAL | Recomputation happens, but at an attacker-chosen path (gap 1) |
| `bench run` | reloaded `.apr` artifact | `reload_verified_run_from_apr` | ✓ WIRED | `setfit_bench.rs:1237`; cold latency spawned as a fresh child under `/usr/bin/time` |
| `run-manifest.json` | contract expectation set | `EXPECTED_CELLS` / `expectation_set` | ✓ WIRED | Proven behaviourally: a repaired 39-cell manifest is refused against the contract-derived 40 |
| `bench report` | `stats::hypothesis` (t-critical, dispersion) | frozen `t = 2.262157162798205`, df = 9 | ✓ WIRED | 95/95 stats tests pass on scipy fixtures; no RNG in the claims path (D-06) |
| bench_gate tests | CI | `cargo test -p aprender-train --features setfit --lib setfit::` | ✓ WIRED | Confirmed not dark: the CI filter enumerates 415 tests, 38 of them `bench_gate` (`--features setfit` is required; `setfit` is NOT a default feature, and `.github/workflows/ci.yml:305` exists precisely for that) |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `report.md` QUALITY table | `f_avg` mean/std/CI | 40 row files → `aggregate` → frozen-t CI | ✓ (recomputes byte-identically; values are non-degenerate and monotone in shots: 0.4746 → 0.5115 → 0.5346 → 0.5607) | ✓ FLOWING |
| `report.md` RESOURCE block | `train_wall_ms`, latencies, RSS | measured per cell, reloaded artifact | ✓ (train wall scales 31s → 113s → 443s → 1739s across s8→s64, i.e. real compute, not constants) | ✓ FLOWING |
| row `quality.*` | confusion matrix, per-class, ECE, Brier | `assemble_quality_block` over test + validation splits | ✓ (`n_test_rows: 280`; confusion matrices differ per cell) | ✓ FLOWING |
| row `evidence.setfit.*` | evidence table + artifact hashes | training run | ✓ (40/40 distinct on both) | ✓ FLOWING |
| gate verdict | `selection_manifest_hash` | nothing — never recomputed | ✗ | ✗ DISCONNECTED |

### Behavioural Spot-Checks

All run against the pinned binary at the closing SHA. Scratch copies excluded `artifacts/` (40 × ~90 MB) and `logs/`; the gate reads neither — verified by a clean rc=0 baseline on the slim copy.

| # | Behaviour | Command | Result | Status |
|---|-----------|---------|--------|--------|
| 0 | Baseline: committed evidence verifies | `apr setfit bench report --bench-dir benchmarks/tweeteval-stance` | rc=0, full report | ✓ PASS |
| 0b | Report is EXACTLY recomputable | regenerate, `cmp` against committed `report.md` | byte-identical, md5 `0abdacc7…` | ✓ PASS |
| A | Missing cell invalidates | delete `rows/setfit-s32-seed41.json` | rc=5, names the cell: "a recorded digest with no bytes behind it is an omission the manifest cannot see" | ✓ PASS |
| B | Manifest tamper invalidates | drop the cell's manifest entry too | rc=5, envelope digest mismatch, "these are not the bytes that were attested" | ✓ PASS |
| C | **Self-consistent shrunk matrix invalidates** | drop entry AND repair the manifest's own digest → a clean 39-cell manifest | rc=5: "the run manifest declares 39 cells … but the contract derives 40 … Completeness is defined by the contract-derived set, never by whatever the manifest happens to list" | ✓ PASS (the strongest EVAL-04 result in this report) |
| D | Doctored metric, all digests repaired | `f_avg` 0.4579 → 0.99 | rc=0; published mean moves 0.4746 → 0.5278 | ⚠️ disclosed residual (advisory 2) |
| E | **Provenance path escape** | `lock_record_path` → `/tmp/outside/anywhere.json`, in-tree lock DELETED, digests repaired | **rc=0**, and the report printed "provenance was recomputed from the committed lock bytes" | ✗ **FAIL — gap 1** |
| F | Doctored selection hash | `selection_manifest_hash` → 64 zeros, digests repaired | rc=0 | ✗ FAIL — gap 2 |
| G | All 40 selection manifests deleted | `rm -rf selections/`, rows untouched | rc=0 | ✗ FAIL — gap 2 |
| H | Single-cell diagnostic door | `bench verify-cell --method setfit --shots 8 --seed 13` | rc=0, and correctly states its own scope is steps 1+4+6 and that "`bench report` is the only door that publishes numbers" | ✓ PASS |
| I | Contract is valid | `pv validate contracts/setfit-benchmark-claims-v1.yaml` | 0 errors, 0 warnings | ✓ PASS |
| J | Claims gate suite green | `cargo nextest run -p aprender-train --lib --features setfit -E 'test(/bench_gate/)'` | 38 run, 38 passed | ✓ PASS |
| K | Row + metric suites green | `… -E 'test(/bench_metrics\|bench_row/)'` | 41 run, 41 passed | ✓ PASS |
| L | Calibration + claims-stats green | `cargo nextest run -p aprender-core --lib -E 'test(/calibration\|claims_stats/)'` | 95 run, 95 passed | ✓ PASS |
| M | CR-01 pipefail probe | `include Makefile` + `(exit 3) \| cat` | `flags=[-e -c]`, `pipeline-rc=0` | ✗ reproduced (advisory 1) |

Independent data checks (Python over the committed tree, not over any SUMMARY):

- 40 rows, 40 locks, 40 selection manifests, 1 run-manifest — 123 tracked files under `benchmarks/tweeteval-stance/`; no `.apr`, log or artifact in the index.
- Every row's `selection_manifest_hash` equals the `semantic_hash` of the committed manifest whose `payload.shots_per_class`/`payload.root_seed` match its cell: **40/40, 0 mismatches**, 40 distinct.
- `evidence_table_hash` 40/40 distinct; `apr_artifact_sha256` 40/40 distinct.
- Envelope digest scheme reverse-engineered and confirmed as `sha256(compact serde-order JSON of payload)` — which is what made attacks B/C/D/E/F possible at all, and is itself evidence the scheme is deterministic and reproducible by a third party.

### Probe Execution

No `scripts/*/tests/probe-*.sh` exist for this phase, and no PLAN declares one. The equivalent
runnable gate is `make setfit-bench-tests` (Makefile:2657-2674 and neighbours), whose three legs
were executed directly as spot-checks J and K rather than through `make` (the Makefile's own
`assert_tests_ran` floors are 27 / 38 / n; the observed counts are 41 combined for row+metrics and
38 for the gate, meeting the gate floor exactly). Status: **PASS**, with the caveat that these
floors run under `--features setfit`, which is not a default feature — confirmed reachable in CI.

### Requirements Coverage

Every phase-5 plan declares `requirements:` frontmatter and **no plan ran `requirements.mark-complete`**;
all 14 executors deliberately left `requirements-completed` empty on the grounds that flipping
requirement state is the verifier's act. That is correct process, and the dispositions below are
this verifier's act. All five IDs are accounted for; there are **no orphaned requirements** —
REQUIREMENTS.md maps exactly EVAL-01..05 to Phase 5 and all five appear across the plans.

| Requirement | Source Plans | Disposition | Evidence |
|-------------|-------------|-------------|----------|
| EVAL-01 | 05-04, 05-08, 05-13 | **MET IN FULL** | Truth 1. Every named metric is implemented, ordered-label-bound, split-gated, present in 40/40 rows, and fixture-tested against scipy. Scoping note, not a shortfall: the COMPLETE set's user-facing door is the bench surface (`bench run`/`verify-cell` + rows), not `apr eval --task classify`, which exposes only F_avg, macro-F1, MCC and per-class F1 — no confusion matrix, no calibration diagnostics. The requirement says "can evaluate", and a user can. |
| EVAL-02 | 05-01, 05-02, 05-03, 05-06, 05-07, 05-11, 05-12, 05-13, 05-14 | **MET NARROWLY** (D-19) | Narrow on two counts. (a) The accepted amendment: one arm, 40 cells not 80; the 9B LoRA arm is deferred to D-ITEM-05-15, and no artifact in this phase states or implies a SetFit-versus-LoRA result — I checked `report.md`, which carries an explicit SCOPE block reading "ONE METHOD WAS MEASURED … Read the absence as absence". (b) Beyond the amendment: the amendment's own load-bearing promise is that the pairing MECHANISM is delivered. The recording half is delivered and independently verified 40/40; the binding half is not enforced by any shipped door (gap 2). |
| EVAL-03 | 05-05, 05-09, 05-12, 05-13 | **MET IN FULL** | Truth 3. Every field the requirement enumerates is present in 40/40 rows, with real, distinct, non-degenerate values, and resource figures carry their measurement boundary and mechanism class. Single-method scope does not narrow this requirement — its unit is the run, not the comparison. |
| EVAL-04 | 05-04, 05-05, 05-10, 05-11, 05-13 | **NOT MET** | Truth 4. The "exactly recompute" clause is met byte-for-byte and the "invalidates the report" clause is met against missing, tampered and self-consistently-shrunk inputs — genuinely strong results. But the gate's provenance recomputation, which is what makes the recomputation trustworthy rather than merely repeatable, reads a file the row chooses. Gap 1. |
| EVAL-05 | 05-06, 05-09, 05-11, 05-12, 05-13 | **MET NARROWLY** (D-19) | Truth 5. All figures present, bounded, and measured from the reloaded production artifact. Narrow because "compare" now spans shot levels within one method rather than two methods; the requirement's own words ("compare … from the reloaded production artifacts") are satisfied for the surviving scope, and the report refuses to average across hosts or across mechanism classes. |

### Anti-Patterns Found

Scope: the 62 code/contract/script files touched by the 109 phase-5 commits
(`git log --format='%H %s' <first-05>^..HEAD | grep -E '\(05[-)]'`), not the 1587 files in the raw
commit range — that range includes merged non-phase main work and would have produced false
attribution.

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | `TBD` / `FIXME` / `XXX` | — | **ZERO** in all 62 phase-5-authored files. The debt-marker gate passes cleanly. |
| — | — | `TODO` / `HACK` / `unimplemented!` / `todo!` | — | Zero introduced. The only `TODO` in a phase-5-*touched* file is `crates/apr-cli/src/commands/finetune.rs:625`, blamed to `4fbe63575c` (Noah Gift, 2026-03-29) — five months pre-phase, on the LoRA export path that D-19 defers. Not a phase-5 marker. |
| `crates/aprender-train/src/train/setfit/bench_gate_tests.rs` | 9-14 | Stale doc header: "MUTATING a programmatically-generated VALID **80-row** set" and "All six run in a default … invocation" | ℹ️ Info | The module doc still describes the pre-D-19 80-cell design. The Makefile banner at 2662-2671 carries the CORRECTED account (four re-mutated at the active 40-cell scope, two retained deferred-scope), and REQUIREMENTS.md carries the 2026-09-08 retraction. Doc drift inside the file most likely to be read when trusting the negatives; worth a one-line fix, not a gap. |

**Regression gate:** not re-run. The orchestrator ran it at the closing SHA (29,607 tests / 29,585
passed / 22 failed) and proved all 22 byte-identical to the pre-phase set and failing at the phase
base — 21 `aprender-train gpu::{guard,ledger,wait}` plus one `aprender-core setfit::artifact::
determinism` golden-hash test. I did not re-derive that and I do not report them as phase defects.
My own targeted runs (174 tests across the four suites this phase owns) were all green.

### Human Verification Required

None blocking. Both gaps are observable in code and reproducible by command; neither needs a human
to decide whether it happened. The one item that IS a human decision:

**Whether to close the phase with EVAL-04 not met.** The mechanism this phase exists to build works
on every axis it was designed against and fails on one it was not: an adversary who writes rows.
That is precisely this gate's threat model — the report's own header says a doctored cell "would
have REFUSED this report" — so I have graded it against that model rather than a softer one. A
maintainer may reasonably judge gap 1 a small, well-localized patch (a path-validation helper plus
three negative tests) rather than a re-plan.

### Gaps Summary

The phase built a great deal that is real, and I want to be precise about which parts.

**What is genuinely achieved.** Forty SetFit cells were actually run — the training walls (31s →
1739s across s8 → s64) and forty distinct artifact hashes are not something a stub produces. The
evidence set is complete, internally consistent, and small enough to audit: 123 tracked files, no
model binaries in the index. The report recomputes to the byte. The fail-closed behaviour is
stronger than the criterion demanded: I repaired a shrunken manifest's own digest so it was
internally flawless, and the gate still refused it because completeness is derived from the
contract rather than read from the manifest — that is the right architecture, and it is the
single best result in this verification. The D-19 descope is handled with unusual discipline: the
deferred two-method design is retained verbatim in the contract rather than deleted, the
paired-t machinery is kept and still fixture-tested, and the report carries an explicit SCOPE block
that names the absence instead of hiding it. The claims layer refuses to average across hosts or
across RSS mechanism classes, and labels every sampled figure a lower bound. The calibration
contract discloses that its production regime is provisional on 4 of 40 measured cells and that
five of six parameter classes have no legal epsilon under the old derivation rule — a phase that
wanted a clean score would not have written that down.

**What is not achieved.** Two links in the chain are asserted rather than recomputed, and both sit
in the one module that IS the phase's contribution.

The first is a hole (gap 1, EVAL-04). `verify_provenance` recomputes the lock hash from
`bench_dir.join(lock_record_path)` where `lock_record_path` is a string the row supplies. An
absolute path discards the base. I deleted the committed lock file, pointed the row at a file in
`/tmp`, and `apr setfit bench report` exited 0 while printing "provenance was recomputed from the
committed lock bytes rather than read off the rows". The gate's own attestation was false on the
run that produced it. This is not the disclosed residual — the residual concedes that a producer
holding the rows *and the lock files* can forge consistently; here there was no lock file at all.

The second is an absence (gap 2, EVAL-02). Under D-19 the entire remaining case for EVAL-02 is
that the pairing mechanism ships so the second arm can be added without re-running this half. The
mechanism's two halves are recording and recomputation. Recording is done and correct — I verified
all forty hashes against the forty committed manifests myself. Recomputation does not exist:
`bench report` returns 0 with the `selections/` directory deleted outright. The forty manifests are
inert files. So the phase goal's word "bound" is currently true of the data and false of the
mechanism, and a future LoRA arm would pair against a key nothing ever attested.

Both are the same shape, and it is the shape CLAUDE.md's Verification Discipline rule 5 names: a
guard that does not scan the surface where the decision is made. The fix for each is small and
local — validate the path, recompute the manifest — and each needs its own RED-turning negative
test at the active 40-cell scope, because by this phase's own rule 4 the 80-cell proofs do not
transfer.

Two findings I decline to charge to this phase. CR-01 is real and I reproduced it, but neither
`.SHELLFLAGS` line was authored by a phase-5 commit, no phase-5 SUMMARY claims the hardening, and
every phase-5 Make floor uses redirection rather than a pipe, so no phase-5 gate is weakened by it;
#2550's fix has simply never worked and should be re-opened separately. And the metric-forgery path
(spot-check D) is exactly what the report's `residual:` line already concedes — I record it as a
cheap hardening opportunity (recompute `f_avg` from the row's own confusion matrix) rather than as
a broken promise.

---

_Verified: 2026-09-08T22:55:00Z_
_Verifier: Claude (gsd-verifier)_
