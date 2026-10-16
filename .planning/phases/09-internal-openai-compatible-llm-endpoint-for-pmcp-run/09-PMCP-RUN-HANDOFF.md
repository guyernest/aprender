# Internal LLM endpoint for pmcp.run — hand-off from the Aprender side

Status date: 2026-10-16. Source of truth for every number below:
`.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-LIVE-DEPLOY-EVIDENCE.json`
(in the aprender repo, branch `gsd/phase-2-contract-gate`). Nothing here is quoted from memory; where
something is *not* proven it says so.

## 1. What this is

`apr serve` (the aprender inference server) running a small Qwen3.5 model as an
**OpenAI-compatible `/v1/chat/completions` endpoint**, packaged as a Lambda container image. Goal of the
phase: a pmcp.run agent selects it by configuration alone and completes a tool-calling turn through the
platform's own `openai_v1` client.

Split of work:

| Aprender side (done / in progress) | pmcp.run side (to do) |
|---|---|
| Server, fixes, image, Lambda function, measurement | Public/authenticated front door (URL, OAuth / API Gateway or other) |
| Conformance contract + probe harness | `LLMProvider` row + Secrets Manager entry |
| Upstream PRs for the server fixes | Per-provider timeout (see 4.3) |
| SDK fix branch (local, unpushed) | Running the platform turn and reviewing the PRs |

## 2. What is deployed right now (account 816069129666, us-east-1, profile `ze-kasher-dev`)

All created by repo recipes (`just llm-endpoint-*`), env `dev`. **No URL and no public surface exists.**
Verified again at the end of the run: `get-function-url-config` returns `ResourceNotFoundException`.

| Resource | Name / value |
|---|---|
| Lambda function | `aprender-llm-endpoint-dev` — `arn:aws:lambda:us-east-1:816069129666:function:aprender-llm-endpoint-dev` |
| Package / arch | container image, arm64 (Graviton) |
| Size | 10,240 MB memory, 10,240 MB `/tmp`, 900 s timeout |
| Reserved concurrency | **1** (deliberate: one conversation per process) |
| Image | ECR `816069129666.dkr.ecr.us-east-1.amazonaws.com/aprender-llm-endpoint-dev`, tag `cpu-v0.71.4`, digest `sha256:f291c0a77f17d9f203953bba748f1a3f7287780d36628883f933d5b069a1dd15` |
| Server | `apr 0.71.4 (5027da624)` (release v0.71.4), behind Lambda Web Adapter 0.9.1 |
| Model | Qwen3.5-0.8B Q4_K_M, sha256 `bd258782e35f7f458f8aced1adc053e6e92e89bc735ba3be89d38a06121dc517` |
| Weights | S3 `aprender-llm-endpoint-weights-816069129666-dev/Qwen3.5-0.8B-Q4_K_M.gguf` (532,517,120 bytes), pulled to `/tmp` at cold start and sha256-checked |
| IAM role | `aprender-llm-endpoint-dev` (basic execution + read on that one bucket) |
| Logs | `/aws/lambda/aprender-llm-endpoint-dev` (7-day retention) |
| Function env var *names* | `APR_API_KEY_HASH`, `MODEL_S3_URI`, `MODEL_SHA256`, `PLATFORM`, `RAYON_NUM_THREADS` |

Untouched: the earlier spike-031 leftovers in the same account (only the spike bucket was *read*, for a
server-side copy of the weights). Idle cost is cents per month. Total spend for deploy + measurement:
**$0.154** (ceiling was $0.50).

The function is reachable today only through `lambda:Invoke` with an API Gateway **v2 (payload 2.0)** event —
that is how all measurements and the conformance probe were run. The test event is in the repo:
`deploy/llm-endpoint/events/apigw-v2-no-auth.json`.

## 3. The blocker you need to know about: server-side auth is NOT in the deployed build

* The deployed release (v0.71.4) **does not enforce the Bearer key** on the GGUF path, even though
  `APR_API_KEY_HASH` is set. Measured: an unauthenticated `POST /v1/chat/completions` returned **200**
  (`just llm-endpoint-prove-401` → `401 NOT PROVEN: statusCode=200`).
* The fix is upstream draft PR **paiml/aprender#5451** (open, awaiting paiml workflow approval + CI, then
  review/merge/release). It wraps the GGUF router with the existing `AuthGate`; tested locally (401 / 401 /
  200 on no key / wrong key / right key) and on an interim build.
* Consequence: anything that can reach this function can use it unauthenticated. **Do not give it a public
  or unauthenticated URL on the current image.** Cost exposure if you did: each cold start is about $0.004–0.005
  and a tool turn about $0.0095 at this tier.
* Two ways forward: (a) wait for a release containing #5451 and re-pin (`just llm-endpoint-image`/`deploy`),
  or (b) keep the function reachable **only** through an authorizing front door that invokes it directly
  (API Gateway → Lambda integration needs no Function URL), in which case the front door *is* the gate and the
  server-side key is defense in depth. Option (b) is your call; see the header-collision note in 4.2.

## 4. What the pmcp.run side needs to know

### 4.1 The provider row is data, not code

Planned shape (from our decision record D-08; please check against `amplify/data/resource.ts` and
`config/loader.rs::map_provider_row_to_config`, which we read but you own):

* `transformerId: openai_v1`
* `endpointTemplate: <HTTPS base URL>/v1/chat/completions` (no `{model}` token)
* `authHeaderName: Authorization`, `authHeaderPrefix: "Bearer "`
* `secretKeyName: <name>` — the key value only in Secrets Manager
* `enabled: true`, `availableBuiltInTools: []`

No new transformer and no provider branch should be needed.

### 4.2 Front door: things we measured that constrain the choice

* **Turns are long.** Warm two-tool turn: **71.5–72.3 s** wall (≈ $0.0095). Cold start to first answer:
  **20.1 s** (init 7.5 s: S3 pull ≈ 19 s inside the entrypoint, sha256 check ≈ 1.8 s). Turn 2 (tool result extending the
  same conversation): **4.6–8.1 s** because the prefix is reused. A **ten-tool** prompt took **148.8 s** — prefill runs at
  only ≈ 6.3 tokens/s on this 0.8B at this tier.
* **API Gateway's integration timeout is the problem for the URL you described.** Our research record (D-12)
  ruled API Gateway out because of its ~30 s integration cap against these turns (HTTP APIs have a hard 30 s
  limit; REST APIs ≈ 29 s by default, with a quota that may be raised — we have **not** verified current
  limits, please check). A Lambda Function URL (up to the function's 900 s), CloudFront (origin read timeout
  is also limited and quota-raisable) or an ALB (idle timeout configurable) were the alternatives we listed.
  We did **not** build or test any of them.
* **Header collision.** The platform client sends `Authorization: Bearer <key>`. If an OAuth authorizer
  at the front door consumes that same header, either strip/replace it before the integration or do not
  rely on the server-side key (and note the server rejects a request whose `Authorization` it cannot match
  once #5451 ships).
* **Streaming is buffered** through the Lambda Web Adapter (`streaming: buffered-through-adapter`).
* **Reserved concurrency 1** means a second simultaneous request is refused by Lambda immediately
  (`TooManyRequestsException`, 0.8 s) — it does not scale out. Expect a 429/502-style failure from the
  front door under concurrency; the server also holds one conversation per process and a foreign request
  resets the session.

### 4.3 Per-provider timeout (a pmcp.run change)

`LLM_HTTP_TIMEOUT_SECS = 120` is a single const in `durable-agent-lambda/src/handler/iteration.rs`, stamped on
every provider. Against this endpoint: the demo two-tool turn (≈ 72 s, plus a cold start if unwarmed) fits; a
**ten-tool turn (≈ 149 s) does not**. Our proposal: an optional `timeoutSecs` on the `LLMProvider` row, stamped
by the loader when present, bounded by the agent Lambda's own timeout, with the staleness invariant test
extended. Until it exists, keep demos to ≤ 2 tools on a pre-warmed function. (Pre-warm: a short request ahead of
the turn; do it inside the 15-minute idle window.)

### 4.4 Wire-level compatibility with the two clients

* The server previously answered **HTTP 422** on turn 2 of any tool loop whose client echoes the assistant's
  tool-call message **without** a `content` key (spike 029). The fix is upstream draft **#5411**.
* `max_completion_tokens` (newer OpenAI SDKs) was silently ignored; alias fix is upstream draft **#5412**.
* pmcp.run's own `openai_v1` transformer already writes `content: ""` on assistant tool-use turns, so it does
  **not** depend on #5411. The SDK path (`pmcp-agent`) omits it; our SDK fix is a **local, unpushed** single
  commit `eb684bbd6` on `fix/openai-compat-tool-use-content-empty-string` in the `rust-mcp-sdk` repo.
* Measured on the *deployed* function (lambda-invoke transport): the conformance table shows exactly the four
  known gaps of the release — tool result → final answer (422), `tool_choice=required` not honoured,
  `max_completion_tokens` not honoured, and the error envelope shape. These are what the three upstream
  PRs close; none of them blocks a turn through the platform's `openai_v1` client.
* Local SDK matrix (laptop, interim build, single 0.8B runs — not a reliability claim): the pmcp-agent
  loop completed 4/4 cells; the control (unfixed SDK against the unfixed release) failed with `http 422`
  on turn 2, as predicted.

### 4.5 The key

* The bearer key lives **only** in a local file on the aprender operator's machine; the function holds its
  SHA-256 as `APR_API_KEY_HASH`. It is not in the repo or in any record.
* It cannot be used at all until #5451 ships (see section 3). When it does, pick the key your side wants in
  Secrets Manager and tell us the hash (`printf %s "$KEY" | shasum -a 256`); the aprender side re-runs
  `just llm-endpoint-deploy dev ze-kasher-dev` to set it. Do not send the key itself over chat or tickets.

## 5. Honest limits of what has been proven

* Model: Qwen3.5-0.8B is the **vehicle for correctness**, not a quality choice. It is not CRUX-certified
  (issue #4030); 2B is the smallest certified size. Do not judge agent quality from it.
* Not tested on the deployed function: any HTTPS front door, OAuth, Function URL, authenticated 401 (the
  deployed build can't produce one), a second concurrent execution environment (reserved concurrency 1
  prevents it), streaming behaviour end to end.
* Cold-sample statistics are small (one or two runs per shape); `cpu_part` was `0xd40` (Graviton3) on every run —
  the spike saw a 2–3× lottery between Graviton generations.
* The local container proof of "10 GB admission" was not valid on a 4 GB Docker VM; the **Lambda tier itself**
  admitted the model (host MemTotal ≈ 10.97 GB, peak 1,602 MB).
* Three bugs in our deploy tooling only surfaced on the live run (weights-copy metadata, a log-host lookup
  pattern, a probe transport seam); all fixed and covered by the repo's self-test.

## 6. How to reproduce / operate (needs repo checkout + `ze-kasher-dev` access)

```bash
just llm-endpoint-selftest                      # offline, shimmed aws, no AWS calls
just llm-endpoint-prove-401 dev ze-kasher-dev   # expected to fail until #5451 is in the pinned release
just llm-endpoint-measure dev ze-kasher-dev deployed ...   # budget-guarded; see deploy/llm-endpoint/README.md
just llm-endpoint-teardown dev ze-kasher-dev    # removes the function, role, bucket object, image tag, log group
```

Everything is documented in `deploy/llm-endpoint/README.md` and the probe contract in
`deploy/llm-endpoint/probe/README.md`. Teardown was **not** run; the resources are live.

## 7. Open items and who owns them

| Item | Owner | State |
|---|---|---|
| paiml/aprender#5451 auth gate (blocks any authenticated URL on the release path) | Aprender / paiml | open draft |
| paiml/aprender#5411 content fix, #5412 alias fix | Aprender / paiml | open drafts |
| Re-pin to a release containing #5451, redeploy | Aprender | waiting on release |
| Front door (URL, OAuth, timeout ≥ 150 s or accept ≤ 2 tools) | pmcp.run | not started |
| `LLMProvider` row + Secrets Manager entry | pmcp.run | not started |
| Optional per-provider `timeoutSecs` | pmcp.run | proposal only |
| SDK `content: ""` fix PR (`pmcp-agent`) | Aprender → pmcp.run review | local branch, unpushed |
| CUDA-path routers still ungated; two upstream issues drafted, not filed | Aprender | deferred |
| Platform end-to-end turn (SC1) and the brief PRs | joint | blocked on the items above |
