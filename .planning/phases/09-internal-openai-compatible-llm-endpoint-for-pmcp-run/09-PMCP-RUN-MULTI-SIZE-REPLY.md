# Aprender reply to the pmcp.run change request: several sizes of the in-account endpoint at the same time

**DRAFT, not sent.** As of: 2026-10-18 (UTC), repo HEAD a17c4959a, branch gsd/phase-2-contract-gate. AWS state was read live on 2026-10-18 with read-only calls under the dev profile; repo files were read at that HEAD. This replies to `2026-10-18-pmcp-run-multi-size-change-request.md` (your message, stored verbatim beside this file).

## How to read this

Your three requested items are answered in order, then the "not changed" list, then the separate asks. Each answer says what we accept, what we change in your spec and why, and what you get back. Statuses as in our 2026-10-18 reply: VERIFIED means checked against a named source; PLANNED means we intend to build it and nothing exists yet; DECIDED ELSEWHERE means the answer is a policy call this reply does not make.

Nothing was deployed, changed on AWS, or written to GitHub to produce this reply.

## Answers at a glance

| Item | Answer in one line | Status |
|---|---|---|
| 1. Name | Accepted, with one amendment: a `variant` segment inside the recipe's own naming, not a free-form name. Default empty, so today's names stay. | PLANNED |
| 2. Model and sizing | Accepted for the model, amended for sizing: a per-variant pin file selects the model; memory and scratch disk stay at the Lambda maximum and are not inputs. | PLANNED |
| 3. Smoke test | Yes, through the recipe's own probe and the same API Gateway v2 event. Image and cold-start facts are below; the timing on the 2B depends on which CPU Lambda hands out. | PLANNED; facts VERIFIED |
| Not changed | Agreed on both points. | VERIFIED |
| Separate asks | Spike functions: both still exist and do not match; redeploy fresh. Fargate ownership, the GPU statement and the SageMaker shim are decisions not made here. | DECIDED ELSEWHERE |

## Item 1: the deployment name

**Answer:** Accepted. One amendment: the variable is a `variant` segment that the recipe places inside its own naming scheme, not a free-form `LLM_ENDPOINT_NAME`. The resulting function name for your example is `aprender-llm-endpoint-2b-dev`.

**Why the amendment.** Every resource the recipe creates is named from one stem today: the function, its IAM role, the ECR repository and the log group are `aprender-llm-endpoint-<env>`; the weights bucket is `aprender-llm-endpoint-weights-<account>-<env>`; the CodeBuild role and project are `aprender-llm-endpoint-codebuild-<env>` and `aprender-llm-endpoint-weights-<env>`. A free-form name would have to be re-derived into each of those, and a free-form name can be mistyped outside your allowlist. A variant segment keeps the `aprender-llm-` prefix under the recipe's control and keeps every derived name consistent.

**The rule.**
- `just llm-endpoint-deploy env=dev variant=2b` names the function `aprender-llm-endpoint-2b-dev`; the role, repository, log group, bucket and CodeBuild resources carry the same `2b-dev` suffix.
- `variant` defaults to empty. With no variant the names are exactly today's (`aprender-llm-endpoint-dev`, `aprender-llm-endpoint-prod`), so the existing 0.8B deployment is untouched by the change itself.
- Allowed variant: 1 to 14 characters, lowercase letters, digits and hyphens, no leading or trailing hyphen. This is narrower than your proposed set (no uppercase, no underscore, shorter) because the weights bucket is named from the same stem and S3 refuses uppercase and underscores and caps a bucket name at 63 characters. With the 30-character stem, the 12-digit account id and `prod`, 14 characters is what remains. Every other resource type allows more, so the bucket is the binding limit. The recipe refuses a variant outside the rule before any AWS call.
- Environment stays `dev` or `prod`.

**Isolation.** Two things make "a second deploy leaves the first untouched" true, and the second is the one that is not obvious from the AWS side:
- Every AWS resource is per name, as listed above, so a deploy or teardown under one name has no resource in common with another name. Nothing is shared, not even the ECR repository or the bucket.
- The recipe's local records are per name too. Today the ids directory (`deploy/llm-endpoint/.ids/`) and the pin file (`deploy/llm-endpoint/pin.env`) are per checkout; a teardown reads the ids directory to learn what to delete. A second name under the same checkout would overwrite the first name's records, and a later teardown would read the wrong ones. Each variant therefore gets its own ids directory and pin file; the empty variant keeps today's paths.

**What "confirmation" will mean.** We will not state isolation from a code reading. The acceptance test is: with the 0.8B deployed, deploy the `2b` variant and read the 0.8B function's configuration, image digest and environment keys back from Lambda; then deploy a throwaway variant, read both the 0.8B and the 2B back, tear the throwaway down, and read both back again. Every read-back equals the record taken before it or the test fails, and the 0.8B's read-backs must still carry its 2026-10-16 modification time. The 2B stays deployed throughout, since it is the function your provider row will select. The recipe's offline self-test gains the name rule as a case table in both polarities (accepted and refused names). Both results come to you as evidence files, the same way the 0.8B deployment record did.

**Status:** PLANNED. The names above are what we will build; nothing is built yet, and no date is offered.

## Item 2: model and sizing inputs

**Answer:** Accepted for the model. Amended for sizing: memory and ephemeral storage are not made inputs on the Lambda tier.

**Model.** The model is already two keys in the pin file: `GGUF_RUNG` and `GGUF_OBJECT` (today `qwen35-0.8b-q4km` and `Qwen3.5-0.8B-Q4_K_M.gguf`). The sha256 is never typed: the recipe reads it from the capability ladder for that rung and refuses to deploy if the ladder has no single pin. The 2B and 4B rungs both carry pins (`qwen35-2b-q4km`, `qwen35-4b-q4km`), so a 2B variant is a per-variant pin file with those two keys changed plus staged weights. The 0.8B's pin file is not touched.

**Why memory and scratch disk stay fixed.** The recipe sets 10,240 MB memory, a 10,240 MB scratch disk, a 900 s timeout and arm64, and refuses to proceed if Lambda reads back anything else. Those are the Lambda maxima, and memory is what buys CPU: 10,240 MB is the only size that gives the function six vCPUs, which is what the server's thread count (6) is set to. Any lower memory is strictly slower for the same model. The scratch disk holds the weights pulled from S3 at cold start: 2B is 1,280,835,840 bytes and 4B 2,740,937,888 bytes, both far inside 10 GB. So an input for either value could only make a Lambda deployment worse, and we would rather the recipe keep refusing a mis-sized function than accept one. If a smaller function is ever wanted for cost, that is a change we would make with the thread count at the same time, as one decision.

**Status:** PLANNED for the per-variant pin file; the model keys, the ladder pins and the fixed sizes are VERIFIED from the recipe and the ladder.

## Item 3: smoke test on the 2B

**What we will run.** Deploy `aprender-llm-endpoint-2b-dev` with the recipe, then the recipe's own measurement, which invokes the function through `aws lambda invoke` with the API Gateway v2 event in `deploy/llm-endpoint/events/` (the same shape the 0.8B measurement used) and records the client-side timings, the server's log lines and the CPU part the function landed on. The result comes back as the same evidence file shape as the 0.8B's. The 401 proof is also re-run, and is expected to record that the key is ignored, as it does today on the 0.8B.

**Image and cold start, stated now.**
- The container image is 154,427,965 bytes in ECR (tag `cpu-v0.71.4`, digest `f291c0a7…`), compressed as ECR stores it. It holds the `apr` binary, the Lambda Web Adapter and the entrypoint, and no weights. The same image serves every size; only the pin file and the staged weights differ.
- Weights are pulled from the per-name bucket into the scratch disk at every cold start and verified against the ladder hash before the server starts. 2B: 1,280,835,840 bytes; 0.8B, for comparison: 532,517,120 bytes.
- The only measured 2B cold start on this tier is spike 031's, on apr 0.70.3 (an older release than the current pin) and 10,240 MB: 76 s from zero to first answer, of which 29 s pulling the weights, 3 s loading and 39 s prefilling the two-tool prompt. Its two-tool turn was 246 s at 1.9 prefill tokens per second. The current pin has not been measured on the 2B; the smoke test will give the first number.

**The number to expect, and why it is not one number.** Lambda hands each environment a Graviton generation that nobody can choose. The 0.8B function landed on Graviton3 and prefills at 6.0 to 6.4 tokens per second; the spike's 2B landed on Graviton2 and prefilled at 1.9. Your 881-token prompt would be about 460 s of prefill on a Graviton2 environment and roughly a third of that on Graviton3, so a cold 2B call can pass or miss your 280 s transport limit on the same deployment from one day to the next. The recipe records the CPU part in every log line and in the evidence file, so each measurement says which host it got. We suggest treating the smoke test as the tool-call check it is, and holding the 2B timing table until the release noted next.

**The change that moves this.** On the CPU, `apr serve` has prefilled every prompt one token at a time, at the decode rate, which is why prompt size decides everything in your measurements. Upstream PR paiml/aprender#5710 (opened 2026-10-18, ours) wires the existing batched prefill kernel into the CPU session. Measured on a laptop at six threads, not on Graviton: the 0.8B's time to first token on a 421-token two-tool prompt fell from 35.0 s to 15.3 s, and the kernel runs 1.8× faster on the 4B; the 2B was not measured. The PR is not merged and no release date exists; our image pins release tags only, so it reaches the endpoint only once a tag carries it. Lambda arm64 numbers follow then.

**Status:** PLANNED; the image size, the weight sizes and the spike numbers are VERIFIED (ECR and S3 read on 2026-10-18; `.planning/spikes/031-openai-endpoint-aws-hosts/README.md`).

## Not changed

- No Function URL and no public endpoint: agreed. The deploy step creates none. The recipe does carry a separate URL step, and it refuses to run until the deployed image has proved a 401 without a key within the last 24 hours, which the current pin cannot; teardown removes any URL configuration regardless. The 2B smoke-test evidence will include the `get-function-url-config` not-found read your Task 7 asks for.
- The bearer key is ignored by the deployed build: agreed. #5451 is closed, #5544 carries the gate and no tag contains it. Our entrypoint still refuses to start unless `APR_API_KEY_HASH` is 64 hex characters, so a malformed hash cannot reach the fail-open case upstream reported as #5479. The key itself is never written anywhere by our side either.

## Separate asks

- **Spike functions.** `llm-spike-031-lambda-0-8b` and `llm-spike-031-lambda-2b` both still exist (state Active, last modified 2026-10-15, 10,240 MB arm64, weights in the spike bucket `llm-spike-031-<account>`). They do not match the current recipe: apr 0.70.3 against v0.71.4, no key hash, no model hash check, their own roles and bucket, and names outside your allowlist. Do not build on them. Redeploy under the new name; the spike functions will be removed in our own clean-up and nothing of yours should depend on them.
- **4B on Fargate.** No recipe exists; the only artifact is spike 031's one-task script on the old entrypoint. Which team writes it is a decision not made in this reply.
- **GPU policy statement.** Our recorded position is that no GPU-backed deployment of this endpoint runs until the CUDA-path routers are gated (#5478) or the deployed 401 proof is repeated on that path. A written agreement that the proxy Lambda alone satisfies that would change the position, and this reply does not change it.
- **SageMaker shim.** No `/ping` or `/invocations` route exists in the server and no shim is in the repo. Our technical view is that the same image on ECS over an EC2 GPU host is less work than a new container contract; the choice is not made here.

## What you get back, and when it exists

- The variable: `variant`, default empty, names as in Item 1. Stated now.
- Isolation: the acceptance-test evidence described in Item 1, when the recipe change lands. Not stated from a code reading.
- The 2B: the same image tag (`cpu-v0.71.4`) and the per-variant pin file naming `qwen35-2b-q4km`, the smoke-test evidence file with cold-start timeline and CPU part. When the deploy runs.
- No dates are offered for any of the three.

## What we ask of you

- Confirm the name `aprender-llm-endpoint-2b-dev` (variant before environment). Your plan's Task 7 writes `aprender-llm-endpoint-dev-2b`; the recipe will produce the first form.
- Keep the per-endpoint transport timeout in your provider row, as your Task 4 proposes; the 2B on a Graviton2 environment will need it.

## Evidence

- Naming today: `justfile:4010` (function and tags), `justfile:4115` (role, repository, log group), `justfile:4116` (bucket), `justfile:4259` and `justfile:4260` (CodeBuild role and project); environment restricted to dev or prod in four recipes (`justfile:3927` and following).
- Fixed sizes and the read-back refusal: `justfile:4035` to `justfile:4069`; threads: `justfile:4028`.
- Local records: `justfile:3903` (`LLM_ENDPOINT_IDS_DIR`, `LLM_ENDPOINT_PIN_FILE`); teardown reads them at `justfile:4582` and following.
- Model keys: `deploy/llm-endpoint/pin.env`; ladder pins: `contracts/model-capability-ladder-v1.yaml:273` (2B) and `:281` (4B); pin resolution: `deploy/llm-endpoint/ladder_pin.py`.
- Current deployment record: `deploy/llm-endpoint/.ids/function.json` (`aprender-llm-endpoint-dev`, apr v0.71.4, deployed 2026-10-16).
- Image size: `aws ecr describe-images` on 2026-10-18, repository `aprender-llm-endpoint-dev`, digest `sha256:f291c0a7…`: 154,427,965 bytes. Weight sizes: `aws s3api head-object` on the spike bucket (2B) and the dev weights bucket (0.8B), 2026-10-18.
- Spike functions: `aws lambda get-function-configuration` on both names, 2026-10-18.
- 2B cold start and turn: `.planning/spikes/031-openai-endpoint-aws-hosts/README.md`, section 5 table; 0.8B current-pin numbers: `09-LIVE-DEPLOY-EVIDENCE.json`.
- Upstream PR: https://github.com/paiml/aprender/pull/5710, and the measurement receipt in its description.
- GPU position: `deferred-items.md`, D-09-03 entry ("Do not run a GPU-backed deployment of this endpoint until these are gated or the deployed 401 proof is repeated on that path").
