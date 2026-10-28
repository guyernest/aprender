---
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
plan: 09
subsystem: decide-deploy
status: complete
tags: [augury-dev, pmcp-run, oauth, m2m, acceptance-replay, cold-start, ze-kasher-dev, isolation]
requires:
  - phase: 13-08
    provides: "the live OAuth-on augury server, the m2m-file-path and m2m-token-host notes, F-13-08-3 (SSL_CERT_FILE)"
  - phase: 13-07
    provides: "ze_kasher_before and augury_antisemite_before three-state Lambda records, the LIVE env array"
  - phase: 13-11
    provides: "the ze-kasher-dev archive MANIFEST.sha256 pin"
provides:
  - "13-DEPLOY-EVIDENCE.json live{}: authenticated cold and warm timings, verify_attempts, recipe_verdict, deployment_acceptance"
  - "13-ACCEPTANCE-REPLAY.json (laya-acceptance-replay-v1): authenticated identity, three singles, largest multi-text call n 7, the 4-text call at 423 tokens, the 9-text count refusal, the closing 401/401, seven derived requester tests"
  - "13-VERIFY-OWNER-REPLY.json: the verify checkpoint, skipped (recipe verified on the first attempt)"
  - "13-DEPLOY-EVIDENCE.json ze_kasher_after (owner-run), augury_antisemite_after, archive_after; both unchanged values derived true"
affects: [13-10]
actuals:
  tokens: 6883     # chars/4 over the added lines of the three evidence records (this SUMMARY excluded)
  tasks: 3
  commits: 4
commits: 4
plan_head_before: 3cd1c8571e884ae09f00737e1530145c673bdf3b
plan_head_after: ae8baec713837dc0c425eec05acee49b9c6d84c2
tech-stack:
  added: []
  patterns:
    - "requester tests and `unchanged` are recomputed by the verify from the lines and records they cite, never trusted as hand-written flags"
    - "reads against an account the executor may not touch are run by the owner into rc/json files; the executor copies and hashes them before recording"
key-files:
  created:
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-ACCEPTANCE-REPLAY.json
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-VERIFY-OWNER-REPLY.json
  modified:
    - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-DEPLOY-EVIDENCE.json
key-decisions:
  - "Authenticated runs against the counter-coach pool set LAYA_M2M_MIN_EXPIRES=900: the pool issues 900 s tokens, below the helper's default 1800 s floor; the recipe documents the override"
  - "The partner HEAD recorded is c632c749..., the value read today; the plan's expected d822f119... has been moved past by the partner, and the check is HEAD and status equality before and after"
  - "ze-kasher-dev after-reads were run by the owner (F-13-07-1); the executor made no ze-kasher-dev call"
requirements-completed: [D-07, D-06, D-02, D-03, D-08]
coverage:
  - id: C1
    description: "authenticated laya-deploy-verify on augury-dev, normalized into live{} with every sample, derived proven_cold and measured-only summaries"
    verification:
      - kind: integration
        ref: "13-09-PLAN.md Task 1 verify (commit 5f8d4867f)"
        status: pass
    human_judgment: false
  - id: C2
    description: "verify checkpoint recorded as a skip after a verified run; deployment_acceptance accepted (recipe verified)"
    verification:
      - kind: integration
        ref: "13-09-PLAN.md Task 2 verify (commit 5e1fa9b88)"
        status: pass
    human_judgment: false
  - id: C3
    description: "authenticated acceptance replay, closing unauthenticated refusal, ze-kasher-dev and augury antisemite unchanged (derived), archive manifest at the 13-11 pin, partner repo unchanged"
    verification:
      - kind: integration
        ref: "13-09-PLAN.md Task 3 verify (commits 74cb28b32, ae8baec71): ACCEPTANCE OK B 8 largest 7"
        status: pass
    human_judgment: false
duration: about 16 min of live work on augury-dev (verify start 05:32:54Z to the closing refusal 05:41:22Z, then the owner's ze-kasher-dev reads at 05:48Z), plus owner wait time
completed: 2026-10-28
---

# Phase 13 Plan 09: augury motivation server measured and accepted behind OAuth Summary

**The augury motivation server passed the partner's function-and-identity tests over an M2M form-body token. Four proven-cold samples took 18,850-22,110 ms against the 30 s cap (worst slack 7,890 ms), and the 4-text batch succeeded at Phase 12's 423 tokens. ze-kasher-dev and the augury antisemite function are unchanged; both `unchanged` values were derived as true.**

## Performance

- **Live work:** 2026-10-28T05:32:54Z (deploy-verify start) to 05:41:22Z (closing unauthenticated refusal). The owner's ze-kasher-dev reads were written at 05:48Z
- **Tasks:** 3/3 (Task 2 skipped and recorded, because the recipe verified on the first attempt)
- **Files modified:** 3 evidence records

## Accomplishments

- **Deploy verify (Task 1, tracer), authenticated:** `m2m-token ok: host counter-coach.eu-west.augury.pmcp.run HTTP 200 expires_in 900 s (auth form)`, then `DEPLOY VERIFY OK aprender-mcp-decide-motivation: 4 cold samples (both shapes), every one < 30000 ms with CloudWatch cold evidence` (rc 0, verify_attempts has one entry).
  - Cold samples (800 tokens each, all proven cold, graviton3): 22,110 / 18,850 / 19,709 / 19,152 ms. In the same order: concentrated, distributed, concentrated, distributed. load_ms 13,709-13,866. cold_wall_ms is min 18,850 and max 22,110. The worst sample is #1 (concentrated), with slack 7,890 ms to the 30,000 ms cap.
  - Warm: 1,161 / 1,175 / 1,131 / 1,122 / 1,105 ms (min 1,105, p50 1,131, max 1,175, n 5).
  - configuration_changes: 4 DECIDE_COLD_BUMP updates on aprender-mcp-decide-motivation only. Timings are reported, not gated.
- **Deployment acceptance (Task 2):** `accepted (recipe verified)`. 13-VERIFY-OWNER-REPLY.json is `{skipped: true}`. The recipe's verdict and the acceptance are separate fields.
- **Acceptance replay (Task 3).** Every call minted once, and every mint line ended `(auth form)` on the 13-08 token host:
  - Identity: identity_matches true, sha d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd, warm.
  - Singles: from 0, 1 and 2 at 105, 106 and 107 tokens, every check true.
  - Largest multi-text call: n = 8 was refused on `classify_max_total_tokens` and recorded in refused_token_budget. n = 7 succeeded at 745 tokens in 4,606 ms.
  - **Batch of 4:** succeeded with every check true at tokens_total 423, equal to Phase 12's T4, in 2,787 ms.
  - Count refusal: 9 texts were refused with is_error, names_count_bound and names_limit_value all true (door 8, unchanged).
  - Max sum deviation: 2.2e-8.
  - All seven requester tests were recomputed from their lines and pass.
- **Closing unauthenticated refusal:** `auth-refusal ok: tools/list 401, tools/call 401 after 1 poll(s) (0 s); first_401_utc 2026-10-28T05:41:22Z; initialize 200 (observed, not asserted); oauth-protected-resource 200 (observed)`.
- **ze-kasher-dev after (owner-run, read-only).** The identity probe returned `{"probe":"identity","probe_id":"probe-18e29b7058f96ed8-af37",...,"identity_matches":true,"artifact_sha256":"d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd",...,"load_header":"cold;load_ms=15283"}`. The Lambda record is present, and its keys equal the 13-07 before-record: LastModified 2026-10-26T22:20:47Z, CodeSha256 9KJKCAWk..., 10240 MB, Active, arm64, env sha equal to the pin, no reserved concurrency. **unchanged = true (derived).** The rc files were 0/0/0, and this time they are true statuses, not inverted.
- **Augury antisemite after:** the keys equal the before-record, so **unchanged = true (derived)**. Its eligibility log is byte-equal (cmp rc 0) to the plan-start scratchpad backup. I re-checked this at the end of the plan.
- **Local record:** the ze-kasher-dev archive MANIFEST.sha256 verifies, and its sha256 equals the 13-11 pin in 13-BASELINE-EVIDENCE.json. models/decide/swap-backup/ is empty.
- **Partner repo:** HEAD c632c749b02011b964c28f1a3a65f943033ff4d7. `status --porcelain` is byte-equal before and after. No partner text was sent and no label was written (D-03).

## Task Commits

1. **Task 1 (tracer): authenticated cold and warm timings:** `5f8d4867f`
2. **Task 2: verify checkpoint, skipped (recipe verified on the first attempt):** `5e1fa9b88`
3. **Task 3, part 1: authenticated acceptance replay and the augury after-checks:** `74cb28b32`
4. **Task 3, part 2: the owner-run ze-kasher-dev after-reads, unchanged derived as true:** `ae8baec71`

## Files Created/Modified

- `.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-DEPLOY-EVIDENCE.json`: live{}, ze_kasher_after, augury_antisemite_after, archive_after
- `.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-ACCEPTANCE-REPLAY.json`: the replay of record
- `.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-VERIFY-OWNER-REPLY.json`: the skip record
- `models/decide/augury-dev/deploy-evidence-aprender-mcp-decide-motivation.json`, `models/decide/augury-dev/plan-*.json` (gitignored, regenerated by the recipe)

## Decisions Made

See key-decisions in the frontmatter.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] The helper's expires_in floor is above the pool's token lifetime**
- **Found during:** Task 1
- **Issue:** the counter-coach Cognito pool issues tokens with expires_in 900 s (13-08). The re-exec helper's default floor is 1,800 s, so it would refuse the token.
- **Fix:** authenticated runs set `LAYA_M2M_MIN_EXPIRES=900`, the override the recipe documents for a shorter Cognito validity. This is recorded in live.measured_by and live.min_expires.
- **Commit:** 5f8d4867f

**2. [Rule 3 - Blocking] The local python3 has no CA bundle (F-13-08-3, carried over)**
- **Found during:** Tasks 1 and 3
- **Fix:** every authenticated call set `SSL_CERT_FILE=/etc/ssl/cert.pem`, the macOS system trust store, as 13-08 did. Verification stayed on. The helper itself is still unfixed.
- **Commits:** 5f8d4867f, 74cb28b32

**3. [Rule 1 - Bug] The rtk hook dropped the trailing newline on the partner status capture**
- **Found during:** Task 3, step 1
- **Issue:** `git status --porcelain` captured through the Bash tool hook was 205 bytes, with no final newline. The verify's own capture (`> file`) is 206 bytes, so `cmp` would have reported a change that never happened.
- **Fix:** I re-captured the before-file raw at 206 bytes and kept the hook capture in the scratchpad as found. Before and after are byte-equal.
- **Commit:** 74cb28b32

### Plan-text drift

**4. The partner HEAD is c632c749..., not the plan's d822f119...**
- The plan says to record "the value read today". The verify compares the recorded HEAD with the HEAD read now, and both are c632c749.... The partner moved past d822f119... before this plan ran.

### Owner-run steps

**5. The ze-kasher-dev after-reads were run by the owner (F-13-07-1)**
- The executor makes no ze-kasher-dev call. The owner ran the three read-only commands: the identity probe, get-function-configuration and get-function-concurrency. The executor copied the output files and hashed them in the session scratchpad (`p13-09-zk-owner.sha256`) before recording them.

## Issues Encountered

- STATE.md: the owner's uncommitted STATE.md is not touched. See the orchestrator report for the state and roadmap verb outcomes (the known Phase 4 `gaps_acknowledged` defect).
- n = 8 at the distributed replay payload exceeded classify_max_total_tokens (800), so the largest admitted multi-text call with the probe's committed texts is n = 7. This is the step-down rule working as planned, not a defect. The door stays at 8 texts.

## Known Stubs

None.

## Threat Flags

None beyond the plan's threat model. No JWT, client secret, absolute path or unknown 12-digit number appears in the committed records, and the Task 3 verify regexes check for each.

## User Setup Required

None. The partner hand-off (13-10) must quote the shared counter-coach token host (F-13-08-2), the 900 s token lifetime, and these timings.

## Next Phase Readiness

- 13-10 can quote this summary's figures: the DEPLOY VERIFY OK line, cold 18,850-22,110 ms (slack 7,890 ms), warm p50 1,131 ms, the batch of 4 at 423 tokens, the largest single call n = 7 within the 800-token budget, and the door at 8.

## Self-Check: PASSED

- FOUND: 13-DEPLOY-EVIDENCE.json, 13-ACCEPTANCE-REPLAY.json, 13-VERIFY-OWNER-REPLY.json
- FOUND commits: 5f8d4867f, 5e1fa9b88, 74cb28b32, ae8baec71 (all ancestors of HEAD)
- Task 3 verify block (re-run after part 2): `ACCEPTANCE OK B 8 largest 7`
