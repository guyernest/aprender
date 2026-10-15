# API Coverage — `apr serve` OpenAI-compatible surface (as consumed by pmcp.run) and the AWS Lambda ingress

> Full coverage by default. Opt-outs are explicit, reasoned decisions.
> Scope source: `contracts/llm-endpoint-openai-conformance-v1.yaml` (plan 09-01), RESEARCH.md Q1/Q2, the spike-029 route list at v0.70.3 (`references/openai-endpoint-conformance.md` §2). The contract is pmcp.run's two clients (IR-1), not the OpenAI spec in the abstract, so INTEGRATE means "proven on the wire for those clients". Rows marked "probed" in the reason are exercised by the conformance probe even though no client depends on them; rows marked "KNOWN LIMITATION" are recorded in the conformance record (D-05) rather than fixed.

## Surface 1 — `apr serve` HTTP API (the endpoint the platform integrates)

| capability | decision | reason |
|---|---|---|
| `POST /v1/chat/completions` (non-streaming) | INTEGRATE | the only route both clients call; probed, fixed (D-03/D-05) and proven end to end (SC1/SC2) |
| `POST /v1/chat/completions` + `tools`, `tool_choice: auto` | INTEGRATE | the two-tool and ten-tool turns; contract rows 004-007 |
| tool-result turn (`role: tool`, `tool_call_id`) | INTEGRATE | gap 1 fix (D-03) + SDK fix (D-09); matrix in 09-05 |
| `POST /v1/chat/completions` with `stream: true` | INTEGRATE | probed (P6/P7) buffered through the adapter so a future streaming client has a verdict; neither platform client streams today, so the Function URL stays BUFFERED (Surface 2) |
| `max_completion_tokens` | INTEGRATE | aliased to `max_tokens` (D-05 cheap fix, plan 09-02) |
| `tool_choice: required` | OPT-OUT | KNOWN LIMITATION — needs constrained decoding to be honest; not on the real wire of either client (D-05) |
| `tool_choice: none` | OPT-OUT | KNOWN LIMITATION — render-path change shared with `apr chat`; not on the real wire; upstream issue, not a PR (D-05) |
| strict `model` id check | OPT-OUT | KNOWN LIMITATION — would make `LLMModel.modelId` load-bearing against a server answering `/v1/models` with `default`; `APR_SERVE_STRICT_MODEL` proposed upstream as an issue (D-05) |
| OpenAI-shaped error envelope `{"error": {message,type,code}}` | OPT-OUT | KNOWN LIMITATION — the string envelope is contracted (`routes.rs:321`, FALSIFY-AUTH-001, Ollama readers); pmcp-agent classifies by status, the platform reads `choices` only (D-05) |
| Bearer auth (`APR_API_KEY[_HASH]`) on the GGUF router incl. `/health` | INTEGRATE | D-04 fix (plan 09-03); 401 proven locally, on the deployed function and through the ingress |
| `GET /v1/models` | INTEGRATE | probed (P0); answers `default` — recorded, no client depends on it |
| `GET /v1/effective-config` | INTEGRATE | probed (P0); informational |
| `GET /health` | INTEGRATE | gated like every route (FALSIFY-AUTH-001); LWA 0.9.1 readiness counts a 401 as ready |
| `GET /metrics` | OPT-OUT | not needed yet — no client reads it; gated like every route |
| `POST /v1/completions` | OPT-OUT | not needed — neither client uses the legacy completions route |
| `POST /v1/embeddings` | OPT-OUT | not needed — the agents do not embed through this endpoint |
| Ollama `/api/*` routes (chat, generate, tags, show, pull, embeddings, delete) | OPT-OUT | explicitly out of scope — pmcp.run speaks OpenAI shapes only; `pull`/`delete` would mutate a function's `/tmp` and are never exposed |
| `response_format` (`json_object` / `json_schema`, v0.71.x) | OPT-OUT | not needed yet — neither client sends it; P8 keeps `{"type":"text"}` tolerated |
| `logprobs` / `top_logprobs` (v0.71.x) | OPT-OUT | not needed — no client reads them |
| `cpu_part` in `/health` | OPT-OUT | KNOWN LIMITATION — recorded by the entrypoint `[entry]` log line instead (D-14); platform-specific, not upstreamed (D-05) |

## Surface 2 — AWS Lambda ingress for the endpoint (RESEARCH Q1, D-12)

| capability | decision | reason |
|---|---|---|
| `lambda:Invoke` with an API-GW v2 event (IAM bench path) | INTEGRATE | the private bench and 401-proof path (D-12/D-14), aws-cli only |
| Function URL `AuthType: NONE` + `InvokeMode: BUFFERED` + two-statement policy | INTEGRATE | the HTTPS ingress the Bearer-only clients need; created only after the 401 proof (plan 09-08); torn down with both statements removed |
| Function URL `RESPONSE_STREAM` (`AWS_LWA_INVOKE_MODE=response_stream`) | OPT-OUT | not needed yet — neither client streams; sticky per URL; documented switch |
| Function URL `AuthType: AWS_IAM` | OPT-OUT | the platform client cannot SigV4-sign (D-12) |
| CloudFront front door (OAC + header check) | OPT-OUT | origin response timeout 30 s (60 s max without a quota increase) cannot carry 71-146 s turns; adds no auth the server lacks |
| ALB Lambda target | OPT-OUT | VPC + hourly cost + an ACM certificate/domain for HTTPS; adds nothing once the server gates |
| API Gateway HTTP API | OPT-OUT | 30 s integration cap (ruled out by the decide contract's own pricing) |
| Reserved concurrency | INTEGRATE | operator option at D-15: blast-radius cap and one-conversation-per-process; `RESERVED_CONCURRENCY` in the deploy recipe |
| Provisioned concurrency / SnapStart | OPT-OUT | explicitly out of scope — the phase is the cost/security option; warm-up is the pre-warm recipe |
| Lambda Managed Instances / Fargate / GPU hosts | OPT-OUT | deferred by CONTEXT (`## Deferred Ideas`) |

## Surface 3 — pmcp.run data model (consumed, not changed, except the one PR)

| capability | decision | reason |
|---|---|---|
| `LLMProvider` row (`openai_v1`, Bearer, `secretKeyName`) | INTEGRATE | D-08: one row, data only |
| `LLMModel` row + `AgentConfig.modelId` / `maxTokens` | INTEGRATE | the three data actions (brief §3) |
| `LLMProvider.timeoutSecs` (new optional field) | INTEGRATE | delivered as a PR for the team (D-10), clamped to 148 s; optional for the demo |
| `availableBuiltInTools` for this provider | OPT-OUT | the endpoint has no native tools; `[]` |
| new transformer / provider branch in `map_provider_row_to_config` | OPT-OUT | forbidden by D-08 (the row is data) |
| Bedrock's OpenAI surface as a provider row | OPT-OUT | deferred by CONTEXT; data-only on pmcp.run, not aprender work (IR-3) |
