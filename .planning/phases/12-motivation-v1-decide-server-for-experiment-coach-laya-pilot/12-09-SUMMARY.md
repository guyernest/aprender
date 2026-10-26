---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 09
subsystem: decide-tool-boundary
tags: [laya, decide, motivation, tool-boundary, contract, door, served-tasks, pilot]
status: complete

requires:
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: "12-TASK-EVIDENCE.json: owner variant (a), measured P 97, chosen_max_texts 8 (plan 12-01)"
provides:
  - "decide-tool-boundary-v1 10.1.0: top-level served_tasks table listing both served decide models with shortest built row and door"
  - "tests::every_served_task_fits_its_door: the budget check holds for every served task, not only the antisemitism one"
  - "12-DOOR-EVIDENCE.json: branch, P, door, contract versions, pv diff classification"
affects: [12-11, 12-12, 12-13]

actuals:
  tokens: 9000
  tasks: 2
  commits: 3
plan_head_before: 0a548d24087a5ddc5563077111f74fe4371c28b0
plan_head_after: bcadb5c4d13a3e8f0225a30dee9c40b602182061

tech-stack:
  added: []
  patterns:
    - "one table per served task: a boundary contract that serves N tasks records each task's measured shortest row and asserts every row against the shared budget"

key-files:
  created:
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-DOOR-EVIDENCE.json
  modified:
    - contracts/decide-tool-boundary-v1.yaml
    - crates/aprender-mcp-decide/src/tests.rs

key-decisions:
  - "Branch (a) taken (owner reply approve-a f74a3ad3cfc0, 2026-10-26): motivation-v1 P 97, 8 x 97 = 776 <= 800, door stays the contracted classify_max_texts 8; no per-artifact door (D-26 branch b) was implemented"
  - "decide-tool-boundary-v1 raised 10.0.0 -> 10.1.0 (MINOR by hand): pv diff reports the files identical because it models neither constants: nor the new table, so it offered no bump; the change only adds a record"

requirements-completed: [D-09, D-26, D-12]
---

# Phase 12 Plan 09: Tool-boundary contract for two served decide tasks (branch a) Summary

**Branch (a), P 97, served door 8 (unchanged), decide-tool-boundary-v1 10.0.0 -> 10.1.0: the contract now lists both served decide models (antisemite 86, motivation 97) in a `served_tasks` table and a mirror test holds every row to the 800-token budget.**

## What was built

- `served_tasks:` (top-level, 2 rows) in `contracts/decide-tool-boundary-v1.yaml`: `aprender-mcp-decide-antisemite` (laya-antisemitism-v1, 2 labels, 86 -> 8) and `aprender-mcp-decide-motivation` (laya-motivation-v1 pilot, 3 labels, 97 -> 8), each with the record its measurement is read from. `served_task_min_row_tokens` stays 86 and its comment now says it is the antisemitism task's.
- `tests::every_served_task_fits_its_door` (`crates/aprender-mcp-decide/src/tests.rs`): per row, `min_row_tokens` in 1..=row_max_tokens, door in classify_min_texts..=classify_max_texts, `door x min_row_tokens <= classify_max_total_tokens`; the antisemite row equals `served_task_min_row_tokens`; both servers present once. Observed RED with the motivation row mutated to 101 (`8 texts x 101-token rows = 808 > budget 800`), then restored.
- v10.1.0 amendment paragraph naming Phase 12 D-09/D-26 and branch (a).
- `12-DOOR-EVIDENCE.json` (schema laya-door-evidence-v1).

Not built, by design (branch a): `classify_min_served_texts`, the `served_max_texts` equation, `served_for` / `measure_min_row_tokens` / `for_model`, the derived-limits wiring through build_server / ClassifyService::served / parse_args / the probe, and any README or claims change. Both READMEs' 8-text sentences stay true for both servers.

## Verification

- `cargo test -p aprender-mcp-decide --lib`: 33 passed (`every_served_task_fits_its_door` and `bounds_match_contract` listed ok); `--test e2e_stdio` ok; `cargo test -p aprender-mcp-decide-lambda --lib` ok.
- `cargo clippy -p aprender-mcp-decide -p aprender-mcp-decide-lambda --lib --examples --no-deps -- -D warnings` clean; `cargo fmt -p aprender-mcp-decide -- --check` clean.
- served_tasks vs 12-TASK-EVIDENCE.json check (variant a, P 97, door 8, label_count 3, antisemite 86 -> 8): `SERVED TASKS OK`. Door code and the lambda crate untouched (`git diff` empty; no `served_for`).
- `pv validate`: 0 errors, 0 warnings. `pv census` equals `contracts/census.json` (fresh; no change needed). `pv diff` old vs new: "Contracts are identical" (no bump offered).
- `just laya-claims-check`: `LAYA CLAIMS OK 140 rows`. Lambda README unchanged since the plan start.
- Door evidence validator (variant, P, door, antisemitism door 8, version moved): `DOOR EVIDENCE OK`.

## Deviations from Plan

None in code or scope. One verification caveat, recorded rather than fixed:

- `"$PV" extract contracts --check` reports "contracts.nt differs from a fresh extraction" in this dirty main tree, while it passes in a clean checkout (`git worktree add` of the contract commit, since removed) and a plain `pv extract contracts` rewrites byte-identical files. The committed `contracts.nt` / `shapes.ttl` are therefore current (the new table is not encoded in them, so no regeneration commit was needed). The main-tree discrepancy is the known untracked-path pollution (08-33: `.claude/worktrees`, untracked `.planning` dirs); the exact cause was not isolated. The plan's literal Task 2 verify line for that check will fail from this tree for that reason.

## Known tooling issue (tracking left to the orchestrator)

`gsd_run query state.*` / roadmap updates were not run: `04-VERIFICATION.md` has `status: gaps_acknowledged` (outside the closed set) and the orchestrator asked that it not be edited. STATE.md, ROADMAP.md and REQUIREMENTS.md were not touched by this plan (they carry foreign uncommitted changes). Requirements to mark: D-09, D-26, D-12.

## Known Stubs

None.

## Threat Flags

None. No new endpoint, auth path or schema change at a trust boundary; no dependency added (Cargo.lock unchanged). T-12-29/30 are moot under branch (a) (no derivation, door unchanged at the contracted 8, batches of 4 fit); T-12-31 mitigated by `served_tasks` plus `every_served_task_fits_its_door`.

## Task commits

| Task | Commit | Content |
|------|--------|---------|
| 1 (tracer) | 5db72b75c | served_tasks table, served_task_min_row_tokens comment, `every_served_task_fits_its_door` |
| 2 | afe44f917 | contract 10.1.0 amendment (version bump) |
| 2 | bcadb5c4d | 12-DOOR-EVIDENCE.json |

## Self-Check: PASSED

- FOUND: contracts/decide-tool-boundary-v1.yaml (version 10.1.0, served_tasks), crates/aprender-mcp-decide/src/tests.rs, 12-DOOR-EVIDENCE.json
- FOUND commits 5db72b75c, afe44f917, bcadb5c4d (ancestors of HEAD)
