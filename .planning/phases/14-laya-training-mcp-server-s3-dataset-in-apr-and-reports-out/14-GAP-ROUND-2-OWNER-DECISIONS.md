# Phase 14, gap round 2: owner decisions (2026-10-30)

Recorded by the orchestrator of `/gsd-plan-phase 14 --gaps`. The answers came from the owner through AskUserQuestion in that session. They respond to the two gaps still open in `14-VERIFICATION.md` (verified 2026-10-30T23:59:00Z, gaps_found, 13/16).

| # | Question | Owner answer (verbatim option) |
|---|----------|--------------------------------|
| 1 | Where does the D-15 amend-per-study change to `contracts/laya-parity-v1.yaml` live? Phase 14 truth 9 promises no contract change (D-22/D-05). | **New Phase 15 (Recommended).** A new roadmap phase owns the contract amendment and the D-16 reproduction. Phase 14's gap plans only record the hand-off, and truth 9 stays true. |
| 2 | How is D-16 closed? Seed 17's margin of 0.2860 is below the 0.30 floor, so re-packing under a looser bound alone would not pass. | **Re-reproduce after D-15.** A new declaration and a fresh SageMaker gate run (about 1 hr of ml.g5.xlarge), after the amended bound lands. |
| 3 | D-15 parameters: how is the per-set (fine_tuned, zero_shot) bound set? | **Planner argues, I approve.** The plan derives a per-set bound from `14-DRIFT-EVIDENCE.json` and stops at a `checkpoint:decision` for owner sign-off before any contract edit. |

## Consequences for Phase 14

- D-16 is not waived. No `overrides:` entry is authorized by these answers.
- The amendment and the reproduction are deferred to Phase 15, not dropped. Phase 14's gap closure is the hand-off: the roadmap entry, and a verification that lists both truths as deferred to Phase 15.
- The paid compute for the reproduction is above the CLAUDE.md check-in threshold of 1 hr on a non-lambda-vector host. Answer 2 approves the run in principle. Each AWS command is still owner-run, as in every live plan of Phase 14.
- The bound parameters are not chosen yet. Phase 15 must not edit the contract before the owner signs off at its checkpoint.
