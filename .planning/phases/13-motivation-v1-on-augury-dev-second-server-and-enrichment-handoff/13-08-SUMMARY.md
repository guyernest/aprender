---
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
plan: 08
subsystem: decide-deploy
status: complete
tags: [augury-dev, pmcp-run, oauth, cognito, m2m, client-credentials, watchdog, eu-west-1]
requires:
  - phase: 13-06
    provides: "laya-probe-auth, m2m_token.py (form-body minting, --token-host precedence), _laya-auth-refusal-check, the LAYA_M2M_FILE re-exec"
  - phase: 13-07
    provides: "the uploaded artifact, the rendered eu-west-1 config, the owner's augury-dev login and token gate"
  - phase: 13-11
    provides: "the ze-kasher-dev archive MANIFEST.sha256 pin"
provides:
  - "live pmcp.run server aprender-mcp-decide-motivation on augury-dev (https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp), Lambda 10240 MB arm64 in eu-west-1 / 783960389328, OAuth on"
  - "13-DEPLOY-EVIDENCE.json: live_attempts, auth (exposure start, deadline, watchdog, refusal, auth window, m2m_attempts, authenticated_identity, negative_control), live_function_after, isolation_after_deploy, findings F-13-08-2 (resolved) and F-13-08-3"
  - "13-OAUTH-OWNER-REPLY.json (laya-oauth-owner-reply-v1): oauth-enable and two m2m-ready steps, path redacted"
  - "gitignored notes models/decide/augury-dev/m2m-file-path and models/decide/augury-dev/m2m-token-host (counter-coach.eu-west.augury.pmcp.run), both mode 600, for 13-09"
affects: [13-09, 13-10]
actuals:
  tokens: 4112     # chars/4 over the added lines of the two evidence records the plan changed (this SUMMARY excluded)
  tasks: 3
  commits: 4
commits: 4
plan_head_before: c2dbe59c1b80c37c4e8ef25f270a4d76be103309
plan_head_after: 4477f1a337242bef680fa17acb68542f70c312c5
tech-stack:
  added: []
  patterns:
    - "conservative exposure start: a line-timestamping wrapper around the deploy recipe; the window runs from the DEPLOYING line, not the recipe's return"
    - "a detached watchdog bounds an open endpoint (contain at deadline, fail-closed) and is stopped by a stop file after the refusal is observed"
    - "an owner-named token host is persisted in a gitignored, non-secret note that every later laya-probe-auth / re-exec reads"
key-files:
  created:
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-OAUTH-OWNER-REPLY.json
  modified:
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-DEPLOY-EVIDENCE.json
key-decisions:
  - "The M2M token is minted on the shared counter-coach Cognito pool host (counter-coach.eu-west.augury.pmcp.run), not on the per-server host the motivation discovery documents advertise; the owner named it, both hosts' openid-configuration name the same Cognito issuer, and the motivation server accepts the token. Carried to the 13-10 partner hand-off"
  - "Local TLS trust: the python.org python3 that m2m_token.py runs under ships no CA bundle, so authenticated runs on this box use SSL_CERT_FILE=/etc/ssl/cert.pem (the macOS system trust store; verification stays on). Not fixed in the helper in this plan (F-13-08-3)"
requirements-completed: [D-07, D-06, D-18, D-02, D-08]
coverage:
  - id: C1
    description: "motivation-v1 published on augury-dev under the reused id, endpoint and target receipt checked, recipe identity chain matched the pin"
    verification:
      - kind: integration
        ref: "13-08-PLAN.md Task 1 verify (run at Task 1, commit 5cd881ddf)"
        status: pass
    human_judgment: false
  - id: C2
    description: "OAuth enabled by the owner; anonymous tools/list and tools/call 401; window bounded and recorded; watchdog stopped; readback and isolation after OAuth"
    verification:
      - kind: integration
        ref: "13-08-PLAN.md Task 2 verify (commit 95d72685b)"
        status: pass
    human_judgment: false
  - id: C3
    description: "authenticated identity with form-body M2M credentials returns the pin; authenticated negative control refused on identity"
    verification:
      - kind: integration
        ref: "13-08-PLAN.md Task 3 verify (commit 4477f1a33)"
        status: pass
    human_judgment: false
duration: about 40 min of live work across three executor runs (launch 04:48Z to the negative control 05:25Z), plus owner wait time
completed: 2026-10-28
---

# Phase 13 Plan 08: motivation-v1 on augury-dev, OAuth on, M2M proven Summary

**motivation-v1 (sha256 d202f495...) is live on augury-dev as `aprender-mcp-decide-motivation`, open for at most 681 s (an upper bound) before the owner's OAuth enablement made anonymous tools/list and tools/call return 401. A form-body M2M token minted on the shared counter-coach Cognito pool returns the pinned identity, and the antisemitism-artifact negative control is refused on identity.**

## Performance

- **Live work:** 2026-10-28T04:48:10Z (deploy launch) to 2026-10-28T05:25:36Z (negative control), across three executor runs with two owner checkpoints in between
- **Tasks:** 3/3
- **Files modified:** 2 evidence records (plus 2 gitignored local notes)

## Accomplishments

- **Publish (Task 1, tracer):** `DEPLOYED aprender-mcp-decide-motivation at https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp: compile log, edge health serverId and live identity (sha256 d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd) name the decide server`. The TARGET RECEIPT names augury-dev and `_laya-endpoint-check` passes for augury-dev. `edge-health-get ok: HTTP 200 after 2 attempt(s) in 17 s`, so health_404_retries = 1 (F-10-12-1). Token recheck: 53 min left.
- **Exposure bound:** exposure_start_utc 2026-10-28T04:49:48Z (basis: the DEPLOYING line), publish_done_utc 04:53:28Z, max_open_s 1800, deadline_utc 05:19:48Z. Watchdog pid 31470 started at 04:53:43Z (`WATCHDOG STARTED pid 31470 deadline 2026-10-28T05:19:48Z`). Outcome: stopped after the refusal (`WATCHDOG STOPPED by the stop file at 2026-10-28T05:02:13Z`). It never contained the function.
- **Refusal (Task 2):** `auth-refusal ok: tools/list 401, tools/call 401 after 1 poll(s) (3 s); first_401_utc 2026-10-28T05:01:09Z; initialize 200 (observed, not asserted); oauth-protected-resource 200 (observed)`. auth_window_s = 681 is an upper bound measured from the DEPLOYING line; the figure from publish_done is 461 s. The owner enabled OAuth shortly after 04:55Z, before the first poll, so the true open interval was shorter. Discovery: `resource` equals the endpoint.
- **Readback after OAuth:** eu-west-1 / 783960389328 / 10240 MB / arm64 / Active / Successful / env sha = pin / S3 key under the server prefix / no reserved concurrency.
- **Authenticated identity (Task 3):** `m2m-token ok: host counter-coach.eu-west.augury.pmcp.run HTTP 200 expires_in 900 s (auth form)`, then identity `{"one_tool_named_classify":true,"description_has_labels_in_order":true,"response_labels_match":true,"identity_matches":true,"artifact_sha256":"d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd", ... "load_header":"cold;load_ms=13769"}`. Expected labels from task.json: social_affiliative, conspiratorial, other.
- **Negative control:** with laya-antisemitism-v1.apr, the token minted (same form-body ok line) and the server answered; the probe exited 1 with identity_matches false and labels false. It was refused on identity, not on a 401.
- **Isolation:** the ze-kasher-dev archive MANIFEST.sha256 verifies and equals the 13-11 pin; models/decide/swap-backup/ is empty; the antisemite eligibility log is byte-equal (cmp) to the scratchpad elig-backup copy; no command addressed ze-kasher-dev (D-02).

## Task Commits

1. **Task 1 (tracer): publish on augury-dev, watchdog armed:** `5cd881ddf`
2. **Task 2: OAuth on, anonymous refusal, watchdog stopped, readback + isolation:** `95d72685b`
3. **Task 3 attempt 1 (finding only): M2M token host mismatch, refused locally (F-13-08-2):** `1d3477583`
4. **Task 3: authenticated identity via the shared counter-coach pool, negative control:** `4477f1a33`

## Files Created/Modified

- `.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-DEPLOY-EVIDENCE.json`: live_attempts, auth, live_function_after, isolation_after_deploy, findings
- `.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-OAUTH-OWNER-REPLY.json`: the owner's replies, verbatim, with the path redacted
- `models/decide/augury-dev/m2m-file-path`, `models/decide/augury-dev/m2m-token-host` (gitignored, mode 600, not committed)

## Decisions Made

See key-decisions in the frontmatter: the shared-pool token host, and the local TLS trust store variable.

## Deviations from Plan

### Owner-directed

**1. The token host is the shared counter-coach pool's, not the per-server host (F-13-08-2)**
- **Found during:** Task 3, attempt 1
- **Issue:** the owner's M2M file names `token_url` on counter-coach.eu-west.augury.pmcp.run, but the motivation server's discovery advertises https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/oauth2/token. laya-probe-auth refused it locally (`REFUSED m2m-host`) before sending any request.
- **Owner reply (verbatim):** "Some of the The servers on the augury-dev are sharing the same OAuth Cognito user pool. The counter-coach is the name of the user pool. Can't you use that token URL?" Orchestrator interpretation, recorded as such: option 2, token host counter-coach.eu-west.augury.pmcp.run, same M2M file.
- **Fix:** wrote the host to the gitignored note `models/decide/augury-dev/m2m-token-host`, which is the route 13-06 designed (`--token-host` precedence). Both hosts' openid-configuration name the same Cognito issuer and list client_secret_post.
- **For 13-10:** the partner hand-off must say that the token URL is the shared counter-coach pool host. The per-server host that discovery advertises is not the one used, and the M2M client the owner put into MotivationAuthSecretArn already names counter-coach.

### Auto-fixed Issues

**2. [Rule 3 - Blocking] The local python3 has no CA bundle (F-13-08-3)**
- **Found during:** Task 3, attempt 2
- **Issue:** `REFUSED m2m-http: transport failure reaching the token endpoint (URLError)`. The cause is `/usr/local/bin/python3` (python.org 3.14.6): `ssl.get_default_verify_paths().cafile` is None, so every https request fails with CERTIFICATE_VERIFY_FAILED. A plain GET reproduces it on both the counter-coach host and the motivation host, while curl reaches both. The TLS handshake failed, so no credentials were sent. 13-06 had tested the helper on loopback http only.
- **Fix:** the authenticated runs used `SSL_CERT_FILE=/etc/ssl/cert.pem`, the macOS system trust store. Verification stays on, nothing was installed and no code changed.
- **Not fixed:** the helper does not fall back to the system bundle. Plan 13-09's re-exec and any later laya-probe-auth on this box need the same variable, or a helper fix.
- **Commit:** 4477f1a33

## Issues Encountered

- STATE/ROADMAP tooling: see the state-update notes in the orchestrator report (the known Phase 4 `gaps_acknowledged` defect may block the verbs; the owner's uncommitted STATE.md is not touched).

## Known Stubs

None.

## Threat Flags

None beyond the plan's threat model. The shared-pool token host widens which clients can call this server to any client of the counter-coach pool that has the server's scopes. That is the owner's stated design (RESEARCH A1) and is recorded for 13-10.

## User Setup Required

None further for this plan. The owner keeps the M2M JSON and puts it into the partner's MotivationAuthSecretArn out of band (D-08).

## Next Phase Readiness

- 13-09 can reuse `models/decide/augury-dev/m2m-file-path` and `m2m-token-host`. It must set `SSL_CERT_FILE=/etc/ssl/cert.pem` (F-13-08-3), and any re-run of laya-deploy must have LAYA_M2M_FILE set (RESEARCH Pitfall 4).
- After any redeploy, re-run `_laya-auth-refusal-check` (RESEARCH A3).

## Self-Check: PASSED

- FOUND: 13-DEPLOY-EVIDENCE.json, 13-OAUTH-OWNER-REPLY.json, models/decide/augury-dev/m2m-token-host (gitignored), models/decide/augury-dev/m2m-file-path (gitignored)
- FOUND commits: 5cd881ddf, 95d72685b, 1d3477583, 4477f1a33 (all ancestors of HEAD)
- Task 3 verify block: `AUTHENTICATED IDENTITY OK counter-coach.eu-west.augury.pmcp.run form`
