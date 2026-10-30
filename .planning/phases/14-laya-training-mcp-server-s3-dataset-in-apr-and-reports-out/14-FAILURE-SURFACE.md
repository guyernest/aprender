---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 16
artifact: failure-surface
schema: laya-failure-surface-v1
json: crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json
checker: scripts/laya_deploy/laya_failure_surface.py
derived_at_head: 327d9cdb2
derived_on: 2026-10-30
updated_by: 14-18
updated_at_head: 1a0606a98
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

**Plan 14-17 (2026-10-30)** then changed production code to close WR-01, WR-02 and IN-01 against this surface:
`finalize.rs` (outcome-first classification, the stage error as detail, the `notes` it writes), `layout.rs`
(`StatusPayload.notes`, absent from the JSON when empty) and `finalizer.rs` (the listed-but-absent drop and the tag
targets). The rows it closed have status `closed`, their fix and the test that now covers them. Its nine source mutations
(`--mutations`, M-WR01-a..d and M-WR02-a..e) each turn a named test or invariant red. Only I4 (WR-03, 14-18) remains in the
ratchet.

**Plan 14-18 (2026-10-30)** closed WR-03, the sanitization half of the class, at the boundaries where text becomes
caller-visible rather than at each site that builds an error: `StatusPayload::to_value` (the one serializer: `error`,
`verdict.pack.line` and every `notes` entry pass `redact::caller_safe`), `RedactingBackend` (the one wrapper around the
DynamoDB task backend, so the lock, `train`, `train_status` and pmcp's task handlers never see the SDK text) and the
dispatch label (`start_error_label`: an AWS error code, never the SDK message). The Catch's Cause is REDUCED by
`redact::stage_error` (an identifier name and a whitelist of reason keys), never forwarded. Eight source mutations
(M-WR03-a..h) each turn a named test or I4 red. The ratchet is empty: `KNOWN_DEFECTS = []`, and the sweep is
`failure_surface_sweep_clean`. **SK-12 decided:** the bucket-name account id in `run_prefix` and the manifest links is a
caller handle (the caller names that bucket in `dataset_uri`), so only the free-text fields are redacted and I4 excludes it
by design.

## The class invariant

Eight numbered invariants. The sweep (`finalizer.rs` tests, section `failure_surface`) computes every one per cell with
an oracle that never calls the implementation's own rules, and reads locks and tags from the records, never a log line.

| Id | Invariant |
|----|-----------|
| I1 | terminal and classified: after an Ok finalization the task record is terminal and holds a stored result whose phase is completed, gate_failed, failed or cancelled |
| I2 | finalization total and lock released: with no injected fault the first pass returns Ok and releases the single-flight lock, and a later reconcile sweep returns Ok; a held lock is allowed only for compute still billing or one injected transient S3 or backend fault the next pass clears; and an object a writer manifest lists but S3 does not hold is listed by neither the stored payload nor manifest.json, and the stored payload names it in exactly one listed-but-absent note (no such note when nothing listed is absent) |
| I3 | class decided from the records: when outcome.json parses and names the task, the phase follows the class table whatever the delivery path (refused_other failed with prefilter_disagreement true and the REFUSED line; inputs-changed failed with the flag false; crash and timeout failed with their class text; cpu fallback failed; pass and gate_fail follow the gate report and the pack record); an absent, malformed, foreign or removed outcome is failed with a non-empty error; a completed or gate_failed run delivered through the Catch (a stage error the records overrode) carries the stage-error-overridden note in the stored payload, and no other delivery of the same records and no failed phase does |
| I4 | caller-safe text: the stored payload, verdict.json and manifest.json carry none of the planted hostile constants (an account-id-shaped digit string, an ARN and an ECR host containing it, and a value no redaction rule knows planted under a Catch Cause key outside stage_error's whitelist, which only the reduction drops) planted in the Catch Cause, the trainer's refused line, the pack verify and packed lines and an injected S3 error. The account id of the run bucket's name in run_prefix and the manifest links is a caller handle (SK-12) and is excluded by design. Closed by 14-18 at three boundaries: StatusPayload::to_value, RedactingBackend and the dispatch code label; the request side is proven by the hostile-backend and hostile-dispatcher tests |
| I5 | no .apr unless completed: neither the stored payload's manifest nor manifest.json lists an .apr for any phase but completed |
| I6 | aborted means cancelled: an ABORTED execution yields a cancelled record and a cancelled stored payload |
| I7 | retention tags: every key the finalizer inventories or names (each manifest.json entry, manifest.json, and every self-listing manifest that exists) carries the run's retention tag (keep only for a completed passing run), read back from the bucket |
| I8 | delivery-path independence: for the same records, every non-aborted trigger that returned Ok reaches the same phase, is_error, prefilter_disagreement and keep |

### The sweep (`failure_surface_sweep_clean`)

The cross product of the six axes is 4800 cells (5 deliveries x 10 outcomes x 2 gate reports x 4 pack records x 3
listings x 4 listed-but-absent objects), plus 10 ABORTED cells on the Event path. It runs in about 3 s. Result after
plan 14-17 (HEAD `736eacdb6`): I1, I2, I3, I5, I6, I7 and I8 hold everywhere; I4 is violated in 1536 cells (`sm_caught`
936, each other delivery 150). Result after plan 14-18 (HEAD `1a0606a98`, the test renamed from
`failure_surface_sweep_known_defects_only`): **all eight invariants hold in every cell, zero violations**, with a fourth
planted constant in the Catch Cause. The table below is the 14-16 result at `327d9cdb2`, kept as the record of what was found:

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

At 14-16 the violated set was exactly `KNOWN_DEFECTS = [I2, I3, I4, I7, I8]`; after 14-17 it was `[I4]`; after 14-18
it is EMPTY, so every violation fails the sweep. With nothing pinned there is nothing left to detect, so the proof that
the sweep still sees a defect is the source mutations (`--mutations`: M-WR01, M-WR02 and M-WR03 turn I2, I3, I7, I8 or I4
red). The ratchet is shrink-only: the sweep fails when a pinned entry is no longer violated (delete it) and when anything outside it is (a
regression). The tracer `failure_surface_tracer_wr01` runs the WR-01 cell (Catch delivery of a trainer refusal the
pre-filter accepted). At 14-16, I3 was violated there because `finalize_run` returned on the stage error before it read
`outcome.json` (`prefilter_disagreement` false, the error the Cause text). Since 14-17 the cell has no violation in its
scope (I1..I3): the REFUSED line and the D-08 flag reach the caller. Since 14-18 the cell violates nothing at all.
`failure_surface_transient_faults` fails each S3 call of a passing run once, with hostile error text, and requires the
next sweep to heal to the uninjected end state. There were 50 calls at 14-16 and 58 since 14-17, which added the drop
listing and the post-write listing.

Two oracle clauses were added by 14-17. **I3:** the `stage-error-overridden` note appears exactly on a Catch delivery the
records decided `completed` or `gate_failed`. **I2:** an object the cell's manifests list but S3 does not hold is listed
by neither the stored payload nor manifest.json, and is named in exactly one `listed-but-absent` note. I8 compares phase,
is_error, prefilter_disagreement and keep, and leaves `notes` out on purpose: only the Catch delivers a stage error.

There is no known defect left (`known_defects: []`, `KNOWN_DEFECTS = []`).

Closed by 14-17 and deleted from the ratchet: I2 (WR-02), I3 (WR-01), I7 (IN-01) and I8 (WR-01). Closed by 14-18 and
deleted: I4 (WR-03).

A fourth planted constant was added by 14-18, `HOSTILE_UNLISTED`: a value no redaction rule knows (no ARN, host, URL,
account or key shape), under a Catch Cause key outside `stage_error`'s whitelist (`OutputDataConfig.KmsKeyId`). The
serializer's blacklist would pass it; only the reduction drops it. Without it, a Cause forwarded instead of reduced would
still read clean (the blacklist redacts every account shape), and the sweep could not see the reduction on its own
(T-14-86, M-WR03-c).

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
| D-j | the sweep after plan 14-17 (HEAD `736eacdb6`) | `python3 -I scripts/laya_deploy/laya_failure_surface.py --sweep`; the sweep test binary with `--nocapture` | 4800 cells + 10 aborted: I1, I2, I3, I5, I6, I7 and I8 hold; I4 violated in 1536 cells (sm_caught 936, each other delivery 150), the one remaining ratchet entry (14-18); 58 single-fault transient cases |
| D-k | the sweep, the redaction tables and the hostile fakes after plan 14-18 (HEAD `1a0606a98`) | `python3 -I scripts/laya_deploy/laya_failure_surface.py --sweep`; the sweep test binary with `--nocapture` | 4800 cells + 10 aborted: I1..I8 all hold, zero violations (`failure_surface_sweep_clean`); 58 single-fault transient cases; the redact, hostile_ and start_error_label runs green |
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

State machine (D-a; since 14-19 also bound both ways to the synthesized template by `laya_trainer_stack_check.py::check_surface_binding`): `Train -> PackVerify -> Finalize -> CatchFinalize`, start `Train`; `Train` and `PackVerify` each `Catch States.ALL -> CatchFinalize`,
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
  exclude it; 14-18 stated the exclusion in I4 and redacts only the free-text fields, never `run_prefix` or a link.
- **Four rows have no covering test at HEAD** (FS-DP-11, SK-03, SK-04, SK-05). They are recorded as `untested at HEAD`
  rather than bound to an unrelated test name; the checker's OK line counts them.

## The rows

Status `ok` is correct at HEAD; `defect` names its finding and the plan that closes it. The test is the one that
covers the row (the checker proves it exists).

### State-machine edges (FS-SM)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-SM-01 | Train succeeds (SageMaker job Completed; run_step exit 0 for pass and gate_fail) -> PackVerify (bound both ways to the synthesized template by 14-19 (check_surface_binding: states, Catch per state, no-catch states, StartAt and Next, TimeoutSeconds, the status rule and the reconcile rate, each refusing under its own surface-binding-<field> rule)) | ok |  |  | `laya_trainer_stack_check.py::check_surface_binding` |  |
| FS-SM-02 | PackVerify succeeds (pack exit 0: skipped, accepted or refused recorded) -> Finalize, payload without error | ok |  |  | `finalizer.rs::tracer_completed` | I3, I8 |
| FS-SM-03 | Train fails (job exit 1: refused, crash, timeout, cpu fallback, or any upload failure) -> Catch States.ALL -> CatchFinalize, resultPath $.error: the finalizer gets a stage error for EVERY catch-fed class; closed by 14-17: the Catch still supplies the stage error, but finalize_run reads outcome.json first: the class, the REFUSED line and the D-08 flag come from the records (I3 and I8 hold over the sweep) | closed | WR-01 | 14-17 | `finalizer.rs::failure_surface_tracer_wr01` | I3, I4, I8 |
| FS-SM-04 | PackVerify fails (PackError exit 1 with no pack-outcome.json, env/contracts refusal exit 2, an upload failure exit 1, the 3 h task timeout) -> Catch States.ALL -> CatchFinalize, resultPath $.error; closed by 14-17: a missing pack record under a stage error fails 'pack-outcome.json is missing; stage error: ...' and decides as the event and reconcile deliveries do | closed | WR-01 | 14-17 | `finalizer.rs::failure_surface_sweep_clean` | I3, I8 |
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
| FS-EX-03 | refused -> train job exit 1 -> CatchFinalize only; closed by 14-17: a caught refusal keeps prefilter_disagreement true and the trainer's REFUSED line | closed | WR-01 | 14-17 | `finalizer.rs::failure_surface_tracer_wr01` | I3, I8 |
| FS-EX-04 | crash -> train job exit 1 -> CatchFinalize only; closed by 14-17: crash keeps its class text on both deliveries; the stage error is appended as detail | closed | WR-01 | 14-17 | `finalize.rs::finalize_crash` | I3, I8 |
| FS-EX-05 | timeout -> train job exit 1 -> CatchFinalize only; closed by 14-17: timeout keeps 'runtime cap' on both deliveries; the stage error is appended as detail | closed | WR-01 | 14-17 | `finalize.rs::finalize_timeout` | I3, I8 |
| FS-EX-06 | a failed upload forces exit 1 for ANY class (Run.finish: code = 1 if failed and code == 0), so pass and gate_fail can arrive through the Catch too, with outcome.json possibly listed but absent; closed by 14-17: a pass or gate_fail that exited 1 only because an upload failed is decided by its records and carries the stage-error-overridden note; an outcome.json listed but absent is dropped (fin_listed_but_absent_outcome) | closed | WR-02 | 14-17 | `finalizer.rs::fin_override_note_reaches_verdict` | I2, I3 |
| FS-EX-07 | a CPU fallback is classified crash with device_ok false (finish_after_trainer), so it is catch-fed; closed by 14-17: the cpu_fallback cells fail on every delivery with the crash class | closed | WR-01 | 14-17 | `finalizer.rs::failure_surface_sweep_clean` | I3 |

### Pack exit paths (FS-PK)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-PK-01 | decide_skip: a non-pass/gate_fail outcome, device_ok false, or a gate_fail without a waiver claim -> pack-outcome skipped, exit 0 when published | ok |  |  | `run_step.py::selftest` |  |
| FS-PK-02 | pack and verify accept -> .apr uploaded, accepted pack-outcome, exit 0 | ok |  |  | `finalizer.rs::tracer_completed` | I5, I7 |
| FS-PK-03 | pack or verify refuse (rc 3) -> accepted false recorded, exit 0 (PackVerify succeeds) | ok |  |  | `finalizer.rs::fin_pack_refused` | I3, I5 |
| FS-PK-04 | PackError (outcome, manifest, download, integrity, base, child) -> exit 1 with NO pack-outcome.json -> PackVerify Catch: stage error plus a missing pack record; closed by 14-17: the pack-record-missing failure carries the stage error as detail | closed | WR-01 | 14-17 | `finalize.rs::finalize_pack_missing` | I3, I8 |
| FS-PK-05 | entry_pack env or contracts refusal -> exit 2 before any download, no pack-outcome.json -> PackVerify Catch; closed by 14-17: an env or contracts refusal leaves no pack record: failed on every delivery, the same class | closed | WR-01 | 14-17 | `finalizer.rs::failure_surface_sweep_clean` | I3 |
| FS-PK-06 | publish_pack lists pack-outcome.json in pack-manifest.json BEFORE uploading it; a failed last upload leaves it listed but absent (exit 1); closed by 14-17: pack-outcome.json listed but absent is dropped at read; the lock is released on the first pass | closed | WR-02 | 14-17 | `finalizer.rs::fin_listed_but_absent_pack_outcome` | I2 |

### Run objects (FS-RO)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-RO-01 | outcome.json absent -> read as missing -> failed 'outcome.json is missing' (or the stage error) | ok |  |  | `finalize.rs::finalize_no_outcome` | I3 |
| FS-RO-02 | outcome.json malformed or over the 1 MiB cap -> warn log, read as missing | ok |  |  | `finalizer.rs::failure_surface_sweep_clean` | I3 |
| FS-RO-03 | outcome.json names another task -> warn log, read as missing | ok |  |  | `finalizer.rs::failure_surface_sweep_clean` | I3 |
| FS-RO-04 | run/gate-report.json absent, malformed or oversize -> missing (it carries no task id, so no foreign check) | ok |  |  | `finalizer.rs::failure_surface_sweep_clean` | I3 |
| FS-RO-05 | pack-outcome.json absent, malformed, oversize or foreign -> missing | ok |  |  | `finalizer.rs::failure_surface_sweep_clean` | I3 |
| FS-RO-06 | training-manifest.json absent or foreign -> partial-manifest.json is read; both absent -> no training entries | ok |  |  | `finalizer.rs::fin_partial_inventory` | I3, I7 |
| FS-RO-07 | pack-manifest.json absent or malformed -> empty (no task-id check: it is a bare array) | ok |  |  | `finalizer.rs::failure_surface_sweep_clean` | I3 |
| FS-RO-08 | any read failing for another reason (5xx, denied, a broken body) -> Store error, nothing settled or the publish resumes; the lock is held and the next pass heals | ok |  |  | `finalizer.rs::failure_surface_transient_faults` | I2 |
| FS-RO-09 | a manifest entry listed but absent in S3 (FS-EX-06, FS-PK-06): read_run_objects keeps the entry; closed by 14-17: drop_absent_entries removes every entry S3 does not hold, from one listing of the run prefix; FinalizeReport.dropped and one listed-but-absent note name them | closed | WR-02 | 14-17 | `finalizer.rs::fin_listed_but_absent_outcome` | I2 |

### Decision precedence of finalize_run (FS-DP)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-DP-01 | aborted -> cancelled, entries minus the .apr | ok |  |  | `finalizer.rs::fin_abort_event` | I5, I6 |
| FS-DP-02 | a path listed by two writers with different content -> failed integrity | ok |  |  | `finalize.rs::finalize_integrity` | I3 |
| FS-DP-03 | no outcome -> failed with the stage error text when one was supplied (else 'outcome.json is missing'); since 14-17 a parsed outcome.json decides the class whatever the delivery path, and the stage error is detail only (appended '; stage error: ...' to crash, timeout, device, missing gate-report and missing pack-record failures); closed by 14-17: every catch-fed class is decided from its records through fixtures::through_catch and with no stage error, to the same class | closed | WR-01 | 14-17 | `finalize.rs::finalize_catch_fed_both_deliveries` | I3, I4, I8 |
| FS-DP-04 | no outcome -> failed 'outcome.json is missing' | ok |  |  | `finalize.rs::finalize_no_outcome` | I3 |
| FS-DP-05 | refused inputs-changed -> failed, prefilter_disagreement false | ok |  |  | `finalize.rs::finalize_inputs_changed` | I3 |
| FS-DP-06 | any other refusal -> failed, prefilter_disagreement TRUE, the trainer's REFUSED line (D-08), on every delivery path including the Catch (stage error not appended); closed by 14-17: a stage error plus a refusal the pre-filter accepted gives prefilter_disagreement true and the REFUSED line | closed | WR-01 | 14-17 | `finalize.rs::finalize_refused_with_stage_error` | I3, I8 |
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
| FS-PB-05 | tag targets = every manifest.json entry plus manifest.json: a listed-but-absent key fails GetObjectTagging (NoSuchKey) and wedges the lock; closed by 14-17: tag targets are intersected with a listing taken after verdict.json and manifest.json are written; an adopted manifest.json that names a gone object no longer wedges the lock, and no unlisted key is ever tagged | closed | WR-02 | 14-17 | `finalizer.rs::fin_adopted_manifest_with_absent_entry` | I2 |
| FS-PB-06 | the self-listing inventories (training-, partial-, pack-manifest.json) are never tag targets: a kept run's inventories keep 'expire'; closed by 14-17: every self-listing inventory that exists carries the run's retention tag: keep for a kept GATE PASS, expire otherwise | closed | IN-01 | 14-17 | `finalizer.rs::fin_kept_run_retags_inventories` | I7 |
| FS-PB-07 | every tag read back; the lock released LAST and re-read; a failure at any step returns Err with the lock held | ok |  |  | `finalizer.rs::fin_crash_resume_after_readback` | I2, I7 |
| FS-PB-08 | record missing past the task TTL -> an existing verdict.json is the decision, else the fresh one; inside the TTL -> integrity, nothing written | ok |  |  | `finalizer.rs::fin_record_expired` | I1 |

### Caller-visible text sinks (SK)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| SK-01 | sfn.rs SfnExecutions::start renders the SDK error (DisplayErrorContext) into StartError.message -> DispatchError -> lib.rs `REFUSED dispatch: {reason}` and the compensated payload's error; closed by 14-18 (dispatch label, an allowlist): SfnExecutions::start builds StartError.message from start_error_label, 'StartExecution <code>' with the AWS error code only when it is an identifier, else 'StartExecution unknown'; the DisplayErrorContext text goes to tracing::error (log-only). lib.rs also passes every DispatchError reason through caller_safe before the REFUSED dispatch error and the compensated payload (defense in depth); start_error_label_table holds the hostile and benign codes | closed | WR-03 | 14-18 | `lib.rs::hostile_dispatch_reasons_are_caller_safe` | I4 |
| SK-02 | finalizer.rs bounded_error forwards the Catch `Error: Cause` (a DescribeTrainingJob JSON naming the ECR image and role ARN) as stage_error -> finalize.rs payload.error -> the stored payload, verdict.json, train_status; closed by 14-18 (the Catch reduction): bounded_error calls redact::stage_error, so the decision gets the Error name when it is an identifier and, from a JSON Cause, only the first four values of FailureReason, errorMessage, StoppedReason, StopCode, Reason, ExitCode, SecondaryStatus and TrainingJobStatus, each caller-safe at 240 characters; a non-JSON Cause is caller-safe at 300; the full text goes to tracing::warn (log-only). The sweep plants a value no redaction rule knows under a Cause key outside the whitelist (HOSTILE_UNLISTED), so I4 sees a forwarded Cause even where the serializer's blacklist would pass it (stage_error_table, M-WR03-c) | closed | WR-03 | 14-18 | `finalizer.rs::failure_surface_sweep_clean` | I4 |
| SK-03 | lib.rs submit: the mint, lock-backend and envelope error texts reach pmcp Error::internal and the compensated payload; closed by 14-18 (RedactingBackend): the mint, lock and envelope texts keep their wording, but the backend text inside them is now 'the task store failed on <op>'; hostile_backend_mint_failure_is_caller_safe, hostile_backend_lock_failure_is_caller_safe and hostile_backend_envelope_failure_is_caller_safe plant the DynamoDB renderer's text and find none of it in the pmcp error or the compensated payload | closed | WR-03 | 14-18 | `lib.rs::hostile_backend_lock_failure_is_caller_safe` | I4 |
| SK-04 | lib.rs status: `no task {task_id} on this server: {e}` carries the store's error text to the caller; closed by 14-18: a NotFound or Expired store error reads 'no task <id> on this server', any other 'the task store could not be read', through caller_safe, never the store's own Display text | closed | WR-03 | 14-18 | `lib.rs::hostile_backend_status_and_task_protocol_are_caller_safe` | I4 |
| SK-05 | the sibling DynamoDbTaskBackend (aprender-setfit-train-lambda dynamodb.rs `aws`) renders DisplayErrorContext into BackendError: the text SK-03 and SK-04 forward; closed by 14-18 at the laya boundary (the sibling crate is live and unchanged): bootstrap.rs wraps the DynamoDbTaskBackend in RedactingBackend before ServerCtx::new, so every Internal store error is logged in full (eprintln) and returned as a fixed text naming the operation; Conflict, NotFound, Expired and InvalidTransition pass through (task ids only). pmcp's task handlers (tasks/get, tasks/result) read the same wrapped backend (hostile_backend_status_and_task_protocol_are_caller_safe). The finalizer binary is not wrapped: its errors are log-only | closed | WR-03 | 14-18 | `lib.rs::hostile_backend_mint_failure_is_caller_safe` | I4 |
| SK-06 | job-written lines reach the caller verbatim: outcome.refused_line in the error, pack verify_line / packed_line in verdict.pack.line and the pack-refused error; closed by 14-18 at the one serializer: StatusPayload::to_value passes error, verdict.pack.line and every note through caller_safe, so the refused line, the verify and packed lines and the pack-refused error are caller-safe in the stored payload, verdict.json, tasks/result and train_status; the real smoke verify line is kept byte for byte (redact_must_keep_table) | closed | WR-03 | 14-18 | `finalizer.rs::failure_surface_sweep_clean` | I4 |
| SK-07 | finalizer-side renderers (objects.rs S3, compute.rs SageMaker / ECS / Step Functions) -> FinalizerError -> the Lambda error and the execution history only, never a stored payload (log-only sink class (14-18): FinalizerError text becomes the Lambda invocation error and the execution history, never a caller-visible string; the transient table plants hostile store text and finds none of it in the stored payload, verdict.json or manifest.json) | ok |  |  | `finalizer.rs::failure_surface_transient_faults` | I4 |
| SK-08 | dataset.rs renderers -> tracing::warn only; the refusal names the bucket and prefix, never the SDK text | ok |  |  | `dataset.rs::dataset_denied_is_a_typed_refusal_naming_bucket_and_prefix` |  |
| SK-09 | sfn.rs describe (dispatch confirm) and stop (cancel sink) renderers -> eprintln / tracing::error only (log-only sink class (14-18): the describe text is printed by the dispatch-unconfirmed eprintln, the stop text by the cancel sink's log; neither reaches a caller (the dispatch label of SK-01 is the only caller-visible StartExecution text)) | ok |  |  | `sfn.rs::cancel_sink_logs_a_failing_stop_and_returns` |  |
| SK-10 | log calls (tracing, eprintln, println) in the two crates: server-side log only | ok |  |  | `sfn.rs::cancel_sink_logs_a_failing_stop_and_returns` |  |
| SK-11 | static caller texts (validation refusals, single-flight, failed_payload reasons with no infrastructure text, orphan dispatch-unconfirmed, stage_error None) | ok |  |  | `lib.rs::submit_single_flight_refuses_naming_the_holder_and_mints_nothing` |  |
| SK-12 | the run bucket is named aprender-laya-train-<env>-<account> (laya-trainer-stack.ts), so StatusPayload.run_prefix, every manifest link and the dataset refusals carry the account id BY CONSTRUCTION (decided by 14-18: a handle, not a leak. The caller names this bucket itself in dataset_uri, so the account id in run_prefix and the manifest links is a value the caller already holds; redacting it would break the s3:// handle train_status and laya-fetch resolve. caller_safe therefore runs on the free-text fields only (error, verdict.pack.line, notes), never on run_prefix or a link, and I4's planted constants exclude the bucket account id by design (stated in I4); an s3:// URI or a bucket-name account id INSIDE a free-text field reads [redacted]) | ok |  |  | `finalizer.rs::tracer_completed` | I4 |
| SK-NOTES | finalize.rs Builder::decide pushes StatusPayload.notes: stage-error-overridden (one fixed sentence) and listed-but-absent (the count and at most eight job-written manifest paths, each clipped to 128 characters) -> the stored payload, verdict.json, tasks/result and train_status. No stage error or infrastructure text by construction, but the paths are job-written, so 14-18 extends its serializer redaction to notes; closed by 14-18 at the one serializer: every notes entry passes caller_safe at 1024 characters in StatusPayload::to_value; the fixed prefixes stage-error-overridden and listed-but-absent are kept (the I2 and I3 oracles read them), a job-written path that embeds an account id or an ARN reads [redacted] (M-WR03-h) | closed | WR-03 | 14-18 | `redact.rs::redact_notes_through_to_value` | I4 |

### Asset provenance (FS-AS)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-AS-01 | the finalizer Lambda asset (deploy-extensions/assets/laya-finalizer/bootstrap) is deployed with an existence check only: no commit, clean-tree or sha256 provenance; closed by 14-19: the synth writes a provenance sidecar (bootstrap.build.json: commit, clean closure, sha256) beside the asset, and infra diff/deploy refuse an unattributable asset (check_provenance, the provenance cases of the ops selftest). The sidecar goes stale by design whenever a closure file changes (14-17 and 14-18 did): 14-22 re-synths before the owner deploys | closed | WR-04 | 14-19 | `laya_trainer_ops.py::selftest` |  |

### Tests that do not model the always-catching machine (FS-UT)

| Id | What | Status | Finding | Closes | Covering test | Invariants |
|----|------|--------|---------|--------|---------------|------------|
| FS-UT-01 | finalize.rs tests hand finalize_run stage_error None for catch-fed classes (refused, inputs-changed, crash, timeout, cpu, partial-manifest timeout): a delivery the machine never makes for those classes; closed by 14-17: finalize_refused, finalize_inputs_changed, finalize_crash, finalize_timeout and finalize_cpu are one table over the two deliveries (fixtures::through_catch and None) | closed | WR-01 | 14-17 | `finalize.rs::finalize_catch_fed_both_deliveries` | I3 |
| FS-UT-02 | finalizer.rs tests deliver catch-fed classes with stage_error None (payload(None), status events, reconcile), and fin_catch_error carries a Cause but no outcome; closed by 14-17: I8 now holds, so a stage_error None delivery (event, reconcile) decides as the Catch does and those tests model a real delivery; fin_listed_but_absent_* and fin_override_note_reaches_verdict deliver with and without the stage error | closed | WR-01 | 14-17 | `finalizer.rs::failure_surface_sweep_clean` | I3 |

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
| `aprender-mcp-laya-train-lambda/src/finalizer.rs` | log | 8 | log-only 8 | SK-10 |
| `aprender-mcp-laya-train-lambda/src/finalizer.rs` | payload-error-assign | 1 | static 1 | SK-11 |
| `aprender-mcp-laya-train-lambda/src/finalizer.rs` | stage-error | 10 | caller 4, static 6 | SK-02, SK-11 |
| `aprender-mcp-laya-train-lambda/src/objects.rs` | aws-renderer | 5 | log-only 5 | SK-07 |
| `aprender-mcp-laya-train-lambda/src/sfn.rs` | aws-renderer | 3 | log-only 3 | SK-01, SK-09 |
| `aprender-mcp-laya-train-lambda/src/sfn.rs` | dispatch-error | 4 | caller 2, static 2 | SK-01, SK-11 |
| `aprender-mcp-laya-train-lambda/src/sfn.rs` | log | 7 | log-only 7 | SK-10 |
| `aprender-mcp-laya-train-lambda/src/sfn.rs` | start-error | 2 | static 2 | SK-01 |
| `aprender-mcp-laya-train/src/finalize.rs` | payload-error-assign | 1 | caller 1 | SK-02, SK-06 |
| `aprender-mcp-laya-train/src/finalize.rs` | payload-notes | 2 | caller 2 | SK-NOTES |
| `aprender-mcp-laya-train/src/finalize.rs` | stage-error | 7 | caller 3, static 4 | SK-02, SK-11 |
| `aprender-mcp-laya-train/src/layout.rs` | payload-error-assign | 1 | caller 1 | SK-02, SK-06 |
| `aprender-mcp-laya-train/src/lib.rs` | dispatch-error | 3 | caller 2, static 1 | SK-01, SK-11 |
| `aprender-mcp-laya-train/src/lib.rs` | failed-payload | 7 | caller 3, static 4 | SK-01, SK-03, SK-11 |
| `aprender-mcp-laya-train/src/lib.rs` | log | 1 | log-only 1 | SK-10 |
| `aprender-mcp-laya-train/src/lib.rs` | payload-error-assign | 1 | caller 1 | SK-01, SK-03 |
| `aprender-mcp-laya-train/src/lib.rs` | pmcp-internal | 4 | caller 3, static 1 | SK-03, SK-11 |
| `aprender-mcp-laya-train/src/lib.rs` | pmcp-validation | 4 | caller 2, static 2 | SK-01, SK-04, SK-11 |
| `aprender-mcp-laya-train/src/redact.rs` | log | 1 | log-only 1 | SK-10 |
| `aprender-mcp-laya-train/src/redact.rs` | stage-error | 9 | caller 1, static 8 | SK-02 |
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
- `finalize.rs` payload-error-assign caller (every failed decision's text, including the stage error and the job lines,
  SK-02, SK-06). stage-error (since 14-17, which removed the early return): the input field, the Builder field and the
  `is_some()` that triggers the override note are static, and so is the Builder's field name at its construction.
  `with_stage` (the detail appended to a failure), the read of `input.stage_error` into the Builder and the no-outcome
  fallback carry the text (caller). payload-notes (since 14-17): the two `notes.push` calls in `Builder::decide`. One
  is a fixed sentence and the other is the count plus at most eight job-written manifest paths. Both are caller-visible
  (SK-NOTES), and 14-18 extends its serializer redaction to them.
- `finalizer.rs` log (since 14-17, 7 calls): two more `tracing::warn!` lines in `finalize_task`, the listed-but-absent
  count and the stage-error override. They name the task id, never object content or the stage error.
- `dynamodb.rs` aws-renderer 121 caller (SK-05: the backend text SK-03 and SK-04 forward).
- `objects.rs`, `compute.rs`, `dataset.rs` renderers and every log call: log-only (SK-07, SK-08, SK-09, SK-10).
- Since 14-18 (the lines above are the 14-16 classification, kept as the record):
  - `sfn.rs` aws-renderer: all 3 are log-only. `SfnExecutions::start` renders `DisplayErrorContext` into a
    `tracing::error!` line, and `StartError.message` is `start_error_label`, so start-error is static 2 (the type and a
    code-only construction). dispatch-error still forwards `e.message`, which is now that label (caller, an allowlist).
    log is 7 (the new `tracing::error!`).
  - `finalizer.rs` stage-error is 10: the `redact::stage_error` import and its doc mention are static, the call in
    `bounded_error` is caller (reduced text). log is 8 (the full Catch Cause, `tracing::warn!`, server-side only).
  - `redact.rs` (new): stage-error 9 (the `pub fn stage_error` producer is caller, 8 doc mentions static); log 1 (the
    `RedactingBackend` `eprintln!` of the full store text, log-only).
  - `layout.rs` payload-error-assign 1: `StatusPayload::to_value` assigns the caller-safe error (caller, the redaction
    point of SK-02 and SK-06).
  - `lib.rs` counts are unchanged; the dispatch reasons and the `train_status` store error now pass `caller_safe`, and
    the store error is a fixed text (SK-04).

## Tests that do not model the always-catching machine (FS-UT, D-i)

These tests hand the decision a stage error of None for classes the machine delivers ONLY through its Catch (or a
Cause with no outcome). They are not wrong about what they test, but none of them models the delivery the machine
really makes. Plan 14-17 remodelled the five `finalize.rs` class tests. `finalize_refused`, `finalize_inputs_changed`,
`finalize_crash`, `finalize_timeout` and `finalize_cpu` are now one table, `finalize_catch_fed_both_deliveries`, run
over `fixtures::through_catch` and over no stage error, so they leave `tests_not_modelling_catch`. The rest stay
listed: since I8 holds, a stage_error None delivery (event, reconcile) decides as the Catch does.

| Test | Delivery | Why it does not model the machine |
|------|----------|-----------------------------------|
| `finalize.rs::finalize_partial_manifest` | finalize_run direct, stage_error None | timeout exits 1 (correction: not in the plan's seed) |
| `finalizer.rs::fin_partial_inventory` | state-machine payload WITHOUT error | a timeout reaches Finalize only through CatchFinalize (correction: not in the plan's seed) |
| `finalizer.rs::fin_timeout_event` | TIMED_OUT status event | a trainer timeout fails Train and is caught; the event re-drives it only after CatchFinalize itself failed |
| `finalizer.rs::fin_reconcile_lost_event` | reconcile sweep | crash is caught; the sweep sees it only after CatchFinalize failed |
| `finalizer.rs::fin_first_writer_wins` | late FAILED status event | tests first-writer-wins; its crash class is incidental |
| `finalizer.rs::fin_catch_error` | state-machine payload WITH a Cause | a Cause but no outcome.json: never a parsed outcome against a stage error |

## Dispositions

- WR-01 (FS-SM-03, FS-SM-04, FS-EX-03..05, FS-EX-07, FS-PK-04, FS-PK-05, FS-DP-03, FS-DP-06, FS-UT-01, FS-UT-02;
  invariants I3, I8): CLOSED by plan 14-17 (`2ba21c6ee`). A parsed `outcome.json` decides the class and the stage error
  is detail. A pass or gate failure the records decided over a stage error carries the `stage-error-overridden` note.
- WR-02 (FS-EX-06, FS-PK-06, FS-RO-09, FS-PB-05; I2) and IN-01 (FS-PB-06; I7): CLOSED by plan 14-17 (`736eacdb6`).
  Listed-but-absent entries are dropped when the objects are read and named in one bounded note. The tag targets are
  intersected with a post-write listing, and every self-listing inventory is tagged.
- WR-03 (SK-01..SK-06, SK-NOTES; I4): CLOSED by plan 14-18 (`79ffa99bb`, `1a0606a98`). Caller text is made safe at
  three boundaries: the one serializer (`StatusPayload::to_value`: error, pack line, notes), `RedactingBackend` around the
  DynamoDB backend, and the dispatch code label. The Catch Cause is reduced (`stage_error`), not forwarded. Finalizer-side
  error text (FinalizerError, SK-07) and the describe and stop texts (SK-09) are the log-only sink class. SK-12 is a
  handle by decision. Accepted residual (T-14-86): a shape the blacklist does not know, in a job-written line; the
  backend and dispatch sinks are allowlists (fixed or code-only text), so the blacklist guards only the Catch reduction
  and the trainer and packer lines.
- WR-04 (FS-AS-01): CLOSED by plan 14-19, which added the asset provenance sidecar and bound the state-machine rows to the
  synthesized template (`check_surface_binding`; FS-SM-01 now cites it). The sidecar is stale by design after 14-17 and
  14-18 changed closure files; 14-22 re-synths before the owner deploys.

## Out of class (kept visible, not dropped)

| Item | Why it is not in this class or this round |
|------|-------------------------------------------|
| D-16 (verification truth 12) | an owner decision: a second reproduction is paid compute the owner recorded as not authorised, and a waiver is an owner-written override |
| the packing-cancel live check (truth 8) | about 62 min of paid compute; an owner-accepted limitation |
| IN-02 (image rebuild after a partial build) | a build-tooling matter outside the finalizer and state-machine boundary |
| IN-03 (pre-filter vs trainer gate-contract skew) | needs an owner decision on what a skew should do to a phase, so the required information is missing |

