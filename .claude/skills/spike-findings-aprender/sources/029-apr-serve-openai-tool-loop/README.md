---
spike: 029
idea: internal-llm-endpoint
name: apr-serve-openai-tool-loop
type: standard
validates: "Given `apr` v0.70.3 serving the ladder-certified Qwen3.5-0.8B/4B Q4_K_M GGUFs on CPU, when pmcp.run's two OpenAI clients (pmcp-agent `OpenAiCompatSource`, durable-agent-lambda `openai_v1`) drive a tool-calling loop against `/v1/chat/completions` with real MCP-shaped tools, then the tool call is emitted (`finish_reason: tool_calls`, parseable args), the `role: tool` turn is accepted, the final answer arrives, and every contract gap is enumerated"
verdict: PARTIAL
related: [016, 017, 020, 021, 023, 026]
tags: [openai-api, qwen3.5, tool-calling, pmcp-agent, apr-serve]
---

# Spike 029: apr serve as pmcp.run's OpenAI-compatible LLM endpoint (tool loop)

## What This Validates

Given `apr` v0.70.3 (the release binary, sha256-verified) serving the two ladder-certified
Qwen3.5 Q4_K_M GGUFs on CPU, when pmcp.run's two OpenAI clients drive a tool-calling agent
loop against `/v1/chat/completions`, then the tool call is emitted in OpenAI shape, the
tool-result turn is accepted, the final answer arrives, and every gap between what the
clients send and what the server accepts is enumerated.

The two clients, both read from source:

- **durable-agent-lambda `openai_v1`** (pmcp.run platform, `amplify/functions/durable-agent-lambda/src/llm/transformers/openai.rs`):
  `model`, `messages`, `max_tokens`, `top_p`, `stream`, `tools`; no `temperature`; renders an
  assistant tool-use turn with `content: ""` ("OpenAI requires content field to be an empty string
  when there's no text"), then `role: tool` + `tool_call_id` per result. Reads `choices[0].message`,
  `finish_reason`, `tool_calls`, `usage`.
- **pmcp-agent `OpenAiCompatSource`** (`crates/pmcp-agent/src/sources/openai_compat.rs`, SDK 2.22.7):
  POST `{base_url}/chat/completions`, Bearer key, `model`, `messages`, `max_tokens`, `temperature`,
  `tools`, `tool_choice` (`auto|required|none`); renders an assistant tool-use turn as
  `{"role":"assistant","tool_calls":[…]}` with **no `content` key**; `role: tool` + `tool_call_id`;
  non-streaming; 60 s default timeout; HTTPS required off-loopback unless `allow_insecure_http`.

## Research

Premises of the request, checked against its own pin (`v0.70.3`, 2026-10-15):

| Premise in the request | What the pin says |
|---|---|
| "expose the same OpenAI API" (to be built) | Already shipped: `apr serve run` routes `/v1/chat/completions` (SSE streaming; tool calls; streamed tool calls since 0.70.3, the release's one change), `/v1/completions`, `/v1/embeddings`, `/v1/models`, `/v1/effective-config`, Ollama `/api/{chat,generate,tags,show,pull,embeddings,delete}`, `/health`, `/metrics`. Bearer auth via `APR_API_KEY` / `APR_API_KEY_HASH` (`contracts/apr-serve-api-key-auth-v1.yaml`) |
| "Aprender support for Qwen 3.5" | Upstream only. The fork's `main` is 0.63.0 with no Qwen3.5; this spike uses the upstream release artifacts (`apr-v0.70.3-aarch64-apple-darwin-cpu.tar.gz`, sha256 `16eb3fcd…`) and the GGUFs the capability ladder pins: `Qwen3.5-0.8B-Q4_K_M.gguf` `bd258782…`, `Qwen3.5-4B-Q4_K_M.gguf` `00fe7986…` (both from `unsloth/Qwen3.5-*-GGUF`, sha256 matched) |
| "deploy to … Bedrock" | Bedrock runs weights, never a binary. Custom Model Import lists `Qwen2, Qwen2.5, Qwen2-VL, Qwen2.5-VL, Qwen3` and for Qwen3 "only `Qwen3ForCausalLM` and `Qwen3MoeForCausalLM`" (docs read 2026-10-15). Qwen3.5 is `Qwen3_5ForCausalLM` (Gated-DeltaNet hybrid): not listed. Bedrock's own OpenAI-compatible surface is `https://bedrock-runtime.<region>.amazonaws.com/openai/v1` for Bedrock-hosted models |
| Release confidence on CPU | 0.70.3's gate was CRUX smoke on an RTX 4090 and a GB10; CPU lanes are nightly. The load log prints `warning: Qwen3.5-0.8B is not CRUX-certified. Known issue #4030 … use thinking off or a certified model (Qwen3.5-2B/4B)`. Chat template resolves to `Qwen3NoThink (thinking off)` for both models |

Competing ways to exercise the contract:

| Approach | Tool | Pros | Cons | Status |
|---|---|---|---|---|
| Replay the clients' request shapes by hand | `tools/probe_openai.py` (stdlib) | Exhaustive, every field isolated, bisectable | Not the real client | **used** (P0–P12) |
| Run the real SDK loop | `agent-loop/` (pmcp-agent `AgentEngine` + `OpenAiCompatSource` by path, local `ToolInvoker`) | The production client; `DecisionTrace` is the forensic artifact | `cargo pmcp agent dev` uses a `NoopInvoker` that advertises no tools, so a custom invoker is required | **used** |
| Run the platform transformer | durable-agent-lambda | The other production client | Needs the Lambda's config/DDB; its request shape is fully determined by `transform_request` + `transform_single_message`, replayed in the probe | shape replayed, binary not run |
| Wire-logging shim | `tools/shim.py` | Records every request/response; can apply the one-line client-side fix without forking either side | One hop of latency | **used** |

## How to Run

```bash
cd .planning/spikes/029-apr-serve-openai-tool-loop
./bin/apr serve run models/Qwen3.5-0.8B-Q4_K_M.gguf --port 8029 --backend cpu     # 3 s to /health
./bin/apr serve run models/Qwen3.5-4B-Q4_K_M.gguf   --port 8030 --backend cpu     # 7 s
python3 -I tools/probe_openai.py --base http://127.0.0.1:8029/v1 --model Qwen3.5-0.8B-Q4_K_M --out results/probe-0.8b.json
python3 -I tools/bisect_422.py  --base http://127.0.0.1:8029/v1 --model Qwen3.5-0.8B-Q4_K_M
python3 -I tools/shim.py --listen 8129 --upstream http://127.0.0.1:8029 --log results/wire-0.8b-plain.jsonl
python3 -I tools/shim.py --listen 8130 --upstream http://127.0.0.1:8029 --log results/wire-0.8b-fixed.jsonl --fix-null-content
cd agent-loop && CARGO_TARGET_DIR=../../../../target cargo run --release -- --base http://127.0.0.1:8129/v1 --model Qwen3.5-0.8B-Q4_K_M
```

`models/` holds symlinks into the Hugging Face cache (gitignored); `bin/apr` is the release
binary (gitignored, sha256 file committed beside it).

## What to Expect

- Probe table: every row ✓ except the gaps listed under Results.
- Agent loop through the plain shim: iteration 1 emits `get_weather`, iteration 2 fails with
  HTTP 422 (`RunOutcome::Failed`). Through the fixing shim: `Completed` with an answer that
  carries the tool result (17 °C, light rain).

## Observability

`tools/shim.py` appends one JSON line per request to `results/wire-*.jsonl` (ISO timestamp,
request body, response body, status, latency, which assistant messages it patched) and serves
`GET /__shim/summary`. The agent driver prints pmcp-agent's `DecisionTrace` and writes a record
with `--out`.

## Investigation Trail

1. **Both models load on CPU in seconds** (mmap): 0.8B `/health` after 3 s, 4B after 7 s.
   `model_loaded: true`, `compute_mode: cpu`.
2. **Contract probe, 0.8B** (`results/probe-0.8b.md`): 10/15 ✓. Tool calls work in OpenAI shape
   (`finish_reason: tool_calls`, `id: call_0`, `type: function`, `arguments` is a JSON string),
   streamed tool calls arrive as `delta.tool_calls` and end with `finish_reason: tool_calls`,
   ten tools → the right one (`convert_currency`, 926 prompt tokens), `usage` is present in both
   plain and streamed responses, unknown OpenAI fields are tolerated.
   **The tool-result turn returned 422**: `{"error": "Invalid request body. Check that the JSON
   structure matches the expected schema."}`.
3. **Bisect** (`tools/bisect_422.py`, `results/bisect-422-0.8b.md`): the 422 is the assistant
   message's `content` being **absent or `null`**. `content: ""` → 200 and the model uses the
   result ("light rain … 17 °C"). `role: tool`, `tool_call_id`, and `tool_calls` on the assistant
   are all accepted (ignored by the template, not refused). So the platform transformer passes
   (it writes `""`) and the SDK source fails (it omits the key).
4. **An exact-prompt KV cache exists.** Repeating one request: 52.5 s → 3.4 s → 3.3 s; the same
   prefix with a different tool result: 51.0 s again. A prefix cache would have reused the shared
   320-token prefix; this one keys on the whole prompt, so an agent loop (which appends every
   turn) never hits it. (Timings here are under contention with the 4B probe; 030 re-measures.)

5. **Contract probe, 4B** (`results/probe-4b.md`): the same 10/15 pattern, same gaps, cleaner
   arguments (`"from":"EUR","to":"USD"`); `max_completion_tokens` ignored → 256 tokens and
   `finish_reason: length`.
6. **The real SDK loop** (`agent-loop/`, pmcp-agent 0.4.0 on SDK 2.22.7, `DecisionTrace` recorded):
   - through the plain shim, 0.8B: iteration 0 emits `get_weather {"city":"Paris","unit":"celsius"}`,
     iteration 1 → `RunOutcome::Failed { "completion decode error: http 422" }`. The wire log
     shows request #3's assistant message as `{"role":"assistant","tool_calls":[…]}`, no `content`.
   - through the `--fix-null-content` shim, 0.8B: `Completed` in 11.7 s, two turns, final text
     "The weather in Paris is currently light rain. The temperature is 17°C." (390 → 424 prompt
     tokens). 4B with the `add` tool: `Completed` in 110.9 s, `add {"a":17,"b":25}` → "17 plus 25
     equals 42." (402 → 428 prompt tokens).
7. **The cache is a session KV state, not a prompt cache.** `qwen35_session.rs` (pin): "a prompt
   that EXTENDS the tokens the state holds prefills only the new suffix; any other prompt resets
   the state in place and prefills whole." That is why an exact repeat hit (3.4 s), a diverging
   tail missed (51 s), and the agent's turn 2 — which extends turn 1's prompt + its own tool call —
   cost 8.3 s on 0.8B and 9.7 s on 4B against 24 s and 101 s for turn 1. One resident state per
   process: a second conversation interleaved between two turns resets it (to measure in 030).
8. **`APR_API_KEY` does nothing on the GGUF path** (`tools/auth_probe.sh`, `results/auth-probe-0.8b.md`):
   with the key set, no header, a wrong key and the right key all return 200 on
   `/v1/chat/completions`, and no "unauthenticated" warning is logged. Source: `AuthGate::from_env()`
   is wired only into `build_apr_cpu_router` (the `.apr` CPU server, `apr-cli/src/commands/serve/handlers.rs:1510`);
   the GGUF server uses `aprender-serve`'s `create_router(state)`, which takes no gate.
9. **Timings in this spike are contaminated**: the 0.8B and 4B servers and their probes ran at the
   same time on one laptop. They order the findings; spike 030 measures alone.

## Results

**Verdict: PARTIAL ⚠.** The OpenAI tool-calling contract works end to end for the shape pmcp.run's
platform transformer sends, and breaks at turn 2 for the shape the pmcp-agent SDK sends, on a
one-field strictness that upstream has already annotated as a follow-up. The server has no
working auth on the GGUF path. Both Qwen3.5 sizes pick the right tool with parseable arguments.

### Works (both models)

| Capability | Evidence |
|---|---|
| `POST /v1/chat/completions`, OpenAI response shape | `id`, `object`, `created`, `model`, `choices[].message`, `finish_reason`, `usage` |
| Tool call emission | `finish_reason: tool_calls`; `tool_calls[0] = {id: call_0, type: function, function: {name, arguments: <JSON string>}}` |
| Right tool among 10 MCP-shaped tools | `convert_currency {"amount":250,"from":"EUR","to":"USD"}` at 926 prompt tokens |
| Tool-result turn → grounded answer | with `content: ""` on the assistant turn: "light rain … 17°C", "17 plus 25 equals 42." |
| Streaming | SSE, `data: [DONE]`, `usage` on the stream; tool calls as `delta.tool_calls` + `finish_reason: tool_calls` (the 0.70.3 change) |
| Unknown OpenAI fields | `stream_options`, `parallel_tool_calls`, `response_format`, `seed`, `top_p`, penalties, `metadata`, `store`: all tolerated |
| Session KV reuse across agent turns | turn 2 prefills only the suffix (see trail 7) |
| `GET /v1/models` | 200, `data: [{id: "default"}]` |

### Gaps, by impact on pmcp.run

| # | Gap | Who it hits | Where | Fix size |
|---|---|---|---|---|
| 1 | **422 when an assistant message has no `content` / `content: null`** | pmcp-agent `OpenAiCompatSource` (omits the key) → every SDK tool loop fails at turn 2. Platform `openai_v1` unaffected (writes `""`) | `ChatMessage.content: String`, "kept REQUIRED on deserialize (PMAT-801)… follow-up — relax to Option" (`aprender-serve/src/api/mod_create_demo.rs`) | one line either side: server `Option<String>` + default, or SDK `"content": ""` on the tool-use turn (`openai_compat.rs::message_to_openai`) |
| 2 | **`APR_API_KEY` / `APR_API_KEY_HASH` not enforced on GGUF models** | any deployment that relies on the server's own auth | `AuthGate` wired only into the `.apr` CPU router | wire `AuthGate::from_env()` into the GGUF router; until then the front door must authenticate |
| 3 | `tool_choice: "required"` ignored (behaves as `auto`) | pmcp-agent `ToolChoiceMode::Required` | server | parse + force |
| 4 | `max_completion_tokens` ignored; default cap 256 → `finish_reason: length` | OpenAI SDK ≥ 1.40 clients; the platform sends `max_tokens` (honoured) | request struct | alias the field |
| 5 | Error envelope `{"error": "<string>"}` vs OpenAI `{"error": {"message","type","code"}}` | OpenAI SDKs show a generic message; pmcp-agent classifies by status only | `router.rs:424` | shape the envelope |
| 6 | `model` is ignored and echoed (`gpt-4o-mini` → served by the loaded model, 200) | platform routing cannot rely on the server refusing a wrong id | handler | compare to the loaded id or `/v1/models` |
| 7 | `tool_choice: "none"` → the model still writes `<tool_call>…` XML into `content` | a loop that disables tools sees XML text | template keeps advertising tools | drop tools from the render when `none` |

### Signal for the build

- pmcp.run can point an agent at `apr serve` **today** through its `openai_v1` transformer and an
  `LLMProvider` row (`endpointTemplate: https://<host>/v1/chat/completions`, Bearer auth header).
  The SDK path (`cargo pmcp agent dev`, pmcp-agent) needs gap 1 closed on either side first.
- Put auth and model-id routing in the front door; do not rely on the server for either (gaps 2, 6).
- Each agent turn re-sends the whole history; the session state makes that cheap only while one
  conversation owns the process. Concurrency vs cache is a 030 measurement and a hosting decision.
- The 0.8B model, flagged "not CRUX-certified (#4030)", still produced correct tool calls with
  thinking off; the ladder's certified floor is 2B.
