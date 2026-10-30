---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 18
subsystem: api
tags: [laya, redaction, wr-03, failure-surface, ratchet, mutation-testing, gap-closure, task-store, step-functions]
status: complete

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: "14-16's sweep and artifact, 14-17's StatusPayload.notes and the --mutations runner with ratchet [I4], 14-19's provenance sidecar and check_surface_binding"
provides:
  - "redact.rs: caller_safe (blacklist redactor, idempotent and bounded), stage_error (Catch reduction, allowlist of reason keys) and RedactingBackend (fixed text for internal store errors)"
  - "StatusPayload::to_value is the redaction point for error, verdict.pack.line and every note"
  - "sfn.rs start_error_label: the dispatch error is 'StartExecution <code>', and the SDK text goes to tracing"
  - "bootstrap.rs wraps the DynamoDbTaskBackend in RedactingBackend; the finalizer binary is not wrapped (its errors are log-only)"
  - "the ratchet is empty; the sweep is failure_surface_sweep_clean (4800 + 10 cells, I1..I8 hold)"
  - "eight WR-03 mutations registered: FAILURE SURFACE MUTATIONS OK 17/17 (the full table) and 8/8 (--only WR-03)"
affects: [14-22]

actuals:
  tokens: 22556
  tasks: 3
  commits: 3
plan_head_before: 730fd483047b8182aa757cf0cafb8854fdeff22b
plan_head_after: f0f0a466941d0de2850284a04bea614e24f0c262

tech-stack:
  added: []
  patterns:
    - "sanitize at the boundary, not at the site: one serializer, one backend wrapper and one code-only label cover every caller-visible path"
    - "allowlist where the text is ours (fixed backend text, error codes, whitelisted Cause keys), and a blacklist only over job-written lines"
    - "a planted value no blacklist rule knows proves the allowlist reduction on its own (otherwise the serializer masks a forwarded Cause)"
    - "truncation that keeps a partial span only if that span passes the rule on its own, so the redactor stays idempotent"

key-files:
  created:
    - crates/aprender-mcp-laya-train/src/redact.rs
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-18-SUMMARY.md
  modified:
    - crates/aprender-mcp-laya-train/src/lib.rs
    - crates/aprender-mcp-laya-train/src/layout.rs
    - crates/aprender-mcp-laya-train/src/finalize.rs
    - crates/aprender-mcp-laya-train-lambda/src/finalizer.rs
    - crates/aprender-mcp-laya-train-lambda/src/sfn.rs
    - crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs
    - scripts/laya_deploy/laya_failure_surface.py
    - crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-FAILURE-SURFACE.md

key-decisions:
  - "SK-12 decided: the account id in the run bucket's name is a caller handle, not a leak. The caller names that bucket in dataset_uri, so the account id in run_prefix and the manifest links is something it already holds, and redacting it would break the s3:// handle that train_status and laya-fetch resolve. caller_safe runs only on the free-text fields (error, verdict.pack.line, notes). I4 excludes the bucket account id by design. Inside a free-text field, an s3:// URI or a bucket-name account id reads [redacted]"
  - "The digit rule redacts a run of exactly twelve digits only when no letter is adjacent and no full stop comes before it, and it keeps the last group of a UUID. The plan's rule (no adjacent digit, no full stop before) would have redacted a hex digest that contains a 12-digit run, and the all-digit fixture task id"
  - "A URL's redaction continues through = , ; until the next whitespace, quote, angle bracket, brace, backslash or pipe, so a presigned URL's signature, credential or token after an = is never left behind"
  - "The sweep plants HOSTILE_UNLISTED under a Cause key outside the whitelist. Without it, a Cause forwarded instead of reduced would still read clean, because to_value's blacklist redacts every account shape, so the sweep could not see M-WR03-c"
  - "start_error_label falls back to a fixed word (TimeoutError, DispatchFailure, ResponseError, ConstructionFailure) for a failure that never reached the service, so the caller can still tell those apart. Each word is itself an identifier code"
  - "The ratchet const stays, empty (KNOWN_DEFECTS = &[]), so the sweep's 'violations outside the ratchet' message, which the mutation runner reads, still names an invariant"

patterns-established:
  - "redact::caller_safe(text, max) is the only function that makes free text caller-visible; new caller text goes through StatusPayload::to_value or a fixed or code-only string"
  - "--sweep is a table of (crate, filter, named tests, minimum) runs"

requirements-completed: [D-05, D-08, D-10, D-11, D-18, D-22]

coverage:
  - id: D1
    description: "caller_safe and stage_error with a rule-7 case table: must-redact (ARNs in three partitions, the IAM sentence, ECR digest URI, presigned URL, s3 URI, account digits, AKIA/ASIA), must-keep (the real smoke verify line byte for byte, 21-digit floats, sha256 hex, 12-fraction floats, 13-digit epoch, 11-digit, UUIDs, RFC 3339, States.TaskFailed, container path, both note shapes), 11/12/13-digit boundaries, idempotence over a generated corpus, the bound"
    requirement: D-18
    verification:
      - kind: unit
        ref: "crates/aprender-mcp-laya-train/src/redact.rs#redact_must_redact_table, redact_must_keep_table, redact_digit_boundaries, redact_is_idempotent_and_bounded, redact_truncation_never_reveals_an_account_id, stage_error_table, stage_error_keeps_only_the_first_four_reasons"
        status: pass
    human_judgment: false
  - id: D2
    description: "The one serializer redacts error, verdict.pack.line and notes; the finalizer reduces the Catch Cause; the sweep's I4 holds in all 4800 + 10 cells with four planted constants and an empty ratchet"
    requirement: D-10
    verification:
      - kind: unit
        ref: "crates/aprender-mcp-laya-train-lambda/src/finalizer.rs#failure_surface_sweep_clean"
        status: pass
      - kind: unit
        ref: "crates/aprender-mcp-laya-train/src/redact.rs#redact_notes_through_to_value, redact_error_and_pack_line_through_to_value"
        status: pass
    human_judgment: false
  - id: D3
    description: "Request side: RedactingBackend (mint, lock, envelope, get/result through pmcp's task store), the train_status store error, and the dispatch reasons and label are free of the planted constants; benign reasons round-trip"
    requirement: D-10
    verification:
      - kind: unit
        ref: "crates/aprender-mcp-laya-train/src/lib.rs#hostile_backend_mint_failure_is_caller_safe, hostile_backend_lock_failure_is_caller_safe, hostile_backend_envelope_failure_is_caller_safe, hostile_backend_status_and_task_protocol_are_caller_safe, hostile_dispatch_reasons_are_caller_safe"
        status: pass
      - kind: unit
        ref: "crates/aprender-mcp-laya-train-lambda/src/sfn.rs#start_error_label_table"
        status: pass
    human_judgment: false
  - id: D4
    description: "Each WR-03 bound has a red-side mutation (M-WR03-a..h) that turns a named test or I4 red; the earlier nine still refuse"
    requirement: D-11
    verification:
      - kind: other
        ref: "python3 -I scripts/laya_deploy/laya_failure_surface.py --mutations -> FAILURE SURFACE MUTATIONS OK 17/17; --only M-WR03-a..h -> 8/8"
        status: pass
    human_judgment: false
  - id: D5
    description: "Artifact, markdown and Rust agree: known_defects empty, every SK row has its final disposition, FS-AS-01 is closed by 14-19, sinks are re-scanned"
    requirement: D-05
    verification:
      - kind: other
        ref: "python3 -I scripts/laya_deploy/laya_failure_surface.py --check (known_defects=none, sinks=26, hits 102), --selftest (14 cases), --sweep (19/19 in 4 runs)"
        status: pass
      - kind: other
        ref: "LAYA_GATES_ONLY=failure-surface just laya-gates-selftest -> ROW failure-surface GREEN, AWS CALLS 0"
        status: pass
    human_judgment: false
  - id: D6
    description: "No new dependency or bin; contracts, decide crate, sibling SetFit crates and workflows unchanged"
    requirement: D-22
    verification:
      - kind: other
        ref: "git diff --quiet HEAD -- contracts crates/aprender-decide crates/aprender-setfit-train-lambda crates/aprender-mcp-setfit-train Cargo.lock Cargo.toml (rc 0); cargo test -p aprender-core --test monorepo_invariants (15 passed)"
        status: pass
    human_judgment: false
  - id: D7
    description: "Whether the redacted caller texts still read usefully (for example 'User: [redacted] is not authorized to perform: dynamodb:PutItem'), and whether the blacklist's accepted residual (T-14-86) is acceptable"
    verification: []
    human_judgment: true
    rationale: "Readability of caller-facing errors and acceptance of a residual risk are owner judgements; the tests prove only absence of the planted shapes"

duration: 36min
completed: 2026-10-30
---

# Phase 14 Plan 18: caller-safe text at the boundaries (WR-03) Summary

**WR-03 is closed at three boundaries rather than at each site:**

- the payload serializer (`StatusPayload::to_value`) redacts `error`, `verdict.pack.line` and every note;
- a `RedactingBackend` around the DynamoDB task store returns fixed text instead of the SDK's;
- the dispatch error is an AWS error code only.

The Catch's Cause is reduced to whitelisted reason keys instead of being forwarded. The sweep's ratchet is empty: all eight invariants hold in 4,810 cells. Each of the eight bounds has a source mutation that turns a named test or I4 red.

## Performance

- **Duration:** 36 min
- **Started:** 2026-10-30T19:46:03Z
- **Completed:** 2026-10-30T20:22:44Z
- **Tasks:** 3
- **Files modified:** 10 (1 created)

## Accomplishments

- **The redactor (Task 1, tracer).** `caller_safe(text, max)` works in four steps:
  - It drops control characters and collapses whitespace.
  - It splits the text into spans at whitespace and at `" ' , ; ( ) [ ] { } < > = \ |`.
  - It replaces a span with `[redacted]` when the span holds any of: `arn:aws` (any case, every partition), `amazonaws.com`, `://`, an account-id digit run, or an AKIA/ASIA access key.
  - It truncates after redaction. A span that the cut leaves partial is kept only if it passes the rule on its own, so the function is idempotent.
- **The Catch reduction.** `stage_error` keeps the Error name only when it is an identifier. From a JSON Cause it keeps the first four values of `FailureReason`, `errorMessage`, `StoppedReason`, `StopCode`, `Reason`, `ExitCode`, `SecondaryStatus` and `TrainingJobStatus`. The finalizer's `bounded_error` now calls it, and the full Cause goes to `tracing::warn`.
- **The serializer.** `StatusPayload::to_value` redacts `error`, `verdict.pack.line` and `notes`. The stored result, verdict.json, tasks/result and train_status are all built from this one function. The real smoke verify line is kept byte for byte.
- **The request side (Task 2).**
  - `RedactingBackend` logs an `Internal` store error in full and returns `the task store failed on <op>`.
  - `bootstrap.rs` wraps the DynamoDB backend with it.
  - `train_status` reads `no task X on this server` or `the task store could not be read`.
  - `start_error_label` makes `StartError.message` read `StartExecution <code>`.
  - Dispatch reasons also pass `caller_safe`, as defense in depth.
- **Proofs (Task 3).**
  - Eight mutations, M-WR03-a..h, plus the nine earlier ones: `FAILURE SURFACE MUTATIONS OK 17/17`. The plan's `--only` run gives `8/8`.
  - `--sweep` now covers 19 named tests in 4 runs.
  - The artifact and the markdown are closed out: SK-01..06 and SK-NOTES are closed, SK-07 and SK-09 are log-only, SK-12 is decided, and FS-AS-01 is closed by 14-19. FS-SM-01 now cites `check_surface_binding`.
  - The sinks were re-scanned: 26 groups, 102 hits.

## Task Commits

1. **Task 1 (tracer): the redactor, the payload serializer and the Catch reduction.** `79ffa99bb` (fix)
2. **Task 2: the request-side boundaries, the backend wrapper and the dispatch label.** `1a0606a98` (fix)
3. **Task 3: red-side proofs, the clean ratchet and the artifact.** `f0f0a4669` (test)

**Plan metadata:** the docs commit that follows this file.

## Files Created/Modified

- `crates/aprender-mcp-laya-train/src/redact.rs` (new): `caller_safe`, `stage_error`, `RedactingBackend`, `PLACEHOLDER`, `MAX_CALLER_TEXT_CHARS`, and 10 tests (the case tables).
- `crates/aprender-mcp-laya-train/src/layout.rs`: `to_value` is the redaction point, and its doc says so.
- `crates/aprender-mcp-laya-train/src/lib.rs`: `pub mod redact`; `caller_safe` on dispatch reasons; the `train_status` store-error mapping; `HostileBackend` and five hostile tests.
- `crates/aprender-mcp-laya-train/src/finalize.rs` (test fixture only): `fixtures::through_catch` now builds the stage error through `stage_error`.
- `crates/aprender-mcp-laya-train-lambda/src/finalizer.rs`:
  - `bounded_error` reduces the Cause and logs it;
  - `MAX_STAGE_ERROR_CHARS` is removed;
  - `KNOWN_DEFECTS = &[]`;
  - the sweep is renamed `failure_surface_sweep_clean`;
  - `HOSTILE_UNLISTED` is planted in the Catch Cause.
- `crates/aprender-mcp-laya-train-lambda/src/sfn.rs`: `start_error_label`, `start_error_code`, the `tracing::error` of the SDK text, and `start_error_label_table`.
- `crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs`: the `RedactingBackend` wrap and its doc.
- `scripts/laya_deploy/laya_failure_surface.py`: the M-WR03 cases, the `SWEEP` table, and selftest cases that work with an empty ratchet.
- `failure-surface-v1.json`, `14-FAILURE-SURFACE.md`: the record, brought in line.

## Decisions Made

See `key-decisions` in the frontmatter. The SK-12 decision and its rationale:

- **What was decided.** The bucket-name account id is a caller handle. Only the free-text fields are redacted, and I4 states the exclusion.
- **Why.** The caller already holds that account id: it names the bucket in `dataset_uri`. Redacting `run_prefix` or the links would break the `s3://` handle.
- **The limit.** Inside an error or a note, an `s3://` URI or a bucket-name account id still reads `[redacted]`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The plan's digit rule would have redacted must-keep values.**
- **Found during:** Task 1.
- **Issue:** The plan's rule only excludes an adjacent digit and a full stop before the run. Under it, a sha256 hex that contains a run of exactly twelve digits flanked by letters would be redacted, which would break the verify line's `artifact_sha256`. So would the last group of an all-digit UUID: the fixture task id `...-000000000001`, and the `RIVAL`-style ids in error text.
- **Fix:** A letter on either side also keeps the run, and a run that is a UUID's last group (8-4-4-4- hex before it) is kept.
- **Verification:** The must-keep table has a hex digest with an inner 12-digit run, a hex digest that opens with one, and the all-digit UUID. The boundary table covers letter neighbours. M-WR03-g (at-least-twelve) still turns the table red.
- **Committed in:** `79ffa99bb`.

**2. [Rule 2 - Missing critical] Presigned-URL tails survived splitting at `=`.**
- **Found during:** Task 1.
- **Issue:** `=` is a delimiter, so `X-Amz-Signature=<sig>` and a security token after `=` would have stayed as separate kept spans.
- **Fix:** Once a span contains `://`, the redaction runs on to the next whitespace, quote, angle bracket, brace, backslash or pipe.
- **Verification:** The presigned-URL must-redact case asserts that no `Signature` and no key id survive.
- **Committed in:** `79ffa99bb`.

**3. [Rule 2 - Missing critical] The sweep could not see the Catch reduction on its own.**
- **Found during:** Task 3, designing M-WR03-c.
- **Issue:** If `stage_error` forwarded the raw Cause, to_value's blacklist would still redact every planted constant, so I4 would stay green.
- **Fix:** A fourth planted constant, `HOSTILE_UNLISTED`, which no blacklist rule knows, under `OutputDataConfig.KmsKeyId` (outside the whitelist). The I4 text records it. This also covers threat T-14-86.
- **Verification:** M-WR03-c is refused by `stage_error_table` and by the sweep with I4.
- **Committed in:** `f0f0a4669`.

**4. [Rule 1 - Bug] The test fixture modelled the old Catch text.**
- **Found during:** Task 1.
- **Issue:** `fixtures::through_catch` in finalize.rs built `<Error>: <Cause>` "the way bounded_error does", which is no longer true.
- **Fix:** It now calls `redact::stage_error`. This edits the test-only fixtures of a file outside the plan's list; the production code of finalize.rs is unchanged.
- **Committed in:** `79ffa99bb`.

**5. [Rule 3 - Blocking] The checker's selftest assumed a non-empty ratchet.**
- **Found during:** Task 3.
- **Issue:** `drop_defect` popped `known_defects`, and `defect_without_plan` searched for a defect row. Both raise with an empty ratchet.
- **Fix:** The cases now add an entry the Rust const does not carry, and turn a row into a defect with no closing plan. Each still refuses only its own rule.
- **Committed in:** `f0f0a4669`.

---

**Total deviations:** 5 auto-fixed (2 bug, 2 missing critical, 1 blocking). **Impact:** each one either keeps a must-keep value intact or makes a bound provable on its own. No new dependency and no scope creep.

**Plan-wording interpretations (recorded; behaviour as specified):**

- `RedactingBackend` landed in the Task 1 commit (in redact.rs, with the rest of the module). Task 2 wired it in and tested it.
- `start_error_label` also takes a fixed word for SDK failures that carry no service code. See the decisions.
- The artifact rename (`failure_surface_sweep_known_defects_only` to `failure_surface_sweep_clean`) was done in Task 3. Between the Task 1 and Task 3 commits, `--check` was therefore red. No hook or CI runs it.
- The "state-machine binding rows" read as FS-SM-01, which now cites `check_surface_binding`, plus the D-a and Dispositions prose. No row other than FS-AS-01 was a defect.

## Issues Encountered

- The rtk hook rewrites `cargo test` and `git diff` output. Test lines were read from the deps binary, from log files and with `rtk proxy`. The checker's own cargo subprocesses are unaffected.
- The mutation runner rewrites sources, so `--check` and the sink scan were run only after each run had restored the tree. `git status` showed only the intended files.
- **STATE tool failure (known):** the `gsd-tools query state.*` verbs error because `04-VERIFICATION.md` has the status `gaps_acknowledged`. That file was not edited. See the state-update note below.
- **Windows ledger:** nothing to append. There are no stubs, no skipped tests and no unrun verify steps.

## Known Stubs

None.

## Threat Flags

None beyond the register:

- **T-14-85** is mitigated at the three boundaries.
- **T-14-86** is accepted and now also partly mitigated: `HOSTILE_UNLISTED` proves that the allowlist reduction drops unknown shapes in the Catch Cause. Unknown shapes in job-written lines remain the residual.
- **T-14-87** is mitigated: eight mutations, and the runner re-runs the tables.
- **T-14-SC:** no package was installed. Cargo.toml and Cargo.lock are unchanged.

## User Setup Required

None. No AWS call was made and nothing was deployed.

## Next Phase Readiness

- **14-22:**
  - The request function (`bootstrap`) and the finalizer both ship this code. The redeploy is owner-run (D-10: the OAuth window of a request-function redeploy is the owner's decision).
  - The finalizer provenance sidecar is stale by design (`finalizer.rs`, `finalize.rs`, `layout.rs` and `redact.rs` are in the closure). The owner must re-run `just laya-trainer-synth <env>` before `infra diff` and `deploy`.
- The failure-surface class has no pinned defect left (`known_defects=none`).

## Self-Check: PASSED

- **Files:** redact.rs and this SUMMARY exist; all nine modified files exist.
- **Commits:** `79ffa99bb`, `1a0606a98` and `f0f0a4669` are ancestors of HEAD. Ledger: `git rev-list --count 730fd4830..HEAD` is 3.
- **Plan verification, run on the final tree:**
  - `cargo test -p aprender-mcp-laya-train --lib`: 113 passed.
  - `cargo test -p aprender-mcp-laya-train-lambda --lib`: 70 passed.
  - `cargo clippy -p aprender-mcp-laya-train -p aprender-mcp-laya-train-lambda --lib --bins --tests --no-deps -- -D warnings`: rc 0.
  - `cargo fmt --all -- --check`: rc 0.
  - `--check`: `FAILURE SURFACE OK rows=69 ... sinks=26 (hits 102) ... known_defects=none`.
  - `--selftest`: `FAILURE SURFACE SELFTEST OK 14 cases`.
  - `--sweep`: `FAILURE SURFACE SWEEP OK 19/19 named tests ok in 4 runs`.
  - `--mutations`: `OK 17/17`. `--only M-WR03-a..h`: `OK 8/8`.
  - `cargo test -p aprender-core --test monorepo_invariants`: 15 passed.
  - `LAYA_GATES_ONLY=failure-surface just laya-gates-selftest`: GREEN, AWS CALLS 0.
  - `git diff --quiet HEAD -- contracts crates/aprender-decide crates/aprender-setfit-train-lambda crates/aprender-mcp-setfit-train Cargo.lock Cargo.toml .github`: rc 0.

---
*Phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out*
*Completed: 2026-10-30*
