---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 05
subsystem: mcp-server
tags: [mcp, lambda, finalizer, step-functions, sagemaker, ecs, s3, dynamodb, laya, retention, rust]

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-01's finalize_run decision, attach_result_if_absent, lock (acquire, release, read), layout; 14-04's lambda crate, execution_arn and the cancel sink
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-03's owner verification of aws-sdk-ecs (14-LOCK-EVIDENCE.json names_verified)
provides:
  - second bin aprender-laya-train-finalizer in aprender-mcp-laya-train-lambda (the CDK asset renames it to bootstrap)
  - FinalizerCtx / FinalizerInput (three shapes) / handle() / reconcile() / FinalizeReport / ReconcileReport / FinalizerError
  - ObjectStore seam (S3 impl plus an in-memory fake that fails the n-th call of an operation)
  - ExecutionReader, TrainingJobs and PackTasks seams with aws-sdk-sfn, aws-sdk-sagemaker and aws-sdk-ecs impls and fakes
  - compute_terminal() and stop_live_compute(): the compute gate for the single-flight lock
  - verdict.json and manifest.json (laya-train-manifest-v1, with images, contracts and unlisted) written under runs/<name>/<task_id>/
affects: [14-08, 14-11, 14-13, 14-14]

plan_head_before: 13da75f5de78d132c31b8c7def35988cb513b59b
plan_head_after: 6373a26b1e870d818335658b2fcc4e63c34b977f

actuals:
  tokens: 38262
  tasks: 3
  commits: 4

tech-stack:
  added: [aws-sdk-sagemaker 1.210.0, aws-sdk-ecs 1.133.1]
  patterns:
    - "Every AWS call behind a small seam (ObjectStore, ExecutionReader, TrainingJobs, PackTasks) so each rule is proven offline on fakes"
    - "Record first (DynamoDB arbiter), then verdict.json, manifest.json, tags read back, lock release LAST; a held lock is the durable 'finalization incomplete' marker"
    - "Idempotent publication: an object already holding the end state is not rewritten, so a duplicate invocation writes nothing"
    - "Fault-injecting fake (fail the n-th call of an operation once) plus an end-state equality check proves crash-resume at every step"

key-files:
  created:
    - crates/aprender-mcp-laya-train-lambda/src/finalizer.rs
    - crates/aprender-mcp-laya-train-lambda/src/compute.rs
    - crates/aprender-mcp-laya-train-lambda/src/objects.rs
    - crates/aprender-mcp-laya-train-lambda/src/bin/aprender-laya-train-finalizer.rs
  modified:
    - crates/aprender-mcp-laya-train-lambda/Cargo.toml
    - crates/aprender-mcp-laya-train-lambda/src/lib.rs
    - crates/aprender-mcp-laya-train-lambda/README.md
    - crates/aprender-mcp-laya-train/src/lock.rs
    - Cargo.lock

key-decisions:
  - "A terminal lock holder stays live until the lock is released (lock.rs is_live): without it a client cancel freed the single-flight lock while compute was still billing"
  - "verdict.json is the stored status payload's own text, byte for byte, so a resume or a duplicate produces identical bytes"
  - "manifest.json is written once: a valid one that already lists this verdict.json (same sha256) is adopted, never rewritten"
  - "Unlisted counts objects under the run prefix that no writer manifest lists, excluding verdict.json, manifest.json and the three inventory files that cannot list themselves (training-manifest.json, partial-manifest.json, pack-manifest.json)"
  - "Reconcile scans RUNNING executions even when the lock is free (a free lock means every RUNNING execution is rogue), and a failed stop is raised after the rest of the sweep has run"
  - "The event's own input is ignored: the execution input is read with DescribeExecution and its task id must derive the event's execution ARN"

patterns-established:
  - "Integrity never writes and never releases; only Store and Compute errors resume"
  - "A gate that cannot read compute (a failed Describe or List) returns Err, never 'terminal'"

requirements-completed: [D-15, D-18, D-19, D-12, D-11, D-04]

coverage:
  - id: D1
    description: "Tracer: a Finalize payload for a completed run becomes a Completed task with a stored phase-completed payload, verdict.json (the payload, byte for byte), manifest.json with the fixtures' images and contracts and unlisted 0, keep tags on every entry and both files (read back), and a free lock; the log proves record -> verdict.json -> manifest.json -> tags -> release LAST"
    requirement: "D-18"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-mcp-laya-train-lambda --lib tracer_completed"
        status: pass
    human_judgment: false
  - id: D2
    description: "Every other end: gate_failed, pilot-waived (completed, verdict.waived, expire), refused pack (failed, no .apr), Catch error (bounded), ABORTED and TIMED_OUT events, a foreign machine and unknown shapes refused with nothing written"
    requirement: "D-15"
    verification:
      - kind: unit
        ref: "fin_gate_fail, fin_waived, fin_pack_refused, fin_catch_error, fin_abort_event, fin_timeout_event, fin_foreign_machine, fin_bad_shape, fin_wrong_prefix"
        status: pass
    human_judgment: false
  - id: D3
    description: "First writer wins in DynamoDB and S3: a duplicate writes nothing, a late FAILED event does not rewrite verdict.json or the stored phase and restores a lost keep tag, a tagging failure keeps the lock and the retry completes"
    requirement: "D-19"
    verification:
      - kind: unit
        ref: "fin_duplicate, fin_first_writer_wins, fin_tag_failure"
        status: pass
    human_judgment: false
  - id: D4
    description: "Client cancel: a (Cancelled, no result) record gets the cancelled payload through attach_result_if_absent (status unchanged), readable via get_result and via the library's train_status over loopback HTTP; a late Finalize payload for a completed run and a cancel racing the finish both end phase cancelled, never keep, verdict.json written once"
    requirement: "D-12"
    verification:
      - kind: unit
        ref: "fin_cancel_first, fin_cancel_late, fin_cancel_race"
        status: pass
    human_judgment: false
  - id: D5
    description: "NotFound is split: expired (older than TASK_TTL_MS) is finalized from the decision and releases the lock; inside the TTL, an owner mismatch against the lock envelope, and a terminal record with no result are Integrity (nothing written, lock untouched)"
    requirement: "D-18"
    verification:
      - kind: unit
        ref: "fin_record_expired, fin_record_missing_within_ttl, fin_owner_mismatch, fin_terminal_no_result"
        status: pass
    human_judgment: false
  - id: D6
    description: "Partial inventory: with no training-manifest.json the partial manifest's four entries plus verdict.json are listed, unlisted counts the two stray objects, manifest_partial is true"
    requirement: "D-18"
    verification:
      - kind: unit
        ref: "fin_partial_inventory"
        status: pass
    human_judgment: false
  - id: D7
    description: "Compute gate: an InProgress job is stopped once (never when Stopping or terminal), a live ECS pack task is stopped, a cancelled run's StopExecution is re-issued every sweep, nothing is written and the lock is held until compute is terminal; a replacement train is refused while it is held"
    requirement: "D-11"
    verification:
      - kind: unit
        ref: "fin_compute_gate_table, fin_awaiting_training, fin_cancel_packing, fin_cancel_retry"
        status: pass
    human_judgment: false
  - id: D8
    description: "Reconcile sweep: a lost event, a free lock (no write, no stop), a running holder, an unconfirmed dispatch (901 s finalizes failed dispatch-unconfirmed, 899 s does not), a rogue execution stopped once"
    requirement: "D-11"
    verification:
      - kind: unit
        ref: "fin_reconcile_lost_event, fin_reconcile_free, fin_reconcile_running, fin_reconcile_orphan, fin_reconcile_rogue"
        status: pass
    human_judgment: false
  - id: D9
    description: "Crash-resume: a failure at the verdict put, the manifest put, the first keep tag, a tag read-back and the lock release each returns Err with the lock held, and the sweep reaches the uninterrupted run's exact end state (objects, tags, stored result, lock)"
    requirement: "D-19"
    verification:
      - kind: unit
        ref: "fin_crash_resume_after_record, fin_crash_resume_after_verdict, fin_crash_resume_after_manifest, fin_crash_resume_after_tag, fin_crash_resume_after_readback"
        status: pass
    human_judgment: false
  - id: D10
    description: "Build: the package has exactly the bins [aprender-laya-train-finalizer, bootstrap]; Cargo.lock only grew (+52 lines, two packages, no removal); clippy -D warnings clean for lib, bins and tests; the finalizer cross-builds for aarch64-unknown-linux-gnu, 24.9 MB, glibc floor 2.30"
    requirement: "D-04"
    verification:
      - kind: other
        ref: "cargo metadata bins check; Cargo.lock HEAD-vs-work package set; cargo clippy -p aprender-mcp-laya-train-lambda --lib --bins --tests --no-deps -- -D warnings; cargo zigbuild --release --target aarch64-unknown-linux-gnu -p aprender-mcp-laya-train-lambda --bin aprender-laya-train-finalizer"
        status: pass
    human_judgment: false

duration: 29min
completed: 2026-10-29
status: complete
---

# Phase 14 Plan 05: Laya training finalizer Summary

**`aprender-laya-train-finalizer`: the one idempotent writer of a run's terminal task record, verdict.json, manifest.json, retention tags and lock release, driven by the state machine, EventBridge and a scheduled sweep, releasing single-flight only when the SageMaker job and every ECS pack task are terminal and publication is complete; proven offline with 36 new tests and an arm64 build of 24.9 MB.**

## Performance

- **Duration:** about 29 min (06:53 to 07:22 UTC)
- **Tasks:** 3 (one tracer, two auto), plus one cross-plan fix (lock.rs)
- **Files modified:** 9 (4 created, 5 modified)

## Accomplishments

- The finalizer accepts exactly three shapes (the state machine's Finalize payload, an EventBridge status event for ABORTED, TIMED_OUT or FAILED, and `{"source":"reconcile"}`) and refuses everything else, another state machine's event included, with nothing written.
- One finalization is ordered: compute terminal, DynamoDB record (the arbiter), verdict.json, manifest.json, tags read back, lock release last. Any failure after the record returns `Err` with the lock held, and the next event or sweep resumes. Five crash-resume tests compare the resumed end state with an uninterrupted run's, byte for byte.
- The compute gate reads the training job and the ECS pack task (matched by their `LAYA_TASK_ID` override) before it reads any output, stops what is live, writes nothing and keeps the lock. A cancel during packing therefore keeps a replacement `train` out until compute is stopped.
- The reconcile sweep makes best-effort delivery durable: a lost event or failed Finalize is re-driven, a lost `StopExecution` is re-issued every sweep for a cancelled run, an unconfirmed dispatch is failed after 900 s, and a rogue RUNNING execution is stopped.
- D-19 is enforced where the decision is: keep is derived from the winning payload (`completed` and `verdict.pass`), so a waived or gate-failed run, a cancelled run and a failed run are all tagged `expire`, every tag read back.

## Task Commits

1. **Task 1 (tracer): finalizer bin, seams and tracer** - `bb01e2f80` (feat)
2. **Task 2: every other end, duplicates, cancels, NotFound split, partial inventory** - `37466a6c5` (test)
3. **Task 3: compute gate stops, reconcile sweep, crash-resume, arm64 build** - `6373a26b1` (test)

Cross-plan fix, committed first because the cancel tests need it: `d0ad5317f` (fix, see Deviations).

**Plan metadata:** the docs commit that carries this file (docs(14-05)).

## Files Created/Modified

- `crates/aprender-mcp-laya-train-lambda/src/finalizer.rs` - `FinalizerCtx`, `FinalizerInput::parse`, `handle`, `reconcile`, the record settle (a/b/c), `publish`, reports, errors, and the `fin_*` tests
- `crates/aprender-mcp-laya-train-lambda/src/compute.rs` - the three compute seams, their AWS impls and fakes, `compute_terminal`, `stop_live_compute`
- `crates/aprender-mcp-laya-train-lambda/src/objects.rs` - `ObjectStore`, `S3ObjectStore`, the fault-injecting `FakeObjects`
- `crates/aprender-mcp-laya-train-lambda/src/bin/aprender-laya-train-finalizer.rs` - the lambda_runtime entry, five env vars read at init, one summary line per invocation
- `crates/aprender-mcp-laya-train-lambda/Cargo.toml`, `src/lib.rs`, `README.md` - the second bin, two SDK crates and sha2, exports and two env names, the finalizer's contract and IAM needs
- `crates/aprender-mcp-laya-train/src/lock.rs` - `is_live` fix (below)
- `Cargo.lock` - +52 lines, two packages added

## Decisions Made

See `key-decisions` above. The reasoning that matters most: the record is written before any S3 object, and verdict.json is the stored payload's own text, so "first writer wins" holds in S3 exactly as in DynamoDB and a duplicate or a resume can only reproduce the same bytes.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The single-flight lock was freed by a terminal holder task**
- **Found during:** Task 3 (writing `fin_cancel_packing`, whose last assertion is "a replacement `train` while the lock is held is refused single-flight")
- **Issue:** 14-01's `lock::is_live` read a holder whose task record was terminal as free. A client `tasks/cancel` makes the task `Cancelled` immediately while its job or pack task may still be billing, so a replacement `train` could take the lock and start a second paid run beside the first. This contradicted this plan's truth ("the lock is released only when compute is terminal") and could not be fixed inside the finalizer, because both `peek` and `acquire` consult `is_live`.
- **Fix:** a holder whose record still exists keeps the lock until it is released, past its lease, or gone. The lease and the sweep bound the wedge risk 14-01 wanted to avoid. `lock_terminal_holder_is_taken_over_without_a_release` was replaced by `lock_terminal_holder_keeps_the_lock_until_it_is_released`, and `lock_holder_with_a_gone_record_is_taken_over` keeps the no-wedge rule. The 93 `aprender-mcp-laya-train` lib tests still pass.
- **Files modified:** `crates/aprender-mcp-laya-train/src/lock.rs`
- **Commit:** d0ad5317f
- **For the owner:** this changes a 14-01 decision ("holder is terminal means free"). It is a behaviour change to the lock, not a new table or service, so it was treated as Rule 1, but revert it if you want the old reading. The consequence of reverting is that `fin_cancel_packing` and 14-14's "replacement train is refused while the job is Stopping" cannot hold.

**2. [Rule 3 - Blocking] The literal `cargo clippy -p X -- -D warnings` fails on two pre-existing `aprender-compute` warnings**
- Already in `deferred-items.md` (14-01) and handled the same way in 14-04: ran `--no-deps` (clean for lib, bins and tests of the lambda crate, and for `aprender-mcp-laya-train` lib and tests). Nothing else in the verify script was changed.

### Differences from the plan text (no behaviour lost)

- `handle` returns `FinalizeReport`, with the sweep's own account in `FinalizeReport.reconcile` (a boxed `ReconcileReport`) when the input was `{"source":"reconcile"}`. The plan's `reconcile(ctx) -> ReconcileReport` exists as written.
- `compute_terminal` returns `Result<ComputeState, String>` and `ComputeState` also carries `pack_task_arns`: a failed Describe or List must not read as "terminal".
- verdict.json is the stored payload as is (schema `laya-train-status-v1`, urls null), as the truths say. The "Artifacts" line naming schema `laya-train-verdict-v1` is not followed; `layout::SCHEMA_VERDICT` is therefore still unused. 14-11's fetch reads `phase` and `waived` from it and gets them.
- The plan's "64 KiB per log tail" read is not implemented: `finalize_run` takes no log, so reading one would only add a failure mode. JSON reads are capped at 1 MiB.
- `unlisted` also excludes `training-manifest.json`, `partial-manifest.json` and `pack-manifest.json`, which inventory the run and cannot list themselves. Without that, `unlisted` is never 0 for a healthy run and `fin_partial_inventory`'s expected 2 is unreachable.
- `fin_*` test count: the plan names 20 (Task 2) and 14 (Task 3, including the tracer) and its verification line says 33; the crate now has 36 new tests (60 lib tests, 24 before): the 34 named plus `fin_time_parsing` and `fin_compute_gate_table`.
- The work was written in one pass with the tests alongside it, then committed in the plan's three tranches (implementation with the tracer, then Task 2's tests, then Task 3's tests, each verified at its own commit). Red-first TDD was not run literally. To compensate, eight mutations of the finalizer and the lock were applied and the named tests were required to fail: the compute gate always terminal, release before the tags, terminal holder read as free, no cancel-first on ABORTED, keep for a waived run, verdict.json always rewritten, no owner check, no rogue stop. Seven were killed at once; the owner-check mutation survived because `fin_owner_mismatch` stopped on the missing record instead, so the test now seeds a decoy record under the other owner and the mutation is killed.

**Total deviations:** 1 Rule 1 fix (cross-plan), 1 Rule 3 (known, deferred), 6 differences from plan text. **Impact:** none beyond the lock reading above.

## Auth Gates

None. No AWS call was made: every test is offline (the only socket is the closed-loopback-port cancel-sink test from 14-04 and the loopback MCP server in `fin_cancel_first`).

## Issues Encountered

- The post-edit hook runs rustfmt on the crate (as in 14-04), so source line counts are larger than what was typed.
- `rtk` rewrites `git diff` into a stat view; `rtk proxy git diff` was used to measure the realized diff for `actuals`.

## Known Stubs

None.

## Residual risks (not stubs, recorded for later plans)

- **Live behaviour unverified.** The SageMaker "not found" classification (a `ValidationException` whose message contains "not found") and the ECS `DescribeTasks` override visibility of `LAYA_TASK_ID` are written from the SDK shapes and AWS documentation and proven only against fakes. 14-14's live cancel tests are the first real check; a wrong reading fails closed for the first (an error, lock held) and open-to-awaiting for the second (a pack task not matched is read as no pack task).
- **A manifest-listed object that no longer exists** makes `PutObjectTagging` fail on every retry, so the lock stays held until its 11 h lease. That is the intended fail-closed direction and trips the FinalizerErrors alarm, but it is a wedge a human must clear.
- **Concurrent invocations** (the state machine's Finalize, an event and a sweep at once) are safe by CAS and idempotent publication, but only the in-memory backend's CAS was exercised.

## Threat Flags

None beyond the plan's register. T-14-15, T-14-16, T-14-57, T-14-17, T-14-18, T-14-19, T-14-63, T-14-64 and T-14-SC are mitigated by the tests named under coverage D1 to D10; the lock-only-grew proof for T-14-SC is in `Cargo.lock` (+52 lines, no package moved or vanished, `aws-sdk-ecs` verified in `14-LOCK-EVIDENCE.json`).

## User Setup Required

None.

## Next Phase Readiness

- **14-08 (CDK stack)** must give the finalizer role exactly the IAM listed in the plan and the README, add `states:ListExecutions` on the machine, set the five env vars, and wire all three triggers:
  - the state machine's Finalize state passes `{"source":"state-machine","execution_arn":<$$.Execution.Id>,"start_time":<$$.Execution.StartTime>,"input":<$$.Execution.Input>}` with `error` present only when a Catch fired (`Error` and `Cause` keys); `input` may be the object or its JSON string;
  - an EventBridge rule on that one machine for `ABORTED`, `TIMED_OUT`, `FAILED`, passing the whole matched event;
  - a 10-minute schedule whose constant input is exactly `{"source":"reconcile"}`.
  The pack task's `EcsRunTask` container overrides must set `LAYA_TASK_ID` to the task id, and the finalizer needs `APRENDER_LAYA_TRAIN_CLUSTER_ARN` and `_PACK_FAMILY`.
- **14-11 (fetch)** can rely on `manifest.json` listing `verdict.json` with its size and sha256 and on `unlisted` being a count of objects no writer manifest lists.
- **14-13** builds this bin: measured arm64 size 24.9 MB (ceiling 100 MB), glibc floor 2.30 (ceiling 2.34); the CDK asset renames it `bootstrap`.
- **14-14** should read the finalizer log lines to say whether a stop came from the `.sync` abort or the finalizer, and check the two live readings in "Residual risks".

## Self-Check: PASSED

Verified: the four created files exist; commits `d0ad5317f`, `bb01e2f80`, `37466a6c5` and `6373a26b1` are ancestors of HEAD; `cargo test -p aprender-mcp-laya-train-lambda --lib` 60 passed 0 failed with every named test `ok`; `cargo test -p aprender-mcp-laya-train --lib` 93 passed; `monorepo_invariants` 15 passed (register still 11, packages not bins); `check_duplicate_bin_names.sh` PASS; clippy `--no-deps -D warnings` clean for lib, bins and tests of the lambda crate and for the lock crate; the package's bins are exactly `[aprender-laya-train-finalizer, bootstrap]`; no HEAD `Cargo.lock` package moved or vanished; the aarch64 binary is `ARM aarch64`, 24,864,272 bytes, glibc floor 2.30.
