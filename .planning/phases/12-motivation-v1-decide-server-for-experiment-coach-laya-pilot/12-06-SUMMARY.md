---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 06
subsystem: setfit-reference-arm
tags: [setfit, laya-adapter, k-labels, dump-probabilities, row-identity, ops-03, apr-eval]
status: complete

requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: laya_dir_to_canonical (two-label), data/decide/goldstandard-2024-setfit (byte-identity control), 10-06-PLAN.md Task 1 (the dump specification, deferred)
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: plan 12-05 bakeoff.py, which already reads the nine-key dump by row_ids and text_sha256
provides:
  - "laya_dir_to_canonical: any K >= 2 criteria in task.json document order; --dataset/--target/--task manifest identity (defaults are Phase 10's literals)"
  - "apr eval --split test --dump-probabilities FILE: nine-key per-row probability document from the same classify pass as the reported accuracy"
  - "a release apr built with --features setfit at the code commit (6a8ba6182), pinned by scripts/apr_bin.sh, whose eval --help lists --dump-probabilities"
affects: [12-10, 12-11, 12-16]

actuals:
  tokens: 11250
  tasks: 2
  commits: 3
plan_head_before: 44f7cd85e7549193a4bb0c27b5b48a596ef0d9a5
plan_head_after: 6a8ba61826d0b70b3bf23c9b16d92a60c5e4f65f

tech-stack:
  added: []
  patterns:
    - "identity strings are arguments with the previous run's literals as defaults, so a generalisation can be proven by byte-identity against the old output"
    - "pure dump builder over per-row structs, built by the same row function the measurement loop calls; the document validates its own column lengths"
    - "dump collected in the pass that counts correct rows (kept only when requested) and written with the crate's atomic_write before report()"

key-files:
  created: []
  modified:
    - crates/aprender-contrastive-data/examples/laya_dir_to_canonical.rs
    - crates/apr-cli/src/commands/eval/setfit.rs
    - crates/apr-cli/src/commands/eval/setfit_tests.rs
    - crates/apr-cli/src/extended_commands.rs
    - crates/apr-cli/src/dispatch_analysis.rs
    - crates/apr-cli/src/setfit_tag_tests.rs
    - crates/apr-cli/src/lib_dispatch_coverage.rs
    - crates/apr-cli/src/lib_verbose_inheritance_parse.rs
    - crates/apr-cli/src/lib_parse_rosetta_02.rs

key-decisions:
  - "predicted is validated as an argmax up to ties (no probability in the row exceeds probabilities[predicted]); the classifier's own tie-break is not assumed"
  - "ProbabilityDump::validate checks every column against n_rows and is called by the builder, so a length mismatch is refused by construction and also testable on a hand-edited document"
  - "a prediction whose label is not one of the artifact's labels is an InferenceFailed error naming the row id only (never the text), and only when a dump was requested"
  - "the dump destination gets the same pre-flight as --lock-out (refuse_existing_output in run, before the dataset is read); the report carries a note naming the dump path and row count"

requirements-completed: [D-08, D-25, D-05]

coverage:
  - id: D1
    description: "the adapter accepts K >= 2 criteria in document order and carries the manifest identity as arguments; Phase 10's adapted directory re-adapts byte for byte with the defaults"
    requirement: "D-25"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-contrastive-data --example laya_dir_to_canonical (18 passed, 0 failed)"
        status: pass
      - kind: integration
        ref: "laya_dir_to_canonical --laya-dir data/decide/goldstandard-2024 --output /tmp/p12-06-gs-setfit, then cmp of train.jsonl, validation.jsonl, test.jsonl, benchmark-manifest.json against data/decide/goldstandard-2024-setfit (all four IDENTICAL)"
        status: pass
    human_judgment: false
  - id: D2
    description: "a synthetic three-label Laya dir adapts, apr data select --shots 8 writes a selection manifest, apr setfit train --dry-run exits 0, and the adapted test split is the eval rows in file order with ids test:<i> and byte-identical inputs"
    requirement: "D-08"
    verification:
      - kind: integration
        ref: "/tmp/p12-06-syn-setfit (labels zeta,alpha,mu; train=[40,40,40] validation=[5,5,5] test=[12,12,12]); /tmp/p12-06-syn-dryrun.rc = 0; python identity check over 36 rows"
        status: pass
    human_judgment: false
  - id: D3
    description: "apr eval --dump-probabilities: nine keys in order, no text, row identity, all refusals (validation run, occupied destination before any dataset read, non-SetFit artifact), written before the report"
    requirement: "D-25"
    verification:
      - kind: unit
        ref: "cargo test -p apr-cli --features setfit --lib eval (248 passed, 0 failed); the six new tests listed ok; full apr-cli lib 7657 passed, 0 failed"
        status: pass
      - kind: other
        ref: "strict clippy on the apr-cli lib (-D warnings) rc 0; clippy over lib+tests reports no finding in any changed file; cargo fmt -p apr-cli --check clean; source-structure assertions (nine fields in order, atomic_write before report, no unwrap)"
        status: pass
    human_judgment: false
  - id: D4
    description: "a fresh release apr with the flag is pinned by scripts/apr_bin.sh and lists --dump-probabilities"
    requirement: "D-25"
    verification:
      - kind: other
        ref: "CARGO_INCREMENTAL=0 cargo build --release --bin apr --features setfit rc 0; scripts/apr_bin.sh pin rc 0 (apr 0.69.3 (6a8ba6182) == HEAD); apr eval --help lists --dump-probabilities"
        status: pass
    human_judgment: false
---

# Phase 12 Plan 06: SetFit reference arm on K labels and the probability dump Summary

The Laya-to-SetFit adapter now takes any K >= 2 labels and the manifest identity as arguments while reproducing Phase 10's adapted directory byte for byte, and `apr eval --split test --dump-probabilities FILE` writes the nine-key per-row document from the one classify pass.

## What was built

**Task 1 (tracer), `laya_dir_to_canonical`.** The `names.len() != 2` refusal became `< 2` ("needs at least two criteria"); empty and duplicate criterion names, malformed rows and cross-split leaks are refused as before. `--dataset`, `--target` and `--task` flow through `Inputs` into the `Manifest` (now owned `String`s); their defaults are exactly Phase 10's three literals and an empty value is a usage error. `CARVE_DOMAIN`, `DEFAULT_LAYA_DIR` and `DEFAULT_OUTPUT_DIR` are unchanged. The module doc carries the motivation usage example.

**Task 2, `--dump-probabilities`.** Executed 10-06's Task 1 as written, with the review-round-1 amendment to nine fields: `ordered_labels, row_ids, text_sha256, truth, predicted, probabilities, artifact_sha256, dataset_fingerprint, n_rows`.
- clap option on `Eval` (doc update on `--force`), threaded through `dispatch_classify_eval` and into `SetFitEvalArgs`; a non-SetFit artifact refuses it in the same loop as `--lock-out`.
- `check_split_flags` refuses it on `--split validation` naming the flag and `--split test`; `run` refuses an occupied destination (`refuse_existing_output`) before any dataset byte is read.
- `evaluate_test` keeps a `RowProbabilities` per row only when a dump is requested, built by `row_probabilities` (canonical id, sha256 of the input bytes, truth, predicted index, probabilities) in the loop that counts correct rows.
- `probability_dump` (pure) refuses no rows, wrong-length probability rows, non-finite values, out-of-range indices, a predicted index that is not an argmax, an empty id and a non-sha256 hash; `ProbabilityDump::validate` checks every column against `n_rows`.
- `run_test` writes the document through `setfit_train::atomic_write` after the measurement and before `report()`, using `grant.artifact_hash()` and `dataset.validation_witness().dataset_fingerprint_hex()`; the report gains a note naming the dump path and row count.
- The existing source-scan tests (one `grant.test()`, no `dataset.test()`, label-map gate before scoring, one atomic writer) pass unchanged.

## Byte-identity result for Phase 10's dir

`laya_dir_to_canonical --laya-dir data/decide/goldstandard-2024 --output /tmp/p12-06-gs-setfit` (defaults) printed `ADAPTED ... labels=not_antisemitic,antisemitic train=[6314,1344] validation=[702,150] test=[1755,374]` and `cmp` reported all four files IDENTICAL to `data/decide/goldstandard-2024-setfit`: train.jsonl, validation.jsonl, test.jsonl, benchmark-manifest.json.

## RED runs of the new tests

**Task 1** (log `/tmp/p12-06-t1-red.log`, not committed as a separate RED commit: Task 1 is a tracer, not `tdd="true"`; behaviour tests were still written and run first). Command `CARGO_INCREMENTAL=0 cargo test -p aprender-contrastive-data --example laya_dir_to_canonical`, exit 101, 13 passed / 3 failed. Failing, on the planned refusal (`two-label cell only`) and the three-criteria `expect`: `three_criteria_are_labels_0_1_2_in_document_order`, `three_label_dir_adapts_with_per_class_ceil_carve_and_eval_as_test`, and `input_defects_are_errors_not_silent_drops` (whose old "three labels are refused" assertion was inverted to "one refused, three accepted"). The identity-argument tests were written with the implementation because they need the new `Inputs` and `Args` fields to compile.

**Task 2** (RED commit `a2b29a67a`, log `/tmp/p12-06-t2-red.log`). The commit carries the clap option and call-site wiring plus not-yet-implemented stubs, so the test targets compile and fail on behaviour rather than on a build error. Command `CARGO_INCREMENTAL=0 rtk proxy cargo test -p apr-cli --features setfit --lib eval`, exit 101. The six target tests executed and FAILED:
- `eval_setfit_refuses_dump_probabilities_on_a_validation_run` (assertion at setfit_tests.rs:556, no flag refusal yet)
- `dump_probabilities_refuses_an_existing_destination_before_the_dataset_is_read` (assertion at :588, run proceeded past the destination)
- `probability_dump_has_exactly_its_keys_and_no_text`, `probability_dump_predicted_is_the_argmax_of_probabilities`, `probability_dump_row_identity_is_the_test_split` (the stub builder returned "not implemented", surfacing at the test's `expect`)
- `eval_dump_probabilities_on_a_non_setfit_artifact_is_refused` (assertion at setfit_tag_tests.rs:520, dispatch ignored the new parameter)

Semantic assessment: each target test ran and failed for the planned reason. `gsd_run check tdd-red-evidence` was not applied: cargo's console report is not one of its supported formats (TAP, JUnit XML, swift-testing, unittest, go test) and this plan is `type: execute`, not `type: tdd`. The RED log also showed one unrelated failure, `commands::eval::inference::execute_python_test_diagnostics_tests::assertion_failure_reports_nonzero_and_traceback`; it passed in the GREEN run and in the later full-lib run (7657 passed, 0 failed), so it is an intermittent test outside this plan's files, recorded and not investigated.

GREEN is commit `6a8ba6182`.

## Deviations from Plan

### Auto-fixed Issues

None - plan executed as written. Notes on judgment calls inside the plan's latitude:

**1. [Clarification] Row-identity builder shape.** The plan says the builder "refuses row_ids / text_sha256 of a length other than n_rows". With per-row structs the columns cannot differ, so the contract is carried by `ProbabilityDump::validate` (called by the builder and tested on hand-truncated documents) plus the builder's per-row checks. Same guarantee, stated where it can fail.

**2. [Clarification] Argmax up to ties.** "predicted is the argmax" is enforced as "no probability exceeds probabilities[predicted]", so a tie resolved either way by the classifier is not a false refusal.

## Authentication Gates

None.

## Known Stubs

None. (The temporary not-yet-implemented stubs existed only in the RED commit `a2b29a67a` and are replaced in `6a8ba6182`.)

## Threat Flags

None beyond the plan's register. The new file-write surface (the dump) is T-12-20/T-12-57/T-12-21 as planned: nine keys and no text, row identity and dataset fingerprint carried, collected only from `grant.test()` rows after the lock chain.

## Tracking updates

`gsd_run query state.*` / roadmap queries were not run: the known tooling issue (`04-VERIFICATION.md` has `status: gaps_acknowledged`, outside the tool's closed set) makes them error, and the instruction was to leave tracking to the orchestrator. STATE.md, ROADMAP.md and REQUIREMENTS.md are untouched, and the pre-existing uncommitted `.planning` and `justfile` changes were left alone.

## Commits

- `58f924f4c` feat(12-06): laya_dir_to_canonical adapts K >= 2 criteria with --dataset/--target/--task identity
- `a2b29a67a` test(12-06): failing tests for apr eval --dump-probabilities
- `6a8ba6182` feat(12-06): apr eval --split test --dump-probabilities writes per-row probabilities

## Binary note for plan 12-10

The release `apr` at `target/release/apr` reports `apr 0.69.3 (6a8ba6182)`, equals the code HEAD at the time and is accepted by `scripts/apr_bin.sh`. The SUMMARY commit that follows changes HEAD, so `apr_bin.sh` may call it stale until rebuilt; plan 12-10 repeats the build as its step 0.

## Self-Check: PASSED

- FOUND: crates/aprender-contrastive-data/examples/laya_dir_to_canonical.rs, crates/apr-cli/src/commands/eval/setfit.rs, setfit_tests.rs, extended_commands.rs, dispatch_analysis.rs, setfit_tag_tests.rs, lib_dispatch_coverage.rs, lib_verbose_inheritance_parse.rs, lib_parse_rosetta_02.rs
- FOUND commits: 58f924f4c, a2b29a67a, 6a8ba6182 (ancestors of HEAD)
