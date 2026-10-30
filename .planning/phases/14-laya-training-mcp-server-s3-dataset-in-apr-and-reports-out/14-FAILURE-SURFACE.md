---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 16
artifact: failure-surface
schema: laya-failure-surface-v1
json: crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json
checker: scripts/laya_deploy/laya_failure_surface.py
derived_at_head: 327d9cdb2
derived_on: 2026-10-30
---

# Phase 14: the finalizer and state-machine failure surface

The four open items the Phase 14 verification found in one place (WR-01, WR-02, WR-03, WR-04, plus IN-01) are one
class: **the finalizer and state-machine boundary must give a classified terminal outcome, release the lock and expose
only caller-safe text at every failure point.** This file is the inspection record of that boundary. Its rows are the
same ids as `failure-surface-v1.json`; `python3 -I scripts/laya_deploy/laya_failure_surface.py --check` refuses any
difference between the two, and re-derives from the code what can be re-derived (the axes, the catch-fed classes and the
ratchet out of `finalizer.rs`, the exit-class map by running `job_exit_code`, every sink count by a fresh scan, the
state machine out of `laya-trainer-stack.ts`).

No production behaviour was changed to produce it (plan 14-16 is test, fixture and tooling only). The production code
it describes is byte-identical to the verification HEAD `359788182` (`git diff 359788182 -- crates/aprender-mcp-laya-train/src
crates/aprender-mcp-laya-train-lambda/src/{compute,dataset,lib,sfn}.rs` is empty, and the non-test prefix of
`finalizer.rs` is unchanged; `objects.rs` gained test-only fakes).

## The class invariant

Eight numbered invariants. The sweep (`finalizer.rs` tests, section `failure_surface`) computes every one per cell with
an oracle that never calls the implementation's own rules, and reads locks and tags from the records, never a log line.

| Id | Invariant |
|----|-----------|
| I1 | terminal and classified: after an Ok finalization the task record is terminal and holds a stored result whose phase is completed, gate_failed, failed or cancelled |
| I2 | finalization total and lock released: with no injected fault the first pass returns Ok and releases the single-flight lock, and a later reconcile sweep returns Ok; a held lock is allowed only for compute still billing or one injected transient S3 or backend fault the next pass clears |
| I3 | class decided from the records: when outcome.json parses and names the task, the phase follows the class table whatever the delivery path (refused_other failed with prefilter_disagreement true and the REFUSED line; inputs-changed failed with the flag false; crash and timeout failed with their class text; cpu fallback failed; pass and gate_fail follow the gate report and the pack record); an absent, malformed, foreign or removed outcome is failed with a non-empty error |
| I4 | caller-safe text: the stored payload, verdict.json and manifest.json carry none of the planted hostile constants (an account-id-shaped digit string, an ARN and an ECR host containing it) planted in the Catch Cause, the trainer's refused line, the pack verify and packed lines and an injected S3 error |
| I5 | no .apr unless completed: neither the stored payload's manifest nor manifest.json lists an .apr for any phase but completed |
| I6 | aborted means cancelled: an ABORTED execution yields a cancelled record and a cancelled stored payload |
| I7 | retention tags: every key the finalizer inventories or names (each manifest.json entry, manifest.json, and every self-listing manifest that exists) carries the run's retention tag (keep only for a completed passing run), read back from the bucket |
| I8 | delivery-path independence: for the same records, every non-aborted trigger that returned Ok reaches the same phase, is_error, prefilter_disagreement and keep |

### The sweep at HEAD (`failure_surface_sweep_known_defects_only`)

The cross product of the six axes is 4800 cells (5 deliveries x 10 outcomes x 2 gate reports x 4 pack records x 3
listings x 4 listed-but-absent objects), plus 10 ABORTED cells on the Event path. It runs in about 3 s. Result at
HEAD:

| Invariant | Cells violating | Where | First offending cell |
|-----------|-----------------|-------|----------------------|
| I1 | 0 | | |
| I2 | 2500 | every delivery, 500 record groups: exactly the listed-but-absent shapes | `sm_clean, outcome=absent, absent_listed=outcome`: `reading the tag of .../outcome.json: NoSuchKey`, and the next sweep fails the same way |
| I3 | 315 | `sm_caught` only | `sm_caught, pass, gate ok, pack accepted`: failed, not completed |
| I4 | 1560 | `sm_caught` 960 (the Cause), each other delivery 150 (the refused and pack lines) | `sm_caught, outcome=absent`: the payload and verdict.json carry the account id, the ARN and the ECR host |
| I5 | 0 | | |
| I6 | 0 | | |
| I7 | 12 | the four non-Catch deliveries of the three kept-run shapes | `sm_clean, pass, accepted, complete`: training-manifest.json and pack-manifest.json read back `expire` on a kept run |
| I8 | 56 | `sm_caught` against the other four | `sm_caught, pass, accepted`: failed / is_error true / keep false where the others complete and keep |

The violated set is exactly `KNOWN_DEFECTS = [I2, I3, I4, I7, I8]`: a shrink-only ratchet. The sweep fails when a pinned
entry is no longer violated (delete it) and when anything outside it is (a regression). The tracer
`failure_surface_tracer_wr01` pins the WR-01 cell (Catch delivery of a trainer refusal the pre-filter accepted): I1 and
I2 hold, I3 is violated because `finalize_run` returns on the stage error before `outcome.json` is read
(`prefilter_disagreement` false, the error is the Cause text). `failure_surface_transient_faults` fails each of the 50 S3
calls of a passing run once, with hostile error text, and requires the next sweep to heal to the uninjected end state.

| Known defect | Finding | Closed by |
|--------------|---------|-----------|
| I2 | WR-02 | 14-17 |
| I3 | WR-01 | 14-17 |
| I4 | WR-03 | 14-18 |
| I7 | IN-01 | 14-17 |
| I8 | WR-01 | 14-17 |

## Derivation (commands run, 2026-10-30, HEAD `327d9cdb2`)

Every fact below was produced by running the command, not recalled from the plan. Where the plan's seed differed, the
derived result is used and the correction is listed afterwards.

| Id | What | Command | Result |
|----|------|---------|--------|
| D-a | state-machine edges | `grep -n "stateName:\\|addCatch\\|\.next(\\|MACHINE_TIMEOUT_SECS =\\|status: \[\\|Schedule.rate" deploy-extensions/lib/laya-trainer-stack.ts; and the synthesized ASL via laya_trainer_stack_check.asl_of` | states Train, PackVerify, Finalize, CatchFinalize; Train and PackVerify each Catch States.ALL -> CatchFinalize ResultPath $.error; Finalize and CatchFinalize: no Catch, the CDK Lambda service-exception Retry only; TimeoutSeconds 36000; status rule FAILED/TIMED_OUT/ABORTED; reconcile rate(10 minutes). The gitignored cdk.out template (synthesized 2026-10-29) agrees |
| D-b | exit-class map | `python3 -I -c 'importlib-load scripts/laya_train/container/run_step.py; {c: job_exit_code(c) for c in EXIT_CLASSES}'` | {"pass": 0, "gate_fail": 0, "refused": 1, "crash": 1, "timeout": 1}; Run.finish: code = 1 if any upload failed and code == 0; finish_after_trainer: a CPU fallback is exit_class crash |
| D-c | pack exit paths | `sed -n '/^def run_pack/,/^# ---.*selftest/p' scripts/laya_train/container/run_step.py (run_pack, decide_skip, publish_pack, entry_pack)` | skip recorded exit 0 if published; accepted or refused recorded exit 0 if published and no upload failed; PackError -> 'ERROR <kind>' exit 1 with NO pack-outcome.json; env / contracts refusal exit 2 before any download; publish_pack lists pack-outcome.json before uploading it |
| D-d | run-object matrix | `read finalizer.rs read_json and read_run_objects` | absent, malformed and oversize (1 MiB) all read as missing; outcome, pack-outcome and both training manifests are dropped when they name another task; gate-report and pack-manifest carry no task check; any other read failure is a Store error (transient) |
| D-e | ordered returns of finalize_run | `grep -n 'return b\.\\|b\.decide(\\|decide_packed(&b' crates/aprender-mcp-laya-train/src/finalize.rs` | aborted; merge conflict; stage_error (BEFORE the outcome); outcome missing; Refused (inputs-changed / other); Crash; Timeout; device_ok false; gate report missing or lacking a field; pass/gate_fail vs report integrity; pass -> decide_packed; gate_fail -> gate_failed or decide_packed with the claim |
| D-f | publication steps | `read finalizer.rs finalize_task, settle_record and publish ((3)..(6) comments)` | owner check; record or TTL; ABORTED cancels the record; compute gate; read objects; decide; settle the record (arbiter); verdict.json; manifest.json (adopt or write, list for unlisted); tags = manifest entries + manifest.json, each read back; lock release LAST |
| D-g | sinks | `python3 -I scripts/laya_deploy/laya_failure_surface.py --check (scan(): SINK_PATTERNS over the text before the first #[cfg(test)] of every .rs under sink_scope)` | 77 hits in 22 (file, pattern) groups, each classified (sinks) |
| D-h | finalizer asset provenance | `grep -n 'FINALIZER_ASSET\\|finalizerAssetPath' scripts/laya_deploy/laya_trainer_ops.py deploy-extensions/bin/app.ts` | cmd_infra and app.ts check only that deploy-extensions/assets/laya-finalizer/bootstrap exists |
| D-i | tests that do not model the always-catching machine | `scan the test modules of finalize.rs and finalizer.rs for catch-fed classes built with stage_error None` | finalize.rs: finalize_refused, finalize_inputs_changed, finalize_crash, finalize_timeout, finalize_cpu, finalize_partial_manifest; finalizer.rs: fin_partial_inventory, fin_timeout_event, fin_reconcile_lost_event, fin_first_writer_wins; plus fin_catch_error (a Cause, no outcome) |

Exit classes (D-b), by running `job_exit_code` for every class of `EXIT_CLASSES`:

| Class | Train job exit | Delivered by |
|-------|----------------|--------------|
| pass | 0 | Finalize (no error) |
| gate_fail | 0 | Finalize (no error) |
| refused | 1 | CatchFinalize only (stage error populated) |
| crash | 1 | CatchFinalize only (stage error populated) |
| timeout | 1 | CatchFinalize only (stage error populated) |

A failed upload forces exit 1 for any class, and a CPU fallback is written as `crash`, so both also arrive through the
Catch.

State machine (D-a): `Train -> PackVerify -> Finalize -> CatchFinalize`, start `Train`; `Train` and `PackVerify` each `Catch States.ALL -> CatchFinalize`,
`ResultPath $.error`; `Finalize` and `CatchFinalize` have no Catch (only the CDK Lambda service-exception Retry, which
does not retry a finalizer `Err`); machine `TimeoutSeconds 36000`; status rule `FAILED/TIMED_OUT/ABORTED`; reconcile `rate(10 minutes)`.

### Corrections to the plan's seed

- **FS-UT is wider than seeded.** Besides `finalize_refused`, `finalize_inputs_changed`, `finalize_crash`,
  `finalize_timeout` and `finalize_cpu`, `finalize_partial_manifest` (timeout) also hands `finalize_run` a stage error
  of None; in `finalizer.rs`, `fin_partial_inventory` delivers a timeout through the state-machine payload WITHOUT an
  error (a delivery the machine never makes), and `fin_timeout_event`, `fin_reconcile_lost_event` and
  `fin_first_writer_wins` deliver catch-fed classes through the event and reconcile paths. `finalize_cpu` builds
  `exit_class pass` with `device_ok false`, but `run_step.py` writes a CPU fallback as `crash`.
- **The pack exit map has two Catch-fed shapes, not one.** A `PackError` exits 1 with no `pack-outcome.json`, and an
  `entry_pack` env or contracts refusal exits 2, also before any record (FS-PK-04, FS-PK-05).
- **A sink outside the seed:** the run bucket is named `aprender-laya-train-<env>-<account>`, so the account id reaches
  every caller in `run_prefix`, every manifest link and the dataset refusals by construction (SK-12). It is recorded as
  a handle, not a leak (the caller names that bucket in `dataset_uri`), and the planted I4 constants deliberately
  exclude it; 14-18 must state the exclusion in its I4 definition.
- **Four rows have no covering test at HEAD** (FS-DP-11, SK-03, SK-04, SK-05). They are recorded as `untested at HEAD`
  rather than bound to an unrelated test name; the checker's OK line counts them.

## The rows

Status `ok` is correct at HEAD; `defect` names its finding and the plan that closes it. The test is the one that
covers the row (the checker proves it exists).

### State-machine edges (FS-SM)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-SM-01 | Train succeeds (SageMaker job Completed; run_step exit 0 for pass and gate_fail) -> PackVerify | ok |  |  | `laya_trainer_stack_check.py::check_machine` |  |
| FS-SM-02 | PackVerify succeeds (pack exit 0: skipped, accepted or refused recorded) -> Finalize, payload without error | ok |  |  | `finalizer.rs::tracer_completed` | I3, I8 |
| FS-SM-03 | Train fails (job exit 1: refused, crash, timeout, cpu fallback, or any upload failure) -> Catch States.ALL -> CatchFinalize, resultPath $.error: the finalizer gets a stage error for EVERY catch-fed class | defect | WR-01 | 14-17 | `finalizer.rs::failure_surface_tracer_wr01` | I3, I4, I8 |
| FS-SM-04 | PackVerify fails (PackError exit 1 with no pack-outcome.json, env/contracts refusal exit 2, an upload failure exit 1, the 3 h task timeout) -> Catch States.ALL -> CatchFinalize, resultPath $.error | defect | WR-01 | 14-17 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I3, I8 |
| FS-SM-05 | Finalize has no Catch (only the CDK Lambda service-exception Retry, 6 attempts): a finalizer Err ends the execution FAILED, which falls to the status rule and the 10-minute reconcile (both deliver stage_error None) | ok |  |  | `finalizer.rs::fin_reconcile_lost_event` | I2, I8 |
| FS-SM-06 | CatchFinalize has no Catch (same Retry): its Err ends the execution FAILED; the status event and the reconcile then re-drive it WITHOUT the stage error | ok |  |  | `finalizer.rs::fin_reconcile_lost_event` | I2, I8 |
| FS-SM-07 | machine TimeoutSeconds 36000 (MACHINE_TIMEOUT_SECS) -> TIMED_OUT -> status rule (and reconcile) | ok |  |  | `finalizer.rs::fin_timeout_event` | I1, I8 |
| FS-SM-08 | a client cancel -> StopExecution (cancel sink) -> ABORTED -> status rule; the finalizer cancels the record and finalizes cancelled | ok |  |  | `finalizer.rs::fin_abort_event` | I6 |
| FS-SM-09 | a lost status-rule delivery (best effort, retryAttempts 4, DLQ) -> the 10-minute reconcile sweep is the floor | ok |  |  | `finalizer.rs::fin_reconcile_lost_event` | I2 |
| FS-SM-10 | StartExecution rejected or ambiguous at dispatch (request side): rejected compensates and releases; ambiguous + unconfirmed leaves the lock for the reconcile's dispatch-unconfirmed path | ok |  |  | `lib.rs::submit_dispatch_unconfirmed` | I2 |

### Exit classes (FS-EX)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-EX-01 | pass -> train job exit 0 -> Finalize path | ok |  |  | `run_step.py::selftest` |  |
| FS-EX-02 | gate_fail -> train job exit 0 (the pack stage runs on a gate_fail) | ok |  |  | `run_step.py::selftest` |  |
| FS-EX-03 | refused -> train job exit 1 -> CatchFinalize only | defect | WR-01 | 14-17 | `finalizer.rs::failure_surface_tracer_wr01` | I3, I8 |
| FS-EX-04 | crash -> train job exit 1 -> CatchFinalize only | defect | WR-01 | 14-17 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I3, I8 |
| FS-EX-05 | timeout -> train job exit 1 -> CatchFinalize only | defect | WR-01 | 14-17 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I3, I8 |
| FS-EX-06 | a failed upload forces exit 1 for ANY class (Run.finish: code = 1 if failed and code == 0), so pass and gate_fail can arrive through the Catch too, with outcome.json possibly listed but absent | defect | WR-02 | 14-17 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I2, I3 |
| FS-EX-07 | a CPU fallback is classified crash with device_ok false (finish_after_trainer), so it is catch-fed | defect | WR-01 | 14-17 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I3 |

### Pack exit paths (FS-PK)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-PK-01 | decide_skip: a non-pass/gate_fail outcome, device_ok false, or a gate_fail without a waiver claim -> pack-outcome skipped, exit 0 when published | ok |  |  | `run_step.py::selftest` |  |
| FS-PK-02 | pack and verify accept -> .apr uploaded, accepted pack-outcome, exit 0 | ok |  |  | `finalizer.rs::tracer_completed` | I5, I7 |
| FS-PK-03 | pack or verify refuse (rc 3) -> accepted false recorded, exit 0 (PackVerify succeeds) | ok |  |  | `finalizer.rs::fin_pack_refused` | I3, I5 |
| FS-PK-04 | PackError (outcome, manifest, download, integrity, base, child) -> exit 1 with NO pack-outcome.json -> PackVerify Catch: stage error plus a missing pack record | defect | WR-01 | 14-17 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I3, I8 |
| FS-PK-05 | entry_pack env or contracts refusal -> exit 2 before any download, no pack-outcome.json -> PackVerify Catch | defect | WR-01 | 14-17 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I3 |
| FS-PK-06 | publish_pack lists pack-outcome.json in pack-manifest.json BEFORE uploading it; a failed last upload leaves it listed but absent (exit 1) | defect | WR-02 | 14-17 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I2 |

### Run objects (FS-RO)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-RO-01 | outcome.json absent -> read as missing -> failed 'outcome.json is missing' (or the stage error) | ok |  |  | `finalize.rs::finalize_no_outcome` | I3 |
| FS-RO-02 | outcome.json malformed or over the 1 MiB cap -> warn log, read as missing | ok |  |  | `finalizer.rs::failure_surface_sweep_known_defects_only` | I3 |
| FS-RO-03 | outcome.json names another task -> warn log, read as missing | ok |  |  | `finalizer.rs::failure_surface_sweep_known_defects_only` | I3 |
| FS-RO-04 | run/gate-report.json absent, malformed or oversize -> missing (it carries no task id, so no foreign check) | ok |  |  | `finalizer.rs::failure_surface_sweep_known_defects_only` | I3 |
| FS-RO-05 | pack-outcome.json absent, malformed, oversize or foreign -> missing | ok |  |  | `finalizer.rs::failure_surface_sweep_known_defects_only` | I3 |
| FS-RO-06 | training-manifest.json absent or foreign -> partial-manifest.json is read; both absent -> no training entries | ok |  |  | `finalizer.rs::fin_partial_inventory` | I3, I7 |
| FS-RO-07 | pack-manifest.json absent or malformed -> empty (no task-id check: it is a bare array) | ok |  |  | `finalizer.rs::failure_surface_sweep_known_defects_only` | I3 |
| FS-RO-08 | any read failing for another reason (5xx, denied, a broken body) -> Store error, nothing settled or the publish resumes; the lock is held and the next pass heals | ok |  |  | `finalizer.rs::failure_surface_transient_faults` | I2 |
| FS-RO-09 | a manifest entry listed but absent in S3 (FS-EX-06, FS-PK-06): read_run_objects keeps the entry | defect | WR-02 | 14-17 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I2 |

### Decision precedence of finalize_run (FS-DP)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-DP-01 | aborted -> cancelled, entries minus the .apr | ok |  |  | `finalizer.rs::fin_abort_event` | I5, I6 |
| FS-DP-02 | a path listed by two writers with different content -> failed integrity | ok |  |  | `finalize.rs::finalize_integrity` | I3 |
| FS-DP-03 | stage_error Some -> failed with the stage text, returned BEFORE outcome.json is read | defect | WR-01 | 14-17 | `finalizer.rs::failure_surface_tracer_wr01` | I3, I4, I8 |
| FS-DP-04 | no outcome -> failed 'outcome.json is missing' | ok |  |  | `finalize.rs::finalize_no_outcome` | I3 |
| FS-DP-05 | refused inputs-changed -> failed, prefilter_disagreement false | ok |  |  | `finalize.rs::finalize_inputs_changed` | I3 |
| FS-DP-06 | any other refusal -> failed, prefilter_disagreement TRUE, the REFUSED line (D-08); reachable only with stage_error None | defect | WR-01 | 14-17 | `finalizer.rs::failure_surface_tracer_wr01` | I3, I8 |
| FS-DP-07 | crash -> failed 'the trainer crashed' | ok |  |  | `finalize.rs::finalize_crash` | I3 |
| FS-DP-08 | timeout -> failed 'runtime cap' | ok |  |  | `finalize.rs::finalize_timeout` | I3 |
| FS-DP-09 | device_ok false (pass or gate_fail) -> failed | ok |  |  | `finalize.rs::finalize_cpu` | I3 |
| FS-DP-10 | gate-report.json missing, or lacking a required field -> failed | ok |  |  | `finalize.rs::finalize_malformed_report_is_refused_not_defaulted` | I3 |
| FS-DP-11 | outcome and gate report disagree on pass -> failed integrity | ok |  |  | none (untested at HEAD: no test builds an outcome that contradicts its gate report, and the sweep's gate report is always consistent with the outcome) | I3 |
| FS-DP-12 | pass -> decide_packed: pack record missing or refused -> failed (D-15); waiver mismatch or apr sha not listed -> integrity; accepted -> completed, keep | ok |  |  | `finalizer.rs::fin_pack_refused` | I3, I5, I7 |
| FS-DP-13 | gate_fail without a waiver claim -> gate_failed (non-error, no pack); with a claim -> decide_packed waived | ok |  |  | `finalizer.rs::fin_waived` | I3, I5 |

### Publication (FS-PB)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-PB-01 | compute gate: a training job or pack task still billing -> stop it, awaiting_compute, nothing written, lock held | ok |  |  | `finalizer.rs::fin_awaiting_training` | I2 |
| FS-PB-02 | the record settles first (first writer wins; Cancelled gets the aborted payload attached; terminal-without-result is integrity) | ok |  |  | `finalizer.rs::fin_first_writer_wins` | I1 |
| FS-PB-03 | verdict.json = the winning payload's text, written only when different | ok |  |  | `finalizer.rs::fin_crash_resume_after_record` | I4 |
| FS-PB-04 | manifest.json: adopted when it already lists this verdict, else written from the decision's entries plus the unlisted count | ok |  |  | `finalizer.rs::fin_crash_resume_after_verdict` | I5 |
| FS-PB-05 | tag targets = every manifest.json entry plus manifest.json: a listed-but-absent key fails GetObjectTagging (NoSuchKey) and wedges the lock | defect | WR-02 | 14-17 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I2 |
| FS-PB-06 | the self-listing inventories (training-, partial-, pack-manifest.json) are never tag targets: a kept run's inventories keep 'expire' | defect | IN-01 | 14-17 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I7 |
| FS-PB-07 | every tag read back; the lock released LAST and re-read; a failure at any step returns Err with the lock held | ok |  |  | `finalizer.rs::fin_crash_resume_after_readback` | I2, I7 |
| FS-PB-08 | record missing past the task TTL -> an existing verdict.json is the decision, else the fresh one; inside the TTL -> integrity, nothing written | ok |  |  | `finalizer.rs::fin_record_expired` | I1 |

### Caller-visible text sinks (SK)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| SK-01 | sfn.rs SfnExecutions::start renders the SDK error (DisplayErrorContext) into StartError.message -> DispatchError -> lib.rs `REFUSED dispatch: {reason}` and the compensated payload's error | defect | WR-03 | 14-18 | `lib.rs::submit_dispatch_rejected` | I4 |
| SK-02 | finalizer.rs bounded_error forwards the Catch `Error: Cause` (a DescribeTrainingJob JSON naming the ECR image and role ARN) as stage_error -> finalize.rs payload.error -> the stored payload, verdict.json, train_status | defect | WR-03 | 14-18 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I4 |
| SK-03 | lib.rs submit: the mint, lock-backend and envelope error texts reach pmcp Error::internal and the compensated payload | defect | WR-03 | 14-18 | none (untested at HEAD: no test exercises these error paths; 14-18 adds the request-side hostile-backend test) | I4 |
| SK-04 | lib.rs status: `no task {task_id} on this server: {e}` carries the store's error text to the caller | defect | WR-03 | 14-18 | none (untested at HEAD: no test exercises this error path; 14-18 adds it) | I4 |
| SK-05 | the sibling DynamoDbTaskBackend (aprender-setfit-train-lambda dynamodb.rs `aws`) renders DisplayErrorContext into BackendError: the text SK-03 and SK-04 forward | defect | WR-03 | 14-18 | none (untested at HEAD: no test exercises the renderer's text; 14-18 wraps the backend (RedactingBackend)) | I4 |
| SK-06 | job-written lines reach the caller verbatim: outcome.refused_line in the error, pack verify_line / packed_line in verdict.pack.line and the pack-refused error | defect | WR-03 | 14-18 | `finalizer.rs::failure_surface_sweep_known_defects_only` | I4 |
| SK-07 | finalizer-side renderers (objects.rs S3, compute.rs SageMaker / ECS / Step Functions) -> FinalizerError -> the Lambda error and the execution history only, never a stored payload | ok |  |  | `finalizer.rs::failure_surface_transient_faults` | I4 |
| SK-08 | dataset.rs renderers -> tracing::warn only; the refusal names the bucket and prefix, never the SDK text | ok |  |  | `dataset.rs::dataset_denied_is_a_typed_refusal_naming_bucket_and_prefix` |  |
| SK-09 | sfn.rs describe (dispatch confirm) and stop (cancel sink) renderers -> eprintln / tracing::error only | ok |  |  | `sfn.rs::cancel_sink_logs_a_failing_stop_and_returns` |  |
| SK-10 | log calls (tracing, eprintln, println) in the two crates: server-side log only | ok |  |  | `sfn.rs::cancel_sink_logs_a_failing_stop_and_returns` |  |
| SK-11 | static caller texts (validation refusals, single-flight, failed_payload reasons with no infrastructure text, orphan dispatch-unconfirmed, stage_error None) | ok |  |  | `lib.rs::submit_single_flight_refuses_naming_the_holder_and_mints_nothing` |  |
| SK-12 | the run bucket is named aprender-laya-train-<env>-<account> (laya-trainer-stack.ts), so StatusPayload.run_prefix, every manifest link and the dataset refusals carry the account id BY CONSTRUCTION (classified a handle, not a leak: the caller names this bucket in dataset_uri; the planted I4 constants deliberately exclude it. 14-18 must state the exclusion in I4 (its redactor would otherwise break the s3:// handle)) | ok |  |  | `finalizer.rs::tracer_completed` | I4 |

### Asset provenance (FS-AS)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-AS-01 | the finalizer Lambda asset (deploy-extensions/assets/laya-finalizer/bootstrap) is deployed with an existence check only: no commit, clean-tree or sha256 provenance (the ops selftest covers only the ABSENT-asset refusal of cmd_infra; nothing ties the binary to HEAD) | defect | WR-04 | 14-19 | `laya_trainer_ops.py::selftest` |  |

### Tests that do not model the always-catching machine (FS-UT)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-UT-01 | finalize.rs tests hand finalize_run stage_error None for catch-fed classes (refused, inputs-changed, crash, timeout, cpu, partial-manifest timeout): a delivery the machine never makes for those classes | defect | WR-01 | 14-17 | `finalize.rs::finalize_refused` | I3 |
| FS-UT-02 | finalizer.rs tests deliver catch-fed classes with stage_error None (payload(None), status events, reconcile), and fin_catch_error carries a Cause but no outcome | defect | WR-01 | 14-17 | `finalizer.rs::fin_catch_error` | I3 |

## Sink scan (D-g)

Scanned: the non-test text (before the first `#[cfg(test)]`) of every `.rs` under `crates/aprender-mcp-laya-train/src`, `crates/aprender-mcp-laya-train-lambda/src`, plus `crates/aprender-setfit-train-lambda/src/dynamodb.rs`, `crates/aprender-mcp-setfit-train/src/task_store.rs`. A new hit changes a
count, or appears in an unclassified (file, pattern) group, and the checker refuses it.

| File | Pattern | Hits | Classes | Rows |
|------|---------|------|---------|------|
| `aprender-mcp-laya-train-lambda/src/bin/aprender-laya-train-finalizer.rs` | log | 4 | log-only 4 | SK-10 |
| `aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs` | log | 2 | log-only 2 | SK-10 |
| `aprender-mcp-laya-train-lambda/src/compute.rs` | aws-renderer | 8 | log-only 8 | SK-07 |
| `aprender-mcp-laya-train-lambda/src/dataset.rs` | aws-renderer | 4 | log-only 4 | SK-08 |
| `aprender-mcp-laya-train-lambda/src/dataset.rs` | log | 2 | log-only 2 | SK-10 |
| `aprender-mcp-laya-train-lambda/src/finalizer.rs` | log | 5 | log-only 5 | SK-10 |
| `aprender-mcp-laya-train-lambda/src/finalizer.rs` | payload-error-assign | 1 | static 1 | SK-11 |
| `aprender-mcp-laya-train-lambda/src/finalizer.rs` | stage-error | 7 | caller 3, static 4 | SK-02, SK-11 |
| `aprender-mcp-laya-train-lambda/src/objects.rs` | aws-renderer | 5 | log-only 5 | SK-07 |
| `aprender-mcp-laya-train-lambda/src/sfn.rs` | aws-renderer | 3 | caller 1, log-only 2 | SK-01, SK-09 |
| `aprender-mcp-laya-train-lambda/src/sfn.rs` | dispatch-error | 4 | caller 2, static 2 | SK-01, SK-11 |
| `aprender-mcp-laya-train-lambda/src/sfn.rs` | log | 6 | log-only 6 | SK-10 |
| `aprender-mcp-laya-train-lambda/src/sfn.rs` | start-error | 2 | caller 1, static 1 | SK-01 |
| `aprender-mcp-laya-train/src/finalize.rs` | payload-error-assign | 1 | caller 1 | SK-02, SK-06 |
| `aprender-mcp-laya-train/src/finalize.rs` | stage-error | 2 | caller 1, static 1 | SK-02 |
| `aprender-mcp-laya-train/src/lib.rs` | dispatch-error | 3 | caller 2, static 1 | SK-01, SK-11 |
| `aprender-mcp-laya-train/src/lib.rs` | failed-payload | 7 | caller 3, static 4 | SK-01, SK-03, SK-11 |
| `aprender-mcp-laya-train/src/lib.rs` | log | 1 | log-only 1 | SK-10 |
| `aprender-mcp-laya-train/src/lib.rs` | payload-error-assign | 1 | caller 1 | SK-01, SK-03 |
| `aprender-mcp-laya-train/src/lib.rs` | pmcp-internal | 4 | caller 3, static 1 | SK-03, SK-11 |
| `aprender-mcp-laya-train/src/lib.rs` | pmcp-validation | 4 | caller 2, static 2 | SK-01, SK-04, SK-11 |
| `aprender-setfit-train-lambda/src/dynamodb.rs` | aws-renderer | 1 | caller 1 | SK-05 |

Per-hit classification at HEAD (line numbers drift; the counts above are what the checker binds):

- `sfn.rs` aws-renderer: 143 `StartError.message` caller (SK-01); 162 describe and 173 stop log-only (SK-09).
- `sfn.rs` dispatch-error: 308 and 315 static texts; 321 and 322 forward `e.message` (caller). start-error: 37 the type,
  141 the construction that carries the SDK text (caller).
- `lib.rs` pmcp-validation: 421 `refuse` and 425 single-flight static; 594 `REFUSED dispatch: {reason}` and 648
  `no task ...: {e}` caller. pmcp-internal: 497 mint, 526 lock backend, 554 envelope caller; 635 a serde error static.
  failed-payload: 444 the definition, 512, 688, 690 static; 523, 551, 591 caller. payload-error-assign 446 caller.
  dispatch-error: 237 the type; 571 and 572 forward the reason (caller). log 578 `eprintln` log-only.
- `finalizer.rs` stage-error: 691 (two hits) and 1072 caller (the Catch text into the decision); 603, 1102, 1217, 1264
  static (the field, and the three `None` deliveries). payload-error-assign 648 static (`dispatch-unconfirmed`).
- `finalize.rs` payload-error-assign 291 caller (every failed decision's text, including the stage error and the job
  lines, SK-02, SK-06); stage-error 60 the field, 430 the early return (caller, WR-01).
- `dynamodb.rs` aws-renderer 121 caller (SK-05: the backend text SK-03 and SK-04 forward).
- `objects.rs`, `compute.rs`, `dataset.rs` renderers and every log call: log-only (SK-07, SK-08, SK-09, SK-10).

## Tests that do not model the always-catching machine (FS-UT, D-i)

These tests hand the decision a stage error of None for classes the machine delivers ONLY through its Catch (or a
Cause with no outcome). They are not wrong about what they test, but none of them models the delivery the machine
really makes. Plan 14-17 remodels them.

| Test | Delivery | Why it does not model the machine |
|------|----------|-----------------------------------|
| `finalize.rs::finalize_refused` | finalize_run direct, stage_error None | refused exits 1: the machine always supplies a stage error |
| `finalize.rs::finalize_inputs_changed` | finalize_run direct, stage_error None | refused exits 1 |
| `finalize.rs::finalize_crash` | finalize_run direct, stage_error None | crash exits 1 |
| `finalize.rs::finalize_timeout` | finalize_run direct, stage_error None | timeout exits 1 |
| `finalize.rs::finalize_cpu` | finalize_run direct, stage_error None | it builds exit_class pass with device_ok false; run_step.py writes a CPU fallback as crash (exit 1) |
| `finalize.rs::finalize_partial_manifest` | finalize_run direct, stage_error None | timeout exits 1 (correction: not in the plan's seed) |
| `finalizer.rs::fin_partial_inventory` | state-machine payload WITHOUT error | a timeout reaches Finalize only through CatchFinalize (correction: not in the plan's seed) |
| `finalizer.rs::fin_timeout_event` | TIMED_OUT status event | a trainer timeout fails Train and is caught; the event re-drives it only after CatchFinalize itself failed |
| `finalizer.rs::fin_reconcile_lost_event` | reconcile sweep | crash is caught; the sweep sees it only after CatchFinalize failed |
| `finalizer.rs::fin_first_writer_wins` | late FAILED status event | tests first-writer-wins; its crash class is incidental |
| `finalizer.rs::fin_catch_error` | state-machine payload WITH a Cause | a Cause but no outcome.json: never a parsed outcome against a stage error |

## Dispositions

- WR-01 (FS-SM-03, FS-SM-04, FS-EX-03..05, FS-EX-07, FS-PK-04, FS-PK-05, FS-DP-03, FS-DP-06, FS-UT-01, FS-UT-02;
  invariants I3, I8): plan 14-17 makes a parsed `outcome.json` decide the class, with the stage error as detail.
- WR-02 (FS-EX-06, FS-PK-06, FS-RO-09, FS-PB-05; I2) and IN-01 (FS-PB-06; I7): plan 14-17 drops listed-but-absent
  entries when the objects are read and tags every self-listing inventory.
- WR-03 (SK-01..SK-06; I4): plan 14-18 redacts at the one serializer, wraps the backend and labels the dispatch error.
- WR-04 (FS-AS-01): plan 14-19 adds the asset provenance sidecar and binds the state-machine rows to the synthesized
  template.

## Out of class (kept visible, not dropped)

| Item | Why it is not in this class or this round |
|------|-------------------------------------------|
| D-16 (verification truth 12) | an owner decision: a second reproduction is paid compute the owner recorded as not authorised, and a waiver is an owner-written override |
| the packing-cancel live check (truth 8) | about 62 min of paid compute; an owner-accepted limitation |
| IN-02 (image rebuild after a partial build) | a build-tooling matter outside the finalizer and state-machine boundary |
| IN-03 (pre-filter vs trainer gate-contract skew) | needs an owner decision on what a skew should do to a phase, so the required information is missing |

