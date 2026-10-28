---
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
plan: 07
subsystem: decide-deploy
status: complete
tags: [augury-dev, pmcp-run, s3, d-18-probe, d-19-bucket, pre-deploy, owner-login, eu-west-1]
requires:
  - phase: 13-01
    provides: "per-profile state dir and the selftest restores of models/decide/augury-dev/"
  - phase: 13-04
    provides: "_laya-target-check, _laya-account-check, _laya-state-snapshot, the augury-dev region parameter of laya-deploy-config"
  - phase: 13-06
    provides: "_laya-auth-selftest, run as a case of laya-deploy-selftest"
  - phase: 13-11
    provides: "the ze-kasher-dev archive and MANIFEST.sha256 pinned at b01bc721..."
provides:
  - "13-PROBE-EVIDENCE.json (laya-probe-evidence-v1): D-18 go, bucket present, three-state Lambda observations"
  - "13-DEPLOY-EVIDENCE.json (laya-deploy-evidence-v3): archive re-check, selftest-before-render, eu-west-1 config, post-render restore check, upload, remote sha256, ze-kasher-dev and augury-antisemite baselines"
  - "13-DEPLOY-OWNER-REPLY.json (laya-deploy-owner-reply-v1): the owner's augury-dev pmcp.run login and the token gate line"
  - "s3://aprender-decide-weights-783960389328-dev/decide/aprender-mcp-decide-motivation/d202f495....apr in eu-west-1, downloaded and hashed equal to the pin"
  - "models/decide/augury-dev/deploy-config-aprender-mcp-decide-motivation.toml (gitignored; sha256 e0c92cf0...)"
affects: [13-08, 13-09, 13-10]
actuals:
  tokens: 3931     # chars/4 over the three evidence records the plan changed (this SUMMARY excluded)
  tasks: 2
  commits: 3
commits: 3
plan_head_before: 52e630857312be9127f2a603b73e16b8f490d0ec
plan_head_after: f48fa67c5
tech-stack:
  added: []
  patterns:
    - "selftest BEFORE the real render, then re-run the path-touching selftests after it and require the config sha256 and the state snapshot unchanged"
    - "remote identity by download-and-hash outside the repository, since laya-upload skips a same-size object"
    - "three-state read-only baselines: present / absent only on NotFound / unreadable with its error code"
    - "owner-run read-only reads when the executor's permission classifier refuses a call against another account; the executor records and hashes the owner's outputs"
key-files:
  created:
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-PROBE-EVIDENCE.json
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-DEPLOY-EVIDENCE.json
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-DEPLOY-OWNER-REPLY.json
  modified: []
key-decisions:
  - "The ze-kasher-dev baseline (P10) was taken by the owner, not the executor, after the permission classifier refused the executor's read-only call against account 816069129666; the executor made no ze-kasher-dev AWS call and recorded the owner's outputs with their sha256"
  - "The owner's probe rc file reads 1 but is inverted by a leading `! ` in the pasted line; the evidence records the file as found and the true rc 0 beside it, rather than rewriting the file"
  - "The existing augury bucket was reused (head-bucket rc 0); the laya-weights-bucket fallback did not run"
requirements-completed: [D-18, D-19, D-06, D-02, D-05]
coverage:
  - id: C1
    description: "D-18 read-only probe passes before any augury write"
    requirement: D-18
    verification:
      - kind: integration
        ref: "13-PROBE-EVIDENCE.json probes P1 P2 P3 pass, d18_go true; Task 1 verify PRE-DEPLOY OK d202f4956cd6"
        status: pass
    human_judgment: false
  - id: C2
    description: "the ze-kasher-dev local record is unchanged against the 13-11 manifest before, during and after the plan"
    requirement: D-06
    verification:
      - kind: integration
        ref: "shasum -a 256 -c MANIFEST.sha256 rc 0 (before, after the selftest, after the post-render check, and again at SUMMARY time); manifest sha256 b01bc721... equals the 13-11 pin"
        status: pass
    human_judgment: false
  - id: C3
    description: "the eu-west-1 config pins the sha, auth off and the augury bucket key, and survives every selftest that touches its path"
    requirement: D-05
    verification:
      - kind: integration
        ref: "13-DEPLOY-EVIDENCE.json config and post_render_check; config sha256 e0c92cf0... re-read at SUMMARY time"
        status: pass
    human_judgment: false
  - id: C4
    description: "the artifact is in the augury bucket under its content-addressed key and its remote bytes hash to the pin"
    requirement: D-19
    verification:
      - kind: integration
        ref: "13-DEPLOY-EVIDENCE.json upload rc 0, head_object 846196612 bytes, remote_identity downloaded and hashed d202f495..."
        status: pass
    human_judgment: false
  - id: C5
    description: "the ze-kasher-dev live server answers with the pinned artifact (baseline before the augury publish)"
    requirement: D-02
    verification:
      - kind: manual
        ref: "owner-run identity probe: identity_matches true, artifact_sha256 d202f495..., cold load_ms 13375"
        status: pass
    human_judgment: true
  - id: C6
    description: "the owner is logged in to the augury-dev tenant with at least 45 minutes left"
    requirement: D-18
    verification:
      - kind: manual
        ref: "13-DEPLOY-OWNER-REPLY.json token_gate_line (58 min left); Task 2 verify LOGIN GATE OK 58 min"
        status: pass
    human_judgment: true
duration: "54 min wall clock (2026-10-28T03:50:51Z to 04:45Z), including two owner waits"
completed: 2026-10-28
---

# Phase 13 Plan 07: Pre-deploy preparation for motivation-v1 on augury-dev Summary

**The unchanged motivation-v1 artifact (d202f495...) is in the augury bucket in eu-west-1, downloaded back and hashed equal to the pin. The eu-west-1 config is rendered with auth off and survived every selftest that touches its path. The D-18 probe passed, the ze-kasher-dev deployment is baselined and untouched, and the owner is logged in to the augury-dev tenant with 58 minutes left.**

## Performance

- **Started:** 2026-10-28T03:50:51Z
- **Completed:** 2026-10-28T04:45Z
- **Tasks:** 2 (1 tracer, 1 blocking-human checkpoint)
- **Files changed:** 3 evidence records (plus the gitignored augury config and one S3 object)

## What was proven

### D-18 probe (read-only, live mode, every control-plane override cleared)

- P1 `aws sts get-caller-identity --profile augury-dev`: `783960389328`
- P2 `_laya-target-check augury-dev eu-west-1`: `augury-dev eu-west-1 .eu-west.augury.pmcp.run 783960389328`. `_laya-account-check`: `target-account ok: profile augury-dev is account 783960389328`. `cargo pmcp configure show augury-dev`: aws_profile augury-dev, region eu-west-1.
- P3 counter-coach /health: HTTP 200, status healthy, serverId counter-coach
- Observations: P4, the motivation id on augury, returns 404 not_found (it is new on the tenant). P5, the augury antisemite server, returns 200 with hasDeployment true. Its Lambda is present: 10240 MB, arm64, Active, env sha 2293bebb..., no reserved concurrency. This was observed read-only and is not claimed. P11 showed the pmcp.run token expired at probe time.
- Bucket `aprender-decide-weights-783960389328-dev`: present (head-bucket rc 0), location eu-west-1. It held only the antisemite key before the upload. The fallback was not used.

### D-06 archive re-check

- `shasum -a 256 -c models/decide/ze-kasher-dev-archive-20261027/MANIFEST.sha256` gave rc 0 at four points: before the augury steps, after the offline selftest, after the post-render check, and again at SUMMARY time (2026-10-28T04:44Z).
- The manifest's sha256 is `b01bc7216ddd1e349b079a2bf9aa314a2f8030a0730ed94233d348429a44ad90`, equal to the 13-11 pin in 13-BASELINE-EVIDENCE.json.
- The antisemite id's two eligibility logs were compared with `cmp` against the plan-start backup: byte-equal after every selftest and again at SUMMARY time.

### Offline selftest before the render

- Last line: `DEPLOY SELFTEST OK (positive dry run SKIPPED: disarmed by LAYA_DEPLOY_SELFTEST_POSITIVE=0)`, with AWS CALLS: 0
- Sub-selftests: `REGION: 14 cases, AWS CALLS: 0, REGION SELFTEST OK`, `TARGET: 58 cases, AWS CALLS: 0, TARGET SELFTEST OK`, `AUTH: 36 case lines, AWS CALLS: 0, AUTH SELFTEST OK`
- models/decide/augury-dev/ did not exist before. `_laya-state-snapshot` was unchanged across the run.

### Config and post-render restore check

- `just laya-deploy-config models/decide/laya-motivation-v1.apr off aprender-mcp-decide-motivation dev augury-dev eu-west-1` exited 0. The config:
  - region: eu-west-1
  - auth.enabled: false
  - APRENDER_DECIDE_SHA256: d202f495...
  - S3 URI: `s3://aprender-decide-weights-783960389328-dev/decide/aprender-mcp-decide-motivation/<sha>.apr`
  - IAM resource: `.../decide/aprender-mcp-decide-motivation/*`
  - sha256: C1 = `e0c92cf0765c60391db0e2b5a163ea585fed4cb4583570b1d69c4f33f8fefe6f`
- After the render, `_laya-region-selftest`, `_laya-target-selftest` and `_laya-auth-selftest` ran with the motivation id. Each printed OK with AWS CALLS: 0. Afterwards the config sha256 was still C1 and the snapshot was unchanged. C1 was re-read at SUMMARY time.

### Upload and remote identity (D-19)

- sts right before the upload: 783960389328
- Try 1 (04:08:31Z to 04:17:49Z) exited rc 0 with `AWS_MAX_ATTEMPTS=10 AWS_RETRY_MODE=standard`. It printed: `UPLOADED s3://aprender-decide-weights-783960389328-dev/decide/aprender-mcp-decide-motivation/d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd.apr (846196612 bytes, sha256 d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd)`
- head-object with checksum mode on reported ContentLength 846196612, equal to the file. The checksum type was FULL_OBJECT and no ChecksumSHA256 came back; this is an observation only.
- The object was downloaded outside the repository. Its sha256 is `d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd`, which matches the pin. The temp file was deleted in the same step.

### ze-kasher-dev baseline (owner-run, read-only)

- Identity probe against https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp: identity_matches true, artifact_sha256 d202f495..., load header `cold;load_ms=13375`
- Lambda (account 816069129666, us-east-1): present, CodeSha256 `9KJKCAWk...`, 10240 MB, arm64, Active, env sha d202f495..., no reserved concurrency

### Owner login (Task 2)

- The owner ran `PMCP_TARGET=augury-dev cargo pmcp deploy login` in their own terminal and replied `logged-in`.
- Token gate (reads `pmcp-run.expires_at` only): `pmcp.run token expires_at 2026-10-28T05:41:33.765569+00:00 (58 min left)`, rc 0. Verify: `LOGIN GATE OK 58 min`.
- The deploy (plan 13-08) was **not** started. The session ends at 05:41Z, so 13-08 must start promptly.

## Task Commits

1. **Task 1 (tracer): the artifact reaches the augury bucket through every gate.** Two commits:
   - `862bd8bd9`: partial evidence, with the ze-kasher-dev baseline still pending the owner
   - `ea740cb86`: the owner-run baseline recorded and F-13-07-1 resolved. Verify printed `PRE-DEPLOY OK d202f4956cd6`.
2. **Task 2 (checkpoint, blocking-human): the owner's pmcp.run login.** Commit `f48fa67c5`.

## Deviations from Plan

### Findings and environment workarounds

**1. F-13-07-1 (executor permission). The ze-kasher-dev baseline was refused by the classifier and resolved by owner-run reads.**
- **Found during:** Task 1 step 2 (P10)
- **Issue:** The Claude Code permission classifier refused the executor's read-only `aws lambda get-function-configuration` against the ze-kasher-dev account. Because of that, the identity probe was not attempted.
- **Fix:** The executor stopped at a checkpoint. The owner ran the three read-only reads (identity probe, get-function-configuration, get-function-concurrency) in their own terminal. The executor recorded the outputs and their sha256 in 13-DEPLOY-EVIDENCE.json `ze_kasher_before.owner_outputs_sha256`. The executor made no ze-kasher-dev AWS call.
- **Commit:** ea740cb86

**2. Inverted probe rc in the owner's output.**
- **Issue:** The owner pasted the probe line with a leading `! ` (shell negation). As a result, `p13-07-zk-probe.rc` reads 1 even though the probe exited 0 (identity_matches true, sha equal to the pin).
- **Handling:** The file is recorded as found, with `recorded_rc 1`, `true_rc 0` and an explanatory note. It was not rewritten.
- **Commit:** ea740cb86

**3. Log and temp paths: the session scratchpad instead of /tmp.**
- **Issue:** The plan names `/tmp/p13-07-*.log` and `/tmp/p13-07-remote.apr`. Under the executor's environment rule these were written to the session scratchpad (`/private/tmp/claude-501/.../scratchpad/p13-07-*`), which is also outside the repository.
- **Evidence handling:** The evidence records the path as `<executor scratch dir>` so that no machine-local path is committed. The owner's own outputs stayed in their /tmp. Verbatim copies are in the scratchpad's `p13-07-zk-owner/`.
- **Verify:** The `/tmp/p13-07-remote.apr` absence check passes trivially, and the scratchpad temp copy was deleted too.

**4. rtk hook quirk with `wc -c`.**
- **Issue:** Byte counts piped through the Bash tool's rtk hook are unreliable. At SUMMARY time, `git diff ... | wc -c` printed 9938 for a diff whose three files alone total 15724 characters (read with Python), because the hook filters the diff output.
- **Handling:** Every byte count in the evidence (file_bytes 846196612, the archive bytes) and in `actuals.tokens` was taken with Python (`os.path.getsize` / `len(read())`), never with `wc -c`.

**5. STATE.md verbs skipped.**
- `.planning/STATE.md` carries the owner's uncommitted edit. The dispatch forbids touching it, so the STATE verbs (`state.advance-plan`, `update-progress`, `record-metric`, `add-decision`, `record-session`) were not run, as in 13-06. Only `roadmap.update-plan-progress` runs.
- The decisions above are left for the orchestrator to record.

**6. Partner repository HEAD differs from the plan's expectation.**
- `git -C /Users/guy/projects/mcp/motivation-experiment rev-parse HEAD` read `c632c749b02011b964c28f1a3a65f943033ff4d7`, not the plan's d822f119.... It is recorded as read, per the plan.

No code was changed. The plan touched only evidence records, one gitignored config and one S3 object.

## Auth gates

- Task 2 was the planned pmcp.run login, done by the owner in their own terminal. The token gate passed with 58 minutes left.

## Known Stubs

None.

## Threat Flags

None. No new surface beyond the plan's threat model:
- one S3 PutObject in the augury account, under the content-addressed key
- read-only reads everywhere else

## Next

Plan 13-08 publishes against the augury-dev tenant inside this session, which expires at 05:41:33Z. The owner has been told to:
- keep the pmcp.run admin UI of augury-dev open
- be ready to switch on OAuth as soon as the deploy recipe returns
- expect the containment watchdog (reserved concurrency 0) if unauthenticated tools/list still answers 1800 s after the deploy begins

## Self-Check: PASSED

- FOUND: 13-PROBE-EVIDENCE.json, 13-DEPLOY-EVIDENCE.json, 13-DEPLOY-OWNER-REPLY.json, models/decide/augury-dev/deploy-config-aprender-mcp-decide-motivation.toml
- FOUND commits: 862bd8bd9, ea740cb86, f48fa67c5 (ancestors of HEAD)
- Task 1 verify `PRE-DEPLOY OK d202f4956cd6` (at ea740cb86); Task 2 verify `LOGIN GATE OK 58 min`
- At SUMMARY time:
  - manifest rc 0 and sha256 b01bc721... equal to the 13-11 pin
  - antisemite eligibility logs cmp-equal to the plan-start backup
  - artifact sha256 d202f495...
  - config sha256 e0c92cf0...
  - justfile clean
