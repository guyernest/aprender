---
phase: 10-antisemitism-v1-decide-server-for-experiment-coach
plan: 07
subsystem: infra
tags: [laya, deploy, pmcp-run, resolver-proof, dry-run, door, owner-checkpoint]

requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: "plan 10-04: the deploy-eligible artifact (sha256 2293bebb, GATE PASS); plan 10-11: the door recorded at B = 8 (no-op)"
provides:
  - "10-LIVE-DEPLOY-EVIDENCE.json (laya-live-redeploy-v1): resolver proof 0.28.1, offline dry run, offline door check, dry-run deploy config, decision null"
  - "models/decide/resolver-proof.txt re-proven for cargo-pmcp 0.28.1 at SDK d0348451 (gitignored)"
affects: [10-08, 10-09, 10-10]

status: partial
status_note: "Task 1 (tracer) complete and committed. Task 2 (checkpoint:decision, gate blocking-human, D-13/D-18) is AWAITING THE OWNER; this plan is NOT complete and no live step may run."

actuals:
  tokens: 6000
  tasks: 1
  commits: 1
plan_head_before: 659dbeb05f140ce92db2a63a0df5eb366631e434
plan_head_after: 9c5d2170b

key-files:
  created:
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-LIVE-DEPLOY-EVIDENCE.json
  modified: []

key-decisions:
  - "No decision is recorded: D-13 (replace the stance demo? auth posture?) and D-18 (deploy access) belong to the owner at the blocking checkpoint; the evidence's decision stays null"

requirements-completed: []
requirements-partial: [SC-5]
---

# Phase 10 Plan 07: Offline deploy readiness and the D-13 owner checkpoint Summary

**The existing pmcp.run deploy path accepts `laya-antisemitism-v1.apr` offline end to end with zero AWS calls (DEPLOY SELFTEST OK, AWS CALLS: 0), the stale 0.24.3 resolver proof is replaced by a 0.28.1 one, and the compiled door admits the task (min_row_tokens 86, texts 8); the plan stops at the owner's blocking D-13 decision.**

## Status

Task 1 done and committed (`9c5d2170b`). Task 2 is a `checkpoint:decision` with `gate="blocking-human"`; it is not self-approved. Plans 10-08 and 10-09 must not start until the owner replies and the decision is recorded in `10-LIVE-DEPLOY-EVIDENCE.json`. `requirements-completed` is empty on purpose: SC-5 is only half-met (the offline half).

## Measured results (Task 1)

Artifact: `models/decide/laya-antisemitism-v1.apr`, `shasum -a 256` = `2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e` (equals the gate evidence).

**Resolver proof** (`just laya-resolver-proof /Users/guy/Development/mcp/sdk/rust-mcp-sdk d0348451 /tmp/p10-07-proof`, rc 0, read from the rc file):

| key | old (stale) | new |
|-----|-------------|-----|
| cargo_pmcp_version | 0.24.3 | 0.28.1 (equals `cargo pmcp --version`) |
| sdk_commit | e0561f8c9086aec56a7f70a3a51537f0671d3ffa | d03484517dce988355b7e9bf3dd2746eec188019 |
| builder_rs_last_commit | c04fb4ccb1418e7f6283954d2686bdbf3fec7ce8 | b65d354ff23c971b8353476ed7fcbde661b2ae41 |
| proven_at | 2026-09-27T04:05:49Z | 2026-10-22T13:31:36Z |

The old proof's full text was kept at `/private/tmp/claude-501/-Users-guy-Development-machine-learning-aprender/4701d6fa-0895-4bdf-a251-5ce757626b3e/scratchpad/resolver-proof.0.24.3.txt`. RESEARCH Pitfall 5 is confirmed and cleared: `laya-deploy` would have refused on the stale proof.

**Deploy selftest, positive leg on the new artifact** (`DRY_RUN=1` with the four `LAYA_ELIGIBLE_*` variables, rc 0, wall 1456 s under load 14-45):

```
DRY-RUN OK aprender-mcp-decide (sha256 2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e; nothing deployed)
RESOLVER PROOF: crates/aprender-mcp-decide-lambda (cargo-pmcp 0.28.1, sdk d03484517dce)
BOOTSTRAP aarch64 OK .../target/aarch64-unknown-linux-gnu/release/bootstrap (26193480 bytes)
AWS CALLS: 0
DEPLOY MARKERS: 0
DEPLOY SELFTEST OK
```

**Offline door check** (`DRY_RUN=1 just laya-deploy-verify models/decide/laya-antisemitism-v1.apr`, rc 0, 31 s, ends `DRY-RUN: stopped before the network (no AWS call, no HTTP request)`):

| shape | texts | tokens_planned | min_row_tokens | max_texts | max_total_tokens |
|-------|-------|----------------|----------------|-----------|------------------|
| concentrated | 2 | 800 | 86 | 8 | 800 |
| distributed | 8 | 800 | 86 | 8 | 800 |

min_row_tokens (Rust) equals P (Python) = 86; texts equal B = 8 (concentrated: min(2, B)); labels in order `not_antisemitic`, `antisemitic`. As the scope notes say, `tokens_planned` is the request total (800), not B x P (688); the assertions are on min_row_tokens and texts.

**Dry-run deploy config** (`DRY_RUN=1 just laya-deploy-config <apr> off aprender-mcp-decide dev ze-kasher-dev`, rc 0): s3 key `decide/aprender-mcp-decide/2293bebb...c776e.apr` on `dry-run-bucket`, `APRENDER_DECIDE_SHA256` pinned to the new sha, auth `false`, IAM `s3:GetObject` on the prefix only.

**Plan's automated verify:** `OFFLINE DEPLOY READINESS OK sha 2293bebb29ac door B 8 P 86`; no 12-digit number, no `Bearer `, no `--bearer` in the evidence.

## What is NOT proven offline

Remote authentication, the cold path, the 30 s gateway behaviour and the live identity are first exercised by plan 10-08. The selftest builds the bootstrap but never a server; the served-task door was exercised through the probe's plan-only path under the compiled constant.

## Deviations from Plan

None to the plan's actions. Orchestrator-directed adjustments, disclosed:

- `just --evaluate` is mangled by the rtk hook; the Laya snapshot dir was read with `rtk proxy` (snapshot `55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`).
- `10-LIVE-DEPLOY-EVIDENCE.json` `door.door_evidence_commit` is `a8734c53c` (plan 10-11's evidence commit); 10-11 was a no-op, so there is no production door commit, and `contract_version` is 9.0.0.
- State verbs refuse (04-VERIFICATION.md `gaps_acknowledged`); STATE.md was hand-edited on its `Last activity:` line only. `roadmap update-plan-progress` was deliberately NOT run: it counts SUMMARY files and would mark this unfinished plan complete.
- This SUMMARY is written at the checkpoint, at the orchestrator's instruction, and is `status: partial`; the continuation after the owner's reply must complete it (decision block, Task 2 commit, `requirements-completed`, roadmap progress).

## Auth Gates

None. Nothing contacted AWS or pmcp.run.

## Known Stubs

None.

## Threat Flags

None. No new network surface; T-10-23 held (every recipe under DRY_RUN=1, AWS CALLS: 0), T-10-24 mitigated (proof re-run, equals the installed tool), T-10-25 held (no account id), T-10-26 and T-10-40 are open until the decision is recorded.

## Self-Check: PASSED

- FOUND: 10-LIVE-DEPLOY-EVIDENCE.json; FOUND: commit 9c5d2170b (ancestor of HEAD); resolver-proof.txt reads 0.28.1; /tmp/p10-07-t1b.log ends `DEPLOY SELFTEST OK`.
