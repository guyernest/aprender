# Phase 9: Internal OpenAI-Compatible LLM Endpoint for pmcp.run - Research

**Researched:** 2026-10-15
**Domain:** `apr serve` (realizar) OpenAI conformance fixes as upstream PRs; a private, authenticated default-Lambda deployment of a release `apr` binary + Lambda Web Adapter; one pmcp.run `LLMProvider` row and a pmcp-agent one-liner, delivered as PRs in foreign repos
**Confidence:** HIGH on everything read from source this session (every code claim carries a path:line); MEDIUM on AWS ingress behaviour (official docs, not exercised); LOW only where marked `[ASSUMED]`

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Model and release pin
- **D-01: Smallest model first** — the endpoint serves Qwen3.5-0.8B Q4_K_M, the ladder-pinned GGUF
  (`contracts/model-capability-ladder-v1.yaml`, sha256
  `bd258782e35f7f458f8aced1adc053e6e92e89bc735ba3be89d38a06121dc517`, `unsloth/Qwen3.5-0.8B-GGUF`).
  The record states plainly that 0.8B is CRUX-uncertified (2B is the smallest certified) and loads
  with the #4030 warning, and that it emitted correct tool calls in every spike-029 run. 2B/4B are
  a later phase, not a stretch goal here. (user, 2026-10-15)
- **D-02: Release artifacts, not a fork build** — the deployment of record runs an upstream release
  `apr` binary at ONE pinned tag, sha256-verified against the release's sha256 file; never the fork's
  `main` (at 0.63, no Qwen3.5). The pin is the newest upstream release that passes the spike-029
  conformance probe: the plan re-probes v0.71.4 (latest, 2026-10-14) before assuming v0.70.3 (the
  measured pin). A PR-branch build (upstream main + the fix PR) may back a pre-merge conformance
  proof only, labelled `interim` in every record, never the deployed endpoint. (idea requirement,
  2026-10-15; verified 2026-10-15 that v0.71.4 still has `content` REQUIRED and no AuthGate on the
  GGUF router)

#### Server conformance fixes (upstream PRs to paiml/aprender)
- **D-03: The null-content fix, server side** — `ChatMessage.content` in
  `crates/aprender-serve/src/api/mod_create_demo.rs` becomes `Option<String>` with `#[serde(default)]`
  on deserialize (an assistant message with `content` absent or `null` is accepted and rendered as
  empty), serialization unchanged (byte-identical responses, the PMAT-801 invariant). Test: the exact
  pmcp-agent turn-2 body from the spike-029 wire log deserializes and the loop completes. (user,
  2026-10-15; the PMAT-801 "follow-up" the code comment promises)
- **D-04: Bearer auth on the GGUF router** — `AuthGate::from_env()` is wired into the GGUF
  `create_router` path (`crates/apr-cli/src/commands/serve/routes.rs`, today `create_router(state)`
  with no gate) the way `build_apr_cpu_router` already is, so `APR_API_KEY` / `APR_API_KEY_HASH`
  (`contracts/apr-serve-api-key-auth-v1.yaml`) are enforced on every route the platform calls, with
  `/health` behaviour declared, and a test that a GGUF server with the key set answers 401 without it.
  This is a precondition for ANY HTTPS ingress (D-12).
- **D-05: Remaining 029 gaps are fixed where cheap, else recorded** — `tool_choice: required`
  honoured, `max_completion_tokens` aliased to `max_tokens`, `model` compared to the loaded id,
  the error envelope shaped `{"error": {"message","type","code"}}`, `tool_choice: none` not leaking
  `<tool_call>` XML, `cpu_part` surfaced in `/health`. Each fix has a test; one that is not a small
  change is carried as a KNOWN LIMITATION in the conformance record rather than blocking the phase.
  None of these may gate the end-to-end turn.
- **D-06: Small upstream PRs, one per fix** — cut from `upstream/main`, each self-contained with its
  test, per the maintainer's 2026-09-29 guidance (small PRs, ~90-minute CI). Opening each PR is a
  human checkpoint (it publishes under the user's GitHub identity). The fork branch carries the same
  commits so the deployment can be re-pinned the day a release ships them.
- **D-07: A conformance contract declared before the fixes** — a `pv`-validated contract under
  `contracts/` states the two client request shapes (durable-agent-lambda `openai_v1`, pmcp-agent
  `OpenAiCompatSource`), the required response fields and every 029 gap as a falsification test;
  the spike-029 `probe_openai.py` is its harness, run against the local pin and the deployed endpoint,
  and its verdict table is the phase's conformance record. (Phase 5/8 claims-gate philosophy)

#### pmcp.run integration (collaboration with the pmcp.run dev team)
- **D-08: The LLMProvider row is data, not code** — pmcp.run's data-driven `LLMProvider` model
  (`amplify/data/resource.ts`, read by `config/loader.rs::map_provider_row_to_config`) already
  exists; the internal endpoint is one row: `transformerId: openai_v1`, `endpointTemplate: <the
  deployed HTTPS URL>/v1/chat/completions` (no `{model}` token), `authHeaderName: Authorization`,
  `authHeaderPrefix: "Bearer "`, `secretKeyName: <name>` with the key value ONLY in Secrets Manager
  (R2-10 / T-107-01), `enabled: true`, `availableBuiltInTools: []`. No new transformer, no provider
  branch in `map_provider_config`. (user; pmcp.run R2-10)
- **D-09: The null-content fix, SDK side too** — pmcp-agent's `message_to_openai`
  (`rust-mcp-sdk/crates/pmcp-agent/src/sources/openai_compat.rs`) writes `content: ""` on assistant
  tool-use turns, as the platform's `openai_v1` transformer already does. Both ends are fixed so
  neither client depends on which release the endpoint runs. (user)
- **D-10: A per-provider timeout is a pmcp.run change** — `LLM_HTTP_TIMEOUT_SECS = 120` is a single
  const in `durable-agent-lambda/src/handler/iteration.rs` stamped on every provider; the internal
  endpoint's two-tool turn is 71 s on Graviton3 and a cold first turn adds ~40 s, a ten-tool turn 100 s
  wall. Proposal to the pmcp.run team: an optional `timeoutSecs` on the `LLMProvider` row, stamped by
  the loader when present, bounded by the agent Lambda's 15-minute timeout, with the staleness
  invariant test extended. Until it lands, the end-to-end demo turn is kept under 120 s (0.8B, two
  tools, a pre-warmed function) and the record says so.
- **D-11: Collaboration through a brief and PRs, never direct commits** — the aprender side produces
  `09-PMCP-RUN-BRIEF.md` (exact row values, request/response fixtures from the spike-029 wire logs,
  the conformance table, the timeout proposal, the PR list); pmcp.run and rust-mcp-sdk changes are
  made on feature branches in those repos and opened as PRs for the pmcp.run dev team to review; those
  plans are `autonomous: false` with a review checkpoint. The aprender phase never blocks on a
  pmcp.run merge: the `openai_v1` path works on today's code, and the SDK path is proven with the
  spike-029 `agent-loop` driver pointed at the PR branch. (user, 2026-10-15)

#### Deployment (decider pattern)
- **D-12: Private and authenticated ingress or none** — the platform's client speaks HTTPS with a
  Bearer header and cannot SigV4-sign, so the endpoint needs an HTTPS URL; server-side Bearer auth
  (D-04) proven with a 401 is a precondition for exposing one, and a Lambda Function URL with
  `AuthType: NONE` before that proof is forbidden (spike 031: no public surface while the GGUF server
  has no auth). The ingress mechanism is a research question judged against turns of 71–146 s, the
  Bearer-only client and streaming: candidates are a Function URL behind server auth, a CloudFront
  front door (OAC + header check) and an ALB; API Gateway is ruled out by its 30 s cap. The spike's
  `lambda:Invoke` with an API-GW v2 event stays the bench path.
- **D-13: One image, weights from S3, default Lambda** — the spike-031 Dockerfile and entrypoint
  (release `apr` + `public.ecr.aws/awsguru/aws-lambda-adapter:0.9.1`, `MODEL_S3_URI` pulled to
  `/tmp` at start) become repo assets built by `just` recipes: arm64 image package, 10,240 MB
  (6 vCPU), ephemeral 10,240 MB, 900 s, `RAYON_NUM_THREADS=6`, weights staged HF → CodeBuild → S3
  (never over the laptop uplink), everything tagged, a teardown recipe. The recipes refuse an unpinned
  binary, a GGUF whose sha256 is not the ladder pin, or a missing `APR_API_KEY_HASH`. The justfile is
  the deployment surface (`just laya-*` precedent); the Makefile stays the quality surface.
  — **Reversibility:** costly — the image, bucket and function names become the operator's runbook.
- **D-14: Measure on the deployed function, record cpu_part** — every log line and the live record
  carry `cpu_part` (Graviton2 vs Graviton3 is a 2–3× lottery); cold start to first answer, seconds
  and dollars per two-tool turn, the ten-tool prefill, turn-2 session reuse and the 2-at-once
  behaviour are re-measured on the deployed function with the spike-031 `remote_bench.py` shapes;
  spike numbers are context, never the measurement; wall time and billed duration are both reported.
- **D-15: Live deploy is a human go-no-go checkpoint** — like Phase 8's D-18: AWS account and
  profile, region, auth posture, cost ceiling and the pin are confirmed by the user before any AWS
  resource is created; every offline-provable task (contract, server fixes with tests, image build,
  recipes dry-run, local probe) comes first; the spike's leftover functions, image and weights under
  `spike=031` are reused or torn down by that decision, never silently.

### Claude's Discretion
- Where the deployment assets live in-repo (a `deploy/llm-endpoint/` directory or a crate mirroring
  `crates/aprender-mcp-decide-lambda/`), file and recipe names, the contract's file name, test names.
- How the conformance probe is packaged (Python under `uv` as in the spike, or a Rust test target) —
  but its verdict table must be reproducible from one command.
- Which of the D-05 gaps count as "cheap"; the ordering of upstream PRs.

### Deferred Ideas (OUT OF SCOPE)
- Prefix/KV cache keyed by token prefix with more than one entry, and a prefill-only request
  (`max_tokens: 0`) — the biggest lever measured (7–15 s per turn instead of 71–156 s); upstream
  performance work, GPU analogue #4214.
- A Q4_K GEMM batched CPU prefill (spike 020's 18× on F32) — upstream.
- pmcp.run: pin and render tool schemas to a stable prefix; route one conversation per process; queue
  per conversation in the front door — depends on the prefix cache.
- Qwen3.5-2B (the smallest CRUX-certified model) and 4B; Fargate for steady traffic; LMI (rejected for
  one model); GPU on `g5g` with the arm64 CUDA release binary.
- Bedrock's OpenAI surface as a provider row — data-only on pmcp.run, not aprender work.
- The Fargate scale-to-zero MCP-Task front door (spike 022) for the LLM endpoint.
</user_constraints>

<phase_requirements>
## Phase Requirements

`phase_req_ids` is null — no REQ-IDs in `.planning/REQUIREMENTS.md` (that document is the SetFit milestone's). The binding requirements are the six `internal-llm-endpoint` idea requirements (`.planning/spikes/MANIFEST.md:112-141`) and the five Success Criteria (`.planning/ROADMAP.md:753-800`). Each is mapped to the research that enables it.

| ID | Description (source) | Research Support |
|----|----------------------|------------------|
| IR-1 | The contract is pmcp.run's clients, not the OpenAI spec in the abstract (MANIFEST.md) | Q2/Q5: both client shapes read from source with line numbers; wire-log fixtures; Q4 contract shape |
| IR-2 | Upstream release artifacts at the cited tag + ladder-certified GGUF, not a fork build (MANIFEST.md) | Q3: v0.71.4 asset names/sizes, what changed v0.70.3→v0.71.4 (host-RAM guard, `response_format` now acted on); ladder sha256 |
| IR-3 | Bedrock is a provider, not a host (MANIFEST.md) | Out of scope here; no aprender work (CONTEXT deferred) |
| IR-4 | The internal endpoint is the option when security or cost outrank speed (MANIFEST.md) | Q1 ingress choice (Function URL, buffered, private-by-auth); Q6 cost model and reserved-concurrency option |
| IR-5 | Deploy with the decider pattern (MANIFEST.md) | Q6: every spike-031 asset mapped to a fail-closed `just` recipe mirroring `laya-deploy`; evidence JSON shape |
| IR-6 | pmcp.run can pin and pre-cache tool schemas (MANIFEST.md) | Deferred by CONTEXT (prefix cache); recorded only as the live-record caveat (one conversation per function) |
| SC-1 | `openai_v1` agent completes a two-tool turn by configuration alone | Q5: LLMModel + LLMProvider rows, loader flow, secret path/JSON key, what `openai_v1` sends/reads; `stream: None` so buffered ingress suffices |
| SC-2 | pmcp-agent `OpenAiCompatSource` turn completes; server `Option<String>` PR + SDK `content: ""` PR | Q2 (D-03 exact edit + test pattern), Q5 (`message_to_openai` edit + test), agent-loop path-dependency mechanics |
| SC-3 | Private + authenticated: GGUF router enforces the key; 401 through the ingress; probe reports every gap | Q2 (D-04 is in `server.rs::serve_router`, NOT `routes.rs`), Q1 (401-before-URL sequence), Q4 (probe as harness) |
| SC-4 | Reproducible deploy: pinned tag, sha256 GGUF, `just` recipes that refuse; live record with cpu_part etc. | Q6 (recipes, refusals, bashrs findings, asset location), Q7 (gates) |
| SC-5 | Every gate green; pmcp.run change set delivered as PRs + brief | Q3 (PR mechanics, hygiene gates), Q7 (Validation Architecture) |
</phase_requirements>

## Summary

The orchestrator's ground truth needs one correction before anything is planned: **the GGUF `/v1/chat/completions` router is not `crates/apr-cli/src/commands/serve/routes.rs::create_router`**. That function (`routes.rs:341-343`) builds the SafeTensors inspection router (`/predict`, `/generate`, `/transcribe`) and already has an auth-taking twin, `create_router_with_auth` (`routes.rs:350-372`). The GGUF server — Qwen3.5 included — is built in `crates/apr-cli/src/commands/serve/server.rs::serve_router` (`server.rs:279-290`): `let app = create_router_with_config(state, config.router_config());` from realizar's `api/router.rs:320`, with **no** `AuthGate`. `start_gguf_server` (`handler_gpu_completion.rs:366`) routes a hybrid to `start_qwen35_server` (`server.rs:111`), which calls `serve_router`. So D-04 is a two-line change in `server.rs`: wrap the realizar router with `super::auth::layer(super::auth::AuthGate::from_env(), app)` (the generic layer at `auth.rs:217-225`), plus a `build_gguf_router(state, config, gate)` seam so a test can drive it with `AuthGate::from_plain_key(..)` and `tower::ServiceExt::oneshot`, exactly as `auth.rs:302-360` already does for the SetFit exemptions. The same omission exists at upstream `main` (2f3da5112, 2026-10-15) and at tags v0.70.3 and v0.71.4.

D-03 is one field: `mod_create_demo.rs:218 pub content: String` → `Option<String>` with `#[serde(default)]`; the call sites that read `content` as a `String` must be adjusted and the response side keeps writing `Some(String::new())`/`""` (serialize unchanged — note that a `None` must serialize as `""` or be skipped to keep PMAT-801 byte-identity; recommend `#[serde(default, deserialize_with = ...)]` that maps `null`→`""` OR `Option<String>` + `serialize_with` emitting `""` — see Pattern 2). The parse-level test pattern to mirror is `qwen35_chat_backend_tests.rs:772` (`serde_json::from_value::<ChatCompletionRequest>`), which needs no model. Of the D-05 gaps, exactly two are small: the `max_completion_tokens` alias (`#[serde(alias = "max_completion_tokens")]` on `max_tokens`, `mod_create_demo.rs:70-71`) and, with medium effort, `tool_choice: none` (drop the tools from the rendered prompt). The error envelope is **not** small: `{"error": "<string>"}` is a contracted shape (`contract_post_error_envelope_preservation!` in `routes.rs:321`; FALSIFY-AUTH-001 asserts `body["error"] == "unauthorized"`; Ollama handlers read `error` as a string at `ollama_handlers.rs:576`), so changing it is a breaking contract edit — carry it as a KNOWN LIMITATION. `tool_choice: required` needs constrained decoding to be honest; `model` checking would make the LLMProvider row's `modelId` load-bearing against a server that answers `/v1/models` with `default` — both recorded, not fixed. On the real wire pmcp-agent sends neither `tool_choice` nor `temperature` (wire log: only `max_tokens: 384`), and the platform sends `stream: None`, so none of the recorded gaps touches the end-to-end turn.

Ingress: pick a **Lambda Function URL, `InvokeMode: BUFFERED`, `AuthType: NONE` with the two-statement resource policy, created only after a deployed 401 proof via the spike's `lambda:Invoke` bench path**. Neither platform client streams (`iteration.rs:2421 stream: None`; pmcp-agent is non-streaming), the function timeout is the Lambda quota (900 s), and the durable-agent Lambda has no `vpc` property (`amplify/modules/agent.ts:196-204`) so it egresses to the public internet and can reach the URL. CloudFront is ruled out by its origin response timeout (30 s default, above the quota only by Service Quotas increase, POST not retried) — a 71–146 s buffered turn never fits; an ALB is a buffered 1 MB-limited Lambda target that needs a VPC and hourly cost for nothing the server does not already do (Bearer check); API Gateway HTTP API's 30 s cap is the reason the decide contract prices itself the way it does. The Lambda Web Adapter 0.9.1 treats any status 100–499 on `/health` as ready, so gating `/health` with `layer` (the whole surface, as FALSIFY-AUTH-001 requires) does not break readiness.

**Primary recommendation:** Plan in this order — (1) contract + probe packaging, (2) D-03/D-04/`max_completion_tokens` as three one-commit PR branches off `upstream/main` with parse-level and `oneshot` tests, (3) local 401 + probe against the v0.71.4 release binary in the image, (4) recipes that refuse unpinned inputs and `DRY_RUN=1`, (5) the D-15 go/no-go, (6) deploy, prove 401 through `lambda:Invoke`, only then create the Function URL, (7) the two platform rows + SDK PR + brief, (8) measure and write the live record.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Accept/reject an assistant message with no `content` | API / Backend (aprender-serve `ChatMessage` deserializer) | Client (pmcp-agent writes `""`) | D-03 + D-09: both ends so neither depends on the other's release |
| Bearer authentication | API / Backend (apr-cli `auth::layer` on the realizar router) | — (no front-door auth; CloudFront/ALB add nothing) | `AuthGate` already exists; the gap is wiring in `server.rs::serve_router` |
| HTTPS ingress, TLS, long requests | CDN / Static tier is NOT used; Lambda Function URL (AWS-managed TLS) | — | Buffered 900 s invocation; neither client streams |
| Model weights | Database / Storage (S3, content-pinned) | Lambda `/tmp` (10,240 MB ephemeral) | Decider pattern: never baked into the image |
| Provider selection, key lookup, timeout | Frontend Server (pmcp.run durable-agent-lambda, DynamoDB rows + Secrets Manager) | Admin UI (creates the rows) | D-08/D-10: data rows, one loader change for the timeout |
| Conformance verdict | Test harness (stdlib Python probe) + Rust parse tests | `just` recipe + committed verdict table | D-07: one command reproduces the table |
| Live measurement (cold start, $/turn, cpu_part) | Deployment scripts (`lambda:Invoke` with `LogType=Tail`) | CloudWatch `[entry]` log lines | D-14: REPORT billed vs client wall, both |

## Project Constraints (from CLAUDE.md)

| Directive | Where it bites this phase |
|-----------|---------------------------|
| **Verification Discipline #1** — never read `$?` through a pipe; `rc=$?` after redirect, `PIPESTATUS` in bash | every recipe and the probe wrapper; the Bash tool here runs zsh (memory: no `PIPESTATUS`), so recipes are `#!/usr/bin/env bash` |
| **#2** — prove the mechanism engaged, never label by intent | `cpu_part` from `/proc/cpuinfo` in every log line (entrypoint already does); the 401 proof is a recorded HTTP status, not a config flag; `apr --version` printed at image build and at entry |
| **#3 / Step 0** — pin the binary; never a bare `apr`, never a hardcoded path on the dev box | in the image the binary is the sha256-verified release tarball; `scripts/check_apr_bin_pinned.sh` scans workflows, Makefile and `scripts/*.sh` only (`check_apr_bin_pinned.sh:87-91`), so an entrypoint under `deploy/` is unscanned — keep it out of `scripts/`, document the pin in the file header |
| **#6** — one failing input is an anecdote | re-measure turns with nonces (spike shapes already do) and more than one sample per shape |
| **#8** — a shadowed artifact is worse than a missing one | `pv` is NOT on PATH on this box; the justfile falls back to `cargo run --release -q -p aprender-contracts-cli --bin pv --` (`justfile:3442`) — reuse that resolution, never a bare `pv` |
| **Contract Validation: `pv`, never bash** | the conformance contract is validated by `pv validate`; the probe is its harness, not a replacement |
| **Shell Scripts: bashrs, `set -euo pipefail`, no `ls`, quoted vars** | the five spike-031 scripts carry 5 bashrs **errors** today (table in Q6); a sourced library must be option-neutral (`scripts/check_sourced_libs_option_neutral.sh`) |
| **justfile for project scripts** (user global CLAUDE.md) + "the justfile is the deployment surface" (D-13) | deploy/teardown/measure recipes join `laya-*`; the Makefile stays the quality surface |
| **Git workflow**: `main` protected; feature branch + PR | upstream PRs from `upstream/main`; fork branch `gsd/phase-2-contract-gate` continues (ROADMAP branch base) |
| **Realizar-first table** — serving is realizar's | `apr serve` on a GGUF *is* realizar (`realizar::api::create_router_with_config`); no exception row is needed; **confirmed** — the only apr-cli code touched is the router wiring and the auth layer, which already lives in apr-cli |
| **Code search policy: `pmat query`** | note for executors; this research used `grep` only to cite line numbers |
| **Publishing safety** — root-anchored ignores | the extracted `apr` binary and any GGUF under the asset dir need root-anchored `.gitignore` lines (`/deploy/llm-endpoint/bin/`), never `bin/` |
| **unwrap() banned** (`.clippy.toml`) | server fixes use `expect`/`?`; tests may `#![allow(clippy::unwrap_used)]` as `falsify_auth_001.rs:15` does |
| **CI reality** | upstream `workspace-test` is `cargo nextest run --profile ci --workspace --lib --exclude aprender-gpu --exclude aprender-cuda-edge --exclude aprender-compute`; contract gates run from `ci/explicit-test-commands.d/` (250/260 cover the auth contract) |

## Answers to the Planner's Questions

### Q1 — Ingress (D-12)

**Facts about the clients** `[VERIFIED: read this session]`
- durable-agent-lambda builds every invocation with `stream: None` (`~/Development/mcp/sdk/pmcp-run/amplify/functions/durable-agent-lambda/src/handler/iteration.rs:2421`), so the `openai_v1` request carries no `stream` key (`llm/transformers/openai.rs:42-44` only sets it when `Some`). It reads `choices.0.message`, `tool_calls`, `usage`, `finish_reason`/`stop_reason`, `refusal` (`openai.rs:70-118`).
- pmcp-agent `OpenAiCompatSource` is non-streaming, 60 s default timeout, HTTPS off-loopback unless `allow_insecure_http` (`rust-mcp-sdk/crates/pmcp-agent/src/sources/openai_compat.rs:60, :86`).
- The durable-agent Lambda is `NODEJS_24_X`, arm64, `timeout: Duration.minutes(15)`, `memorySize: 1024`, and the `lambda.Function` props at `amplify/modules/agent.ts:196-204` carry **no `vpc`/`vpcSubnets`/`securityGroups`** (grep over `agent.ts` finds no `vpc`) → default Lambda networking, public egress, **a Function URL is reachable** `[VERIFIED: agent.ts:196-250]`.
- Per-request HTTP timeout is `.timeout(Duration::from_secs(config.timeout))` at `llm/service.rs:242` (the shared client also has `.timeout(120)` at `:114`); `config.timeout` is stamped from the const at `config/loader.rs:965`.

**Candidates judged**

| Candidate | Verdict | Why (sources) |
|-----------|---------|---------------|
| **Lambda Function URL, `AuthType: NONE`, `InvokeMode: BUFFERED`** | **PICK FIRST** | invocation bound only by the function timeout (900 s quota) `[CITED: docs.aws.amazon.com/lambda/latest/dg/gettingstarted-limits.html]`; buffered responses ≤ 6 MB — a chat completion is kilobytes; NONE skips IAM but **requires a resource policy allowing BOTH `lambda:InvokeFunctionUrl` and `lambda:InvokeFunction`** (October 2025 rule), with `lambda:FunctionUrlAuthType = NONE` and `lambda:InvokedViaFunctionUrl = true` conditions; CLI/CFN/API callers must add the two `add-permission` statements themselves; deleting the URL does **not** delete the policy `[CITED: docs.aws.amazon.com/lambda/latest/dg/urls-auth.html]`. Bearer is checked by the server (D-04). LWA 0.9.1 supports Function URLs and `AWS_LWA_INVOKE_MODE=buffered` (the image's current setting, `deploy/Dockerfile`) `[CITED: raw.githubusercontent.com/awslabs/aws-lambda-web-adapter/v0.9.1/README.md]` |
| Function URL `RESPONSE_STREAM` + `AWS_LWA_INVOKE_MODE=response_stream` | not needed now; keep as a documented switch | neither client streams; streaming mode is sticky per URL and caps bandwidth at 2 MB/s after 6 MB `[CITED: docs.aws.amazon.com/lambda/latest/dg/configuration-response-streaming.html]` |
| CloudFront (OAC → Function URL, header check) | **ruled out** | origin *response timeout* = wait for first byte AND between packets, default 30 s; above the account quota only via Service Quotas increase (CDK accepts 1–180 only with an approved increase); on POST CloudFront drops the connection and does not retry `[CITED: docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/DownloadDistValuesOrigin.html]`. A buffered 71–146 s turn, or a streamed 10-tool prefill (TTFT ~100 s on Lambda-0.8B, spike 031), cannot fit without a quota increase, and CloudFront adds no auth the server does not already do |
| ALB with a Lambda target | ruled out | synchronous invoke that waits for the function (1 MB request/response limit, no WebSockets) `[CITED: docs.aws.amazon.com/elasticloadbalancing/latest/application/lambda-functions.html]`; needs a VPC, subnets and an hourly charge; no streaming `[ASSUMED]` |
| API Gateway HTTP API | ruled out | "an API Gateway HTTP API whose 30 s integration timeout cannot be raised" `[VERIFIED: contracts/decide-tool-boundary-v1.yaml:22-24]` |

**What may not be exposed before what (the sequence the plan must encode)**
1. Local: image built from the pinned tarball; `docker run` with `APR_API_KEY_HASH`; `tools/auth_probe.sh` pattern (bash, not zsh — `${h:+-H "$h"}` collapses in zsh, spike 029) shows `no header → 401`, `wrong key → 401`, `right key → 200`, and `/health` behaviour as declared.
2. Deployed, **no URL yet**: `lambda:Invoke` with the spike's API-GW v2 event (`remote_bench.py:27-32`) WITHOUT the `authorization` header → `statusCode 401` in the payload; WITH it → 200. This is the "server 401 proven" gate of D-12.
3. Only then `create-function-url-config --auth-type NONE --invoke-mode BUFFERED` + the two `add-permission` statements; then the probe over HTTPS without a key → 401, with → the full table.
4. Teardown deletes the URL **and** the two policy statements (`remove-permission`), because deleting a URL leaves the policy behind.

**Cost-amplification note for D-15:** a NONE URL lets anyone who knows it trigger a cold start that is 401'd after ~40 s of a 10,240 MB environment (≈ $0.005 each). Mitigations to offer the user at the checkpoint: reserved concurrency 1 on the function (also enforces "one conversation per process"; the platform's Class-2 path retries a 429 with durable waits, `iteration.rs:74-85`), a CloudWatch alarm on invocation count, and keeping the URL out of every committed file. Phase 8 accepted a comparable open-function risk explicitly (`08-LIVE-DEPLOY-EVIDENCE.json` `auth.risk_acceptance`).

### Q2 — Server fixes (D-03..D-05)

**Where the GGUF router really is** `[VERIFIED: read this session]`
- `crates/apr-cli/src/commands/serve/handler_gpu_completion.rs:366` `fn start_gguf_server` → `:395-397`: `if realizar::gguf::hybrid_forward_handles(arch) { return start_qwen35_server(mapped_model, config); }`
- `crates/apr-cli/src/commands/serve/server.rs:111-117` `fn start_qwen35_server` → `build_qwen35_state` (`:122-200`) → `serve_router(state, config)`.
- `server.rs:279-290` `fn serve_router`: `let endpoints = realizar::api::advertised_routes(&config.router_config(), &state); let app = create_router_with_config(state, config.router_config());` — **no auth layer**. Non-hybrid GGUFs reach the same function through `run_cpu_server` (`server.rs:67-98`).
- `routes.rs:341-343` `create_router(state)` = `create_router_with_auth(state, AuthGate::disabled())`; `:350-372` builds `/`, `/health`, `/metrics`, `/predict`, `/generate`, `/transcribe` and ends `super::auth::layer(auth_gate, router)`. This is the SafeTensors inspection router, not the chat router.
- Other realizar-router call sites without a gate (GPU paths, out of scope but the same one-liner): `handlers_include_01.rs:86, :197, :283`, `handler_gpu_completion.rs:725`. At `upstream/main` (2f3da5112): `server.rs:363, :444, :563` — same omission `[VERIFIED: git show upstream/main]`.

**The auth layer** `[VERIFIED: crates/apr-cli/src/commands/serve/auth.rs]`
- `AuthGate::from_env()` `:49-70`: `APR_API_KEY_HASH` (64 hex) wins; else `APR_API_KEY` plaintext; neither → disabled + one stderr WARNING.
- `apply` `:142-168`: 401 + `{"error":"unauthorized","message":"Missing or invalid Authorization: Bearer <key> header"}` + `WWW-Authenticate: Bearer`.
- `layer<S>(gate, router)` `:217-225` gates everything; `layer_public_ops` `:193-201` exempts `OPTIONS` and `/health*` (the SetFit server's WR-07 fix, `:177-192`).
- **`/health` decision (D-04 "declared")**: use `layer` (gate all). FALSIFY-AUTH-001 says "Health/metrics endpoints are NOT exempt" (`contracts/apr-serve-api-key-auth-v1.yaml:103-104`), and LWA 0.9.1 readiness counts any status 100–499 as ready (`AWS_LWA_READINESS_CHECK_MIN_UNHEALTHY_STATUS` default 500) `[CITED: LWA v0.9.1 README]`, so a 401 on `/health` still flips the adapter to ready. State this in the contract and verify it on the first deploy (the `[entry]` log shows the adapter proxying after `apr` prints "Model ready"). If a k8s-style probe is ever needed, `layer_public_ops` is the one-word alternative.
- Contract drift to report upstream, not fix here: the contract's `references` and description name a `--auth-disabled` CLI flag (`apr-serve-api-key-auth-v1.yaml:34-36, :54-55`); no such flag exists in `crates/apr-cli/src/commands/serve/` (only the comment at `routes.rs:339`).

**Exact edits**

| Fix | Edit | Test to add (pattern to mirror) | Size |
|-----|------|-------------------------------|------|
| D-04 Bearer on GGUF | `server.rs::serve_router`: factor `fn build_gguf_router(state: AppState, config: &ServerConfig, gate: AuthGate) -> axum::Router { super::auth::layer(gate, create_router_with_config(state, config.router_config())) }`; `serve_router` calls it with `AuthGate::from_env()` | apr-cli unit test: build the router with a model-free `AppState` (`realizar::api::test_helpers` / the `create_test_app_shared()` fixture used by `aprender-serve/src/api/tests/openai_chat.rs:4`) wrapped by `AuthGate::from_plain_key("k")`, `oneshot` `POST /v1/chat/completions` and `GET /health` with no header → 401 + `error == "unauthorized"`; with `Bearer k` → not 401. Mirror `auth.rs:302-360` (`public_ops` module) and `tests/falsify_auth_001.rs:28-70`. Keep the existing `build_apr_cpu_router(state, AuthGate::disabled())` test shape (`handlers.rs:1723, :1755`) | small (2 lines + seam + 1 test) |
| D-03 `content` optional | `mod_create_demo.rs:218` `pub content: String` → `#[serde(default, deserialize_with = "deserialize_null_as_empty")] pub content: String` **(preferred: keeps every `content: String` call site and byte-identical serialization)** or `Option<String>` with a `serialize_with` that writes `""` for `None`. Update the PMAT-801 comment `:210-217` | parse test beside `qwen35_chat_backend_tests.rs:772` pattern: `serde_json::from_value::<ChatCompletionRequest>` over the exact request #3 body from `results/wire-0.8b-plain.jsonl` (assistant message `{"role":"assistant","tool_calls":[…]}` with no `content`) and over `content: null`; assert `content == ""`; plus a serialization round-trip asserting the response bytes are unchanged | small (1 attribute + helper + 2 tests) |
| `max_completion_tokens` | `mod_create_demo.rs:70-71`: `#[serde(default, alias = "max_completion_tokens")] pub max_tokens: Option<usize>` | parse test: `{"max_completion_tokens": 12}` → `max_tokens == Some(12)`; both present → document precedence (serde: last wins, or refuse) | small |
| `tool_choice: none` leaks `<tool_call>` | render: when `request_tool_choice(..) == Some(ToolChoice::None)` pass `tools = None` into the prompt render (`qwen35_chat_backend.rs:109` builds `prompt_text` via `format_chat_messages_official…`; tools reach the render at `:226-227`) | parse-level test on the render function with tools + `none` → prompt contains no tool schema | medium — needs the render signature; carry as KNOWN LIMITATION if the render path is shared with `apr chat` |
| `tool_choice: required` | `OpenAiToolChoice::to_grammar` already maps `"required"` → `ToolChoice::Required` (`mod_create_demo.rs:291-301`; enum at `grammar/tool.rs:44-54`) but nothing forces a call; honest enforcement = constrained decoding (v0.71.x has `constrain::ConstraintRequest` for `response_format`) | — | **not small → KNOWN LIMITATION**; not on the real wire (pmcp-agent sends no `tool_choice`) |
| error envelope `{"error":{…}}` | would change `router.rs:364` (fallback), `:426` (panic), `:524-525` (`ErrorResponse { error }` in `sanitize_json_rejection`), `auth.rs:161-164`, the `contract_post_error_envelope_preservation!` macro (`routes.rs:321`), FALSIFY-AUTH-001's `body["error"].as_str()` and Ollama's string read (`ollama_handlers.rs:576`) | — | **not small → KNOWN LIMITATION** (pmcp-agent classifies by status; the platform reads `choices` only) |
| `model` compared to loaded id | server answers `/v1/models` with `default` (`openai_handlers.rs:268, :371`); a refusal would make `LLMModel.modelId` load-bearing | — | **KNOWN LIMITATION**; if upstream wants it, propose opt-in `APR_SERVE_STRICT_MODEL=1` accepting `default` or the file stem |
| `cpu_part` in `/health` | `HealthResponse` is built by `build_health_response` (`router.rs:703-712`); adding a Linux-only `/proc/cpuinfo` read is small but platform-specific | — | **record via the entrypoint log** (`[entry] host: … cpu_part=…` already) and the live record; do not upstream |

**Existing test patterns, with paths**
- Request deserialization without a model: `crates/aprender-serve/src/api/qwen35_chat_backend_tests.rs:769-806` (`chat_template_kwargs_parse_and_refuse_unknown_keys_3723`).
- Tool-calling unit tests without a model: `crates/aprender-serve/src/api/openai_handlers.rs:1287-1340` (`pmat801_tool_calling_tests`).
- Router-level auth tests: `crates/apr-cli/tests/falsify_auth_001.rs` (tower `oneshot`, `apr_cli::serve_auth::{layer, AuthGate}`), `auth.rs:302-360`.
- HTTP-level router tests (axum-test `TestServer`): `crates/apr-cli/src/commands/serve/tests_e2e_http.rs:1-50` — but it drives `routes::create_router`, the SafeTensors router; not the GGUF path.
- Contract-to-test binding: `scripts/check_contract_test_binding.sh` runs `pv lint contracts/ --strict-test-binding` (PV-VER-002), resolves `falsification_tests[].test` as cargo substring filters, ratchets against `scripts/contract_test_binding_baseline.txt`, refuses to pass under `MIN_REFS=250` `[VERIFIED: scripts/check_contract_test_binding.sh:1-80]`.

**What changed v0.70.3 → v0.71.4 that matters** `[VERIFIED: git diff/log on fetched tags]`
- 50 commits; in `aprender-serve/src/api` only `mod_create_demo.rs` (+160/−5) and `openai_handlers.rs` (+195) changed; `router.rs`, `qwen35_session.rs`, `apr-cli/.../server.rs` unchanged.
- `ChatCompletionRequest` gains `response_format: Option<ResponseFormat>` (`text` | `json_object` | `json_schema{…}`; **"A `type` apr does not know is refused at deserialization"**), `logprobs`, `top_logprobs`; `Timings` gains `load_ms`, `first_token_ms`. The probe's P8 sends `{"type":"text"}` → still accepted; a client sending `json_object` now gets constrained decoding instead of being ignored — neither platform client sends it.
- `fbe921e04` "Qwen3.5 host-RAM guard" (#4947/#4992): the host build is **refused** when `need = 2.75 × file_bytes + f32 token_embd bytes > 90 % × MemAvailable` (bounded by the cgroup `memory.max`; no measurement at all → refused). For the 0.8B Q4_K_M (532 MB file) that is ≈ 1.46 GB + token_embd (vocab 151,936 × hidden 1,024 × 4 B ≈ 0.62 GB `[ASSUMED: dims from training memory]`) ≈ 2.1 GB against a 10,240 MB cgroup → admitted; **the re-probe on v0.71.4 (D-02) must include a cold start on the deployed tier because this guard did not exist at the measured pin.**
- Nothing in the range mentions #4030 in a commit subject (the warning is a load-time message, unchanged).

### Q3 — Upstream PR mechanics (D-06)

- Remotes: `origin` = guyernest/aprender, `upstream` = paiml/aprender; `upstream/main` fetched at 2f3da5112 (2026-10-15) `[VERIFIED: git remote -v; git log -1 upstream/main]`.
- Precedent: `#4634` is **CLOSED** (draft, 100 commits, head `gsd/phase-2-contract-gate`) — the maintainer asked for slices; `#4941` is the OPEN slice (draft, **1 commit**, base `main`, head `laya-decide-library`), its body structured as *What is in / What is not in (later pieces) / Known gap / Verification (on the committed tree)*; `#5235` is a second open slice `[VERIFIED: gh pr list/view]`. Mirror `#4941`: one squashed commit per PR, body in that shape, base `main`.
- Branches: `git worktree add ../aprender-pr-content upstream/main` (keeps this tree on `gsd/phase-2-contract-gate`); commit; push to `origin`; `gh pr create --repo paiml/aprender --base main --draft` — **human checkpoint** (publishes under the user's identity). Cherry-pick the same commits onto the fork branch so the deployment can be re-pinned when a release ships them.
- CI expectations: fork PRs sit `action_required` until a paiml maintainer approves workflows (08-32 evidence); `workspace-test` ≈ 90 min; contract hygiene gates that bit Phase 8 (08-33-SUMMARY): `formal_prose` and `contracts_without_valid_under` shrink-only ratchets, `pv lint contracts` (0 errors), a regenerated contract graph, `pv extract contracts --check`, `check_contract_test_binding.sh`; `make tier1` = `cargo fmt --check` + `cargo clippy -- -W clippy::all` + `cargo check`; `tier2` = `cargo test --lib` + `cargo clippy -- -D warnings` (`Makefile:238-250`). A new contract must start at zero binding findings (`check_contract_test_binding.sh` baseline rule) and carry `metadata.kind` + `valid_under` (08-33: every kernel-kind contract carries `valid_under: {world: committed}`).
- Release assets at **v0.71.4** (2026-10-14T18:17:52Z): `apr-v0.71.4-aarch64-unknown-linux-gnu-cpu.tar.gz` (25,051,867 B) + `apr-v0.71.4-aarch64-unknown-linux-gnu-cpu.tar.gz.sha256` (115 B); also `-cuda.tar.gz`, `aarch64-apple-darwin-cpu`, `x86_64-…` and `pv-v0.71.4-aarch64-unknown-linux-{gnu,musl}.tar.gz` `[VERIFIED: gh release view v0.71.4 --json assets]`. The sha256 file sits beside each tarball, as the spike's `build_image.sh` assumes (`shasum -a 256 -c <tar>.sha256`).
- Re-pinning later: the recipes read `APR_TAG` + the expected tarball sha256 from one pinned file (e.g. `deploy/llm-endpoint/pin.env`); moving the pin is a one-line `pv diff`-free edit plus a re-run of the probe and the live record.

### Q4 — Conformance contract (D-07)

- Kinds the parser accepts: `ContractKind` at `crates/aprender-contracts/src/schema/kind.rs:54` (`kernel`, `registry`, `kaizen`, `pattern`, `model-family`, …) `[VERIFIED: kind.rs:54-81]`. Two live shapes to copy:
  - `kind: registry` + `falsification_conditions[]{id, description, test_file, test_name, status}` + `qa_gate{run}` — `contracts/apr-serve-api-key-auth-v1.yaml` (the auth contract; its CI binding is `ci/explicit-test-commands.d/250-…` and `260-…`).
  - `kind: kernel` + `valid_under: {world: committed}` + `constants / served_fields / untrusted_input_bounds / equations / proof_obligations / falsification_tests[]{id, rule, prediction, implemented_by, test, if_fails} / kani_harnesses / qa_gate` — `contracts/decide-tool-boundary-v1.yaml:1-30, :283, :348, :401, :459, :560, :592-660, :670` `[VERIFIED]`. The `test:` value is a `cargo test …` invocation string (e.g. `:598`).
- **Recommendation:** `contracts/llm-endpoint-openai-conformance-v1.yaml`, `kind: kernel`, `valid_under: {world: committed}`, with `constants` (the two client shapes as literal field lists; the pin tag; the ladder sha256), `untrusted_input_bounds`-style table of request fields each client sends, and one `falsification_tests` row per 029 gap. Bind each row to a **Rust test** so PV-VER-002 resolves (`check_contract_test_binding.sh` only understands cargo filters): the D-03/D-04/alias rows bind to the new unit tests; the probe rows bind to a Rust test that reads the committed verdict JSON (`results/probe-<pin>.json`) and asserts each gap's expected verdict (`✓` or the string `KNOWN LIMITATION`). Rows a Python harness proves live carry the `just` recipe in `implemented_by` (Phase 8's training rows did this) and the Rust reader in `test:`.
- Probe packaging: keep `tools/probe_openai.py` **stdlib-only under `python3 -I`** (it already is — `probe_openai.py:12-17` imports `argparse, json, sys, time, urllib`), invoked by `just llm-endpoint-probe <base> <model> <key> <out>`; no `uv`, no PyPI. Its 15-row table (`results/probe-0.8b.md`) is the record; add the `KNOWN LIMITATION` verdict as a third symbol beside `✓/✗/ℹ` so the deployed table can be green with limitations stated. The 029 cases: `GET /models`, `GET /effective-config`, chat plain, tools auto → tool_call, tool result → final answer, `tool_choice=required|none`, 10 tools, stream+tools, stream plain, extra fields (P8: `response_format {"type":"text"}` — still accepted on v0.71.4), `max_completion_tokens` (P9), error envelope (P10: `messages: "not-a-list"`), foreign model id (P11), no Authorization header (P12) `[VERIFIED: probe_openai.py:159-300]`.
- `pv validate` runs through the justfile fallback (`cargo run --release -q -p aprender-contracts-cli --bin pv --`) because `pv` is not on PATH here.

### Q5 — pmcp.run side (D-08..D-11)

**Rows and lookup flow** `[VERIFIED: pmcp-run a27d46b8e]`
- `LLMProvider` (`amplify/data/resource.ts:1450-1470`): `id` (the provider string key), `displayName`, `transformerId`, `endpointTemplate` (stored verbatim, `{model}` resolved only in `service.rs::resolve_endpoint` `:35-44` — no token → returned unchanged), `authHeaderName`, `authHeaderPrefix` (optional), `secretKeyName` (**key NAME only**), `enabled` (default true), `availableBuiltInTools`.
- `LLMModel` (`resource.ts:1414-1437`): `provider: a.string()` (free-form, **not an enum**), `modelId` (the string sent as `"model"`), `displayName`, `maxTokens`, `contextWindow`, `supportsTools`, `supportsBuiltInTools`, `isActive`; GSIs on `provider`, `modelId`.
- `AgentConfig.modelId` references `LLMModel.id` (`resource.ts:1066`), with `temperature` (default 0.7, not sent by `openai_v1`), `maxTokens` (default **4096**, sent as `max_tokens` — `iteration.rs:2419`), `maxIterations`.
- Loader (`config/loader.rs:423-495`): GetItem `LLMModel` by `AgentConfig.modelId` → `provider` + `modelId`; if `LLM_PROVIDERS_TABLE` is set (it is: `agent.ts:214-216`), GetItem `LLMProvider` by `id = provider` → `map_provider_row_to_config` (`:919-968`): `enabled == false` → hard error; `secret_path = "pmcp/orgs/{org}/agents/llm-keys"` when the agent row has `organizationId`, else `"/ai-agent/llm-secrets/prod"` (`:949-951`, `:468-470`); `timeout = LLM_HTTP_TIMEOUT_SECS` (`:965`). Row absent → hardcoded seed `map_provider_config` (anthropic/openai/google; `:804-860`).
- Secret: `SecretsManager::get_api_key(secret_path, key_name)` (`llm/secrets.rs:54-130`) — the secret is **one JSON object**; the value under `secretKeyName` is the Bearer token; cached 5 min. So the operator adds a new key (e.g. `"aprender_internal": "<token>"`) to the org's existing `pmcp/orgs/{id}/agents/llm-keys` secret — no new secret, no code.
- Creating rows: admin page `components/agents/llm-provider-management.tsx` (form requires `secretKeyName`, `:98-99`) through `hooks/use-llm-providers.ts::createProvider` (`:55-62`, `client.models.LLMProvider.create`); models through `hooks/use-llm-models.ts`; an agent's model is picked in the agent config UI. **Three data actions, zero code:** (1) `LLMProvider{id:"aprender-internal", transformerId:"openai_v1", endpointTemplate:"https://<function-url>/v1/chat/completions", authHeaderName:"Authorization", authHeaderPrefix:"Bearer ", secretKeyName:"aprender_internal", enabled:true, availableBuiltInTools:[]}`; (2) `LLMModel{provider:"aprender-internal", modelId:"Qwen3.5-0.8B-Q4_K_M", maxTokens: 512, supportsTools:true, isActive:true}`; (3) set the demo agent's `modelId` to that row; switching back is editing (3).
- `openai_v1` request (`llm/transformers/openai.rs:21-60`): `model`, `messages`, `max_tokens`, `top_p`, `stream` (each only when `Some`), `tools` (`{type:function, function:{name, description, parameters}}`); **no `temperature`** (`:33-35`). Assistant tool-use turn: `content: ""` when there is no text (`:239-243`), then `tool_calls` with `arguments` as a string (`:246-256`). Response: `choices.0.message` (fallback `results.0`), `tool_calls`, `usage`, `finish_reason` | `stop_reason`, `message.refusal` (`:70-118`).
- **Timeout** (D-10): `pub(crate) const LLM_HTTP_TIMEOUT_SECS: u64 = 120;` `iteration.rs:62`, checkpoint comment `:42-61`; `LLM_MAX_CLASS1_ATTEMPTS = 2` `:67`, `LLM_CLASS1_BASE_BACKOFF_SECS = 2` `:72`; the guards are `test_staleness_invariant_class1_worst_case` (`:2946-2967`: `120×2 + 2×1 = 242 < 300`) and `test_staleness_invariant_class2_single_step` (`:2969-2984`: `120 < 300`). **Smallest change:** read an optional `timeoutSecs` number from the provider row in `map_provider_row_to_config` and stamp `pc.timeout`; the reqwest per-request `.timeout(config.timeout)` at `service.rs:242` then governs the call (the shared client's `:114` 120 s is overridden by the per-request timeout `[ASSUMED: reqwest semantics]`). The invariant bounds the proposal: with two Class-1 attempts the worst case is `2×T + 2 < 300` → **`T ≤ 148 s`**; a cold 40 s + 146 s ten-tool turn (186 s) breaks it, so the proposal must either (a) clamp `timeoutSecs ≤ 148` and keep two attempts, or (b) let the row also set `maxClass1Attempts = 1` (then `T < 298`), or (c) raise `TeamLimits.max_sub_task_staleness_seconds` for teams using the provider. Put all three in the brief; recommend (a) for the demo (two-tool warm turn 71 s fits) and (b) as the follow-up. Extend the two tests with the row-bound maximum.
- `AgentConfig.maxTokens` default 4096 reaches the server as `max_tokens: 4096`; at 6.4 tok/s a runaway answer is 640 s — under the 900 s function timeout but far over 120 s. Set the demo agent's `maxTokens` to 256–512 and say so in the brief.

**pmcp-agent** `[VERIFIED: rust-mcp-sdk d0348451, pmcp-agent 0.4.0]`
- `message_to_openai` (`crates/pmcp-agent/src/sources/openai_compat.rs:167-200`): the `ToolUse` arm (`:172-183`) emits `{"role":"assistant","tool_calls":[…]}` with no `content`. **Edit:** add `"content": ""` to that `json!`. **Test:** extend `tool_use_message_becomes_tool_calls` (`:364-382`) with `assert_eq!(v["content"], "")`, and add a request-level assertion in `request_hoists_system_and_sets_fields` (`:353-362`) if a tool-use history message is added to the fixture.
- Real-wire shape (from `results/wire-0.8b-fixed.jsonl`): only `max_tokens: 384`; **no `tool_choice`, no `temperature`** on the `AgentEngine` path — `with_tool_choice` appears only in the module's own tests `[VERIFIED: grep]`.
- Running the spike driver against a PR branch: `agent-loop/Cargo.toml` depends by **path** on `../../../../../../mcp/sdk/rust-mcp-sdk/crates/pmcp-agent` (features `openai-compat`), `pmcp-package`, `pmcp` (`agent-loop/Cargo.toml:14-16`); it builds with `CARGO_TARGET_DIR=../../../../target`. A path dependency follows the working tree, so check the SDK PR branch out in `~/Development/mcp/sdk/rust-mcp-sdk` (or point the path at a `git worktree` of that branch) and rebuild. The driver raises the 60 s timeout to `--timeout-s` (default 900) and sets `allow_insecure_http: true` (`agent-loop/src/main.rs:46-56`); against the HTTPS URL set `allow_insecure_http: false`.

### Q6 — Deployment (D-13..D-15)

**Spike-031 assets, what each does, what must change** `[VERIFIED: read this session]`

| Asset | Does today | Becomes (fail-closed) |
|-------|-----------|------------------------|
| `deploy/Dockerfile` | `FROM public.ecr.aws/awsguru/aws-lambda-adapter:0.9.1 AS lwa`; AL2023-minimal + `awscli-2 procps-ng`; `COPY apr /usr/local/bin/apr`; `RUN … apr --version`; env `AWS_LWA_PORT=8080 AWS_LWA_READINESS_CHECK_PATH=/health AWS_LWA_READINESS_CHECK_PROTOCOL=http AWS_LWA_ASYNC_INIT=true AWS_LWA_INVOKE_MODE=buffered PORT=8080 MODEL_LOCAL=/tmp/model.gguf RAYON_NUM_THREADS=6 HOME=/tmp` | unchanged except: pin the LWA image by digest as well as tag; `ARG APR_TAG` + `LABEL` with tag and tarball sha256; no secrets in the image (`APR_API_KEY_HASH` is a function env var) |
| `deploy/entrypoint.sh` | `set -uo pipefail`; logs `[entry] t=…ms` timeline, `cpu_part` from `/proc/cpuinfo`, mem/cgroup, env; pulls `MODEL_S3_URI` → `MODEL_LOCAL` unless present; `exec /usr/local/bin/apr serve run "$MODEL_LOCAL" --host 0.0.0.0 --port "$PORT" --backend cpu ${APR_SERVE_EXTRA:-}` | add: refuse to start when `APR_API_KEY_HASH` is unset/non-64-hex (**the server would start open with only a warning**, `auth.rs:66-69`); verify the pulled GGUF's sha256 against `MODEL_SHA256` (env) before exec, or use the S3 object's `sha256` metadata the CodeBuild job writes; keep `exec` (LWA needs the server as the process); bashrs: 0 errors / 9 warnings today (DET004 on `nproc` etc.) |
| `tools/build_image.sh` | `shasum -a 256 -c <tar>.sha256`; `tar xzf … --strip-components=1 <tag>/apr`; ECR login; `docker buildx build --platform linux/arm64 --provenance=false -t $ECR:cpu-v0.70.3 --push deploy` | `just llm-endpoint-image`: download the tarball + `.sha256` from the release (`gh release download <tag> --repo paiml/aprender -p 'apr-<tag>-aarch64-unknown-linux-gnu-cpu.tar.gz*'`), refuse on mismatch, tag `cpu-<tag>`, record the image digest; bashrs 0 errors / 3 warnings (SEC014 tar path) |
| `tools/fetch_weights_codebuild.sh` | CodeBuild ARM project from `--cli-input-json` (multi-line buildspec), downloads from HF, checks the ladder sha256 (0.8B `bd258782…`, 2B `aaf42c8b…`, 4B `00fe7986…`), `aws s3 cp --metadata sha256=…` | `just llm-endpoint-weights`: 0.8B only; the sha256 is read from `contracts/model-capability-ladder-v1.yaml` by `pv`/yq-free Python (`-I`), never typed twice; **bashrs 2 errors** (SC1078 unclosed quotes at lines 14, 16 — the inline JSON policy) |
| `tools/aws_up.sh` | bucket `llm-spike-031-<acct>`, ECR `llm-spike-031`, roles (`-lambda`: basic + VPC exec + S3 read; `-ecs-*`; `-lmi-operator`), SG, log groups (7-day), ECS cluster, subnets; writes `results/aws-ids.json` | `just llm-endpoint-up`: Lambda role + bucket + ECR + log group only (no ECS/LMI/SG); names `aprender-llm-endpoint-<env>`; tags `project=aprender-llm-endpoint phase=09`; **bashrs 2 errors** (SC1035 at lines 44, 49) |
| `tools/lambda_up.py` | boto3 `create_function(PackageType=Image, Architectures=[arm64], MemorySize=10240, Timeout=900, EphemeralStorage=10240, Environment={MODEL_S3_URI, RAYON_NUM_THREADS, PLATFORM}, Tags={spike:031})`; function name `llm-spike-031-lambda-0-8b` (no `.` allowed) | `just llm-endpoint-deploy`: aws-cli (`aws lambda create-function --package-type Image …`) so no PyPI dependency; refuse unless: tarball sha256 matches the pin file, the S3 object's `sha256` metadata equals the ladder pin, `APR_API_KEY_HASH` is 64 hex, the image digest matches the one `llm-endpoint-image` recorded; `DRY_RUN=1` stops before any AWS write (as `laya-deploy`, `justfile:1796-1900`) |
| `tools/remote_bench.py` | `lambda:Invoke` with an API-GW v2 event (`authorization: Bearer spike-031` header), `LogType=Tail`, parses `REPORT` (Duration, Init Duration, Max Memory, Billed), cost = `mem/1024 × billed_ms/1000 × 0.0000133334`; shapes from `shapes.py` (`shape_short`, `shape_tools2`, `shape_tools10`, `shape_turn2`, nonces) | `just llm-endpoint-measure`: keep as the bench path (D-12) but drop boto3 (`aws lambda invoke --log-type Tail` returns the same base64 `LogResult`) or keep boto3 behind the SUS checkpoint; add the **401 proof case** (no header) and the HTTPS probe case; write `09-LIVE-DEPLOY-EVIDENCE.json` |
| `tools/teardown.sh` | deletes LMI/Lambda functions, ECS, ECR, bucket (unless `KEEP_BUCKET=1`), SG, log groups, CodeBuild, roles | `just llm-endpoint-teardown`: + `delete-function-url-config` + `remove-permission` for both statements; **bashrs 1 error** (SC2105 `break` outside loop at line 33) |
| `results/aws-ids.json` | account `816069129666`, region us-east-1, profile `ze-kasher-dev`, bucket `llm-spike-031-816069129666`, ECR `…/llm-spike-031`, vpc, sg, 4 subnets, 4 role ARNs | not reused; the phase writes its own ids file under the asset dir (gitignored) |

**What spike 031 left in the account** (README "Left running"; `aws-ids.json`): the two Lambda functions `llm-spike-031-lambda-0-8b` and `-2b` (no idle cost), the ECR image `llm-spike-031:cpu-v0.70.3`, the S3 weights (three GGUFs, cents/month), four IAM roles, the SG, log groups, the ECS cluster, the CodeBuild project; LMI torn down in-session. The 0.8B GGUF already in `s3://llm-spike-031-816069129666/Qwen3.5-0.8B-Q4_K_M.gguf` with `sha256` metadata can be **copied server-side** into the phase bucket (no CodeBuild run needed) — a D-15 choice. The AWS profile resolves today: `ze-kasher-dev` → account 816069129666, `arn:aws:iam::816069129666:user/Guy` `[VERIFIED: aws sts get-caller-identity]`.

**Phase 8 precedents to mirror** `[VERIFIED: justfile, 08-*-EVIDENCE.json]`
- `laya-deploy` (`justfile:1796`) refusal ladder, each `REFUSED <check>: …` before any AWS call: config present → no placeholder → sha-pin equals the file → IAM statements exact → resolver proof for the installed tool version → eligibility (`just laya-verify`) → S3 object size → env bucket; `DRY_RUN=1` exits `DRY-RUN OK` after building. `laya-build-bootstrap` (`:1759`) proves identity: output newer than a stamp, aarch64 ELF, names the crate. `laya-teardown` (`:2742`), `laya-deploy-verify` (`:2599`), `laya-weights-bucket` (`:2354`), `laya-upload` (`:2384`).
- Live record keys (`08-LIVE-DEPLOY-EVIDENCE.json`): `outcome`, `decided_by`, `decided_by_who`, `decided_on`, `option`, `auth{enabled, provider, mechanism, risk_acceptance}`, `memory_decision`, `function_configuration{memory_size_mb, timeout_s, architecture, runtime, ephemeral_storage_mb, sha256_pin_equals_artifact, log_group}`, `readiness{A10,…}`, `precondition_gates`, `cold_samples`, `warm`, `identity_matches`, `teardown_commands_not_run`, `recommendations_not_acted_on`. The hold shape (`08-DEPLOY-EVIDENCE.json`): `outcome: hold`, `option: hold-no-aws`, `readiness_open[]`. The D-18 checkpoint text is `08-CONTEXT.md:107-111`.
- Identity for this phase = `apr --version` line in the `[entry]` log (tag + git sha, e.g. `apr 0.70.3 (95f64ba14)` in the spike), the image digest, the GGUF sha256 — all three in the record.

**Where the assets live (discretion):** a plain directory, **`deploy/llm-endpoint/`**, not a crate. Reasons: no Rust binary is built (release `apr` + LWA), so FALSIFY-MONO-011's `deployment_unit_bins` register (`crates/aprender-core/tests/monorepo_invariants.rs:312-322`, which requires a CONTEXT decision to extend) is untouched; `crates/<dir>` without a manifest inflates the "dirs under crates/" confusion CLAUDE.md already documents. The root `deploy/` is the cargo-pmcp CDK project (`deploy/bin/app.ts`, `deploy/lib/stack.ts`, `cdk.json` with `"app": "npx ts-node --prefer-ts-exts bin/app.ts"`, `tsconfig.json` excluding only `node_modules`/`cdk.out`) — a sibling subdirectory with a Dockerfile, two `.sh` files and a README does not enter the TypeScript build (no `.ts` files) `[VERIFIED: deploy/cdk.json, deploy/tsconfig.json]`. Add root-anchored ignores `/deploy/llm-endpoint/bin/` and `/deploy/llm-endpoint/.ids/`. Alternative if the planner prefers isolation from CDK: top-level `deployments/llm-endpoint/`.

### Q7 — Validation Architecture

See `## Validation Architecture` below (nyquist enabled; `.planning/config.json` `workflow.nyquist_validation: true`).

## Standard Stack

### Core (nothing new is installed in the Rust workspace)
| Library / tool | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `axum` + `tower::ServiceExt` | workspace (already deps of apr-cli / aprender-serve) | router + `oneshot` tests | the auth tests already use them (`falsify_auth_001.rs`) |
| `serde` `alias`/`default`/`deserialize_with` | workspace | D-03 and the `max_completion_tokens` alias | zero new deps |
| `subtle`, `sha2` | workspace | `AuthGate` | already wired (`auth.rs:11-12`) |
| Upstream release `apr` | **v0.71.4** (`apr-v0.71.4-aarch64-unknown-linux-gnu-cpu.tar.gz`, 25,051,867 B + `.sha256`) | the deployed binary | D-02; re-probe before pinning; v0.70.3 is the measured fallback |
| Lambda Web Adapter | `public.ecr.aws/awsguru/aws-lambda-adapter:0.9.1` | HTTP ↔ Lambda invoke | spike 031 ran it on three hosts |
| AWS CLI | 2.29.0 (installed) | all AWS writes from recipes | removes the PyPI dependency the spike had |
| `just` | 1.46.0 | deployment surface | D-13 / `laya-*` precedent |
| `bashrs` | 6.66.3 | lint every new script | CLAUDE.md |
| `pv` | built via `cargo run -p aprender-contracts-cli --bin pv --` (not on PATH) | contract validation | CLAUDE.md |
| `python3 -I` (stdlib) | 3.14.6 | the probe | `probe_openai.py` is stdlib-only |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `gh` | 2.64.0 | release download, PR creation | `gh release download v0.71.4 --repo paiml/aprender -p '…-cpu.tar.gz*'`; `gh pr create --draft` at the human checkpoint |
| `docker buildx` | 29.8.2 / buildx 0.37.2 | arm64 image | `--platform linux/arm64 --provenance=false --push` (spike) |
| `boto3` (PyPI) | 1.43.114 on the index | only if `remote_bench.py` is kept as-is | flagged SUS by the seam — see audit; prefer aws-cli |
| `cargo-zigbuild` | — | NOT needed | no Rust binary is cross-compiled in this phase |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Function URL (buffered) | Function URL `RESPONSE_STREAM` | needed only if a client streams; sticky per URL; 2 MB/s after 6 MB |
| aws-cli in recipes | boto3 drivers (spike) | boto3 is cleaner for waiters but adds a PyPI install the legitimacy seam flags; `aws lambda wait function-active-v2` covers the waiters |
| stdlib Python probe | Rust test target hitting a live server | Rust needs a running model (the realizar Qwen3.5 tests skip without `/home/noah/models/…`); the committed verdict JSON + a Rust reader gives `pv` binding without a model |

**Installation:** none for the workspace. Tooling present: see Environment Availability.

**Version verification:** `gh release view v0.71.4 --repo paiml/aprender --json assets` (done, 2026-10-15); `pip index versions boto3` → 1.43.114 (done).

## Package Legitimacy Audit

Run 2026-10-15 via `gsd-tools query package-legitimacy check --ecosystem pypi boto3 huggingface_hub`.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `boto3` | PyPI | latest release 2026-10-14 (the seam reads the **latest** release date, hence "too-new") | unknown to the seam | github.com/boto/boto3 | [SUS] (`too-new`, `unknown-downloads`) | Flagged — planner adds `checkpoint:human-verify` before any `uv run --with boto3`; **recommended: avoid entirely** (aws-cli covers create-function, invoke `--log-type Tail`, waiters) |
| `huggingface_hub` | PyPI | latest release 2026-10-08 | unknown | github.com/huggingface/huggingface_hub | [SUS] (same reasons) | Flagged — only needed for a laptop HF download, which D-13 forbids (weights stage via CodeBuild `curl`); **drop** |
| `public.ecr.aws/awsguru/aws-lambda-adapter:0.9.1` | ECR Public (container, not a package registry) | — | github.com/awslabs/aws-lambda-web-adapter | n/a (seam has no ecosystem) | Approved by precedent (spike 031 ran it); pin by digest in the Dockerfile `[ASSUMED: digest to be read at build]` |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** `boto3`, `huggingface_hub` — both well-known upstream projects; the seam's `too-new` signal is its heuristic on the newest release, but the protocol stands: gate behind `checkpoint:human-verify` or, better, do not install them.

## Architecture Patterns

### System Architecture Diagram

```
pmcp.run durable-agent-lambda (arm64, 15 min, no VPC)          pmcp-agent AgentEngine (spike agent-loop driver)
  AgentConfig.modelId ─► LLMModel{provider, modelId}            OpenAiCompatSource{base_url, Bearer, timeout}
        └─► LLMProvider{id=provider, openai_v1, endpointTemplate,        │
             Authorization / "Bearer ", secretKeyName}                   │
        └─► Secrets Manager pmcp/orgs/{org}/agents/llm-keys[secretKeyName]
        └─► openai_v1.transform_request → POST {model, messages, max_tokens, top_p, tools}
                      │ (buffered HTTPS, ≤ LLM_HTTP_TIMEOUT_SECS=120 s)      │ (buffered HTTPS, --timeout-s)
                      ▼                                                      ▼
            ┌──────── Lambda Function URL (AuthType NONE, InvokeMode BUFFERED, resource policy ×2) ────────┐
            │   Lambda function aprender-llm-endpoint-<env>: image, arm64, 10,240 MB, /tmp 10,240 MB, 900 s │
            │   env: MODEL_S3_URI, MODEL_SHA256, APR_API_KEY_HASH, RAYON_NUM_THREADS=6                       │
            │   ┌ Lambda Web Adapter 0.9.1 (readiness GET /health, any 100–499 = ready; ASYNC_INIT) ┐       │
            │   │  entrypoint.sh: [entry] cpu_part/mem → refuse if no APR_API_KEY_HASH → aws s3 cp  │       │
            │   │   → sha256 check → exec apr serve run /tmp/model.gguf --backend cpu                │       │
            │   │   apr-cli serve_router: auth::layer(AuthGate::from_env(), realizar router)   ◄── D-04     │
            │   │     401 {"error":"unauthorized"} ──────────────────────────────────────────► client       │
            │   │     200: realizar /v1/chat/completions → Qwen35Session (one state per process)   │       │
            │   │          ChatMessage.content absent/null accepted ◄── D-03                        │       │
            │   └────────────────────────────────────────────────────────────────────────────────────┘       │
            └───────────────────────────────── S3 s3://<bucket>/Qwen3.5-0.8B-Q4_K_M.gguf (sha256 metadata) ──┘
                                                       ▲ CodeBuild ARM: HF → sha256 == ladder pin → S3
Bench/record path (D-12/D-14): lambda:Invoke (API-GW v2 event, LogType=Tail) → REPORT billed/init/max-mem + client wall
```

### Recommended Project Structure
```
contracts/llm-endpoint-openai-conformance-v1.yaml     # D-07 (kind: kernel, valid_under committed)
deploy/llm-endpoint/
├── README.md                 # not part of the CDK app in deploy/; runbook; identity lines
├── Dockerfile                # from spike 031; ARG APR_TAG; LWA pinned
├── entrypoint.sh             # from spike 031 + APR_API_KEY_HASH refusal + GGUF sha256 check
├── pin.env                   # APR_TAG=v0.71.4; APR_TARBALL_SHA256=<from .sha256>; GGUF_SHA256 read from the ladder
├── buildspec-weights.yaml    # CodeBuild (multi-line, from --cli-input-json)
├── probe/probe_openai.py     # stdlib; from spike 029 (+ KNOWN LIMITATION verdict)
├── probe/shapes.py, bench.py # from spike 031 shapes/remote_bench (aws-cli or boto3 behind checkpoint)
├── bin/                      # gitignored: extracted apr, downloaded tarball
└── .ids/                     # gitignored: account/bucket/function/url ids
justfile                      # llm-endpoint-image | -weights | -up | -deploy (DRY_RUN) | -prove-401 | -url | -probe | -measure | -teardown
crates/apr-cli/src/commands/serve/server.rs            # D-04 (build_gguf_router + test)
crates/aprender-serve/src/api/mod_create_demo.rs        # D-03, max_completion_tokens alias (+ parse tests)
.planning/phases/09-*/09-PMCP-RUN-BRIEF.md, 09-LIVE-DEPLOY-EVIDENCE.json, 09-CONFORMANCE-<pin>.md
```

### Pattern 1: Gate the realizar router in apr-cli (D-04)
**What:** the auth layer is generic over the router state (`auth.rs:217-225`), so it wraps realizar's router exactly as it wraps `build_apr_cpu_router`.
**Example:**
```rust
// crates/apr-cli/src/commands/serve/server.rs — mirrors handlers.rs:1502 and routes.rs:371
#[cfg(feature = "inference")]
fn build_gguf_router(
    state: realizar::api::AppState,
    config: &ServerConfig,
    auth_gate: super::auth::AuthGate,
) -> axum::Router {
    let app = realizar::api::create_router_with_config(state, config.router_config());
    super::auth::layer(auth_gate, app) // every route, /health included (FALSIFY-AUTH-001)
}
// serve_router: let app = build_gguf_router(state, config, super::auth::AuthGate::from_env());
```
Test shape (`auth.rs:302-360` / `falsify_auth_001.rs:28-70`): `build_gguf_router(test_state, &ServerConfig::default(), AuthGate::from_plain_key("k")).oneshot(Request::post("/v1/chat/completions")…)` → `401`, `body["error"] == "unauthorized"`, `WWW-Authenticate: Bearer`.

### Pattern 2: Accept absent/null `content` without changing bytes out (D-03)
**What:** keep `content: String` for every reader; make the *deserializer* tolerant.
```rust
// crates/aprender-serve/src/api/mod_create_demo.rs:218
#[serde(default, deserialize_with = "deserialize_null_as_empty_string")]
pub content: String,

fn deserialize_null_as_empty_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}
```
Serialization is untouched (PMAT-801 byte-identity holds by construction). If the planner prefers the literal `Option<String>` wording of D-03, add `serialize_with` that writes `""` for `None` and touch every `content` reader — larger diff, same wire bytes.

### Pattern 3: Fail-closed `just` recipe (D-13), the `laya-deploy` shape
```bash
# justfile — refusals print `REFUSED <check>: …` before any AWS call; DRY_RUN=1 ends at DRY-RUN OK
llm-endpoint-deploy env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    . deploy/llm-endpoint/pin.env
    [ -f "deploy/llm-endpoint/bin/apr-${APR_TAG}-aarch64-unknown-linux-gnu-cpu.tar.gz" ] || refuse pin "tarball absent: run just llm-endpoint-image"
    (cd deploy/llm-endpoint/bin && shasum -a 256 -c "apr-${APR_TAG}-aarch64-unknown-linux-gnu-cpu.tar.gz.sha256" >/dev/null) || refuse sha-pin "release sha256 mismatch"
    LADDER_SHA="$(python3 -I - <<'PY'
    …read contracts/model-capability-ladder-v1.yaml, print the 0.8B Q4_K_M sha256…
    PY
    )"
    [ "$LADDER_SHA" = "bd258782e35f7f458f8aced1adc053e6e92e89bc735ba3be89d38a06121dc517" ] || refuse ladder "ladder pin moved; re-run spike-029 probe first"
    case "${APR_API_KEY_HASH:-}" in ????????????????????????????????????????????????????????????????) ;; *) refuse auth "APR_API_KEY_HASH must be 64 hex chars" ;; esac
    REMOTE_SHA="$(aws s3api head-object --profile "{{profile}}" --bucket "$BUCKET" --key "$KEY" --query 'Metadata.sha256' --output text)" || refuse s3 "object absent"
    [ "$REMOTE_SHA" = "$LADDER_SHA" ] || refuse s3-sha "S3 object sha256 $REMOTE_SHA != ladder $LADDER_SHA"
    [ "${DRY_RUN:-0}" = "1" ] && { echo "DRY-RUN OK (nothing deployed)"; exit 0; }
    aws lambda create-function … > "$LOG" 2>&1; rc=$?   # never $? through a pipe
```

### Pattern 4: 401 before URL (D-12)
```bash
# just llm-endpoint-prove-401: invoke through the bench path with NO authorization header
EVENT='{"version":"2.0","routeKey":"$default","rawPath":"/v1/chat/completions","headers":{"content-type":"application/json"},"requestContext":{"http":{"method":"POST","path":"/v1/chat/completions"}},"body":"{\"model\":\"x\",\"messages\":[{\"role\":\"user\",\"content\":\"Hi\"}],\"max_tokens\":1}","isBase64Encoded":false}'
aws lambda invoke --function-name "$FN" --payload "$EVENT" --cli-binary-format raw-in-base64-out /tmp/out.json > /tmp/invoke.log 2>&1; rc=$?
jq -e '.statusCode == 401' /tmp/out.json || refuse auth-proof "deployed function answered $(jq .statusCode /tmp/out.json) without a key"
# only now: aws lambda create-function-url-config --auth-type NONE --invoke-mode BUFFERED
#           aws lambda add-permission … --action lambda:InvokeFunctionUrl --principal '*' --function-url-auth-type NONE
#           aws lambda add-permission … --action lambda:InvokeFunction    --principal '*' --invoked-via-function-url
```

### Anti-Patterns to Avoid
- **Wiring the gate into `routes.rs::create_router`** — it is the SafeTensors router; the GGUF path never calls it. The CONTEXT wording is corrected above; the planner should cite `server.rs:279-290`.
- **Changing the error envelope "while there"** — it is contracted (`routes.rs:321` macro, FALSIFY-AUTH-001, Ollama string reads). Record, don't fix.
- **Creating the Function URL in the same recipe as the function** — the 401 proof must sit between them.
- **Baking the GGUF or the key into the image** — 021/031 measured 7–18× worse cold starts; the key is an env var hash.
- **Sharing one function between agents** — one `Qwen35Session` behind a `Mutex`; a foreign request resets the state (030/031).
- **Reading REPORT `Duration` as the client's wait** — they disagreed by up to 46 s (031); record both.
- **`uv run --with boto3` on a whim** — the legitimacy seam flags it; aws-cli does the job.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Bearer check | a header compare in `serve_router` | `super::auth::layer(AuthGate::from_env(), router)` | constant-time, contracted, tested (`auth.rs`) |
| HTTP ↔ Lambda bridging | a `bootstrap` crate | Lambda Web Adapter 0.9.1 extension | the OpenAI API is already the server's surface (031) |
| Weights transfer | laptop upload | CodeBuild ARM job → S3 with `sha256` metadata | the uplink dropped twice (031) |
| Contract validation | yq/bash checks | `pv validate` via the justfile fallback | CLAUDE.md |
| Verdict table | ad-hoc curl loops (zsh collapses `${h:+-H "$h"}`) | `probe_openai.py` under `python3 -I` + `auth_probe.sh` in bash | 029 |
| Secret storage on pmcp.run | a new secret or a DDB field | one JSON key in the org's existing `llm-keys` secret, read by `secrets.rs:54` | R2-10 |
| Timeout per provider | a provider branch in `map_provider_config` | optional row field stamped in `map_provider_row_to_config` | D-08/D-10 |

**Key insight:** every mechanism this phase needs already exists on one side of a seam; the work is wiring, pinning and proving, not building.

## Common Pitfalls

### Pitfall 1: Planning D-04 against the wrong file
**What goes wrong:** a task edits `routes.rs` and the GGUF server stays open.
**Why:** `create_router` in `routes.rs` is the SafeTensors inspection router; the chat router is realizar's, built in `server.rs::serve_router`.
**How to avoid:** cite `server.rs:279-290`; the test must POST `/v1/chat/completions` on the realizar router.
**Warning signs:** a passing test that never mounts `/v1/chat/completions`.

### Pitfall 2: The server starts open when the env var is missing
**What goes wrong:** `AuthGate::from_env()` with no `APR_API_KEY_HASH` → disabled + one stderr line (`auth.rs:66-69`); a Function URL then exposes an open LLM.
**How to avoid:** the entrypoint refuses to exec without a 64-hex `APR_API_KEY_HASH`; the deploy recipe refuses without it; the 401 proof runs before the URL exists.

### Pitfall 3: v0.71.4 is not v0.70.3
**What goes wrong:** the pin moves to v0.71.4 on the strength of 0.70.3 measurements; the new host-RAM guard (#4947/#4992) or `response_format` handling changes behaviour.
**How to avoid:** D-02's re-probe includes a cold start on the deployed tier and the full probe table; the record names the tag and git sha from `apr --version`.

### Pitfall 4: Staleness invariant on pmcp.run
**What goes wrong:** a per-provider timeout > 148 s with two Class-1 attempts breaks `test_staleness_invariant_class1_worst_case` (`iteration.rs:2946`).
**How to avoid:** the brief states the bound and the three options; the demo turn stays under 120 s (D-10).

### Pitfall 5: `AgentConfig.maxTokens` default 4096
**What goes wrong:** a runaway answer at 6.4 tok/s runs minutes past the 120 s client timeout; the Lambda keeps generating (billed) after the client gave up.
**How to avoid:** the demo agent's `maxTokens` 256–512; say so in the brief and the live record.

### Pitfall 6: Function URL policy outlives the URL
**What goes wrong:** teardown deletes the URL config; the public `lambda:InvokeFunction` statement with `InvokedViaFunctionUrl` stays.
**How to avoid:** teardown runs `remove-permission` for both statement ids; the record lists them.

### Pitfall 7: bashrs errors already in the spike scripts
**What goes wrong:** copying `aws_up.sh`, `teardown.sh`, `fetch_weights_codebuild.sh` as-is fails SC-5 (`bashrs lint` errors SC1035, SC2105, SC1078).
**How to avoid:** rewrite as `just` recipes with the JSON policies in files, loops with real bodies, and `mktemp` scratch files.

### Pitfall 8: The Bash tool runs zsh; recipes do not
**What goes wrong:** a guard case table in zsh goes vacuous (memory); `PIPESTATUS` is absent.
**How to avoid:** every recipe is `#!/usr/bin/env bash`; the probe wrapper records `rc=$?` after a redirect.

## Code Examples

### The pmcp-agent one-liner (D-09)
```rust
// rust-mcp-sdk/crates/pmcp-agent/src/sources/openai_compat.rs:172-183
SamplingMessageContent::ToolUse { id, name, input, .. } => json!({
    "role": "assistant",
    "content": "",            // OpenAI-compatible servers that keep `content` required (apr serve ≤ 0.71.4)
    "tool_calls": [{ "id": id, "type": "function",
                     "function": { "name": name, "arguments": input.to_string() } }],
}),
// test (:364-382): assert_eq!(v["content"], "");
```

### The row the loader expects (D-08), and the optional timeout (D-10)
```rust
// pmcp-run amplify/functions/durable-agent-lambda/src/config/loader.rs:919-968 (today) — proposal adds:
let timeout = item.get("timeoutSecs").and_then(|v| v.as_n().ok()).and_then(|n| n.parse::<u64>().ok())
    .map(|t| t.min(MAX_PROVIDER_TIMEOUT_SECS))                 // 148 with two Class-1 attempts
    .unwrap_or(crate::handler::iteration::LLM_HTTP_TIMEOUT_SECS);
```
```json
{ "id": "aprender-internal", "displayName": "aprender apr serve (Qwen3.5-0.8B, Lambda)",
  "transformerId": "openai_v1",
  "endpointTemplate": "https://<url-id>.lambda-url.us-east-1.on.aws/v1/chat/completions",
  "authHeaderName": "Authorization", "authHeaderPrefix": "Bearer ",
  "secretKeyName": "aprender_internal", "enabled": true, "availableBuiltInTools": [] }
```

### The exact failing request (fixture for D-03), from `results/wire-0.8b-plain.jsonl` request #3
```json
{"role":"assistant","tool_calls":[{"id":"call_0","type":"function","function":{"name":"get_weather","arguments":"{\"city\":\"Paris\",\"unit\":\"celsius\"}"}}]}
```
(no `content` key → 422 at v0.70.3/v0.71.4; `content: ""` → 200 and a grounded answer; 029 bisect).

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `apr serve` GGUF with no auth (0.70.3 … 0.71.4, upstream main) | `AuthGate` exists for `.apr` CPU, SafeTensors, SetFit routers only | HELIX-IDEA-009, 2026-05 | D-04 is wiring |
| `response_format` tolerated/ignored (0.70.3) | enum, unknown `type` refused, `json_*` constrains decoding (0.71.x, #3568 PR 4) | v0.71.0–0.71.4 | probe P8 still passes; document |
| Qwen3.5 host build straight into OOM | host-RAM admission guard (#4947/#4992) | v0.71.x | re-probe cold start on the tier |
| LWA `AWS_LWA_READINESS_CHECK_MIN_UNHEALTHY_STATUS` | `AWS_LWA_READINESS_CHECK_HEALTHY_STATUS` (default 100-499) | LWA 1.0 docs | 0.9.1 semantics identical for a 401 |
| Function URL NONE needed `InvokeFunctionUrl` only | needs `InvokeFunctionUrl` **and** `InvokeFunction` | October 2025 | two `add-permission` calls |

**Deprecated/outdated:** the auth contract's `--auth-disabled` flag reference (no such flag in apr-cli); `#4634` as the PR vehicle (closed; slices are the pattern).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Qwen3.5-0.8B token_embd is 151,936 × 1,024 f32 (≈0.62 GB), so the #4947 guard admits it at 10,240 MB | Q2 (v0.71.4 changes) | a refused build on the first deploy → the re-probe catches it before the URL |
| A2 | reqwest's per-request `.timeout()` overrides the client-level 120 s | Q5 timeout | the shared client at `service.rs:114` would also need the change |
| A3 | ALB Lambda targets do not support response streaming | Q1 | irrelevant unless streaming is required |
| A4 | The LWA 0.9.1 image digest is read at build time | Stack | tag-only pin until the first build |
| A5 | CodeBuild's `aws s3 cp --metadata sha256=…` is readable back as `Metadata.sha256` via `head-object` | Pattern 3 | fall back to re-hashing the object in the entrypoint (already planned) |

## Open Questions (RESOLVED)

All six were resolved by the plan set (plan-check revision 1, 2026-10-15); each keeps its original text with the plan/section that answers it.

1. **Will PV-VER-002 accept a `test:` that is a `just` recipe?**
   - What we know: the binding checker resolves cargo substring filters (`check_contract_test_binding.sh`); Phase 8 placed training-side rows in `laya_claims.tsv` kinds (`just`/`python`/`pv`/`evidence`).
   - Recommendation: bind every row to a Rust test (verdict-JSON reader for probe rows); put the recipe in `implemented_by`.
   - **RESOLVED → plan 09-01 Task 1:** no `just` recipe is ever a `test:` value. Probe rows (007-012) bind to the Rust reader `api::llm_endpoint_conformance_tests::conformance_record_matches_contract`; `just llm-endpoint-probe` is named in `implemented_by`; live rows 013-016 use the `LIVE-PENDING` marker; rows whose tests land later carry no `test:` key (staged binding, bound by 09-02 rows 001/003 and 09-03 row 002).
2. **Does upstream want strict `model` checking or a shaped error envelope at all?**
   - Recommendation: record both as KNOWN LIMITATIONS; raise as an issue, not a PR.
   - **RESOLVED → plan 09-01 Task 1 (rows 009/010 as KNOWN LIMITATION) and plan 09-03 Task 1 / its PR checkpoint ("raised as an issue, not fixed"); COVERAGE.md Surface 1 records both as OPT-OUT.** Neither is on the real wire of either pmcp.run client.
3. **`/health` gated vs exempt on the GGUF path** — contract says gated; SetFit server exempts. Recommendation: gated (`layer`), stated in the new contract, verified by the first deploy's `[entry]`/adapter timeline.
   - **RESOLVED → gated. Plan 09-03 Task 1** (`build_gguf_router` wraps the router with `auth::layer`, not `layer_public_ops`; test `gguf_router_refuses_without_bearer` asserts 401 on `GET /health`; the seam's doc comment states the LWA 0.9.1 readiness rule — any 100-499 counts as ready); **plan 09-01** equation `bearer_gate` and row 002 state it in the contract; **plan 09-07 Task 2** reads the first deploy's `[entry]`/adapter timeline.
4. **Reuse or tear down the spike-031 leftovers** (two functions, image, bucket with three GGUFs, roles) — a D-15 user decision; server-side copy of the 0.8B object is the cheap reuse.
   - **RESOLVED → plan 09-07 Task 1 (the D-15 checkpoint):** options `go-dev-copy-weights` (server-side copy, leftovers untouched), `go-dev-codebuild-weights`, `go-and-teardown-spike-031`; **plan 09-06 Task 2** `llm-endpoint-weights mode=copy` and the `TEARDOWN_SPIKE_031=1` guard with the `NOT RUN — the human's call` list. Nothing is reused or deleted silently.
5. **Reserved concurrency = 1** on the function (blast-radius cap + one-conversation-per-process) — offer at D-15; platform retries 429 via the Class-2 path.
   - **RESOLVED → offered at D-15: plan 09-07 Task 1** option `go-dev-copy-weights` names reserved concurrency 1; **plan 09-06 Task 1** `RESERVED_CONCURRENCY` in `llm-endpoint-deploy` (`put-function-concurrency` + read-back); **plan 09-10** threat T-09-35 records the 429/Class-2 consequence in `posture`.
6. **Where the pmcp.run org id comes from** for the secret path (`pmcp/orgs/{id}/agents/llm-keys`) — the demo agent's `organizationId`; the brief must name it.
   - **RESOLVED → plan 09-09 Task 2** (brief §4 "The secret" states the path and names the org-id question; §10 "Review questions" asks it) and **plan 09-10 Task 1** (the data-actions checkpoint's resume-signal requires the `organizationId` before Task 2 runs).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| aws-cli | recipes, 401 proof, URL | ✓ | 2.29.0 (x86_64 build on Darwin) | — |
| AWS profile `ze-kasher-dev` | all AWS writes | ✓ | account 816069129666, `user/Guy` | — |
| docker + buildx | arm64 image | ✓ | 29.8.2 / 0.37.2 | — |
| gh | release download, PRs | ✓ | 2.64.0 | curl against the release API |
| just | deployment surface | ✓ | 1.46.0 | — |
| bashrs | script lint | ✓ | 6.66.3 | — |
| pv | contract validation | ✗ on PATH | — | `cargo run --release -q -p aprender-contracts-cli --bin pv --` (`justfile:3442`) |
| cargo | tests, pv build | ✓ | 1.93.0 (pinned toolchain) | — |
| python3 | probe (`-I`, stdlib) | ✓ | 3.14.6 | — |
| uv | only if boto3 drivers are kept | ✓ | 0.9.5 | aws-cli |
| node | pmcp.run repo tooling (not needed for the brief) | ✓ | 24.19.0 | — |
| jq | recipes | ✓ | 1.7.1 | python3 -I |
| Upstream tags v0.70.3, v0.71.4 | diffs, pin | ✓ fetched locally | — | — |
| A local Qwen3.5-0.8B GGUF | local probe against the image | ✓ present in the HF cache (checked 2026-10-15 by plan 09-05: `~/.cache/huggingface/hub/models--unsloth--Qwen3.5-0.8B-GGUF/snapshots/6ab461498e2023f6e3c1baea90a8f0fe38ab64d0/Qwen3.5-0.8B-Q4_K_M.gguf`; the spike-029 `models/` symlink points at the same blob). Plan 09-05 Task 1 sha256-checks it against `ladder_pin.py` before any local server starts — that check, not this row, is the control | — | pull once via `hf download` (sha256 `bd258782…`) or run the local probe inside the image with the S3 copy |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** `pv` (cargo fallback already encoded in the justfile).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test / cargo nextest (Rust), `pv` (contracts), `bashrs` (scripts), stdlib Python probe (conformance), aws-cli invoke (live) |
| Config file | workspace `Cargo.toml` lints; `.pmat-gates.toml`; `scripts/contract_test_binding_baseline.txt` |
| Quick run command | `cargo test -p aprender-serve --lib api::` and `cargo test -p apr-cli --lib commands::serve` |
| Full suite command | `cargo nextest run --profile ci --workspace --lib --exclude aprender-gpu --exclude aprender-cuda-edge --exclude aprender-compute` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SC-2 / D-03 | assistant message with absent/null `content` deserializes to `""`; response bytes unchanged | unit | `cargo test -p aprender-serve --lib api::tests::<new>::assistant_message_without_content_parses_pmat801_followup` | ❌ Wave 0 |
| SC-3 / D-04 | GGUF router 401s without Bearer on `/v1/chat/completions` and `/health`; 200-path with the key | unit (tower oneshot) | `cargo test -p apr-cli --lib commands::serve::server::tests::gguf_router_refuses_without_bearer` | ❌ Wave 0 |
| D-05 alias | `max_completion_tokens` → `max_tokens` | unit | `cargo test -p aprender-serve --lib api::tests::<new>::max_completion_tokens_aliases_max_tokens` | ❌ Wave 0 |
| D-07 | contract validates; every row bound | contract | `cargo run --release -q -p aprender-contracts-cli --bin pv -- validate contracts/llm-endpoint-openai-conformance-v1.yaml` ; `bash scripts/check_contract_test_binding.sh` | ❌ Wave 0 |
| D-07 | probe table reproducible | integration (live server) | `just llm-endpoint-probe http://127.0.0.1:8029/v1 Qwen3.5-0.8B-Q4_K_M "$KEY" results/probe-local-v0.71.4.json` | ❌ Wave 0 (copy from spike) |
| D-07 | committed verdict JSON matches the contract's expected verdicts | unit (reader) | `cargo test -p aprender-serve --lib api::tests::<new>::conformance_record_matches_contract` | ❌ Wave 0 |
| SC-4 / D-13 | recipes refuse unpinned/unverified inputs; dry run | script self-test | `DRY_RUN=1 just llm-endpoint-deploy` ; mutation: wrong `APR_API_KEY_HASH`, wrong sha → `REFUSED` | ❌ Wave 0 |
| SC-5 | scripts lint clean | lint | `bashrs lint deploy/llm-endpoint/entrypoint.sh` (+ every new `.sh`) | ❌ Wave 0 |
| SC-5 | formatting/clippy on touched crates | lint | `cargo fmt --all -- --check` ; `cargo clippy -p aprender-serve --lib -- -D warnings` ; `cargo clippy -p apr-cli --lib -- -D warnings` | ✅ |
| SC-3 live | 401 through `lambda:Invoke` then through HTTPS | live, human-gated | `just llm-endpoint-prove-401` ; probe with no key over the URL | ❌ (D-15) |
| SC-1 | platform two-tool turn | live, pmcp.run team | brief + the agent run id, recorded in `09-LIVE-DEPLOY-EVIDENCE.json` | ❌ (D-11/D-15) |
| SC-2 | SDK two-tool turn | live | `cd …/agent-loop && cargo run --release -- --base https://<url>/v1 --model Qwen3.5-0.8B-Q4_K_M --out ../results/agent-run-<pin>.json` with the SDK PR branch checked out | ❌ (driver exists in the skill sources) |
| D-09 | SDK writes `content: ""` | unit (foreign repo) | `cargo test -p pmcp-agent --features openai-compat sources::openai_compat::tests::tool_use_message_becomes_tool_calls` | ✅ (assert to add) |
| D-10 | staleness invariant with the row bound | unit (foreign repo) | `cargo test -p durable-agent-lambda test_staleness_invariant` | ✅ (extend) |

### Sampling Rate
- **Per task commit:** the two quick commands + `cargo fmt --all -- --check` on the touched crate; `bashrs lint` on any touched script; `pv validate` on the touched contract.
- **Per wave merge:** `cargo nextest run --profile ci -p aprender-serve -p apr-cli --lib`; `bash scripts/check_contract_test_binding.sh`; `DRY_RUN=1 just llm-endpoint-deploy`.
- **Phase gate:** full workspace nextest command; `pv lint contracts`; every `ci/explicit-test-commands.d/250-*` and `260-*` fragment; the probe table against the deployed pin; `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `crates/aprender-serve/src/api/tests/<new>.rs` (registered in `api/tests/mod.rs`) — D-03 parse tests, alias test, verdict-JSON reader
- [ ] `crates/apr-cli/src/commands/serve/server.rs` test module — D-04 oneshot test (needs a model-free `AppState` fixture; `realizar::api::test_helpers` / `create_test_app_shared` pattern)
- [ ] `contracts/llm-endpoint-openai-conformance-v1.yaml` — D-07
- [ ] `deploy/llm-endpoint/{Dockerfile,entrypoint.sh,pin.env,probe/}` + justfile recipes — copied from the skill sources and hardened
- [ ] Framework install: none

## Security Domain

`security_enforcement: true`, ASVS level 1 (`.planning/config.json`).

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | single-key Bearer, SHA-256 + `subtle::ConstantTimeEq` (`auth.rs`), hash-only in the function env; the key value lives only in Secrets Manager on the platform (R2-10) |
| V3 Session Management | no (stateless HTTP) | — ; note the model-session state is per process, not per user |
| V4 Access Control | yes | Function URL resource policy (public, NONE) + server 401; teardown removes both statements; optional reserved concurrency 1 |
| V5 Input Validation | yes | serde-typed requests (unknown `response_format.type` refused; `ChatTemplateKwargs` `deny_unknown_fields`); realizar's `sanitize_json_rejection` keeps parser internals off the wire; `MAX_REQUEST_SIZE` on the apr-cli routers |
| V6 Cryptography | yes | sha256 of the release tarball and the GGUF (content pins); TLS by the Function URL; never hand-rolled |
| V8 Data Protection | yes | no request text logged by the entrypoint; `[entry]` logs only host facts; the key hash, never the key, in config |
| V14 Configuration | yes | pinned tag, pinned LWA image, root-anchored ignores for the extracted binary; the recipe refuses an open server |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Open LLM endpoint (no `APR_API_KEY_HASH`) | Elevation of privilege / cost | entrypoint + recipe refusals; 401 proof before the URL |
| Cost amplification via cold starts on a public URL | Denial of service (billing) | reserved concurrency, CloudWatch alarm, URL kept out of git; accepted explicitly at D-15 like Phase 8 |
| Key leak through DDB/admin UI | Information disclosure | `secretKeyName` is a name; value in Secrets Manager (`loader.rs:915-918`, `llm-provider-management.tsx:17-19`) |
| Timing attack on the key | Information disclosure | constant-time digest compare (FALSIFY-AUTH-003) |
| Prompt/tool-result content in logs | Information disclosure | entrypoint logs no bodies; apr `--verbose` off in the image |
| Supply chain: wrong binary in the image | Tampering | release `.sha256` check at build, `apr --version` at build and at entry recorded |
| Supply chain: PyPI drivers | Tampering | aws-cli instead of boto3; any PyPI use behind `checkpoint:human-verify` |

## Sources

### Primary (HIGH confidence — read this session)
- aprender tree `gsd/phase-2-contract-gate` (0.69.3 + upstream): `crates/aprender-serve/src/api/{mod_create_demo.rs, openai_handlers.rs, router.rs, qwen35_chat_backend.rs, qwen35_chat_backend_tests.rs, tests/openai_chat.rs}`, `crates/aprender-serve/src/grammar/tool.rs`, `crates/apr-cli/src/commands/serve/{server.rs, routes.rs, auth.rs, handlers.rs, handlers_include_01.rs, handler_gpu_completion.rs, server_runtime.rs, tests_e2e_http.rs}`, `crates/apr-cli/tests/falsify_auth_001.rs`, `contracts/apr-serve-api-key-auth-v1.yaml`, `contracts/decide-tool-boundary-v1.yaml`, `crates/aprender-contracts/src/schema/kind.rs`, `scripts/check_contract_test_binding.sh`, `scripts/check_apr_bin_pinned.sh`, `crates/aprender-core/tests/monorepo_invariants.rs`, `justfile` (laya recipes, pv fallback), `Makefile` tiers, `ci/explicit-test-commands.d/`, `deploy/{cdk.json,tsconfig.json}`, `.gitignore`
- git: tags v0.70.3 / v0.71.4 diffs and logs; `upstream/main` 2f3da5112; `gh release view v0.71.4`; `gh pr view 4634 / 4941`; `gh pr list`
- Spike skill sources: `.claude/skills/spike-findings-aprender/{references/*.md, sources/029-*/*, sources/031-*/*}`; `bashrs lint` run on the five 031 scripts
- pmcp-run `main` a27d46b8e: `amplify/data/resource.ts`, `amplify/modules/agent.ts`, `amplify/functions/durable-agent-lambda/src/{config/loader.rs, handler/iteration.rs, llm/service.rs, llm/secrets.rs, llm/models.rs, llm/transformers/openai.rs}`, `hooks/use-llm-providers.ts`, `components/agents/llm-provider-management.tsx`
- rust-mcp-sdk d0348451: `crates/pmcp-agent/src/sources/openai_compat.rs`, `crates/pmcp-agent/Cargo.toml`
- Phase 8: `08-CONTEXT.md` D-18, `08-LIVE-DEPLOY-EVIDENCE.json`, `08-DEPLOY-EVIDENCE.json`, `08-33-SUMMARY.md`
- Environment probes: tool versions, `aws sts get-caller-identity`, `pip index versions boto3`, `gsd-tools query package-legitimacy`

### Secondary (MEDIUM confidence — official docs fetched this session)
- Lambda Web Adapter v0.9.1 README (readiness 100–499, `AWS_LWA_INVOKE_MODE`, `ASYNC_INIT`, supported sources) — raw.githubusercontent.com/awslabs/aws-lambda-web-adapter/v0.9.1/README.md
- Lambda function URL access control (NONE + two actions, condition keys, policy persists) — docs.aws.amazon.com/lambda/latest/dg/urls-auth.html
- CloudFront origin settings (response timeout semantics, defaults, quota) — docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/DownloadDistValuesOrigin.html
- Lambda response streaming (RESPONSE_STREAM, 6 MB/2 MB/s) — docs.aws.amazon.com/lambda/latest/dg/configuration-response-streaming.html
- Lambda quotas (900 s timeout) — docs.aws.amazon.com/lambda/latest/dg/gettingstarted-limits.html
- ALB Lambda targets (1 MB, waits for the function) — docs.aws.amazon.com/elasticloadbalancing/latest/application/lambda-functions.html
- LWA current env var names (`AWS_LWA_READINESS_CHECK_HEALTHY_STATUS`) — aws.github.io/aws-lambda-web-adapter/configuration/environment-variables.html

### Tertiary (LOW confidence)
- reqwest per-request timeout precedence (A2); ALB streaming (A3); Qwen3.5-0.8B embedding dims (A1) — training knowledge

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — nothing new in the workspace; every tool probed; release assets listed from the registry
- Architecture: HIGH on the server/platform seams (all read with line numbers); MEDIUM on the Function URL/LWA path (official docs, not yet exercised in this phase — the spike used `lambda:Invoke` only)
- Pitfalls: HIGH — each traced to a file, a measured spike result, or a lint run

**Research date:** 2026-10-15
**Valid until:** 2026-11-14 for the code seams (upstream moves ~1 release/week — re-check `server.rs::serve_router` and `mod_create_demo.rs:218` at the chosen tag); 7 days for the pmcp.run/SDK line numbers (both repos are active).
