# Aprender reply to the pmcp.run brief: taking the in-account endpoint beyond 0.8B

As of: 2026-10-18 (UTC), repo HEAD 3330a4142, branch gsd/phase-2-contract-gate. GitHub state was read live on 2026-10-18; repo files were read at that HEAD. This replies to `/Users/guy/Development/mcp/sdk/pmcp-run/docs/superpowers/plans/2026-10-18-in-account-llm-model-ladder-aprender-brief.md`.

## How to read this

Each of the brief's four asks and five questions has its own numbered section below: the answer first, then a status, then evidence you can open.

- VERIFIED: every claim in the answer was checked against a source named under Evidence.
- PARTLY: the answer says which part was checked and which was not.
- UNKNOWN: not established from the repo or from read-only GitHub; the answer says what would settle it.

Nothing was deployed or run on AWS and no GitHub write was made to produce this reply. Numbers come from the repo, from the spike result files and from read-only GitHub reads; where nothing supports a number, the text says "unknown" or "not measured". Two things changed since the 2026-10-16 hand-off and affect several answers: #5451 was closed unmerged on 2026-10-18 (its change now rides in #5544), and the CUDA-router issue is filed (#5478).

## Answers at a glance

| Item | Answer in one line | Status |
|---|---|---|
| Ask 1 | 2B on Lambda is a pin change plus staged weights, but the recipe cannot name functions `aprender-llm-<size>-<host>` and a second run replaces the first; 4B on Fargate has no recipe. | PARTLY |
| Ask 2 | No GPU recipe; the repo cannot support naming an instance family; the CUDA-path routers ignore the key, so your proxy Lambda is the only gate there. | PARTLY |
| Ask 3 | Qwen3.5 9B and 27B were green on RTX 4090 and GB10 hosts in the 0.69.1 receipts; tool calling is exercised only on Qwen3.5 0.8B, 2B and 4B. | PARTLY |
| Ask 4 | Yes for both tickets (#5475, #5476, open); no date exists for the release carrying the auth gate. | PARTLY |
| Question 1 | 0.8B only has been deployed and measured; 2B and 4B are a pin change away but untried with this recipe. | PARTLY |
| Question 2 | Deploy fresh: the spike functions differ in release, naming, roles, bucket and checks; whether they still exist is unknown. | PARTLY |
| Question 3 | Both x86_64 and aarch64 CUDA builds are released; no record covers A10G or T4G devices. | PARTLY |
| Question 4 | Neither `/ping` nor `/invocations` exists in the server, and no shim or ECS recipe exists in the repo. | PARTLY |
| Question 5 | No date is committed; #5451 is closed, #5544 carries the gate and is open, no tag contains it. | PARTLY |

## Asks

### Ask 1 - 2B (Lambda) and 4B (Fargate) images or recipes, named to pass the `aprender-llm-` allowlist

**Answer:** Only partly. The image holds no weights, so a 2B Lambda function needs a two-key pin change and staged weights, but the recipe can only produce `aprender-llm-endpoint-dev` or `aprender-llm-endpoint-prod`, and a second deploy in the same environment replaces the function instead of adding one. A 4B Fargate service has no recipe in the repo at all.

**Status:** PARTLY - read from the code and the ladder; no recipe was run.

**What would have to change (a list, not a promise):** (1) a recipe edit so function, role, bucket and repository names carry size and host; (2) for 2B, set GGUF_RUNG and GGUF_OBJECT in `pin.env` and stage weights; (3) for 4B on Fargate, a new recipe (task definition or service, log group, roles, authenticated environment) with the spike scripts as the only starting point; (4) thread count and memory are literals today (6 threads, 10,240 MB).

**Evidence:**
- `deploy/llm-endpoint/pin.env`: the only model keys are GGUF_RUNG=qwen35-0.8b-q4km and GGUF_OBJECT=Qwen3.5-0.8B-Q4_K_M.gguf; the sha256 is derived from the ladder at deploy time (`justfile:3954`).
- `ladder_pin.py` exits 0 for qwen35-2b-q4km and qwen35-4b-q4km, so both have pinned hashes (`contracts/model-capability-ladder-v1.yaml:273`).
- Environment accepts dev or prod only (`justfile:3927`); the function is named `aprender-llm-endpoint-$ENV` (`justfile:4010`).
- An existing function is updated, not duplicated (`justfile:4035`); memory, timeout, storage and arch are literals checked on read-back (`justfile:4069`); threads are the literal "6" (`justfile:4028`).
- No Fargate recipe exists in the justfile or `deploy/`; the only artifacts are spike 031's `.planning/spikes/031-openai-endpoint-aws-hosts/tools/fargate_run.py:28`, which ran one task with the old entrypoint and apr 0.70.3.
- Whether the current hardened entrypoint runs on Fargate is UNKNOWN: nobody on record has tried it (`deploy/llm-endpoint/entrypoint.sh:47`).

### Ask 2 - GPU recipe: instance family and host authentication

**Answer:** There is no GPU recipe, and the repo does not support naming an instance family: it certifies RTX 4090 (sm_89) and GB10 (sm_121) hosts only, and no record shows apr on an A10G (sm_86) or T4G (sm_75) device. Your default stands for the CUDA path: five CUDA-path routers do not wrap the auth gate at this commit, so `APR_API_KEY_HASH` is ignored there and the private proxy Lambda is the only gate, provided nothing else can reach the host (the upstream issue is #5478, open).

**Status:** PARTLY - the call sites and the absence of records are VERIFIED; which family is best is not established by anything in the repo.

**Evidence:**
- `deploy/llm-endpoint` has a CPU asset pin, a CPU image tag and `--backend cpu` only (`deploy/llm-endpoint/entrypoint.sh:86`).
- Ungated call sites at this commit: `crates/apr-cli/src/commands/serve/server.rs:567`, `crates/apr-cli/src/commands/serve/handler_gpu_completion.rs:725`, `crates/apr-cli/src/commands/serve/handlers_include_01.rs:86`, `:197` and `:283`.
- The CPU GGUF router is wrapped (`crates/apr-cli/src/commands/serve/server.rs:420`); #5451 and #5544 wrap only that one router.
- The exception is documented in contract row FALSIFY-LLM-CONF-002 (`contracts/llm-endpoint-openai-conformance-v1.yaml:188`).
- Aprender's recorded position: no GPU-backed deployment until the routers are gated or the 401 proof is repeated on that path (`.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/deferred-items.md:36`).
- #5479 (open): a malformed key hash makes the gate fail open; our entrypoint refuses anything but 64 hex characters, so our image cannot reach it (`deploy/llm-endpoint/entrypoint.sh:47`).
- g5g is an unverified list-price note from a spike write-up, about $0.42/h for the xlarge size per a price site, with sm_75 "unmeasured" (`.planning/spikes/030-qwen35-agent-turn-cost-cpu/README.md:90`).

### Ask 3 - Certified sizes above 4B, and non-Qwen tool-calling models

**Answer:** Qwen3.5 9B and 27B Q4_K_M are required ladder rungs and were green in the committed 0.69.1 receipts of 2026-09-23 on an RTX 4090 host and a GB10 host; nothing measures them on a Lambda or Fargate path, and no receipt covers v0.71.4. Tool calling through the OpenAI routes has been exercised only on Qwen3.5 0.8B, 2B and 4B; for any other model it is not established.

**Status:** PARTLY - receipts are VERIFIED as dated and scoped; tool-calling on other models is UNKNOWN (a measurement on that model through `/v1/chat/completions` would settle it).

**Evidence:**
- Ladder rungs and the green definition: `contracts/model-capability-ladder-v1.yaml:289` and `contracts/model-capability-ladder-v1.yaml:307`.
- Receipts: `evidence/dogfood/models/0.69.1/lambda.json` and `evidence/dogfood/models/0.69.1/gx10.json` (apr 0.69.1, qa passed, no failed gates, CPU and CUDA ran without fallback).
- 35B-A3B is listed as refused on CUDA with ticket #3977, which is now closed; the ladder entry is unchanged and whether a CUDA forward shipped is not established (`contracts/model-capability-ladder-v1.yaml:200`).
- Other architectures appear in the ladder or the receipts (qwen2, qwen3, tinyllama); the two 30B MoE files were not green; none of those receipts exercise tool calls.
- Tool-call records are all Qwen3.5: `.planning/spikes/029-apr-serve-openai-tool-loop/README.md:89` (0.8B) and `:106` (4B) and spike 030 (22 of 22 correct tool choices on a laptop).
- "2B is the smallest certified size" is the repo's own wording (`deploy/llm-endpoint/README.md:334`); #4030 behind the 0.8B exception is closed, and we did not re-derive 0.8B's status.

### Ask 4 - Tickets for batched Q4_K CPU prefill or a multi-entry prefix cache, and a date for the release with the auth gate

**Answer:** Yes, both tickets exist and are open: #5475 (CPU prefill is one token at a time, batched prefill is GPU-only) and #5476 (the session keeps one decode state, no multi-entry prefix cache); both were filed on 2026-10-17, after your 2026-10-16 hand-off, and neither has a merged fix. There is no date for the release carrying the auth gate, and the PR you named, #5451, was closed unmerged on 2026-10-18; the same change now rides in #5544 (see Question 5).

**Status:** PARTLY - the tickets and their state are VERIFIED; "no other ticket" rests on the search phrases listed in the facts table; the release date is UNKNOWN.

**Evidence:**
- `gh issue view 5475` and `gh issue view 5476`: both OPEN, created 2026-10-17, updated 2026-10-18.
- #5475 states the batched prefill is cuda-only; confirmed at `crates/aprender-serve/src/gguf/inference/forward/qwen35_session.rs:458` and its call at line 440.
- The batched CPU prefill that spike 020 built is F32-weights only (`.planning/spikes/020-qwen35-batched-prefill/README.md:74`); no Q4_K_M batched prefill has been measured.
- Older, closed: #4228 (NOT_PLANNED, 2026-09-25, parked as over budget) reported a batched CPU Q4_K_M prefill on Qwen3.5-4B at 20.8-25.1 tok/s against 5.0-6.2 per token on a 48-thread host; its test and function are not in the tree at this commit.
- Related but GPU path: #5305 (open), #4214 (open), #4274 (closed 2026-10-14).

## Questions

### Question 1 - Which sizes can you deploy today with the current recipe?

**Answer:** 0.8B is the only size the current recipe has deployed and measured. 2B and 4B can be selected by changing two pin keys (their sha256 pins exist in the ladder) and staging weights, but neither has been deployed or measured with this recipe and pin; 4B on default Lambda has never been measured by anyone, and the recipe deploys nothing but a Lambda function.

**Status:** PARTLY - recipe behaviour is read from code and not run; the 0.8B deployment is VERIFIED from the record.

**Evidence:**
- Deployed record: function `aprender-llm-endpoint-dev`, apr 0.71.4, 0.8B only, 2026-10-16 (`.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-LIVE-DEPLOY-EVIDENCE.json`).
- That record lists "2B or 4B model" and "Fargate" as not acted on (same file, key recommendations_not_acted_on).
- Pin and naming details: see Ask 1 (`deploy/llm-endpoint/pin.env`, `justfile:4010`).
- Spike 031 coverage: 0.8B and 2B on Lambda, 4B on Fargate and Lambda Managed Instances only (`.planning/spikes/031-openai-endpoint-aws-hosts/README.md:149`).

### Question 2 - Do the spike-031 functions match the current recipe, or deploy fresh?

**Answer:** Deploy fresh. The spike functions `llm-spike-031-lambda-0-8b` and `llm-spike-031-lambda-2b` differ from the current recipe in release (apr 0.70.3 against v0.71.4), image repository, role, bucket, environment (no key hash, no model hash check) and name (neither passes an `aprender-llm-` prefix check). Whether they still exist is UNKNOWN: no AWS call was made.

**Status:** PARTLY - the field-by-field differences are VERIFIED from the spike scripts and the deployed record; existence is UNKNOWN.

**Evidence:**
- Spike side: names, image tag `cpu-v0.70.3`, role, env names (`.planning/spikes/031-openai-endpoint-aws-hosts/tools/lambda_up.py:23`); entrypoint without refusals (`.planning/spikes/031-openai-endpoint-aws-hosts/deploy/entrypoint.sh:29`).
- Current side: `aprender-llm-endpoint-dev`, tag `cpu-v0.71.4`, env names APR_API_KEY_HASH, MODEL_S3_URI, MODEL_SHA256, PLATFORM, RAYON_NUM_THREADS (`.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-LIVE-DEPLOY-EVIDENCE.json`).
- Same on both: arm64, 10,240 MB, 900 s, 10,240 MB storage, 6 threads, so old 0.8B and 2B timings are comparable context.
- Neither release carries the auth gate, the content fix or the alias (`deploy/llm-endpoint/pin.env`: profile baseline).
- The spike README says the two functions were left running; the last record of them is 2026-10-16 (`.planning/spikes/031-openai-endpoint-aws-hosts/README.md:214`).

### Question 3 - For GPU: x86 `ml.g5` (A10G) or arm64 `g5g`, and which CUDA build covers it?

**Answer:** Both CUDA builds are released (x86_64 and aarch64 in v0.71.4), but no record shows either running on an A10G (sm_86) or a T4G (sm_75) device, so which one "covers" it is UNKNOWN. The x86_64 build is built on an RTX 4090 (sm_89) host and the aarch64 build on a GB10 (sm_121) host (no GPU is needed to build); the source JIT-compiles PTX for the real device and gates the int8 dot-product paths to sm_75 and later, which is a code reading, not a run.

**Status:** PARTLY - the assets and build hosts are VERIFIED; coverage of A10G and T4G devices is UNKNOWN (a dogfood cell on each device would settle it).

**Evidence:**
- Release assets: `gh release view v0.71.4` lists cpu and cuda tarballs for x86_64 and aarch64 Linux.
- Build hosts: `.github/workflows/binary-release.yml:241` (matrix) and the header at line 24; certification hosts: `.github/workflows/cuda-nightly.yml:27` and `.github/workflows/b2-gpu.yml:45`.
- Capability handling: `crates/aprender-serve/src/cuda/gpu_profile.rs:111` and `crates/aprender-serve/src/cuda/gpu_profile.rs:121`; `crates/aprender-serve/src/cuda/kernels_generate.rs:20`.
- Spike 030 says the arm64 CUDA binary embeds PTX for sm_70, sm_75 and sm_90; that is a secondary claim we did not re-derive (no `cuobjdump` run) (`.planning/spikes/030-qwen35-agent-turn-cost-cpu/README.md:88`).
- No A10G, T4G, g5g or ml.g5 record or ticket exists (searched tree and GitHub); contract domain lists name sm_86 only (`contracts/ptx-target-parity-v1.yaml:15`).

### Question 4 - SageMaker needs `/ping` and `/invocations`: thin shim, or ECS on EC2 GPU?

**Answer:** Neither path exists in `apr serve`, and the repo holds no shim, no SageMaker container and no ECS-on-EC2 recipe, so none of the three options has an artifact today; we are not choosing for you. The premise that SageMaker needs those two paths is the brief's statement and was not verified here, and a proxy outside the endpoint does not remove a container-side requirement if the premise holds.

**Status:** PARTLY - the absence of the routes is VERIFIED; the SageMaker requirement and its per-invocation time limit are UNKNOWN here.

**Evidence:**
- No `/ping` or `/invocations` string registers a route anywhere under `crates/`; the router table registers `/health`, `/v1/chat/completions` and Ollama-style routes (`crates/aprender-serve/src/api/router.rs:89`, `crates/aprender-serve/src/api/router.rs:169`).
- No issue or PR title mentions SageMaker, `/ping` or `/invocations` (GitHub searches, state all).
- Measured CPU turns to compare with any SageMaker limit: 71.5-72.3 s warm two-tool turn on 0.8B at the current pin (`.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-LIVE-DEPLOY-EVIDENCE.json`); 246 s for 2B on Lambda (`.planning/spikes/031-openai-endpoint-aws-hosts/results/bench-lambda-2b.json`).
- The limit of the chosen SageMaker mode is not established in this repo and is not stated here.

### Question 5 - When is the release with the auth gate (#5451) expected?

**Answer:** No date is committed for the release that carries the auth gate, and none is offered here. The observable state on 2026-10-18: #5451 is closed unmerged; the same GGUF-router auth gate is carried by #5544 (open, 8 checks success, 3 skipped, none failed, 3 comment-only reviews, no approval decision, no milestone), so no release tag contains it; the latest tag v0.71.4 equals the tag our image is pinned to.

**Status:** PARTLY - the PR, tag and cadence facts are VERIFIED; the date itself is UNKNOWN and nothing in the repo or on GitHub settles it.

**Evidence:**
- `gh pr view 5451`: CLOSED 2026-10-18T10:28:20Z, mergedAt null, closed for WIP-cap clearance with the branch `fix/gguf-router-auth-gate` kept at e9b6112c61; `git tag --contains e9b6112c61` prints nothing.
- `gh pr view 5544`: OPEN, supersedes #5411, GGUF router behind AuthGate::from_env(); #5412 was closed 2026-10-18 as superseded by #5411.
- `deploy/llm-endpoint/pin.env`: APR_TAG=v0.71.4, APR_PIN_PROFILE=baseline; `gh release list`: v0.71.4 is Latest, published 2026-10-14T18:17:52Z; no v0.72 or v0.73 tag exists upstream.
- Cadence, derived from git tags (method: newest 15 release tags without an rc suffix, gaps in days between consecutive tags): 0, 0, 2, 0, 3, 2, 3, 1, 8, 2, 2, 2, 2, 0; median 2 days over a 27-day span. This is history, not a forecast.
- GitHub milestones carry plan dates only for trains, not for this PR: 0.72.0 is due 2026-10-17 (open, no v0.72.0 tag yet), 0.73.0 is due 2026-11-06 and its own text says "plan dates, not measured".
- A release carrying the fix passes the gates in `docs/specifications/APR-RELEASE-001-train-and-build-kaizen.md:241` and `docs/specifications/PP-LLAMA-001-MASTER.md:235`.
- `contracts/release-schedule-06x-v1.yaml:5` covers 0.67.0 to 0.70.0 only; `contracts/release-readiness-v1.yaml` holds no 0.71 or 2026 date.
- Our pin rule allows release artifacts only, never a fork build, so the gate commit on our branch cannot ship in the image (`deploy/llm-endpoint/pin.env:5`).
- Re-pin cost on our side: edit the pin keys, rebuild the image, probe, then a dry-run, a preflight and a live deploy, then the 401 proof and a re-measurement (`deploy/llm-endpoint/README.md`, section "Re-pin"); a re-pin invalidates every earlier record.

## What we have and have not measured

Measured (host, CPU generation and apr version stated for each):
- Current pin, 0.8B, default Lambda 10,240 MB arm64, Graviton3 (cpu_part 0xd40), apr 0.71.4 (5027da624): cold to first answer 20.1 s; prefill about 6.0-6.4 tok/s; warm two-tool turn 71.5-72.3 s, about $0.0095 per turn; ten-tool prefill 148.8 s; turn 2 extending turn 1 8.1 s against 66.0 s; the second of two simultaneous requests refused at once (reserved concurrency 1); peak memory 1,602 MB (`.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-LIVE-DEPLOY-EVIDENCE.json`).
- Same function, prefix reuse: a different user line over the same tools prefix 101.9 s, identical repeat 1.8 s, conversation X then Y then X 101.4 s and 101.5 s (`.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-PREFIX-REUSE-EVIDENCE.json`).
- Spike 031, apr 0.70.3 (an older release than the pin): Lambda 2B on Graviton2 (0xd0c) two-tool turn 246 s, about $0.033, prefill 1.9 tok/s, peak memory 4.8 GB; Fargate 4B at 16 vCPU on Graviton4 (0xd4f) 39.8 s to ready, two-tool turn 131 s, about $0.023, $0.632 per hour while up; 4B on Lambda Managed Instances (c9g.16xlarge host) two-tool turn 79 s with one of two simultaneous requests throttled (`.planning/spikes/031-openai-endpoint-aws-hosts/README.md:149`).
- Spike 030, apr 0.70.3, an M4 Pro laptop (not Graviton): two-tool turn at 6 threads 35 s (0.8B), 76 s (2B), 156 s (4B); prefill runs at the decode rate (`.planning/spikes/030-qwen35-agent-turn-cost-cpu/README.md:116`).
- Tool calling: one two-tool request each on 0.8B, 2B and 4B returned a get_weather call in spike 031, and 22 of 22 tool choices were correct in spike 030; neither is a reliability claim.
- GPU: one number, #4214 (apr 0.69.1 CUDA, RTX 4090 host, 4B Q4_K_M, about 850 tokens, 11.0 s to first token, no reuse); not an agent turn and not on an AWS GPU host.

Not measured:
- 4B on default Lambda; 2B or 4B on Fargate with the current image; anything above 4B on a private host of this pattern.
- Any GPU Qwen3.5 agent turn through the platform path, and any A10G or T4G device.
- x86_64 CPU hosts, although an x86_64 cpu asset is released.
- Multi-conversation prefix reuse beyond the single-slot measurement on 0.8B.
- Tool-call reliability on a fixed message set for 2B and 4B.
- The current pin on any size but 0.8B, and Q4_K_M batched CPU prefill.

## Open items and owners

- Merge and release of #5544 (the auth gate, content fix and alias): paiml; open, no date.
- Re-pin to a release containing the gate, then redeploy and re-prove the 401: Aprender; waiting on that release.
- CUDA-path routers without the gate (#5478) and the fail-open malformed hash (#5479): upstream; both open, #5478 in milestone 0.73.0.
- CPU batched prefill (#5475) and multi-entry prefix cache (#5476): upstream; open, no merged fix.
- Recipe naming by size and host, and a Fargate recipe: Aprender; no artifact exists, nothing here commits to build them.
- Any GPU measurement on A10G or T4G: nobody; no ticket exists.
