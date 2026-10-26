---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 12
subsystem: deploy
tags: [pmcp.run, lambda, laya, motivation-v1, identity-probe, pilot]
requires:
  - phase: 12-04
    provides: laya-deploy with the edge-health 404 retry and the id-resolver proof
  - phase: 12-11
    provides: the deploy-eligible motivation-v1 artifact (gate pass, no waiver)
provides:
  - live pmcp.run server aprender-mcp-decide-motivation (pilot), identity-proven
  - 12-DEPLOY-EVIDENCE.json (schema laya-deploy-evidence-v2) for plan 12-13
affects: [12-13]
tech-stack:
  added: []
  patterns: [upload/deploy split around the owner's pmcp.run login, token gate re-run before each live attempt, read-only before/after proof for a neighbouring live server]
key-files:
  created:
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-DEPLOY-EVIDENCE.json
  modified: []
key-decisions:
  - "Deployed under the owner's authorization for aprender-mcp-decide-motivation only (ze-kasher-dev, dev, us-east-1); nothing else was touched"
  - "D-01: labelled a pilot (gate passed on the first declared run; waived is null)"
patterns-established:
  - "The classifier did not refuse the executor's laya-deploy this time (Phase 10 expected it to); the owner-run branch was not needed"
requirements-completed: [D-23, D-01, D-03, D-12, D-02]
status: complete
commits: 3
plan_head_before: 59bae98c363afa34e73e646ebdc0707e82031ad9
plan_head_after: 90a03da532fca82d0958ad389d65588d51309575
actuals:
  tokens: 20000
  tasks: 3
  commits: 3
duration: ~25 min of live work across two agents (deploy 4 min 36 s)
completed: 2026-10-26
---

# Phase 12 Plan 12: motivation-v1 live on pmcp.run Summary

**aprender-mcp-decide-motivation is live (10,240 MB arm64 Lambda, auth off, us-east-1) serving the motivation-v1 pilot artifact sha d202f495...aecd, identity-proven, with the antisemitism server shown unchanged.**

## What was done

- **Task 1 (adc32ded7, earlier agent):** D-23 equivalence (79 recipes identical to HEAD, generated config equal to HEAD's), offline selftest (AWS CALLS: 0), antisemite read-only baseline, laya-upload of the artifact to `decide/aprender-mcp-decide-motivation/<sha>.apr`.
- **Task 2 (99da60a31):** the owner ran `cargo pmcp login` and replied `logged-in`. Token gate (expires_at only): `pmcp.run token expires_at 2026-10-26T23:08:32.571574+00:00 (59 min left)`.
- **Task 3 (90a03da53):** token recheck immediately before the deploy: `... (58 min left)`. The executor ran `just laya-deploy` (lockf, positional args only), 22:10:00Z to 22:14:36Z, rc 0.

## Results

- **DEPLOYED line:** `DEPLOYED aprender-mcp-decide-motivation at https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp: compile log, edge health serverId and live identity (sha256 d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd) name the decide server`
- **Health retries:** the edge `/health` returned 404 once (attempt 1/40), then `edge-health-get ok: HTTP 200 after 2 attempt(s) in 16 s`. Phase 10's brand-new id took about 8 minutes; this one took 16 s. No sticky 503.
- **Identity:** probe with `--expect-sha256` returned identity_matches true, all four checks true, artifact_sha256 d202f495...aecd, 0 retries (warm, 342/186/1069 ms).
- **Negative control:** the antisemitism-v1 artifact is refused by the new endpoint (probe exit 1, identity_matches false, description labels not in order).
- **Readback:** function `aprender-mcp-decide-motivation`, 10240 MB, arm64, Active/Successful, us-east-1, APRENDER_DECIDE_SHA256 equals the pin, S3 URI ends with `/decide/aprender-mcp-decide-motivation/<sha>.apr` (URI not recorded).
- **Antisemite unchanged:** identity probe passes (sha 2293bebb...6c776e); all seven keys equal the baseline (LastModified 2026-10-23T01:13:24, CodeSha256 lhqq6c5Z...Er0=, 10240 MB, Active, arm64, env sha, reserved concurrency null). Only read-only verbs were used against it.
- **Served path:** classify only; softmax(z / T); no prior correction (D-03, D-12). Pilot label: `pilot` (D-01).

## Findings

- **F-12-12-1:** laya-upload's `aws s3 cp` lost its connection mid-multipart twice (parts 19 and 1) on this host. Try 3 with `AWS_MAX_ATTEMPTS=10 AWS_RETRY_MODE=standard` in the environment succeeded (310 s). Recipe unchanged; the upload is content-addressed and idempotent. A recipe-level default is a candidate follow-up, not done here.
- **F-12-12-2:** the new id's first `/health` 404 resolved on the second attempt in 16 s, as the 404 retry from plan 12-04 intends.

## Deviations from Plan

None - plan executed as written. The owner-run branch was not needed: the permission classifier did not refuse the executor's launch.

## Issues Encountered

- A first draft of the evidence writer failed on the empty `get-function-concurrency` response (an empty body means no reservation, the same shape as the baseline). It failed before writing anything; the parse was fixed and re-run. No evidence file was left half-written.

## Not done by this plan (orchestrator-owned)

STATE.md, ROADMAP.md and REQUIREMENTS.md were not updated: `gsd_run` state/roadmap tracking errors on `04-VERIFICATION.md` (`gaps_acknowledged` is outside the tool's closed set) and that file was deliberately not edited. The orchestrator should record plan 12-12 as complete.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-12-42 (open endpoint, auth off) is the owner's accepted posture; containment is `just laya-teardown aprender-mcp-decide-motivation dev ze-kasher-dev`.

## Self-Check: PASSED

- 12-DEPLOY-EVIDENCE.json present; Task 1, Task 2 and Task 3 verify blocks pass.
- Commits adc32ded7, 99da60a31, 90a03da53 are ancestors of HEAD.
- Evidence contains no 12-digit account id, bucket name or bearer token.
