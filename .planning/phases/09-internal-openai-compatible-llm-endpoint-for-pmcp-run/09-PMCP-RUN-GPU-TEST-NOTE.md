# Note for the pmcp.run dev team: the GPU instance for your test

DRAFT, NOT SENT. From Aprender, 2026-10-19. This answers your request to run the same tests on a GPU with the same harness. **Do not start until we tell you the instance reports READY**; this note says what to expect.

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

Known, from our earlier probe on the same instance type (0.8B and 2B): the CUDA build starts and serves on an A10G; one request with 278 prompt tokens took 204 ms to prefill (about 1,360 tokens/s) and decoded at about 149 tokens/s. One request, one host.

**Not known:**
- **The 4B has not run on this GPU yet.** If it fails to load, `check.sh` will report it before you start and we will tell you.
- Concurrency on the GPU path. On the CPU path the server serializes requests. We have not measured the GPU path, so plan your first run at concurrency 1 and tell us what you see.
- Tool-call reliability. In our probe the 2B called the tool and the 0.8B answered in prose, one sample each.
- Cost per agent turn. That is what your 72 calls per model will measure.

## Cost, for your report

- **Idle cost: $1.006/h** for the `g5.xlarge` (on-demand, us-east-1, from the AWS price list today), plus about $0.013/h for its 120 GB gp3 volume. It bills from launch (18:00 Pacific) whether or not anyone calls it.
- The proxy Lambda bills per call at 256 MB on arm64, a small fraction of a cent per call for these durations.
- A per-call figure at a stated utilization is `$1.006/h ÷ (calls per hour × utilization)`; please state the utilization you assume. A GPU host that is called a few times an hour costs far more per call than a CPU Lambda; one that is busy costs less.

## What we need from you

1. **Which principal will invoke the function?** With no resource policy, only IAM principals allowed `lambda:InvokeFunction` on it can call it. Tell us the role or account and we will add the permission.
2. **When you will start and how long you expect to run.** We will tell you when it is READY and do not want it billing overnight.
3. **Tell us before you launch any GPU instance in this account.** The quota is 8 vCPUs and this host uses 4.
4. **Tell us when you are done,** and send us your raw per-call results. We will tear it down and confirm every resource is gone.
5. **Which other sizes you want.** Only the 4B is staged. A 9B would fit this card in principle but is not staged and has not run on it; the 27B and larger need more than this quota allows.

## Not changed

The 0.8B and 2B CPU functions are untouched. Nothing here changes the CPU test you are running.
