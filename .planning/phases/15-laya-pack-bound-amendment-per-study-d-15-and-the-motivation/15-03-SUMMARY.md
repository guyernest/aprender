---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 03
subsystem: decide
tags: [laya, deploy-eligibility, pack-record, cross-cpu, contract, binding-repair]
requires:
  - phase: 15-01
    provides: 15-BOUND-DECISION.json (verify_role role-drift-advisory) and the SC1 anchor 91969e11e
provides:
  - scripts/laya_deploy/laya_eligibility.py (role, record-path, check-record, judge, --selftest)
  - laya-upload and laya-deploy step (5) judged through the helper; the Graviton pack record is the eligibility gate for server-packed runs
  - decide-apr-v1 4.0.0 with the stated role and FALSIFY-DECIDE-APR-015
  - the missing laya-finetune-gate-v1 pilot_waiver_honoured binding row (pre-existing BIND-001 repaired)
affects: [15-02, 15-04, 15-05]
tech-stack:
  added: []
  patterns: [one pure helper owns the decision, recipes only capture rc and print its line; must-match / must-not-match table for the refusal matcher; role as an internal parameter so both consequence tables are tested]
key-files:
  created:
    - scripts/laya_deploy/laya_eligibility.py
  modified:
    - justfile
    - scripts/laya_gates.tsv
    - contracts/decide-apr-v1.yaml
    - contracts/aprender/binding.yaml
key-decisions:
  - "ROLE literal is role-drift-advisory, as 15-BOUND-DECISION.json signed; only the exact anchored RescoreDrift line is demoted to a logged CROSS-CPU FINDING"
  - "Under role-all-advisory (not signed, but implemented and tested) a local refusal is only demoted when the local verify printed a REFUSED line with exit 2 or 3; a bare non-zero exit with no REFUSED line (a cargo or process failure) still blocks"
  - "decide-apr-v1 bumped 3.1.0 to 4.0.0 because pv diff suggested a major bump (the deploy_eligible formula changed)"
patterns-established:
  - "Pack record discovery: LAYA_PACK_OUTCOME is explicit (absent file refuses); a convention record exists only when the run basename is run and <dir>/logs/pack-outcome.json is a file; otherwise the local gate decides"
requirements-completed: [D-08, D-09, D-10, D-01, D-04]
duration: n/a
completed: 2026-10-31
status: complete
actuals:
  tokens: 11000
  tasks: 3
  commits: 4
plan_head_before: bb5d9e391052b02b26dd6efa07e57e337503bb5f
plan_head_after: c717b85ecadb4851b6a8804046f216e10dc7d84b
---

# Phase 15 Plan 03: Pack record as the deploy/upload eligibility gate Summary

**A Graviton-accepted artifact is now upload- and deploy-eligible on this Mac through its pack record bound to the exact bytes by sha256, a Mac `RescoreDrift` refusal is logged as a cross-CPU finding instead of blocking, every other refusal still blocks, a run with no record is judged exactly as before, and decide-apr-v1 4.0.0 says so.**

## Commits

| Task | Commit | What |
|------|--------|------|
| 1 (tracer) | 270ca045c | laya_eligibility.py (role, record-path, check-record, judge, --selftest) and laya-upload wired to it |
| 2 | 8b2517029 | laya-deploy step (5) wired; offline record cases; gate rows; tsv text |
| 3a | 860c53bba | binding.yaml: the one missing pilot_waiver_honoured row, committed alone |
| 3b | c717b85ec | decide-apr-v1 4.0.0, role statement, FALSIFY-DECIDE-APR-015 |

All four descend from the sign-off commit 91969e11e (`SC1 ANCESTRY OK 4 commits`).

## Verification results

- RECORD TRACER OK (real bytes, DRY_RUN, aws recorder empty). Eligible line: `ELIGIBLE pack-record sha256 31ace8ea95ca0c750b6d007d0f4c68e4322d20363117f837673a936d6f696c85 (CROSS-CPU FINDING: REFUSED RescoreDrift which=fine_tuned row=199 max_abs=0.00003439188003540039 bound=0.00003259039634251337 (nothing written))`, then `DRY-RUN: would upload`. The finding log line carried `pack_image=sha256:504084d0...ae3090f3 parity_contract=7c646b74...b267878`.
- RECORD PATH OK.
- `ELIGIBILITY SELFTEST OK 65 cases (2 must-match, 9 must-not-match, roles role-drift-advisory,role-all-advisory)`.
- ROLE PINNED role-drift-advisory (rescore_drift logged, other_local_refusal blocks).
- Disarmed `just laya-deploy-selftest` last line byte-identical `DEPLOY SELFTEST OK (positive dry run SKIPPED: disarmed by LAYA_DEPLOY_SELFTEST_POSITIVE=0)`; cases 60 (helper selftest) and 61-72 (record-a..f through deploy and upload) all present; aws recorder empty.
- `LAYA_GATES_ONLY=deploy-selftest-skip,deploy-refusals,upload-eligibility just laya-gates-selftest`: `DRIFT CHECK OK`, `LAYA GATES ROWS OK 3 of 56` (deploy-selftest-skip 1687 s including the default-armed multi-GB run).
- FULL tier, `LAYA_GATES_FULL=1 LAYA_GATES_ONLY=upload-eligibility`: GREEN (373 s), including the new `smoke-record` must-pass (`eligible: ELIGIBLE pack-record sha256 31ace8ea... (CROSS-CPU FINDING: REFUSED RescoreDrift ...`), `AWS CALLS: 0`.
- BINDING ROW OK 860c53bba7d5, BINDING REPAIR OK, DECIDE-APR OK, DECIDE-APR BINDING OK, SC1 ANCESTRY OK, `scripts/check_ont_ratchet.sh` PASS.

## Mutation table (Discipline 4 and 7)

Each mutation was applied to a copy of the helper and run through `--selftest` (and where noted through the real recipe), then discarded. All seven went RED.

| # | Mutation | First failing instrument |
|---|----------|--------------------------|
| m1 | DRIFT_RE without the leading `^` | selftest `must-not-match leading-space` |
| m2 | check-record skips the sha equality | selftest `check-record record-sha-mismatch` plus the judge sha-mismatch cases under both roles |
| m3 | judge takes the LAST REFUSED line | selftest `judge record-drift-second-refused-line` under both roles |
| m4 | record-path ignores the `run` basename rule | selftest `record-path other-basename`. Note: the plan's RECORD PATH CLI check does NOT catch m4 (`models/decide/laya-stance-64` has no `models/decide/logs/pack-outcome.json`, so the existence rule masks it); the selftest case is the detector |
| m5 | an absent explicit record treated as no record | selftest `check-record record-absent` and judge `record-explicit-absent` cases; recipe level: `just laya-upload` with `LAYA_PACK_OUTCOME` naming a missing file no longer printed `pack record ... does not exist` (case d RED) |
| m6 | record-path prints the convention path when the file is absent | selftest `record-path run-without-record`; the RECORD PATH check's third part printed a path; recipe level: a `run` dir with no record printed `REFUSED eligibility: pack record ... does not exist` instead of the local SyntheticNotDeployable refusal (case e RED) |
| m7 | judge ignores its role parameter (always role-drift-advisory) | selftest `[role-all-advisory] judge record-synthetic`, `record-rc3`, `record-ceiling`, `record-drift-second-refused-line` |

## Pre-existing defect repaired

`make contract-audit-phase8` was red before any Phase 15 change: plan 12-02 (commit 0125d26f8, 2026-10-26) added the `pilot_waiver_honoured` equation and the `#[contract]` attribute on `aprender_decide::verify::decide_pilot_waiver` without a binding.yaml row. It is a local tier3 gate, not run by CI.

- RED before the repair (this tree, after Tasks 1-2): `/tmp/p15-03-t3-red-before.log`, rc 2, exactly one finding, `[ERROR] BIND-001: Equation 'pilot_waiver_honoured' in laya-finetune-gate-v1.yaml has no binding entry`, then `FAIL: unbound equations remain in: contracts/laya-finetune-gate-v1.yaml`.
- Repair: one row (status implemented, signature copied from verify.rs) in commit 860c53bba, binding.yaml alone, +7 lines, nothing removed.
- GREEN after: `/tmp/p15-03-t3-green.log`, rc 0, `Phase 8 source resolution: resolved 45 Phase 8 binding rows to definition sites` and `Phase 8 binding audit: 4 contract(s) audited, no binding finding outside PHASE8_LIVE_EXEMPT=''`.
- Re-observation against the phase-base file (`BINDING=` a copy of binding.yaml at dd3c96937): `/tmp/p15-03-t3-red.log`, rc 2 with the same single BIND-001 line.
- No other row changed; the Makefile, `PHASE8_LIVE_EXEMPT` (still empty), `contracts/laya-finetune-gate-v1.yaml` and `crates/aprender-decide` are byte-identical to dd3c96937. All `make` runs used `rtk proxy make`.

## pv

`pv diff` of HEAD's decide-apr-v1 against the edited file: `Contract diff: v3.1.0 -> v3.1.0 / Suggested bump: major` (deploy_eligible formula and invariants changed, FALSIFY-DECIDE-APR-015 added). Version set to 4.0.0. `pv validate`: 0 errors, 0 warnings; `pv lint` (file form) PASS. No pinned reference to the 3.1.0 version exists in the tree.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Historical eligibility log overwritten by an ad hoc mutation check**
- **Found during:** Task 2 mutation proof (recipe-level m5/m6 baseline)
- **Issue:** my own `just laya-upload` check run wrote `models/decide/eligibility-upload-aprender-mcp-decide-antisemite.log` (gitignored) without a backup, violating the plan's prohibition.
- **Fix:** the Task 1 tracer's backup (`/tmp/p15-03-t1/bk-...`) held the pre-plan content; it was restored byte for byte, then re-backed up and restored around the later FULL run. No cross-cpu-finding log remains (it did not exist before).
- **Files modified:** none tracked.

**2. [Design choice] role-all-advisory demotes only a refusal that printed a REFUSED line (exit 2 or 3)**
- The plan text says any non-zero rc with a valid record is logged under role-all-advisory. A cargo or process failure with no REFUSED line is not a local refusal, so it still blocks (selftest case `record-rc101-no-refused-line` under both roles). The signed role is drift-advisory, so no signed behaviour differs.

### Notes

- bashrs on the extracted recipe bodies: no new finding class versus the baseline; the only increase is two more `SC1028 Parentheses inside [ ]` false positives (message text) in each of laya-deploy-selftest and laya-gates-selftest, a class that already has 26 and 85 hits there. The justfile recipes are not part of the repository's bashrs gate.
- The decide-apr-v1 text states only the signed role's rule; the role-all-advisory forged-record caveat was deliberately not written (plan: say it only when that role is signed).
- `contracts/contracts.nt` is expected to be stale after the new binding row; plan 15-04's graph step regenerates it from a clean worktree (not touched here).

## Authentication gates

None.

## Known Stubs

None.

## Threat Flags

None beyond the plan's threat model (T-15-05..09, 35, 42, 43). No network surface was added; the helper is pure stdlib.

## State updates

See the orchestrator note: `gsd_run query state.*` verbs and the ROADMAP progress verb were attempted after this file was written.

## Self-Check: PASSED

- `scripts/laya_deploy/laya_eligibility.py` present; commits 270ca045c, 8b2517029, 860c53bba, c717b85ec are ancestors of HEAD.
