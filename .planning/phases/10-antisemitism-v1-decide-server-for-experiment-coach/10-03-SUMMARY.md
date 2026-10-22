---
phase: 10-antisemitism-v1-decide-server-for-experiment-coach
plan: 03
subsystem: testing
tags: [laya, fine-tune, probe, mps, timing, host-load, epochs]

requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: "plan 10-01: the owner-approved task (10-TASK-EVIDENCE.json, P = 86, door 8) and the gitignored data dir data/decide/goldstandard-2024"
provides:
  - "scripts/laya_train/probe_epoch.py and `just laya-probe-epoch`: the D-10 one-epoch measurement driver (writes no run dir)"
  - "10-PROBE-EVIDENCE.json: a measured s_per_step on mps:0 at the real row length, the per-epoch monitor cost, the hours table, epochs_declared = 4"
  - "10-PROBE-OWNER-REPLY.json: the recorded skip (no trigger fired), bound to the evidence by sha256"
affects: [10-04, 10-11, phase-10-gate-run, D-18-date]

actuals:
  tokens: 6600
  tasks: 3
  commits: 3
plan_head_before: cbae84d2e86ba9eb1f563a9435ea48ab0028a7f8
plan_head_after: 58e1645cbdddba56d4e817148d69bf9f59be403d

tech-stack:
  added: []
  patterns:
    - "a measurement driver that reuses the production functions (train.train_seed, data.py validators, contract.py values) rather than re-implementing them, and writes only its evidence JSON"
    - "a 15 s host-load sampler recorded in the evidence, so a contended window is a fact in the record and a checkpoint trigger, not a silent assumption"

key-files:
  created:
    - scripts/laya_train/probe_epoch.py
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-PROBE-EVIDENCE.json
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-PROBE-OWNER-REPLY.json
  modified:
    - justfile
    - scripts/laya_gates.tsv
    - scripts/laya_train/README.md

key-decisions:
  - "E = 4 for the declared run: s_per_step 1.4695 s is above the 1.0 s that would admit E = 6, so the written rule gives 4"
  - "At E = 4 early stopping cannot fire (earliest stop epoch 1 + 3 = 4 = E); it acts only as best-epoch restore, and the record says so"
  - "The probe times the monitor with one calibration_logits pass plus gate.calibration_monitor over the 2,128-row slice, because train_seed skips it under fixed_epochs and production pays it every evaluated epoch"

requirements-completed: [SC-2, D-08, D-10, D-18]

coverage:
  - id: D1
    description: "probe_epoch.py --dry-run validates the real data dir exactly as train.main step 1 and writes nothing; the guard refuses with exit 2 once the declared run dir exists; the torch-free --host-load case table matches by exact basename"
    requirement: "D-10"
    verification:
      - kind: other
        ref: "just laya-probe-epoch data/decide/goldstandard-2024 /tmp/p10-03-dry.json --dry-run; then the Task 1 verify block (guard refusal, host-load busy/idle/live case table, laya-train-selftest, LAYA_GATES_ONLY=prefix-budget just laya-gates-selftest)"
        status: pass
    human_judgment: false
  - id: D2
    description: "A measured one-epoch probe on mps:0 with the device read back from the DEVICE log line, s_per_step, monitor cost, hours for E in 4..12 and E declared by the written rule"
    requirement: "D-08"
    verification:
      - kind: other
        ref: "Task 2 verify block against 10-PROBE-EVIDENCE.json: PROBE EVIDENCE OK device mps:0 s/step 1.4695488721804513 monitor s/epoch 66.83721613883972 E 4 exceeds False truncated rows 0"
        status: pass
    human_judgment: false
  - id: D3
    description: "The owner-checkpoint skip is recorded on the sha256 of the evidence it was computed from"
    requirement: "D-18"
    verification:
      - kind: other
        ref: "Task 3 verify block: PROBE REPLY: skipped, no trigger fired"
        status: pass
    human_judgment: false

duration: 36min
completed: 2026-10-21
status: complete
---

# Phase 10 Plan 03: One-epoch probe Summary

**A measured 1.4695 s/step on mps:0 at the approved task's real row length (monitor 66.8 s per epoch), which fixes E = 4 and puts the three-seed run at 4.13 training hours, with no owner trigger fired.**

## Performance

- **Duration:** about 36 min wall clock, of which the probe itself was 20 min 47 s (log written 03:19:05Z, rc file 03:39:52Z)
- **Tasks:** 3 (one tracer, one auto, one conditional blocking-human checkpoint, skipped on the record)
- **Commits:** 3 task commits (measured from the persisted ledger, base `cbae84d2e`)

## The measurement

Log lines, quoted from `/tmp/p10-03-probe.log`:

```
DEVICE seed=13 requested=mps used=mps:0 torch=2.14.0
TRAIN seed=13 fit_rows=6382 epochs=1 epochs_run=1 steps=798/798 seconds=1172.7 ce_first=0.4204 ce_last=0.6499
MONITOR seed=13 calibration_rows=2128 seconds=66.8 nll=0.290583 t_star=2.2425
PROBE OK device_used=mps:0 s_per_step=1.4695 steps=798 P=86
```

The device was read back from `train_seed`'s `info["device_used"]` and matches the `used=` value of the DEVICE line (the Task 2 verify asserts it), so the number is an MPS number by trace line, not by request.

| Quantity | Value |
|---|---|
| s_per_step | 1.4695488721804513 s (1172.7 s / 798 steps); threshold 3.0 s, not exceeded |
| 08-16 demo baseline | 1.16 s/step at rows of at most about 110 tokens; this is 27 % slower at built rows of p50 142 / p95 181 / max 452 tokens |
| monitor_s_per_epoch | 66.84 s (one `calibration_logits` pass plus `gate.calibration_monitor`; the first pass, so it includes any one-off warm-up) |
| P | 86, equal to 10-TASK-EVIDENCE measured.P |
| Rows right-truncated at 512 | 0 of 6,382 fit rows; raw post tokens p50 56 / p95 95 / p99 133.2 / max 366, equal to the retained lengths (max built row 452 <= 512, and no post lost content) |
| task_json_sha256 | `33fd8d7c...0248e3`, equal to 10-TASK-EVIDENCE |

Projected three-seed training hours, monitor included, `(s_per_step x 798 x E + monitor x E) x 3 / 3600`:

| E | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 |
|---|---|---|---|---|---|---|---|---|---|
| hours | 4.13 | 5.16 | 6.20 | 7.23 | 8.26 | 9.30 | 10.33 | 11.36 | 12.40 |

**epochs_declared = 4.** The written rule is "4 unless s_per_step <= 1.0, then 6"; 1.47 s is above 1.0 s. Hours at E = 4 are 4.13 against 6.20 at E = 6, so E = 6 would add 2.07 hours of training for no early-stopping benefit the rule would then be entitled to. Early stopping (D-08) cannot end a seed early at E = 4: its earliest stop is `first_candidate_epoch` 1 + `patience_epochs` 3 = epoch 4, which is E, so every seed runs all four epochs and the rule acts only as best-epoch restore (`restore: best`). The record states this in `epochs_reasoning`.

Schedule: 4.13 h training + 1.0 h scoring tail + 0.7 h pack/verify = 5.83 h to the artifact, plus 5 downstream verify runs of 18 min (1.5 h) = 7.33 h to live. These are compute hours from an immediate start, not a calendar promise. The tail, pack/verify and 18-minute figures are the plan's carried [ASSUMED] budgets, not measured here. `repriced_ready_by` is null because neither the threshold nor the cpu trigger fired.

## Threshold verdict and Task 3

No trigger fired: outcome `measured` (not oom), `exceeds_threshold` false, device `mps:0` (not cpu), `host_load.contended` false. Task 3 therefore wrote its skip record: `10-PROBE-OWNER-REPLY.json` has `"skipped": true`, `"triggers": []`, null reply, option and date, and `probe_evidence_sha256` = `d249314a...822601` (the committed evidence bytes). Plan 10-04 may start with `--epochs 4`. No checkpoint was returned.

## Host-load record, stated plainly

`host_load`: 84 samples every 15 s, 0 with a competing build or test (the list was empty at the start, so the precondition's 60-minute wait was not needed and did not expire); load1 2.33 at the start and 4.63 at the end; ncpu 14; `contended` false. The lock `/tmp/p10-03-probe.running` was held from 03:19:05Z and released when the rc file appeared; the verify confirmed it gone.

Disclosure: the host was not otherwise idle. An unrelated `agy` review process for another project (a network LLM call, not in the COMPETING list) was running throughout, and the starting load average was already about 2.3. The record's `contended` flag is defined on the COMPETING build/test names only, so it does not see that process. A step-time inflated by it would be an upper bound on the clean cost; it cannot have pushed the number below the true one.

## Accomplishments

- `probe_epoch.py`: validates through `data.py` / `contract.py` as `train.main` step 1 does, trains seed 13 for one `fixed_epochs` epoch through `train.train_seed` on the real fit rows, times the skipped per-epoch monitor, records built / raw / retained row lengths and truncation, and samples host load. Writes only the `--out` JSON; refuses `REFUSED run-dir-exists` once `models/decide/laya-antisemitism-v1` exists; turns an out-of-memory error into an `outcome: "oom"` record plus `REFUSED probe-oom` (exit 2) instead of a traceback. The OOM branch was not exercised by a real OOM here; it is covered by the verify's branch only in the sense that the verify accepts that record shape.
- `just laya-probe-epoch`, the `not-a-gate` row, and the README lines. The one-row gates sweep still prints `DRIFT CHECK OK: 32 laya recipes enumerated ... 4 not-a-gate; 30 rows` and `LAYA GATES ROWS OK 1 of 30`.
- Nothing was written under `models/decide` (`find models/decide -newer <stamp>` empty) and the declared run dir does not exist. No tweet text appears in the log or evidence (the first 50 train texts were searched in both: 0 hits).

## Task Commits

1. **Task 1 (tracer): probe_epoch.py --dry-run through the real validators** - `7b5165e26`
2. **Task 2: the one-epoch measurement** - `eb7ce23e3`
3. **Task 3: the owner-checkpoint skip record** - `58e1645cb`

## Deviations from Plan

None - the plan executed as written. Notes that are not deviations:

- The Task 1 and Task 2 verify blocks were saved to files and run under `bash` (the Bash tool is zsh and the blocks use bash functions); the commands are the plan's, unchanged. Probe artifacts stayed at the plan's fixed `/tmp/p10-03-*` paths because the plan, the lock protocol and plan 10-11 name them.
- The plan's text says E = 6 is "the first E at which early stopping ... can end a seed at epoch 4 or 5". E = 5 would also let it end at epoch 4, so the record's wording says instead that s_per_step <= 1.0 admits the rule's E = 6, at which a seed can end at epoch 4 or 5. This changes prose in the E = 6 branch only; this run took the E = 4 branch.

## Observations (not acted on)

- `ce_last` 0.6499 is above `ce_first` 0.4204. These are the cross-entropy of the first and last single batch of 8 on a 82 %-majority task, not a convergence curve; one epoch cannot show a trend, and nothing in this plan gates on them.
- Every run prints Laya's RuntimeWarning that the base checkpoint's `choice:11+` temperature 0.1006 is outside [0.5, 5] and is clamped to 0.5. That is a property of the pinned base, present in the 08-16 runs, unrelated to this plan.

## Known Stubs

None.

## Threat Flags

None. The probe adds no network endpoint, auth path or trust-boundary file access beyond the threat model: T-10-09 (device read back and asserted), T-10-10 (nothing under `models/decide`), T-10-11 (no row text), T-10-45 (task sha and P asserted equal to 10-TASK-EVIDENCE), T-10-47 and T-10-52 (host_load recorded, lock bounded, wait not needed) all hold as mitigated.

## State and roadmap bookkeeping (disclosure)

The `state` mutation verbs (`advance-plan`, `record-metric`, `add-decision`, `record-session`) refuse in this repo because 04-VERIFICATION.md has status `gaps_acknowledged`, which the tool's closed set does not contain; that report was not re-keyed. STATE.md was therefore hand-edited minimally: the single `Last activity:` line (line 50) had one sentence appended, the same way the 10-02 close-out did. `roadmap update-plan-progress 10` was run through gsd-tools for ROADMAP.md.

## Self-Check: PASSED

All created files exist (`probe_epoch.py`, 10-PROBE-EVIDENCE.json, 10-PROBE-OWNER-REPLY.json), commits `7b5165e26`, `eb7ce23e3` and `58e1645cb` are ancestors of HEAD, the Task 1 acceptance block, the Task 2 verify (`PROBE EVIDENCE OK`) and the Task 3 verify (`PROBE REPLY: skipped, no trigger fired`) all passed, the probe lock is released, and `models/decide` holds nothing newer than the stamp.
