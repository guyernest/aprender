---
phase: 10-antisemitism-v1-decide-server-for-experiment-coach
plan: 08
subsystem: infra
tags: [laya, deploy, pmcp-run, lambda, identity-probe, cold-start, live]

requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: "plan 10-04: the deploy-eligible artifact (sha256 2293bebb, GATE PASS); plan 10-07: resolver proof 0.28.1, offline dry runs and the owner's D-13 decision (replace-auth-off); plan 10-11: the door at 8"
provides:
  - "the live function aprender-mcp-decide serving laya-antisemitism-v1.apr (sha256 2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e) at 10,240 MB, arm64, auth off"
  - "10-LIVE-DEPLOY-EVIDENCE.json: live_attempts[], live_function_after (with a Lambda configuration readback), live{} with outcome verified and final true"
  - "the negative control: the stance artifact (24a44d7e...) is no longer served"
affects: [10-09, 10-10]

status: complete
actuals:
  tokens: 14000
  tasks: 3
  commits: 4
plan_head_before: 82f7acdc8e136ab02c55ce40b11c249bab090982
plan_head_after: 55b4f4a01c84ac080be62fed23bbb5698fe35e7b
commits_note: "rev-list count 4 = this plan's three evidence commits (ee6c5f457, 76a371c81, 55b4f4a01) plus c948591e2, the orchestrator's 10-06 deferral commit that landed in the same range. Task 3 was skipped (verified outcome)."

key-files:
  created:
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-08-SUMMARY.md
  modified:
    - .planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-LIVE-DEPLOY-EVIDENCE.json

key-decisions:
  - "Decision applied: D-13 replace-auth-off (owner, 2026-10-22). The stance demo on aprender-mcp-decide was replaced in place; no bearer exists, so APRENDER_DECIDE_PROBE_TOKEN was never set or read."
  - "laya-deploy was held back once (14:45Z) because 32 min of pmcp.run token could not cover its re-verify, build and deploy; it ran after the owner's fresh login with 36 min of token left when `cargo pmcp deploy` began."

requirements-completed: [SC-5, D-12, D-13, D-15, D-16]
requirements-note: "SC-5 is satisfied only for its live half. The docs, contract constants and claims ledger that still describe the stance deployment are plan 10-10's."

coverage:
  - id: D1
    description: "The verified artifact went through laya-deploy-config, laya-upload and laya-deploy (all rc 0, no refusal) and the live endpoint returns its sha256 with all four identity checks true"
    requirement: "SC-5"
    verification:
      - kind: other
        ref: "probe --url https://aprender-mcp-decide.us-east.true-mcp.com/mcp --apr models/decide/laya-antisemitism-v1.apr --expect-sha256 2293bebb... : rc 0, one_tool_named_classify, description_has_labels_in_order, response_labels_match, identity_matches all true, zero retries"
        status: pass
    human_judgment: false
  - id: D2
    description: "The stance demo is no longer served: the probe against laya-stance-64.apr exits 1 with identity_matches false; the Lambda readback shows 10240 MB, arm64, Active, with APRENDER_DECIDE_SHA256 equal to the artifact"
    requirement: "SC-5"
    verification:
      - kind: other
        ref: "negative control exit 1, identity_matches false; aws lambda get-function-configuration readback"
        status: pass
    human_judgment: false
  - id: D3
    description: "Cold and warm timings from laya-deploy-verify recorded as reported numbers in live{} with proven_cold derived from each sample's CloudWatch line; recipe verdict DEPLOY VERIFY OK"
    requirement: "D-16"
    verification:
      - kind: other
        ref: "the plan's Task 2 python assertion block against 10-LIVE-DEPLOY-EVIDENCE.json: LIVE VERIFY RECORD OK verified True 4 25267"
        status: pass
    human_judgment: false
---

# Phase 10 Plan 08: Live deploy of laya-antisemitism-v1 under aprender-mcp-decide Summary

**`aprender-mcp-decide` now serves the antisemitism-v1 artifact (sha256 `2293bebb...c776e`) at 10,240 MB with auth off; the live identity probe passes all four checks, the stance artifact is refused, and `laya-deploy-verify` ended `DEPLOY VERIFY OK` with four proven-cold samples at 22.8-25.3 s against the 30 s gateway cap.**

## Decision applied and AWS writes

Owner decision `replace-auth-off` (2026-10-22). Access was re-established by read-only checks before any write: `aws sts get-caller-identity --profile ze-kasher-dev` rc 0, and the pmcp.run `expires_at` key read (no token value). The executor never refreshed or rewrote `~/.pmcp`.

AWS write count:
1. One S3 PutObject: `decide/aprender-mcp-decide/2293bebb...c776e.apr` (846,220,868 bytes), content-addressed.
2. One `cargo pmcp deploy` to pmcp.run (function and stack update). `laya-deploy-verify` additionally bumps the function's cold-start environment between samples and reads CloudWatch; that is the recipe's own mechanism, not a separate write step.

Containment (`laya-teardown`) was never run: nothing failed.

## Live steps

| Step | Result |
|------|--------|
| `laya-deploy-config ... off` | rc 0; sha pin = artifact sha; IAM statement is `s3:GetObject` on `decide/aprender-mcp-decide/*` only |
| `laya-upload` | rc 0; 14:20:46Z to 14:45:19Z (1473 s: laya-verify then the transfer); `UPLOADED ... (846220868 bytes, sha256 2293bebb...)` |
| hold | at 14:45:33Z the token had 32 min left; `laya-deploy` was not started (owner's credential rule); owner re-logged in (expiry 18:01:22Z) |
| `laya-deploy` | rc 0; 17:03:31Z to 17:27:57Z; re-verify accepted the file; `DEPLOYING` at about 17:25Z with 36 min of token; grant ok; `edge-health ok: serverId aprender-mcp-decide`; `DEPLOYED ... (sha256 2293bebb...)` |
| endpoint | read from `[deployment] endpoint` of the state file: `https://aprender-mcp-decide.us-east.true-mcp.com/mcp` (equals the expected URL) |
| identity probe (this plan) | rc 0 on the first try, no retries |
| negative control | stance artifact: exit 1, `identity_matches` false |
| `laya-deploy-verify ... 2` | rc 0; 17:30:12Z to 17:33:26Z; `DEPLOY VERIFY OK aprender-mcp-decide: 4 cold samples (both shapes), every one < 30000 ms with CloudWatch cold evidence` |

Identity line (live, this plan):

```
{"probe":"identity","one_tool_named_classify":true,"description_has_labels_in_order":true,"response_labels_match":true,"identity_matches":true,"artifact_sha256":"2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e","elapsed_ms":[396,136,929],"tokens_total":94,"load_header":"warm"}
```

Negative control (stance artifact against the live endpoint): `description_has_labels_in_order` false, `response_labels_match` false, `identity_matches` false, exit 1 (the server answered with the new artifact's sha, so the stance file does not match).

Configuration readback (read-only `aws lambda get-function-configuration`): MemorySize 10240, State Active, LastUpdateStatus Successful, architecture arm64, `APRENDER_DECIDE_SHA256` equal to the artifact sha.

## Timings (reported, not judged; D-16)

Plan-only shapes (both from the recipe's plan files): concentrated texts 2, distributed texts 8, `min_row_tokens` 86 (= P), `max_texts` 8, `tokens_planned` 800 as a request total (never 8 x P = 688).

| # | Shape | elapsed_ms | server load_ms | download / sha / build ms | proven_cold (derived) |
|---|-------|-----------:|---------------:|---------------------------|----------------------|
| 1 | concentrated | 24639 | 15325 | 9023 / 3487 / 2729 | true |
| 2 | distributed | 22765 | 15312 | 8934 / 3517 / 2775 | true |
| 3 | concentrated | 25267 | 15406 | 9041 / 3484 / 2785 | true |
| 4 | distributed | 22794 | 15465 | 8978 / 3484 / 2846 | true |

`proven_cold` was derived for each sample as `cold_evidence == "cloudwatch"` and `performed_load=true` and its own `probe_id=` on the CloudWatch line (all four hold). Cold wall time min 22765 ms, max 25267 ms (concentrated sample 3); worst slack to the 30,000 ms cap 4733 ms. Warm calls (5): 1149, 1095, 1080, 1085, 1180 ms; min 1080, p50 1095, max 1180. Peak RSS on load about 2.45 GB, graviton2. All four were measured on a lightly loaded host; they remain upper-bound-style reported numbers, not guarantees.

`live.outcome` is `verified`, `live.final` true, `function_state` serving, `failed_checks` []. Task 3 (the owner decision after a failed verify) was not needed and is recorded as skipped. That releases the precondition plans 10-10 and 10-09 read.

## Deviations from Plan

None of Rules 1-4 fired; the plan ran as written. Process notes (disclosed, not code changes):

1. **Task 1 ran across two executor runs.** Steps 1-2 completed at 14:45Z; step 3 was held for a fresh pmcp.run login under the owner's credential rule, then run to completion. Evidence key `pre_deploy_steps` (an addition to the plan's schema) records the config, the upload and the hold.
2. **The tracer's `<verify>` was executed as its components, not as the single literal line.** I ran the identity probe and negative control with the same binary and flags, then the plan's Python assertion block unchanged; the 8-retry loop was not needed. The same applies to the Task 2 assertion block (run unchanged against the evidence).
3. **rtk rewrote `aws` output** into a type schema; the Lambda readback used `rtk proxy aws ...` so the real JSON was read.
4. The evidence file was re-serialised with `json.dump(indent=2)` on each edit, which expanded the compact single-line arrays of plan 10-07's record. Content is unchanged.
5. **STATE.md was hand-edited, Last activity line only** (appended one sentence at the end, leaving plan 10-06's deferral sentence from c948591e2 intact); the state verbs refuse on `04-VERIFICATION.md` status `gaps_acknowledged`, which was not re-keyed.

## Auth Gates

The owner's pmcp.run token (expiring 15:17Z) could not cover `laya-deploy`; the executor stopped before the deploy and the owner re-logged in. Normal flow, not a failure.

## Known Stubs

None. The plan creates no code, only live state and evidence.

## Threat Flags

None new. The live endpoint is open (auth off, the owner's D-13 choice), which the plan's threat model already carries (T-10-29/T-10-38): every cold call buys about 25 s of 10 GB compute.

## Secret hygiene

10-LIVE-DEPLOY-EVIDENCE.json was checked after every edit: no 12-digit number, no `Bearer `, no `--bearer`, no `DECIDE_BEARER`. The bucket name (which embeds the account id) stays only in the gitignored `deploy.toml`. No tweet text was printed or committed.

## Next

Plan 10-10 (rewrite the contract constants, README, CLAUDE.md and claims ledger that still describe the stance deployment) and 10-09 can proceed: `live.outcome` is `verified`, `final` true. The served-task `min_row_tokens` in the committed contract still reads 57 until 10-10.

## Self-Check: PASSED

- Evidence file exists and the plan's two assertion blocks pass (LIVE IDENTITY OK 2293bebb29ac; LIVE VERIFY RECORD OK verified True 4 25267).
- Commits ee6c5f457, 76a371c81 and 55b4f4a01 are ancestors of HEAD.
