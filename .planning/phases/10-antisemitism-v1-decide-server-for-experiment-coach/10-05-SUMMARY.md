---
phase: 10-antisemitism-v1-decide-server-for-experiment-coach
plan: 05
subsystem: testing
tags: [setfit, comparison-cell, adapter, attested-data, blocked, sc-4]

requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: "plan 10-01: the Laya data dir (task.json, train.jsonl 8510 rows, eval.jsonl 2129 rows); plan 10-04: the shipped Laya artifact the cell is compared beside"
provides:
  - "laya_dir_to_canonical: Laya {text,label} dir -> attested two-label canonical dir through the crate's own doors (adapter + 13 tests)"
  - "scripts/laya_train/setfit_cell_e1b16_cpu.json: the reference e1b16 CPU recipe at seed 13, proven equal to SetFitTrainConfig::reference_defaults(13)"
  - "just laya-setfit-cell + the setfit-cell gate row (select -> train -> eval validation -> eval test), unrun end to end"
  - "10-SETFIT-CELL-EVIDENCE.json: the BLOCKED outcome with the measured memory table; no SetFit number"
affects: [10-06, 10-09, SC-4, D-11]

# actuals: tokens is chars/4 over the files this plan changed (estimate 108000 for 3 tasks)
actuals:
  tokens: 36000
  tasks: 1
  commits: 4
plan_head_before: d4ea9847c39536b2d5ee4b1e9b55196926b6e49b
plan_head_after: 018199dae38476910ff8bc395685b5e2b4386be3

tech-stack:
  added: []
  patterns:
    - "a pure bytes-in/bytes-out adapter (adapt) with IO only in run(): every hash is computed by the crate, never by the adapter"
    - "criteria read through an ordered map visitor so the label index is document order, not lexical order"
    - "a rehearsal at s8 on the pinned checkout found the model-dir and memory problems in minutes instead of after a 30 minute run"

key-files:
  created:
    - crates/aprender-contrastive-data/examples/laya_dir_to_canonical.rs
    - scripts/laya_train/setfit_cell_e1b16_cpu.json
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-SETFIT-CELL-EVIDENCE.json
  modified:
    - crates/apr-cli/src/commands/setfit_train.rs
    - justfile
    - scripts/laya_gates.tsv

key-decisions:
  - "The owner chose C at the blocked-cell checkpoint (2026-10-22): record the comparison as blocked, hold SC-4 open, do not run the cell, do not change apr eval, no cloud"
  - "The evidence adds a third branch the plan's verify does not have: outcome 'blocked: ...' with failure_class implementation. The plan's verify accepts only 'completed' and 'refused (domain-refusal)'; the blocked branch was checked with an equivalent assertion block"

requirements-completed: []
requirements-partial: [SC-4, D-11]

coverage:
  - id: D1
    description: "The adapter turns the Laya splits into an attested canonical dir whose test split IS the Laya eval set (2129 rows, equal normalized-hash sets), validation carved from train only"
    requirement: "D-11"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-contrastive-data --example laya_dir_to_canonical: 13 passed, including label_order_is_document_order_not_lexical_order and source_split_equals_the_role"
        status: pass
      - kind: other
        ref: "python check: SAME EVAL SPLIT OK test 2129 train 7658 validation 852; apr data select and apr setfit train --dry-run exit 0"
        status: pass
    human_judgment: false
  - id: D2
    description: "The e1b16 cell config equals the reference recipe at seed 13 through the production parse door"
    requirement: "D-11"
    verification:
      - kind: unit
        ref: "crates/apr-cli/src/commands/setfit_train.rs#goldstandard_cell_config_equals_the_reference_recipe"
        status: pass
    human_judgment: false
  - id: D3
    description: "A like-for-like SetFit number beside the Laya numbers (SC-4)"
    requirement: "SC-4"
    verification:
      - kind: other
        ref: "NOT MET: apr eval exceeds host memory on the 852-row validation split (killed at a 90 GB footprint on a 48 GB host); no cell was run"
        status: fail
    human_judgment: true
    rationale: "The owner decided C (hold SC-4 open); whether SC-4 is later met by a fixed apr eval or a larger-memory host is the owner's call"

duration: about 6h elapsed across two interruptions (a machine reboot wiped /tmp and the scratchpad)
completed: 2026-10-22
status: partial
---

# Phase 10 Plan 05: SetFit comparison cell Summary

**The plan's goal, a like-for-like SetFit number beside the Laya gate numbers, was NOT met: the adapter, the reference config and the cell recipe are built and verified up to the encoder load, but `apr eval` needs more memory than this 48 GB host has for the 852-row validation split, so the cell was never run. No SetFit number exists and none is claimed. The owner held SC-4 open (decision C, 2026-10-22).**

## Outcome of record

`10-SETFIT-CELL-EVIDENCE.json`: outcome `blocked`, `failure_class` `implementation` (NOT domain-refusal: no lifecycle refusal was ever observed because training and test evaluation were not reached), `sc4_status` `refused-open` (the plan's vocabulary for "comparison incomplete, SC-4 held open"; the cause here is a blocked implementation), `sc4_owner_decision` C, `sc4_decided_on` 2026-10-22.

What was performed and measured:

| Item | Result | How measured |
|---|---|---|
| ADAPTED line | `train=[6314,1344] validation=[702,150] test=[1755,374]`, `dataset_fingerprint=6324c535...928abe`, labels `not_antisemitic,antisemitic` | the adapter's own stdout |
| Same eval split | test 2129 rows, normalized-hash set equal to `eval.jsonl`; train 7658 + validation 852 = the 8510 Laya train rows; validation disjoint from test | python with the Laya `data.normalized_sha256` |
| Config | `setfit_cell_e1b16_cpu.json`, sha256 `19f7b10bd418f2047570c7f02ee7c7b7bcec89252b95c81938d191f1350212a9`, equal to `reference_defaults(13)` | test `goldstandard_cell_config_equals_the_reference_recipe` ok |
| Select | exit 0, semantic hash `923a4afca4707336b30d1c8fcea6030f687744df3726b55f6d9d51d50123cf7c`, 0 rows excluded | `apr data select --shots 64 --seed 13` |
| Dry run | exit 0, wrote nothing; scope is request, config, attested boundary and selection replay only | `apr setfit train --dry-run` |
| Gate row | `LAYA GATES ROWS OK 1 of 31`, `ROW setfit-cell: GREEN`; the FULL-tier cell case is "not run" and the sweep line names it | `LAYA_GATES_ONLY=setfit-cell just laya-gates-selftest` |
| s8 rehearsal train | exit 0, 28 s, 9.0 GB peak | `/usr/bin/time -l` |

### The blocker (measured)

`/usr/bin/time -l apr eval <s8 artifact> --task classify --split validation --lock-out --json`, apr 0.69.3 built at HEAD, host 48 GB:

| Eval rows | Batches | Exit | Wall | Max RSS | Peak memory footprint |
|---|---|---|---|---|---|
| 61 | 1 | 0 | 3 s | 9.4 GB | 9.4 GB |
| 241 | 1 | 0 | 14.8 s | 29.0 GB | 40.3 GB |
| 852 (the real validation split) | 4 | killed | 40.9 s | 11.5 GB at death | 90.0 GB at death |

About 9 GB baseline plus roughly 0.13 GB per row. The 852-row footprint is above the single-batch 40 GB, which suggests memory accumulates across the 256-row batches (not confirmed). The test split is 2129 rows. The code path is `predict_rows` in `crates/aprender-train/src/train/setfit/apr_evaluate.rs`, which chunks by core's `MAX_BATCH_TEXTS` = 256 and calls `VerifiedSetFitModel::classify`. An earlier identical attempt ended with exit 137. Host load was 35 to 396 with 32 users, so wall times are upper bounds; the footprint is intrinsic to the eval path.

`apr eval` was deliberately not changed: `crates/aprender-train/CLAUDE.md` requires a provable contract before code, a test pins the `MAX_BATCH_TEXTS` chunking, and core documents batched versus singleton classification as equal only within 1e-6. The owner also said not to change it.

**Training-time estimate (not a measurement):** the pair budget `2*max(pos, neg)` is 8192 pairs at s64 with 2 classes, so 512 steps at batch 16, against 1536 steps and about 29 min for the 3-class stance cell. Step scaling gives about 10 min; 3x per-step gives about 30 min; about 45 min is an upper bound on this loaded host. Derived from the contract formula and the 28 s s8 run; the cell was never started.

## Task commits

1. **Task 1 (tracer): adapter, e1b16 config, config-equals-reference test** - `7e350e150`
2. **Task 2 (code): `laya-setfit-cell` recipe and `setfit-cell` gate row** - `05cd45386`
3. **Task 2 (fix): recipe uses the pinned `full_model.apr` checkout** - `659dbeb05`
4. **Task 2 (record): evidence, blocked** - `018199dae`
5. **Task 3: the owner's decision** - taken (C) at the blocked-cell checkpoint, recorded in the evidence; the plan's refusal-specific checkpoint did not apply because the outcome is blocked, not refused

`commits: 4` is counted by `git log d4ea9847c..HEAD --grep '(10-05)'`. `rev-list d4ea9847c..HEAD` is 9 because plan 10-07's commits landed in the same window on this branch; they are not this plan's.

## Deviations from Plan

**1. [Rule 3 - Blocking] Wrong model directory in the plan.** Found during Task 1/2 rehearsal. The plan's HF snapshot has only `model.safetensors`; `apr setfit train` exits 6 with `ImportIo(full_model.apr: no APR weights present)`. The pinned checkout is `~/.cache/aprender/minilm-l6-v2-1110a243` (justfile `minilm_dir`), whose manifest records revision `1110a243...` and a source safetensors sha256 equal to the HF snapshot's. Fix: the recipe uses `{{minilm_dir}}` and refuses without `full_model.apr`. Files: `justfile`. Commit `659dbeb05`. (The recorded dry run used the HF path because a dry run never opens the model dir; the evidence says so.)

**2. [Rule 4 - Architectural, owner decision] The cell cannot be evaluated on this host.** Measured above. Stopped and asked; the owner chose C. The plan's verify has no blocked branch, so the evidence adds one (`blocked`, `implementation`) instead of pretending a domain refusal. Commit `018199dae`.

**3. [Process] The recipe labels every non-zero step `SETFIT CELL REFUSED <step>`.** That is the plan's fixed string, but it makes an implementation failure (for example a killed eval) read like a refusal. Not changed here; classification is by the log markers, as the plan says. A follow-up could emit a distinct `SETFIT CELL FAILED` for signals.

**Total deviations:** 1 auto-fixed, 1 owner-decided, 1 noted. **Impact:** the plan's goal is unmet; everything upstream of the encoder load is verified.

## Honesty of the gate row

With no cell artifact, nothing reports a cell result: the recipe prints `SETFIT CELL OK` only after the test-split eval JSON is read, and a run with an existing `cell.apr` printed `SETFIT CELL REFUSED train: ... already exists` and exit 2 with zero `SETFIT CELL OK` lines (checked). The `setfit-cell` row is GREEN in the default tier only because its two must-fail cases (missing adapted dir, missing config) behave; the one-row sweep names "FULL tier not run: the s64e1b16 cell". That GREEN means "the refusals work", not "the cell ran". Anyone running `LAYA_GATES_FULL=1` will get an honest RED today.

## What plans 10-06 and 10-09 must be told

- **There is no SetFit number.** `10-SETFIT-CELL-EVIDENCE.json` has `eval_validation`, `eval_test`, `metric_tag` and every dump-derived field null. Plan 10-06 must not fill a SetFit column and must not infer one; its side-by-side should read "SetFit s64e1b16: not run (blocked), SC-4 held open".
- **SC-4 is NOT met** and the owner held it open (decision C, 2026-10-22). Plan 10-09 (hand-off) must carry it as an open item with the cause: `apr eval` memory, an implementation limit, not a lifecycle refusal. It must not word it as "the SetFit cell was refused" or as a SetFit-versus-Laya result.
- The Laya numbers stand alone: macro-F1 0.7956 against zero-shot 0.5596 on the same 2129-row eval set (10-GATE-RUN-EVIDENCE.json). Any claim built on them is in-distribution, as 10-04 said.
- The adapted dir, the selection manifest, the config and the recipe are ready, so a later run costs only the eval fix (or a larger-memory host) plus the train time.

## Suggested follow-up (not done)

Add an item for the `apr eval` memory defect: find where the classify path in `predict_rows` retains activation or autograd memory per row and across the 256-row batches, bound it contract-first in `aprender-train`, check batched-versus-chunked parity (core documents only 1e-6 agreement), then re-run `just laya-setfit-cell`. Alternative with no code change: an evaluation host with more than about 300 GB for the 2129-row test split (extrapolated from about 0.13 GB per row, a rough figure). Neither was done; no cloud was used.

## Known Stubs

None. The unrun FULL-tier gate case is a recorded skip, not a stub.

## Threat Flags

None. T-10-16 held (counts and fingerprints only; the first 50 CSV texts were checked absent from the adapter, evidence, justfile and step logs). T-10-17 held (the same-eval-split proof ran before any training). T-10-18 held (the evidence states what was and was not run). T-10-19 held (`apr_bin.sh` proved the HEAD build; the recipe sources it). T-10-42 held (config sha256 recorded; the equality test passes).

## State and roadmap bookkeeping (disclosure)

The `state` mutation verbs refuse in this repo because 04-VERIFICATION.md has status `gaps_acknowledged`; that report was not re-keyed. STATE.md was hand-edited minimally: one sentence appended to the single `Last activity:` line. ROADMAP.md is handled as described in the close-out commit message.

## Self-Check: PASSED

Files exist: the adapter, the config JSON, `10-SETFIT-CELL-EVIDENCE.json`. Commits `7e350e150`, `05cd45386`, `659dbeb05` and `018199dae` are ancestors of HEAD. The blocked-branch assertions on the evidence passed and the leak check found no tweet text. The plan's own Task 2 verify was not run as written because it has no blocked branch.
