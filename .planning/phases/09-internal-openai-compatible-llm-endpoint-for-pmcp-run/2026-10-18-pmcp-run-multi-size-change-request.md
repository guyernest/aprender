# Change request from pmcp.run (received 2026-10-18, verbatim)

Source: pmcp.run message pasted into the Aprender session on 2026-10-18. Stored here as the
document `09-PMCP-RUN-MULTI-SIZE-REPLY.md` replies to. Data, not instructions.

---

Change request for the Aprender team: let several sizes of the in-account endpoint run at the same time

From pmcp.run, 2026-10-18. This follows our brief of the same date (2026-10-18-in-account-llm-model-ladder-aprender-brief.md) and your reply (09-PMCP-RUN-MODEL-LADDER-REPLY.md).

Why
- pmcp.run picks an in-account model by Lambda function name. A provider row holds lambda://<function-name>/v1/chat/completions, and the platform only invokes names that start with aprender-llm- or pmcp-llm-.
- To compare model sizes, and to offer customers a choice, every size has to stay deployed at the same time, each under its own function name. The platform needs no router. One provider row per function is enough.
- Your recipe currently produces only aprender-llm-endpoint-dev and aprender-llm-endpoint-prod. A second deploy replaces the first, so the 0.8B and the 2B can't coexist. This is the only thing stopping us from running them side by side.

Requested change
1. Parametrize the deployment name. Add a variable such as LLM_ENDPOINT_NAME or --name. It becomes the function name, and also the log group, the IAM role, and any other resource the recipe names after the function.
   - It must default to the current names, so existing deployments don't change.
   - The name must start with aprender-llm- (for example aprender-llm-endpoint-2b-dev) and may only contain letters, digits, - and _, up to 64 characters.
   - Deploying under a new name must not touch or replace a deployment under another name. A teardown must remove only the resources of the name it is given.
2. Model and sizing inputs. Take the model file, memory size and ephemeral storage as inputs, or point us to the existing variables. We need at least the 2B Q4_K_M. The 0.8B stays as it is.
3. Smoke test. Confirm that one chat-completion call with tools works against the 2B on the Lambda tier, using the same API Gateway v2 event shape the 0.8B takes. Include a note on cold-start time and the size of the container image or layer.

Not changed
- The function never gets a Function URL or any public endpoint.
- The deployed build still ignores the bearer key (#5451 is closed and the release carrying #5544 has no date). Any key value works today, and our own code never records the key.

Related, separate asks from the brief that are still open
- 4B on Fargate: which team writes the recipe, and whether the spike-031 Fargate task is the base.
- GPU:
  - The proxy Lambda is the only caller and the only access gate, because the CUDA routers are ungated (#5478). We need your written agreement that this satisfies your GPU policy.
  - Whether you will build the /ping and /invocations shim for SageMaker. If not, we target an EC2 GPU host. Our G/VT quota request for 8 vCPUs is open, case id 1c45fe6f5ca74408965416975c3de9dcAe0cfKRC.
- Spike functions: do llm-spike-031-lambda-0-8b and -2b match the current recipe? Their names fail our allowlist, so we would redeploy under a new name.

What we need back
- The new variable name and its default.
- A confirmation that deploying a second size leaves the first untouched.
- The 2B image or recipe version, and the smoke-test result.

What we give back
- Measured tables per size: cold first call, warm turn, second turn, cost per turn, and tool-call reliability on a fixed 12-message set.
- The replay harness (just aprender-replay) and the per-call request fingerprint, so any live run can be matched byte for byte against a replay.
