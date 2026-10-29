---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 04
subsystem: mcp-server
tags: [mcp, pmcp, lambda, step-functions, s3, dynamodb, laya, deployment-unit, rust]

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-01's aprender-mcp-laya-train library (ServerCtx::new, build_server, Dispatcher, DatasetSource, layout::ExecutionInput)
  - phase: 06-native-time-series-forecasting-stack
    provides: DynamoDbTaskBackend, CancelSink and the SetFit loopback-proxy bootstrap shape, reused as is
provides:
  - crate aprender-mcp-laya-train-lambda (publish = false) with its README and one bin, `bootstrap`
  - S3DatasetSource over an ObjectReader seam, with HeadObject size caps and the missing/denied mapping
  - SfnDispatcher over Executions and Presign seams, classify_start_error, execution_arn, guard_link_key
  - SfnCancelSink, a best-effort StopExecution on its own current-thread runtime with a 10 s bounded join
  - the FALSIFY-MONO-011 deployment-unit register grown 10 -> 11, the dup-bin allowlist with six claimants
affects: [14-05, 14-08, 14-10, 14-13]

plan_head_before: 5521a3a9e272e63b0d637b14d87e087691e5484c
plan_head_after: 46b9606f9ac82e0e50b54364aa1b9f4c84d3b711

actuals:
  tokens: 19200
  tasks: 3
  commits: 3

tech-stack:
  added: [aws-sdk-sfn 1.108.0, aprender-mcp-laya-train-lambda workspace member]
  patterns:
    - "AWS calls behind small seams (Executions, Presign, ObjectReader) so every rule is proven offline on fakes"
    - "classify_start_error: Rejected only for an answer that proves nothing started; everything else is Ambiguous and goes through confirm"
    - "cancel sink: own thread, own enable_all current-thread runtime, bounded join, block_in_place on a multi-thread caller"
    - "tracer drives the real build_server over loopback streamable HTTP (stateless config shared with the bootstrap)"

key-files:
  created:
    - crates/aprender-mcp-laya-train-lambda/Cargo.toml
    - crates/aprender-mcp-laya-train-lambda/README.md
    - crates/aprender-mcp-laya-train-lambda/src/lib.rs
    - crates/aprender-mcp-laya-train-lambda/src/dataset.rs
    - crates/aprender-mcp-laya-train-lambda/src/sfn.rs
    - crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs
  modified:
    - Cargo.toml
    - Cargo.lock
    - crates/aprender-core/tests/monorepo_invariants.rs
    - scripts/duplicate_bin_names_allowlist.txt
    - justfile

key-decisions:
  - "Unknown, unmodeled and construction errors from StartExecution are Ambiguous, not Rejected: the safe default costs one DescribeExecution and never a double run"
  - "guard_link_key requires the prefix to be exactly runs/<name>/<task_id>/ (three safe segments), not merely runs/-prefixed"
  - "The bootstrap reads its three env vars twice: once in main() before run() so an unset one fails the function's init, once in build()"
  - "server_config() (stateless + MAX_REQUEST_BYTES) lives in the lib so the bootstrap and the tracer test build the same HTTP config"

patterns-established:
  - "A tracer that needs a private handler drives the public MCP surface over loopback HTTP instead of widening the library's API"

requirements-completed: [D-22, D-03, D-07, D-11, D-18, D-10]

coverage:
  - id: D1
    description: "Tracer: one train over the real server on loopback HTTP reads a dataset through S3DatasetSource and starts exactly one execution laya-<task_id> with the closed nine-key input; link mints inside the run prefix and refuses ../x"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-mcp-laya-train-lambda --lib tracer_request"
        status: pass
    human_judgment: false
  - id: D2
    description: "Dataset caps and mapping: 33,554,433-byte file and over-total refused before any get, body cut at the admitted size, absent files None, shift absent is no refusal, AccessDenied is the typed dataset-access refusal"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-mcp-laya-train-lambda --lib dataset_ (9 tests)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Dispatch classification and confirm: every Rejected kind, every Ambiguous kind (timeout, connection, throttling, 5xx, ExecutionAlreadyExists), confirm from DescribeExecution on the deterministic ARN, malformed ARN is None"
    verification:
      - kind: unit
        ref: "execution_dispatch_classify_table, execution_dispatch_maps_kinds_to_dispatch_errors, execution_confirm_answers_from_describe, execution_arn_is_derived_from_the_state_machine_arn"
        status: pass
    human_judgment: false
  - id: D4
    description: "Link guard: only keys inside runs/<name>/<task_id>/, no .., leading /, empty segment, %, backslash, ? or #; 3,600 s TTL in the dispatcher's own bucket"
    verification:
      - kind: unit
        ref: "link_guard_confines_keys_to_the_run_prefix, link_guard_mints_with_the_read_time_ttl_in_the_dispatchers_bucket"
        status: pass
    human_judgment: false
  - id: D5
    description: "Cancel sink: returns within the join bound when the stop hangs, logs and returns on failure, never panics; the real aws_sdk_sfn client against a closed loopback port from a multi-thread runtime returns in under 11 s with one error line naming StopExecution and no credential"
    verification:
      - kind: unit
        ref: "cancel_sink_returns_within_the_join_bound_when_the_stop_hangs, cancel_sink_real_client (+3 more cancel_sink tests)"
        status: pass
    human_judgment: false
  - id: D6
    description: "Register: FALSIFY-MONO-011 green at deployment units 11 and allowed_bins 27; dup-bin guard, its self-test and the dup-bin-names gate row green with six claimants; models/decide snapshot unchanged"
    verification:
      - kind: other
        ref: "cargo test -p aprender-core --test monorepo_invariants; scripts/check_duplicate_bin_names.sh (+ --self-test); LAYA_GATES_ONLY=dup-bin-names just laya-gates-selftest"
        status: pass
    human_judgment: false
  - id: D7
    description: "arm64 bootstrap builds for aarch64-unknown-linux-gnu with glibc floor 2.30 (ceiling 2.34), 25 MB (ceiling 250 MB); fails closed with a message naming the variable when one env var is unset"
    verification:
      - kind: other
        ref: "cargo zigbuild --release --target aarch64-unknown-linux-gnu -p aprender-mcp-laya-train-lambda --bin bootstrap; host bootstrap run with each env var unset"
        status: pass
    human_judgment: false

duration: 19min
completed: 2026-10-29
status: complete
---

# Phase 14 Plan 04: Laya training request function (deployment unit) Summary

**`aprender-mcp-laya-train-lambda`: the pmcp.run request function for the Laya training server, a size-capped S3 dataset source, a Step Functions dispatcher that separates "proven not started" from "may have started", a bounded best-effort cancel sink, and a read-time presign guard, all proven offline, registered as the eleventh deployment unit, and cross-built for Lambda arm64 (glibc 2.30, 25 MB).**

## Performance

- **Duration:** about 19 min (06:03 to 06:22 UTC)
- **Tasks:** 3 (one tracer, two auto)
- **Files modified:** 11 in the plan's list, plus the crate's `Cargo.toml`/`README.md` as new files

## Accomplishments

- The tracer drives the real `build_server` over loopback streamable HTTP with the same `server_config()` the bootstrap uses, and proves one `train` reads a 3-class dataset through `S3DatasetSource` and starts exactly one execution named `laya-<task_id>` whose input has exactly the nine `ExecutionInput` keys, with `inputs_sha256` parsing back to the three files' sha256.
- `classify_start_error` is a pure function over the SDK error: Rejected only for `ExecutionLimitExceeded`, `InvalidArn`, `InvalidExecutionInput`, `InvalidName`, `StateMachineDoesNotExist`, `StateMachineDeleting`, `ValidationException` and AccessDenied; a timeout, a connection failure, throttling, a 5xx, an unknown code and `ExecutionAlreadyExists` are Ambiguous. `confirm` answers from `DescribeExecution` on the deterministic execution ARN.
- The dataset source applies the transport bounds on HeadObject sizes before any GetObject, again while the body streams, maps an absent file to `None` (so validate answers `<role>-missing`), and maps a 403 to the typed refusal `dataset-access` naming bucket and prefix without echoing the SDK text.
- The cancel sink is best effort by design and says so: durability is the `Cancelled` record, the held lock and 14-05's reconcile sweep. `cancel_sink_real_client` uses a real `aws_sdk_sfn::Client` (static credentials, closed loopback port, 2 s operation timeout) called from a multi-thread runtime.
- D-22 landed in one commit: the `bootstrap` bin, FALSIFY-MONO-011 entry 11 and baseline 11 with the `10 -> 11` note, the allowlist's six-package bootstrap line, and the justfile row expecting 6. No commit left `readme_contract`, FALSIFY-MONO-011 or the dup-bin guard red.

## Task Commits

1. **Task 1 (tracer): lib-only crate, README, tracer through the S3 and Step Functions seams** - `07a3ac0b7` (feat)
2. **Task 2: S3 size caps and denied mapping, classified start errors, bounded cancel sink, link guard** - `fd11187c6` (feat)
3. **Task 3: bootstrap bin and the register grown 10 -> 11, arm64 cross-build** - `46b9606f9` (feat)

**Plan metadata:** the docs commit that carries this file (docs(14-04)).

## Files Created/Modified

- `crates/aprender-mcp-laya-train-lambda/src/sfn.rs` - `Executions`/`Presign` seams and their SDK implementations, `classify_start_error`, `SfnDispatcher`, `execution_arn`, `guard_link_key`, `SfnCancelSink`, `CANCEL_JOIN_BOUND`
- `crates/aprender-mcp-laya-train-lambda/src/dataset.rs` - `ObjectReader`, `S3ObjectReader`, `S3DatasetSource`, `ReadError`
- `crates/aprender-mcp-laya-train-lambda/src/lib.rs` - env names, `require_env`, `server_config`, re-exports, the tracer
- `crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs` - the SetFit loopback proxy with only `build()` changed, plus an eager `read_env()` in `main`
- `crates/aprender-core/tests/monorepo_invariants.rs`, `scripts/duplicate_bin_names_allowlist.txt`, `justfile` - the D-22 register edits
- `Cargo.toml`, `Cargo.lock` - one `members` line; `aws-sdk-sfn` and the crate added

## Decisions Made

- Ambiguity is the default for everything not proven harmless (construction failures and unknown error codes included); the cost is one `DescribeExecution`, never a second execution.
- The link guard demands the exact `runs/<name>/<task_id>/` shape for the prefix. A `runs/`-only prefix passed the first draft and was caught by the case table.
- The bootstrap fails the Lambda init when an env var is unset (checked in `main` before `run`), rather than at the first `train`; verified by running the host build with each variable removed.
- HeadObject not-found is trusted as "absent" only because the grant carries a prefix-scoped `s3:ListBucket` (S3 answers 403 for a missing key without it). 14-08/14-10 must grant it.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The literal `cargo clippy -p X -- -D warnings` fails on two pre-existing `aprender-compute` warnings**
- **Found during:** Task 2 and Task 3 verification (the Task 3 script reached it on its first run)
- **Fix:** ran with `--no-deps` (clean for lib, bins and tests of this crate). Already logged in `deferred-items.md` by 14-01. The Task 3 verify script was run with `--no-deps` added to its clippy line and nothing else changed.
- **Files modified:** none

**2. [Rule 1 - Bug, own code] `guard_link_key` accepted a bare `runs/` prefix**
- **Found during:** Task 2 (`link_guard_confines_keys_to_the_run_prefix` failed on first run)
- **Fix:** the prefix must be exactly three safe segments, `runs/<name>/<task_id>/`.
- **Commit:** fd11187c6

**3. [Rule 3 - Blocking] The tracer cannot call `submit` directly**
- **Issue:** `submit` is private in 14-01's `lib.rs` and the plan says this crate uses only the public API. Widening 14-01's surface for a test was the alternative.
- **Fix:** the tracer starts the real server on `127.0.0.1:0` with `server_config()` and calls `tools/call train` with `"task": {}` over HTTP. That proves more than a direct call (the typed-tool schema, the task create gate, the stateless config).
- **Commit:** 07a3ac0b7

### Finding for the verifier: Cargo.lock moved 19 dependency edges (no package version moved)

The plan says Cargo.lock "changes only by ADDING packages". The package-level proof holds: every `(name, version, source)` in the starting lock is still present, and the lock gained exactly two packages (`aws-sdk-sfn 1.108.0` and the new crate). But cargo re-resolved on adding `aws-sdk-sfn` and re-pointed 19 existing packages' dependency edges to already-locked newer versions, all within their declared ranges: `hyper-util`, `quinn` and `quinn-udp` from `socket2 0.5.10` to `0.6.4`, `generator`'s `windows-link`/`windows-result`, `iana-time-zone`'s `windows-core`, and fifteen `windows-sys` edges to 0.61.2 (`anstyle-*`, `colored`, `dirs-sys`, `errno`, `is-terminal`, `nu-ansi-term`, `os_pipe`, `rustix`, `rustls-platform-verifier`, `socket2 0.6.4`, `tempfile`, `terminal_size`, `winapi-util`). The `socket2` re-point is the only one that touches a non-Windows runtime path (`hyper-util`'s socket handling). I did not try to undo it: cargo offers no way to pin an edge, and the old versions remain in the lock for their other users. Re-run the workspace `workspace-test` before merging if this matters.

**Total deviations:** 3 auto-fixed, 1 finding. **Impact:** no scope creep.

## Issues Encountered

- `rtk` rewrites `grep`/`diff` output in the Bash tool and showed "0 matches" for patterns that do have matches; basic-regex `\|` alternation needs `grep -E`, and a diff piped through grep printed nothing. Python reads of the diff were used for the lock evidence.
- The post-edit hook runs rustfmt on the crate, which splits `new_current_thread().enable_all()` across lines; the doc comment on `SfnCancelSink` carries the literal string the plan's grep looks for.

## Known Stubs

None. No hardcoded empty values reach a response, and nothing is deferred to a later plan inside this crate.

## Threat Flags

None beyond the plan's register. T-14-10, T-14-11, T-14-12, T-14-62, T-14-13, T-14-SC and T-14-14 are mitigated by the tests named under coverage D1 to D7.

## User Setup Required

None. No AWS call was made; every test is offline (the only socket is a closed loopback port).

## Next Phase Readiness

- 14-08's `RequestLambdaPolicy` needs `states:StartExecution` on the machine, `states:DescribeExecution` and `states:StopExecution` on its executions, `s3:GetObject`/`s3:ListBucket` on `datasets/` (ListBucket prefix-scoped, or a missing file reads as a 403 `dataset-access`), and `s3:GetObject` on `runs/` for the presign.
- Environment the deployed function needs: `APRENDER_LAYA_TRAIN_TABLE`, `APRENDER_LAYA_TRAIN_BUCKET`, `APRENDER_LAYA_TRAIN_STATE_MACHINE_ARN`.
- 14-05 (finalizer): its bin must NOT be named `bootstrap` in this crate (the comment in the manifest says why); the reconcile sweep owes the repeated StopExecution/StopTrainingJob/StopTask that makes the cancel sink's best-effort safe.
- 14-13 builds this bin through cargo-pmcp: measured arm64 `bootstrap` is 25 MB with a glibc floor of 2.30 (ceilings 250 MB and 2.34).

## Self-Check: PASSED

Verified: the six created files exist; commits `07a3ac0b7`, `fd11187c6`, `46b9606f9` are ancestors of HEAD; `cargo test -p aprender-mcp-laya-train-lambda --lib` 24 passed 0 failed; `readme_contract` 16 passed; `monorepo_invariants` passed with baselines 11 and 27; `check_duplicate_bin_names.sh` and its `--self-test` pass; the dup-bin-names gate row is GREEN; `cargo metadata` shows bins `[bootstrap]`, `publish = []`; the aarch64 binary is `ARM aarch64`, glibc floor 2.30.
