# Note for the pmcp.run dev team: several endpoint sizes side by side

From Aprender, 2026-10-18. This answers your change request of the same date. The longer reply with evidence references is `09-PMCP-RUN-MULTI-SIZE-REPLY.md` beside this file.

## Where it stands

The recipe change is built, tested offline (75 self-test rows against shims) and used live on 2026-10-19. It is not merged to our main branch. `aprender-llm-endpoint-dev-2b` is deployed and stays up, reserved concurrency 1. Evidence files: `09-VARIANT-ISOLATION-EVIDENCE.json` and `09-2B-SMOKE-EVIDENCE.json`.

## What you can rely on now

- **The variable** is `variant`, default empty. With no variant the names are exactly today's: `aprender-llm-endpoint-dev` and `aprender-llm-endpoint-prod`. The existing 0.8B is not touched by the change.
- **The 2B function name** is `aprender-llm-endpoint-dev-2b`, the form your plan writes: environment first, then the variant. Your provider row can be `lambda://aprender-llm-endpoint-dev-2b/v1/chat/completions`.
- **The variant rule:** 1 to 14 characters, lowercase letters, digits and hyphens, no leading or trailing hyphen, and not `codebuild` or ending in `-codebuild`. It is narrower than your proposed character set because the weights bucket is named from the same stem and S3 refuses uppercase and underscores and caps names at 63 characters. The name always starts with `aprender-llm-`.
- **Everything is per name:** function, IAM role, ECR repository, log group, weights bucket, CodeBuild project and role, and the recipe's local records. A deploy or teardown under one name has no resource in common with another.
- **The model** is a per-variant pin file naming the ladder rung and the GGUF object; the sha256 comes from the capability ladder and is never typed. The 2B uses `qwen35-2b-q4km`.
- **Memory and scratch disk are not inputs.** They stay at the Lambda maximum (10,240 MB each, 900 s, arm64) because 10,240 MB is the only size that gives the six vCPUs the server is set to use, and the recipe refuses a function that reads back anything else.

## Measured

- **Isolation holds.** With the 0.8B deployed, we deployed the 2B, then a throwaway variant, and tore the throwaway down. Four read-backs of the 0.8B are field-for-field equal (still modified 2026-10-16), two of the 2B are equal, and the throwaway reads as not found.
- **The 2B answers a tool call.** One two-tool chat completion returned `finish_reason: tool_calls` with `get_weather` through the same API Gateway v2 event the 0.8B takes.
- **Timing, one sample on Graviton2 (CPU part `0xd0c`):** cold first call (1 token) 62 s; two-tool turn 245 s at $0.033; a turn that extends the session 26 s; prefill 1.87 and decode 1.89 tokens per second. The 0.8B landed on Graviton3 and is about 3x faster. Lambda assigns the CPU part, so another environment of the same function can differ.
- **That is inside your 280 s transport limit by 35 s**, with our two-tool prompt. Your 881-token prompt is larger. Treat the 2B as at the edge on Graviton2.
- **Image and weights:** image 154,429,272 bytes (tag `cpu-v0.71.4`, no weights); 2B weights 1,280,835,840 bytes, pulled from S3 at each cold start. Test spend $0.10.
- **Auth:** the 401 proof returns `NOT PROVEN` (status 200 without a key), as on the 0.8B. No Function URL exists on the 2B.

## Unchanged

No Function URL, ever. The deployed build still ignores the bearer key (#5451 closed; the release carrying #5544 has no date).

## Still open, and not decided in this note

- 4B on Fargate: no recipe exists, and which team writes it is undecided.
- GPU: our position stays that no GPU deployment runs until the CUDA routers are gated (#5478) or the 401 proof is repeated on that path. Your proposal that the proxy Lambda is the only gate has not been agreed.
- SageMaker `/ping` and `/invocations` shim: not built, not decided.
- The spike functions `llm-spike-031-lambda-0-8b` and `-2b` do not match the recipe. Do not build on them.

## What we need from you

Nothing blocks us. Keep a per-endpoint transport timeout in your provider row, as your Task 4 proposes; the 2B will need it.
