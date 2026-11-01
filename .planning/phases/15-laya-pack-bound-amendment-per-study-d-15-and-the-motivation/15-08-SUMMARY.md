---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 08
subsystem: infra
tags: [laya, trust-bar, gate-run, not-reproduced, owner-run, pack-bound-amendment]
requires:
  - phase: 15-07
    provides: the committed epochs-4 declaration (c078910ba, sha256 1471c106...) and the orchestrator's 15-RUN-GO.json (a2f22a639)
  - phase: 15-06
    provides: the amended pack image dd38865c... and the laya-parity-v1 A2 bound
provides:
  - 15-GATE-RUN-EVIDENCE.json (laya-gate-run-evidence-v3): the single paid attempt, its evaluator verdict (not-reproduced) and next_trusted false
affects: []
tech-stack:
  added: []
  patterns: [verdict from the pure evaluator over the MATERIALIZED committed declaration; attempt consumption classified by the CONSUMPTION RULE, never one missing file]
key-files:
  created:
    - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GATE-RUN-EVIDENCE.json
  modified: []
key-decisions:
  - "Verdict not-reproduced, next_trusted false: the gate passed and the Graviton pack accepted a deploy-eligible artifact, but the shipped-seed margin 0.2795 is below the declared 0.30 floor; no waiver, no overrides entry, no band change (D-02, D-13)"
requirements-completed: [D-02, D-03, D-09, D-12, D-13, D-14, D-16, D-17]
duration: n/a
completed: 2026-11-01
status: complete
actuals:
  tokens: 30000
  tasks: 3
  commits: 3
plan_head_before: a2f22a639094c38dcbf490eaa785c8bc7d1a41d9
plan_head_after: c259f972e
---

# Phase 15 Plan 08: The trust-bar attempt Summary

**The one declared attempt ran once under the amended pack bound and was judged not-reproduced: the gate passed (seed 13 shipped, ECE 0.0973 inside the band) and the Graviton pack task accepted a deploy-eligible artifact, but the margin 0.2795 sits below the declared 0.30 floor, so SC4 stays unmet and the Phase 14 training server stays untrusted (`next_trusted` false).**

## Result

| Item | Value |
|---|---|
| Task id | 604c5a87-83a1-4321-977c-c6ec624d4331 (owner ran validate, train, poll, fetch, hygiene; no retry, nothing cancelled) |
| Authorization | 15-RUN-GO.json (a2f22a639), declaration sha256 1471c10638e78e924229f0038d50536b813190b89b81807a9b499452cd0e5f86, declaration committed 04:40:28Z, run started 05:01:45Z |
| Consumption (rule) | consumed: outcome exit_class `pass`, `GATE PASS` line, gate report present |
| Evaluator (materialized declaration) | class `defect` (out-of-band), verdict_if_final `not-reproduced`; must clauses all true (deploy_eligible, device cuda, recipe a9f1c4a7..., inputs, seeds 13/17/23) |
| Band | margin 0.2795 outside [0.30, 0.45]; ece_post 0.0973 inside [0.07, 0.12] |
| Pack | ran and accepted, deploy_eligible true, apr sha256 42efa308c305d3618fa0a7d69a8cd4b1dd03a354875bbe29568f3faa0b94feab; pack_image_digest equals 15-ROLL-EVIDENCE image.digest (dd38865c...), parity_contract_sha256 equals the committed contract (466891fa...); rescore 5.87e-06 (bound 5.0e-05) and zero-shot 1.73e-05 (bound 3.0e-05) |
| D-09 on the trust-bar bytes | DRY_RUN `laya-upload` for server aprender-mcp-decide-motivation: `ELIGIBLE pack-record sha256 42efa308... (local verify accepted)`, no cross-CPU finding, aws recorder empty (0 calls), nothing uploaded or deployed |
| next_trusted | false (class is not pass, verdict is not reproduced) |

Per-seed: seed 13 margin 0.2795 ece 0.0973 pass (shipped by the median-ECE rule); seed 17 margin 0.3122 ece 0.0923 pass; seed 23 margin 0.3955 ece 0.1096 fail (above the 0.10 gate bar). The claim scope is the declaration's literal: gate outcome and band only, not an identical reproduction of attempt 1 or of Phase 12.

What this does and does not settle: the amendment removed the pack refusal (attempt 1's blocker), so SC1 to SC3 stand on their own evidence from plans 15-01 to 15-06. The draw itself missed the owner's band; per D-13 nothing further is authorized or planned here, and a further attempt is the owner's decision.

## Task Commits

1. 33b092be4 - Task 1 (tracer): evidence skeleton, binding hashes, controls, owner commands
2. 7e520a8a6 - orchestrator: owner replies recorded verbatim (counted in `commits`)
3. c259f972e - Task 3: verdict, evaluation, pack record, local verify, hygiene, findings

## Verification

- TRUST BAR READY and FAILURE PATHS OK (before the run); TRUST BAR RECORDED verdict=not-reproduced next_trusted=False incomplete=none (after it).
- The evaluator was re-run on a copy materialized from c078910ba (hash re-checked, working file equal, exactly one commit on the declaration); the recorded evaluation equals its output.

## Deviations from Plan

**1. [Rule 3 - verify adjustment, directed by the orchestrator] run-go check.** The owner's literal reply was "go". The Task 1 verify tests `'run-go' in (owner_reply_verbatim + ' ' + interpreted_as)` instead of the bare substring in `owner_reply_verbatim`; 15-RUN-GO.json was not edited.

**2. [Rule 3 - verify adjustment, the 15-07 lesson] structural overrides check.** The "no overrides" assertions (Task 1 and Task 3 verifies) use a recursive key check over the parsed evidence instead of the substring `'"overrides"' not in txt`. The identifier scans (bucket, endpoint host, 12-digit run, ARN, presigned) stay textual.

**3. [Rule 1 - bug in my owner command H, found by the owner's run] lock read-back key.** My H command used the key attributes `owner`/`task_id`, but the lock table's partition key is `owner_id` (deploy-extensions/lib/laya-trainer-stack.ts:151), so the first `get-item` failed with ValidationException. The orchestrator recorded a corrected read-back: lock row task.status `completed`, which `crates/aprender-mcp-laya-train/src/lock.rs:125` defines as released (the retained envelope holder is by design). `owner_commands.H` in the evidence is the text that was run; the correction is in `owner_replies` and `findings`. I wrote the key names from `sfn.rs` without reading the table definition; no harm resulted because the read-back was read-only.

**4. [Scope] billable_seconds not recorded.** The owner reply carried no describe-training-job read-back, so `billable_seconds` is null with a note; `train_seconds` (1159.0) and `pack_seconds` (149.31) are from the fetched outcome records.

**5. [Plan adaptation] per-command preamble.** Each owner command repeats the P0 preamble because each bang call is a fresh shell; the task id travels through a gitignored file.

## Known Stubs

None.

## Threat Flags

None. No file under crates/, contracts/ or Cargo.* was touched; no waiver or overrides entry; the declaration was never edited; no account id, bucket, ARN, host or presigned URL is in the committed evidence.

## Issues Encountered

`gsd_run query state.*` verbs may fail on 04-VERIFICATION.md status `gaps_acknowledged` (known); reported, not re-keyed.

## Self-Check: PASSED

- 15-GATE-RUN-EVIDENCE.json exists; commits 33b092be4, 7e520a8a6 and c259f972e are ancestors of HEAD.
