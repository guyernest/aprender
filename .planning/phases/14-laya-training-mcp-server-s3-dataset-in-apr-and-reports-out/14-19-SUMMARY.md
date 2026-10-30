---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 19
subsystem: infra
tags: [laya, finalizer, provenance, cdk, step-functions, failure-surface, gap-closure, wr-04]
status: complete

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: "14-16's failure-surface-v1.json (state_machine object) and checker; 14-08's trainer stack and stack check; 14-09's laya_trainer_ops.py infra path"
provides:
  - "bootstrap.build.json beside the finalizer asset, written by just laya-trainer-synth (commit, cargo-tree source closure, closure_clean + dirty paths, sha256, target, UTC time)"
  - "laya_trainer_ops.py finalizer-provenance write|verify; check_provenance (pure rule); infra diff/deploy refuse an unattributable asset, DRY_RUN notes it"
  - "a provenance case table: 13 pure single-field mutations, a cargo-tree parser sample, write through go(), and 10 real-git checks in a temporary repository"
  - "check_surface_binding in laya_trainer_stack_check.py with 11 binding mutation cases (52 total, was 41)"
  - "deploy-extensions/README.md section 'Finalizer provenance' (what it attests, not reproducibility, ships inside the Lambda package, the closure is wide)"
affects: [14-17, 14-18, 14-22]

actuals:
  tokens: 14431
  tasks: 3
  commits: 3
plan_head_before: 3dd8d98fa5ff5371c6b9fd2b43c4d66fb1c72e26
plan_head_after: c60b487455d7ea5b20cd625e7506f0441e61864c

tech-stack:
  added: []
  patterns:
    - "provenance as attestation: a sidecar recorded at build time, a pure check over facts gathered by thin git helpers, refused on the deploy path only"
    - "closure-scoped staleness (git diff <build>..HEAD -- <closure>) instead of HEAD equality, so unrelated commits never refuse"
    - "binding an enumeration artifact to the synthesized template both ways, one rule name per compared field"

key-files:
  created:
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-19-SUMMARY.md
  modified:
    - justfile
    - scripts/laya_deploy/laya_trainer_ops.py
    - scripts/laya_deploy/laya_trainer_stack_check.py
    - deploy-extensions/README.md

key-decisions:
  - "The provenance check runs AFTER the account check and BEFORE any cdk call. The documented invariant that _laya-account-check is the first command of a live run stays true, and a refusal still stops the run before any cdk call"
  - "write records a dirty closure (closure_clean false, the dirty paths) and does not refuse it. It refuses only an absent binary, a failing cargo tree, a path dependency outside the checkout (git cannot attest it) and a malformed target"
  - "verify does not re-run cargo tree. A dependency added after the synth changes Cargo.lock and a closure crate's Cargo.toml, and both are in the closure"
  - "Binding rules are named per field (surface-binding-states, -catch, -no-catch, -flow, -timeout, -events, -reconcile, -artifact), so each mutation case is held to its own refusal. start_at and next are bound as well as the fields the plan listed"
  - "Untracked files under a closure directory count as dirty (status --untracked-files=all). A mutant that ignores them is caught by the real-git table"

patterns-established:
  - "Sim.git answers merge-base and diff; world() gives every simulated run an asset dir with a matching sidecar, so existing infra cases exercise the real gather path"
  - "SURFACE_CASES table in the stack check: (name, template mutation, artifact mutation, every rule that must refuse)"

requirements-completed: [D-01, D-02, D-12, D-14, D-22]

coverage:
  - id: D1
    description: "Tracer: synth writes the sidecar; infra deploy accepts a matching asset and refuses a tampered asset or a dirty-closure build (dry run notes it)"
    requirement: D-02
    verification:
      - kind: unit
        ref: "python3 -I scripts/laya_deploy/laya_trainer_ops.py --selftest#infra-provenance-tracer"
        status: pass
      - kind: other
        ref: "mutant: check_provenance returning None turns infra-provenance-tracer red"
        status: pass
      - kind: integration
        ref: "just laya-trainer-synth dev -> TRAINER SYNTH OK env=dev and bootstrap.build.json (12 crates, clean=true); finalizer-provenance verify -> FINALIZER PROVENANCE OK; a tampered copy -> REFUSED provenance: ... sha mismatch"
        status: pass
    human_judgment: false
  - id: D2
    description: "Provenance case table: every refusal for its own reason, the unrelated-commit case accepted, against a real temporary git repository"
    requirement: D-12
    verification:
      - kind: unit
        ref: "laya_trainer_ops.py --selftest#provenance-pure, provenance-closure-parse, provenance-write, provenance-write-refusals, provenance-real-git"
        status: pass
      - kind: other
        ref: "mutants: HEAD-equality instead of the closure diff, untracked files ignored and an unscoped diff each turn provenance-real-git red"
        status: pass
      - kind: integration
        ref: "LAYA_GATES_ONLY=trainer-synth,trainer-image,trainer-infra,failure-surface just laya-gates-selftest -> LAYA GATES ROWS OK 4 of 55"
        status: pass
    human_judgment: false
  - id: D3
    description: "The failure-surface state-machine rows are bound to the synthesized template both ways"
    requirement: D-22
    verification:
      - kind: unit
        ref: "python3 -I scripts/laya_deploy/laya_trainer_stack_check.py --selftest -> 52 mutation cases (41 template, 11 surface-binding), STACK CHECK SELFTEST OK"
        status: pass
      - kind: other
        ref: "mutant: check_surface_binding returning after the artifact check fails 9 of the 11 binding cases"
        status: pass
      - kind: other
        ref: "python3 -I scripts/laya_deploy/laya_failure_surface.py --check -> FAILURE SURFACE OK rows=68"
        status: pass
    human_judgment: false
  - id: D4
    description: "No CDK resource, IAM statement, contract, decide crate or workflow changed"
    requirement: D-14
    verification:
      - kind: other
        ref: "git diff --quiet HEAD -- deploy-extensions/lib contracts crates/aprender-decide .github (rc 0)"
        status: pass
    human_judgment: false
  - id: D5
    description: "The README states what the sidecar does and does not prove, that it ships in the Lambda package, and that any closure commit forces a re-synth"
    verification: []
    human_judgment: true
    rationale: "Whether the operator-facing prose is clear enough that the owner is not surprised at 14-22 deploy time is a reader judgement"

duration: 13min
completed: 2026-10-30
---

# Phase 14 Plan 19: finalizer provenance and the state-machine binding Summary

**The finalizer asset can no longer be deployed unless it is attributable. `just laya-trainer-synth` writes `bootstrap.build.json`, which holds the commit, a cargo-tree-derived source closure of 12 crates plus three root files, the sha256, the target and the UTC time. `infra diff` and `deploy` refuse on any of these: sha mismatch, a dirty build, a commit that is not an ancestor of HEAD, a closure path changed since the build, or a closure path dirty now. A commit that touches only planning documents is still accepted. The 14-16 enumeration's state machine is now bound, both ways, to the synthesized template by 11 new mutation cases (52 total).**

## Performance

- **Duration:** 13 min
- **Started:** 2026-10-30T17:25:48Z
- **Completed:** 2026-10-30T17:38:44Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments

- **Tracer (Task 1).** The `finalizer-provenance write` verb runs right after the synth copies and chmods the binary. It derives the closure with `cargo tree -p aprender-mcp-laya-train-lambda -e normal --prefix none --format {p} --target aarch64-unknown-linux-gnu` and writes the sidecar.
  - `cmd_infra` calls `verify_provenance` for `diff` and `deploy`, after the account check and before any `cdk` call. A refusal prints `REFUSED infra: finalizer provenance: <reason>; re-run just laya-trainer-synth <env> ...`; a dry run prints the same words as a `DRY RUN NOTE`.
  - The real synth ended `TRAINER SYNTH OK env=dev` with `clean=true`, 12 crates and 15 paths. After the Task 1 commit, which touched only the justfile and a script, `verify` still answered `FINALIZER PROVENANCE OK` on the real repository.
- **Case table (Task 2).**
  - **Pure cases:** 13 single-field mutations of an accepted record. Each opens with its own phrase and names its path.
  - **Parser sample:** a captured cargo tree sample with the root replaced, path deps, registry and git sources, duplicates, `(proc-macro)`, `(*)` and a `[build-dependencies]` header. The parser names the four finalizer crates plus aprender-decide, aprender-core and aprender-compute, and no registry crate. An out-of-checkout path is reported.
  - **`write` through `go()`:** a dirty closure is recorded, not refused. Infra then refuses that record and names the path.
  - **Real git:** 10 checks in a temporary repository. An unrelated commit (planning plus a non-closure crate) is ACCEPTED. Each of these is refused for its own reason:
    - an uncommitted closure edit;
    - an untracked closure file;
    - a committed closure edit;
    - a `Cargo.lock` bump;
    - a build on another branch;
    - a build from a dirty tree that was later reverted.
  - **Mutants:** HEAD-equality, ignoring untracked files and an unscoped diff each turn the table red.
  - **README:** the provenance section and step 4 of the deployment order.
- **Binding (Task 3).** `check_surface_binding` compares the artifact's state machine with the real ASL and EventBridge rules: states, Catch per state, no-catch states, StartAt and Next, TimeoutSeconds, the status rule's statuses and the reconcile rate. Each field refuses under its own `surface-binding-<field>` name.
  - The 11 new cases include an edge the template gained (a Catch on Finalize, which no pre-existing rule saw) and an edge the artifact claims but the template lacks.
  - The selftest now reports `52 mutation cases (41 template, 11 surface-binding)`. Trainer assertions went from 205 to 216.

## Task Commits

1. **Task 1 (tracer): sidecar and fail-closed infra check** - `a0902975d` (feat)
2. **Task 2: provenance case table against a real temporary git repository, README** - `950e702f0` (test)
3. **Task 3: bind the failure-surface state-machine rows to the template** - `c60b48745` (feat)

## Files Created/Modified

- `justfile` - `laya-trainer-synth` calls `finalizer-provenance write "$ASSET" --target {{target}}` after the copy; comment updated
- `scripts/laya_deploy/laya_trainer_ops.py` - the provenance section (`parse_closure`, `check_provenance`, `gather_provenance`, `build_sidecar`, `write_provenance`, `verify_provenance`, `cmd_provenance`), the `cmd_infra` wiring, `Ctx.finalizer_dir`, Sim `merge-base`/`diff`/`cargo tree`, and 6 new selftest cases (59 -> 65)
- `scripts/laya_deploy/laya_trainer_stack_check.py` - `load_surface`, `check_surface_binding`, `SURFACE_CASES`, the selftest loop and the docstring
- `deploy-extensions/README.md` - "Finalizer provenance (plan 14-19, WR-04)" and deployment-order step 4

## Decisions Made

See key-decisions in the frontmatter. The two that shape behaviour:

- The check sits after the account check, so "the account check is the first command" still holds.
- `write` never refuses a dirty tree. The refusal belongs to the deploy path, which keeps the offline trainer-synth gate row usable from a dirty tree.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `write` ran cargo tree before checking that the binary exists**
- **Found during:** Task 2, while building the provenance-write-refusals case.
- **Issue:** The absent-binary case was refused for the out-of-repository reason instead of its own, because the closure was derived first. That is the "refused for another reason" trap the plan warned about.
- **Fix:** `write_provenance` now checks for the binary before running cargo. The case asserts that no command at all runs.
- **Files modified:** scripts/laya_deploy/laya_trainer_ops.py
- **Committed in:** 950e702f0

**Total deviations:** 1 auto-fixed (Rule 1). **Impact:** an ordering fix inside the new code; no scope change.

## Issues Encountered

- None blocking. My own case table caught two counting mistakes before commit: the real-git list length (10, not 11), and a "no cargo call" window that looked at earlier calls.

## Flagged, not done

- Wiring the provenance check or the binding into CI (`.github/workflows`) is out of scope by prohibition. The binding runs wherever `laya_trainer_stack_check.py` runs (the trainer-synth row); the provenance check runs on the owner's deploy path.
- The sidecar beside the asset goes stale once 14-17 and 14-18 change `finalize.rs` / `finalizer.rs`. That is intended: 14-22 re-synths before the owner deploys. The 14-22 runbook should carry the two operational facts in the README (the record ships inside the package; any closure commit forces a re-synth). This plan did not edit the 14-22 plan.
- The FS-AS-01 row and the binding row in `failure-surface-v1.json` / `14-FAILURE-SURFACE.md` are left for 14-18 to close, as the plan specifies.

## User Setup Required

None. No AWS call was made and nothing was deployed.

## Next Phase Readiness

- 14-17 can run now. Its mutation runner rewrites finalizer sources, which are in the closure, so it must not run concurrently with a synth. This plan has finished, so there is no overlap.
- 14-22 must run `just laya-trainer-synth <env>` on a clean, committed closure immediately before `infra diff` and `deploy`.

## State and roadmap updates

- `roadmap.update-plan-progress 14` succeeded: Phase 14 went from 16/22 to 17/22, and 14-19 is checked.
- `state.advance-plan`, `state.record-session` and `state.add-decision` all failed with the known tooling error: `.planning/phases/04-apr-artifact-and-production-parity/04-VERIFICATION.md` has status `gaps_acknowledged`, which is outside the tool's closed set. That file was deliberately not edited, so STATE.md is unchanged by this plan.
  - The tool also warned that STATE.md's Current Position points at phase 08.
  - The decisions above are recorded here instead.
- Requirement IDs (D-01, D-02, D-12, D-14, D-22) are phase decision IDs, not REQUIREMENTS.md entries. Per the plan ("Requirement-ID probes: SKIPPED visibly"), `requirements.mark-complete` was not run.
- Windows ledger: no stub, skipped test or unrun verify was left, so nothing was appended.

## Self-Check: PASSED

- Files: the justfile, both scripts, the README and `deploy-extensions/assets/laya-finalizer/bootstrap.build.json` (untracked, gitignored) all exist.
- Commits: `a0902975d`, `950e702f0` and `c60b48745` are ancestors of HEAD. `evaluation-scope --plan 14-19 --commits-only` resolves them from plan subjects.
- Plan verification, re-run after the last task commit:
  - ops selftest: `LAYA TRAINER OPS SELFTEST OK` (65 ok);
  - stack-check selftest: `STACK CHECK SELFTEST OK` (52 cases);
  - `laya_failure_surface.py --check`: OK;
  - `LAYA_GATES_ONLY=trainer-synth,trainer-image,trainer-infra,failure-surface just laya-gates-selftest`: `LAYA GATES ROWS OK 4 of 55` (the trainer-synth row re-synthesized and ended `TRAINER SYNTH OK env=dev`);
  - `finalizer-provenance verify` on the re-synthesized asset: `FINALIZER PROVENANCE OK`;
  - `git diff --quiet HEAD -- deploy-extensions/lib contracts crates/aprender-decide .github`: rc 0.

---
*Phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out*
*Completed: 2026-10-30*
