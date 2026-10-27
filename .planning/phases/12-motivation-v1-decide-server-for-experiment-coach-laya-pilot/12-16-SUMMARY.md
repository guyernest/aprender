---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 16
subsystem: ml-decision-classifier
tags: [bakeoff, handoff, false-assignment, pilot, kev-blocked-budget, claude-md]

requires:
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: "12-05 bakeoff.py, 12-11 gate run (gate_pass), 12-13 deploy and replay, 12-14/15 SetFit cell and Kev arm records"
provides:
  - "12-BAKEOFF-EVIDENCE.json (motivation-bakeoff-v1): Laya, Laya zero-shot baseline, SetFit s32e1b16, Kev blocked:budget; deterministic"
  - "12-HANDOFF.md: partner hand-off leading with false assignment per arm and band, verbatim-rendered tables, pointer-checked numbers table"
  - "CLAUDE.md decision-model row names aprender-mcp-decide-motivation; lambda README server list; laya_train README bakeoff.py row; four claims rows"
affects: []

actuals:
  tokens: 23000
  tasks: 3
  commits: 3

key-files:
  created:
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-BAKEOFF-EVIDENCE.json
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-HANDOFF.md
  modified:
    - CLAUDE.md
    - crates/aprender-mcp-decide-lambda/README.md
    - scripts/laya_train/README.md
    - scripts/laya_claims.tsv

key-decisions:
  - "Kev-4B enters the bake-off as blocked:budget (owner decision, 12-KEV-OWNER-REPLY-2.json); not ranked, no parity or superiority claim; Laya stays served (D-06)"
  - "The gate passed (no waiver), so the hand-off says pilot + gate passed + waiver declared but unused, not 'gate-failed pilot'"

requirements-completed: [D-01, D-03, D-04, D-05, D-06, D-12, D-13, D-14, D-15, D-19, D-22, D-25, D-07]

plan_head_before: 3d24e436c37fd1ba83fbfcb82ecced63e497c9b4
plan_head_after: 2c08921c034c22957347a98323ce4d23cc0b66a3
duration: single session, 2026-10-26
completed: 2026-10-26
status: complete
---

# Phase 12 Plan 16: bake-off, partner hand-off and project docs Summary

**Laya ranks first (macro-F1 0.7476) over SetFit s32 (0.6254); Kev-4B is `blocked:budget` by the owner's decision and unranked; the hand-off leads with the false-assignment rate per arm and length band, and the project documents now name the second served model.**

## Ranking and headline numbers (12-BAKEOFF-EVIDENCE.json)

| rank | arm | macro-F1 | ECE (95 % interval) |
|------|-----|----------|----------------------|
| 1 | laya (shipped seed 13) | 0.7476 | 0.0936 [0.0607, 0.1732] |
| 2 | setfit s32e1b16 | 0.6254 | 0.1111 [0.0733, 0.2069] |
| - | kev | blocked (budget) | - |
| baseline | laya_zero_shot | 0.3696 | 0.1393 |

Findings: none (no reference arm beats Laya by 0.05). Laya's macro-F1, ECE and zero-shot macro-F1 equal the gate record bit for bit.

False assignment (share of the 63 eval `other` rows whose largest non-other probability is above the threshold; the `>=` client_ge counts equal the strict counts at every cell):

| arm | >0.6 | >0.85 | recall @0.6 social / consp | recall @0.85 social / consp |
|-----|------|-------|----------------------------|------------------------------|
| laya | 7/63 = 0.1111 | 4/63 = 0.0635 | 0.7391 / 0.5714 | 0.6087 / 0.3571 |
| setfit | 13/63 = 0.2063 | 0/63 = 0.0000 | 0.6522 / 0.4286 | 0.4348 / 0.2857 |
| laya zero-shot (baseline) | 6/63 = 0.0952 | 1/63 = 0.0159 | 0.0435 / 0.4286 | 0.0000 / 0.0714 |
| kev | blocked (budget) | blocked (budget) | - | - |

By band, Laya's false assignment at 0.6 is 2/28, 1/21, 4/14 (shortest to longest), on very few rows per band. Per-band and per-class figures are noisy (14 conspiratorial and 23 social_affiliative eval rows; the bands hold 2, 5 and 7 conspiratorial rows).

## Pilot / gate status line

PILOT (D-01) on Dr. Iris Yuster's 25 October 2026 draft guide; **gate passed on the first declared run** (`gate_pass`, margin 0.3780, ece_post 0.0936 against the 0.10 bar), **no waiver used** (`motivation-v1-pilot-ece` was declared as a contingency and unused). Stated plainly in the hand-off: the shipped ECE is a narrow pass on a 100-row split (interval upper end 0.1732, seed 17 above the bar, per-band ECE points above 0.10).

## Kev-4B status

`blocked`, failure class `budget`: stopped by the owner's decision (not a runtime failure) after 134 of 395 rows at a median 142.4 s per post (uncached ~6,800-token guide prefill, against ~1 s warm for Laya). Raw uncalibrated zero-shot partial on the 100 eval rows (12-KEV-ARM-EVIDENCE.json `partial_eval`): one-vs-rest AUC conspiratorial 0.9045, social_affiliative 0.6957, other 0.6727; argmax accuracy 0.17 because 95 of 100 rows are predicted conspiratorial (uncalibrated; 14 conspiratorial eval rows, so noisy). All of these sit in the hand-off's numbers table with JSON pointers. No calibrated Kev number exists, so Kev is not ranked and no parity or superiority claim is made. Reopen route: prefix cache of the shared guide (about 60x less compute per post) plus batching, possibly GPU, as a separate spike (not started). D-07 facts kept: branch `spike/016-upstream-sync`, worktree commit 32103ba8, Section 4.5 omitted (D-24).

## Task results

1. **Task 1 (tracer), 638c5ac61.** `bakeoff.py` printed `BAKEOFF OK ... arms=3`; second run `cmp`-identical. Verify: `BAKEOFF EVIDENCE OK ranking ['laya', 'setfit'] findings 0` (PASS).
2. **Task 2, 427b58dc5.** Verify: `HANDOFF OK: tables re-rendered identically, 98 numbers resolved against their records` (PASS). Hand-off built by filling a scratch template (scratchpad `fill_handoff.py`) so the tables are the verbatim `--render-tables` output and the 98 pointer rows are generated from the records rather than typed.
3. **Task 3, 2c08921c0.** `test_documented_paths_exist ... ok` (readme_contract 16 passed); `LAYA CLAIMS OK 144 rows`; the grep chain passed (PASS).

## Deviations from Plan

### Auto-fixed / adapted

**1. [Adaptation] Kev arm passed as blocked, per owner decision.** The plan's happy path expected an `eval-probs.json`; the plan's own blocked branch handled it, so no check was weakened. Kev arm string: `blocked:budget:<12-KEV-ARM-EVIDENCE.json>`.

**2. [Adaptation] Gate passed, so the "gate-failed pilot" wording is conditional and not used.** The plan's verify requires it only for `gate_fail_waived`; the hand-off states pass, no waiver, plus the caveats above.

**3. [Rule 2 - missing critical] Four claims rows added** (CLAUDE.md sha and ECE anchors; lambda README server row and model row, all `evidence` kind pointing at 12-DEPLOY-EVIDENCE.json and 12-GATE-RUN-EVIDENCE.json), so the new documentation sentences are anchored in `laya-claims-check` like the antisemitism ones.

**4. [Note] Some small counts in hand-off prose** (per-band row counts, 2/5/7 conspiratorial per band, "best epoch 2, 5 epochs run") are typed in prose and also appear in the generated tables or numbers table; the verify does not pointer-check prose, only the tables and the numbers table.

Foreign uncommitted changes (`.planning/STATE.md`, `.planning/config.json`, `.planning/state.json`, `justfile`) were not staged. CLAUDE.md and the other doc files had no foreign diff before this plan. STATE.md, ROADMAP.md and REQUIREMENTS.md were not edited (the orchestrator reconciles tracking).

## Known Stubs

None.

## Threat Flags

None. The hand-off carries no post text, account id or bucket name (verified by the Task 2 scan and a grep).

## Self-Check: PASSED

- 12-BAKEOFF-EVIDENCE.json, 12-HANDOFF.md present; commits 638c5ac61, 427b58dc5, 2c08921c0 are ancestors of HEAD; `git rev-list --count` from the recorded plan head is 3.
