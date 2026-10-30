---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 16
subsystem: testing
tags: [laya, finalizer, step-functions, failure-surface, ratchet, sweep, gap-closure]
status: complete

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: "the finalizer (14-05), run_step.py (14-06/14-07), the trainer stack (14-08) and the verification/review findings WR-01..WR-04, IN-01"
provides:
  - "ONE parameterized sweep over the finalizer and state-machine boundary (4800 cells + 10 aborted cells, ~3 s) computing I1..I8 through the real handle()"
  - "a shrink-only ratchet KNOWN_DEFECTS = [I2, I3, I4, I7, I8], proven to be exactly the violated set at HEAD"
  - "a transient-fault table: each of the 50 S3 calls of a passing run failing once heals on the next sweep"
  - "the checked-in enumeration failure-surface-v1.json + 14-FAILURE-SURFACE.md (68 rows, 22 sink groups, 77 hits)"
  - "scripts/laya_deploy/laya_failure_surface.py (--check, --selftest, --sweep, --mutations stub), just laya-failure-surface, gate row failure-surface"
affects: [14-17, 14-18, 14-19, 14-22]

actuals:
  tokens: 41879
  tasks: 3
  commits: 3
plan_head_before: 468b55b18c47cee1e748ecfe581ee050bc717795
plan_head_after: 9a8c933506b40ae435aa179530c3ea69fb991117

tech-stack:
  added: []
  patterns:
    - "a class invariant written as numbered clauses, swept over a declared cross product with an oracle that never calls the implementation's rules"
    - "a shrink-only ratchet that fails on a stale entry as well as on an unpinned violation"
    - "an enumeration artifact whose re-derivable facts (consts, exit map, sink counts, state machine) are re-derived by its checker, with a selftest that requires EXACTLY the intended rule per planted mutation"

key-files:
  created:
    - crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-FAILURE-SURFACE.md
    - scripts/laya_deploy/laya_failure_surface.py
  modified:
    - crates/aprender-mcp-laya-train-lambda/src/objects.rs
    - crates/aprender-mcp-laya-train-lambda/src/finalizer.rs
    - justfile
    - scripts/laya_gates.tsv

key-decisions:
  - "The tracer's exact assertion is scoped to I1..I3 (TRACER_SCOPE): the planted Catch Cause also leaks (I4), so the full set of the WR-01 cell is [I3, I4]; the tracer additionally requires every violation of its cell to be pinned, so 14-17 can assert 'no violations in scope' while I4 stays pinned for 14-18"
  - "absent axis values mean neither written nor listed; listed-but-absent is its own axis (absent_listed), so I2 is violated in exactly the 500 record groups of the WR-02 shape and nowhere else"
  - "Rows with no covering test at HEAD (FS-DP-11, SK-03, SK-04, SK-05) are recorded as 'untested at HEAD' rather than bound to an unrelated test name; the checker allows a null test only with that note and counts them in its OK line"
  - "SK-12: the run bucket aprender-laya-train-<env>-<account> puts the account id in every run_prefix, manifest link and dataset refusal by construction; recorded as a caller handle (the caller names the bucket in dataset_uri), excluded from the planted I4 constants, for 14-18 to state explicitly"

patterns-established:
  - "failure_surface section in finalizer.rs tests: Cell / files_for / setup / deliver / Observed / findings; later plans extend the oracle and shrink KNOWN_DEFECTS"
  - "laya_failure_surface.py MUTATIONS table: 14-17 and 14-18 register (id, file, old, new, filter, expect) source mutations"

requirements-completed: [D-05, D-08, D-11, D-12, D-14, D-15, D-18, D-19, D-22]

coverage:
  - id: D1
    description: "Tracer: a caught refused run through the real handle() reports I3 (WR-01) while I1 and I2 hold"
    requirement: D-08
    verification:
      - kind: unit
        ref: "crates/aprender-mcp-laya-train-lambda/src/finalizer.rs#failure_surface_tracer_wr01"
        status: pass
    human_judgment: false
  - id: D2
    description: "Swept gate over the full 4800-cell product with the ratchet; the violated set equals KNOWN_DEFECTS exactly"
    requirement: D-11
    verification:
      - kind: unit
        ref: "crates/aprender-mcp-laya-train-lambda/src/finalizer.rs#failure_surface_sweep_known_defects_only"
        status: pass
      - kind: other
        ref: "mutant: dropping I7 from KNOWN_DEFECTS turns the sweep red ('violations outside the ratchet: [(I7, ...')"
        status: pass
    human_judgment: false
  - id: D3
    description: "Transient-fault table: each single S3 call failure (50 cases, hostile text) heals to the uninjected end state"
    requirement: D-12
    verification:
      - kind: unit
        ref: "crates/aprender-mcp-laya-train-lambda/src/finalizer.rs#failure_surface_transient_faults"
        status: pass
    human_judgment: false
  - id: D4
    description: "Enumeration artifact + markdown bound to the code by the checker; 14 planted mutations each refused for exactly their own rule"
    requirement: D-18
    verification:
      - kind: other
        ref: "python3 -I scripts/laya_deploy/laya_failure_surface.py --check"
        status: pass
      - kind: other
        ref: "python3 -I scripts/laya_deploy/laya_failure_surface.py --selftest"
        status: pass
      - kind: other
        ref: "python3 -I scripts/laya_deploy/laya_failure_surface.py --sweep"
        status: pass
    human_judgment: false
  - id: D5
    description: "Recipe laya-failure-surface and gate row failure-surface (tampered copy refused, check and selftest pass)"
    requirement: D-19
    verification:
      - kind: other
        ref: "LAYA_GATES_ONLY=failure-surface just laya-gates-selftest"
        status: pass
    human_judgment: false
  - id: D6
    description: "The inspection itself: that the 68 rows, their dispositions and the sink classifications are complete and correct"
    verification: []
    human_judgment: true
    rationale: "The checker binds what can be re-derived (counts, consts, exit map, machine), but whether every hit is classified into the RIGHT class and whether a failure point is missing from the enumeration is a reviewer judgement"

duration: 36min
completed: 2026-10-30
---

# Phase 14 Plan 16: the finalizer / state-machine failure surface Summary

**The finalizer boundary is now one swept class: 4800 cells through the real `handle()` with an independent I1..I8 oracle, a shrink-only ratchet that equals exactly {I2, I3, I4, I7, I8} at HEAD, a 50-case transient-fault table, and a checked-in 68-row enumeration whose checker re-derives the consts, the exit map, every sink count and the state machine. No production code changed.**

## Performance

- **Duration:** 36 min
- **Started:** 2026-10-30T16:45:09Z
- **Completed:** 2026-10-30T17:21:53Z
- **Tasks:** 3
- **Files modified:** 7 (3 created, 4 modified)

## Accomplishments

- **Tracer (Task 1).** `failure_surface_tracer_wr01` delivers a trainer refusal the pre-filter accepted the way the CDK `CatchFinalize` does (an RFC 3339 `start_time`, `$$.Execution.Input`, and `error: {Error, Cause}` with a DescribeTrainingJob-shaped Cause). It passes through payload parse, `check_execution`, `finalize_task`, `read_run_objects`, `finalize_run`, `settle_record`, `publish`, the tags and the lock release. The harness reports I3: the stored error is the Cause text and `prefilter_disagreement` is false. I1 and I2 hold.
- **The sweep (Task 2).** `failure_surface_sweep_known_defects_only` runs the full cross product (5 deliveries x 10 outcomes x 2 gate reports x 4 pack records x 3 listings x 4 listed-but-absent objects = 4800, asserted equal to the product of `SURFACE_AXES`), plus 10 ABORTED cells, in about 3.1 s. The result at HEAD:

  | Invariant | Cells | Where |
  |-----------|-------|-------|
  | I2 | 2500 | all five deliveries, exactly the 500 listed-but-absent record groups (WR-02) |
  | I3 | 315 | `sm_caught` only (WR-01) |
  | I4 | 1560 | `sm_caught` 960 (Cause), 150 per other delivery (refused / pack lines) (WR-03) |
  | I7 | 12 | the kept-run shapes on the four non-Catch deliveries (IN-01) |
  | I8 | 56 | `sm_caught` against the other four (WR-01) |

  I1, I5 and I6 hold everywhere. Because I3 appears only on `sm_caught`, the oracle agrees with HEAD on the other four deliveries.
- **The transient table.** `failure_surface_transient_faults` fails each of the 50 S3 calls of a passing run once, with hostile error text. In every case the next sweep heals to the uninjected objects, tags and record, and the error text never reaches a caller surface.
- **The enumeration (Task 3).**
  - `failure-surface-v1.json` and `14-FAILURE-SURFACE.md` hold 68 rows: FS-SM, FS-EX, FS-PK, FS-RO, FS-DP, FS-PB, SK, FS-AS and FS-UT.
  - Every row has a status, a WR/IN reference and a closing plan if it is a defect, and a covering test (4 are explicitly untested).
  - The sink scan classifies 22 (file, pattern) groups, 77 hits in all, as caller, log-only or static.
- **The checker.**
  - `--check` binds the artifact to `finalizer.rs` (SURFACE_AXES, CATCH_FED, KNOWN_DEFECTS), to `job_exit_code` (run through importlib), to a fresh sink scan and to `laya-trainer-stack.ts`.
  - `--selftest` refuses 14 planted mutations, each for exactly its own rule.
  - `--sweep` runs the three sweep tests.
  - `--mutations` is the stub table that 14-17 and 14-18 fill.
- **The recipe and gate row.** `just laya-failure-surface` and gate row `failure-surface` are in place; `LAYA_GATES_ONLY=failure-surface just laya-gates-selftest` ends `LAYA GATES ROWS OK` with AWS CALLS: 0.

## Task Commits

1. **Task 1 (tracer): caught-refused cell detects WR-01**: `918ebebfd` (test)
2. **Task 2: swept gate, transient-fault table, ratchet**: `327d9cdb2` (test)
3. **Task 3: enumeration artifact, checker, recipe, gate row**: `9a8c93350` (feat)

**Plan metadata:** the docs commit that follows this file.

## Files Created/Modified

- `crates/aprender-mcp-laya-train-lambda/src/objects.rs`: test-only `FakeObjects::remove`, `fail_nth_with` (planted store text), `calls`, `Op::ALL`. `fail_nth` delegates and keeps its message.
- `crates/aprender-mcp-laya-train-lambda/src/finalizer.rs`: the test section `failure_surface` (`SURFACE_AXES`, `CATCH_FED`, `KNOWN_DEFECTS`, Cell, Observed, Invariant, the oracle, and the three tests). Its non-test prefix is byte-identical to HEAD.
- `crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json`: the machine-readable surface.
- `.planning/phases/14-.../14-FAILURE-SURFACE.md`: the inspection record (derivation output, corrections, dispositions, out-of-class list).
- `scripts/laya_deploy/laya_failure_surface.py`: the checker, standard library only.
- `justfile`: recipe `laya-failure-surface` and `row_failure_surface`.
- `scripts/laya_gates.tsv`: row `failure-surface`.

## Decisions Made

See `key-decisions` in the frontmatter. In brief:

- The tracer's exact claim is scoped to I1..I3, and any other violation of its cell must be a pinned one.
- The `absent` axis values and `absent_listed` are orthogonal.
- Untested rows are recorded as untested, not bound to a test that does not cover them.
- The account id carried in the bucket name is recorded as a caller handle (SK-12) for 14-18 to decide on.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The tracer's "violations equals exactly [I3]" could not survive Task 2 as written**
- **Found during:** Task 2
- **Issue:** The plan plants the hostile constants in the tracer's Catch Cause. Once Task 2 computes I4, the cell's full violation set is [I3, I4], so the exact assertion would fail. It would also block 14-17's "the WR-01 cell has no violations" while I4 stays pinned for 14-18.
- **Fix:** The exact assertion now covers `TRACER_SCOPE = [I1, I2, I3]`. The tracer additionally asserts that every violation of its cell is in KNOWN_DEFECTS.
- **Files modified:** crates/aprender-mcp-laya-train-lambda/src/finalizer.rs
- **Verification:** the tracer passes, and the full set of its cell is [I3, I4] ⊆ KNOWN_DEFECTS.
- **Committed in:** 327d9cdb2

**2. [Rule 1 - Bug] Covering tests that named production functions**
- **Found during:** Task 3
- **Issue:** The first draft bound some rows to `finalize_run`, `job_exit_code`, `decide_skip` and `cmd_infra`, which are production functions, not tests. A bare existence check could not tell the difference.
- **Fix:** Each of those rows now names the test that really exercises it: `finalize_integrity`, `finalize_malformed_report_is_refused_not_defaulted`, run_step.py `selftest` and laya_trainer_ops.py `selftest`. Rows that no test covers are marked `untested at HEAD`.
- **Files modified:** failure-surface-v1.json, 14-FAILURE-SURFACE.md, laya_failure_surface.py
- **Committed in:** 9a8c93350

**3. [Rule 2 - Missing critical] The selftest originally accepted a case when the intended rule was merely among the refusals**
- **Found during:** Task 3
- **Issue:** The plan requires each planted fault to be refused "for its own rule". Membership is weaker than that, and the stricter test immediately caught the sink-count case also tripping `surface-sink-class`.
- **Fix:** The refused set must equal {intended rule}. The sink-count mutation now models a consistently stale record.
- **Committed in:** 9a8c93350

---

**Total deviations:** 3 auto-fixed (2 bug, 1 missing critical).
**Impact on plan:** Each one tightens an assertion. No scope creep, and no production change.

**Plan-wording interpretations (recorded, not deviations of behaviour):**
- "Every row has ... a covering test": 4 rows (FS-DP-11, SK-03, SK-04, SK-05) have none at HEAD. They are explicit, counted rows.
- "Corrections to the seed" (all in 14-FAILURE-SURFACE.md):
  - FS-UT is wider than seeded: it also includes `finalize_partial_manifest`, `fin_partial_inventory`, `fin_timeout_event`, `fin_reconcile_lost_event` and `fin_first_writer_wins`.
  - `finalize_cpu` builds `pass` with `device_ok` false, while run_step writes a CPU fallback as `crash`.
  - The pack stage has two Catch-fed record-less shapes: PackError exit 1 and the env refusal exit 2.
  - SK-12 is a sink the seed did not list.

## Issues Encountered

- The rtk hook filters `cargo test --nocapture` output (a known memory item), so the sweep's printed report was read by running the test binary directly.
- `gsd-tools windows append` refused because ledger entry 24 has the status `resolved`, which is outside the tool's set. The ledger is best effort and I did not edit it. **Unrecorded window:** the `--mutations` runner stub in `scripts/laya_deploy/laya_failure_surface.py` (`MUTATIONS = []`), intentional and owned by 14-17.

## Known Stubs

| File | What | Why / resolved by |
|------|------|-------------------|
| scripts/laya_deploy/laya_failure_surface.py (`MUTATIONS = []`, `mutations()`) | `--mutations` prints that none is registered; the runner body refuses until cases exist | Specified by this plan; 14-17 implements the runner and registers M-WR01/M-WR02, and 14-18 adds its own |

## Threat Flags

None. No new network, auth, file-access or schema surface. Mitigations T-14-78..T-14-81 are realised:
- T-14-78 (WR-02): I2 over the listing and absent-listed axes, plus the transient table.
- T-14-79 (WR-01): I3 and I8, with the tracer.
- T-14-80 (WR-03): I4 with planted constants, plus the sink-count binding.
- T-14-81 (a vacuous gate): stale-entry and cell-count assertions, plus the const binding. Proven red by the I7 ratchet mutant.

T-14-SC: no package was installed or added.

## User Setup Required

None. No external service configuration required.

## Next Phase Readiness

- 14-17 can shrink KNOWN_DEFECTS to [I4], remodel the FS-UT tests and register its mutations in `MUTATIONS`.
- 14-18 owns I4 and the SK rows. It must decide SK-12, the account id in the bucket-name handle, explicitly.
- 14-19 can bind `state_machine` (states, catch per state, no_catch_states, machine_timeout_secs, event_statuses, reconcile_rate_minutes) to the synthesized template. The field names are the ones its plan reads.
- No CI step was added: wiring a workflow is an owner-approved edit.

## Self-Check: PASSED

- FOUND: all 3 created files and all 4 modified files
- FOUND: commits 918ebebfd, 327d9cdb2, 9a8c93350 (ancestors of HEAD; ledger count 3)
- Plan verification: `cargo test -p aprender-mcp-laya-train-lambda --lib` passes 63/63. `--check`, `--selftest` and `--sweep` print their OK lines. `git diff HEAD -- contracts crates/aprender-decide` is empty. finalize.rs, layout.rs, lib.rs and sfn.rs are unchanged since 359788182.

---
*Phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out*
*Completed: 2026-10-30*
