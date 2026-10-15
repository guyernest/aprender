---
spike: 030
idea: internal-llm-endpoint
name: qwen35-agent-turn-cost-cpu
type: standard
validates: "Given `apr` v0.70.3 serving Qwen3.5 0.8B/2B/4B Q4_K_M on CPU alone on the host, when agent-shaped requests stream at RAYON_NUM_THREADS=6 (the 10 GB Lambda envelope) and 14 (all cores), then TTFT, prefill tok/s, decode tok/s, RSS and seconds per agent turn are measured, turn-2 session-KV reuse is measured against a foreign request in between, and 2/4 concurrent requests show whether the server parallelises or serialises"
verdict: VALIDATED
related: [019, 020, 021, 023, 029]
tags: [qwen3.5, cpu, prefill, decode, latency, concurrency, session-kv]
---

# Spike 030: what an agent turn costs on CPU

## What This Validates

Given the same server and models as spike 029, running **alone** on the host (029's timings
were contaminated by two servers and their probes sharing one laptop), when requests shaped
like pmcp.run agent turns are streamed, then the four numbers a hosting decision needs are
measured per model and thread count: time to first token (prefill), decode tokens per
second, resident memory, and seconds per turn. Two agent-specific questions ride along:
how much the session KV state saves on turn 2 (and loses when another conversation
interleaves), and what happens to two or four requests at once.

Thread counts: **6** is the vCPU of a 10,240 MB default Lambda (the only serverless CPU
envelope); **14** is every core of this M4 Pro (10 P + 4 E). The Graviton projection
follows the measured 3.2–3.3× M4→Graviton2 ratio from spikes 021/026 (6 threads).

## Research

- `apr serve` reads `RAYON_NUM_THREADS` (`crates/aprender-serve/src/inference/thread.rs`,
  `configure_optimal_thread_pool`), so the Lambda envelope is a one-variable proxy, as in 019.
- The session state (`qwen35_session.rs`, pin): a prompt that extends the held tokens prefills
  only the suffix; any other prompt resets the state and prefills whole. One state per process.
- Request shapes come from 029's wire logs: pmcp-agent sends system + user + tools; the
  platform transformer the same plus `stream`. Tool schemas cost ~170 prompt tokens per 2 tools
  (390 tokens with 2, 926 with 10, measured).
- All requests stream (`stream: true`) so TTFT is read from the first content/tool-call delta
  and decode from the delta timestamps; `usage` arrives on the final chunk.

## How to Run

```bash
cd .planning/spikes/030-qwen35-agent-turn-cost-cpu
bash tools/run_matrix.sh                 # 6 configs: {0.8B,2B,4B} x RAYON_NUM_THREADS {6,14}
bash tools/run_matrix.sh "4B 6"          # one config
cat results/bench-*.md
```

Each config starts its own `apr serve` on :8040, warms up once, runs each shape `REPS` times
(default 2, medians reported), the turn-2 experiment, and 2/4 concurrent requests; then kills
the server. A request over `BUDGET_S` (300 s) skips the larger shapes for that config.

## What to Expect

A table per config with TTFT, prefill tok/s, decode tok/s, total seconds and RSS for: `short`
(~55-token prompt, 128-token answer), `answer256`, `tools2` (~390 prompt tokens), `tools10`
(~926), `long2k`, `long4k`; then `turn1` → `turn2_extend` → `foreign_between` →
`turn2_after_foreign`; then `conc2` / `conc4` wall time against the sequential `short` cost.

## Investigation Trail

1. **First clean config (0.8B, 6 threads, alone):** warm-up 10.0 s; `short` (73 prompt tokens,
   ~75 out): TTFT 5.5 s → prefill **13.3 tok/s**, decode **13.2 tok/s**, RSS 2.25–2.30 GB.
   Prefill at the decode rate says the prompt is paid one token at a time on this path. The pin
   does have a batched CPU prefill (`forward_prefill_qwen35`, 64-token chunks, #4228) and the
   session defaults to it unless `APR_QWEN35_SESSION_PREFILL=per-token`; whether the serve path
   engages it is the A/B in `tools/ab_prefill.sh`, run after the matrix.
2. `apr serve` holds ONE `Qwen35Session` behind a `std::sync::Mutex`
   (`api/mod_app_state_qwen35.rs`, `qwen35_chat_backend.rs::spawn_streaming_generate`): every
   `/v1/chat/completions` generation takes the lock, so concurrent requests queue. The `conc2` /
   `conc4` rows measure the queue.
3. Upstream tracks the same gap: #5305 (2026-10-14) "Qwen3.5 speed: prefill and decode parity with
   llama.cpp" — prefill "about 0.2x llama.cpp" (GPU), sub-tickets #4313/#4376/#4442/#4450/#4483 for
   0.71.x, and #4274 (closed 2026-09-24) "turn 2 re-prefills from position 0; the session extend
   never engages via /v1/chat/completions" on 0.69.3. The `turn2_*` rows test that on 0.70.3.
4. The 2.25 GB RSS on a 0.53 GB file: the loader keeps `QuantizedTensorRef`s (no f32 widening in
   `qwen35_load.rs`), and `apr serve run --help` documents `--no-fp8-cache` as "Skip FP8 weight
   cache warmup. Saves ~1.5 GB RSS" — variant C of the A/B.

5. **A/B after the matrix** (`tools/ab_prefill.sh`, `results/ab-*.md`): default vs
   `APR_QWEN35_SESSION_PREFILL=per-token` vs `--no-fp8-cache` on 0.8B at 6 threads — identical
   within noise on all three shapes; details under Results.

## Hosting facts carried to 031 (read 2026-10-15)

- Release artifacts exist for every host: `apr-v0.70.3-aarch64-unknown-linux-gnu-cpu.tar.gz`
  (sha256 `8dd8b30a…`, glibc floor **2.30** < the 2.34 that `provided.al2023` needs, so no rebuild
  for Lambda) and `…-cuda.tar.gz` (sha256 `99a28cc6…`, links `libcublas.so.12`, embeds PTX for
  `sm_70`/`sm_75`/`sm_90` and code for `sm_89`/`sm_121`).
- A GPU host on arm64 exists: EC2 `g5g` (Graviton2 + NVIDIA T4G, **sm_75**; one GPU per instance up to
  4xlarge), on-demand about $0.42/h for `g5g.xlarge` in us-east-1 per cloudprice.net (verify on the
  EC2 pricing page). The release gate certified sm_89 and sm_121 only; sm_75 is unmeasured.
- Lambda Managed Instances "currently support only Lambda Event Functions"; multi-concurrent
  invocations per environment; scales on CPU utilisation, no cold starts, 3 instances by default,
  15 % fee; the capacity provider is the security boundary.
- Lambda response streaming: Function URLs, `InvokeWithResponseStream`, or API Gateway proxy
  integration; up to 200 MB; first 6 MB uncapped then 2 MB/s; custom runtimes via the Runtime API
  or the Lambda Web Adapter; **Function URLs do not stream inside a VPC** (use
  `InvokeWithResponseStream` through an interface endpoint). pmcp-agent is non-streaming; the
  platform transformer passes `stream` through.
- Bedrock Custom Model Import: `Qwen3ForCausalLM` / `Qwen3MoeForCausalLM` only; Qwen3.5 is not
  importable; Bedrock's own OpenAI surface is `…/openai/v1`.

## Results

**Verdict: VALIDATED ✓ — measured as asked, and the measurement says CPU hosting of an agent LLM
on these hosts is not viable at 0.70.3.** Every number below is from one `apr serve` alone on an
M4 Pro (10 P + 4 E cores, 52 GB), medians of two runs (`results/bench-*.md`, `results/SUMMARY.md`,
raw records in `results/bench-*.json`). Tool choice was correct in all 22 tool requests across the
six configs (`get_weather`, `convert_currency` among ten).

### Throughput and an agent turn

| config (RAYON_NUM_THREADS) | prefill tok/s | decode tok/s | RSS GB | 2-tool turn, 421 prompt tok | 10-tool turn, 937 tok | 2k-token handbook | 4k |
|---|---|---|---|---|---|---|---|
| 0.8B, 6 | 12.3 | 13.1 | 2.4 | **35 s** | 78 s | 154 s | 327 s |
| 0.8B, 14 | 16.5 | 17.9 | 2.4 | 26 s | 57 s | 118 s | 263 s |
| 2B, 6 | 5.9 | 6.1 | 2.8 (4.7 peak) | 76 s | 166 s | 325 s | skipped |
| 2B, 14 | 8.9 | 9.2 | 4.8 | 49 s | 108 s | 214 s | 445 s |
| 4B, 6 | 3.0 | 3.0 | 5.6 | **156 s** | 338 s | skipped | skipped |
| 4B, 14 | 4.2 | 4.4 | 5.6 | 103 s | 226 s | 469 s | skipped |

Three facts fall out of the table:

1. **Prefill runs at the decode rate, flat from 73 to 3,770 prompt tokens** (0.8B at 6 threads:
   13.3 → 12.0 → 12.1 → 12.4 → 11.6 tok/s). The prompt is paid like generated text, so an agent
   turn's cost is its prompt length: tool schemas (~170 tokens per two tools), instructions and the
   growing history. A pmcp.run agent with ten tools pays 78 s per turn on 0.8B and 338 s on 4B at the
   Lambda thread count. (Whether the pin's batched CPU prefill engages on this path: A/B below.)
2. **Threads buy little, model size costs everything.** 6 → 14 threads: 1.35× (0.8B), 1.5× (2B),
   1.4× (4B). 0.8B → 2B: 2.1× slower; 2B → 4B: 2× slower. Decode at 3–18 tok/s is memory-bound.
3. **Memory is not the constraint.** 2.4 / 4.8 / 5.6 GB peak RSS all fit a 10,240 MB Lambda; the
   loader keeps Q4_K weights quantised (`QuantizedTensorRef`); `--no-fp8-cache` changes nothing on
   CPU (A/B below), so the ~4× file-to-RSS ratio is working memory, not a cache to switch off.

### Turn-2 session reuse works on 0.70.3, and one foreign request destroys it

| config | turn 1 TTFT (421 tok) | turn 2, extending turn 1 (460 tok) | turn 2 after one foreign request | saving |
|---|---|---|---|---|
| 0.8B, 6 | 35.6 s | **3.6 s** | 35.2 s | 9.7× |
| 2B, 6 | 76.2 s | 7.6 s | 75.8 s | 10.0× |
| 4B, 6 | 153.9 s | 15.5 s | 153.6 s | 9.9× |
| 4B, 14 | 104.4 s | 10.8 s | 104.9 s | 9.8× |

Turn 2 prefills only its 39 new tokens (the assistant tool call + the tool result), exactly what
`qwen35_session.rs` promises and what #4274 reported broken on 0.69.3: **fixed by 0.70.3** on
`/v1/chat/completions`. The saving is ~10× in every config. But the state is one per process, keyed
on the previous prompt + completion: a single request from another conversation in between resets
it, and turn 2 pays the whole prompt again (35.2 s vs 3.6 s). In a shared endpoint serving several
agents the reuse rate is the probability that two consecutive requests belong to the same
conversation.

### Concurrency: the server serialises

| config | 1 request | 2 at once (wall / each) | 4 at once (wall / each) |
|---|---|---|---|
| 0.8B, 6 | 11.2 s | 20.7 s / 10.3, 20.7 | 31.2 s / 10.3, 20.6, 25.9, 31.2 |
| 4B, 6 | 48.4 s | 89.6 s / 44.8, 89.6 | 158.4 s / 45.5, 90.3, 113.5, 158.4 |

Wall time is the sum of the requests: `Qwen35Served.session` is a `std::sync::Mutex` and every
generation holds it. Aggregate decode throughput does not rise with concurrency (0.8B: 13 tok/s
alone, 6.2–8.2 tok/s per request at 2–4). A second agent's request waits for the first agent's
whole turn — and, by the previous section, then evicts its session state.

### A/B: prefill path and FP8 cache (0.8B, 6 threads, `tools/ab_prefill.sh`)

| variant (0.8B, 6 threads, reps=1) | short 73 tok TTFT | tools2 421 tok TTFT | tools10 937 tok TTFT | prefill tok/s | RSS MB |
|---|---|---|---|---|---|
| A default (session batched prefill, 64-token chunks) | 5.50 s | 35.9 s | 79.2 s | 11.7–13.3 | 2,251–2,298 |
| B `APR_QWEN35_SESSION_PREFILL=per-token` (server logs "prompts prefill one token at a time, not batched") | 5.51 s | 35.3 s | 77.9 s | 11.9–13.3 | 2,250–2,306 |
| C `--no-fp8-cache` (server logs "FP8 cache: DISABLED … saves ~1.5 GB") | 5.63 s | 36.2 s | 79.1 s | 11.6–13.0 | 2,249–2,323 |

The batched path **is** engaged by default (variant B had to be asked for and announced itself), and
it is **no faster than one token at a time on the CPU**: 79.2 s vs 77.9 s at 937 tokens, inside
run-to-run noise. Chunking amortises a Q4_K dequant per 64 rows on the GPU path; on this CPU path
the per-token cost stays whatever it is (the DeltaNet recurrence is sequential in t, and spike 020's
18× came from GEMM-ing the projections over the row and parallelising the recurrence per head —
neither visible here). `--no-fp8-cache` changed neither speed nor RSS (2.25–2.32 GB in all three):
the FP8 cache is a GPU artefact, so the "saves ~1.5 GB" does not apply on CPU and the 2.3 GB RSS
for a 0.53 GB file is something else (working buffers, the mmap'd file counted as resident).

### Graviton projection (est.)

Projected with the 3.2× M4 → Graviton2 ratio measured at 6 threads in spikes 021 and 026 on
GEMM-bound work; decode here is bandwidth-bound and the ratio is unmeasured for it, so these are
estimates until 031 measures. `T(turn) = prompt / prefill + output / decode`:

| config | 2-tool turn (390 + 38) | 10-tool turn (926 + 53) | 2k handbook (2000 + 64) |
|---|---|---|---|
| 0.8B, 6 threads | 35 s → **est. 111 s** | 79 s → est. 254 s | 167 s → est. 536 s |
| 4B, 6 threads | 143 s → est. 457 s | 326 s → est. 1,044 s | 688 s → est. 2,202 s |

pmcp-agent's default request timeout is 60 s (`HttpSourceOptions::DEFAULT_TIMEOUT`); pmcp.run's
gateway cap on the decide server is 30 s; Lambda's own ceiling is 900 s.

### Signal for the build

- **A CPU `apr serve` is not an agent LLM endpoint at 0.70.3**, on default Lambda least of all: the
  smallest certified model (2B) needs 76 s for a two-tool turn on six M4 threads before any Graviton
  penalty, and the model pmcp.run would actually want (4B+) needs minutes. Fargate (16 vCPU) and
  LMI (Graviton4, 16 vCPU) buy at most ~1.5× by the thread-scaling row.
- **The lever is upstream prefill, not hosting.** Prefill at the decode rate is the whole gap:
  #5305's sub-tickets (#4313 MMQ, #4376, #4442, #4450, #4483) target 0.71.x on GPU; nothing in the
  tracker targets the CPU path. Spike 020 showed an 18× batched CPU prefill on the hybrid (F32
  weights) — a Q4_K GEMM path for the CPU would be the contribution that changes this verdict.
- **GPU is the host class the release actually certifies** (CRUX smoke on sm_89 / sm_121). The arm64
  CUDA release binary (`…-cuda.tar.gz`, PTX for sm_70/75/90) makes an EC2 `g5g` (Graviton2 + T4G,
  sm_75, ~$0.42/h) on ECS/EKS the cheapest GPU candidate — unmeasured by any gate. That is a scope
  change for 031: from "which CPU host" to "GPU on ECS/EKS vs CPU on LMI", for the operator to decide.
- **If a CPU endpoint is kept for small, infrequent calls** (one process per conversation, 0.8B/2B):
  put one conversation per process to keep the 10× session reuse; never share one server between
  agents; cap `max_tokens`; raise the SDK timeout well past 60 s; and keep tool lists short — every
  schema token is paid at ~12 tok/s.
