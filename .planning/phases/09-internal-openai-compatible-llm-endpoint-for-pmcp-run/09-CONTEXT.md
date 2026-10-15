# Phase 9: Internal OpenAI-Compatible LLM Endpoint for pmcp.run - Context

**Gathered:** 2026-10-15
**Status:** Ready for planning
**Source:** Assembled by the plan-phase orchestrator from (a) the user's `/gsd-plan-phase` brief of
2026-10-15 — "start planning the real implementation (the LLMProvider row, the null-content fix, the
Lambda deployment). Let's start with the smallest qwen 3.5 model for now, to implement the end-to-end
including the pmcp.run parts, which we can collaborate with the pmcp.run dev team on the plans and the
implementation" — and (b) the six operator requirements recorded 2026-10-15 against the
`internal-llm-endpoint` idea in `.planning/spikes/MANIFEST.md`, plus the open items spikes 029–031
surfaced. No discuss-phase session was held; re-run `/gsd-discuss-phase 9` to amend any decision below.

<domain>
## Phase Boundary

Productise spikes 029–031 as a working, private, authenticated LLM endpoint for pmcp.run agents:

1. **Server conformance (upstream aprender):** the spike-029 gaps that break pmcp.run's clients are
   fixed as small PRs to paiml/aprender — assistant `content` null/absent (422), `APR_API_KEY` not
   enforced on the GGUF router — plus the cheaper gaps where a one-line fix exists.
2. **Deployment (this repo):** the decider pattern — one arm64 image (release binary + Lambda Web
   Adapter), GGUF from S3, default Lambda 10,240 MB — for the smallest model, Qwen3.5-0.8B Q4_K_M,
   reproducible from `just` recipes, measured live.
3. **pmcp.run integration (collaboration):** one `LLMProvider` row (`openai_v1`, Bearer), a
   per-provider timeout, and the pmcp-agent `content: ""` fix — delivered as PRs in the pmcp.run and
   rust-mcp-sdk repos with a shareable brief, so an agent switches between a hosted provider and the
   internal endpoint by configuration alone.

The proof is one real two-tool agent turn through BOTH platform clients against the deployed endpoint.

Not in this phase: 2B/4B models, Fargate, LMI or GPU hosts, the prefix/KV cache and batched Q4_K
CPU prefill (upstream performance work), tool-schema pinning and per-conversation routing on
pmcp.run, Bedrock-as-provider rows, a training or fine-tuning path.

</domain>

<decisions>
## Implementation Decisions

### Model and release pin
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

### Server conformance fixes (upstream PRs to paiml/aprender)
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

### pmcp.run integration (collaboration with the pmcp.run dev team)
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

### Deployment (decider pattern)
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

</decisions>

<specifics>
## Specific Ideas

- "Let's start with the smallest qwen 3.5 model for now, to implement the end-to-end including the
  pmcp.run parts" — correctness of the loop before any speed work; 0.8B is the vehicle.
- "which we can collaborate with the pmcp.run dev team on the plans and the implementation" — plans
  for the pmcp.run side must read as a brief a second team can act on, not as internal GSD tasks.
- "The internal endpoint is the option when security or cost outrank speed" — a 71 s turn is
  acceptable here; the GPU host is the performance option and is not in this phase.
- One foreign request resets the session state and the server serialises behind one Mutex: the live
  record must state this, so nobody shares one function between agents.

</specifics>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Spike evidence (the three references and their sources)
- `.claude/skills/spike-findings-aprender/references/openai-endpoint-conformance.md` — the two
  client shapes, routes at v0.70.3, the three-layer probe, the gap table (the fix list for D-03..D-05)
- `.claude/skills/spike-findings-aprender/references/llm-endpoint-agent-turn-cost.md` — the
  performance model (prefill at the decode rate, session reuse, serialisation, Graviton ratios)
- `.claude/skills/spike-findings-aprender/references/llm-endpoint-aws-deployment.md` — the image,
  weights staging, shared resources, the three hosts measured, the Lambda decision and its constraints
- `.claude/skills/spike-findings-aprender/sources/029-apr-serve-openai-tool-loop/` — `tools/probe_openai.py`,
  `tools/bisect_422.py`, `tools/shim.py`, `tools/auth_probe.sh`, `agent-loop/` (the real SDK loop
  driver), `results/wire-*.jsonl` (the exact request bodies to fixture), `results/probe-*.md`
- `.claude/skills/spike-findings-aprender/sources/031-openai-endpoint-aws-hosts/` — `deploy/Dockerfile`,
  `deploy/entrypoint.sh`, `tools/build_image.sh`, `tools/fetch_weights_codebuild.sh`, `tools/aws_up.sh`,
  `tools/lambda_up.py`, `tools/remote_bench.py`, `tools/shapes.py`, `tools/teardown.sh`,
  `results/aws-ids.json`, `results/bench-lambda-0.8b.json`
- `.planning/spikes/MANIFEST.md` §`internal-llm-endpoint` — the six operator requirements of record
- `.planning/spikes/WRAP-UP-SUMMARY.md` §"Session 5" — key findings and the open-items list this
  phase draws its scope from

### Contracts and architecture rules
- `contracts/model-capability-ladder-v1.yaml` — the Qwen3.5-0.8B Q4_K_M sha256 pin and the
  certification status this phase must quote honestly
- `contracts/apr-serve-api-key-auth-v1.yaml` — the `APR_API_KEY` / `APR_API_KEY_HASH` contract D-04
  must satisfy on the GGUF router
- `CLAUDE.md` §"Verification Discipline" — `$?` through a pipe, label-by-intent, pin the binary,
  cpu_part-style mechanism proof; §"Contract Validation" — `pv`, never bash; §"Shell Scripts" — bashrs
- `.claude/skills/spike-findings-aprender/references/aws-mcp-model-hosting.md` — the decider pattern
  (021–023, 026) the deployment reuses

### Existing code to mirror
- `crates/aprender-mcp-decide-lambda/` and the Phase 8 planning directory's
  `08-LIVE-DEPLOY-EVIDENCE.json`, `08-DEPLOY-EVIDENCE.json` — the deploy recipes, identity checks and
  live-record shape; `08-CONTEXT.md` D-18 — the go/no-go checkpoint precedent for D-15
- `justfile` (repo root) — the deployment surface the new recipes join
- `crates/aprender-serve/src/api/mod_create_demo.rs` — `ChatMessage` (line ~218, `content: String`,
  PMAT-801 comment), `tool_choice`, `max_tokens` handling
- `crates/apr-cli/src/commands/serve/routes.rs` (`create_router`), `handlers.rs`
  (`build_apr_cpu_router(state, AuthGate::from_env())`), `auth.rs` (`AuthGate`)

### pmcp.run and the SDK (outside this repo — read, never commit to main)
- `~/Development/mcp/sdk/pmcp-run/amplify/data/resource.ts` — the `LLMProvider` model (fields D-08 fills)
- `~/Development/mcp/sdk/pmcp-run/amplify/functions/durable-agent-lambda/src/config/loader.rs` —
  `map_provider_config` (hardcoded seed) and `map_provider_row_to_config` (the data-driven path)
- `~/Development/mcp/sdk/pmcp-run/amplify/functions/durable-agent-lambda/src/handler/iteration.rs` —
  `LLM_HTTP_TIMEOUT_SECS` and its operator checkpoint comment (D-10)
- `~/Development/mcp/sdk/pmcp-run/amplify/functions/durable-agent-lambda/src/llm/transformers/openai.rs` —
  the `openai_v1` request/response shape
- `~/Development/mcp/sdk/pmcp-run/amplify/functions/durable-agent-lambda/src/llm/service.rs` —
  `resolve_endpoint` (`{model}` templating)
- `~/Development/mcp/sdk/pmcp-run/components/agents/llm-provider-management.tsx`,
  `hooks/use-llm-providers.ts` — the admin surface that creates a row
- `~/Development/mcp/sdk/pmcp-run/.agents/skills/spike-findings-pmcp-run/references/multi-provider-llm-gemini.md`
  — R2-10, the platform team's own decisions on data-driven providers and where keys live
- `~/Development/mcp/sdk/rust-mcp-sdk/crates/pmcp-agent/src/sources/openai_compat.rs` —
  `message_to_openai`, the 60 s default timeout, `allow_insecure_http`

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- The spike-031 image, entrypoint and every deploy driver are already written and ran on three hosts;
  this phase moves them in-repo under recipes with fail-closed pins, it does not redesign them.
- The spike-029 probe, 422 bisector, wire-logging shim and the `agent-loop` SDK driver are the
  conformance harness; the wire logs are ready-made fixtures for the server tests.
- `crates/apr-cli/src/commands/serve/auth.rs::AuthGate` already implements the key check; D-04 is
  wiring, not a new gate.
- Phase 8's deploy recipes, identity checks and evidence JSON shape (`crates/aprender-mcp-decide-lambda/`)
  are the template for D-13..D-15.

### Established Patterns
- Contracts declared before runs, `pv`-validated, with falsification tests (Phases 5, 6, 8).
- Thin-server rule: one model per deployed server; here one conversation per function is the
  analogue, because the session state is per process.
- Live deploys are human-gated and produce a machine-checked evidence file; claims are licensed only
  by that record (Phase 8 D-16 exception row).

### Integration Points
- Upstream `paiml/aprender`: serve api structs, GGUF router wiring, tests — small PRs.
- This repo: `contracts/`, `justfile`, a deployment asset directory, the phase evidence files.
- pmcp.run: one DDB row + one Secrets Manager key, a loader/timeout change (PR), the admin page as-is.
- rust-mcp-sdk: one serialisation line in pmcp-agent (PR).

</code_context>

<deferred>
## Deferred Ideas

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

</deferred>

---

*Phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run*
*Context gathered: 2026-10-15*
