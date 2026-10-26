---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 01
subsystem: laya-data-prep
tags: [laya, decide, motivation, data-split, task-json, prefix-measurement, pilot]
status: complete

requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: prepare_goldstandard.py measurement helpers, the owner-approval checkpoint shape, the thin just wrapper
provides:
  - "scripts/laya_train/prepare_motivation.py: declared nfc-trim-ws-v1 / seed-13 / 25 % split rule, --measure, --selftest, --selftest-tokenizer"
  - "data/decide/motivation-v1/{task.json,train.jsonl,eval.jsonl} (gitignored), written once"
  - "12-TASK-OWNER-REPLY.json and 12-TASK-EVIDENCE.json: the owner-approved task bytes, variant, door and measured prefix"
  - "just laya-prepare-motivation"
affects: [12-02, 12-07, 12-09, 12-11, 12-14]

actuals:
  tokens: 10421
  tasks: 3
  commits: 2
plan_head_before: 2d85bf88cb8b17eb13bef1edd9748ac1bcb7d006
plan_head_after: 9805ec4b559ec88acf60356aa92d1d058b79b525

tech-stack:
  added: []
  patterns:
    - "approved-bytes pin: the script asserts the task.json it will write hashes to the owner-approved sha256 before write_once (T-12-02)"
    - "one measurement implementation (OPS-03): measure_task / door_fields / tweet_report imported from prepare_goldstandard"

key-files:
  created:
    - scripts/laya_train/prepare_motivation.py
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-TASK-OWNER-REPLY.json
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-TASK-EVIDENCE.json
  modified:
    - scripts/laya_train/prepare_goldstandard.py
    - scripts/laya_train/README.md
    - justfile

key-decisions:
  - "Owner chose variant (a) on 2026-10-26: P 97, served door stays 8, task.json sha256 f74a3ad3cfc0 (full f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3); no decide-tool-boundary-v1 change"
  - "Variant (b) (P 190, door 4) was shown and not chosen"
  - "motivation-v1 is a PILOT on the 25 October 2026 guide draft (docx sha256 0e1d5a35...29167), to be retrained after the freeze"
  - "Eval fraction 0.25 is declared in the script (EVAL_DIVISOR = 4); laya-finetune-gate-v1 has no eval-fraction key, so D-13's contract-amendment branch did not fire"

requirements-completed: [D-01, D-09, D-10, D-13, D-16, D-23]
---

# Phase 12 Plan 01: Motivation data prep, measured task variants and the owner-approved task.json Summary

**The partner motivation CSV becomes a seed-13 stratified text-disjoint 25 % Laya data dir by a rule declared in the script's docstring, and the owner-approved short condensation of the 25 October guide (P 97, door 8, sha256 f74a3ad3cfc0) is pinned as task.json.**

## What was built

- `prepare_motivation.py`: reads the partner CSV in place, groups by `nfc-trim-ws-v1`, drops mixed-label groups (none exist), draws the eval set per class by `ceil(0.25 x groups)` with seed 13, and asserts data.py's own validators (`group_train`, `refuse_overlap`, `calibration_split`) before anything is written. Prints counts and sha256s only; refusals name a row number, a field and a character count, never a value.
- `--measure` (tokenizer-only), `--selftest` (torch-free, sentinel proof over stdout and stderr on success and every malformed-input path), `--selftest-tokenizer` (exact-48 versus 49, head cut, literal stance control P 57).
- `just laya-prepare-motivation`, a thin wrapper.
- Repaired the red `prepare_goldstandard.py --selftest-tokenizer` (stance control asserts a module literal `STANCE_CONTROL_P = 57`, not the live contract constant).

## Measured table (served Laya tokenizer, markers 3, no head cut, no option cut for both)

| Draft | P | head raw/kept | instruction room | options raw/kept (social_affiliative, conspiratorial, other) | served door | texts/call p50 / p95 | batch of 4 fits p50 / p95 / max (147) | posts truncated |
|-------|---|---------------|------------------|----------------|-------------|------------|-------|-----|
| a (chosen) | 97 | 64/64 | 160 | 10/10, 8/8, 8/8 | 8 | 5 / 4 | yes / yes / no | 0 |
| b (not chosen) | 190 | 153/153 | 156 | 12/12, 9/9, 9/9 | 4 | 3 / 2 | no / no / no | 0 |

Post tokens (395 posts): p50 45, p75 61, p90 69, p95 77, p99 108, max 147.

## Owner decision

Reply on 2026-10-26: `approve-a f74a3ad3cfc0` (verbatim). The 12 hex are the prefix of the sha256 of the exact bytes shown for (a). `12-TASK-EVIDENCE.json` records `chosen_max_texts` 8, `door_mechanism` "contract classify_max_texts 8, unchanged", the task_json_sha256 equal to the approved and the written bytes, the docx sha256 and designation `25 October 2026 draft`, and routing to Dr. Yuster as not required for the pilot (D-10). Plan 12-09 branch (a) applies (no per-artifact door).

## Data (counts only)

rows_in 395, distinct 395, duplicates 0, mixed-label groups 0. Eval {social_affiliative 23, conspiratorial 14, other 63} = 100 rows; train {66, 41, 188} = 295. Calibration slice at 0.25: 75 rows. File sha256s are in the evidence JSON; a second `just laya-prepare-motivation` printed `unchanged (byte-identical)`. The approved task re-measured under the enforced door 8: exit 0, `fits_door` true, door_total 776 of 800.

## Task commits

| Task | Commit | Content |
|------|--------|---------|
| 1 (tracer) | f83c55e4a | prepare_motivation.py, goldstandard selftest repair, justfile recipe (own hunks only), two measured drafts |
| 2 (checkpoint) | n/a | owner reply recorded in 12-TASK-OWNER-REPLY.json (committed with Task 3) |
| 3 | 9805ec4b5 | GUIDES collapsed to INSTRUCTIONS / DESCRIPTIONS, APPROVED line, approved-bytes assertion, README, both 12-TASK-* files, data dir written once |

`commits: 2` is measured from the persisted ledger (`2d85bf88c..9805ec4b5`); the docs commit that carries this SUMMARY is separate.

## Deviations from Plan

**1. [Rule 3 - Blocking] Partner repo "clean" check is unsatisfiable at baseline**
- **Found during:** Task 1 verify
- **Issue:** `git status --porcelain -- datasets/motivation/` in `motivation-experiment` shows ` D Round1_200_Posts_Classified.xlsx` plus two `??` files, all predating this session, so `test -z` can never pass.
- **Fix:** compared against the recorded 3-line baseline (`partner_state.txt`) before and after; identical both times. Nothing was written there by this plan.

**2. [Rule 1 - Bug] Draft (a) first wording measured P 116, over the cap of 100**
- **Found during:** Task 1
- **Fix:** shortened the wording to P 97 before presenting it; the owner saw and approved the P 97 bytes.

**3. [Rule 2 - Missing functionality] Added `--no-pin`**
- **Why:** the synthetic-CSV selftests and a deliberate PINNED update need to build from a CSV that is not the partner file, while the default stays pinned (T-12-03).

**4. [Rule 2 - Integrity] Added `APPROVED_TASK_SHA256` and `check_approved`**
- **Why:** the plan required asserting written bytes equal the approved sha; implemented as a constant plus a selftest case (tampered bytes refused) so the pin cannot silently drift.

**5. Task 2 record has no separate commit.** The plan lists both 12-TASK-* files in Task 3's single commit; followed that.

No auth gates. The `04-VERIFICATION.md` `gaps_acknowledged` status was not touched (see below).

## Known Stubs

None.

## Threat Flags

None. No network endpoint, auth path or trust-boundary schema change; all post text stayed out of git, logs and records (all 395 posts scanned against every file this plan writes).

## Issues Encountered

- STATE.md and ROADMAP.md updates were left to the orchestrator: the working tree carries the owner's pre-existing uncommitted STATE.md / config.json / state.json changes, and `04-VERIFICATION.md`'s `gaps_acknowledged` status makes the `gsd_run` state/roadmap queries error. Not silently re-keyed.
- The owner's uncommitted `justfile` region-parameter diff remains in the working tree untouched (`git diff HEAD -- justfile` equals the foreign patch, 25 +/- lines); the committed range from the start commit holds exactly this plan's 12 own +/- lines.

## Verification

Plan Task 1 verify, Task 2 verify (`OWNER REPLY RECORDED approve-a a`) and Task 3 verify (`TASK EVIDENCE OK variant a P 97 B 8 sha f74a3ad3cfc0`, `NO POST TEXT`, `FOREIGN JUSTFILE DIFF PRESERVED 25 lines; committed range == own hunks 12 lines`) all passed. The accuracy claim is a hypothesis: no accuracy number exists before the gate run.

## Self-Check: PASSED

- FOUND: scripts/laya_train/prepare_motivation.py, 12-TASK-OWNER-REPLY.json, 12-TASK-EVIDENCE.json, data/decide/motivation-v1/{task.json,train.jsonl,eval.jsonl}
- FOUND commits: f83c55e4a, 9805ec4b5
