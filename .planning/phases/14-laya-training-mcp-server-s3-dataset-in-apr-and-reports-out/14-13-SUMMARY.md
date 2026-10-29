---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 13
subsystem: infra
tags: [laya, pmcp-run, cargo-pmcp, oauth, iam, mcp, ze-kasher-dev, live]
status: complete

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-10 publish/grant/call recipes and deploy template; 14-12 live stacks, images and staged datasets
provides:
  - aprender-mcp-laya-train live on pmcp.run (ze-kasher-dev), OAuth on, inline grant aprender-laya-train-dev on its role
  - 14-DEPLOY-EVIDENCE.json (laya-trainer-deploy-evidence-v1): the D-10 order observed with timestamps, role inspection before and after the grant, every free live check
  - a deploy template that cargo-pmcp 0.28.1's renderer actually parses
affects: [14-14, 14-15]

plan_head_before: aa6d87925d85c3b48dd849cd31daedec5abc7555
plan_head_after: dff3609ab73a59cb07deb38c42496f6f129cd09c

# commits is MEASURED (git rev-list --count plan_head_before..plan_head_after = 5): four are this plan's, the fifth
# (dff3609ab) is the orchestrator recording the owner's smoke-go reply.
actuals:
  tokens: 3000     # chars/4 over the realized diff (10,043-char evidence file + 27-line template addition)
  tasks: 3
  commits: 5

tech-stack:
  added: []
  patterns:
    - "A live security ordering is observed, not only derived: unauthenticated and authenticated pre-grant calls refused dataset-access, role simulation implicitDeny before and allowed after, 401s seen, then the grant"
    - "Read-only proof of no-mint: task table COUNT, the single-flight lock record and ListExecutions compared across the refusal calls"

key-files:
  created:
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-DEPLOY-EVIDENCE.json
  modified:
    - crates/aprender-mcp-laya-train-lambda/.pmcp/deploy.toml.template

key-decisions:
  - "Deploy template mirrors the SetFit training template's descriptor shape (server sizing, observability, auth.dcr) because the renderer's DeployDescriptor requires those fields; auth stays off and no IAM statements are declared"
  - "exposure_start_utc is the deployment-creation time embedded in the cargo-pmcp deployment id, because the deploy log carries no wall-clock stamps; this is the larger reading of the open window"
  - "The grant was run by the owner's own shell after the executor's attempt was denied by the permission system and not retried; recorded as such in the evidence"

requirements-completed: [D-10, D-01, D-08, D-21, D-06, D-09, D-03, D-07]

coverage:
  - id: D1
    description: "Publish with no access: the right crate is live, serves exactly train, train_status and validate_dataset, and holds no SFN, DynamoDB or bucket right before the grant"
    requirement: "D-10"
    verification:
      - kind: other
        ref: "Task 1 verify -> PUBLISHED https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp unauth rule dataset-access; role simulation implicitDeny x3; table 0/0, executions 0"
        status: pass
    human_judgment: false
  - id: D2
    description: "OAuth observed on (tools/list 401, tools/call 401), window measured, and the authenticated caller still refused dataset-access before the grant"
    requirement: "D-10"
    verification:
      - kind: other
        ref: "Task 2 verify -> OAUTH ON; window 10114 (form-auth mint line)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Grant in place and the whole free surface answers as decided: simulation allowed x3, motivation-v1 valid (188 shots/class, epochs required 4-12, Phase 12 hashes), tweet-stance-64 shift, tweet-stance-16 epochs_fixed 12, task-missing, dataset-uri, four refused trains with table count, lock record and executions unchanged"
    requirement: "D-08"
    verification:
      - kind: other
        ref: "Task 3 verify (all assertions, including the owner's smoke-go reply) -> LIVE SURFACE OK"
        status: pass
    human_judgment: false
---

# Phase 14 Plan 13: Publish on pmcp.run, OAuth, grant and the free live checks Summary

**The Laya training request function is live on pmcp.run behind OAuth, granted only after the 401 was observed, and every free path of its tool surface was exercised live without minting a task or spending anything.**

## Accomplishments

- **Publish with no access (Task 1, tracer):** `aprender-mcp-laya-train` published through the shared crates root; the deploy log names `aprender-mcp-laya-train-lambda`, `crates/.pmcp` and `crates/deploy` restored byte-identical. `/health` 200 naming the server; `tools/list` shows exactly the three tools. An unauthenticated `validate_dataset` was refused `dataset-access` (no posture finding). Table count 0 before and after, ListExecutions empty. The role held inline policies `pmcp-declared` and `PmcpCompositionPolicy` and the managed `AWSLambdaBasicExecutionRole`; `simulate-principal-policy` answered `implicitDeny` for `states:StartExecution`, `dynamodb:PutItem` and `s3:GetObject`.
- **OAuth and the 401s (Task 2, owner checkpoint):** the owner enabled OAuth and created the M2M client. The M2M file was verified outside the repository, mode 600, holding the three keys; the mint went through the form-auth path (`(auth form)`). The first poll already answered `tools/list 401, tools/call 401`. The authenticated `list-tools` showed three tools and the authenticated `validate` on motivation-v1 was still `dataset-access`, so the grant was provably the only thing between a caller and the bucket.
- **Grant and free checks (Task 3):** the owner ran `laya-trainer-grant` in their own shell (`GRANT OK`, policy `aprender-laya-train-dev`, its in-invocation 401 check passed). Read-back showed the five statements, with `s3:ListBucket` under `StringLikeIfExists s3:prefix datasets/*`. The simulation now answers `allowed` x3; the role `MaxSessionDuration` is 3600 s (the answer for 14-08's presigned-link TTL question: a link cannot outlive the role session, and one is 1 h at most from this role).
- **Live validations:** motivation-v1 valid, labels `[social_affiliative, conspiratorial, other]`, counts 188 / 66 / 41, `shots_per_class` 188, `epochs_required` true, `epochs_range` [4, 12], `deferred_rules` empty, `inputs_sha256` equal to Phase 12's; tweet-stance-64 valid with `shift_present` true; tweet-stance-16 valid with `epochs_fixed` 12; absent prefix `task-missing`; another bucket `dataset-uri`.
- **Four refused trains:** unknown `seeds` field (argument deserialization, no `REFUSED` rule), name `Bad` (`name`), epochs 3 on motivation-v1 (`epochs`), no epochs on motivation-v1 (`epochs`, D-21). Task table count, lock record (absent before and after) and ListExecutions identical across the calls.

## Findings

- **Open-window finding.** The function was open (auth off) from publish (exposure start 19:04:44Z, publish done 19:05:52Z) until the first 401 at 21:53:18Z: `auth_window_s` 10114 (about 2 h 49 min). The 1-poll answer means OAuth was already on when the check ran, so the true enable time is not observed and the window is an upper bound. It is long because the owner enabled OAuth hours after the publish, not because anything lingered. The window was harmless by construction, which this plan observed rather than assumed: the role simulation was `implicitDeny` for all three spend-relevant actions and the unauthenticated validate was refused `dataset-access`. The plan's no-watchdog decision held.
- **404-vs-403 observation (first live one).** The absent prefix `datasets/no-such-prefix/` answered `task-missing` (task.json does not exist), not an access denial. With `s3:ListBucket` conditioned `StringLikeIfExists`, a missing key under the granted prefix answers 404 live. This closes the open item 14-12 left (the owner's IAM user had been used there, so the role-conditioned behaviour was never exercised) and the fix in `65e1b754a` is confirmed in the deployed policy.
- **Deploy-template defect, and what 14-10's offline gates could not catch.** The first two publish attempts failed at cargo-pmcp's descriptor parse: `missing field enable_xray in observability`, then `missing field timeout_seconds in server`, each followed by a `cdk synth` fallback that cannot run. Nothing was created either time and the crates root restored byte-identical. The 14-10 template declared only a fragment of the schema pmcp-cfn-renderer's `DeployDescriptor` requires. 14-10's offline selftests, gate rows and `DRY_RUN` chain exercise the recipe's own config render and swap guards but never run the renderer, and `deploy`'s DRY_RUN stops before `cargo pmcp deploy`, so no offline gate could have seen it; only a real `cargo pmcp deploy` parses the file. Fix `e28ecbaf0` mirrors the SetFit training template (`[server] memory_mb`/`timeout_seconds`, `[observability]` with xray/dashboard/alarms, `[auth.dcr]`, `[auth.scopes.custom]`), checked against the descriptor schema in pmcp-package; auth stays off and no IAM is declared. The third attempt rendered, applied `[environment]` and `[server]` sizing (MemorySize 256 -> 1024) and deployed (the bootstrap upload needed three retries on network errors). A gate that parses the generated config with the real renderer before any live run would close this class.
- **Untracked secret-looking file.** `.env.m2m` sits untracked and not gitignored in the repository root. I never read it (the secret-read guard blocked it) and did not stage it. It should be deleted, moved out of the checkout, or gitignored.

## Task Commits

1. **Task 1 (tracer):** `e28ecbaf0` (fix: deploy template descriptor fields), `b416bb223` (evidence: published, no access)
2. **Task 2:** `ba46ba3d0` (OAuth observed, 401s, authenticated pre-grant refusal)
3. **Task 3:** `f5c57a429` (owner-run grant, post-grant simulation, free live checks); `dff3609ab` is the orchestrator's record of the owner's `smoke-go`

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Deploy template missing descriptor fields**
- **Found during:** Task 1
- **Issue:** `.pmcp/deploy.toml.template` did not parse under cargo-pmcp 0.28.1's renderer (two missing required fields in turn).
- **Fix:** added the SetFit template's shape for `[server]` sizing, `[observability]`, `[auth.dcr]`, `[auth.scopes.custom]`; `[auth] enabled = false` and the no-IAM rule are unchanged.
- **Files modified:** `crates/aprender-mcp-laya-train-lambda/.pmcp/deploy.toml.template`
- **Commit:** `e28ecbaf0`

### Other

- **Grant run by the owner.** The executor's `laya-trainer-grant` call was denied by the permission system ([Permission Grant]) and was not retried or worked around; the owner ran the same command in their own shell. The recipe's own in-invocation 401 check still ran, so the ordering guarantee is intact. Recorded in the evidence (`grant_run_by`).
- **`exposure_start_utc`** is derived from the deployment id's embedded creation time rather than a log timestamp (the log has none); stated in `exposure_start_basis`.

## Authentication Gates

- The pmcp.run login for ze-kasher-dev was confirmed by the owner at `deploy-go` and held; no gate mid-deploy.
- OAuth enablement and the M2M client are owner-only pmcp.run admin actions (Task 2 checkpoint).
- The grant was an owner-run step after the permission denial (above).

## Known Stubs

None.

## Threat Flags

None. The surfaces touched (the public endpoint, OAuth, the role grant) are the plan's own `<threat_model>` rows T-14-47, T-14-48, T-14-49, T-14-76 and T-14-77, each mitigated and observed: no access before the grant, grant only after a 401 in the grant's own invocation, the credential file outside the checkout and never printed, role simulation deny then allow, and table count / lock record / executions unchanged across the refusals.

## Next Phase Readiness

The server is live, authenticated and granted; the owner replied `smoke-go` for plan 14-14 (one tweet-stance-64 train with epochs 4, one tweet-stance-16 run cancelled during training and one cancelled during packing, all on ml.g5.xlarge; the third trains to completion before its cancel). 14-14 is a separate dispatch; no training job was started here. Carry forward from 14-12 the open production-batch memory-headroom question (96.7% of the A10G in the probe step).

## Self-Check: PASSED

- Evidence file present; the Task 1, Task 2 and Task 3 verify scripts all pass (Task 3 including the `smoke-go` reply)
- Commits `e28ecbaf0`, `b416bb223`, `ba46ba3d0`, `f5c57a429`, `dff3609ab` are ancestors of HEAD
- No 12-digit account id, JWT, client secret or absolute path in the evidence; no execution started; no `.env.m2m` content read
