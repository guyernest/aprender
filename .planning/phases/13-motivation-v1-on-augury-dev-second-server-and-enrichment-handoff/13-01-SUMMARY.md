---
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
plan: 01
subsystem: decide-deploy-tooling
status: complete
tags: [justfile, region, eu-west-1, positional-arguments, gates, mutation-testing, offline-selftest]
requires:
  - "the owner's uncommitted justfile region diff (D-05)"
  - "13-11 D-06 baseline (commit 98ad31476)"
provides:
  - "laya-deploy-config / laya-weights-bucket region parameter, positional and ERE-validated"
  - "LAYA_DEPLOY_TEMPLATE (DRY_RUN=1 only), LAYA_REGION_SELFTEST_TEMPLATE"
  - "_laya-region-selftest (R1-R7, BUCKET), _laya-state-snapshot"
  - "laya-deploy-selftest eligibility export-before-restore; armed_deploy_selftest freshness-checked consumer"
  - "gate rows region-selftest, weights-bucket-region, prepare-motivation-args"
  - "13-REGION-EVIDENCE.json"
affects: [13-03, 13-04, 13-07, 13-08]
tech-stack:
  added: []
  patterns:
    - "[positional-arguments] + `${N#name=}` instead of textual {{param}} interpolation"
    - "[[ =~ ]] whole-string ERE in a C-locale subshell (a newline cannot pass on its first line)"
    - "export-before-restore with an mtime marker so a restored historical file can never satisfy a gate"
key-files:
  created:
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-REGION-EVIDENCE.json
  modified:
    - justfile
    - scripts/laya_gates.tsv
key-decisions:
  - "The region check runs before _laya-server-check and before any aws call; the refusal prints only the character count"
  - "R3 and R6 follow the render's own `wrote <path>` line, so plan 13-04's move of augury renders to models/decide/augury-dev/ needs no selftest change"
  - "Recorded log lines in the evidence have the scripted fake aws's 12-digit fake account ids replaced by <fake-account> (noted in the file's redaction field)"
requirements-completed: [D-05, D-20]
duration: 42 min
completed: 2026-10-27
commits: 4
plan_head_before: 3ecf7ac29acd94607fd80b07db0be6ecffc78e36
plan_head_after: f36241a6ed170ecd0f672973324772cc1baaf057
actuals:
  tokens: 7750
  tasks: 3
  commits: 4
coverage:
  - deliverable: "Region path in laya-deploy-config (positional, validated, asserted), default render byte-identical to the pre-phase justfile"
    human_judgment: false
    verification:
      - kind: command
        ref: "Task 1 <automated> (just _laya-region-selftest; pre-phase render cmp)"
        status: pass
  - deliverable: "Refusal half: R4-R7, BUCKET argv table, positional laya-weights-bucket and laya-prepare-motivation, three gate rows"
    human_judgment: false
    verification:
      - kind: command
        ref: "Task 2 <automated> (region selftest, LAYA_GATES_ONLY=region-selftest,weights-bucket-region,prepare-motivation-args, ELIG block)"
        status: pass
  - deliverable: "Eligibility export contract between laya-deploy-selftest and row verify"
    human_judgment: false
    verification:
      - kind: command
        ref: "Task 2 ELIG block; Task 3 M9/M10/M12 and the restored LAYA_GATES_ONLY=verify run"
        status: pass
  - deliverable: "13-REGION-EVIDENCE.json with M1..M12"
    human_judgment: false
    verification:
      - kind: command
        ref: "Task 3 <automated> (REGION EVIDENCE OK 12 mutations)"
        status: pass
---

# Phase 13 Plan 01: Region Parameter, Proven Offline Summary

**The owner's `region` parameter is committed and safe. `laya-deploy-config` and `laya-weights-bucket` now take positional arguments and check the region against `^[a-z]{2}(-[a-z]+)+-[0-9]$` before any AWS call. eu-west-1 changes exactly one line of the render, and the default render is byte-identical to the pre-phase justfile's. Each of the 12 mutations of the new guard turned its named check RED.**

## Timing

- Start: 2026-10-27T23:08Z. End: 2026-10-27T23:50Z (42 min).
- Tasks: 3. Files changed: 3 (justfile, scripts/laya_gates.tsv, 13-REGION-EVIDENCE.json).

## Accomplishments

- **D-05 committed.** The owner's three-hunk diff is the starting point.
  - `laya-deploy-config` gets `[positional-arguments]`. It reads `$1..$6`, strips `env=` and `region=`, refuses a bad region as `REFUSED region: the region argument (<N> characters) is not an AWS region name` (exit 2), adds `assert cfg["aws"]["region"] == region`, and prints `  region   <r>`.
  - `LAYA_DEPLOY_TEMPLATE` is honoured only under DRY_RUN=1. In a live run it is `REFUSED template:`.
  - `laya-weights-bucket` gets the same treatment. The owner's LocationConstraint branch is kept as it was.
- **D-20 proven offline.** `_laya-region-selftest` covers:
  - R1/R2: the default render is cmp-equal to an explicit us-east-1 render.
  - R3: an eu-west-1 render differs by exactly the region line.
  - R4/R5: a template with zero or two region lines is refused, and the pre-seeded output is unchanged.
  - R6: seven malformed or injecting values are refused, including a newline and a `touch` canary. The `region=eu-west-1` form renders.
  - R7: a live-mode template is refused with zero recorder calls.
  - BUCKET: create-bucket argv in us-east-1 and eu-west-1, and zero fake-aws calls for a malformed region.
  - It ends `AWS CALLS: 0` / `REGION SELFTEST OK`, and `laya-deploy-selftest` now requires that result.
- **Argument interpolation closed for `laya-prepare-motivation`** (`"$@"`). A spaced, absent `--csv` path is now `REFUSED csv-missing`, never `unrecognized arguments`.
- **Eligibility contract (review Round 1, Codex HIGH).**
  - `laya-deploy-selftest` backs up the id's two eligibility logs and removes the live copies.
  - On EXIT it first exports what this run wrote to `models/decide/selftest/cases/eligibility-export-<id>.log` (and the upload log), then restores the historical files with `cp -p`.
  - `armed_deploy_selftest` deletes the export before the child runs and reads it only when it is newer than `$W/armed-start`. Row verify's red line names why: `no export` or `export not from this run`.
- **`_laya-state-snapshot`** is a pure sha256 listing of every persistent models/decide file.
- **Gate table.** Rows `region-selftest`, `weights-bucket-region` and `prepare-motivation-args` are added, each with a `row_*` function, plus a `bucket-absent` fake-aws mode. The `not-a-gate laya-weights-bucket` row is removed.

## Quoted results

- `REGION SELFTEST OK` (17 CASE/BUCKET lines, `AWS CALLS: 0`)
- `D-20 DEFAULT RENDER EQUALS PRE-PHASE eb33a11e4a01`. Both nested helpers that the pre-phase recipe calls (`_laya-server-check`, `_laya-iam-check`) are byte-identical at bc52bf3d and HEAD, so no worktree render was needed.
- `ELIGIBILITY EXPORT OK`
- `DEPLOY SELFTEST OK (positive dry run SKIPPED: disarmed by LAYA_DEPLOY_SELFTEST_POSITIVE=0)`, with `AWS CALLS: 0`
- `LAYA GATES ROWS OK 3 of 36` with `DRIFT CHECK OK: 38 laya recipes ..., 33 gates, all covered`
- Restored armed run: `must-pass deployed: laya-verify accepted 24a44d7e…6275a (deploy_eligible true, via the default-armed selftest)`

### Mutation table (CLAUDE.md verification rule 4; each applied in place, restored with `git checkout -- justfile`, `git diff --quiet` confirmed)

| ID | Mutation | Check | RED line (first matching) | Restored |
|----|----------|-------|---------------------------|----------|
| M1 | delete `text = text.replace(region_line, ...)` | region selftest | `FAIL CASE R3 eu-west-1: exit 1, no rendered config` | REGION SELFTEST OK |
| M2 | count `!= 1` -> `< 1` | region selftest | `FAIL CASE R5 two-region-lines: exit 0, expected the one-line refusal` | REGION SELFTEST OK |
| M3 | count `!= 1` -> `> 1` | region selftest | `FAIL CASE R4 zero-region-lines: exit 1, expected the one-line refusal` | REGION SELFTEST OK |
| M4 | weights-bucket `=` -> `!=` us-east-1 | BUCKET table | `FAIL BUCKET us-east-1: exit 0, create-bucket argv ... --create-bucket-configuration` | REGION SELFTEST OK |
| M5 | default region -> eu-west-1 | R1/R2 | `FAIL CASE R2 explicit us-east-1: exit 0, not cmp-equal to R1` | REGION SELFTEST OK |
| M6 | drop the region ERE check | R6 | `FAIL CASE R6 upper-case: exit 0, expected exit 2 REFUSED region:` (7 R6 FAILs) | REGION SELFTEST OK |
| M7 | `REGION="{{region}}"` (textual) | R6 canary | `FAIL CASE R6 canary: exit 127 ...` and `the injected touch ran` | REGION SELFTEST OK |
| M8 | prepare-motivation back to `{{args}}` | gates row prepare-motivation-args | `RED  prepare-motivation-args: must-fail spaced-csv: exit 2, expected ... 'REFUSED csv-missing'` | LAYA GATES ROWS OK |
| M9 | cleanup() loses the export copy | gates row verify (armed, real weights) | `RED  verify: must-pass deployed: no deploy_eligible true ... (no export)` | LAYA GATES ROWS OK |
| M10 | consumer `SRC=` the historical path | gates row verify (armed) | `RED  verify: must-pass deployed: no deploy_eligible true ... (export not from this run)` | LAYA GATES ROWS OK |
| M11 | template override without the DRY_RUN condition | R7 | `FAIL CASE R7 live-template: exit 97, recorder calls 0 -> 1` | REGION SELFTEST OK |
| M12 | restore before export (order swapped) | Task 2 ELIG block | `the export holds the historical log, not this run's` | ELIGIBILITY EXPORT OK (and LAYA GATES ROWS OK from the restored verify run) |

## TDD (Task 2, `tdd="true"`; `workflow.tdd_mode` is false, so the gate is advisory)

- **RED** `0f04649c3`: the R4-R7 and BUCKET cases, three gate rows and the `bucket-absent` fake mode, with no implementation.
  - `_laya-region-selftest` exited 1. R1-R7 and the two BUCKET argv cases passed, because Task 1 had already implemented them. The one target case failed on its planned assertion: `FAIL BUCKET refuse: exit 0 with 6 fake-aws call(s); expected exit 2 REFUSED region: and no call`.
  - The filtered gate sweep had `DRIFT CHECK OK`, and all three rows went RED for their named reasons: `must-fail malformed: exit 0`, `prepare_motivation.py: error: unrecognized arguments: such dir/a b.csv`, and region-selftest's default must-pass failing.
  - Semantic assessment: the failures are the intended behaviours. There was no setup, parse or fixture fault.
  - `gsd-tools check tdd-red-evidence` was not run. Its adapters cover TAP, JUnit, swift-testing, unittest and go test, and none of them reads a justfile selftest's CASE/FAIL lines. The evidence above is the raw command output.
- **GREEN** `6c9045e2e`: the implementation. The Task 2 verify passes: region selftest OK, `LAYA GATES ROWS OK 3 of 36`, the disarmed deploy selftest OK, unchanged snapshot, `ELIGIBILITY EXPORT OK`. The neighbouring rows `sha256-helper`, `server-check`, `iam-check` and `gates-selftest` also pass.
- **REFACTOR:** none.

## Task Commits

1. Task 1 (tracer): `946c3f660` feat(13-01): positional, validated region path for laya-deploy-config and the offline region selftest
2. Task 2 RED: `0f04649c3` test(13-01): failing region refusal cases, BUCKET argv table, three gate rows
3. Task 2 GREEN: `6c9045e2e` feat(13-01): validate laya-weights-bucket's region, forward prepare-motivation args whole, export eligibility before restore
4. Task 3: `f36241a6e` docs(13-01): record the region evidence and the 12-mutation table

The tracer gate ran in end-of-phase mode with an automated-only verify. The verify was re-run green before expanding.

## Deviations from Plan

**1. [Rule 1 - Side effect, recorded] The Task 1 deploy-selftest run rewrote the antisemite id's two eligibility logs before the restore existed.**
- **Found during:** Task 3's snapshot comparison.
- **Issue:** Task 1's verification included a disarmed `laya-deploy-selftest` run, at 2026-10-27T23:15:32Z to 23:17:44Z. The export/restore only arrives in Task 2, so that run left `models/decide/eligibility-aprender-mcp-decide-antisemite.log` and `eligibility-upload-aprender-mcp-decide-antisemite.log` holding the tiny fixture's `REFUSED SyntheticNotDeployable` output. This is the exact pre-existing defect that Task 2 item 7 (Fable LOW) fixes.
- **What was there before:** the earlier bytes cannot be recovered (gitignored, no backup, and the pre-run snapshot file was overwritten by a later verify run). The pre-plan listing showed both files at 2.2K, the same size as a tiny-fixture refusal. That is consistent with an earlier selftest run having written them too, but it is not proof.
- **Impact:** low. `laya-deploy` and `laya-upload` rewrite this log and then read their own fresh write. Row verify no longer reads it at all; it reads the export.
- **Runs since then:** every later run, including all 12 mutations and the three armed runs, left the snapshot byte-identical (the md5 of the 29-line listing was unchanged).
- **Commit:** none (state file, gitignored).

**2. [Rule 3 - Evidence hygiene] Fake account ids redacted in recorded log lines.** M4's FAIL line echoes the scripted fake aws's create-bucket argv, which includes the fake 12-digit account in the bucket name. The plan requires no unknown 12-digit number in the evidence. The builder replaces only the two scripted fake ids with `<fake-account>` (it asserts each replaced number is one of them), and the file's `redaction` field records this.

**3. [Rule 2 - Evidence completeness] Two evidence additions.**
- M12's `restored_line` is the restored `LAYA_GATES_ONLY=verify` run (`LAYA GATES ROWS OK`), which reads the same unmutated export. The verify schema allows only `REGION SELFTEST OK` or `LAYA GATES ROWS OK` there. The actual restored ELIG output, `ELIGIBILITY EXPORT OK`, is kept in `restored_check_line`.
- `task2_red_commit` is recorded beside the two `region_commits` (Task 1 and the Task 2 GREEN, the final tested revision).

**Total deviations:** 3 (1 recorded side effect, 2 evidence-shape). **Impact:** none on the plan's goal.

## Known Stubs

None.

## Threat Flags

None. T-13-01, T-13-02, T-13-03 and T-13-38 are mitigated as registered and are each shown RED under their own mutation (M6/M7, M11, the R6 value-not-echoed check, and M9/M10/M12).

## Deferred / Notes

- `interpolation_finding`: other laya recipes still interpolate parameters textually. This is pre-existing and out of scope, recorded in the evidence for a later phase.
- STATE/ROADMAP verbs: see below. The owner's uncommitted `.planning/STATE.md`, `config.json` and `state.json` were never staged.

## Next

Ready for the next Phase 13 plan. Plans 13-07/13-08 can render with `region eu-west-1`.

## Self-Check: PASSED

- FOUND: justfile `_laya-region-selftest`, `_laya-state-snapshot`; scripts/laya_gates.tsv `region-selftest`; 13-REGION-EVIDENCE.json (`REGION EVIDENCE OK 12 mutations`)
- FOUND commits: 946c3f660, 0f04649c3, 6c9045e2e, f36241a6e (ancestors of HEAD)
- `git diff --quiet -- justfile` holds; commits measured: `git rev-list --count 3ecf7ac29..HEAD` = 4
