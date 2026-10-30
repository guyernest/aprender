---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 17
subsystem: api
tags: [laya, finalizer, step-functions, failure-surface, ratchet, mutation-testing, gap-closure, wr-01, wr-02, in-01]
status: complete

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: "14-16's failure-surface sweep (I1..I8 over 4800 cells), the ratchet KNOWN_DEFECTS = [I2, I3, I4, I7, I8], the enumeration artifact and the --mutations stub; 14-19's provenance sidecar (which this plan makes stale by design)"
provides:
  - "finalize_run classifies from outcome.json on every delivery path; the stage error is the no-outcome fallback and '; stage error: ...' detail on crash, timeout, device, gate-report-missing and pack-record-missing (WR-01, D-08)"
  - "StatusPayload.notes (skipped when empty): NOTE_STAGE_ERROR_OVERRIDDEN when the records decide completed/gate_failed over a stage error, and one bounded listed-but-absent note (D-18)"
  - "FinalizeDecision.stage_error_overridden, FinalizeInput.dropped, FinalizeReport.{stage_error_overridden, dropped}"
  - "drop_absent_entries: one listing of the run prefix drops every writer-manifest entry S3 does not hold before the decision (WR-02, D-11)"
  - "publish tags every manifest entry, manifest.json and every self-listing inventory that exists, intersected with a post-write listing (IN-01, D-19)"
  - "laya_failure_surface.py --mutations: a runner with controls, exactly-once anchors and byte-proven restoration; nine registered cases, FAILURE SURFACE MUTATIONS OK 9/9"
  - "the ratchet, the artifact and the markdown agree that only I4 (WR-03, 14-18) remains; the SK-NOTES sink is counted"
affects: [14-18, 14-22]

actuals:
  tokens: 19635
  tasks: 3
  commits: 3
plan_head_before: 79311edfbc7bcdbb502d9f85964bc5f31ca539dc
plan_head_after: 0d34b173fef53964b6fb3ed7da21c7f3daae16d0

tech-stack:
  added: []
  patterns:
    - "records decide, delivery is detail: a value supplied by only one delivery path may annotate a decision but never select it"
    - "a fix that keeps a non-error outcome over a reported failure must say so in the payload (a fixed-shape note), not only in a log line"
    - "totality by pruning at read: absent objects are removed once, before every consumer (decision, payload, manifest, links, tags)"
    - "on-disk source mutation runner: control green first, anchor exactly once, finally-restore with re-read, end-of-run byte comparison"

key-files:
  created:
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-17-SUMMARY.md
  modified:
    - crates/aprender-mcp-laya-train/src/finalize.rs
    - crates/aprender-mcp-laya-train/src/layout.rs
    - crates/aprender-mcp-laya-train-lambda/src/finalizer.rs
    - scripts/laya_deploy/laya_failure_surface.py
    - crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-FAILURE-SURFACE.md

key-decisions:
  - "A parsed outcome.json decides the class on every delivery; the stage error is never the class. When a stage error was supplied and the records still decide completed or gate_failed, the records win and the payload carries the stage-error-overridden note (no stage error text copied)"
  - "The stage error is appended only where the class text alone does not explain the failure (crash, timeout, device, gate-report-missing, pack-record-missing), never to a refusal: the trainer's REFUSED line is the better text"
  - "Listed-but-absent entries are pruned at read time (one listing in finalize_task), not only at tag time, because the same entries feed manifest.json, the presigned links and laya-fetch"
  - "The tag targets are a superset (entries, manifest.json, every SELF_LISTING name) restricted by ONE intersection with a listing taken after verdict.json and manifest.json are written; an adopted manifest.json is kept as written and only what exists is tagged"
  - "The listed-but-absent note is bounded whatever the job wrote: the count, at most 8 paths each clipped to 128 characters, then ', and N more'"
  - "FinalizeReport.stage_error_overridden reports this invocation's decision (as the plan specifies); the stored payload is the durable signal"
  - "Every row the plan closes is marked closed, not only the FS-DP, FS-PB and FS-UT groups the plan named: leaving FS-SM/EX/PK/RO rows 'defect, closes 14-17' after 14-17 would make the record stale"

patterns-established:
  - "MUTATIONS case shape: (id, file, old, new, runs) with runs = [(crate, filter, expected tokens)]; a token is a test name (its FAILED line) or an invariant id (the sweep's 'violations outside the ratchet' assertion)"
  - "mutation new-text is written warning-free (let _ = ...), so the case still compiles under -D warnings"

requirements-completed: [D-05, D-08, D-11, D-15, D-18, D-19]

coverage:
  - id: D1
    description: "WR-01: a refused, crashed or timed-out run is classified from its records with the D-08 flag on every delivery path; the stage error is detail"
    requirement: D-08
    verification:
      - kind: unit
        ref: "crates/aprender-mcp-laya-train/src/finalize.rs#finalize_catch_fed_both_deliveries"
        status: pass
      - kind: unit
        ref: "crates/aprender-mcp-laya-train/src/finalize.rs#finalize_refused_with_stage_error"
        status: pass
      - kind: unit
        ref: "crates/aprender-mcp-laya-train-lambda/src/finalizer.rs#failure_surface_tracer_wr01"
        status: pass
      - kind: other
        ref: "python3 -I scripts/laya_deploy/laya_failure_surface.py --mutations (M-WR01-a, -b, -c)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The override is visible: a pass or gate_fail the records decided over a stage error carries the stage-error-overridden note in the stored payload and verdict.json; no other phase or delivery does"
    requirement: D-18
    verification:
      - kind: unit
        ref: "crates/aprender-mcp-laya-train/src/finalize.rs#finalize_stage_error_overridden"
        status: pass
      - kind: unit
        ref: "crates/aprender-mcp-laya-train-lambda/src/finalizer.rs#fin_override_note_reaches_verdict"
        status: pass
      - kind: unit
        ref: "crates/aprender-mcp-laya-train/src/layout.rs#status_payload_notes_are_optional_on_the_wire"
        status: pass
      - kind: other
        ref: "--mutations M-WR01-d (finalize_stage_error_overridden and the I3 oracle turn red)"
        status: pass
    human_judgment: false
  - id: D3
    description: "WR-02: no run-object shape holds the lock; listed-but-absent entries are dropped at read, listed nowhere, tagged never, and named in one bounded note"
    requirement: D-11
    verification:
      - kind: unit
        ref: "crates/aprender-mcp-laya-train-lambda/src/finalizer.rs#fin_listed_but_absent_outcome, fin_listed_but_absent_pack_outcome, fin_listed_but_absent_report, fin_adopted_manifest_with_absent_entry"
        status: pass
      - kind: unit
        ref: "crates/aprender-mcp-laya-train/src/finalize.rs#finalize_dropped_note"
        status: pass
      - kind: other
        ref: "--mutations M-WR02-a, -b, -d, -e"
        status: pass
    human_judgment: false
  - id: D4
    description: "IN-01: every self-listing inventory carries the run's retention tag (keep for a kept GATE PASS, expire otherwise)"
    requirement: D-19
    verification:
      - kind: unit
        ref: "crates/aprender-mcp-laya-train-lambda/src/finalizer.rs#fin_kept_run_retags_inventories"
        status: pass
      - kind: other
        ref: "--mutations M-WR02-c (I7 turns red)"
        status: pass
    human_judgment: false
  - id: D5
    description: "The sweep: 4800 cells + 10 aborted, I1, I2, I3, I5, I6, I7, I8 hold; the violated set equals KNOWN_DEFECTS = [I4]"
    requirement: D-11
    verification:
      - kind: unit
        ref: "python3 -I scripts/laya_deploy/laya_failure_surface.py --sweep"
        status: pass
    human_judgment: false
  - id: D6
    description: "The artifact, the markdown and the Rust ratchet agree (known_defects I4), the SK-NOTES sink is counted, and no contract, decide crate or container program changed"
    requirement: D-05
    verification:
      - kind: other
        ref: "python3 -I scripts/laya_deploy/laya_failure_surface.py --check and --selftest"
        status: pass
      - kind: other
        ref: "git diff --quiet HEAD -- contracts crates/aprender-decide scripts/laya_train/container (rc 0)"
        status: pass
    human_judgment: false
  - id: D7
    description: "Whether the two note sentences read correctly to a caller of train_status, and whether the closed-row notes in 14-FAILURE-SURFACE.md are the right dispositions"
    verification: []
    human_judgment: true
    rationale: "Caller-facing wording and the completeness of an inspection record are reader judgements; the checker binds counts, ids and test existence, not meaning"

duration: 37min
completed: 2026-10-30
---

# Phase 14 Plan 17: outcome-first classification, total finalization and retained inventories Summary

**The finalizer now classifies a run from its records on every delivery path: the Catch's stage error is only detail, and a pass rescued from a failed Train job says so in a payload note. Finalization is total: listed-but-absent objects are pruned from one listing before every consumer and named in one bounded note. A kept run keeps its training, partial and pack inventories. Nine source mutations each turn a named test or invariant red, and the ratchet is down to I4 (14-18).**

## Performance

- **Duration:** 37 min
- **Started:** 2026-10-30T17:42:45Z
- **Completed:** 2026-10-30T18:20:14Z
- **Tasks:** 3
- **Files modified:** 6

## Accomplishments

- **WR-01 (Task 1, tracer).**
  - `finalize_run` now reads `outcome.json` first. The stage error is used for the error text only when no outcome parsed. Otherwise `Builder::with_stage` appends it as `; stage error: ...` to five failures: crash, timeout, device-not-ok, gate-report-missing and pack-record-missing.
  - A refusal keeps its REFUSED line and `prefilter_disagreement: true` on the Catch delivery too.
  - When a stage error was supplied and the records decide `completed` or `gate_failed`, `stage_error_overridden` is set and the fixed `NOTE_STAGE_ERROR_OVERRIDDEN` goes into `StatusPayload.notes`. The note therefore reaches verdict.json, tasks/result and train_status. `keep` is unchanged.
  - After the change, the sweep's I3 and I8 hold in all 4800 cells.
- **Tests remodelled (Task 1).**
  - The fixture helper `fixtures::through_catch` builds the stage error the way the Catch and `bounded_error` do.
  - The five catch-fed class tests are now one table (`CATCH_FED_CASES`), run over both deliveries. Each keeps its name, because the artifact rows refer to them.
  - New tests: `finalize_refused_with_stage_error`, `finalize_stage_error_overridden`, the layout notes test and `fin_override_note_reaches_verdict`.
  - The tracer now asserts that the WR-01 cell has no violation in scope.
- **WR-02 and IN-01 (Task 2).**
  - `drop_absent_entries` lists the run prefix once after `read_run_objects` and removes every writer-manifest entry S3 does not hold. It records the removed paths, which reach `FinalizeReport.dropped`, a warn with the count, and one bounded `listed-but-absent` note in every phase.
  - `publish` now builds its tag targets from three sources: every manifest entry, manifest.json, and every `SELF_LISTING` inventory. That set is intersected with a listing taken after both writes.
  - An adopted manifest.json that names a gone object no longer wedges the lock, and an unlisted key is never tagged.
  - The ratchet is now `[I4]`.
- **Red-side proofs (Task 3).**
  - `--mutations` runs each (crate, filter) unmutated first and requires it green, with the expected tests reported ok.
  - Each anchor must occur exactly once. The runner rewrites the file, requires every run to exit non-zero and name its tokens, then restores the bytes in a `finally` block and re-reads them. At the end it compares every touched file with its pre-run bytes.
  - A negative check proved the fail path: a behaviour-neutral mutation is reported SURVIVED, an absent anchor is reported INVALID, the run exits 1, and the file is byte-identical afterwards.
  - Result: `FAILURE SURFACE MUTATIONS OK 9/9`, run twice, the second time on the final script (272 s).
- **Artifact (Task 3).**
  - `known_defects` is `[I4]`.
  - 17 rows are `closed`, each with its fix note and covering test.
  - The I2 and I3 texts gained their note clauses.
  - New: the `SK-NOTES` row and the `payload-notes` sink pattern (2 caller hits).
  - Sink counts were refreshed: finalizer.rs log 7; finalize.rs stage-error 7, of which 3 caller and 4 static.
  - The five remodelled tests left `tests_not_modelling_catch`, and derivation D-j records the post-fix sweep.
  - Checker output: `FAILURE SURFACE OK rows=69 ... sinks=23 (hits 86) ... known_defects=I4`.

## Task Commits

1. **Task 1 (tracer): outcome decides the class, stage error is detail (WR-01), visible override note.** `2ba21c6ee` (fix)
2. **Task 2: total finalization over listed-but-absent objects, every inventory tagged (WR-02, IN-01).** `736eacdb6` (fix)
3. **Task 3: red-side mutation proofs, with the ratchet and artifact brought in line.** `0d34b173f` (test)

**Plan metadata:** the docs commit that follows this file.

## Files Created/Modified

- `crates/aprender-mcp-laya-train/src/finalize.rs`:
  - the reordered `finalize_run`, `Builder.stage_error` and `with_stage`;
  - the two notes, with `NOTE_STAGE_ERROR_OVERRIDDEN`, `NOTE_LISTED_BUT_ABSENT` and `dropped_note`;
  - `FinalizeInput.dropped` and `FinalizeDecision.stage_error_overridden`;
  - the updated module doc table and its "A stage error is detail" section;
  - `fixtures::through_catch` and the remodelled and new tests.
- `crates/aprender-mcp-laya-train/src/layout.rs`: `StatusPayload.notes` (`serde(default, skip_serializing_if = "Vec::is_empty")`), empty in `base()`, plus its wire test.
- `crates/aprender-mcp-laya-train-lambda/src/finalizer.rs`:
  - `RunObjects.dropped`, `writer_paths_of` and `drop_absent_entries`;
  - `FinalizeReport.{dropped, stage_error_overridden}` and two warn lines;
  - the new `publish` tag targets and the `SELF_LISTING` doc;
  - five new fin_ tests and `fin_override_note_reaches_verdict`;
  - in the sweep: the I2 and I3 oracle clauses, the I8 comment, the ratchet `[I4]` and the tracer.
- `scripts/laya_deploy/laya_failure_surface.py`: the runner (`cargo_test`, `names_token`, `restore`, `mutations`), the nine `MUTATIONS` and the `payload-notes` sink pattern.
- `crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json` and `14-FAILURE-SURFACE.md`: the record, brought in line as described above.

## Decisions Made

See `key-decisions` in the frontmatter. The two that shape behaviour:

- **The records win, and the payload says so.** The class never depends on the delivery path. A stage error that the records overrode is recorded as a fixed note, never as its text.
- **Pruning happens at read time.** The absent entries leave every consumer at once. The tag-time intersection is still needed for the self-listing names and for an adopted manifest.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `tracer_completed` pinned the IN-01 defect.**
- **Found during:** Task 2.
- **Issue:** The test asserted `report.tagged == 13` (12 entries plus manifest.json). That count encodes the old behaviour, which left the inventories untagged.
- **Fix:** The test now expects 15 and explicitly asserts that training-manifest.json and pack-manifest.json read back keep.
- **Files modified:** crates/aprender-mcp-laya-train-lambda/src/finalizer.rs.
- **Committed in:** 736eacdb6.

**2. [Rule 2 - Missing critical] Closing all 17 rows the plan closes, not only FS-DP, FS-PB and FS-UT.**
- **Found during:** Task 3.
- **Issue:** FS-SM-03/04, FS-EX-03..07, FS-PK-04..06 and FS-RO-09 are the same WR-01/WR-02 defects. Leaving them `defect, closes 14-17` would leave the inspection record stale.
- **Fix:** Each of these rows is `closed` with a fix note and a covering test that exists. The checker verifies the tests.
- **Committed in:** 0d34b173f.

**3. [Rule 2 - Missing critical] A stronger I2 oracle.**
- **Found during:** Task 2.
- **Issue:** The plan asked only for the positive note clause.
- **Fix:** The oracle also requires that neither the stored payload nor manifest.json lists the removed object, that exactly one listed-but-absent note exists, and that no such note appears when nothing was removed.
- **Effect:** M-WR02-a is refused for several reasons, not one.
- **Committed in:** 736eacdb6.

**4. [Rule 3 - Blocking] The mutation case shape is wider than the plan's single filter.**
- **Found during:** Task 3.
- **Issue:** M-WR01-d and M-WR02-e must each be refused by a test in the server crate and by the sweep in the lambda crate.
- **Fix:** A case is now `(id, file, old, new, runs)` with `runs = [(crate, filter, expected tokens)]`.
- **Committed in:** 0d34b173f.

---

**Total deviations:** 4 auto-fixed (1 bug, 2 missing critical, 1 blocking). **Impact:** each one tightens an assertion or keeps the record true. No scope creep and no production change beyond the plan.

**Plan-wording interpretations (recorded; behaviour is as specified):**

- **FS-DP row texts and `decision_order` were updated in Task 1.** Task 1's acceptance requires them, although the files are listed under Task 3. Their status was closed in Task 3.
- **The five remodelled tests keep their names.** The artifact rows and `tests_not_modelling_catch` refer to them. Each one runs its row of `CATCH_FED_CASES` on both deliveries, and `finalize_catch_fed_both_deliveries` runs the whole table.
- **`finalize_cpu` still builds `pass` with `device_ok` false.** That is the device arm, the one that gets the stage detail. A comment notes that run_step's own CPU fallback is the crash row.
- **The mutation new-texts are warning-free** (`let _ = ...`), so they compile under a `-D warnings` RUSTFLAGS.

## Issues Encountered

- The rtk hook rewrites `cargo test` output into a one-line summary, so individual test lines were read by running the deps test binary directly (a known memory item). The checker's own `cargo` subprocesses are not affected.
- My first `--check` run overlapped the first background mutation run, which rewrites the sources it scans. I re-ran `--check` and `--selftest` after the run had restored the tree, and both passed. The source files equal HEAD (`git diff --quiet` rc 0).
- `gsd-tools windows append` refused with `Ledger entry 24 has invalid status: "resolved"` (the known tooling issue), so nothing was appended. No stub, skipped test or unrun verify was left. The would-be entry was the mutation case-shape deviation above.

## Known Stubs

None. The `MUTATIONS = []` stub that 14-16 left is filled (nine cases).

## Threat Flags

None beyond the plan's register. T-14-82 (listed-but-absent DoS), T-14-83 (D-08 hidden, rescued pass reported clean) and T-14-84 (mutated source left behind) are mitigated as planned. T-14-84 was also proven by the negative runner check. The new caller sink SK-NOTES is in the plan, and the artifact records it as a 14-18 defect row. T-14-SC: no package installed; Cargo.toml and Cargo.lock are unchanged.

## User Setup Required

None. No AWS call was made and nothing was deployed.

## Next Phase Readiness

- **14-18** owns I4, the only ratchet entry left.
  - It must extend its serializer redaction to `StatusPayload.notes`. The listed-but-absent note carries job-written paths (SK-NOTES).
  - It must also keep the note's prefixes, which the I2 and I3 oracles read: `listed-but-absent`, `stage-error-overridden`.
  - The I4 count is now 1536 cells (`sm_caught` 936). A caught refusal now carries the REFUSED line instead of the Cause.
- **14-22:** this plan changed `finalize.rs`, `layout.rs` and `finalizer.rs`, all in the provenance closure, so `bootstrap.build.json` is stale by design (14-19). The owner must re-run `just laya-trainer-synth <env>` on the committed tree before `infra diff` and `deploy`.
- No CI workflow, contract, decide crate, container program or image changed (`git diff --quiet HEAD -- contracts crates/aprender-decide scripts/laya_train/container` rc 0).

## Self-Check: PASSED

- **Files:** all six modified files and this SUMMARY exist.
- **Commits:** `2ba21c6ee`, `736eacdb6` and `0d34b173f` are ancestors of HEAD. Ledger: `git rev-list --count 79311edfb..HEAD` is 3.
- **Plan verification, re-run on the final HEAD:**
  - `cargo test -p aprender-mcp-laya-train --lib`: 98 passed.
  - `cargo test -p aprender-mcp-laya-train-lambda --lib`: 69 passed.
  - `cargo clippy -p aprender-mcp-laya-train -p aprender-mcp-laya-train-lambda --lib --tests --no-deps -- -D warnings`: rc 0.
  - `cargo fmt --all -- --check`: rc 0.
  - `--check`: `FAILURE SURFACE OK rows=69 ... known_defects=I4`.
  - `--selftest`: `FAILURE SURFACE SELFTEST OK 14 cases`.
  - `--sweep`: `FAILURE SURFACE SWEEP OK`.
  - `--mutations --only` (all nine): `FAILURE SURFACE MUTATIONS OK 9/9`.
  - `LAYA_GATES_ONLY=failure-surface just laya-gates-selftest`: `LAYA GATES ROWS OK 1 of 55`, AWS CALLS 0.
  - `git diff --quiet HEAD -- contracts crates/aprender-decide`: rc 0.

---
*Phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out*
*Completed: 2026-10-30*
