# What an Agent Turn Costs on a CPU `apr serve` (Qwen3.5, measured)

The performance model behind every hosting decision for the internal LLM endpoint: prefill runs at the
decode rate on the CPU path, so an agent turn costs its prompt length; the session state makes turn 2
~10× cheaper and one foreign request destroys that; the server serialises. Read this before promising
any latency, choosing a model size, or sizing a host.

## Requirements

From idea `internal-llm-endpoint`:
- **The internal endpoint is the option when security or cost outrank speed** (operator, 2026-10-15);
  the GPU host is the performance option, not the default.
- **pmcp.run can pin and pre-cache an agent's tool schemas** (operator, 2026-10-15): the platform hosts
  both the MCP servers and the agent runtime, tool lists change rarely, so the stable prefix (system
  prompt + tool schemas) should be prefilled once and reused instead of re-sent per turn.
- Use the release artifacts at the cited tag and the ladder-certified GGUFs.

## How to Build It

### 1. Measure like 030 (`sources/030-*/tools/`)
- `run_matrix.sh`: one `apr serve` per (model, `RAYON_NUM_THREADS`), **alone on the host**, warm-up
  once, medians of two, a per-request budget that skips larger shapes.
- `bench.py`: every request streams (`stream: true`) so TTFT (first content / tool-call delta) is
  prefill and the delta timestamps are decode; `usage` arrives on the final chunk. **Every prompt
  carries a nonce** so the session state never helps by accident; then the state is tested on
  purpose: `turn1 → turn2_extend → foreign_between → turn2_after_foreign`; `conc2`/`conc4` report wall
  and per-request latency. `ab_prefill.sh`: default vs `APR_QWEN35_SESSION_PREFILL=per-token` vs
  `--no-fp8-cache`. `summarize.py` → `results/SUMMARY.md`.
- Request shapes come from the 029 wire logs: system + user ≈ 73 tokens; + 2 tools ≈ 421; + 10 tools
  ≈ 937; a 2k- and 4k-token handbook.

### 2. The numbers (apr 0.70.3, Q4_K_M, M4 Pro 10P+4E, alone)
| config (RAYON_NUM_THREADS) | prefill tok/s | decode tok/s | RSS GB | 2-tool turn (421 + 38) | 10-tool turn (937 + 53) | 2k handbook | 4k |
|---|---|---|---|---|---|---|---|
| 0.8B, 6 | 12.3 | 13.1 | 2.4 | **35 s** | 78 s | 154 s | 327 s |
| 0.8B, 14 | 16.5 | 17.9 | 2.4 | 26 s | 57 s | 118 s | 263 s |
| 2B, 6 | 5.9 | 6.1 | 2.8 (4.7 peak) | 76 s | 166 s | 325 s | – |
| 2B, 14 | 8.9 | 9.2 | 4.8 | 49 s | 108 s | 214 s | 445 s |
| 4B, 6 | 3.0 | 3.0 | 5.6 | **156 s** | 338 s | – | – |
| 4B, 14 | 4.2 | 4.4 | 5.6 | 103 s | 226 s | 469 s | – |

- **Prefill = decode rate, flat from 73 to 3,770 prompt tokens** (0.8B/6: 13.3 → 12.0 → 12.1 → 12.4 →
  11.6 tok/s). `T(turn) ≈ (prompt + output) / rate`. Tool schemas cost ~170 tokens per two tools.
- 6 → 14 threads buys 1.35–1.5×; 0.8B → 2B 2.1× slower; 2B → 4B 2× slower. Memory never binds.
- The pin's **batched CPU prefill (`forward_prefill_qwen35`, 64-token chunks, #4228) is engaged by
  default and is no faster than per-token** (79.2 vs 77.9 s at 937 tokens). `--no-fp8-cache` changes
  neither speed nor RSS on CPU. Tool choice was correct in all 22 tool requests.
- **Turn-2 session reuse** (`qwen35_session.rs`: a prompt that extends the held tokens prefills only
  the suffix; anything else resets): 9.4–10.6× saving in every config (0.8B/6: 3.6 s vs 35.6 s; 4B/6:
  15.5 s vs 153.9 s). One foreign request in between → the whole prompt again. Upstream #4274 (turn 2
  re-prefills from 0 via `/v1/chat/completions` on 0.69.3) is **fixed by 0.70.3**.
- **Concurrency serialises**: `Qwen35Served.session` is a `std::sync::Mutex`; wall ≈ the sum of the
  requests (0.8B/6: 11.2 s alone; 2 at once 10.3 + 20.7 s; 4 at once up to 31.2 s).
- On AWS (031): Graviton3 Lambda 6 vCPU = 0.53× M4-6 threads (6.5 tok/s on 0.8B); Graviton2 = 0.15×
  (1.9 tok/s on 2B); Fargate 16 vCPU Graviton4 ≈ M4-14 threads (4.5 tok/s on 4B); LMI Neoverse V3
  16 vCPU ≈ 5.7 tok/s on 4B.

### 3. Use the model
- A pmcp.run agent turn is **prompt-bound**: on 0.8B at the Lambda thread count a two-tool turn is 35 s
  on the M4 and 71 s on Graviton3; a ten-tool turn 78 s / 100 s. Promise that, or change what is paid.
- **The lever is the prefix, not the host.** Caching the stable prefix (system + tools) turns every
  turn into a suffix-only prefill: the turn-2 rows are the upper bound (7–15 s instead of 71–156 s).
  Needs (a) upstream: a prefix cache keyed by token prefix with more than one entry (today one state,
  extend-or-reset) and a prefill-only request (`max_tokens: 0`) to warm it; (b) pmcp.run: pin the
  agent's tool list, render it to identical bytes every turn, route one conversation to one process.
  Upstream tracks the GPU analogue as #4214 ("no prompt-prefix reuse"); the CPU path has no ticket.
  Upstream #5305 (2026-10-14) tracks prefill parity with llama.cpp on GPU for 0.71.x.
- The second lever is a **batched CPU prefill that actually amortises**: spike 020 measured 18× on the
  hybrid with F32 weights (GEMM over the row, DeltaNet per head in parallel); a Q4_K GEMM path would
  be the contribution that changes the verdict on CPU.
- If a CPU endpoint is kept: one conversation per process (keeps the 10× reuse), never share a server
  between agents, cap `max_tokens`, raise the SDK's 60 s timeout, keep tool lists short.

## What to Avoid
- Timing a server while another server or build runs on the same host (029's numbers were 2–3× off).
- Reading `RAYON_NUM_THREADS` as linear speed-up (1.35–1.5× for 2.3× the threads).
- Projecting Graviton from the 3.2× Graviton2 ratio of GEMM-bound work: decode here is bandwidth-bound
  and Lambda hands out Graviton2 **or** Graviton3 (see the deployment reference).
- Expecting the session cache to help a shared endpoint: the reuse rate is the probability that two
  consecutive requests belong to the same conversation.
- Forgetting the nonce: a repeated identical request is ~10× faster and says nothing.

## Constraints
- Measured on apr 0.70.3 CPU build, macOS arm64; the CPU path was not in the release gate (GPU-only
  CRUX smoke). `RAYON_NUM_THREADS` is read by `aprender-serve/src/inference/thread.rs`.
- RSS ≈ 4× the GGUF (2.4 / 4.8 / 5.6 GB for 0.53 / 1.28 / 2.74 GB files); the loader keeps
  `QuantizedTensorRef`s, so this is working memory, not a cache to switch off.
- Known issue #4030: 0.8B with thinking on may not close its think block; the server renders
  `Qwen3NoThink` (thinking off) for these models.

## Origin
Synthesized from spike 030 (VALIDATED), with 031's Graviton ratios.
Source files: `sources/030-qwen35-agent-turn-cost-cpu/` (bench, matrix, A/B, summarize, per-config
results tables and JSON, SUMMARY.md). Server logs stay in `.planning/spikes/030-*/results/`.
