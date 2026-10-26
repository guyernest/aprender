---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 13
subsystem: deploy-verification
tags: [pmcp.run, lambda, laya, motivation-v1, cold-start, acceptance-replay, pilot]
requires:
  - phase: 12-12
    provides: the live aprender-mcp-decide-motivation server, identity-proven, antisemite shown unchanged
  - phase: 12-09
    provides: the served door (8 texts) in 12-DOOR-EVIDENCE.json
provides:
  - live{} cold and warm timings in 12-DEPLOY-EVIDENCE.json (outcome verified)
  - 12-DEPLOY-OWNER-REPLY.json (skipped, verify ended verified)
  - 12-ACCEPTANCE-REPLAY.json (laya-acceptance-replay-v1, all seven requester tests pass)
affects: [12-14]
tech-stack:
  added: []
  patterns: [every cold sample kept with explicit nulls and a derived proven_cold, timing summaries from measured values only]
key-files:
  created:
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-DEPLOY-OWNER-REPLY.json
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-ACCEPTANCE-REPLAY.json
  modified:
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-DEPLOY-EVIDENCE.json
key-decisions:
  - "Timings are reported, not gated; the recipe's own 30,000 ms per-sample rule decided its verdict (it passed)"
  - "The 8-text step-down is a finding, not a defect: the door admits 8 by count but 8 rows of 105-108 built tokens total 850 > 800, so the largest admitted call is 7 texts"
requirements-completed: [D-12, D-09, D-23, D-01]
status: complete
commits: 3
plan_head_before: 9231795b7ed05d222f31d4b00318a8841899e4f1
plan_head_after: 3fdfa1418c9185761f0796f7d69050424f08ad15
actuals:
  tokens: 30000
  tasks: 3
  commits: 3
duration: ~10 min (Task 1 deploy-verify 3 min 6 s of the live work)
completed: 2026-10-26
---

# Phase 12 Plan 13: live motivation server measured cold and warm, acceptance replay passed Summary

**The live pilot `aprender-mcp-decide-motivation` (artifact d202f495...aecd) passed `laya-deploy-verify` with 4 proven-cold samples at 22.6-25.4 s (worst slack 4,647 ms to the 30 s gateway cap, Phase 10: 4,500 ms) and answers all seven of the partner's function-and-identity tests, with the largest admitted call at 7 texts rather than the door's 8.**

## What was done

- **Task 1 (tracer, 9a66e695f):** ran `just laya-deploy-verify models/decide/laya-motivation-v1.apr aprender-mcp-decide-motivation ze-kasher-dev 2`, rc 0. Verdict line: `DEPLOY VERIFY OK aprender-mcp-decide-motivation: 4 cold samples (both shapes), every one < 30000 ms with CloudWatch cold evidence`. Normalized into `live{}` with every sample kept (explicit nulls, error, graviton, derived `proven_cold`). The plan's Task 1 verify passed.
- **Task 2 (8578107a4):** skipped, since Task 1 ended verified with final true. `12-DEPLOY-OWNER-REPLY.json` records `{schema laya-deploy-owner-reply-v1, skipped true}`. No owner checkpoint fired.
- **Task 3 (3fdfa1418):** acceptance replay with the probe's committed synthetic texts only. The plan's Task 3 verify passed, including the partner-repo `cmp`.

## Results

**Cold (reported, not gated)** — all four samples proven cold from the CloudWatch `decide.load performed_load=true probe_id=` line, all graviton2:

| Sample | Shape | Elapsed ms | load_ms |
|---|---|---|---|
| 1 | concentrated | 25,353 | 15,320 |
| 2 | distributed | 23,066 | 15,533 |
| 3 | concentrated | 25,341 | 15,659 |
| 4 | distributed | 22,620 | 15,263 |

Cold min 22,620, max 25,353, **worst slack 4,647 ms** (Phase 10 antisemite: 4,500 ms, so a 97-token prefix and a similar load left the margin essentially unchanged). Both shapes were sized to 800 tokens; plan-only records: concentrated 2 texts, distributed 8 texts, `min_row_tokens` 97 (equal to P), `max_texts` 8.

**Warm:** five calls 1,226 / 1,227 / 1,253 / 1,309 / 1,314 ms, min 1,226, p50 1,253, max 1,314 (Phase 10 antisemite: 1,083-1,148 ms, p50 1,124; the motivation server is about 11 percent slower warm, plausibly its longer prefix).

**Configuration change recorded:** four `DECIDE_COLD_BUMP` environment updates on `aprender-mcp-decide-motivation` (the measurement mechanism, T-12-62); the antisemite function received no write.

**Acceptance replay (D-12):**

- Identity: `identity_matches` true, sha equal to the pin, labels in task order (warm).
- Three distinct one-text calls (`--replay-from` 0, 1, 2): 105 / 106 / 107 built tokens, 1,210 / 1,210 / 1,272 ms, every check true.
- Largest admitted multi-text call: **7 texts**, 745 built tokens, 6,630 ms. The 8-text call was refused with `classify_max_total_tokens` (rows total 850 > 800; per-text rows 105-108). Recorded under `refused_token_budget`, as the plan's step-down rule prescribes.
- experiment-coach's 4-text batch: admitted whole, 423 built tokens, 3,908 ms, passes all seven replay checks.
- Count refusal at door + 1 = 9 texts: isError, names `classify_max_texts`, the limit 8 and `decide-tool-boundary-v1`, 398 ms.
- Probability sums within 1e-5 (largest deviation 2.2e-8), label equals argmax, three probabilities in task order [social_affiliative, conspiratorial, other].
- All seven requester tests recorded `pass`. Partner repo `/Users/guy/projects/mcp/motivation-experiment`: HEAD `0c46b68...` unchanged and `git status --porcelain` identical before and after. No post text was sent or recorded.

## Findings

- **F-12-13-1 (for the partner and for 12-14):** the door admits 8 texts by count, but the motivation prefix (P 97) makes every built row 105-108 tokens for the synthetic replay texts, so an 8-text call exceeds the 800-token budget (850). Equal-length 8-text batches only fit when each row is at most 100 tokens; 12-DOOR-EVIDENCE.json's `b_times_p` 776 is the minimum (empty-text) case. Batches of 4 (and 7 of these texts) fit with room. experiment-coach splits on a `classify_max_total_tokens` refusal, which the replay record states in `observed`. Not a defect against the contract; worth stating in any partner-facing note, since the README's "8-text" sentence is a count bound, not a promise that eight real posts fit.
- The seven-text call took 6.6 s warm, roughly 0.9 s per text, against 1.2 s for one text: batching is sub-linear but a full batch is several seconds, which the partner should expect.

## Deviations from Plan

**1. [Rule 3 - Blocking] Over-limit refusal line carries booleans, not the literal bound names**
- **Found during:** Task 3
- **Issue:** the probe's `replay_over_limit` line reports the refusal as three booleans (`names_count_bound`, `names_limit_value`, `names_contract`) computed from the server's message; it does not print the message. The plan's verify checks that the literal string `classify_max_texts` appears in `expected_count_refusal.line`. The Phase 10 record had the same line shape.
- **Fix:** kept the probe line verbatim and added a clearly labelled `derived_from_flags` object inside it that spells out the bound, the limit and the contract, each set only when the matching probe flag is true. No new measurement is claimed.
- **Files modified:** `12-ACCEPTANCE-REPLAY.json`
- **Commit:** 3fdfa1418

**2. [Rule 3 - Blocking] Partner-repo status baseline had no trailing newline**
- **Found during:** Task 3 verify
- **Issue:** the `git status --porcelain` baseline captured through the tool hook was 205 bytes (no final newline); the same command later wrote 206. `diff` confirmed the content was identical and HEAD matched.
- **Fix:** regenerated the baseline from the identical content so the plan's byte-exact `cmp` compares like with like. The partner repo was never touched (read-only commands only).
- **Files modified:** none in the repo (`/tmp` baseline only)

**3. [Note] Requester-test values are objects, not prose strings**
The plan's verify accepts only `"pass"` or `{result: "pass"}`, unlike Phase 10's `"pass: ..."` strings, so each test is `{"result": "pass", "proof": "..."}`.

**Total deviations:** 2 auto-fixed (Rule 3), 1 note. **Impact:** none on any measurement or verdict.

## Authentication Gates

None. The AWS profile and the open (auth off) endpoint worked without prompts; no tokens were read, printed or recorded.

## Issues Encountered

None beyond the deviations above. Task 2's owner checkpoint did not fire.

## Not done by this plan (orchestrator-owned)

STATE.md, ROADMAP.md and REQUIREMENTS.md were not updated: `gsd_run` state/roadmap tracking errors on `04-VERIFICATION.md` (`gaps_acknowledged` is outside the tool's closed set) and that file was deliberately not edited. The orchestrator should record plan 12-13 as complete.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-12-44 (every sample kept, proven_cold derived, summaries from measured values) and T-12-46 (synthetic texts only, partner repo unchanged) are mitigated and re-derived by the verifies.

## Self-Check: PASSED

- 12-DEPLOY-EVIDENCE.json (live{}), 12-DEPLOY-OWNER-REPLY.json, 12-ACCEPTANCE-REPLAY.json present; the Task 1, Task 2 and Task 3 verify blocks pass.
- Commits 9a66e695f, 8578107a4, 3fdfa1418 are ancestors of HEAD; `git rev-list --count` from the ledger base 9231795b7 is 3.
- No 12-digit account id, bucket name or token in the three evidence files.
