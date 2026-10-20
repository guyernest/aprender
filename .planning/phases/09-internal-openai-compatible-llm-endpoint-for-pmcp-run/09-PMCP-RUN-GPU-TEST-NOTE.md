# Note for the pmcp.run dev team: the GPU instance for your test

DRAFT, NOT SENT. From Aprender, 2026-10-19. This answers your request to run the same tests on a GPU with the same harness. **The instance reports READY** (checked 2026-10-20 01:05 UTC); you can start. This note says what to expect.

## What it is

| | |
|---|---|
| Function name | `aprender-llm-endpoint-dev-gpu-4b` (passes your `aprender-llm-` allowlist) |
| Provider row | `lambda://aprender-llm-endpoint-dev-gpu-4b/v1/chat/completions` |
| Model | Qwen3.5-4B Q4_K_M (ladder-pinned, sha256 checked on the host) |
| Host | one `g5.xlarge`: NVIDIA A10G (22.9 GiB), 4 vCPU, us-east-1b, private subnet address |
| Server | the released `apr` v0.71.4 x86_64 CUDA build, `--backend cuda` |
| In front | a small proxy Lambda in the VPC. It takes the API Gateway v2 event your harness already sends and returns the same response shape |
| Lifetime | self-terminates at **04:00 UTC on 2026-10-20 (21:00 Pacific, 2026-10-19)**; we tear it down sooner when you say you are done |

The host is persistent, so there is no cold start to measure. We send one warm-up request after the server starts, so your first call does not pay the one-time kernel compilation (about 14 s on our earlier probe).

## What the proxy lets through

`POST /v1/chat/completions`, `GET /v1/models`, `GET /health`. Everything else returns 404 from the proxy and never reaches the host. There is no streaming. The proxy adds no authentication and logs no request or response bodies.

## How it is protected, and what that does not cover

- There is **no Function URL** and no resource policy on the proxy. Who can call it is decided by IAM alone.
- The host's security group admits TCP 8080 from the proxy's security group and nothing else (no CIDR rules). Nothing else can reach the server.
- **The server does not check the bearer key.** The CUDA-path routers ignore it (paiml/aprender#5478, open), and the deployed CPU build ignores it too. Any key value works. Please do not send a real key, and do not record one.
- We have **not** run the 401 proof on this path. This arrangement, the proxy as the only gate, is agreed for this short test only. It is not the written agreement you asked for, and our GPU position is unchanged: no GPU-backed deployment until those routers are gated or the proof is repeated.

## What we know, and what we do not

Known:
- **The 4B runs on the A10G through the proxy.** We sent one chat and one tool-call request through `aprender-llm-endpoint-dev-gpu-4b` after READY. The chat answered `ready`; the tool-call request returned a `get_weather` tool call (`finish_reason: tool_calls`). Both responses carried `used_gpu: true`. Wall time at the caller, proxy included: 1.0 s and 1.6 s. The server's own timings: prefill about 1,460 tokens/s on the tool-call prompt and decode about 47 tokens/s. Two requests, one host; use them as a smoke check, not as figures.
- The host answers `GET /health` through the proxy (`compute_mode: gpu`, `model_loaded: true`), and a blocked path (`/v1/gpu/status`) is refused by the proxy with a 404.
- From our earlier probe on the same instance type with the 0.8B and 2B: one request with 278 prompt tokens took 204 ms to prefill (about 1,360 tokens/s) and decoded at about 149 tokens/s.

**Not known:**
- Concurrency on the GPU path. On the CPU path the server serializes requests. We have not measured the GPU path, so plan your first run at concurrency 1 and tell us what you see.
- Tool-call reliability beyond one request. The 4B made the call once; in our earlier probe the 2B called the tool and the 0.8B answered in prose.
- Cost per agent turn. That is what your 72 calls per model will measure.

## Cost, for your report

- **Idle cost: $1.006/h** for the `g5.xlarge` (on-demand, us-east-1, from the AWS price list today), plus about $0.013/h for its 120 GB gp3 volume. It bills from launch (18:00 Pacific) whether or not anyone calls it.
- The proxy Lambda bills per call at 256 MB on arm64, a small fraction of a cent per call for these durations.
- A per-call figure at a stated utilization is `$1.006/h ÷ (calls per hour × utilization)`; please state the utilization you assume. A GPU host that is called a few times an hour costs far more per call than a CPU Lambda; one that is busy costs less.

## What we need from you

1. **Invocation is settled:** `arn:aws:iam::816069129666:user/Guy` can already invoke the function through its group, so no grant was added and the function still has no resource policy. A role for the platform path is a separate step when you want it.
2. **Tell us when you start and when you finish.** You said under an hour; the instance stops itself at 04:00 UTC regardless.
3. **Tell us before you launch any GPU instance in this account** (you said you will not). The quota is 8 vCPUs and this host uses 4.
4. **Tell us when you are done,** and send us your raw per-call results. We will tear it down and confirm every resource is gone.
5. **Other sizes:** you said only the 4B for now. Only the 4B is staged. A 9B would fit this card in principle but is not staged and has not run on it; the 27B and larger need more than this quota allows.

## Not changed

The 0.8B and 2B CPU functions are untouched. Nothing here changes the CPU test you are running.
