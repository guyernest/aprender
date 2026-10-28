---
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
plan: 06
subsystem: decide-deploy-tooling
status: complete
tags: [oauth, m2m, client-credentials, justfile, offline-selftest, mutation-testing, pmcp-run, augury-dev]
requires:
  - phase: 13-04
    provides: "_laya-target-check, _laya-account-check, _laya-state-dir, _laya-endpoint-check, the augury-dev state dir, _laya-pmcp-fixture-config"
  - phase: 13-11
    provides: "the ze-kasher-dev archive and its pinned MANIFEST.sha256 (re-verified after every run)"
provides:
  - "scripts/laya_deploy/m2m_token.py: mint-and-exec helper, --token-host, --stub-server, --selftest (47 cases)"
  - "laya-probe-auth <profile> <url> <probe args>: the probe under a token minted from LAYA_M2M_FILE"
  - "_laya-auth-refusal-check <url> <wait_s>: 401 on tools/list then tools/call, first_401_utc"
  - "the LAYA_M2M_FILE re-exec of laya-deploy-verify (floor 1800 s) and laya-deploy (floor 2400 s)"
  - "_laya-auth-selftest (CHAIN, REFUSAL, PROBE-AUTH, RE-EXEC, RE-EXEC LIVE, TOKEN HOST, WIRING), run by laya-deploy-selftest"
  - "gate rows auth-refusal-check, probe-auth, auth-selftest"
  - "13-AUTH-EVIDENCE.json (laya-auth-evidence-v1)"
affects: [13-08, 13-09, 13-10]
actuals:
  tokens: 25781
  tasks: 3
  commits: 5
commits: 5
plan_head_before: 1847edfebf73ec31232bebca453cb22ec0ab2764
plan_head_after: 38db44c97e7326cbd4715e2f4b8ff7dd03d2b64a
tech-stack:
  added: []
  patterns:
    - "a secret becomes a token only inside the child's environment: os.execvpe, never argv, never printed"
    - "re-exec the recipe itself under the helper, guarded by LAYA_M2M_MINTED, so every probe of the run carries the token"
    - "EXECUTE a live branch offline: a python3 shim that intercepts only the minting helper plus a fake aws that stops at the first lambda call"
    - "the shared token-host rule lives in the helper (--token-host), not in three recipe copies"
key-files:
  created:
    - scripts/laya_deploy/m2m_token.py
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-AUTH-EVIDENCE.json
  modified:
    - justfile
    - scripts/laya_gates.tsv
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/deferred-items.md
key-decisions:
  - "The token-host precedence is a mode of the helper (--token-host), shared by laya-probe-auth, laya-deploy-verify and laya-deploy; a separate recipe would have been a fourth gate row the evidence schema does not list"
  - "laya-deploy runs a live account check before minting (step 4b) whenever LAYA_M2M_FILE is set or the run is the minted child; the (5b) check after eligibility still runs, so the no-auth path is unchanged"
  - "laya-probe-auth refuses its environment and argv problems (no LAYA_M2M_FILE, a pre-set token, a --bearer* word, a second --url) BEFORE the target checks, so a misconfigured call makes no sts call"
  - "The helper ignores ambient http(s)_proxy (ProxyHandler({})) and refuses a non-header-safe access_token, beyond the plan's list"
requirements-completed: [D-07, D-08, D-06]
coverage:
  - id: D1
    description: "m2m_token.py mints from a descriptor-checked file, never follows a redirect, requires form-body minting unless --allow-basic, and execs the command with the token in its environment only"
    requirement: D-07
    verification:
      - kind: unit
        ref: "python3 -I scripts/laya_deploy/m2m_token.py --selftest -> M2M TOKEN SELFTEST OK (47 cases, 0 FAIL)"
        status: pass
    human_judgment: false
  - id: D2
    description: "CHAIN: file -> helper -> probe environment -> wire against a loopback stub; the mismatched-bearer run fails"
    requirement: D-07
    verification:
      - kind: integration
        ref: "just _laya-auth-selftest -> CHAIN bearer reached the wire (authorization_bearer_matches=true), AWS CALLS: 0, AUTH SELFTEST OK; LAYA_AUTH_SELFTEST_STUB_MISMATCH=1 -> AUTH SELFTEST FAILED"
        status: pass
    human_judgment: false
  - id: D3
    description: "_laya-auth-refusal-check, the laya-probe-auth refusals, the dry and EXECUTED live re-exec of laya-deploy-verify and laya-deploy, the token-host precedence and the wiring scan"
    requirement: D-07
    verification:
      - kind: integration
        ref: "Task 2 <automated>: motivation-id auth selftest, LAYA_GATES_ONLY sweep of 8 rows (DRIFT CHECK OK, LAYA GATES ROWS OK), motivation-id deploy selftest (disarmed OK line, AWS CALLS: 0), 13-11 manifest and pin"
        status: pass
    human_judgment: false
  - id: D4
    description: "D-06: the auth and deploy selftests leave every models/decide file byte-identical, the ze-kasher-dev record included"
    requirement: D-06
    verification:
      - kind: other
        ref: "_laya-state-snapshot cmp around the auth selftest and around the deploy selftest; shasum -a 256 -c on the 13-11 manifest after every run"
        status: pass
    human_judgment: false
  - id: D5
    description: "A1..A10 each turned its named case RED; 13-AUTH-EVIDENCE.json records the chain, the policies, both live branches and the mutation table with no credential-shaped value"
    requirement: D-08
    verification:
      - kind: other
        ref: "Task 3 <automated>: AUTH EVIDENCE OK"
        status: pass
    human_judgment: false
duration: 2h 1m
completed: 2026-10-28
---

# Phase 13 Plan 06: The Authenticated Path to an OAuth-Protected Decide Server Summary

**A client-credentials token is minted from a mode-0600 file by a stdlib helper. The helper checks the opened descriptor, never follows a redirect and requires form-body credentials. It then execs the probe with the token in its environment only. laya-deploy-verify and laya-deploy re-exec themselves under the helper when LAYA_M2M_FILE is set. A refusal probe measures where an unauthenticated tools/list and tools/call get 401. All of this was proven offline: on a loopback stub, with both live branches executed through shims, and with each of A1..A10 turning its named case RED.**

## Performance

- **Duration:** 2h 1m (2026-10-28T01:44Z to 03:46Z)
- **Tasks:** 3 (tracer TDD, TDD auto, auto)
- **Files:** 2 created (the helper, the evidence); 3 modified (justfile, scripts/laya_gates.tsv, deferred-items.md)

## Accomplishments

- **`scripts/laya_deploy/m2m_token.py`** (stdlib only, `python3 -I`):
  - It opens the file with O_NOFOLLOW and runs every check on the fstat of that descriptor: a regular file, owned by the caller, mode exactly 0600 or 0400, at most 16384 bytes.
  - The file must hold exactly client_id, client_secret and token_url, plus an optional scope.
  - token_url must be https on `--expect-host` with the path `/oauth2/token`.
  - The opener's redirect handler refuses, so any 3xx is `REFUSED m2m-redirect`. Ambient proxies are ignored.
  - Credentials are sent as a form body. HTTP Basic is one diagnostic attempt (add_unredirected_header). A Basic-only success is `REFUSED m2m-auth-method` unless `--allow-basic`, which prints `(auth basic, NOT partner-ready)`.
  - expires_in has a floor. The command is exec'd with `APRENDER_DECIDE_PROBE_TOKEN` and `LAYA_M2M_MINTED=1`.
  - There are 14 refusal rules, `REFUSED m2m-<rule>:`, each with exit 2.
- **`laya-probe-auth`**: checks the target, the account, the endpoint and the server id, then execs the helper with the probe as its command.
- **`_laya-auth-refusal-check`**: sends tools/call only after tools/list has answered 401. It records initialize and the discovery document as observations, never as pass criteria.
- **The re-exec of laya-deploy-verify (1800 s floor) and laya-deploy (2400 s floor)**:
  - Each is guarded by LAYA_M2M_MINTED, and a pre-set token is refused.
  - DRY_RUN mints nothing.
  - LAYA_M2M_MIN_EXPIRES overrides the floor, within 300..86400.
  - laya-deploy refuses when no endpoint exists yet.
- **Token host**: LAYA_M2M_TOKEN_HOST wins, else the `<state dir>/m2m-token-host` note, else the endpoint host. A conflict between the first two is refused.
- **Three gate rows**: auth-refusal-check, probe-auth and auth-selftest. laya-deploy-selftest now runs `_laya-auth-selftest` as one of its cases.

### Quoted lines (final restored run, `just _laya-auth-selftest`; local paths shortened)

```
m2m-token ok: host 127.0.0.1 HTTP 200 expires_in 3600 s (auth form)
CHAIN bearer reached the wire (authorization_bearer_matches=true); token and secret absent from the recipe output
REFUSAL accept: tools/list 401, tools/call 401 -> exit 0, auth-refusal ok: tools/list 401, tools/call 401 after 1 poll(s) (0 s); first_401_utc 2026-10-28T03:24:03Z; initialize 200 (observed, not asserted); oauth-protected-resource 200 (observed)
REFUSAL accept: tools/list 200 then 401 within wait 30 -> exit 0, auth-refusal ok: tools/list 401, tools/call 401 after 2 poll(s) ...
REFUSAL accept: call count after polling -> tools/list, tools/list, then 1 tools/call (none while tools/list was open)
REFUSAL refuse: tools/list 200 with wait 0 -> exit 1, REFUSED auth-open: unauthenticated tools/list answered HTTP 200 after 1 poll(s) (0 s, wait 0 s); tools/call was not sent
REFUSAL refuse: call count while tools/list is open -> 0 tools/call requests sent
REFUSAL refuse: tools/list 401, tools/call 200 -> exit 1, REFUSED auth-open: tools/call answered HTTP 200 without a bearer although tools/list answered 401
REFUSAL refuse: curl transport failure -> exit 1, REFUSED auth-check: transport (curl exit 7) on tools/list, poll 1
REFUSAL refuse: an http URL -> exit 2, REFUSED auth-check: config the URL (65 characters) is not https://<host>/mcp
REFUSAL refuse: a URL not ending /mcp -> exit 2, REFUSED auth-check: config ...
REFUSAL refuse: a non-integer wait -> exit 2, REFUSED auth-check: config wait_s is not a whole number of seconds
REFUSAL refuse: a wait over 3600 -> exit 2, REFUSED auth-check: config wait_s is over 3600
RE-EXEC LIVE laya-deploy-verify: 1 helper invocation, argv preserved (--file <selftest dir>/m2m-fixture.json --expect-host aprender-mcp-decide-antisemite.eu-west.augury.pmcp.run --min-expires 1800 -- just laya-deploy-verify <selftest dir>/laya_tiny.apr aprender-mcp-decide-antisemite augury-dev 2); the child's lambda get-function-configuration --prof call carried the fixture token (sha256 a56fd4ea2e37) and LAYA_M2M_MINTED=1; token absent from the output
RE-EXEC LIVE laya-deploy-verify with LAYA_M2M_MIN_EXPIRES=600: 1 helper invocation, argv preserved (... --min-expires 600 -- just laya-deploy-verify ...); ... LAYA_M2M_MINTED=1; token absent from the output
RE-EXEC LIVE laya-deploy: 1 helper invocation, argv preserved (--file <selftest dir>/m2m-fixture.json --expect-host aprender-mcp-decide-antisemite.eu-west.augury.pmcp.run --min-expires 2400 -- just laya-deploy <selftest dir>/laya_tiny.apr crates/aprender-decide/tests/fixtures/laya_tiny crates/aprender-decide/tests/fixtures/laya_tiny/data crates/aprender-decide/tests/fixtures/laya_tiny/checkpoint aprender-mcp-decide-antisemite dev augury-dev); the child's sts get-caller-identity --profile augury call carried the fixture token (sha256 a56fd4ea2e37) and LAYA_M2M_MINTED=1; token absent from the output
RE-EXEC LIVE laya-deploy: the child stopped at REFUSED eligibility: REFUSED SyntheticNotDeployable before any non-sts AWS call
AWS CALLS: 0
AUTH SELFTEST OK
```

The run under the motivation id (`LAYA_SELFTEST_SERVER=aprender-mcp-decide-motivation`, Task 2) printed the same lines for that id.

### Mutation table (13-AUTH-EVIDENCE.json `mutations`; each restore by `git checkout -- <file>`)

| id | edit | RED (abridged) | restored |
|---|---|---|---|
| A1 | the helper also prints the token to stderr | helper `FAIL token-absent` (3 FAIL lines); auth `FAIL CHAIN: ... leaked: token` | M2M TOKEN SELFTEST OK |
| A2 | the helper skips the file-mode check | `FAIL file mode 0644 refused m2m-file-mode` (0700 and 0640 too) | M2M TOKEN SELFTEST OK |
| A3 | the refusal check accepts 200 on tools/list | `FAIL REFUSAL refuse: tools/list 200 with wait 0: exit 0 (want 1)` | justfile clean |
| A4 | laya-deploy-verify's re-exec `if` loses the LAYA_M2M_MINTED guard | `FAIL WIRING laya-deploy-verify: the helper exec (line 64) is not under an if that requires LAYA_M2M_MINTED != 1`; both live verify cases show 3 helper invocations (the shim's loop cap) | justfile clean |
| A5 | tools/call sent right after every tools/list poll | `FAIL REFUSAL refuse: call count: 1 tools/call request(s) sent while tools/list answered 200` | justfile clean |
| A5 variant | tools/call only on a poll that repeats | `FAIL REFUSAL accept: call count after polling: the requests were 'POST tools/list,POST tools/call,POST tools/list,POST tools/call,'` | justfile clean |
| A6 | the helper appends the token to the exec'd argv | `FAIL argv: the exec'd child's sys.argv does not carry the token` | M2M TOKEN SELFTEST OK |
| A7 | the default urllib opener | `FAIL redirect 301/302/303` and the http-Location case (the second listener counted the followed request); 307/308 stay green, see below | M2M TOKEN SELFTEST OK |
| A8 | a Basic-only mint accepted without --allow-basic | `FAIL auth-method: a Basic-only endpoint is refused without --allow-basic` | M2M TOKEN SELFTEST OK |
| A9 | the mode check becomes `mode & 0o077 == 0` | `FAIL file mode 0700 refused m2m-file-mode` | M2M TOKEN SELFTEST OK |
| A10 | the verify re-exec drops "$N" | `FAIL RE-EXEC LIVE laya-deploy-verify: helper argv ... is not ...` | justfile clean |

After the last restore, both files were clean, and the helper selftest and the auth selftest were OK on `6f6ef9b82`.

## Task Commits

1. **Task 1 (tracer): the helper, laya-probe-auth and the CHAIN case.** `dabbc5f71` (feat). This was the tracer gate in end-of-phase mode with an automated-only verify. The verify was re-run and passed (the mismatch run FAILED as required) before Task 2 began.
2. **Task 2 RED: the tables and three rows.** `4ab56e789` (test)
3. **Task 2 GREEN: the refusal check and the two re-execs.** `1a27e1298` (feat)
4. **Task 3: the call-order test fix that A5 exposed.** `6f6ef9b82` (test)
5. **Task 3: 13-AUTH-EVIDENCE.json.** `38db44c97` (docs)

## TDD Gate Compliance

- **Task 1:** one commit, following the precedent of the 13-04 tracer. The helper's selftest and the CHAIN case were written together with the helper. The mismatch run shows the CHAIN case can fail: `AUTH SELFTEST FAILED (1 failed checks)` on `authorization_bearer_matches=false`.
- **Task 2 RED (`4ab56e789`):** `AUTH SELFTEST FAILED (18 failed checks)`.
  - Nine were REFUSAL cases: `Justfile does not contain recipe _laya-auth-refusal-check`.
  - Four were RE-EXEC dry cases: no DRY-RUN line, and the pre-set token was not refused.
  - Three were RE-EXEC LIVE cases with 0 helper invocations.
  - Two were WIRING cases with 0 exec lines.
  - CHAIN, PROBE-AUTH and TOKEN HOST passed, because they exercise Task 1 code. The shell ran without error.
  - A first RED run also flagged the selftest's own WIRING case-table line as a probe line. That was a harness fault, and it was fixed before the RED commit by building the planted string at runtime.
  - The classifier (`tdd-red-evidence`) does not read this justfile output format, so this text assessment stands in for its record, as in 13-01 and 13-04.
- **Task 2 GREEN (`1a27e1298`):** `AUTH SELFTEST OK`, and the full Task 2 verify passed (see deviation 1).
- **REFACTOR:** none.

## Decisions Made

See `key-decisions` in the frontmatter.

## Deviations from Plan

**1. [Rule 1 - Side effect, restored] The filtered gates sweep rewrote the antisemite id's eligibility log again.** This is the deferred defect from 13-04.
- The Task 2 verify takes its first snapshot before the sweep, so its last `cmp` tripped. The diff was exactly one file: `models/decide/eligibility-aprender-mcp-decide-antisemite.log`.
- **Fix:** both antisemite eligibility logs were restored from the plan-start backup (`cmp` equal). The deploy-selftest bracket was then re-run on its own: a snapshot before, `laya-deploy-selftest` for the motivation id (`DEPLOY SELFTEST OK (positive dry run SKIPPED: ...)`, `AUTH: 35 case lines`), and a snapshot after, which were equal. The 13-11 manifest and pin were verified afterwards.
- The reproduction is recorded in deferred-items.md.

**2. [Rule 2 - Test gap found by re-mutation] A5 first stayed GREEN.**
- The first A5 edit sent tools/call during polling, but the only call-count case used wait 0, which never polls twice.
- **Fix (`6f6ef9b82`):** the polled accept case now asserts the exact request order: tools/list, tools/list, tools/call.
- A5 was re-defined as the plan words it: tools/call is sent although tools/list was not 401, right after each poll. That A5 and the original variant were both re-run, and both are RED.

**3. [Finding] A7 does not turn every redirect case RED.** Python's own redirect handler never re-sends a POST on 307 or 308 (it raises HTTPError). The helper then refuses that response as `m2m-redirect`, so those two cases stay green under A7. The other four (301, 302, 303 and the http Location) go RED, with the second listener counting the followed request. This is recorded in the evidence (`mutations[A7].note`).

**4. [Rule 3 - Shared helper placement] The token-host precedence is `m2m_token.py --token-host`, not a recipe.**
- A gating recipe would need a fourth row, but the evidence schema pins exactly three rows.
- The python3 shim therefore intercepts only `-I scripts/laya_deploy/m2m_token.py --file ...`, so the `--token-host` call passes through to the real python3.
- The shim also caps re-execs at 3, so A4 fails instead of looping.

**5. [Rule 2] The helper does more than the plan's list.**
- It ignores ambient proxies.
- It refuses a non-header-safe access_token.
- It refuses a command line carrying the client secret.
- It rejects userinfo, a query or a fragment in token_url.
- A file larger than 16384 bytes is reported as `m2m-file-type`.

**6. [Placement] The account checks around the re-execs.**
- laya-deploy-verify keeps its existing account check, which comes before the STATE read. Its re-exec follows the STATE and endpoint checks. So the parent makes one sts call before the re-exec, and the child repeats it with the token.
- laya-deploy gains an account check at step (4b), on the auth path only, before laya-verify. This is what the plan's "after the config, pin, target and account checks and before laya-verify" requires, because its existing check comes after eligibility.

**7. [Environment]** The verify commands' `/tmp/p13-06-*` logs were written to the session scratchpad instead.

**Total deviations:** 7. **Impact:** no change to the goal. One test gap was closed. No ze-kasher-dev local file changed.

## Issues Encountered

None beyond the deviations.
- No live AWS, pmcp.run or OAuth call was made, and every selftest reported `AWS CALLS: 0`.
- The only network traffic went to 127.0.0.1 listeners started by the selftests.
- No client id, secret or token was printed or committed. The CHAIN stub's and the shims' values are random per run, and only their sha256 prefixes appear in the output.

## Known Stubs

None.

## Threat Flags

None beyond the register. T-13-16, -17, -18, -19, -20, -45 and -46 are mitigated as planned, and each is shown RED under its mutation: A1/A6 (T-13-16), A7 (T-13-45), A8 (T-13-46), A3/A5 (T-13-18/-19) and A4/A10 (T-13-20).

## STATE / ROADMAP

- `roadmap.update-plan-progress 13`: the outcome is recorded in the docs commit that follows this summary.
- The STATE.md verbs were deliberately not run, as in 13-04 and 13-05: STATE.md carries the owner's uncommitted edit.
- `requirements.mark-complete` was skipped: D-07, D-08 and D-06 are CONTEXT decision ids, not REQUIREMENTS.md ids.

## Next Phase Readiness

Plans 13-08 and 13-09 can now take three steps on augury-dev without a token on argv:
- accept an auth-on augury server with `LAYA_M2M_FILE=<file> just laya-probe-auth augury-dev <url> ...`;
- run `laya-deploy-verify` and redeploys under the token;
- record the auth window from `_laya-auth-refusal-check`'s `first_401_utc`.

Partner-ready acceptance requires `(auth form)` in the helper's line. Plan 13-08 writes the `m2m-token-host` note when the owner names a Cognito host.

## Self-Check: PASSED

- FOUND: scripts/laya_deploy/m2m_token.py, the evidence JSON (`AUTH EVIDENCE OK`), the justfile recipes laya-probe-auth, _laya-auth-refusal-check, _laya-auth-selftest and _laya-auth-curl-shim, and the three new rows in scripts/laya_gates.tsv.
- FOUND commits: dabbc5f71, 4ab56e789, 1a27e1298, 6f6ef9b82 and 38db44c97, all ancestors of HEAD. `git rev-list --count 1847edfeb..HEAD` gave 5 before this summary's commit.
- `git diff --quiet -- justfile scripts/laya_deploy/m2m_token.py` holds.
- `shasum -a 256 -c` on the 13-11 manifest passes.
- `_laya-state-snapshot` equals the plan-start snapshot.
