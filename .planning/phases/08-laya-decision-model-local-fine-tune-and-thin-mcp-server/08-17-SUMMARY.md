---
phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
plan: 17
subsystem: infra
tags: [laya, aprender-mcp-decide-lambda, pmcp-run, lambda, deploy, tier-policy, decide-tool-boundary, containment, 3008mb]
outcome: deploy-refused

requires:
  - phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
    provides: "08-16: the one deploy-eligible artifact models/decide/laya-stance-64.apr (gate_pass, sha256 24a44d7e…)"
  - phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
    provides: "08-10: the fail-closed deploy recipes (bucket, upload, config, laya-deploy, grant, teardown, verify)"
provides:
  - "decide-tool-boundary-v1 2.0.0: the classify budget re-priced for the 3,008 MB Lambda tier (lambda_memory_mb 3008, 120 built tokens, 2 texts), with 10,240 MB recorded as the target tier"
  - "a tier mirror: the deploy template's memory_mb is asserted equal to the contract's lambda_memory_mb"
  - "laya-deploy fixed for two pre-grant defects (--no-post-deploy-test, IAM propagation wait) and passing --no-oauth for auth off"
  - "08-LIVE-DEPLOY-EVIDENCE.json: outcome deploy-refused, contained, readiness measured, 5 live assumptions judged"
affects: [08-17 continuation, 08-18, 08-12, decide deploy recipes, pmcp.run edge behaviour]

actuals:
  tokens: 10951    # chars/4 over the realized diff a90a5cd41..371c6bfe7 (43803 chars); this SUMMARY excluded
  tasks: 2         # Task 1 recorded (answered by the user) and Task 2 executed to a contained refusal; Task 3 skipped by the plan's own rule
  commits: 3       # MEASURED: git rev-list --count a90a5cd41..HEAD before this SUMMARY commit
plan_head_before: a90a5cd41e91a3d1b153da91d104e5fba6e3822f

tech-stack:
  added: []
  patterns:
    - "Tier policy in the contract: every envelope constant is priced for ONE lambda_memory_mb, and the deploy template's memory is a tested mirror of it"
    - "Extrapolated budgets show every term and its anchor, so the live cold log (download_ms, sha_ms, build_ms) can falsify each term separately"
    - "A maximal request that the budget would refuse is never built (MaximalError::OverBudget)"

key-files:
  created:
    - .planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-LIVE-DEPLOY-EVIDENCE.json
  modified:
    - contracts/decide-tool-boundary-v1.yaml
    - contracts/aprender/binding.yaml
    - crates/aprender-mcp-decide/src/lib.rs
    - crates/aprender-mcp-decide/src/tests.rs
    - crates/aprender-mcp-decide/tests/e2e_stdio.rs
    - crates/aprender-mcp-decide/README.md
    - crates/aprender-mcp-decide-lambda/src/probe.rs
    - crates/aprender-mcp-decide-lambda/src/tests.rs
    - crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template
    - justfile
    - .planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/deferred-items.md

key-decisions:
  - "USER DECISION 2026-09-27: deploy-auth-off-accept-risk. Deploy open like the chronos precedent, and the user explicitly accepts the cost-amplification risk"
  - "USER DECISION 2026-09-27: memory 3,008 MB, not 10,240 MB. The account is capped at 3 GB until AWS Support approves more; profile ze-kasher-dev"
  - "The 3,008 MB budget is an extrapolation from spike 026's Graviton2 anchors (10,240 and 4,096 MB), scaled by vCPU. Per-token uses the larger anchor (40 ms). The fixed cold cost uses the nearer anchor (4,096 MB), because it refutes the 10 GB anchor for the load term. A software sha256 pass is priced explicitly (4521 ms). The result: 120 tokens and 2 texts"
  - "The GET health-body check in laya-deploy is unrealizable through the pmcp.run edge. A 405 from the edge is a recipe-assumption failure, not a model identity failure. Per the user's failure rule, the contained deploy is recorded as deploy-refused and the plan stops for the human's resume decision"

patterns-established:
  - "Budget tiers: re-deriving a door bound for a new memory size changes the contract first, then the Rust mirror, then the deploy memory, in one commit, before any AWS call"

requirements-completed: []

coverage:
  - id: D1
    description: "decide-tool-boundary-v1 2.0.0 re-priced for the 3,008 MB tier (120 tokens, 2 texts, 17000/3900/40 envelope, margin and cap unchanged), with Rust mirrors, the tier mirror test and the OverBudget builder guard. Committed before any AWS call"
    verification:
      - kind: other
        ref: "pv validate contracts/decide-tool-boundary-v1.yaml -> 0 error(s); pv diff suggested major -> 2.0.0"
        status: pass
      - kind: unit
        ref: "cargo test -p aprender-decide -p aprender-mcp-decide -p aprender-mcp-decide-lambda --lib -> 167 passed"
        status: pass
      - kind: unit
        ref: "crates/aprender-mcp-decide-lambda/src/tests.rs#deploy_memory_is_the_contract_tier (red with the template at 10240, green at 3008)"
        status: pass
      - kind: integration
        ref: "cargo test -p aprender-mcp-decide --test e2e_stdio (tiny leg, and the real laya-stance-64 leg: 71 and 66 built tokens, one text per call)"
        status: pass
      - kind: other
        ref: "make contract-audit-phase8 rc 0; probe --plan-only on laya-stance-64.apr: concentrated 1 text / 120 tokens, distributed 2 texts / 120 tokens"
        status: pass
    human_judgment: false
  - id: D2
    description: "laya-deploy fixed for the pre-grant post-deploy suite and IAM propagation, and passes --no-oauth when auth is off. The offline selftest still passes with zero AWS calls"
    verification:
      - kind: integration
        ref: "just laya-deploy-selftest -> DEPLOY SELFTEST OK, AWS CALLS: 0, DEPLOY MARKERS: 0"
        status: pass
    human_judgment: false
  - id: D3
    description: "Read-only readiness before any write: A10 confirmed, A2 = 3008 (the largest existing MemorySize; no memory field in the account settings; the created function reads back 3008), and no decide or laya function existed"
    verification:
      - kind: other
        ref: "aws sts get-caller-identity / lambda list-functions / lambda get-account-settings (profile ze-kasher-dev); get-function-configuration MemorySize 3008"
        status: pass
    human_judgment: false
  - id: D4
    description: "Live deploy of the gated model on pmcp.run with identity == H proven by a warm classify (D-18, D-11)"
    requirement: "D-18"
    verification:
      - kind: other
        ref: "just laya-deploy -> rc 1: GET health 405 at the pmcp.run edge -> contained (reserved concurrency 0 verified)"
        status: fail
    human_judgment: true
    rationale: "Refused and contained by the recipe's unrealizable health check (D-ITEM-08-17-A). Resuming needs a human choice of the replacement check and approval to lift the containment"
  - id: D5
    description: "Cold accepted region of both maximal shapes at 3,008 MB, with CloudWatch Max Memory Used and init duration per sample (D-10)"
    requirement: "D-10"
    verification: []
    human_judgment: true
    rationale: "Not taken: Task 3 runs only after an identity-ok deploy. FALSIFY-DECIDE-TOOL-009 stays LIVE-PENDING"

duration: 51min
completed: 2026-09-27
status: halted
---

# Phase 8 Plan 17: Go/No-Go and Live Decide Deploy Summary

**The classify budget is now priced for the 3,008 MB tier the account allows: 120 built tokens and 2 texts, extrapolated term by term from spike 026, in contract 2.0.0. The gated stance model deployed to pmcp.run at 3,008 MB with its sha256 pin, and the compile log names only the decide package. laya-deploy then contained it. Its GET health check can never reach the function, because the pmcp.run edge answers GET `/mcp` itself with 405. The function is throttled to reserved concurrency 0 and the grant is removed. Resuming needs a human choice.**

## Performance

- **Duration:** 51 min, from 2026-09-27T20:25:16Z to 21:16:48Z.
- **Tasks:** Task 1 was recorded, and Task 2 executed up to a contained refusal. Task 3 was skipped by the plan's own rule, because a refused deploy takes no cold samples.
- **Files:** 1 created, 11 modified.

## Task 1: the go/no-go answer (recorded)

| Item | Answer |
|---|---|
| Option | `deploy-auth-off-accept-risk`, the user's decision on 2026-09-27 |
| Auth | Off: `[auth] enabled = false` and `--no-oauth`. pmcp.run reported `oauthEnabled=false`. No OAuth provider |
| Risk | **The user explicitly accepts the cost-amplification risk of an open function** (RESEARCH Security Domain V2). This matches the live chronos precedent |
| Memory | **3,008 MB**, not 10,240 MB. The account is capped at 3 GB until AWS Support approves more |
| Profile | `ze-kasher-dev` (A10). The user did not confirm it explicitly, so it was verified read-only below |
| Start sha | `a90a5cd41` (`/tmp/p08-17-start.sha`). Deploy baseline after the amendment: `968d73e99` (`/tmp/p08-17-deploy-base.sha`) |

## The 3,008 MB tier amendment (commit `968d73e99`, before any AWS call)

The contract's own rule applied: lower `classify_max_total_tokens` after re-deriving it here, and never raise the margin. The arithmetic below is in the contract description. It is an extrapolation, and the live cold samples are what falsify it.

vCPU scales with memory at 1 vCPU per 1,769 MB: 6 vCPU at 10,240 MB, 2.315 at 4,096 MB and 1.700 at 3,008 MB. The anchors are spike 026's Graviton2 raw records.

| Term | Derivation | Value |
|---|---|---|
| Per built token at 512 | 10,240 MB: 5660/512 x 6/1.700 = 39.0. 4,096 MB: 12501/512 x 2.315/1.700 = 33.3. The larger, rounded up | `tier_ms_per_token_at_512` **40** (renamed from `g2_ms_per_token_at_512`) |
| Gateway + client overhead | max(client wall - duration), not memory-scaled | 895 |
| Download | 9035-9222 ms, flat between 10 GB and 4 GB | 9222 |
| sha256 pin | sha2 0.10.9 runs its SOFTWARE backend (no `asm`). 1370 ms on M4, x3.3 (spike's G2/M4 ratio) | 4521 |
| Parse + widen | 4 GB: 1675 x 2.315/1.700. The 10 GB anchor over-predicts the 4 GB measurement 2x, so it is not used here | 2281 |
| Cold fixed | 16919, declared UP | `cold_start_budget_ms` **17000** |
| Probe replay | 2 x 48 x 40 = 3840, declared UP | `probe_budget_ms` **3900** |
| Budget | (30000 - 17000 - 3900 - 4000) / 40 = 127, declared DOWN | `classify_max_total_tokens` **120** |
| Count | floor(120 / 57). 57 is the shortest built row of the stance task, measured with `probe --plan-only` | `classify_max_texts` **2** |

- **Unchanged:** `margin_ms` 4000 and `api_gateway_timeout_ms` 30000.
- **Proof obligation:** 120 x 40 + 17000 + 3900 + 4000 = 29700, which is at most 30000.
- **Target tier, recorded:** 10,240 MB with budget 1024 and 8 texts. Restoring it is a re-derivation plus a memory change.
- **Consequences at 3,008 MB:**
  - A legal request is one text of about 63 state tokens (a tweet), or two very short texts.
  - `truncated: true` is unreachable, because a 512-token row is always over the budget.
  - Two real sentences no longer fit in one call: they build to 71 + 66 = 137 tokens.
- **New constants:** `lambda_memory_mb` 3008 and `served_task_min_row_tokens` 57.
- **Mirrors updated:**
  - `ClassifyLimits::CONTRACTED` is now 2 texts and 120 tokens.
  - The deploy template's `memory_mb` is 3008.
  - New tests: `deploy_memory_is_the_contract_tier` (it went red when the template alone was set to 10240) and `max_texts_fit_the_budget_at_the_shortest_row`.
  - The maximal builder now refuses an over-budget request (`MaximalError::OverBudget`, with a new test).
  - The count tests were renamed so they hold at any tier: `one_over_max_texts_refused_naming_max_texts` and `max_texts_accepted_and_classified`. The contract `test:` lines were updated with them.
  - The description test now reads the bounds from the contract.
  - The e2e test sends contract-sized batches.
  - The binding note and the README were updated.
- **Checks, all passing:**

  | Check | Result |
  |---|---|
  | `pv validate` | 0 errors |
  | `pv diff` | suggested major, so 2.0.0 was applied |
  | `make contract-audit-phase8` | rc 0 |
  | Lib tests (decide crates) | 26 in aprender-mcp-decide and 29 in aprender-mcp-decide-lambda. With aprender-decide, 167 across the three crates |
  | `cargo clippy --no-deps --lib --tests --examples -D warnings` | clean on both crates |
  | rustfmt | clean |
  | e2e | tiny leg and real-model leg pass |
  | `probe --plan-only` on the real artifact | concentrated: 1 text, 120 tokens. Distributed: 2 texts, 120 tokens |
  | `just laya-deploy-selftest` | DEPLOY SELFTEST OK, AWS CALLS: 0 |

## Readiness (read-only, before the first write)

- **A10: confirmed.**
  - `ze-kasher-dev` resolves to an IAM user. The account id was seen in the session only and is not recorded anywhere.
  - 228 functions are visible in it, including `chronos-forecaster` and `pmcp-*`.
  - cargo-pmcp resolves the shared `crates` root to target `dev`, which is `ze-kasher-dev` in us-east-1.
- **A2: 3,008 MB.**
  - The largest existing MemorySize is 3008, held by 2 functions.
  - `get-account-settings` has no memory field. It reports only code sizes and ConcurrentExecutions 1000.
  - The proof is the deploy itself: `aprender-mcp-decide` exists at MemorySize 3008 and reads `Active` / `Successful`.
- **No decide or laya function** existed before the deploy.
- **pmcp.run auth:** the cached token had expired, and cargo-pmcp refreshed it on a read-only `outputs` call.

## Task 2: the live sequence

| Step | rc | Result |
|---|---|---|
| Precondition gates on `968d73e99` | 0 | Audit rc 0. Strict-binding guard: the lifted copy resolved 684 refs, and only the 2 pre-existing contracts dangle. Three-crate lib tests pass. Selftest OK. `laya-verify` reports deploy_eligible true with sha H |
| `just laya-weights-bucket dev ze-kasher-dev` | 0 | Bucket created: private, SSE-S3, tagged |
| `just laya-upload ...` | 0 | `laya-verify` ran first. 846,196,868 bytes went to `decide/aprender-mcp-decide/<H>.apr` |
| `just laya-deploy-config <apr> off ...` | 0 | memory 3008, timeout 30, auth false |
| `just laya-deploy ...` (on `b30f437da`) | **1** | Eligibility and the resolver proof passed. `cargo pmcp deploy --regenerate-stack --no-post-deploy-test --no-oauth` returned 0. The compile log names only `aprender-mcp-decide-lambda`. The grant was applied. **GET health returned 405, and the function was contained** |

- **Refusal:** `IDENTITY FAILURE: GET https://aprender-mcp-decide.us-east.true-mcp.com/mcp failed -- containing (reserved concurrency 0, grant removed)` (`curl: (56) ... 405`).
- **Diagnosis, read-only.** The GET never reached the function, for four reasons:
  - The bootstrap answers every GET with 200 and its package body, so a 405 cannot come from it.
  - The pmcp.run edge answers GET `/mcp` itself: `"SSE streams are not offered at this endpoint. Use POST /mcp."`. The live chronos endpoint gives the same 405.
  - The edge's `/health` returns platform JSON (`status`, `serverId`, `hasDeployment`) with no package field.
  - CloudWatch has no invocation for the GET.
- **Containment is verified:** `get-function-concurrency` reads 0, and `list-role-policies` no longer lists the decide weights policy.
- **Left in place for the human:** the deployment, the function and the S3 object.
- **The shared root was restored byte-identical:** state sha256 `7c6f25eb…`, and the porcelain for `crates/.pmcp` and `crates/deploy` is empty.
- **The only invocation** came at 21:12:10Z. pmcp.run made it itself before the grant. The load failed at the S3 length lookup and re-armed. REPORT: 159 ms, Max Memory Used 37 MB, init 67 ms, memorySize 3008. No model load has happened, so there is **no memory, probe-replay or cold-time evidence yet**.
- **Live assumptions (D-ITEM-08-10-C):**
  - Confirmed: function name == server id; the deployment.toml endpoint; compile lines in the log; the log group is `/aws/lambda/aprender-mcp-decide`.
  - **Refuted:** that a GET reaches the bootstrap's health branch.

## Task 3

Skipped. It runs only after an identity-ok deploy, and the plan's rule is that "Task 3 is skipped; plan 08-18 records it". FALSIFY-DECIDE-TOOL-009 stays `LIVE-PENDING`.

## Task Commits

1. **The 3,008 MB tier amendment** — `968d73e99` (feat)
2. **laya-deploy pre-grant fixes** — `b30f437da` (fix)
3. **Task 2: the deploy-refused evidence record and deferred items** — `371c6bfe7` (feat)
4. **Resume: laya-deploy checks the edge /health serverId** — `3115c690e` (fix)

## Deviations from Plan

**1. [User scope change] Memory 3,008 MB and the contract amendment**
- **Source:** the user's go/no-go answer.
- **What changed:** the plan's 10,240 MB became 3,008 MB. Contracts and crate sources changed inside this plan, in commit `968d73e99`.
- **Verify adaptations:**
  - The plan's Task 2 verify asserts `memory_mb == 10240` and no contract or source diff since the start sha. It was run with `3008` and with the post-amendment baseline `968d73e99`.
  - Both passed: no contract or crate source changed after the amendment.

**2. [Rule 3 - Blocking] laya-deploy ran cargo-pmcp's post-deploy suite before its own grant**
- **Found:** before any write, by reading the recipe against `cargo pmcp deploy --help`.
- **Why it blocks:** every MCP request loads the model from S3, and the grant follows the deploy. So the suite could only see a 403 and exit 3 before the grant and before the identity chain. CloudWatch later confirmed that pmcp.run itself invokes the function before the grant (D-ITEM-08-17-B).
- **Fix:** `--no-post-deploy-test`, plus a `LAYA_GRANT_PROPAGATION_S` (20 s) wait after `put-role-policy`, so a not-yet-visible policy is not contained as an identity failure. `--no-oauth` is passed when auth is off, per the user's instruction.
- **Commit:** `b30f437da`. The selftest was re-run: OK, AWS CALLS: 0.

**3. [Rule 1 - Bug, found in the test] The bound-order proptest lost its token branch at 2 texts**
- **Issue:** with 2 texts, 2 x 20 tokens never reaches the shrunk 64-token budget.
- **Fix:** the shrunk instance now pins `max_texts: 8`, and the KANI harness text names it.
- **Commit:** `968d73e99`.

**4. [Rule 2 - Missing critical] The maximal builder could emit an illegal request**
- **Issue:** DISTRIBUTED with a count above floor(budget / shortest row) produced a request over the budget, which the live probe would have timed as "maximal".
- **Fix:** `MaximalError::OverBudget`, with a test.
- **Commit:** `968d73e99`.

**Total deviations:** 1 user scope change, 3 auto-fixed (1 blocking, 1 bug, 1 missing critical).
**Impact:** no margin, tolerance or artifact moved. The budget was lowered only after re-deriving it.

## Issues Encountered

- **The deploy was refused and contained** by the recipe's unrealizable health check (D-ITEM-08-17-A). This is the blocker below.
- **The `.planning/WINDOWS.md` ledger still refuses every append** (`Ledger entry 24 has invalid status: "resolved"`). The unrun Task 3 is recorded in the evidence and in D-ITEM-08-17-A instead.
- **The rtk hook condenses `aws` output,** so every JSON read used `rtk proxy aws`.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: open-endpoint | 08-LIVE-DEPLOY-EVIDENCE.json | `aprender-mcp-decide` exists on pmcp.run with auth off (user-accepted). It is currently throttled to reserved concurrency 0, so no invocation runs |

## CHECKPOINT: human decision needed to resume (blocking-human)

- **Where things stand.** The function is live but contained: reserved concurrency 0, grant removed. The S3 object is in place.
- **What failed.** The 08-10 health-body check cannot pass on pmcp.run, because the edge answers every GET itself.
- **What still proves identity.** The compile log (passed), and the identity probe, a POST `tools/call` that must return `artifact_sha256 == 24a44d7e…` with labels none, against, favor. The probe is still realizable.

**Options:**
1. **Replace the health GET** with the edge's `/health` `serverId == aprender-mcp-decide` check, and let the identity probe carry package identity. (Recommended: it is realizable, and the probe already defeats the wrong-binary threat.)
2. **Reach the bootstrap directly** with `aws lambda invoke` and a synthetic GET event, so the package-naming body is still checked. This needs one more AWS call type.
3. **Stop at deploy-refused.** Tear down with the commands laya-teardown printed, and let 08-18 record the refusal.

**For 1 or 2, the continuation then runs these steps:**
1. Commits the recipe change.
2. Runs `aws lambda delete-function-concurrency --function-name aprender-mcp-decide`. This is the resume step laya-teardown leaves to the human, so it needs the human's approval.
3. Re-runs `just laya-deploy`, which redeploys, re-grants and runs the identity probe.
4. Takes the warm identity classify.
5. Runs Task 3 (`just laya-deploy-verify ... 2`) at the new 3 GB budget.

## Resume attempt 1 (2026-09-27): blocked on a pmcp.run auth gate

- **User decision:** option 1 above, and the user approved lifting the containment.
- **Recipe change, committed before any AWS write (`3115c690e`):**
  - The health step now derives `https://<host>/health` from the endpoint and cross-checks cargo pmcp's logged `health_endpoint`. It requires `serverId == aprender-mcp-decide` and asserts no package field.
  - The identity probe still carries package identity.
  - The logic lives in two pure helpers, `_laya-edge-health-url` and `_laya-edge-health-check`.
  - `laya-deploy-selftest` drives both through a must-accept/must-refuse table: 6 URL cases, plus 8 body cases that include the measured decide, chronos and 405 bodies and the bootstrap's own body. It also checks the wiring, and re-mutating the wiring (an `$ENDPOINT` GET, or a dropped check) turns that check red.
  - Selftest result: DEPLOY SELFTEST OK, AWS CALLS: 0.
- **Containment was already lifted when this continuation started.** CloudTrail shows `DeleteFunctionConcurrency` at 21:54:40Z by the profile's own IAM user. It was read back, not re-run. Current state:
  - Reserved concurrency is unset.
  - The grant is absent, so any invocation fails the S3 load fast.
  - No invocation was logged between the lift and 22:08Z.
  - The edge `/health` already returns `serverId aprender-mcp-decide`.
- **The re-run of `just laya-deploy` (22:01-22:06Z) exited 1 at an auth gate:**
  - Eligibility and compile passed.
  - It then stopped at pmcp.run's `Failed to get upload URLs`, with `UnauthorizedException: Valid authorization header not provided.`
  - Nothing was uploaded, and no AWS resource changed. The shared root was restored byte-identically.
  - A read-only `cargo pmcp deploy outputs` gets the same error. It refreshed the token on the first run, but it does not now.
- **Blocked on the human:** run `cargo pmcp deploy login --target-type pmcp-run` (browser OAuth). The continuation then re-runs `just laya-deploy`, the warm identity classify and Task 3.

## Next Phase Readiness

- **ROADMAP and STATE stay at 15/18,** with a blocker, because this plan is halted.
- **The artifact, bucket object, config and resolver proof are all in place,** so a resume is one recipe change plus lifting the containment.

---
*Phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server*
*Completed: 2026-09-27 (halted at deploy-refused)*

## Self-Check: PASSED

- Files exist: 08-LIVE-DEPLOY-EVIDENCE.json, contracts/decide-tool-boundary-v1.yaml (2.0.0), crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template (memory_mb 3008).
- Commits exist: 968d73e99, b30f437da, 371c6bfe7.
- Adapted Task 2 verify: rc 0, with no bucket prefix, no account id, reserved=0, no source diff after the amendment, and an empty shared root. Task 3 verify: "Task 3 skipped on deploy-refused".
