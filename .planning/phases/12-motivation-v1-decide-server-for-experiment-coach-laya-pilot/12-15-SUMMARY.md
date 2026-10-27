---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 15
subsystem: ml-decision-classifier
tags: [kev-4b, qwen3.5, gguf, prefill-cost, owner-stop, blocked-budget, partial-eval]

requires:
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: "12-14 Kev-4B Rust path proven against torch fp32, identity-bound resumable dump driver, 395 encoded rows, owner option full"
provides:
  - "scores.jsonl (gitignored): identity header + 134 scored rows (all 100 eval, 34 train), resumable"
  - "12-KEV-ARM-EVIDENCE.json: outcome blocked, failure_class budget, run record, raw partial_eval"
  - "12-KEV-OWNER-REPLY-2.json: the owner's stop and the three-step routing, verbatim, superseding the first reply by sha256"
  - "tools/partial_eval.py: reproducible raw AUC / accuracy read, keyed by (split, row), no fit"
  - "spike 033 verdict PARTIAL with the prefix-cache reopen condition"
affects: [12-16]

actuals:
  tokens: 21000
  tasks: 1
  commits: 2

tech-stack:
  added: []
  patterns:
    - "key every lookup by (split, row): eval rows 0-99 and train rows 0-99 collide on `row` alone"
    - "an arm stopped by the owner is a blocked:budget record, not a failure and not a silent drop"

key-files:
  created:
    - .planning/spikes/033-motivation-kev-4b-arm/tools/partial_eval.py
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-KEV-OWNER-REPLY-2.json
  modified:
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-KEV-ARM-EVIDENCE.json
    - .planning/spikes/033-motivation-kev-4b-arm/README.md
    - .planning/spikes/MANIFEST.md

key-decisions:
  - "Kev-4B enters the bake-off as `blocked:budget:<12-KEV-ARM-EVIDENCE.json>` (owner decision 2026-10-26); no calibrated Kev number exists"
  - "Kev prefix-cache / GPU optimisation is a separate future spike, not started; it is the reopen route"

requirements-completed: [D-24, D-07]

coverage:
  - id: D1
    description: "D-24: the owner's option was run until the owner stopped it; the arm is recorded blocked with failure class budget (owner declined), with the 134 rows scored, timings and the resume command"
    requirement: "D-24"
    verification:
      - kind: other
        ref: "adapted check (see Deviations): blocked record outcome blocked / failure_class budget, rows_scored {eval 100, train 34}, reply-2 cites the first reply's sha256"
        status: pass
    human_judgment: false
  - id: D2
    description: "D-07: branch, commit, GGUF sha256, threads, per-row seconds and total hours recorded; ran locally only; nothing deleted, GGUF kept (free disk 41.4 GB, above the 25 GiB threshold)"
    requirement: "D-07"
    verification:
      - kind: other
        ref: "evidence run / cleanup_12_15 fields; git ls-files -- models/kev data/kev empty; spike 017 models dir present"
        status: pass
    human_judgment: false
  - id: D3
    description: "D-11 (train-only fit) NOT exercised: the owner stopped the dump at 34 of 295 train rows, no fit ran, fit_kev.py was not written"
    requirement: "D-11"
    verification:
      - kind: other
        ref: "evidence fit is null with a reason; no eval row was used for any fitting (partial_eval.py fits nothing)"
        status: na
    human_judgment: false
  - id: D4
    description: "D-05: the Kev arm is ABSENT from the bake-off as blocked:budget; bakeoff.py parse_arm_spec accepts the `blocked:budget:<path>` form and blocked_record hashes the evidence"
    requirement: "D-05"
    verification:
      - kind: other
        ref: "bakeoff.parse_arm_spec('kev', 'blocked:budget:<evidence>') == ('blocked', 'budget', <evidence>); blocked_record sha256 equals the evidence file's"
        status: pass
    human_judgment: false

plan_head_before: 3a073ce6fd9720fe5fc56fe4cc48d7574bbd81d7
plan_head_after: df80ec0d6

duration: multi-session (Task 1 dump 2026-10-26, stopped by the owner after 134 rows; closing 2026-10-26)
completed: 2026-10-26
status: partial — closed by owner decision
---

# Phase 12 Plan 15: Kev-4B reference arm, closed as a documented partial Summary

**The full Kev-4B dump ran 134 of 395 rows at a median 142 s per post and was stopped by the owner as impractical without a prefix cache or GPU; the arm is recorded `blocked:budget`, with a raw uncalibrated partial read (AUC conspiratorial 0.904, argmax accuracy 0.17) and a prefix-cache reopen route.**

## What happened

- Task 1 ran the Kev-4B dump (option `full`, 395 rows) detached under the identity-bound resumable driver. Speed: 139 to 160 s per row (median 142.4 s) on 14 CPU threads, 6,853 to 6,966 tokens per row. The post is about 100 tokens; the rest is the shared guide, about 6,800 tokens, prefilled uncached for every post. The served Laya is about 1 s warm per post.
- The owner stopped the run after 134 rows: all 100 eval rows and 34 train rows (5.35 h of scoring, the sum of the per-row `ms` over the scored rows, excluding the 56 s model load). Owner, 2026-10-26, verbatim: "Let's stop it and rethink the model." Then, after a corrected comparison: "It seems that the Laya is winning on speed and performance, and running the Kev for the classification with more than 300 seconds per post is not reasonable. If we don't optimize Kev to use prefill or cache, and use GPU, it is not practical. What do you think?" Then, approving the recommendation: "yes, proceed with the three steps". The three steps: (1) close 12-15 as a documented partial (this summary), (2) run 12-16 with Kev as a blocked / AUC-only reference arm, (3) a Kev prefix-cache / GPU optimisation spike, separate and not started.
- The process was killed, so there is no rc file and `/usr/bin/time -l` never printed; peak RSS of this run is not recoverable (`peak_rss_bytes: null` with the reason). Earlier measurements stand: 12.97 GB on the timed row, 24.0 GB on the parity run. `scores.jsonl` is complete: 135 lines (identity header plus 134 rows), it ends with a newline, every line parses, and each `(split, row)` appears once.

## The partial numbers (raw, uncalibrated, zero-shot)

From `tools/partial_eval.py` on the 100 eval rows (gold: other 63, social_affiliative 23, conspiratorial 14), the head's own temperature, no fit, no eval row used for any fitting:

| Quantity | Value |
|---|---|
| argmax accuracy | 0.17 |
| predicted classes | conspiratorial 95, social_affiliative 5, other 0 |
| AUC one-vs-rest, conspiratorial | 0.9045 |
| AUC one-vs-rest, social_affiliative | 0.6957 |
| AUC one-vs-rest, other | 0.6727 |

Read with care: only 14 conspiratorial rows, so the AUCs are noisy (no interval computed); the ranking signal and the argmax disagree because the probabilities are uncalibrated; the prompt is about 6.6x Kev's training branch length. This is not a calibrated Kev number, no `eval-probs.json` exists, and no parity or superiority of either arm is claimed.

Label order of the three probabilities is `[social_affiliative, conspiratorial, other]`, confirmed from source (not assumed): `encode_rows.py` passes task.json's criteria in document order, `kev/api.py` `question_keys()` returns `list(criteria)`, and `kev/model.py` `encode()` builds the option spans in `q["options"]` order. The script also asserts the dump's option token positions are ascending.

**Keying defect, for the record.** An earlier ad-hoc analysis in this session keyed gold labels by `row` alone. Eval rows 0-99 collide with train rows 0-99, so train labels silently overwrote eval labels and the result looked like chance, which contributed to the owner's stop. `partial_eval.py` keys by `(split, row)`, asserts the eval label sequence of `rows-chosen.json` equals that of `eval.jsonl`, and asserts the class counts. The corrected numbers above reproduced the values from the corrected check made in-session. The cost finding (142 s per post) does not depend on the labels and stands.

## Requirements, stated honestly

- **D-24:** satisfied as `blocked: budget, owner declined` (the plan's own class for an owner-declined budget). The owner's chosen rows were run until the owner stopped them; the evidence records rows scored, timings and the resume command.
- **D-07:** branch `spike/016-upstream-sync`, commit, GGUF sha256, threads, per-row seconds and total hours are recorded; peak RSS for this run is null with a reason. Local only. Nothing was deleted: `models/kev/kev-4b/merged` no longer existed (removed in 12-14), and the GGUF is kept because free disk is 41.4 GB, above the plan's 25 GiB threshold (`free_bytes_after` 41,364,643,840).
- **D-11:** not exercised. No fit ran, so no train-only fit was demonstrated or violated; `partial_eval.py` fits nothing.
- **D-05:** the Kev arm is absent from the bake-off as `blocked:budget`. The bake-off arm spec is `--kev blocked:budget:.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-KEV-ARM-EVIDENCE.json`.

## Task Commits

1. **Task 1 close-out: owner reply 2, blocked evidence, partial_eval.py** - `0a584dbe2` (feat)
2. **Spike 033 README verdict and MANIFEST row** - `df80ec0d6` (docs)

(The dump itself, Task 1's compute, produced gitignored outputs only and no commit.)

## Deviations from Plan

1. **[Owner decision - stop] Task 1 ended `stopped_by_owner`, not a completed or runtime-blocked run.** The plan's blocked path names only a preparation-time block or an owner-declined option; an owner stop mid-run is the closest case and is recorded as `blocked` / `budget` with `run.status: stopped_by_owner`. The first reply file `12-KEV-OWNER-REPLY.json` is byte-identical (sha256 `b510ef4d74ee4d7d219c26812c709ad0e254ccee47a56a4b7c1bb6d3bcf094be`); the new `12-KEV-OWNER-REPLY-2.json` (option `stopped`) supersedes it by that sha256.
2. **Task 2 not executed.** `tools/fit_kev.py` was not written, no fit ran, no `eval-probs.json` or `zero-shot-eval-probs.json` was produced, and the `fit`, `eval_probs` and `zero_shot_eval_*` evidence fields are `null` with reasons. Task 2's spike README and MANIFEST updates were done (verdict PARTIAL). `tools/partial_eval.py` is new and is not in the plan's file list (reproducible raw read, requested by the owner routing).
3. **Adapted verification.** Task 1's own `<automated>` reads a single reply file and, for a non-blocked run, expects rc 0, 395 rows and a recorded peak RSS, so it cannot pass as written for a stopped run; Task 2's reads the full-completion fields. Checks actually run instead: (a) evidence parses with outcome `blocked`, failure_class `budget`, branch `spike/016-upstream-sync`, a 40-hex commit, `run.status` `stopped_by_owner`, rows_scored {eval 100, train 34}; (b) reply-2 exists with option `stopped` and `supersedes.sha256` equal to the live sha256 of the original reply; (c) the original reply's sha256 equals its sha256 in `HEAD` and `git status` shows it unmodified; (d) `bakeoff.parse_arm_spec("kev", "blocked:budget:<evidence>")` returns `("blocked", "budget", <evidence>)` and `blocked_record` hashes the evidence; the full bake-off was not run (that is 12-16); (e) `git ls-files -- models/kev data/kev` is empty; (f) `.planning/spikes/017-kev-rust-forward-parity/models` still exists; (g) the MANIFEST has one `| 033 |` row; (h) `partial_eval.py` run offline under `uv run --offline --project scripts/laya_train --frozen python -I` exited 0 with its internal asserts (eval label sequence, class counts, identity header, unique `(split, row)`, probabilities summing to 1).
4. **Evidence layout.** The plan-14 `cleanup` block is kept as history; plan 12-15's cleanup is in a new `cleanup_12_15` key, and the file's `plan` is now `12-15` with `prior_plan` `12-14`.

**Total deviations:** the owner stop and the consequences listed. No Rule 1 to 3 code fixes. **Impact:** the Kev arm contributes no calibrated number; the bake-off (12-16) takes it as blocked.

## Reopen route

A CPU or GPU prefix cache of the shared guide prefix would cut per-post compute roughly 60x (about 100 new tokens instead of about 6,900), plus batching. That is a separate spike, not started. Upstream's prefix/prefill state is recorded in the memory note `aprender-upstream-qwen35-prefix-and-prefill-state.md`. AWS GPU quota in ze-kasher-dev us-east-1 is G/VT 8 vCPU only, so GPU hosting needs a quota step. The kept `scores.jsonl`, `scores-identity.json` and `rows-chosen.json` (gitignored) make the same `dump` command resumable; `run.resume_command` holds it. Running it needs a new owner decision, because the first reply's compute authorisation is spent.

## Issues Encountered

- `gsd_run` state and roadmap tracking fails on `04-VERIFICATION.md` (`gaps_acknowledged` is outside the tool's closed set) by design. STATE.md, ROADMAP.md and REQUIREMENTS.md were not touched; the orchestrator reconciles them.

## Known Stubs

None.

## Threat Flags

None. No new endpoint, auth path or trust-boundary file access.

## Self-Check: PASSED

- Files present: `tools/partial_eval.py`, `12-KEV-OWNER-REPLY-2.json`, `12-KEV-ARM-EVIDENCE.json`, spike README and MANIFEST edits.
- Commits `0a584dbe2` and `df80ec0d6` are ancestors of HEAD.
