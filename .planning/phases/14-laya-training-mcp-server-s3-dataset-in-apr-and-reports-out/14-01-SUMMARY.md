---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 01
subsystem: mcp-server
tags: [mcp, pmcp, mcp-tasks, laya, single-flight, finalize, golden-fixtures, rust]

requires:
  - phase: 06-native-time-series-forecasting-stack
    provides: the SetFit training server's TaskBackend, AprenderTaskStore, mint handoff and CancelSink, reused as is
  - phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
    provides: aprender-decide's task.json parser, sha256 helper and the committed laya_tiny gate report the verdict extraction is proven against
provides:
  - crate aprender-mcp-laya-train (lib only, no bin, publish = false) with its README
  - layout.rs, the shared vocabulary of S3 keys, schema ids, bounds, run-output structs and the one status payload
  - ServerCtx::new and build_server(ctx, version) exposing validate_dataset, train and train_status
  - Dispatcher (dispatch / confirm / link / bucket), DatasetSource and LocalDatasetSource seams
  - the single-flight lease (lock.rs) and the pure finalize decision (finalize.rs)
  - attach_result_if_absent for client-cancelled tasks
  - five golden fixtures under crates/aprender-mcp-laya-train/fixtures/
affects: [14-02, 14-04, 14-05, 14-06, 14-07, 14-11]

plan_head_before: f1d1de83c743a4f5666880396e010719e2a8f45e
plan_head_after: 74bd585b7460049c3ada24acc51ee406a80db51f

actuals:
  tokens: 48500
  tasks: 3
  commits: 3

tech-stack:
  added: [aprender-mcp-laya-train workspace member]
  patterns:
    - "one Arc<dyn TaskBackend> shared by the task store and the lock through ServerCtx::new"
    - "Rejected vs Ambiguous dispatch classification; lock released only on a confirmed non-start"
    - "verdict copied from gate-report.json at the locations train.py writes them, never recomputed"
    - "lease-only lock expiry: the lock record carries no backend TTL"
    - "read-time links: every stored payload has url null, train_status mints links"

key-files:
  created:
    - crates/aprender-mcp-laya-train/Cargo.toml
    - crates/aprender-mcp-laya-train/README.md
    - crates/aprender-mcp-laya-train/src/lib.rs
    - crates/aprender-mcp-laya-train/src/layout.rs
    - crates/aprender-mcp-laya-train/src/lock.rs
    - crates/aprender-mcp-laya-train/src/finalize.rs
    - crates/aprender-mcp-laya-train/src/validate.rs
    - crates/aprender-mcp-laya-train/fixtures/outcome-v1.json
    - crates/aprender-mcp-laya-train/fixtures/training-manifest-v1.json
    - crates/aprender-mcp-laya-train/fixtures/pack-outcome-v1.json
    - crates/aprender-mcp-laya-train/fixtures/manifest-v1.json
    - crates/aprender-mcp-laya-train/fixtures/gate-report-v1.json
  modified:
    - Cargo.toml
    - Cargo.lock

key-decisions:
  - "ServerCtx owns the backend Arc and builds the store from it, so the store and the lock can never address two backends"
  - "validate_files inspects and reports epochs_required / epochs_range / epochs_fixed; only validate_for_submit (train) refuses epochs"
  - "A dispatch error is Rejected or Ambiguous; Ambiguous calls confirm and releases the lock only on NotStarted, an unconfirmable start keeps the lock and the Working task for the 14-05 reconcile sweep"
  - "The lock's liveness also checks the holder's own task record, so a finalizer that crashed before releasing cannot wedge the deployment"
  - "Verdict numbers are Option<f64>: copied or null, never a default; finalize_run refuses a report that lacks a required field"
  - "A waived acceptance (gate_fail + claim + accepted pack with waived = claim) is completed with verdict.pass false and keep false"

patterns-established:
  - "Golden fixtures are the single source of run-output shapes; the tracer and every finalize row build from them with targeted edits"
  - "Refusals display as REFUSED <rule>: <detail> and never echo row text"

requirements-completed: [D-06, D-07, D-09, D-11, D-15, D-18, D-21, D-05]

coverage:
  - id: D1
    description: "Tracer: one train call reaches dispatch, a fixture-fed finalize makes train_status report a completed verdict with read-time links while the stored result carries every url null"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-mcp-laya-train --lib tracer (7 tests, tracer_shared_backend among them)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Argument refusals: exactly dataset_uri, name and epochs; epochs rule from the contract; name and dataset_uri grammars"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-mcp-laya-train --lib args / name / uri"
        status: pass
    human_judgment: false
  - id: D3
    description: "Single-flight lease with no backend TTL, holder-only release, CAS takeover, a race with one winner, and the ambiguous-dispatch compensation paths"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-mcp-laya-train --lib lock / submit_single_flight / submit_dispatch"
        status: pass
    human_judgment: false
  - id: D4
    description: "validate_dataset inspects (reports epochs_required for 20 shots per class) while train refuses missing epochs"
    verification:
      - kind: unit
        ref: "validate_reports_epochs_above_16, validate_reports_epochs_above_16_through_the_tool, submit_epochs_enforced"
        status: pass
    human_judgment: false
  - id: D5
    description: "Finalize decision table (every phase row, pilot-waiver rows, inputs-changed, partial manifest, images, contracts, integrity) with the verdict proven against the committed real laya_tiny gate report"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-mcp-laya-train --lib finalize / verdict_from_real_report / payload"
        status: pass
    human_judgment: false
  - id: D6
    description: "Client-cancelled task reads as cancelled before the finalizer and holds a stored cancelled payload after attach_result_if_absent; nothing replaces a stored result"
    verification:
      - kind: unit
        ref: "status_cancelled_unfinalized, attach_cancelled, attach_second_noop, attach_refuses_working"
        status: pass
    human_judgment: false
  - id: D7
    description: "Crate is lib only with a README; readme_contract and monorepo_invariants stay green"
    verification:
      - kind: other
        ref: "cargo metadata (no bin, publish = []); cargo test -p aprender-core --test readme_contract; cargo test -p aprender-core --test monorepo_invariants"
        status: pass
    human_judgment: false

duration: 75min
completed: 2026-10-29
status: complete
---

# Phase 14 Plan 01: Laya training MCP server library Summary

**A lib-only `aprender-mcp-laya-train` crate: three MCP tools over MCP Tasks with a DynamoDB-safe single-flight lease, an ambiguity-aware dispatcher seam, and a pure finalize decision whose verdict is copied from the trainer's real report nesting, proven end to end on the in-memory backend with five golden fixtures.**

## Performance

- **Duration:** about 75 min (start time not captured at spawn; estimated)
- **Completed:** 2026-10-29
- **Tasks:** 3 (one tracer, two auto/tdd)
- **Files modified:** 14 (12 new under the crate and fixtures, root `Cargo.toml` and `Cargo.lock`)

## Accomplishments

- One `train` call is proven from argument parse through fetch, validate, lock, mint, envelope and dispatch to `working`, and a fixture-fed `finalize_run` then makes `train_status` answer `completed` with read-time links while the stored `tasks/result` payload carries every `url` null.
- `ServerCtx::new` builds the task store from the same backend `Arc` the lock uses (`AprenderTaskStore`'s backend is private), and `tracer_shared_backend` proves the minted task and the lock record land in one backend.
- The epochs rule is read from `contracts/laya-finetune-gate-v1.yaml` (never literals) and split by D-21: `validate_files` inspects and reports `epochs_required / epochs_range / epochs_fixed`, only `validate_for_submit` (what `train` calls) refuses `epochs`.
- Dispatch uncertainty never frees the lock around a started execution: `Rejected` releases and compensates, `Ambiguous` asks `confirm` and releases only on `NotStarted`, an unconfirmable start keeps the lock and the Working task for the 14-05 reconcile sweep.
- `finalize_run` implements the full decision table including the pilot-waiver rows (pack and verify decide), `inputs-changed` without `prefilter_disagreement`, partial-manifest inventories, image digests and contract hashes; `verdict_from_report` is tested on the committed real `laya_tiny` report and a synthetic 3-seed report with the real nesting (`ece_post` is `fine_tuned.ece_post`).
- A client-cancelled task reads as `cancelled` at read time and `attach_result_if_absent` gives it a stored payload by CAS without changing its status or replacing a result.

## Task Commits

1. **Task 1 (tracer): one train -> finalize -> train_status path** - `239ca2268` (feat)
2. **Task 2: argument refusals and the single-flight lease behaviours** - `3e4cdf1f1` (feat)
3. **Task 3: golden fixtures, finalize table, payload invariants, cancelled-task rule** - `74bd585b7` (feat)

**Plan metadata:** the docs commit that carries this file (docs(14-01)).

_Note: the tracer's description already puts the whole decision table, the dispatch classification and the validation split in Task 1, so Task 1's commit carries most of the implementation; Task 2 and Task 3 add the tests, fixtures, `attach_result_if_absent` and the read-time cancelled branches the plan assigns to them. Separate RED commits were not made._

## Files Created/Modified

- `crates/aprender-mcp-laya-train/src/lib.rs` - `ServerCtx`, `Dispatcher`, `DatasetSource`, `LocalDatasetSource`, `RunSpec`, `mod args` (`TrainArgs`, `ValidateArgs`, `StatusArgs`, `resolve_epochs`, `check_name`, `parse_dataset_uri`), submit / status / validate handlers, `build_server`, `http_config`
- `crates/aprender-mcp-laya-train/src/layout.rs` - keys, tags, schema ids, bounds, `Outcome`, `TrainingManifest`, `PackOutcome`, `Manifest`, `ExecutionInput`, `StatusPayload`, `Verdict`, `inputs_string`
- `crates/aprender-mcp-laya-train/src/lock.rs` - `LOCK_OWNER`, `acquire`, `release`, `peek`, `read`
- `crates/aprender-mcp-laya-train/src/finalize.rs` - `finalize_run`, `verdict_from_report`, `attach_result_if_absent`, the fixtures test module
- `crates/aprender-mcp-laya-train/src/validate.rs` - `validate_files`, `validate_for_submit`, `ValidationPolicy`, `Refusal`
- `crates/aprender-mcp-laya-train/fixtures/*.json` - the five golden fixtures
- `crates/aprender-mcp-laya-train/{Cargo.toml,README.md}`, root `Cargo.toml` (one `members` line), `Cargo.lock` (one package)

## Decisions Made

- `validate_dataset` answers a refusal as a tool error (`REFUSED <rule>: ...`), the same voice as `train`, rather than a `{valid: false}` result. The plan said "answers ... or the Refusal"; this keeps one refusal shape.
- The lock's liveness also checks the holder's task record (found through the stored `holder_owner`): terminal or gone means the lock is free. This is the plan's "holder is terminal" rule made independent of the finalizer remembering to release.
- `Verdict` metric fields are `Option<f64>` so a field absent from the report reads `null`, never a default; `finalize_run` fails a run whose report lacks `pass`, `margin`, `fine_tuned.ece_post`, `fine_tuned.macro_f1` or `zero_shot.macro_f1`.
- The envelope's `run_prefix` stores the full `s3://` URI (what the working answer needs); `RunSpec` and `ExecutionInput` carry the key form.

## Deviations from Plan

### Auto-fixed / extended items

**1. [Rule 3 - Blocking] The literal clippy gate cannot pass for a reason outside this crate**
- **Found during:** Task 1 verification
- **Issue:** `cargo clippy -p aprender-mcp-laya-train --lib --tests -- -D warnings` exits 101 because `-D warnings` is applied to every workspace member clippy-driver compiles and `aprender-compute` has two pre-existing unused-variable warnings (`quant_ops/mod.rs:219,318`).
- **Fix:** ran `cargo clippy -p aprender-mcp-laya-train --lib --tests --no-deps -- -D warnings`, which is clean; logged the pre-existing warnings in `deferred-items.md` (out of scope).
- **Files modified:** none

**2. [Rule 2 - Missing critical] `StatusPayload.manifest_partial` and `FinalizeDecision.manifest_partial`**
- **Found during:** Task 3 (`finalize_partial_manifest` requires "the payload records the inventory as partial")
- **Fix:** one bool on the payload (same key set for every phase) and on the decision, so the finalizer can persist it.
- **Files modified:** `layout.rs`, `finalize.rs`

**3. [Rule 2 - Missing critical] `FinalizeInput.bucket`**
- **Issue:** the payload's `run_prefix` is an `s3://` URI but `ExecutionInput` carries the key form only.
- **Fix:** `FinalizeInput` carries the bucket (the finalizer knows it). 14-05 must supply it.

**4. [Rule 2 - Missing critical] Extra integrity checks in `finalize_run`**
- **Issue:** the table did not say what to do with a report missing a required field, an outcome and report that disagree about `pass`, or an accepted pack whose `.apr` is not listed with the reported sha256.
- **Fix:** each is `failed` with an `integrity` (or `gate-report.json lacks ...`) error rather than a defaulted verdict. Tests: `finalize_malformed_report_is_refused_not_defaulted`, `finalize_without_a_listed_model_is_an_integrity_failure`.

**5. [Rule 1 - interface shape] `lock::acquire` returns `AcquireError {Held, Backend}`, and `peek_at(backend, now)` exists beside `peek(backend)`**
- **Issue:** a backend failure is not "held", and the tests need a controllable clock. The plan's `Held {holder}` struct is kept inside `AcquireError::Held`.

**6. Owner check order.** The reserved-owner refusal runs before the dataset fetch (fail closed before any I/O) rather than at step (d) after validation; still before any lock or mint.

**Total deviations:** 6 (1 blocking-environment, 4 missing-critical or interface, 1 ordering). **Impact:** no scope creep; items 2 to 5 add fields or checks that later plans (14-04, 14-05) must honor and are listed under Next Phase Readiness.

## Issues Encountered

- Rustfmt (run by a post-edit hook and by `cargo fmt -p`) expanded the files to about 3x the written line count; no behavior effect.
- The first full test run after adding Task 3's tests had one wrong expectation (`attach_cancelled` compared a stored payload with a read-time one that, correctly, carries minted links); fixed in the test.

## Known Stubs

- `DatasetSummary.deferred_rules` is an always-empty `Vec` by plan design ("empty until 14-02"). It is not wired to any UI and does not affect this plan's goal; 14-02 populates it.

## Threat Flags

None. This plan adds no network endpoint, auth path or file access; T-14-01 to T-14-05, T-14-56, T-14-59 to T-14-61 are mitigated by the tests named in the plan.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 14-02 completes the rule set behind `validate_files` (and `deferred_rules`) and adds `validate_bound_timing`.
- 14-04 implements `Dispatcher` (Step Functions, with `confirm` on the deterministic execution name), `DatasetSource` (S3) and a `TaskBackend` (DynamoDB) behind `ServerCtx::new`.
- 14-05 (finalizer) must build `FinalizeInput` including `bucket`, pass the partial manifest when the complete one is absent, call `attach_result_if_absent` after a client cancel, and use `lock::read` / `release` in its reconcile sweep.
- 14-06 and 14-07 read `fixtures/outcome-v1.json`, `training-manifest-v1.json` and `pack-outcome-v1.json`; 14-11 reads `manifest-v1.json` and `gate-report-v1.json`.
- Later plans copying the literal `cargo clippy ... -- -D warnings` gate need `--no-deps` until the two `aprender-compute` warnings are fixed (see `deferred-items.md`).

## Self-Check: PASSED

Verified: all 12 created files exist; the three task commits (`239ca2268`, `3e4cdf1f1`, `74bd585b7`) are ancestors of HEAD; `cargo test -p aprender-mcp-laya-train --lib` 84 passed 0 failed; the plan's three `<verify>` scripts pass (with the `--no-deps` clippy variant); `cargo test -p aprender-core --test readme_contract` and `--test monorepo_invariants` pass; `cargo metadata` shows the crate as a member with no bin and `publish = []`.
