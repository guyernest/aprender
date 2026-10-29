---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 10
subsystem: infra
tags: [pmcp-run, cargo-pmcp, iam, oauth, mcp-client, gate-row, python-stdlib, laya]

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-09's laya_trainer_ops.py (Runner seam, state files, selftest scaffolding, outputs-<env>.json with RequestLambdaPolicy); 14-04's request-function crate aprender-mcp-laya-train-lambda; 13-06's _laya-auth-refusal-check and m2m_token.py
provides:
  - crates/aprender-mcp-laya-train-lambda/.pmcp/deploy.toml.template (+ .gitignore): server aprender-mcp-laya-train, auth off, three placeholders, NO IAM
  - laya_trainer_ops.py subcommands config, deploy, grant (57 selftest cases in all)
  - scripts/laya_deploy/train_call.py (authenticated MCP client, 19 selftest cases, TRAIN CALL SELFTEST OK)
  - recipes laya-trainer-config, laya-trainer-deploy, laya-trainer-grant, laya-trainer-call; _laya-auth-refusal-check url wait_s [tool]
  - gate rows trainer-config, trainer-deploy, trainer-grant, trainer-call; auth-refusal-check row extended
  - the gitignored state files deploy-config-<env>.toml, deploy-<env>.json, deploy-<env>.log under the trainer state dir
affects: [14-12, 14-13, 14-14, 14-15]

plan_head_before: 2a0a00d8ad3a837079b8b6680f9e15031a88e1e9
plan_head_after: cb5fd8cbee9f237c2c65c5e8731ee37906929d18

commits: 2
actuals:
  tokens: 27300     # chars/4 over the added lines of the realized diff (109,192 chars)
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "A security ordering (publish, OAuth on, 401 observed, then grant) is made mechanical: the template cannot declare IAM, the grant takes no URL, and the 401 check runs in the grant's own invocation on an endpoint bound to the granted function"
    - "A revoke step is fail-closed by classification, not by exit code: NotFound and NoSuchEntity proceed, a found policy is deleted and re-read, and every other answer refuses"
    - "An authenticated client refuses every redirect itself (a RedirectHandler that raises) because urllib would re-send the Authorization header to the Location"
    - "A scripted curl shim, extended additively (CURL_BODIES, CURL_HEALTH), serves both the grant's /health binding check and the auth check, so one PATH shim drives a whole dry run"

key-files:
  created:
    - crates/aprender-mcp-laya-train-lambda/.pmcp/deploy.toml.template
    - crates/aprender-mcp-laya-train-lambda/.pmcp/.gitignore
    - scripts/laya_deploy/train_call.py
  modified:
    - scripts/laya_deploy/laya_trainer_ops.py
    - justfile
    - scripts/laya_gates.tsv

key-decisions:
  - "The deploy log must name aprender-mcp-laya-train-lambda AND may name only that crate plus aprender-setfit-train-lambda (its one real -lambda dependency); any other -lambda crate in the log is the same `wrong package` refusal"
  - "A live deploy touches crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs first (as laya-deploy does for the decide crates), because a cached build prints no Compiling line and the package check reads that line"
  - "The /health binding check goes through the Runner as `curl -o FILE -w %{http_code}`, not urllib, so the seam stays the one place a command runs and the same PATH shim serves it and the auth check"
  - "grant and deploy use rule `approval` (LAYA_TRAINER_APPROVED_GRANT / LAYA_TRAINER_APPROVED_DEPLOY equal to the env), checked before any command, like 14-09's spend guard"
  - "laya-trainer-call makes no AWS call, so it runs the pure target and endpoint checks (tenant suffix, server id) but not _laya-account-check; its token-host note lives in the trainer state dir models/laya-trainer"
  - "A JSON-RPC error and a tool result with isError are `ok:false` and exit 1, so a script can branch on the exit status and still read the one JSON line"

patterns-established:
  - "A survivor names a missing case: three selftest mutations survived (delete error ignored, `does not name` vs `also builds`, duplicate placeholder, 401 on the call rather than initialize) and each became an assertion"
  - "A dry run reports what a live run would refuse, and a dry grant still makes the two READ-ONLY http probes; only the aws and cargo-pmcp commands are printed instead of run"

requirements-completed: [D-10, D-01, D-11, D-18, D-06]

coverage:
  - id: D1
    description: "Tracer: template -> rendered config -> swap-guarded deploy, end to end under DRY_RUN. The tracked template declares no IAM and auth off; config fills each placeholder exactly once from outputs-<env>.json (mode 600) and refuses an IAM-declaring template; deploy refuses a foreign profile, a missing config and an unapproved live run, and in DRY_RUN prints the _laya-crates-root-swap line with cargo pmcp deploy --manifest-path crates ... --no-oauth, AWS CALLS: 0, crates/.pmcp and crates/deploy byte-identical"
    requirement: "D-10"
    verification:
      - kind: integration
        ref: "python3 -I scripts/laya_deploy/laya_trainer_ops.py --selftest -> config-render, config-render-dry, config-iam-refused, config-profile, deploy-dry-run, deploy-refusals; LAYA_GATES_ONLY=trainer-config,trainer-deploy just laya-gates-selftest -> DRIFT CHECK OK, LAYA GATES ROWS OK 2 of 52; crates/.pmcp + crates/deploy digest cmp-identical"
        status: pass
    human_judgment: false
  - id: D2
    description: "Fail-closed grant revocation before an OAuth-off redeploy: an absent function (ResourceNotFoundException) or an absent policy (NoSuchEntity) proceeds; a found policy is deleted and must re-read NoSuchEntity (`GRANT REVOKED for redeploy`); AccessDenied, throttling, an unparseable reply, a failing delete or a re-read that does not answer NoSuchEntity is `REFUSED deploy: cannot confirm the grant is absent` with no cargo pmcp call and no state"
    requirement: "D-10"
    verification:
      - kind: unit
        ref: "selftest cases deploy-revoke-grant, deploy-revoke-absent, deploy-revoke-no-function, deploy-revoke-inspect-error, deploy-revoke-delete-error, deploy-revoke-reread (call order asserted: get-function, get-role-policy, delete-role-policy, get-role-policy, touch, swap)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Wrong package and the verified endpoint: the deploy log must name aprender-mcp-laya-train-lambda (and no other -lambda crate but aprender-setfit-train-lambda) or the recipe stops `REFUSED deploy: wrong package`; the endpoint is parsed from the log, its host label must be aprender-mcp-laya-train under the profile's tenant suffix, and deploy-<env>.json {server_id, package, endpoint, deployed_at} is written mode 600; a failed, wrong-label, off-tenant or endpoint-less deploy records nothing"
    requirement: "D-10"
    verification:
      - kind: unit
        ref: "selftest cases deploy-wrong-package (no package, cached build, another -lambda crate), deploy-state"
        status: pass
    human_judgment: false
  - id: D4
    description: "The grant cannot run unless the 401 is observed in the same invocation on the endpoint bound to the granted function: no URL argument (refused as unknown), endpoint from deploy-<env>.json, host label and /health serverId equal aprender-mcp-laya-train, then `_laya-auth-refusal-check <endpoint> 0 train_status`, then the account check, the role lookup and put-role-policy aprender-laya-train-<env>; three classified refusals (not bound, unreachable, OAuth not confirmed (tools/list answered <code>)), none makes an IAM call"
    requirement: "D-10"
    verification:
      - kind: unit
        ref: "selftest cases grant-open-refused, grant-unreachable, grant-url-binding (7 binding variants + URL argument + --url), grant-dry-run, grant-order (health, auth, account, get-function, put-role-policy), grant-refusals"
        status: pass
      - kind: integration
        ref: "LAYA_GATES_ONLY=trainer-grant just laya-gates-selftest -> a scripted curl on PATH, REFUSED grant lines for url-argument, no-deploy-state, foreign-label, health-names-other, oauth-open, unreachable; the dry run prints health, the train_status auth check, get-function and put-role-policy in order, AWS CALLS: 0, the aws recorder never called"
        status: pass
    human_judgment: false
  - id: D5
    description: "`_laya-auth-refusal-check url wait_s [tool]`: the optional third argument selects the tools/call payload; train_status sends {\"name\":\"train_status\",\"arguments\":{\"task_id\":\"auth-refusal-probe\"}} (asserted on the scripted curl's received body); with no third argument the payload is still classify; any other tool is `REFUSED auth-check: config` before a request is sent"
    requirement: "D-10"
    verification:
      - kind: integration
        ref: "LAYA_GATES_ONLY=auth-refusal-check ... -> auth-tool, auth-default, auth-tool-unknown; probe-auth and auth-selftest rows (the decide callers, 231 s and 457 s) stay GREEN"
        status: pass
    human_judgment: false
  - id: D6
    description: "train_call.py: list-tools, validate, train (with and without epochs, task-augmented), status, result (tasks/result), cancel (tasks/cancel) and call-tool (the argument object embedded byte for byte, an unknown seeds key included); bearer only from the minted environment and scrubbed from every output line; every redirect refused (the redirect target receives no request); a 401 on initialize or on the call is one error line, exit 1, no body echoed; the recipe mints through m2m_token.py and the minted bearer reaches the loopback stub"
    requirement: "D-10"
    verification:
      - kind: unit
        ref: "python3 -I scripts/laya_deploy/train_call.py --selftest -> 19 cases incl. call-tool, call-redirect, call-401, call-401-call, call-scrub, call-iserror, call-sse, call-recipe; TRAIN CALL SELFTEST OK"
        status: pass
      - kind: integration
        ref: "LAYA_GATES_ONLY=trainer-call just laya-gates-selftest -> five must_fail refusals, the selftest, and the chain case (authorization_bearer_matches=true at the stub, token and client secret absent from the recipe output)"
        status: pass
    human_judgment: false
  - id: D7
    description: "The selftests can fail: 14 source mutations of the config/deploy code and 29 of the grant and client code were each applied in place; every one turned a named case red except four first-round survivors (delete error ignored, a log with no package line, a duplicate placeholder, a 401 on the call rather than on initialize), each of which got a stronger assertion or a new case and was then killed; both files restored byte-identical after each table"
    requirement: "D-01"
    verification:
      - kind: other
        ref: "in-place mutation tables run from a scratch script with the original cmp-checked after each; no mutation survives the final selftests"
        status: pass
    human_judgment: false

duration: 55min
completed: 2026-10-29
status: complete
---

# Phase 14 Plan 10: Publish, grant and call the Laya training server Summary

**Publishing the request function, granting it access and calling it are four guarded recipes whose order is mechanical: the deploy template cannot declare IAM, a redeploy first deletes any grant (fail-closed), the grant takes no URL and sees the 401 itself on an endpoint bound to the function it grants to, and the M2M client never prints the bearer or follows a redirect; every one is proven offline.**

## Performance

- **Duration:** about 55 min wall clock, most of it two gate sweeps (the decide rows probe-auth and auth-selftest are 4 and 8 minutes)
- **Tasks:** 2 (one tracer, one auto/tdd)
- **Files:** 3 created, 3 modified (`laya_trainer_ops.py` 1,891 -> 2,612 lines, `train_call.py` 588 lines, `justfile` +291, `laya_gates.tsv` +4 rows and one row extended)

## Accomplishments

- **Template, no IAM.** `deploy.toml.template` names server `aprender-mcp-laya-train`, `[auth] enabled = false` with the reason, exactly three `[environment]` placeholders (the names the bootstrap reads) and no IAM table; its header records the package-resolution trap, the publish -> OAuth -> 401 -> grant order, and that the revoke-before-redeploy rule stays correct if cargo-pmcp ever renders auth on for pmcp-run. `config` fills each placeholder once, validates each value's shape, re-parses the result (no `iam` key, server name, auth off) and writes it mode 600; a template that declares IAM, has a wrong placeholder count or turns auth on is refused.
- **Deploy.** `deploy` revokes `aprender-laya-train-<env>` first, fail-closed, then runs `_laya-crates-root-swap crates <cfg> <snap> cargo pmcp deploy --manifest-path crates --regenerate-stack --no-post-deploy-test --no-oauth --no-color` with PMCP_TARGET/AWS_PROFILE/AWS_REGION set and PMCP_API_URL unset. The log must name the right package; the endpoint is parsed from it and bound by host label and tenant suffix; `deploy-<env>.json` is the only thing `grant` trusts.
- **Grant.** No URL by construction (an extra argument or `--url` is refused as unknown). Order: approval guard, outputs policy, target, endpoint from deploy state, `/health` must name the server, `_laya-auth-refusal-check <endpoint> 0 train_status`, account check, role lookup, put-role-policy. Refusals are classified so the owner knows what to do.
- **Auth-check tool argument.** `_laya-auth-refusal-check url wait_s tool="classify"`: `train_status` reads a task id that cannot exist, so the probe could never start a job even on an open server; the default and every decide caller are unchanged (the probe-auth and auth-selftest rows stay green).
- **Client.** `train_call.py` plus `just laya-trainer-call` (token through m2m_token.py, never argv/output; https only; redirects refused; one JSON line per run).
- **Gate rows.** trainer-config, trainer-deploy, trainer-grant, trainer-call, and the auth-refusal-check row extended; the full drift check passes (58 laya recipes enumerated, all gates covered).

## Task Commits

1. **Task 1 (tracer): template, config, swap-guarded fail-closed deploy** - `3b4ed0b2c` (feat)
2. **Task 2: grant, auth-check tool argument, authenticated client** - `cb5fd8cbe` (feat)

The tracer's `<verify>` was re-run end to end before Task 2 (template negative check, ignore check, 51 selftest cases, rows green with `DRIFT CHECK OK`, crates/.pmcp and crates/deploy digest identical).

## Decisions Made

See `key-decisions`. The two that matter for 14-12 and 14-13: the deploy and grant approval variables (`LAYA_TRAINER_APPROVED_DEPLOY=<env>`, `LAYA_TRAINER_APPROVED_GRANT=<env>`), and the state file shape below.

## State and interface (for 14-12, 14-13)

- `deploy-<env>.json`: `{schema: "laya-trainer-deploy-v1", env, server_id, package, endpoint, deployed_at}`; `deploy-<env>.log` (mode 600) is the full cargo-pmcp log; `deploy-config-<env>.toml` is the rendered config (all gitignored, default state dir `models/laya-trainer`).
- Order for the live run: `just laya-trainer-config dev ze-kasher-dev`, `LAYA_TRAINER_APPROVED_DEPLOY=dev just laya-trainer-deploy dev ze-kasher-dev`, the owner enables OAuth for the deployment on pmcp.run, `LAYA_TRAINER_APPROVED_GRANT=dev just laya-trainer-grant dev ze-kasher-dev`, then `just laya-trainer-call ze-kasher-dev <endpoint> ...` with `LAYA_M2M_FILE`.
- `laya-trainer-call` commands: `list-tools`, `validate URI`, `train URI NAME [EPOCHS]`, `status TASK_ID`, `result TASK_ID`, `cancel TASK_ID`, `call-tool TOOL JSON_ARGS` (e.g. `call-tool train '{"dataset_uri":"...","name":"n","seeds":[1]}'` for the unknown-argument negative test). Exit 0 only when `ok` is true; a 401 is exit 1 with `{"ok":false,"error":{"http_status":401,...}}`.
- `grant` and `deploy` need the stack's `outputs-<env>.json` (14-09); `grant` refuses a placeholder `RequestLambdaPolicy` on a live run.
- On a python.org Python the client falls back to `/etc/ssl/cert.pem` when it has no CA bundle (13-HANDOFF section 9); `m2m_token.py` itself still needs `SSL_CERT_FILE=/etc/ssl/cert.pem` there.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing critical] A live deploy forces the request crate to recompile**
- **Found during:** Task 1, reading `laya-deploy`'s live path
- **Issue:** the plan requires the deploy log to name `aprender-mcp-laya-train-lambda`, but a cached cargo build prints no `Compiling` line, so a correct deploy could be refused as the wrong package (or, worse, the check be made vacuous by dropping it).
- **Fix:** a live run touches `crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs` first (printed in the Runner, skipped in a dry run), as `laya-deploy` does for the decide crates; the case `deploy-revoke-grant` pins the call order (`... touch, SWAP`). A log with no `Compiling` line at all is the `wrong package` refusal (`deploy-wrong-package`).
- **Commit:** `3b4ed0b2c`

**2. [Rule 1 - Bug avoided] The package check had to allow the request crate's one real -lambda dependency**
- **Issue:** `aprender-mcp-laya-train-lambda` depends on `aprender-setfit-train-lambda`, so a correct build log names both; a check that refused any second `-lambda` crate would refuse every correct deploy (the decide recipe allows `aprender-mcp-decide-lambda` for the same reason).
- **Fix:** exactly those two crates are accepted; any other `-lambda` crate in the log is refused (`deploy-wrong-package`, the `aprender-mcp-chronos-lambda` variant).
- **Commit:** `3b4ed0b2c`

**3. [Rule 2 - Missing critical] The health probe goes through the Runner and the shared curl shim, and the shim grew two additive variables**
- **Issue:** the plan names a fake /health and a fake curl but not how they share one seam. A urllib GET would bypass the Runner (and DRY_RUN's printing) and need a second fake.
- **Fix:** `curl -sS -m 30 -o FILE -w %{http_code}` through `Runner.run(local=True)`; `_laya-auth-curl-shim` gained `CURL_BODIES` (logs each request body) and `CURL_HEALTH`/`CURL_HEALTH_CODE` (answers a GET of `/health`); unset, the shim behaves exactly as before (probe-auth and auth-selftest rows green).
- **Commit:** `cb5fd8cbe`

### Differences from the plan text (no behaviour lost)

- **`--template FILE`** on `config` is an addition, honoured only under `DRY_RUN=1` (refused otherwise), so the must_fail row can hand the recipe a template copy that declares IAM, mirroring 14-09's `--stage-json`.
- **Extra refusals and cases:** `config-render-dry`, `config-profile`, `deploy-refusals`, `deploy-revoke-reread` (a delete that does not take, or a re-read that cannot answer NoSuchEntity), `grant-order`, `grant-refusals` (foreign account, unapproved live grant, absent function, failing put, placeholder policy), `call-401-call`, `call-iserror`, `call-sse`, `call-scrub`, `call-token-env`, `call-url`, `train-usage`. A `Failure` (exit 1) with last lines `DEPLOY FAILED rc=N` / `GRANT FAILED` marks infrastructure errors.
- **`laya-trainer-call` runs the pure target and endpoint checks but not `_laya-account-check`**: a call makes no AWS call (it uses an M2M bearer). It additionally refuses an endpoint naming another server, a preset `APRENDER_DECIDE_PROBE_TOKEN`, a `--bearer`/`--url` argument and a foreign profile before anything runs; loopback http is accepted only under `DRY_RUN=1` (the chain case).
- **A dry-run grant still makes the two read-only HTTP probes** (the `/health` GET and the unauthenticated 401 check), because they are the evidence the refusal depends on; only the aws commands are printed instead of run. Against a deployed endpoint that is two GETs and two unauthenticated POSTs; the gate rows put a scripted curl first on PATH so no row touches the network.
- The deploy's `--no-oauth` and the plan's note that cargo-pmcp cannot render OAuth for pmcp-run are taken as given; nothing here enables OAuth (that is the owner's step, 14-13).

**Total deviations:** 3 auto-fixed (2 missing-critical, 1 bug avoided), 4 differences. **Impact:** none weakens a must-have.

## Auth Gates

None. No AWS, pmcp.run or network call was made: every selftest and row used the simulated world, the PATH curl shim, the aws recorder (never called, `AWS CALLS: 0`) or the loopback stub. `models/laya-trainer/` was never created; no ambient AWS credential or profile was read (the Runner strips `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN` from every child, and the target and account checks read the fixture config under DRY_RUN).

## Issues Encountered

- The first run of both selftests and both mutation tables passed or killed nearly everything at once; the survivors (four) were each a missing assertion, not a defect, and are listed in `coverage` D7.
- `grep` on this box is wrapped by rtk: a `grep -v ... file | grep -q` pipeline printed a diagnostic instead of its verdict, so the template's no-IAM check was re-run in Python (no non-comment line contains `iam`) as well.
- A background waiter that matched `LAYA GATES ROWS OK` fired early because the gates-selftest row's own one-row case prints that string; the sweep (13 rows, 13 GREEN) was read from its own log.

## Known Stubs

None.

## Not Proven Here (belongs to 14-12 and 14-13, owner-run)

- No real `cargo pmcp deploy` ran: the log shapes the parser reads (`mcp_endpoint: "..."`, `Server URL:`, `Compiling <crate> `) are copied from the three archived decide deploy logs under `models/decide/`, not from a laya-train deploy.
- Whether pmcp.run accepts a function with no IAM statements and no `[auth]` OAuth, whether a `--no-oauth` redeploy actually resets OAuth (Phase 13 RESEARCH A3 is the assumption the revoke rule rests on), the real edge `/health` body for this server, and the real Cognito pool and token host for the trainer's M2M client are first exercised by the live run. The M2M client itself is new owner setup (14-13).
- `deploy-<env>.json`'s endpoint is trusted as the log reported it; the live binding evidence is the `/health` serverId and the 401 check at grant time.

## Threat Flags

None new. T-14-40 (no IAM in the template, grant only after an in-invocation 401, revoke before redeploy), T-14-41 (bearer via exec env only, scrubbed, `call-scrub`), T-14-42 (swap, package check, byte-identical restore), T-14-43 (ze-kasher-dev only, account check before live deploy and grant, approval variables), T-14-71 (no URL, deploy-state binding, `grant-url-binding`), T-14-72 (`call-redirect`) and T-14-73 (`deploy-revoke-*`) are each covered by the named cases above.

## User Setup Required

None for this plan. Owner-run later (14-13): the M2M client file for the trainer server, enabling OAuth on pmcp.run after the publish, and the two approval variables.

## Next Phase Readiness

- **14-12/14-13** can run the live order in "State and interface" above; judge `deploy` by its last line `DEPLOY OK aprender-mcp-laya-train` / `DEPLOY FAILED rc=N`, and `grant` by `GRANT OK role <name>` / `REFUSED grant: ...`.
- `python3 -I scripts/laya_deploy/laya_trainer_ops.py --selftest` (57 cases) and `python3 -I scripts/laya_deploy/train_call.py --selftest` (19 cases) run offline in a few seconds; `LAYA_GATES_ONLY=trainer-config,trainer-deploy,trainer-grant,trainer-call,auth-refusal-check just laya-gates-selftest` is the row-level check.

## Self-Check: PASSED

Verified: the three created files exist (`deploy.toml.template`, `.pmcp/.gitignore`, `train_call.py`); commits `3b4ed0b2c` and `cb5fd8cbe` are ancestors of HEAD and `commits: 2` is measured with `git rev-list --count 2a0a00d8a..HEAD`; the ops selftest prints 57 ok, 0 FAIL and ends `LAYA TRAINER OPS SELFTEST OK`; the client selftest ends `TRAIN CALL SELFTEST OK` with 0 FAIL; every case family the two `<verify>` scripts name is present; the 13-row sweep (the new rows, the four 14-09 trainer rows, auth-refusal-check, probe-auth, auth-selftest, target-selftest, gates-selftest) is GREEN with `DRIFT CHECK OK` and `AWS CALLS: 0`; `crates/.pmcp` and `crates/deploy` are cmp-identical before and after; the generated `crates/aprender-mcp-laya-train-lambda/.pmcp/deploy.toml` is gitignored; no commit deleted a file.
