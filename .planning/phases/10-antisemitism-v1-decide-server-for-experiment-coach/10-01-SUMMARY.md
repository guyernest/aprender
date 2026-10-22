---
phase: 10-antisemitism-v1-decide-server-for-experiment-coach
plan: 01
subsystem: ml-data-prep
tags: [laya, decide, goldstandard, task-json, prefix-budget, served-tokenizer, data-split, owner-checkpoint]

requires:
  - phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
    provides: data.py validators, the served-tokenizer build_sequence method, the decide-tool-boundary-v1 door constants, laya-gates-selftest
provides:
  - "scripts/laya_train/prepare_goldstandard.py: the declared GoldStandard2024 split rule, the owner-approved task (INSTRUCTIONS, DESCRIPTIONS), --measure with --max-texts, --selftest, --selftest-tokenizer"
  - "data/decide/goldstandard-2024/{task.json,train.jsonl,eval.jsonl} (gitignored, no shift.jsonl), written once, byte-identical on re-run"
  - "10-TASK-EVIDENCE.json: chosen_max_texts 8 beside measured P 86, retention, sha256s, counts, paper citation and fetched-bytes sha256, the owner's approval"
  - "just laya-prepare-goldstandard and the prefix-budget gate row (door-independent) in laya-gates-selftest"
affects: [10-03, 10-04, 10-07, 10-08, 10-09, 10-10, 10-11]

actuals:
  tokens: 11331
  tasks: 3
  commits: 2
plan_head_before: 624372c39a6d7246be0b886e9acffe5e7d7c76ef
plan_head_after: 0603585102da99b4cdfefde8d35e6f9d14c09430

tech-stack:
  added: []
  patterns:
    - "the served door is a PARAMETER of the measurement; a refusal needs an explicit --max-texts, so gate rows assert only facts of P"
    - "a write-once data dir whose rule is declared in the docstring before the run, with a pinned CSV hash and counts so a changed file fails loudly"
    - "no-text promise proven by a synthetic-sentinel selftest over captured stdout and stderr on the success and malformed-input paths"

key-files:
  created:
    - scripts/laya_train/prepare_goldstandard.py
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-TASK-EVIDENCE.json
  modified:
    - scripts/laya_train/README.md
    - justfile
    - scripts/laya_gates.tsv

key-decisions:
  - "Owner reply (2026-10-21) `door-8 p90`: the exact p90 bytes are the task, not_antisemitic first; the door stays at 8 (chosen_max_texts 8), so no contract, runtime, ledger or documentation change is owed by plan 10-11"
  - "The owner did not ask to redraft from arXiv 1910.01214: the text paraphrases the IHRA examples with the 2304.14599 labelling rules; 2304.14599 cites the 12-paragraph guideline separately (1910.01214, not fetched). Recorded in the docstring and the evidence"
  - "A fuller guide is a hypothesis measured only by the gate run; the owner chose the draft that keeps door 8 (D-07)"
  - "approved_text.sha256 is the sha256 of the task.json bytes rendered from the approved strings, equal to task_json_sha256 (the bytes train.py binds as inputs_sha256.task_json)"

patterns-established:
  - "One task per script: INSTRUCTIONS and DESCRIPTIONS constants replace the draft dict and its selector once the owner approves"

requirements-completed: [SC-1, D-02, D-03, D-04, D-05, D-06, D-15]

coverage:
  - id: D1
    description: "prepare_goldstandard.py writes data/decide/goldstandard-2024 from the partner CSV by the declared rule (nfc-trim-ws-v1 groups, 5 mixed-label groups dropped, seed-13 stratified group-disjoint 20 percent eval) and the dir passes data.py's own validators"
    requirement: D-03
    verification:
      - kind: unit
        ref: "prepare_goldstandard.py --selftest (PREPARE SELFTEST OK, no FAIL line)"
        status: pass
      - kind: integration
        ref: "just laya-prepare-goldstandard twice: first 'written', second 'unchanged (byte-identical)', PREPARE OK; t3_check.py counts: 10,639 texts, 5 mixed groups, 667 duplicates, eval = ceil(0.2 x class)"
        status: pass
    human_judgment: false
  - id: D2
    description: "task.json lists not_antisemitic first and carries the owner-approved exact text; its prefix is measured with the served tokenizer (P 86, no head or option cut) under the enforced door of 8"
    requirement: D-05
    verification:
      - kind: unit
        ref: "prepare_goldstandard.py --selftest-tokenizer (PREPARE TOKENIZER SELFTEST OK: exact-48 kept, 49 cut, stance P 57 with 3 markers)"
        status: pass
      - kind: other
        ref: "prepare_goldstandard.py --measure --task data/decide/goldstandard-2024/task.json --csv <partner csv> --max-texts 8 (P 86, fits_door true, exit 0)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The owner approved the exact text and the door at a blocking checkpoint before any training; the approval, the paper citation and fetched-bytes sha256 are recorded together"
    requirement: D-06
    verification:
      - kind: other
        ref: "10-TASK-EVIDENCE.json owner_reply 'door-8 p90', owner_approved_on 2026-10-21, guide_source.fetched_text_sha256 (t3_check.py TASK EVIDENCE OK)"
        status: pass
    human_judgment: true
    rationale: "The task text is an accuracy-relevant human choice; the evidence records it, it cannot judge it"
  - id: D4
    description: "No tweet text printed, committed or recorded; no shift.jsonl; the partner repo untouched"
    requirement: SC-1
    verification:
      - kind: unit
        ref: "--selftest sentinel cases (success, header, Biased 2, empty Text, missing file) over stdout and stderr"
        status: pass
      - kind: other
        ref: "first-50-texts sample absent from the script, README, evidence and logs; partner datasets/antisemite/ git status empty; no GoldStandard2024.csv tracked here"
        status: pass
    human_judgment: false
  - id: D5
    description: "The prefix-budget gate row holds at every served door and LAYA_GATES_ONLY=prefix-budget just laya-gates-selftest is green after the --guide removal"
    requirement: D-15
    verification:
      - kind: integration
        ref: "LAYA_GATES_ONLY=prefix-budget just laya-gates-selftest (DRIFT CHECK OK, four cases, ROW prefix-budget: GREEN, LAYA GATES ROWS OK 1 of 30)"
        status: pass
    human_judgment: false

duration: not captured
completed: 2026-10-21
status: complete
---

# Phase 10 Plan 01: GoldStandard2024 data dir, approved task text and measured prefix Summary

**The partner CSV becomes a seed-13, stratified, text-group-disjoint Laya data dir by a rule declared before the run, the owner-approved p90 guide is fixed as the single task (P 86, door unchanged at 8), and `10-TASK-EVIDENCE.json` records `chosen_max_texts` 8 beside the measured prefix for every later plan.**

## Performance

- **Duration:** not captured at plan start (Task 1 and Task 3 ran in separate contexts around a blocking owner checkpoint; Task 3 about 15 min)
- **Completed:** 2026-10-21
- **Tasks:** 3 (Task 2 is the owner checkpoint and produced no commit)
- **Commits:** 2 (measured from the plan ledger: `02a1689ce`, `060358510`)
- **Files:** 2 created in git (`prepare_goldstandard.py`, the evidence file), 3 modified (`README.md`, `justfile`, `laya_gates.tsv`)

## Accomplishments

- **The rule, declared and applied.** `prepare_goldstandard.py` reads the pinned partner CSV in place (sha256 `79f4d0eb...992f`, 11,311 rows), groups under `nfc-trim-ws-v1` through `data.normalize`, drops the 5 mixed-label groups (13 rows), and draws a seed-13, label-stratified, group-disjoint 20 percent eval. A changed CSV exits 1 unless `--no-pin`.
- **The data dir** `data/decide/goldstandard-2024/` (gitignored, no `shift.jsonl`) was written once; the second run reported `unchanged (byte-identical)`.
- **The approved text is the single task.** The draft dict and `--guide` are gone; `INSTRUCTIONS` and `DESCRIPTIONS` hold the exact owner-approved strings, `not_antisemitic` first, and `--measure` without `--task` measures them.
- **Evidence in one place.** `10-TASK-EVIDENCE.json` (schema `laya-task-evidence-v2`) carries `chosen_max_texts: 8`, `door_choice: keep`, the measured prefix with raw equal to kept counts, the texts-per-call table, the file hashes and counts, the paper citation with the fetched-bytes sha256, and the owner's approval.

### The owner's choice (Task 2, 2026-10-21)

Reply `door-8 p90`: approved draft p90, door unchanged at 8. Approved exact text, `not_antisemitic` first:

- instructions: `Is this tweet antisemitic by the IHRA definition? Antisemitic: hostile stereotypes of Jews, Jewish power or conspiracy myths, Holocaust denial, blaming Jews collectively. Not antisemitic: criticism of Israel, or reporting antisemitism.`
- not_antisemitic: `no hostility toward Jews as Jews`
- antisemitic: `hostile to Jews or an antisemitic trope`

The owner did not ask for a redraft from arXiv 1910.01214. The text paraphrases the IHRA examples with the 2304.14599 labelling rules, and 2304.14599 cites the 12-paragraph guideline separately (1910.01214, not fetched). A fuller guide is a hypothesis; the owner chose the draft that keeps door 8, so no `decide-tool-boundary-v1` bump is owed.

### Measured table (Task 1 drafts, served tokenizer, 2026-10-21; never estimates)

| Draft | P | head raw/kept | options raw/kept | max_texts_that_fit | texts/call p50 / p95 | state room | posts truncated |
|-------|---|---------------|------------------|--------------------|----------------------|------------|-----------------|
| p44 | 44 | 21/21 | 10/10, 7/7 | 8 | 8 / 5 | 468 | 0 |
| p60 | 59 | 30/30 | 10/10, 13/13 | 8 | 6 / 5 | 453 | 0 |
| **p90 (approved)** | **86** | 54/54 | 12/12, 14/14 | 8 | 5 / 4 | 426 | 0 |
| pfull | 193 | 161/161 | 12/12, 14/14 | 4 | 3 / 2 | 319 | 24 |

Approved task re-measured against the written file with `--max-texts 8` enforced: P 86, markers 2, no head or option cut, 8 x 86 = 688 <= 800, instruction room 161, `p_cap` 196, tweet tokens p50 56 / p95 94 / max 366 over n 11,311, 0 posts truncated, 1.7 percent of tweets fit P + t <= 100. Control: the stance task measures 57 with 3 markers (`--selftest-tokenizer`).

### Identity and counts

- `task.json` sha256 `33fd8d7cd51b1b2864033ee313d30ae445e76120cc2b26e3849369c0602248e3` (equal to `task_json_sha256` in the evidence and to `shasum -a 256` of the file); `train.jsonl` `81a923d4b1b05bfd809e1c04534f714c2bc57c3d3a5e11e33c79fa44f7a79a9e`; `eval.jsonl` `f4a0cf7c2ea6d2ebce8580a82287410daa4106688d3e224e081a04fe141b38a8`.
- `train.jsonl` 8,510 lines (not_antisemitic 7,016, antisemitic 1,494); `eval.jsonl` 2,129 lines (1,755 and 374). Total 10,639 distinct kept texts; 667 duplicates collapsed; 5 mixed-label groups (13 rows) dropped. Eval per class equals ceil(0.2 x class total) (1,755 and 374).
- Paper: Jikeli, Karali, Miehling, Soemer, arXiv 2304.14599, fetched 2026-10-21, 357,202 bytes, sha256 `75fbaabdba7f759542a30f0f9dfdf76fe06ddb552f69ba68177ea1f43921ba30`.

## Task Commits

1. **Task 1 (tracer): prepare_goldstandard.py end to end, the four measured drafts, the prefix-budget gate row** - `02a1689ce`
2. **Task 2 (checkpoint): owner decision** - no commit; reply `door-8 p90`
3. **Task 3: approved text as the single task, data dir, evidence, README** - `060358510`

## Deviations from Plan

### Auto-fixed Issues

None - plan executed as written. `--measure` with no `--task` now measures the approved task (the plan removes the selector without naming a default; this is the minimal replacement).

### Handled Tooling Refusals (not code deviations)

**1. State verbs refused by an unrelated report status**
- **Found during:** state update
- **Issue:** `gsd-tools` `state.advance-plan`, `state.record-metric`, `state.add-decision` and `state.record-session` refuse because `04-VERIFICATION.md` carries `status: gaps_acknowledged`, which is outside the tool's closed set (known issue; that report was deliberately not re-keyed).
- **Handling:** `roadmap.update-plan-progress` works and was used for ROADMAP.md; STATE.md was edited by hand, minimally (see the final docs commit).

## Issues Encountered

- The Bash tool's `rtk` hook rewrites `grep` and `ls` output; exact-output checks used `rtk proxy` or `python3 -I`. One verify chain ended in a `grep -c` that matched zero lines, which exits 1 by grep's own convention; every assertion before it had passed.

## Known Stubs

None. No placeholder text or unwired values in the files created.

## Threat Flags

None. The plan's threat register (T-10-01 .. T-10-05, T-10-46) is covered: counts and hashes only in every output, the sentinel selftest passes, the pinned CSV hash and counts guard the source, the partner repo is clean, and the gate row asserts only door-independent facts.

## Next Phase Readiness

- Plans 10-03, 10-04, 10-07, 10-08, 10-09, 10-10 and 10-11 read `chosen_max_texts` (8) and `measured.P` (86) from `10-TASK-EVIDENCE.json`. Plan 10-04 binds `task_json_sha256` as the expected `inputs_sha256.task_json`.
- Plan 10-11 owes no door edit because the owner kept 8.
- No accuracy number exists yet; the first one is the plan 10-04 gate run (hypothesis until then, D-07).

## Self-Check: PASSED

Checked after writing: created files exist (script, evidence, SUMMARY, data dir with exactly three files); commits `02a1689ce` and `060358510` are ancestors of HEAD; the task.json hash above matches `shasum -a 256` of the file and the evidence field.
