# `apr serve` as pmcp.run's OpenAI-compatible LLM Endpoint (conformance)

What pmcp.run's two OpenAI clients send, what `apr serve` (v0.70.3, GGUF Qwen3.5 on CPU) accepts,
the one byte that breaks the SDK loop, and the gaps a front door must cover. Read this before wiring
an `LLMProvider` row at the internal endpoint or before touching the server's chat request structs.

## Requirements

From idea `internal-llm-endpoint`:
- **The contract is pmcp.run's clients, not the OpenAI spec in the abstract**: verdicts are read
  from the durable-agent-lambda `openai_v1` request shape and pmcp-agent's `OpenAiCompatSource`,
  replayed against the real server.
- **Use the upstream release artifacts at the cited tag and the ladder-certified GGUFs**
  (`contracts/model-capability-ladder-v1.yaml` sha256 pins), not a fork build.
- **Bedrock is a provider the agent can point at (`/openai/v1`), not a host** for aprender or
  Qwen3.5 (Custom Model Import lists `Qwen3ForCausalLM` / `Qwen3MoeForCausalLM` only).
- **The internal endpoint is the option when security or cost outrank speed**; the GPU host is the
  performance option, not the default.

## How to Build It

### 1. Know the two clients (both read from source, 2026-10-15)
| client | request | assistant tool-use turn | reads |
|---|---|---|---|
| pmcp.run `durable-agent-lambda` `openai_v1` (`amplify/functions/durable-agent-lambda/src/llm/transformers/openai.rs`) | `model`, `messages`, `max_tokens`, `top_p`, `stream`, `tools`; **no `temperature`** | `{"role":"assistant","content":"","tool_calls":[…]}` ("OpenAI requires content … empty string") then `role: tool` + `tool_call_id` | `choices[0].message`, `finish_reason`, `tool_calls`, `usage`, `refusal` |
| pmcp-agent `OpenAiCompatSource` (`rust-mcp-sdk/crates/pmcp-agent/src/sources/openai_compat.rs`) | POST `{base_url}/chat/completions`, Bearer key, `model`, `messages`, `max_tokens`, `temperature`, `tools`, `tool_choice` (`auto`/`required`/`none`); non-streaming; **60 s default timeout**; HTTPS off-loopback unless `allow_insecure_http` | `{"role":"assistant","tool_calls":[…]}` — **no `content` key** | `choices[0].message.tool_calls`, `finish_reason`, `usage.total_tokens` |

The platform's provider mapping is `config/loader.rs::map_provider_config` (hardcoded `anthropic` /
`openai` arms; R2-10 makes it data-driven: `LLMProvider{transformerId, endpointTemplate,
authHeaderName, secretKeyName}`). An aprender endpoint is one row with `transformerId: openai_v1`.

### 2. Run the server from the pin (no build)
```bash
gh release download v0.70.3 --repo paiml/aprender -p 'apr-v0.70.3-aarch64-apple-darwin-cpu.tar.gz*'   # sha256 file beside it
uv run --python 3.12 --with huggingface_hub hf download unsloth/Qwen3.5-0.8B-GGUF Qwen3.5-0.8B-Q4_K_M.gguf   # sha256 bd258782… (ladder pin)
./apr serve run models/Qwen3.5-0.8B-Q4_K_M.gguf --port 8029 --backend cpu      # /health in 3 s; "chat template: Qwen3NoThink (thinking off)"
```
Routes at v0.70.3: `/v1/chat/completions` (SSE streaming; tool calls; streamed tool calls as
`delta.tool_calls` + `finish_reason: tool_calls`), `/v1/completions`, `/v1/embeddings`, `/v1/models`
(returns `default`), `/v1/effective-config`, Ollama `/api/{chat,generate,tags,show,pull,embeddings,delete}`,
`/health`, `/metrics`. 4B needs 7 s to `/health`; 2B is the smallest CRUX-certified model (0.8B loads
with a `#4030` warning and still produced correct tool calls in every run).

### 3. Prove the contract with the three-layer probe (`sources/029-*/tools/`)
1. `probe_openai.py` — replays every field the clients send and the OpenAI extras others send;
   prints a Markdown verdict table (`results/probe-*.md`).
2. `bisect_422.py` — one hypothesis per request when a shape is rejected.
3. `shim.py` — a wire-logging reverse proxy (one JSON line per request/response) with
   `--fix-null-content`, so a client-side one-line fix is proven without forking either side; then run
   the **real SDK loop** (`agent-loop/`: pmcp-agent `AgentEngine` + `OpenAiCompatSource` by path, a
   `ToolInvoker` that advertises tools via `list_tools`, the user turn seeded into `RunState.history`).
   `cargo pmcp agent dev` cannot do this: its `NoopInvoker` advertises no tools.

### 4. What works (0.8B and 4B)
OpenAI response shape with `usage`; `finish_reason: tool_calls` with `tool_calls[0] = {id: call_0,
type: function, function: {name, arguments: <JSON string>}}`; the right tool among ten
(`convert_currency {"amount":250,"from":"EUR","to":"USD"}`); the tool-result turn → grounded
answer when the assistant turn has `content: ""`; SSE with `[DONE]` and `usage` on the stream;
unknown fields (`stream_options`, `parallel_tool_calls`, `response_format`, `seed`, `top_p`,
penalties, `metadata`, `store`) tolerated.

### 5. The gaps, by impact
| # | gap | hits | fix |
|---|---|---|---|
| 1 | **422 when an assistant message has no `content` / `content: null`** (`ChatMessage.content: String`, "kept REQUIRED on deserialize (PMAT-801) … follow-up", `aprender-serve/src/api/mod_create_demo.rs`) | every pmcp-agent SDK tool loop at turn 2 (`completion decode error: http 422`); platform unaffected | one line either side: server `Option<String>` + default, or SDK `"content": ""` in `openai_compat.rs::message_to_openai` |
| 2 | **`APR_API_KEY` / `APR_API_KEY_HASH` not enforced on GGUF models** (`AuthGate::from_env()` wired only into the `.apr` CPU router, `apr-cli/src/commands/serve/handlers.rs`; the GGUF path uses `create_router(state)`) | anything relying on server auth | wire the gate; until then the front door authenticates |
| 3 | `tool_choice: "required"` behaves as `auto` | pmcp-agent `ToolChoiceMode::Required` | server |
| 4 | `max_completion_tokens` ignored; default cap 256 → `finish_reason: length` | OpenAI SDK ≥ 1.40 clients (`max_tokens` is honoured) | alias |
| 5 | error envelope `{"error": "<string>"}` not `{"error": {"message","type","code"}}` (`router.rs:424`) | SDKs show a generic message; pmcp-agent classifies by status only | shape it |
| 6 | `model` ignored and echoed (`gpt-4o-mini` → 200 from the loaded model) | platform routing cannot rely on the server refusing a wrong id | compare to the loaded id |
| 7 | `tool_choice: "none"` → `<tool_call>…` XML leaks into `content` | a loop that disables tools | drop tools from the render |

## What to Avoid
- Judging conformance from the OpenAI spec or from `curl` alone — the clients' shapes differ from
  each other on exactly the field that matters.
- Running the servers you measure on the same host as other servers: 029's timings were 2–3× off.
- Reading pmcp-agent's `cargo pmcp agent dev` as a tool-loop test (no tools advertised).
- Exposing the server directly: no auth on GGUF, `model` unchecked.
- `${h:+-H "$h"}` in the zsh harness collapses to one argument (an auth probe read every row 200);
  header-passing curl loops live in a `#!/bin/bash` script.

## Constraints
- Measured on apr 0.70.3 (`95f64ba14`), Qwen3.5-0.8B/4B Q4_K_M (ladder pins `bd258782…`, `00fe7986…`),
  macOS arm64 CPU build. The release gate for 0.70.3 ran on GPUs only (CRUX smoke, sm_89 / sm_121).
- Session state: one `Qwen35Session` per process behind a `Mutex`; a prompt that extends the previous
  prompt + completion prefills only the suffix, anything else resets (see the agent-turn-cost reference).
- pmcp-agent 0.4.0 on rust-mcp-sdk 2.22.7; cargo-pmcp 0.28.1.

## Origin
Synthesized from spike 029 (PARTIAL).
Source files: `sources/029-apr-serve-openai-tool-loop/` (probe, bisect, shim, `agent-loop/` driver, the
scaffolded `agent/spike029` package, results tables, wire logs). Binaries and models are not copied.
