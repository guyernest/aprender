# Note for the pmcp.run dev team: several endpoint sizes side by side

From Aprender, 2026-10-18. This answers your change request of the same date. The longer reply with evidence references is `09-PMCP-RUN-MULTI-SIZE-REPLY.md` beside this file.

## Where it stands

The recipe change is built and tested offline on our working branch. It is not merged, and nothing has been deployed to AWS for it. The live steps (the 2B deploy and the isolation test) start only when our operator says go. No dates.

## What you can rely on now

- **The variable** is `variant`, default empty. With no variant the names are exactly today's: `aprender-llm-endpoint-dev` and `aprender-llm-endpoint-prod`. The existing 0.8B is not touched by the change.
- **The 2B function name** is `aprender-llm-endpoint-dev-2b`, the form your plan writes: environment first, then the variant. Your provider row can be `lambda://aprender-llm-endpoint-dev-2b/v1/chat/completions`.
- **The variant rule:** 1 to 14 characters, lowercase letters, digits and hyphens, no leading or trailing hyphen, and not `codebuild` or ending in `-codebuild`. It is narrower than your proposed character set because the weights bucket is named from the same stem and S3 refuses uppercase and underscores and caps names at 63 characters. The name always starts with `aprender-llm-`.
- **Everything is per name:** function, IAM role, ECR repository, log group, weights bucket, CodeBuild project and role, and the recipe's local records. A deploy or teardown under one name has no resource in common with another.
- **The model** is a per-variant pin file naming the ladder rung and the GGUF object; the sha256 comes from the capability ladder and is never typed. The 2B uses `qwen35-2b-q4km`.
- **Memory and scratch disk are not inputs.** They stay at the Lambda maximum (10,240 MB each, 900 s, arm64) because 10,240 MB is the only size that gives the six vCPUs the server is set to use, and the recipe refuses a function that reads back anything else.

## What is not proven yet

- **Isolation.** We will not state it from a code reading. The test: with the 0.8B deployed, deploy the 2B and read the 0.8B back from Lambda, deploy a throwaway variant and tear it down, then read both back again. Every read-back must equal the one before it. You get the evidence files.
- **The 2B smoke test.** One tool-calling chat completion through the recipe's own probe, with the same API Gateway v2 event the 0.8B used, plus the cold-start timeline. Invocation spend is capped at $3.
- **The 2B timing depends on the CPU Lambda hands out.** The 0.8B landed on Graviton3 and prefills at about 6 tokens per second; spike 031's 2B landed on Graviton2 at 1.9. At your 881-token prompt that is roughly 460 s of prefill on Graviton2, so a cold 2B call can miss your 280 s transport limit on one day and pass on the next. The recipe records the CPU part with every measurement.
- **Image and weights:** the image is 154,427,965 bytes (tag `cpu-v0.71.4`, no weights); the 2B weights are 1,280,835,840 bytes, pulled from S3 at each cold start.

## Unchanged

No Function URL, ever. The deployed build still ignores the bearer key (#5451 closed; the release carrying #5544 has no date).

## Still open, and not decided in this note

- 4B on Fargate: no recipe exists, and which team writes it is undecided.
- GPU: our position stays that no GPU deployment runs until the CUDA routers are gated (#5478) or the 401 proof is repeated on that path. Your proposal that the proxy Lambda is the only gate has not been agreed.
- SageMaker `/ping` and `/invocations` shim: not built, not decided.
- The spike functions `llm-spike-031-lambda-0-8b` and `-2b` do not match the recipe. Do not build on them.

## What we need from you

Nothing blocks us. Keep a per-endpoint transport timeout in your provider row, as your Task 4 proposes; the 2B will need it.
