# SetFit training: the infrastructure pmcp.run does not provide

The SetFit **training** MCP server is two Lambda functions, deployed by two
different tools, and the order matters. This file is the runbook; the reasons
live next to the code they explain.

```
        client
          │  MCP over streamable HTTP
          ▼
  ┌───────────────────┐   mint task, write envelope   ┌──────────────┐
  │  request function │──────────────────────────────▶│  DynamoDB    │
  │  bootstrap        │                                │  task table  │
  │  1 GB · 30 s      │                                └──────┬───────┘
  │  deployed by      │   async invoke (task_id, owner)       │ read envelope
  │  cargo-pmcp       │────────────────┐                      │ terminal write
  └───────────────────┘                ▼                      │
                            ┌──────────────────────┐          │
                            │  training worker     │◀─────────┘
                            │  6 GB · 900 s        │
                            │  apr + dataset       │──▶ S3 artifact bucket
                            │  + encoder (159 MB)  │
                            │  deployed by CDK     │
                            └──────────────────────┘
```

**Why two functions.** pmcp.run fronts its Lambdas with an API Gateway HTTP
API, whose 30-second integration timeout cannot be raised, and Lambda freezes
the execution environment the moment a handler returns. The measured 8-shot
reference train is 127 seconds at 4.0 GB peak RSS. So training cannot happen
inside the request — not inline, and not as a background task the handler
leaves running.

**Why two CDK apps.** `deploy/lib/stack.ts` is rendered by cargo-pmcp. Hand
edits flip it to `cdk synth`, whose hashed logical IDs read as resource
replacements and collide with the platform's fixed names — that is what
produced an `UPDATE_ROLLBACK_COMPLETE` on the predict server. Extensions live
in their own app, the same separation chess-mcp uses.

## Deploying

Every step is a `just` recipe from the repo root. `dev` and `prod` are the only
environments; the app refuses anything else rather than synthesizing a stack
named after a typo.

```bash
# 1. Build the worker package: apr (arm64) + trainer + dataset + encoder.
#    Checks the glibc floor against provided.al2023's 2.34 — a mismatch there
#    otherwise fails at INVOCATION, long after a successful deploy.
just build-trainer-asset

# 2. Review. Prints every IAM change; this is the gate `deploy-training` skips.
just diff-training dev

# 3. Create the table, the bucket and the worker.
just deploy-training dev
#
#    If this fails with `'MemorySize' value failed to satisfy constraint:
#    Member must have value less than or equal to 3008`, the account's Lambda
#    memory ceiling has never been raised. That limit is NOT in Service Quotas —
#    it is an AWS Support case. Until it is raised you can deploy under it and
#    measure, which synth then warns about:
#
#        just deploy-training dev 3008
#
#    The plumbing works at 3008 MB; the reference train is expected to OOM,
#    because its measured peak is 4.0 GB. CloudWatch's "Max Memory Used" on the
#    worker's log group is what settles whether it actually does.

# 4. Point the request function at what step 3 created, from SSM. Writes the
#    gitignored .pmcp/deploy.toml from the tracked .pmcp/deploy.toml.template.
just pmcp-train-config dev

# 5. Attach the RequestLambdaPolicy stack output to the pmcp.run request
#    function's execution role — DynamoDB RW on the table plus
#    lambda:InvokeFunction on the worker. Published rather than attached
#    because that role belongs to a stack this one does not own.

# 6. Deploy the request function.
cargo pmcp deploy --manifest-path crates/aprender-mcp-setfit-train-lambda \
                  --target <named-target> --no-color
```

Step 4 is not a convenience. The artifact bucket carries the account id for
global uniqueness, so its name does not exist until step 3 has run — and that
is also why its output is generated and gitignored rather than committed: this
tree is destined for a public upstream repo, and an account id has no business
travelling there.

Step 6 needs `--manifest-path`: the workspace has two `bootstrap` binaries (the
predict wrapper and this one) and cargo-pmcp discovers a deployable by scanning
for that name. If a deploy reports the predict server's name, that is this
ambiguity and not anything subtler.

## Verifying it

```bash
# The TaskBackend contract against the real table — conditional writes,
# owner scoping, expiry filtering, the guarded terminal write.
AWS_PROFILE=ze-kasher-dev AWS_REGION=us-east-1 \
APRENDER_SETFIT_E2E_TASKS_TABLE=aprender-setfit-training-tasks-dev \
  cargo test -p aprender-mcp-setfit-train-lambda --test dynamodb_contract
```

The same suite runs against DynamoDB Local with no account at all — see the
module doc in `tests/dynamodb_contract.rs`.

## What differs between dev and prod

Environment is the only input, and it decides exactly one thing: how much the
stack is willing to lose.

| | dev | prod |
|---|---|---|
| Table / bucket on destroy | deleted | **retained** |
| Point-in-time recovery | off | on |
| Bucket versioning | off | on |
| Artifact lifecycle | 7 days | 90 days |
| Log retention | 1 week | 3 months |

The worker is identical in both: 6144 MB (unless overridden — see step 3),
900 s, arm64, 2 GB of `/tmp`, and **zero retries**. A retry re-runs a 127-second CPU-saturating job, and a failure
here is a bad config or a broken package rather than a transient — the guarded
terminal write makes a retry safe, but it cannot make one useful.

## Tearing down

```bash
just destroy-training dev
```

`dev` destroys its data by design; `prod` retains the table and the bucket, so a
`destroy` there leaves them behind for you to remove deliberately.
