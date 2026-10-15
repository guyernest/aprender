---
spike: 031
idea: internal-llm-endpoint
name: openai-endpoint-aws-hosts
type: comparison
validates: "Given one arm64 image (the v0.70.3 apr release binary + Lambda Web Adapter, GGUF pulled from S3 at start) deployed with the 021–023 pattern to (a) default Lambda 10,240 MB, (b) ECS Fargate 16 vCPU, (c) Lambda Managed Instances on Graviton4, when pmcp.run-shaped agent turns are sent, then cold start → first token, Graviton prefill/decode tok/s, seconds and dollars per agent turn, memory and the session-reuse behaviour are measured per host, so the security/cost-over-speed option is priced on real hardware"
verdict: VALIDATED
related: [021, 022, 023, 026, 029, 030]
tags: [aws, lambda, fargate, lmi, graviton, openai-api, qwen3.5]
---

# Spike 031: the internal LLM endpoint on AWS hosts, priced

## What This Validates

The operator's framing after 030: the internal endpoint is the option when **security or cost
outrank speed**, and it should deploy the way the decision models do (one image, weights in S3,
default Lambda / Fargate / Lambda Managed Instances). So this spike does not ask "is it fast"; it
asks what one pmcp.run agent turn **costs and takes** on each private host, on real Graviton
hardware, with the 029/030 request shapes, and whether the session reuse that makes turn 2 cheap
survives each host's execution model.

## Research

- **One image, every host** (021–023 pattern, adapted): `deploy/Dockerfile` = AL2023-minimal +
  the sha256-verified `apr-v0.70.3-aarch64-unknown-linux-gnu-cpu` binary (glibc floor 2.30) +
  AWS CLI v2 + the **Lambda Web Adapter** 0.9.1 as an extension. On Lambda the adapter is the
  runtime and proxies each invocation to `apr serve` on :8080 once `/health` answers; on Fargate
  the extension directory is ignored and the same container is a plain HTTP server. No custom
  `bootstrap` crate: the OpenAI API is already the server's own surface.
- **Weights in S3, pulled at start** (`deploy/entrypoint.sh`, timeline in the log): 021 measured
  ~80–95 MB/s per default-Lambda environment and 675–790 MB/s on Fargate; baking weights into an
  image was 7–18× worse. The bucket carries the three ladder-pinned GGUFs with their sha256 in
  object metadata.
- **Lambda facts** (docs read 2026-10-15): 10,240 MB → ~6 vCPU; ephemeral storage up to 10,240 MB
  (`/tmp` holds the GGUF); 900 s timeout; buffered responses ≤ 6 MB; response streaming via
  Function URLs or `InvokeWithResponseStream` (not Function URLs inside a VPC). The spike invokes
  with an API-GW v2 event, no public URL, so no auth surface is exposed (029: the GGUF server does
  not enforce `APR_API_KEY`).
- **LMI facts**: Event Functions only; multi-concurrent per environment; capacity provider is the
  security boundary; create the provider with its final `MaxVCpuCount`; `min = max = 1` after the
  first publish; hosts hidden from `DescribeInstances` without `IncludeManagedResources`; 15 % fee.
- Measurement on buffered Lambda: prefill from a `max_tokens: 1` request, decode from the
  difference to a `max_tokens: 64` request on the same shape; `LogType=Tail` gives Init Duration,
  Duration, Max Memory; the `[entry]` timeline in CloudWatch gives download and load.

## How to Run

```bash
cd .planning/spikes/031-openai-endpoint-aws-hosts
bash tools/aws_up.sh                                   # bucket, ECR, roles, SG, logs, cluster -> results/aws-ids.json
uv run --with boto3 python3 tools/upload_weights.py --profile ze-kasher-dev --bucket llm-spike-031-<acct> models/*.gguf
(cd deploy && docker buildx build --platform linux/arm64 --provenance=false -t <ecr>/llm-spike-031:cpu-v0.70.3 --push .)
uv run --with boto3 python3 tools/lambda_up.py create 0.8B
uv run --with boto3 python3 tools/remote_bench.py lambda llm-spike-031-lambda-0.8b --model 0.8B --label lambda-0.8b --cold --conc 2 --out results/bench-lambda-0.8b.json
uv run --with boto3 python3 tools/fargate_run.py 4B --vcpu 16 --mem-gb 32
uv run --with boto3 python3 tools/lmi.py up 4B && uv run --with boto3 python3 tools/lmi.py scale 1 && ... && uv run --with boto3 python3 tools/lmi.py down
bash tools/teardown.sh                                 # everything tagged spike=031
```

## What to Expect

Per host: a cold-start timeline (image pull / init, S3 pull, model load, first token), warm
prefill and decode tok/s on the Graviton generation the host handed out, the two-tool agent turn
and its turn-2 extension, two concurrent requests, Max Memory, and a dollars-per-turn figure from
the host's price.

## Investigation Trail

1. **Shared resources** (`tools/aws_up.sh`, `results/aws-ids.json`): bucket `llm-spike-031-816069129666`,
   ECR `llm-spike-031`, four roles, SG `sg-028b07efce1ecdc3d` (8080 from the client IP only), log
   groups, ECS cluster, default VPC `vpc-0f418c2e98c4ee551` with subnets in 1a/1b/1d/1f (1c and 1e
   excluded: LMI refused 1c in 023, 1e has no Fargate/Graviton capacity historically).
2. **The laptop uplink failed twice in a row**: boto3's gentle multipart upload (16 MB parts, 2
   concurrent) died at part 17 of the 0.8B GGUF with `ConnectionClosedError`, and the image push
   died at 190 of 312 MB with `use of closed network connection` while the upload ran. The image
   push alone succeeded on retry (150 MB compressed in ECR). Weights go through a **CodeBuild ARM
   job** (`tools/fetch_weights_codebuild.sh`) that downloads from Hugging Face inside AWS, checks
   each file against the ladder's sha256 pin, and copies to S3 — the uplink never carries them.
   (The CLI's `--source type=NO_SOURCE,buildspec=…` shorthand cannot carry a multi-line buildspec;
   the project is created from `--cli-input-json`.)
3. **Image**: AL2023-minimal + `awscli-2` (96 packages, the bulk of the 312 MB layer) + the 60 MB
   `apr` binary + Lambda Web Adapter 0.9.1; `apr --version` runs in the build (`apr 0.70.3
   (95f64ba14)`). One tag `cpu-v0.70.3` for every host.
4. Lambda function names allow `[a-zA-Z0-9-_]` only: the 0.8B function is `…-lambda-0-8b`.
5. **Default Lambda, 0.8B** (`results/bench-lambda-0.8b.*`): the environment is **Graviton3**
   (`cpu_part=0xd40`, Neoverse V1), 6 vCPU, 10.9 GB. Cold start to the first token of a 73-token
   prompt: **40.5 s** = 5.1 s reading the 60 MB binary from the lazily loaded image + 19 s S3 pull
   (532 MB at 28 MB/s on a fresh worker; a second fresh environment pulled it in 6.1 s = 88 MB/s,
   so the first-ever pull pays image-layer reads of the AWS CLI itself) + 2 s model load + ~11 s
   prefill; Init Duration 10.0 s (the adapter's async-init cap), Max Memory 2.3 GB. Warm: prefill
   **6.5 tok/s**, decode **6.4 tok/s** (M4 at 6 threads: 12.3 / 13.1 → Graviton3 is 1.9× slower,
   not the 3.2× projected for Graviton2). Two-tool turn 71.1 s (≈ $0.0095 at 10 GB); 10-tool
   prefill 100 s wall. **Session reuse works on Lambda**: turn 2 prefilled only its suffix
   (7.3 s vs 64.7 s) because the same environment served both turns. Two simultaneous requests:
   one warm (16.1 s), one spawned a second environment and took 117.8 s — Lambda's unit of
   concurrency is an environment, each with its own cold start and its own session state.
   Unexplained: the 10-tool request returned to the client at 99.8 s while its REPORT line says
   146.3 s Duration (billed), and the first of the two simultaneous requests waited ~47 s before
   its new environment even started — the environment looks busy to Lambda longer than the HTTP
   response takes (`results/bench-lambda-0.8b.json`, request e6265c1f).
6. **Fargate, 4B, 16 vCPU / 32 GB** (`results/fargate-fargate-4b-16vcpu.json`): the task landed on
   **Graviton4** (`cpu_part=0xd4f`, Neoverse V2) in us-east-1a. From `RunTask` to model ready
   **39.8 s**: provisioning 23.1 s, image pull 4.3 s, container start 3.1 s, then the 2.7 GB GGUF
   from S3 in **6.3 s (435 MB/s)** and ~3 s load. Warm: prefill **4.5 tok/s**, decode **4.5 tok/s**
   — the same as 14 M4 cores (4.2 / 4.4), so 16 Graviton4 vCPUs ≈ one M4 Pro here. Two-tool turn
   131 s; 10-tool prefill 114 s (937 tokens, 8.2 tok/s: the long prompt ran faster per token than
   the 421-token one, unlike on the M4). Turn 2 extended the session (10.9 s vs 94.4 s). Two
   simultaneous requests serialise (23.2 s, 46.0 s). Price $0.632/h while running; the session
   cost $0.13. The weights never touched the laptop: HF → CodeBuild → S3 → task.
7. **Lambda Managed Instances, 4B, 32 GB / 16 vCPU function** (`results/bench-lmi-4b.*`,
   `results/lmi-up.log`): the capacity provider went Active in 2 s; `create_function` with a
   **container image** was accepted (023 had only tried a zip); `$LATEST` is `ActiveNonInvocable`,
   which the `function_active_v2` waiter does not understand (5-min timeout, then publish by hand);
   version 1 went Active in **82.7 s** and launched **three `c9g.16xlarge`** (64 vCPU / 128 GB,
   `cpu_part=0xd84` Neoverse V3, $2.782/h each on-demand + 15 %) for a 16-vCPU function — the host
   is the unit again, 4× the function; `min = max = 1` applied in 5 s. Each environment loaded the
   model **at publish time**: S3 pull of 2.7 GB in 5.8–6.5 s (≈ 450 MB/s), model ready 7 s after
   start, before any request. First request 12.8 s (73-token prefill + the adapter's first
   readiness), two-tool turn **79.0 s**, 10-tool prefill 166 s (937 tokens, 5.6 tok/s), decode
   ≈ 6 tok/s; turn 2 extended the session (8.4 s). **LMI throttles instead of queueing**: a
   request sent right after a finished one got `TooManyRequestsException: The request could not be
   processed because of high CPU utilization`, and one of two simultaneous requests was rejected
   the same way in 0.3 s while the other took 17.9 s. `LogType=Tail` is rejected on LMI (023).
   Torn down in-session; one host 24/7 would be $2.78 × 1.15 × 730 ≈ **$2,335 / month** before
   a single token.
8. **Default Lambda, 2B** (`results/bench-lambda-2b.*`): this function's environment was a
   **Graviton2** (`cpu_part=0xd0c`, Neoverse N1) — same configuration as the 0.8B function, a
   different generation by Lambda's choice. Cold start to first token 76.0 s (init 9.9 s, S3 pull of
   1.3 GB in 28.6 s = 45 MB/s, model ready at 32 s, then a 73-token prefill at 1.9 tok/s). Warm:
   prefill **1.9 tok/s**, decode **1.9 tok/s**; two-tool turn **246 s** ($0.033); 10-tool prefill
   **506 s** ($0.067); turn 2 extended (25 s vs 271 s); Max Memory 4.8 GB. Two simultaneous
   requests: 53.8 s and 55.8 s, the second in a fresh environment (init 9.8 s). Session cost for
   the whole run: $0.175; the 0.8B run cost $0.054.

## Results

**Verdict: VALIDATED ✓ — one image deploys to all three hosts, every host serves pmcp.run's
agent turn end to end, and the option is now priced.** For "security or cost over speed", default
Lambda is the host: zero idle cost, about one to three cents per two-tool turn, a private
invocation surface, and turns of one to four minutes. Fargate is the steadier-traffic variant at
$0.63/h with the fastest CPU here. LMI is the wrong shape for this: a 64-vCPU host per function,
$2,335/month before a token, and it throttles rather than queues.

### One table

| host (us-east-1, arm64) | model | CPU handed out | from zero to first answer | 2-tool turn (421 tok + 38) | 10-tool prefill (937 tok) | prefill / decode tok/s | turn 2 (extends) | 2 at once | peak memory | $ per 2-tool turn | idle cost |
|---|---|---|---|---|---|---|---|---|---|---|---|
| default Lambda 10,240 MB | 0.8B | Graviton3 `0xd40` | **40.5 s** (init 10.0 + pull 19 + load 2 + prefill 11) | **71 s** | 100 s wall (146 s billed) | 6.5 / 6.4 | 7.3 s vs 64.7 s | 2nd environment: 118 s cold | 2.4 GB | **$0.0095** | $0 |
| default Lambda 10,240 MB | 2B | Graviton2 `0xd0c` | 76 s (pull 29 + load 3 + prefill 39) | 246 s | 506 s | 1.9 / 1.9 | 25 s vs 271 s | 54 s + 56 s (2nd environment) | 4.8 GB | $0.033 | $0 |
| Fargate 16 vCPU / 32 GB | 4B | Graviton4 `0xd4f` | **39.8 s** from `RunTask` (provision 23, pull 4, start 3, weights 6, load 3) | 131 s | 114 s | 4.5 / 4.5 | 10.9 s vs 94.4 s | 23 s, 46 s (serialised) | 64 GB host | $0.023 | $0.632/h while up |
| LMI 32 GB / 16 vCPU on `c9g.16xlarge` | 4B | Neoverse V3 `0xd84` | model ready 7 s after publish; no request-path cold start | 79 s | 166 s | ~5.7 / ~6 | 8.4 s vs 79 s | **1 of 2 throttled** (`high CPU utilization`) | 32 GB cgroup | $0.070 | **$3.20/h ≈ $2,335/mo** |

Dollar figures: Lambda arm64 $0.0000133334 per GB-s at 10 GB; Fargate $0.03238/vCPU-h + $0.00356/GB-h;
LMI c9g.16xlarge $2.782/h on-demand + 15 %. Session totals: Lambda $0.23, Fargate $0.13, LMI ≈ $1.

### What the numbers say

1. **The deployment pattern holds.** The same `cpu-v0.70.3` image ran as a Lambda function, an LMI
   function (image packaging accepted) and a Fargate task, with the Lambda Web Adapter doing the
   invocation plumbing and `apr serve`'s own OpenAI surface doing the rest. Weights stage from
   Hugging Face into S3 inside AWS in seconds; the laptop never carries them. This is the
   decider pattern (021–023, 026) with the release binary in place of a custom crate.
2. **Cold start is 40–76 s and is made of four parts** that each host pays differently: reading the
   image lazily (5 s for the 60 MB binary on Lambda; the AWS CLI's own files slowed the first S3
   pull to 28–45 MB/s), the S3 pull (88 MB/s on a second Lambda environment; 435–470 MB/s on
   Fargate and LMI), the model load (1–3 s), and the first prefill, which at Graviton speeds is as
   long as the rest. Fargate's 40 s is mostly provisioning (23 s); LMI's is paid at publish.
3. **Graviton speed is a lottery on Lambda.** Two identical functions got a Graviton3 and a
   Graviton2; the generation alone is a 2–3× factor on top of the model size. Fargate gave
   Graviton4 and LMI a Neoverse V3 host. On Graviton3 the 0.8B runs at half the M4's six-thread
   rate (6.5 vs 12.3 tok/s), not the third the 3.2× Graviton2 ratio predicted.
4. **Session reuse survives every host**: turn 2 paid only its new tokens on Lambda (same warm
   environment), Fargate (one task) and LMI (one environment) — 9–11× cheaper than turn 1. It is
   lost the moment another conversation reaches the same process (030), and on Lambda the second
   concurrent agent gets a fresh environment (a 40–120 s cold start) with its own state — which
   is also the natural "one conversation per process" layout, if requests from one conversation
   keep landing on the same warm environment.
5. **Concurrency**: Lambda scales by environments (cold each); Fargate serialises inside the task;
   LMI with one environment **rejects** the second request with `TooManyRequestsException … high CPU
   utilization` instead of queueing it. Any front door has to queue per conversation itself.
6. **Memory is a non-issue** on every host (2.4 / 4.8 / ~6 GB); vCPU and generation are what you buy.

### The operator's prefix idea, priced from these runs

Every agent turn re-sends system prompt + tool schemas + history; the stable part is 300–900 of the
first ~1,000 tokens. The turn-2 rows are the upper bound of caching it: when the server already
holds the prefix, a turn costs its suffix — 7.3 s instead of 71 s on Lambda-0.8B, 8.4 s instead of
79 s on LMI-4B, 10.9 s instead of 131 s on Fargate-4B. Two things would deliver that for turn 1 of
every conversation, not just turn 2:

- **Server side (upstream aprender)**: a prefix cache keyed by token prefix with more than one
  entry (today: one state, "extends the previous prompt + completion or resets"), plus a
  prefill-only request (`max_tokens: 0`) so a front door can warm the system + tools prefix without
  generating. Upstream tracks the GPU analogue as #4214 ("no prompt-prefix reuse"); the CPU path has
  no ticket. This is the single largest lever measured in this idea — larger than any host choice.
- **Platform side (pmcp.run)**: pin an agent's tool list, render the schemas once into a stable
  prefix (identical bytes every turn — the template is deterministic), and route one conversation
  to one process. With the server feature above, a 10-tool turn on Lambda-0.8B drops from 100 s to
  roughly 10 s; without it, pinning only buys determinism.

### Signal for the build

- **Ship the option on default Lambda**: 10,240 MB, 10 GB ephemeral, image + adapter, weights in
  S3, invoked privately; start with 0.8B (CRUX-uncertified but correct in every run here) or 2B
  (certified) and state the latency honestly: 1–4 minutes per turn, 40–76 s first turn. Raise the
  SDK timeout (60 s default) and the gateway cap for this provider.
- **Fargate for steady traffic**: one task per tenant or conversation at $0.63/h, 40 s from zero,
  the 4B at 131 s per turn; scale to zero behind the 022 MCP-Task front door.
- **Not LMI** for a single CPU model: $2,335/month minimum, 64-vCPU granularity, throttling.
- **The GPU host remains the performance option** (certified sm_89/sm_121; arm64 CUDA release
  binary has sm_75 PTX for `g5g`); it was not deployed in this spike and is the next one if speed
  is ever the priority.
- **Left running**: the two Lambda functions (no idle cost), the ECR image and the S3 weights
  (cents per month), so the deployment is repeatable; `bash tools/teardown.sh` removes them.
