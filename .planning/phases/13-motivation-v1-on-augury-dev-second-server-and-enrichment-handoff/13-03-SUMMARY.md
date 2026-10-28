---
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
plan: 03
subsystem: laya-data-prep
tags: [laya, motivation-v1, enrichment, prepare_motivation, d-09, d-10, d-11, d-12, d-15, d-16, no-text, mutation-testing]

requires:
  - phase: 13-02
    provides: "contracts/motivation-enrichment-v1.yaml: the enrichment format, the 12-cell mapping (with the pending d15_amendment), the five merge rule names, 17 refusal names, caps, patterns and character limits, manifest.filename and schema"
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: "prepare_motivation.py's v1 build, PINNED csv, APPROVED_TASK_SHA256 and the v1 data dir bytes"
provides:
  - "prepare_motivation.py --enrichment PATH (repeatable) and --enrichment-sha256 HEX (repeatable): a second source merged into train only"
  - "check_v1_identity: every pinned run must build v1's task/train/eval bytes (train exempt only when enriched)"
  - "read_enrichment / merge_enrichment / _build_v1 / _synthetic_enrichment and the contract-mirrored constants"
  - "enrichment-manifest.json (schema motivation-enrichment-manifest-v1): counts and sha256 values only"
  - "13-INGEST-EVIDENCE.json (schema laya-ingest-evidence-v1): v1 pins, 147-case selftest, the pinned-CSV fixture merge, N1-N9"
affects: [13-04, 13-05, 13-10]

actuals:
  tokens: 15900
  tasks: 3
  commits: 4
plan_head_before: 0cd862df392199356bd2a9b895bb9728117b5013
plan_head_after: ddb80123b6d5474fb132af9939e743062a4cfacb

tech-stack:
  added: []
  patterns:
    - "Refactor without changing bytes: _build_v1 keeps build()'s statements in order and returns the merge context beside the unchanged bytes; build() wraps it"
    - "Partner files are identified by generated ids (source-<k>) and sha256, never by file name"
    - "A refusal raised by a reused helper is wrapped into the declared name set (refuse_unencodable -> enrichment-encoding), and a coverage case fails the selftest when any name outside the 17 escapes"
    - "Boundary cases read their limits from the loaded contract, so a changed contract value moves the test"

key-files:
  created:
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-INGEST-EVIDENCE.json
  modified:
    - scripts/laya_train/prepare_motivation.py

key-decisions:
  - "check_v1_identity runs twice in a pinned enriched run: on the base build (all three files must be v1's) and on the merged files (task and eval must still be v1's). The plan asked for one call; the base-build call is stricter"
  - "Header field failures (format, artifact_sha256, guide type or length, keys) are enrichment-header and row field failures are enrichment-field. The plan's boundary list requires a 201-character guide to be enrichment-header"
  - "The manifest's per-source cells list the 9 served (label, evidence) cells. A refused pair aborts the run, so its count could only ever be 0"
  - "collapsed_same_label counts the enriched rows collapsed away: all of a group's rows when the partner has the text, else all but the first"
  - "The label-evidence refusal names the row and 'fields label and evidence' but not the pair's values, following the no-text rule strictly even though the values are enum constants"

patterns-established:
  - "Mutation driver: exact-substring edit asserted to match once, restore with git checkout -- in finally inside a bash EXIT trap, git diff --quiet after each restore, red and restored lines recorded with the sentinel scrubbed"

requirements-completed: [D-09, D-10, D-11, D-12, D-15, D-16, D-03]

coverage:
  - id: D1
    description: "A run without --enrichment writes exactly v1's bytes, enforced by check_v1_identity in every pinned run; the no-flag run reports unchanged (byte-identical)"
    requirement: D-10
    verification:
      - kind: integration
        ref: "13-03-PLAN.md Task 1 <automated> (no-flag run: unchanged (byte-identical), PREPARE OK)"
        status: pass
      - kind: other
        ref: "13-INGEST-EVIDENCE.json mutations N1 (seed+1 -> PREPARE FAILED naming eval.jsonl; restored no-flag run unchanged)"
        status: pass
    human_judgment: false
  - id: D2
    description: "--enrichment merges synthetic expert rows into train only; eval.jsonl and task.json stay byte-identical on the pinned CSV; the manifest holds counts and sha256 values only"
    requirement: D-11
    verification:
      - kind: integration
        ref: "13-03-PLAN.md Task 1 <automated> (TRACER MERGE OK; cmp eval.jsonl/task.json against data/decide/motivation-v1)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The declared rule set: eval overlap, conflict groups dropped whole with no precedence, non-resurrection, the D-15 mapping and its refusals, annotation_mode never acted on, every one of the 17 refusals text-free, the contract mirror and the boundaries"
    requirement: D-12
    verification:
      - kind: unit
        ref: "uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --selftest (147 ok, 0 FAIL)"
        status: pass
      - kind: other
        ref: "13-03-PLAN.md Task 2 <automated>; 13-INGEST-EVIDENCE.json N2-N9 each RED on its named case"
        status: pass
    human_judgment: false
  - id: D4
    description: "The ingest behaves correctly on the partner's real enrichment file"
    requirement: D-09
    verification: []
    human_judgment: true
    rationale: "No real enrichment file exists yet (D-09). The ingest is proven on synthetic fixtures only. The first real file arrives with the partner's expert round, and the d15_amendment is still pending owner confirmation in 13-10"

duration: 13min
completed: 2026-10-28
status: complete
---

# Phase 13 Plan 03: The Second-Source Enrichment Ingest Summary

**`prepare_motivation.py --enrichment PATH` now merges an expert's motivation-enrichment-v1 file into train.jsonl only. It refuses in-repo paths (both sides resolved, so symlinks are followed) and drops eval overlaps and multi-label groups whole, with no source precedence. It maps (label, evidence) to the three served labels exactly as the contract declares and records annotation_mode without acting on it. It writes a counts-and-sha256-only manifest under generated `source-<k>` ids. `check_v1_identity` proves in every pinned run that a run without the flag still writes v1's exact bytes.**

## Performance

- **Duration:** about 13 min
- **Started:** 2026-10-28T00:04:34Z
- **Completed:** 2026-10-28T00:17:30Z
- **Tasks:** 3
- **Files modified:** 2 (1 modified, 1 created)

## Accomplishments

- **D-10 identity is checked, not assumed.** The no-flag status line is `prepared data/decide/motivation-v1 (unchanged (byte-identical))`, with task.json `f74a3ad3...` (9 lines), train.jsonl `f1d84f19...` (295 lines), eval.jsonl `ee95d71f...` (100 lines) and `PREPARE OK`. Mutation N1 (split seed + 1) turns the pinned run into `PREPARE FAILED` naming eval.jsonl.
- **D-11 fixture merge on the real pinned CSV**, read in place from the partner repo: a 4-row synthetic file gave exit 0 with `PREPARE OK`. eval.jsonl and task.json were `cmp`-identical to data/decide/motivation-v1, v1's train bytes were a prefix of the new train.jsonl with exactly 4 rows added, and the planted sentinel was absent from the log and the manifest. The train_counts were conspiratorial 42, other 190 and social_affiliative 67.
- **Selftest: 147 ok cases and 0 FAIL** (37 v1 cases unchanged and 110 new), ending `PREPARE MOTIVATION SELFTEST OK`. Each of the 17 refusal names is produced through `main()` with exit 2, the row and field named and no sentinel in either stream, and the coverage case confirms no other name escapes.
- **Bound to the contract:** the selftest loads `contracts/motivation-enrichment-v1.yaml` and compares 20 values: refusals, header and row keys, the format const, every enum, the 12-cell mapping, caps, the patterns, the text and guide limits, `unique`, the merge rule names, the v1 pins and `manifest.filename` and schema. The boundaries are read from it: post_id 128 and 129 characters, text 20000 and 20001, guide 200 and 201, rows 20000 and 20001, and file bytes 16777216 and 16777217.

## Mutation table (13-INGEST-EVIDENCE.json)

| Id | Edit | Check | RED line (sentinel scrubbed) | Restored |
|----|------|-------|------------------------------|----------|
| N1 | `RandomState(SPLIT_SEED)` -> `+ 1` in the v1 build | no-flag pinned run, `--out <tmp>` | `PREPARE FAILED: the pinned CSV no longer builds v1's bytes (D-10): eval.jsonl (7191f414576a, not v1's ee95d71fcb5d); train.jsonl (...)` | SELFTEST OK; no-flag run `unchanged (byte-identical)` |
| N2 | skip the eval-overlap drop | D-11 case | `FAIL dropped_eval_overlap: ... exit 2 eval-train-overlap` | SELFTEST OK |
| N3 | keep the partner row of a conflict group | D-12 case | `FAIL dropped_conflict_groups: a partner train text under another mapped label drops the group, partner row included` | SELFTEST OK |
| N4 | drop rows when annotation_mode is blind | D-16 identical-train case | `FAIL annotation_mode: the two fixtures give byte-identical train.jsonl` | SELFTEST OK |
| N5 | let a partner mixed-group match through | non-resurrection case | `FAIL dropped_partner_mixed_group: both synthetic-CSV mixed groups stay dropped, whatever the enriched label` | SELFTEST OK |
| N6 | remove `enrichment-out` from ENRICHMENT_REFUSALS | contract mirror | `FAIL contract mirror: refusal names (17, in order)` | SELFTEST OK |
| N7 | call `refuse_unencodable` unwrapped | every-refusal-rule coverage | `FAIL every refusal rule: ... (18) and no other missing [], outside ['enrichment-row-encoding']` | SELFTEST OK |
| N8 | print and record the basename instead of `source-<k>` | file-name sentinel case | `FAIL file name: fixtures whose FILE NAME carries the sentinel succeed with it absent ...` | SELFTEST OK |
| N9 | `dropped_conflict_rows_partner` stays 0 | D-12 partner-row drop manifest case | `FAIL dropped_conflict_rows_partner 1 (partner-row drop manifest): ...` | SELFTEST OK |

Each mutation was an exact-substring replacement asserted to match exactly once. The script was restored with `git checkout -- scripts/laya_train/prepare_motivation.py` inside `finally` under a bash EXIT trap, and `git diff --quiet` was confirmed after every restore and at the end. The plan's coupling note holds: no `just laya-gates-selftest` ran while a mutation was applied.

**The one transient copy:** Task 1's verify writes the merged train.jsonl (v1's partner-derived rows plus synthetic rows) to a `mktemp -d` directory outside the repository and deletes it in the same command. Nothing was copied into the repository. The partner repo `/Users/guy/projects/mcp/motivation-experiment` had the same HEAD (`d822f119...`) and the same `status --porcelain` before and after Tasks 1 and 3.

## Task Commits

1. **Task 1 (tracer) RED:** `29c1f0ed0` test(13-03): add failing tracer cases for the enrichment ingest and check_v1_identity
2. **Task 1 (tracer) GREEN:** `66ed0f4ec` feat(13-03): --enrichment ingest into train only, with check_v1_identity guarding the no-flag bytes
3. **Task 2:** `ee9efcacd` test(13-03): exercise the whole enrichment rule set, text-free and bound to the contract
4. **Task 3:** `ddb80123b` docs(13-03): record the ingest evidence and the N1-N9 mutation table

The tracer gate is row 3 (interactive, `end-of-phase`, automated-only verify). The Task 1 verify was re-run green at `66ed0f4ec` before Task 2 started.

## TDD (`tdd="true"` on Tasks 1 and 2; `workflow.tdd_mode` is false, so the gate is advisory)

- **Task 1 RED** `29c1f0ed0`: the selftest exited 12, with the 37 existing cases passing and the 12 new cases failing.
  - Nine failed on their planned assertions. The main() paths exited 2 because argparse rejected `--enrichment`, so they showed no `REFUSED enrichment-out` or `enrichment-in-repo` line and no merged files.
  - The three direct `check_v1_identity` cases failed on `NameError`, caught and reported as FAIL.
  - Semantic assessment: the failures are the intended missing behaviour. There was no fixture or parse fault.
  - `gsd-tools check tdd-red-evidence` was not run: its adapters (TAP, JUnit, swift-testing, unittest, go test) do not read this selftest's `ok`/`FAIL` lines, the same limitation 13-01 recorded.
- **Task 1 GREEN** `66ed0f4ec`: the selftest passes (49 ok at that commit), and the Task 1 verify passes on the real pinned CSV.
- **Task 2:** this was an unexpected GREEN, and investigating it showed it was expected. The 98 cases added in Task 2 passed against Task 1's implementation with no fix needed, because Task 1's action items 4 and 5 already implement the full rule set. So Task 2 is test-only (`test(13-03)`, no `feat`). The RED proof for these cases is Task 3: N2 to N9 each turn a named Task 2 case into a FAIL.
- **REFACTOR:** none.

## Files Created/Modified

- `scripts/laya_train/prepare_motivation.py`
  - the five RULE enrichment-* entries plus RULE v1-identity in the docstring, and the usage line;
  - the contract-mirrored constants;
  - `_build_v1`, `check_v1_identity`, `read_enrichment`, `merge_enrichment`, `_synthetic_enrichment` and `_enrichment_cases`;
  - the `--enrichment` and `--enrichment-sha256` flags and the per-source and totals print lines.
- `.planning/phases/13-.../13-INGEST-EVIDENCE.json`: schema `laya-ingest-evidence-v1` (created)

## Decisions Made

See `key-decisions` in the frontmatter. The implementation follows the contract as committed, including its pending `d15_amendment`:
- the evidence level `none` is accepted;
- other+clear, social_affiliative+none and conspiratorial+none are refused as `enrichment-label-evidence`;
- other+none maps to other.

If the owner changes the amendment in 13-10, that is a motivation-enrichment-v2, and the contract-mirror case fails until this script follows it.

## Deviations from Plan

### Auto-fixed Issues

None. The plan was executed as written, with these small additions inside its scope:

- **[Additive] check_v1_identity on the base build as well as the merged files.** Any pinned run proves first that the base is v1's bytes and then that the enriched task and eval still are. The plan's single call is a subset of this. Commit `66ed0f4ec`.
- **[Additive] A few extra refusal paths** that the contract's refusal meanings imply:
  - `--enrichment-sha256` with a count different from `--enrichment`, or without `--enrichment`, is refused as `enrichment-sha256`;
  - `--enrichment` together with `--measure` is an argparse error;
  - a directory passed as `--enrichment` is refused as `enrichment-missing`, since it is not a regular file;
  - a zero-byte file is refused as `enrichment-header` (row 0 missing).
- **[Additive] `train_rows` and `format` in the manifest.** Both are counts or constants, and the partner-row drop case reads `train_rows`.
- **[Environment] The plan's verify blocks ran under `bash`** from script files, with their `/tmp/p13-03-*` scratch files redirected to the session scratchpad. The Bash tool's shell is zsh. The commands are otherwise verbatim.

**Total deviations:** 0 auto-fixed. **Impact:** none on scope. All additions sit inside the declared 17 refusal names and the contract.

## Issues Encountered

- STATE.md and ROADMAP.md verbs follow the 13-01 and 13-02 convention:
  - `roadmap.update-plan-progress 13` runs after this SUMMARY is committed.
  - The STATE.md verbs are deliberately not run, because STATE.md carries the owner's uncommitted edit, which the dispatch says never to touch.
  - `requirements.mark-complete` is skipped, because the D-ids are CONTEXT decision ids, not REQUIREMENTS.md ids.
- The justfile and laya_gates.tsv were not touched (13-04 owns them in this wave). No gates sweep ran while a mutation was applied.

## User Setup Required

None. No external service configuration is required.

## Next Phase Readiness

- When the partner's expert round returns, the retrain phase runs one command: `just laya-prepare-motivation --enrichment "<partner path>" [--enrichment-sha256 <hex>] --out data/decide/motivation-v2`. The eval stays comparable with motivation-v1's numbers.
- Plan 13-10 has two items:
  - It must put the `d15_amendment` to the owner.
  - Its `enrichment-manifest-compat` fixture suite must check that laya-pack-fixture and laya-verify behave the same with `enrichment-manifest.json` in the data dir. train.py reads only named files.
- With `annotation_mode: prediction_shown`, the hand-off of any retrain must say that its labels may be anchored to motivation-v1's predictions (D-16, the contract's `anchoring`).

## Self-Check: PASSED

- FOUND: scripts/laya_train/prepare_motivation.py, .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-INGEST-EVIDENCE.json
- FOUND on this branch: 29c1f0ed0, 66ed0f4ec, ee9efcacd, ddb80123b (4 commits measured from the ledger, 0cd862df3..ddb80123b)
- The Task 1, 2 and 3 `<automated>` verifies all exit 0 at ddb80123b. The selftest shows 147 ok and 0 FAIL. `git diff --quiet` passes on the script.
