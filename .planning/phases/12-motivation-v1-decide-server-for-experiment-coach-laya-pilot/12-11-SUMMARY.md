---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 11
subsystem: laya-finetune-gate
tags: [laya, gate-run, pilot-waiver, motivation-v1, pack, verify, mps]
status: complete

requires:
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: "12-TASK-EVIDENCE (approved task variant a, P 97), 12-PROBE-EVIDENCE (E 12), 12-SLICE-OVERRIDE-EVIDENCE (0.25, not_needed), 12-DOOR-EVIDENCE (door 8), pilot waiver machinery (12-02, 12-03)"
provides:
  - "contracts/laya-finetune-gate-v1.yaml 5.1.1: pilot_waivers declaration motivation-v1-pilot-ece and the motivation_v1 run declaration, committed before the run"
  - "models/decide/laya-motivation-v1.apr (gitignored): GATE PASS, deploy-eligible, sha256 d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd"
  - "12-GATE-RUN-EVIDENCE.json (index, schema v3), 12-GATE-RUN-ATTEMPT-1.json (write-once record), 12-GATE-OWNER-REPLY.json (skip record)"
affects: [12-12, 12-13, 12-16]

actuals:
  tokens: 4000
  tasks: 3
  commits: 4
plan_head_before: 9b7d8b307f22c2524d6e1363d472ae468f016c3d
plan_head_after: 7c9918e1a24c11568f6253f3e086aaff2e7d4b83

tech-stack:
  added: []
  patterns:
    - "declaration commit before the run, its unix time compared with the run's started_at by the verify"
    - "write-once per-attempt record indexed by sha256; the index's top level mirrors the final attempt"

key-files:
  created:
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-GATE-RUN-EVIDENCE.json
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-GATE-RUN-ATTEMPT-1.json
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-GATE-OWNER-REPLY.json
  modified:
    - contracts/laya-finetune-gate-v1.yaml
    - crates/aprender-decide/src/verify/tests.rs

key-decisions:
  - "Contract version published as 5.1.1 (pv diff classified the additive declaration PATCH, followed as the plan says)"
  - "The declared waiver was not needed: the one gate run passed both clauses, so waived is null and the outcome is gate_pass (a pilot, not a gate-failed pilot)"

requirements-completed: [D-13, D-17, D-19, D-20, D-21, D-01]

metrics:
  duration: "about 45 min wall (training 12.5 min, pack and verify and plan-only about 2 min of cargo build time each)"
  completed: 2026-10-26
---

# Phase 12 Plan 11: motivation-v1 declared gate run Summary

**The one declared motivation-v1 gate run PASSED the unchanged gate (margin 0.3780, ece_post 0.0936 on seed 13, the median-ECE seed); the pilot waiver was declared but not used, and a deploy-eligible artifact exists.**

## Declaration (Task 1, tracer)

Declaration commit `e3bef4cb0a4d39104314944e2f64015ed93c0f8a` (committed_at 1793049333) added, before any training:

- `pilot_waivers.declarations[motivation-v1-pilot-ece]`, waives `[ece_post]`, applies_to the sha256s of task.json (`f74a3ad3...`), train.jsonl (`f1d84f19...`), eval.jsonl (`ee95d71f...`) and recipe_id `f88f5b71ef925fdabb30d26a15c2eb5195a151dadfe0882c858b73ca7bf54d7e` (production, shots_per_class 188, E 12, seed 13, early_stopping, the seed-selection declaration).
- the top-level `motivation_v1` block (pilot, guide docx sha `0e1d5a35...` "25 October 2026 draft", E 12, seeds 13/17/23 median-ECE, calibration slice 0.25 as the contract constant per 12-08 `not_needed`, eval fraction 0.25, claim scope).
- `metadata.version` 5.1.0 to 5.1.1 (`pv diff` suggested patch). Policy revision recorded: `{5.1.1, contract blob 0af135e86bb35912109b3c1898e082c32aaf39fb}`.
- `waiver_real_contract_declares_nothing_yet` replaced by `waiver_real_contract_declarations_parse` in the same commit (listed `ok`); FALSIFY-LAYA-GATE-020's `test:` field renamed to match.

Round trip: `contract.pilot_waiver_declarations()` returns the entry; `gate.matching_pilot_waiver` returns `{id motivation-v1-pilot-ece, waives [ece_post]}` for an ECE-only failure and None for `[margin]` and `[margin, ece_post]`. `just laya-train ... --epochs 99` exits 2 with `REFUSED epochs: --epochs 99 is outside [4, 12] at 188 shots/class` and wrote no run dir. The skeleton is commit `ed4d61bcb`.

## The run (Task 2, attempt 1 of the cap of 2)

Started at unix 1793049437 (104 s after the declaration commit), detached under the host lock, rc 0, 752 s wall. `RECIPE WRITTEN f88f5b71ef925fdabb30d26a15c2eb5195a151dadfe0882c858b73ca7bf54d7e stopping=early_stopping epochs_max=12` equals the declared recipe_id. All three `DEVICE seed=<s> requested=mps used=mps:0 torch=2.14.0` lines agree.

| seed | macro-F1 | ece_post | margin | T applied | clamp | best / run epochs | pass |
|------|----------|----------|--------|-----------|-------|-------------------|------|
| 13 | 0.7476 | 0.0936 | 0.3780 | 4.648 | no | 2 / 5 | yes |
| 17 | 0.7197 | 0.1197 | 0.3502 | 3.730 | no | 2 / 5 | no |
| 23 | 0.7405 | 0.0914 | 0.3709 | 5.000 | yes | 2 / 5 | yes |

```
MEDIAN seeds=13,17,23 rank_keys=935,1196,914 shipped=13 (policy median_ece, tie_break smaller_seed)
NOISE which=fine_tuned max_abs=1.820794e-05 bound=7.283177e-05 argmax=100/100 t_applied=4.647908
NOISE which=zero_shot max_abs=1.252460e-05 bound=5.009839e-05 argmax=100/100 t_applied=1.760152
RESULT zero_shot macro_f1=0.3696 f_avg=null ece=0.1393 | fine_tuned macro_f1=0.7476 f_avg=null ece_pre=0.1786 ece_post=0.0936 nll=0.5843 | margin=0.3780 (need >= 0.05) ece_post (need <= 0.1)
GATE PASS
```

No `PILOT WAIVER` line (none needed). Outcome `gate_pass`, failure_kind null, pilot label `pilot`.

- Pack: `PACKED models/decide/laya-motivation-v1.apr sha256=d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd ... shipped_seed=13 argmax=100/100 waived=none`.
- ONE verify: `deploy_eligible true`, `artifact_sha256` d202f495..., recomputed margin 0.37803, ece_post 0.09355, `waived null`.
- Inspect: labels `[social_affiliative, conspiratorial, other]`, variant production, recipe_id as declared, waiver none, embedded gate pass.
- Artifact: `models/decide/laya-motivation-v1.apr`, 846,196,612 bytes (807 MiB), sha256 `d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd`.
- Plan-only door check ok: `min_row_tokens` 97 == P, `max_texts` 8 == door, distributed texts 8, concentrated texts 2 == min(2, 8), `tokens_planned` 800 <= 800 in each (not compared with door x P).
- Calibration slice counts 17 / 11 / 47 == max(2, ceil(0.25 x n_c)) for train counts 66 / 41 / 188 (the declared fraction reached the trainer).
- Record `12-GATE-RUN-ATTEMPT-1.json` sha256 `71cb849ba9954c527e7165c5d87b1287969c1271d724b5821e273950cc02a040`, indexed in 12-GATE-RUN-EVIDENCE.json, whose top level mirrors it (run_dir, artifact.path, artifact.sha256, waived, gate, outcome).

## Final disposition (Task 3)

Skipped: the final attempt is deploy-eligible, so `12-GATE-OWNER-REPLY.json` is `{skipped true, final_attempt 1, outcome gate_pass, option null}`. No rerun authorization exists and none was needed; one declared run, no second attempt.

## Honest scope

- Claim scope is the contract's `eval_set.claim`: in-distribution margin and calibration on a held-out 25 % of the same file (100 rows; 14 conspiratorial), not robustness to a shifted population; the source batch is unmeasured.
- `ece_post` 0.0936 passes with 0.0064 to spare. Seed 17 fails ECE (0.1197) and the seed spread is 0.0914 to 0.1197; the median rule chose a passing seed, but ECE here sits near the threshold. 12-07's warning (median holdout ECE 0.117 at the spike) did not materialise on this run; it remains plausible on a retrain.
- Seed 23's temperature hit the 5.0 clamp; it did not ship and is reported only.
- This is a pilot on the 25 October draft guide, not a frozen one (D-01): retrain after the freeze.
- Training stops at epoch 5 on every seed (best epoch 2): `ce_last` reaches 0.0000 by epoch 3, as 12-07 anticipated.

## Deviations from Plan

None - plan executed exactly as written. Choices inside the plan's latitude:
- Version bumped to 5.1.1 because `pv diff` suggested a patch bump.
- The Rust waiver test was verified by running the compiled test binary directly (`waiver_real_contract_declarations_parse ... ok`), because the rtk hook condenses `cargo test` output so the plan's literal `grep ... ... ok$` on the cargo log cannot match.
- `pv extract contracts --check` fails in this dirty main tree (the known untracked `.claude/worktrees` pollution rewrites `contracts.nt` unresolved-reason strings; I reverted that file, not committed). Proven current in a clean detached checkout of the declaration commit: `pv extract contracts --check` rc 0. `pv census` is byte-identical to `contracts/census.json`.
- The record carries `report_inputs_sha256_extra` (base_model and tokenizer_json hashes from the report) and `variance`; `inputs_sha256` holds the three data hashes so it equals the skeleton.

## Authentication gates

None. No AWS or pmcp.run action was taken.

## Known Stubs

None.

## Tooling note

`gsd_run` state and roadmap updates were not run: the known `status: gaps_acknowledged` in the Phase 4 verification file makes `state.*` and `roadmap.*` queries error, and the orchestrator owns tracking. STATE.md, ROADMAP.md and REQUIREMENTS.md were not touched. The foreign diffs in STATE.md, config.json, state.json and justfile were left unstaged and untouched.

## Self-Check: PASSED

- Files exist: 12-GATE-RUN-EVIDENCE.json, 12-GATE-RUN-ATTEMPT-1.json, 12-GATE-OWNER-REPLY.json, models/decide/laya-motivation-v1.apr (sha256 matches the record).
- Commits in HEAD history: e3bef4cb0 (declaration, before the run), ed4d61bcb (skeleton), 1064ec9aa (attempt record and index), 7c9918e1a (skip record).
- Task 1, 2 and 3 verifies re-run and green (`DECLARATION OK`, `GATE RUN RECORD OK attempts 1 final gate_pass`, `GATE CHECKPOINT RECORD OK attempts 1 skipped`).
