---
phase: 10-antisemitism-v1-decide-server-for-experiment-coach
plan: 09
subsystem: infra
tags: [laya, acceptance-replay, hand-off, pmcp-run, live, documentation]

requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: "plan 10-12: aprender-mcp-decide-antisemite live and the previous server retired; plan 10-10: the contract, README and CLAUDE.md state the live sha; plan 10-04: the gate run; plan 10-01: the approved task text; plan 10-02: the probe"
provides:
  - "10-ACCEPTANCE-REPLAY.json: the requester's acceptance tests run live against https://aprender-mcp-decide-antisemite.us-east.true-mcp.com/mcp"
  - "10-HANDOFF.md: sha, labels, endpoint and auth, task text, limits as deployed, replay, timings, readout form, dates and scope"
  - "10-CONTEXT.md: D-13 decision and rename, D-14 amendment, D-05 and D-15 resolution continuations"
affects: []

status: complete
actuals:
  tokens: 4500
  tasks: 2
  commits: 2
plan_head_before: 536b01599eaa514899cc365ca0426ea6c9d16393
plan_head_after: bd607bfc81f986a3ee9e376bc543438bb161ae8e
commits_note: "rev-list count 2 = the two task commits, measured from the on-disk ledger before this SUMMARY was written; the final docs commit that carries this SUMMARY follows and is not in the count. The tokens figure is chars/4 over the diff (18,140 characters), not a harness count."

key-files:
  created:
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-ACCEPTANCE-REPLAY.json
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-HANDOFF.md
  modified:
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-CONTEXT.md

key-decisions:
  - "SC-4 is not claimed. The hand-off carries the blocked comparison branch: the record's label verbatim (NO CELL WAS RUN), sc4_status refused-open, the blocker and the owner's decision C, with no table, no number and no comparison sentence."
  - "The hand-off says `clients` and states the transport items (SigV4 Function URL against a plain HTTPS POST, the server-to-server note, who calls the classifier) as for the partner to confirm; it never says how any named client consumes the server."
  - "The README example result was NOT filled in (see Not done here)."

requirements-completed: [SC-3, SC-5, D-01, D-04, D-05, D-07, D-13, D-14, D-15, D-16, D-17, D-18]
requirements-note: "SC-4 is not among them and stays open by the owner's decision of 2026-10-22. The ids are the plan's frontmatter list; Phase 10's requirements live in ROADMAP.md, not REQUIREMENTS.md, so no requirements.mark-complete was run."

coverage:
  - id: D1
    description: "The acceptance replay against the live new deployment: three distinct one-text calls, the largest admitted call (8), the expected count refusal at 9, the identity probe"
    requirement: "D-16"
    verification:
      - kind: other
        ref: "Task 1 automated verify: ACCEPTANCE REPLAY OK B 8 successful n = [1, 8] count refusal at 9; no tweet text in the replay record; partner repo status and HEAD equal before and after"
        status: pass
    human_judgment: false
  - id: D2
    description: "The hand-off record and the CONTEXT amendments"
    requirement: "D-17"
    verification:
      - kind: other
        ref: "Task 2 automated verify: HANDOFF OK sha 2293bebb29ac P 86 B 8 shipped seed 23; CONTEXT decisions parse: 18; partner repo unchanged"
        status: pass
    human_judgment: false
---

# Phase 10 Plan 09: Acceptance replay and hand-off Summary

**The requester's own acceptance tests pass live against `aprender-mcp-decide-antisemite` (sha256 2293bebb29ac...): three one-text replays and one eight-text replay with every check true, the count refusal at 9 texts naming `classify_max_texts` and the limit 8, and a hand-off record that a client can pin and call from. SC-4 stays open and is not claimed.**

## Performance

- **Duration:** not measured (a short live session; the longest step was the 2 min 3 s probe build).
- **Tasks:** 2 of 2. **Commits:** 2 task commits (measured from the ledger), `105c6a0ed` and `bd607bfc8`.
- **Files:** 2 created, 1 modified, all under `.planning/`. No source, contract, deployment or AWS change.

## Task 1: the acceptance replay (commit 105c6a0ed)

Run on 2026-10-23 against `https://aprender-mcp-decide-antisemite.us-east.true-mcp.com/mcp`, auth off so no credential was needed or sent, with the plan 10-02 probe example and only its committed synthetic sentences. B = 8 (10-TASK-EVIDENCE.json `chosen_max_texts`). Every call retried only on the transient settling refusal; none needed a retry.

| Call | Texts | Result | Elapsed (ms) |
|------|-------|--------|--------------|
| identity probe | 3 POSTs | all four checks true, sha equal to the pin; the first call reached a cold function (`cold;load_ms=13545`) | 14,463 / 181 / 692 |
| `--replay 1 --replay-from 0` | 1 | every check true, deviation 1.8e-8 | 806 |
| `--replay 1 --replay-from 1` | 1 | every check true, deviation 2.6e-8 | 803 |
| `--replay 1 --replay-from 2` | 1 | every check true, deviation 1.8e-8 | 804 |
| `--replay 8 --replay-from 0` | 8 (762 built tokens) | every check true, deviation 2.6e-8, **no step-down** | 4,187 |
| `--replay-over-limit` | 9 | `isError` naming `classify_max_texts`, the limit 8 and the contract | 329 |

`largest_multi_text_n` is 8 and `refused_token_budget` is empty. The count refusal at B + 1 is what proves the deployed door is 8 and not larger. The seven requester tests are all `pass`, each pointing at the replay that proves it. Cold and warm figures are copied from 10-RENAME-DEPLOY-EVIDENCE.json and labelled reported-not-gated.

The verify block passed unmodified except that the four partner-repo baseline files live in the session scratchpad instead of `/tmp` (see Deviations).

## Task 2: the hand-off (commit bd607bfc8)

- **Sha and endpoint:** `artifact_sha256` `2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e`, equal across the gate, rename-deploy and replay records and found in CLAUDE.md, the lambda README and the contract; endpoint `https://aprender-mcp-decide-antisemite.us-east.true-mcp.com/mcp`, auth off; the previous server id and its endpoint named only as retired by the owner on 2026-10-23.
- **Task text:** instructions and both descriptions quoted verbatim; `task_json_sha256` `33fd8d7c...2248e3` recomputed from the file; P = 86, nothing cut at the tokenizer.
- **Limits as deployed:** 8 texts, 800 built tokens, 16 KiB per text, 512-token row cap, right-truncation beyond 426 tokens with `truncated: true` (0 of 11,311 posts affected), and the batching sentence "a client's batch size (the request document's `classify_batch`) is at most the served classify_max_texts, which is 8" with the `min(classify_max_texts, floor(800 / built_row_tokens))` rule and the SUM rule.
- **Gate:** one declared run (so no "two declared runs" sentence), seeds 13/17/23, seed 23 shipped, macro-F1 0.7956 against zero-shot 0.5596, margin 0.2360, ECE 0.0424, claim scope quoted.
- **SetFit record (blocked branch):** label verbatim including `NO CELL WAS RUN: no number exists.`, `refused-open`, SC-4 open and NOT met, the apr eval memory blocker, the owner's decision C, no table, no number, no comparison.
- **Dates:** proposed 2026-10-29; actual ready 2026-10-23 (artifact deploy-eligible and first live 2026-10-22 under the previous id; the final server id verified and replayed 2026-10-23), six days early. Probe `s_per_step` 1.4695 s, E = 4, host-load verdict `contended` false, no owner trigger (reply file is a skip), no gate re-run.
- **CONTEXT amendments** (continuation text, titles unchanged; the extractor still parses 18 decisions): D-13 `Decided 2026-10-22 (owner, plan 10-07 Task 2)` and `Renamed 2026-10-22 (owner, plan 10-12)`; D-14 `Amended 2026-10-22 (owner, plan 10-09)`; D-05 and D-15 `Resolved 2026-10-21 (owner, plan 10-01 Task 2)`.

The D-13 decision line as written: "**Decided 2026-10-22 (owner, plan 10-07 Task 2):** replace-auth-off (replace the stance demo, auth off, no bearer); auth off; deployed by plan 10-08 on 2026-10-22 under the server id aprender-mcp-decide."

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The rtk hook rewrote the first partner-repo baseline**
- **Found during:** Task 1 verify (first run failed on "the partner repo changed during this task").
- **Issue:** the baseline was captured with a plain `git status --porcelain`, which the rtk hook rewrites to the string `ok` on a clean tree; the verify's own capture runs without the hook and wrote an empty file, so `cmp` differed.
- **Fix:** re-captured the baseline with `rtk proxy git ... status --porcelain` (empty, 0 bytes, HEAD `32ce5815d...` unchanged) and re-ran the verify, which then passed. Task 2's baseline was captured with `rtk proxy` from the start. The partner repo was never written.
- **Commit:** none (a check, not a code change).

**2. [Rule 1 - Bug] Two hand-off sentences did not satisfy the verify's own requirements**
- **Found during:** Task 2 verify (two failed runs before the third passed).
- **Issue:** the full sha appeared once where the check requires at least two, and the batching sentence began with a capital "A client's" where the check matches the exact lower-case phrase.
- **Fix:** restated the full sha as the pin in the replay section (where it is useful), and led the batching item with "In short: a client's batch size ...". No claim changed.
- **Commit:** bd607bfc8.

**Other departures:** the verify blocks were extracted from the plan and run with the partner-baseline files in the scratchpad directory instead of `/tmp` (the session rule); the script bodies are otherwise the plan's text. STATE.md is hand-edited because the state verbs refuse on 04-VERIFICATION.md status `gaps_acknowledged` (as in every earlier Phase 10 plan); `requirements.mark-complete` was not run (Phase 10's ids are not in REQUIREMENTS.md).

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug). **Impact:** none on the delivered records.

## Not done here (hand-off)

- **The README example result is still unrecorded.** The lambda README's example sentence ("The weather today is mild and sunny.", the first replay text) has no live probabilities. Plan 10-09 does not list the README in `files_modified`, and the probe prints checks, not per-text probabilities, so the acceptance replay record holds no number to copy. Filling it needs one more read-only call that returns the probabilities and a small README edit; both are outside this plan and were not made. No number was invented.
- SC-4 (the comparison) stays open by the owner's decision; plan 10-05's cell is not run and plan 10-06 is deferred.
- The old content-addressed weights object under `decide/aprender-mcp-decide/` is still present (the owner's call, from plan 10-12).
- `contracts/aprender/binding.yaml` still carries a dated note naming contract 9.0.0 (from plan 10-10).
- The phase verifier, `10-VERIFICATION.md` and a re-run of the phase learnings extraction have not been run.

## Known Stubs

None.

## Threat Flags

None. Read-only calls to the existing endpoint with synthetic text; no new endpoint, auth path or file access.

## Authentication Gates

None. Auth is off by the owner's D-13 decision, so no credential was needed.

## Self-Check: PASSED

Both created files exist, commits `105c6a0ed` and `bd607bfc8` are ancestors of HEAD, and both verify blocks ended rc=0.
