---
phase: 10-antisemitism-v1-decide-server-for-experiment-coach
plan: 04
subsystem: testing
tags: [laya, fine-tune, gate, mps, goldstandard, pack, verify, apr, door]

requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: "plan 10-01: approved task (task_json_sha256 33fd8d7c...0248e3, P = 86, door 8) and the gitignored data dir; plan 10-03: E = 4 and s_per_step 1.4695 on mps:0; plan 10-11: the recorded no-op, door 8"
provides:
  - "10-GATE-RUN-EVIDENCE.json: outcome gate_pass, per-seed numbers, gate, both NOISE records, PACKED line, the single verify line, inspect, both plan-only records, artifact identity, device, wall"
  - "models/decide/laya-antisemitism-v1/ (gitignored run dir) and models/decide/laya-antisemitism-v1.apr (gitignored, 846,220,868 bytes, sha256 2293bebb...c776e), deploy-eligible"
affects: [10-05, 10-06, 10-07, 10-08, 10-09, 10-10, D-18-date]

actuals:
  tokens: 2450
  tasks: 3
  commits: 2
plan_head_before: 80da239728d258366ccbd5998e23262dced666c1
plan_head_after: 0177cf57d0477d66993a0e5569065451e77fd5f4

tech-stack:
  added: []
  patterns:
    - "a multi-hour run launched detached (caffeinate + nohup) with its exit status written to a file by the wrapper, polled with bounded until-loops; the status is read from the file, never through a pipe"
    - "the evidence is built by one script from the run's own files (gate-report, variance-report, rescore-noise) and the single recorded verify/inspect/plan-only lines, so no number is retyped"

key-files:
  created:
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-GATE-RUN-EVIDENCE.json
  modified: []

key-decisions:
  - "Task 3 (the stop-rule checkpoint) was skipped on the record: outcome gate_pass, deploy_eligible true, door check ok; nothing was retuned and laya-train ran exactly once"
  - "The door cross-check asserts min_row_tokens (Rust, 86) == P (Python, 86) and texts == chosen_max_texts, never tokens_planned == 8 x P (688); tokens_planned is 800 as the request total"

requirements-completed: [SC-2, SC-3, D-05, D-08, D-09, D-15]

coverage:
  - id: D1
    description: "The exact recipe of record loads the real data through train.py's validators and refuses only for the missing epoch count, with no run dir written; pre-flight green"
    requirement: "D-08"
    verification:
      - kind: other
        ref: "Task 1 verify: SKELETON OK E 4 expected_hours 5.83 (laya-train-selftest OK; aprender-decide lib 165 passed; refusal rc 2 'REFUSED epochs: above 16 shots/class (7016)')"
        status: pass
    human_judgment: false
  - id: D2
    description: "One declared three-seed run judged by the unchanged gate on mps:0: GATE PASS, shipped seed 23, margin 0.2360, ece_post 0.0424"
    requirement: "SC-2"
    verification:
      - kind: other
        ref: "Task 2 verify: GATE RUN RECORD OK outcome gate_pass run models/decide/laya-antisemitism-v1 declared_runs 1 checkpoints 1"
        status: pass
    human_judgment: false
  - id: D3
    description: "The deploy-eligible artifact: PACKED, one laya-verify (deploy_eligible true, artifact_sha256 equal to the file's recomputed sha256), inspect labels and variant, plan-only min_row_tokens == P and texts == chosen_max_texts"
    requirement: "SC-3"
    verification:
      - kind: other
        ref: "Task 2 verify (sha256 recomputed over the 846,220,868-byte file == verify line == record; door_check.ok recomputed true)"
        status: pass
    human_judgment: false
  - id: D4
    description: "What the passing gate means for the partner-facing claim (in-distribution calibration on a held-out group-disjoint 20 percent of the same labelled set, not shift robustness)"
    requirement: "D-07"
    human_judgment: true
    rationale: "The wording of any claim built on these numbers is the owner's decision and belongs to the hand-off plan; no test asserts it"

duration: 5h 4m run wall plus pre-flight and close-out
completed: 2026-10-22
status: complete
---

# Phase 10 Plan 04: The declared GoldStandard run Summary

**One declared three-seed fine-tune at E = 4 on mps:0 passed the unchanged gate (macro-F1 0.7956 against zero-shot 0.5596, margin 0.2360 against 0.05; post-calibration ECE 0.0424 against 0.10); seed 23 ships, and its packed `.apr` (sha256 `2293bebb...c776e`) is deploy-eligible by the single `laya-verify` and admitted by the compiled door at 8 texts with min_row_tokens 86 equal to P.**

## Outcome of record

`gate_pass`, decided by the gate. `declared_runs` 1, `shipped_run` null (no re-run). Task 3 was not entered. The owner's `approve-local` authorised this one run on this machine; nothing outside it was done (no cloud, no other epoch count).

| Item | Value |
|---|---|
| Command | `just laya-train data/decide/goldstandard-2024 models/decide/laya-antisemitism-v1 --seeds 3 --epochs 4` (E read from 10-PROBE-EVIDENCE.json `epochs_declared`) |
| Device | `DEVICE seed=13/17/23 requested=mps used=mps:0 torch=2.14.0` (three trace lines; gate-report `device_used` mps:0, `device_is_cpu` false) |
| Run window | 2026-10-22T04:06:21Z to 08:46:45Z; `total_seconds=16822.3` (4.67 h) against the probe's 5.13 h projection for training plus the 1.0 h scoring tail |
| Pack | 08:47:14Z to 09:08:36Z (about 21 min) |
| Verify (one run) | 09:08:48Z to 09:30:01Z (about 21 min) |
| Eval set | 2,129 rows (374 antisemitic, 1,755 not); fit 6,382 rows, calibration slice 2,128 |
| recipe_id | `d70c3f3da0daae129f3ee1ce928eb21a0ef71a83a6f09dc18e0621f3cc8adcbc` |

`RECIPE WRITTEN` appears once in `/tmp/p10-04-train.log`; the rc file reads 0; `WARN pack will refuse` did not appear.

### Per-seed table (from gate-report.json and variance-report.json)

| seed | macro_f1 | ece_post | margin | T applied | clamp_hit | pass | best_epoch / run | rank_key |
|---|---|---|---|---|---|---|---|---|
| 13 | 0.7954 | 0.0117 | 0.2358 | 1.4109 | false | true | 1 / 4 | 117 |
| 17 | 0.8054 | 0.0456 | 0.2458 | 5.0000 | **true** | true | 3 / 4 | 456 |
| **23 (shipped)** | **0.7956** | **0.0424** | **0.2360** | **1.5104** | false | true | 1 / 4 | 423 |

`MEDIAN seeds=13,17,23 rank_keys=117,456,423 shipped=23 (policy median_ece, tie_break smaller_seed)`. Variance, information only: macro_f1 0.7988 +/- 0.0057, ece_post 0.0332 +/- 0.0187, margin 0.2392 +/- 0.0057. Zero-shot on the same rows: macro_f1 0.5596, ECE 0.0482. Shipped seed: ece_pre 0.0604, nll 0.2789.

### Log lines of record

```
RESULT zero_shot macro_f1=0.5596 f_avg=null ece=0.0482 | fine_tuned macro_f1=0.7956 f_avg=null ece_pre=0.0604 ece_post=0.0424 nll=0.2789 | margin=0.2360 (need >= 0.05) ece_post (need <= 0.1)
MEDIAN seeds=13,17,23 rank_keys=117,456,423 shipped=23 (policy median_ece, tie_break smaller_seed)
NOISE which=fine_tuned max_abs=3.592974e-06 bound=1.437190e-05 argmax=2129/2129 t_applied=1.510358
NOISE which=zero_shot max_abs=1.507090e-05 bound=6.028359e-05 argmax=2129/2129 t_applied=1.906356
SEEDS median-ECE seed of 3 seeds | shipped=23 device_used=mps:0 torch=2.14.0 train_seconds=4864.2 total_seconds=16822.3
GATE PASS
```

`train_seconds` on the SEEDS line is one seed's (the shipped seed 23); the wall is `total_seconds`.

### The artifact

```
PACKED models/decide/laya-antisemitism-v1.apr sha256=2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e rescore_max_abs=0.000007554888725280762 zs_rescore_max_abs=0.00001195073127746582 rescore_bound=0.000014371895413756164 zs_rescore_bound=0.000060283592210108594 noise=0.000003592973853439041 zs_noise=0.000015070898052527149 shipped_seed=23 argmax=2129/2129
```

The single `laya-verify` line (exit 0, run once, never repeated): `deploy_eligible` true, `artifact_sha256` `2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e`, `argmax` 2129/2129, `recomputed` {zs_macro_f1 0.5596, ft_macro_f1 0.7956, ece_post 0.0424, margin 0.2360}, `rescore_max_abs` 7.55e-06 against bound 1.437e-05, `zs_rescore_max_abs` 1.195e-05 against bound 6.028e-05, `shipped_seed` 23. The Task 2 verify recomputed `shasum -a 256` over the file (846,220,868 bytes) in Python and it equals the verify line and the record.

`laya-inspect`: labels `["not_antisemitic","antisemitic"]`, variant `production`, method `laya`, base `laya-en-root@55cf4c4e`, recipe_id equal to the run's, embedded gate {pass true, margin 0.2360, ece_post 0.0424}.

### Plan-only cross-check (the served door)

| Shape | texts | tokens_planned | min_row_tokens | max_texts | max_total_tokens | exit |
|---|---|---|---|---|---|---|
| distributed | 8 | 800 | 86 | 8 | 800 | 0 |
| concentrated | 2 | 800 | 86 | 8 | 800 | 0 |

p_rust 86 == p_python 86 (10-TASK-EVIDENCE `measured.P`); distributed texts 8 == chosen_max_texts 8; concentrated texts 2 == min(2, 8); both commands exit 0, so the server's own door check admits the task; tokens_planned 800 <= 800 as the request total. `door_check.ok` true with no reasons. Artifact and task are bound: gate-report `inputs_sha256.task_json` equals the approved `33fd8d7c...0248e3`, and the three data files hash the same after the run as before (T-10-12).

## Accomplishments

- Pre-flight: `just laya-train-selftest` ended `LAYA TRAIN SELFTEST OK`; `aprender-decide --lib` 165 passed; free disk 61.3 GB; the recipe of record without `--epochs` exited 2 with `REFUSED epochs: above 16 shots/class (7016) --epochs is required, in [4, 12]` and wrote no run dir, which proves the real data loads through train.py's own validators.
- One declared run, one gate decision, one pack, one verify, one inspect, two plan-only records, all in `10-GATE-RUN-EVIDENCE.json` (numbers, hashes and literal numeric log lines only; no tweet text, no home path, asserted).
- Exactly one `model.safetensors` remains, at `<run>/checkpoint`; the other two seeds' checkpoints were deleted by train.py (`DELETED seed=13`, `DELETED seed=17`).

## Task Commits

1. **Task 1 (tracer): pre-flight, refusal path, evidence skeleton** - `381780697`
2. **Task 2: the declared run, pack, one verify, inspect, plan-only, completed record** - `0177cf57d`
3. **Task 3: stop-rule decision** - not entered (gate_pass, eligible artifact, passing door check)

## What the pass does and does not say

Stated plainly, because the numbers are good and the claim is narrow. The gate certifies margin and calibration on the held-out 20 percent of GoldStandard2024, split group-disjoint at seed 13 from the same labelled set the model trained on. It is in-distribution. It does not certify robustness to a different input population, and the live labelled sample (`shift_jsonl`, D-04) does not exist yet: those slots are null. No accuracy figure beyond the table above is claimed anywhere.

## Observations (not acted on, nothing retuned)

- **Overfitting after epoch 1 is visible in the log.** Training cross-entropy reaches 0.0000 by epoch 3 in every seed, and the calibration monitor's fitted temperature runs to the 5.0 clamp from epoch 2 or 3 onward. Two of three seeds (13 and 23, including the shipped one) restored their epoch-1 weights (`best_epoch` 1); seed 17 restored epoch 3. E = 4 is the probe's written rule, so early stopping could only act as best-epoch restore, as 10-03 recorded. The shipped model is therefore, in effect, a one-epoch fine-tune. That is a recipe fact for the owner to weigh if a later run is ever considered; it is not a reason the gate result is wrong.
- **Seed 17 hit the T clamp** (`t_fitted` 5.0, `clamp_hit` true). It is not the shipped seed (gate `clamp_hit` false for seed 23, T 1.5104) and its ece_post 0.0456 still passes. The contract's median rule picked seed 23 by ECE rank, not by this.
- **Seed-to-seed ece_post spread is wide** (0.0117 to 0.0456, sd 0.0187) while margin is tight (sd 0.0057). The shipped seed's 0.0424 is the median, not the best, and is 0.0576 under the 0.10 bar.
- **Pack and verify each took about 21 min,** not the plan's assumed 18 (RESEARCH A2 was an [ASSUMED] linear estimate), so each of the five downstream verify runs plan 10-10's budget assumes should be priced at about 21 min.
- Every run prints Laya's RuntimeWarning that the base checkpoint's `choice:11+` temperature 0.1006 is outside [0.5, 5] and is clamped; a property of the pinned base, as in 08-16 and 10-03.

## Deviations from Plan

None to the plan as revised - it executed as written. Notes that are not deviations:

- **The `tokens_planned == 8 x P` cross-check is not in this plan** (its first revision had it, review round 1 removed it); the task brief repeated the warning. On this artifact `tokens_planned` is 800 while `8 x P` is 688, and the stance artifact had 800 against `min_row_tokens` 57 for the same reason (the distributed shape fills each text to its share of the budget). The assertions made are `min_row_tokens == P`, `texts == chosen_max_texts`, exit 0 and `tokens_planned <= max_total_tokens`.
- **Host load was not sampled during the run.** The plan asks for `host_load` honestly: only the pre-launch load average (2.46 / 3.18 / 4.32) and a process listing at launch were taken, no 15 s sampler like 10-03's. The record says `contended: null`, `sampled_during_run: false`: contention is unmeasured, not ruled out. This plan started no other compute while the run was going. The run finished faster than the probe projected, so contention did not visibly cost time.
- The Bash tool is zsh; the Task 2 verify's logic was run as a Python block with the plan's assertions unchanged (plus a post-run re-hash of the three data files). The plan's fixed `/tmp/p10-04-*` log and rc paths were kept because the evidence and the verify name them.
- The CLAUDE.md "stop and report" rule for >1 h non-lambda compute was satisfied by the owner's `approve-local` for this plan's declared run only.

## Known Stubs

None.

## Threat Flags

None. The plan added no endpoint, auth path or trust-boundary file access. T-10-12 (data hashes equal before and after the run, train.py's guard did not fire), T-10-13 (verify sha == file sha == record; inspect labels and variant; plan-only compares min_row_tokens with P and the door with chosen_max_texts), T-10-14 (python assertion: no home path; numbers and hashes only), T-10-15 (one recipe-written line; no second run) and T-10-43 (not triggered: the artifact is eligible) hold as mitigated.

## State and roadmap bookkeeping (disclosure)

The `state` mutation verbs (`advance-plan`, `record-metric`, `add-decision`, `record-session`) refuse in this repo because 04-VERIFICATION.md has status `gaps_acknowledged`, which the tool's closed set does not contain; that report was not re-keyed. STATE.md was hand-edited minimally: one sentence appended to the single `Last activity:` line, the frontmatter untouched, as the 10-02 and 10-03 close-outs did. `roadmap update-plan-progress 10` was run through gsd-tools for ROADMAP.md.

## Next

Plan 10-05 onward read `run_dir` and the artifact from `10-GATE-RUN-EVIDENCE.json`: `models/decide/laya-antisemitism-v1.apr`, sha256 `2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e`, `deploy_eligible` true. The artifact and run dir are gitignored and stay uncommitted.

## Self-Check: PASSED

`10-GATE-RUN-EVIDENCE.json` exists; commits `381780697` and `0177cf57d` are ancestors of HEAD; the artifact exists with the recorded sha256; `models/decide/laya-antisemitism-v1/checkpoint/model.safetensors` is the only checkpoint; the Task 1 verify (`SKELETON OK`) and the Task 2 verify (`GATE RUN RECORD OK`) both passed; `git status` shows no stray tracked changes outside this plan's files.
