# Deploying the Internal LLM Endpoint on AWS (one image; Lambda, Fargate, LMI; priced)

The decider pattern (021–023, 026) applied to `apr serve`: one arm64 image, weights in S3, invoked
privately, measured on real Graviton hardware in us-east-1 under profile `ze-kasher-dev`. Read this
before deploying the endpoint for pmcp.run or quoting a cost per turn.

## Requirements

From idea `internal-llm-endpoint`:
- **The internal endpoint is the option when security or cost outrank speed.**
- **Deploy with the decider pattern** (operator, 2026-10-15): the same one-image / S3-weights /
  Lambda–Fargate–LMI shape used for Kev and Laya, so the LLM endpoint and the decision models ship
  through one mechanism.
- **Use the release artifacts at the cited tag and the ladder-certified GGUFs.**
- **Bedrock is a provider, not a host** for aprender or Qwen3.5.

## How to Build It

### 1. The image (`sources/031-*/deploy/`)
```dockerfile
FROM public.ecr.aws/awsguru/aws-lambda-adapter:0.9.1 AS lwa
FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal
RUN microdnf install -y awscli-2 procps-ng && microdnf clean all
COPY --from=lwa /lambda-adapter /opt/extensions/lambda-adapter
COPY apr /usr/local/bin/apr            # apr-v0.70.3-aarch64-unknown-linux-gnu-cpu, sha256-verified; glibc floor 2.30
COPY entrypoint.sh /entrypoint.sh
ENV AWS_LWA_PORT=8080 AWS_LWA_READINESS_CHECK_PATH=/health AWS_LWA_ASYNC_INIT=true PORT=8080 MODEL_LOCAL=/tmp/model.gguf RAYON_NUM_THREADS=6 HOME=/tmp
ENTRYPOINT ["/entrypoint.sh"]           # pulls $MODEL_S3_URI to /tmp, logs a [entry] timeline + cpu_part, exec apr serve run --host 0.0.0.0
```
The Lambda Web Adapter is the runtime on Lambda / LMI (proxies each invocation to :8080 once `/health`
is 200; `ASYNC_INIT` lets download + load outrun the 10 s init phase) and inert on Fargate. No custom
`bootstrap` crate: the OpenAI API is the server's own surface. `tools/build_image.sh` extracts the
binary from the release tarball (`deploy/apr` is gitignored, 57 MB) and `docker buildx
--platform linux/arm64 --provenance=false --push`. One tag for every host.

### 2. Weights: HF → CodeBuild → S3, never the laptop
The laptop uplink dropped a gentle boto3 multipart upload (part 17) and a `docker push` (190/312 MB)
within an hour. `tools/fetch_weights_codebuild.sh` creates an ARM CodeBuild project from
`--cli-input-json` (the `--source type=…,buildspec=…` shorthand cannot carry a multi-line spec) whose
single `- |` command block downloads each GGUF from Hugging Face, checks it against the ladder's
sha256 pin, and `aws s3 cp`s it: 0.5 / 1.3 / 2.7 GB in 3 / 3 / 10 s. CodeBuild runs each
`commands:` entry in its own shell, so a helper function and its calls share one block.

### 3. Shared resources (`tools/aws_up.sh` → `results/aws-ids.json`)
Bucket `llm-spike-031-<acct>`, ECR `llm-spike-031`, roles (`-lambda`: basic + VPC exec + S3 read;
`-ecs-exec`; `-ecs-task`: S3 read; `-lmi-operator`: `AWSLambdaManagedEC2ResourceOperator`;
`-codebuild`), SG with 8080 from the client IP only, log groups (7-day retention), ECS cluster,
default-VPC subnets excluding us-east-1c (LMI refused it in 023) and 1e. Everything tagged `spike=031`;
`tools/teardown.sh` removes it all (`KEEP_BUCKET=1` keeps the weights).

### 4. Hosts
- **Default Lambda** (`tools/lambda_up.py create 0.8B|2B`): image package, arm64, 10,240 MB (→ 6 vCPU),
  `EphemeralStorage 10240` (`/tmp` holds the GGUF), 900 s, `MODEL_S3_URI`, `RAYON_NUM_THREADS=6`.
  Function names forbid `.` (`0.8B` → `0-8b`). Invoke with an **API-GW v2 event** through the
  adapter (`tools/remote_bench.py lambda <fn>`): no Function URL, no public surface (029: the GGUF
  server does not enforce `APR_API_KEY`). `LogType=Tail` gives Init Duration / Duration / Max Memory.
- **Fargate** (`tools/fargate_run.py 4B --vcpu 16 --mem-gb 32`): registers the task definition (ARM64,
  30 GiB ephemeral), `RunTask` with a public IP in the SG, waits for :8080, benches over HTTP
  (streamed, TTFT direct), pulls the `[entry]` timeline from CloudWatch, stops the task.
- **Lambda Managed Instances** (`tools/lmi.py up 4B --mem=32768 --gib-per-vcpu=2.0`, `publish`,
  `scale 1`, `census`, `down`): capacity provider (arm64, `MaxVCpuCount` set to its FINAL value at
  creation — 023), **container-image packaging accepted**, `PerExecutionEnvironmentMaxConcurrency 1`;
  `$LATEST` is `ActiveNonInvocable` (the `function_active_v2` waiter times out — publish a version and
  wait on the version's state); `LogType=Tail` is rejected (take client wall time); `census` lists the
  hidden hosts (`IncludeManagedResources=True`). Tear down in-session and re-run `census` until empty.

### 5. What was measured (2026-10-15, apr 0.70.3, Q4_K_M)
| host (us-east-1, arm64) | model | CPU handed out | from zero to first answer | 2-tool turn (421 + 38) | 10-tool prefill (937) | prefill / decode tok/s | turn 2 (extends) | 2 at once | memory | $ per 2-tool turn | idle |
|---|---|---|---|---|---|---|---|---|---|---|---|
| default Lambda 10,240 MB | 0.8B | Graviton3 `0xd40` | **40.5 s** (init 10.0 + pull 19 + load 2 + prefill 11) | **71 s** | 100 s wall (146 s billed) | 6.5 / 6.4 | 7.3 s vs 64.7 s | 2nd environment, 118 s cold | 2.4 GB | **$0.0095** | $0 |
| default Lambda 10,240 MB | 2B | Graviton2 `0xd0c` | 76 s (pull 29 + load 3 + prefill 39) | 246 s | 506 s | 1.9 / 1.9 | 25 s vs 271 s | 54 + 56 s (2nd environment) | 4.8 GB | $0.033 | $0 |
| Fargate 16 vCPU / 32 GB | 4B | Graviton4 `0xd4f` | **39.8 s** from `RunTask` (provision 23, pull 4, start 3, weights 6, load 3) | 131 s | 114 s | 4.5 / 4.5 | 10.9 s vs 94.4 s | 23 s, 46 s (serialised) | 64 GB host | $0.023 | $0.632/h while up |
| LMI 32 GB / 16 vCPU on `c9g.16xlarge` | 4B | Neoverse V3 `0xd84` | model ready 7 s after publish; no request-path cold start | 79 s | 166 s | ~5.7 / ~6 | 8.4 s vs 79 s | **1 of 2 throttled** (`high CPU utilization`) | 32 GB cgroup | $0.070 | **$3.20/h ≈ $2,335/mo** |

Prices: Lambda arm64 $0.0000133334/GB-s; Fargate $0.03238/vCPU-h + $0.00356/GB-h; LMI `c9g.16xlarge`
$2.782/h + 15 % (c9g.4xlarge, the right-sized 16 vCPU, is $0.696/h but LMI chose the 64-vCPU host).
The whole spike cost under $2.

### 6. Decision
- **Ship the cost/security option on default Lambda**: zero idle cost, 1–3 cents per two-tool turn,
  private invocation, turns of 1–4 minutes and a 40–76 s first turn. 0.8B (correct in every run,
  CRUX-uncertified) or 2B (certified). Raise the SDK's 60 s timeout and the gateway cap for this
  provider; queue per conversation in the front door.
- **Fargate for steady traffic**: one task per tenant or conversation at $0.63/h, 40 s from zero, the
  fastest CPU here; scale to zero behind the 022 MCP-Task front door.
- **Not LMI** for a single CPU model: 64-vCPU host granularity, $2,335/month minimum, throttling.
- **GPU** remains the performance option: certified sm_89 / sm_121; the arm64 CUDA release binary
  (`…-cuda.tar.gz`, links `libcublas.so.12`, PTX for sm_70/75/90) fits EC2 `g5g` (Graviton2 + T4G,
  sm_75, ≈ $0.42/h for `g5g.xlarge`) on ECS/EKS — not deployed, unmeasured.

## What to Avoid
- **Moving weights or images over the laptop uplink** (3.7 MB/s, drops): stage in AWS.
- **Comparing two Lambda functions' latencies without `cpu_part`**: identical functions got
  Graviton3 and Graviton2 (2–3×). Record it in every response or log line.
- **Function URLs** for this endpoint (no server auth; and they do not stream inside a VPC — use
  `InvokeWithResponseStream` through an interface endpoint if streaming is needed).
- **LMI for one model**: it also rejects, not queues, a request that arrives while the one
  environment's CPU is saturated.
- **Trusting the REPORT `Duration` as the client's wait, or vice versa**: they disagreed by up to 46 s
  on long requests through the adapter (10-tool prefill: 99.8 s wall, 146.3 s billed). Report both.
- Baking weights into the image (021: 7–18× worse) or reading `$?` through a pipe.

## Constraints
- Default Lambda: 10,240 MB → 6 vCPU; ephemeral ≤ 10,240 MB; 900 s; buffered responses ≤ 6 MB;
  first-ever pull of an image on a worker is slow (5 s to read the 60 MB binary, 28–45 MB/s S3 pull
  while the CLI's files page in; 88 MB/s on a second environment). Each concurrent agent = a new
  environment with its own cold start and its own session state.
- Fargate arm64: 16 vCPU / 120 GB max; 23 s provisioning per wake-up; S3 at 435 MB/s.
- LMI: Event Functions only; image packaging accepted; `c9g.16xlarge` hosts for a 16-vCPU function;
  no cold start on the request path (environments load at publish); 15 % fee; hosts hidden from
  `DescribeInstances` by default; `aws-cli` 2.29 lacks the LMI verbs (boto3 ≥ 1.43 via `uv run`).
- Bedrock Custom Model Import: `Qwen3ForCausalLM` / `Qwen3MoeForCausalLM` only; Bedrock's OpenAI
  surface is `https://bedrock-runtime.<region>.amazonaws.com/openai/v1`.
- Left in the account after the spike: the two Lambda functions, the ECR image and the S3 weights
  (idle-free or cents per month); `bash tools/teardown.sh` removes them.

## Origin
Synthesized from spike 031 (VALIDATED), on the pattern of 021–023 and 026.
Source files: `sources/031-openai-endpoint-aws-hosts/` (Dockerfile, entrypoint, every driver, results
JSON and tables, `aws-ids.json`). Build and transfer logs stay in `.planning/spikes/031-*/results/`.
