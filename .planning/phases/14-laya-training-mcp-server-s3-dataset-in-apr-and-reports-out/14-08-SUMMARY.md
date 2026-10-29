---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 08
subsystem: infra
tags: [cdk, step-functions, sagemaker, fargate, ecr, codebuild, eventbridge, s3-lifecycle, iam, laya]

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-05's finalizer (three input shapes, five env vars, IAM list); 14-06/14-07's run_step env contract and the arm64 pack image; 14-11's gate-row helpers; 14-02's validate.rs compiled into the finalizer asset
provides:
  - deploy-extensions stacks aprender-laya-trainer-images-<env> (stack=laya-trainer-images) and aprender-laya-trainer-<env> (stack=laya-trainer, trainImageDigest, packImageDigest)
  - scripts/laya_deploy/laya_trainer_stack_check.py (TRAINER STACK CHECK OK; --selftest, 36 mutation cases)
  - recipe `laya-trainer-synth env train_digest pack_digest` and gate row trainer-synth
  - deploy-extensions/README.md "Laya training server (Phase 14): ORDER MATTERS"
affects: [14-09, 14-10, 14-12, 14-13, 14-14]

plan_head_before: 1100d9353793960b04a926e32c6f4e0056204bd8
plan_head_after: 1e20885aa64c513b83b41d26160c65eec3e465e7

commits: 2
actuals:
  tokens: 24800     # chars/4 over the added lines of the realized diff (99,293 chars)
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "Every spend and access limit is a literal in the synthesized template and is asserted from it, with a mutation table proving each assertion can fail"
    - "Offline synth runs the CDK CLI with no credentials and no profile (AWS_CONFIG_FILE=/dev/null, AWS_EC2_METADATA_DISABLED, notices and version reporting off), so no account is contacted and the template is the same on every machine"
    - "A SageMaker channel location that carries no IAM grant of its own (LiteralLocation), because the CDK's own S3Location helpers add wildcard-resource grants to the training role"
    - "A derived ARN from the fixed state machine name (Stack.formatArn) breaks the finalizer <-> machine circular dependency"

key-files:
  created:
    - deploy-extensions/lib/laya-trainer-stack.ts
    - deploy-extensions/lib/laya-trainer-images-stack.ts
    - scripts/laya_deploy/laya_trainer_stack_check.py
  modified:
    - deploy-extensions/bin/app.ts
    - deploy-extensions/README.md
    - deploy-extensions/.gitignore
    - justfile
    - scripts/laya_gates.tsv

key-decisions:
  - "The SageMaker channel and output locations are a small LiteralLocation subclass, not S3Location.fromBucket / fromJsonExpression: both add S3 grants (s3:GetBucket*, s3:List*, or * resources) to the training role, which would have contradicted the exact per-principal S3 rights this plan states once"
  - "The data channel S3Uri is JsonPath.format over $.dataset_prefix, rendered States.Format('s3://<bucket>/{}', $.dataset_prefix) under the key S3Uri.$; the base channel is the exact literal s3://<bucket>/base/laya-55cf4c4e/"
  - "No state-machine log group: the CDK's log-delivery grant puts logs:PutResourcePolicy on every resource into the machine's role, more privilege than the machine's observability is worth; Step Functions keeps execution history itself"
  - "The offline synth is credential-free: the CDK CLI resolves the default account from ambient credentials, which both puts the account id into the template and makes a (read-only) STS call"
  - "The check's S3 grants are compared for EQUALITY per principal, not as a superset, so an extra widening is refused as well as a missing PutObjectTagging"
  - "Training volume 100 GB (VolumeSizeInGB), a choice the plan left open: CUDA image layers, the 0.85 GB base tar and its extraction, three seeds of checkpoints"

patterns-established:
  - "A mutation that cannot be applied is a FAIL, not a skip (the ECR case caught itself: one() returned None for two repositories)"

requirements-completed: [D-02, D-03, D-12, D-13, D-14, D-19, D-11, D-10, D-01]

coverage:
  - id: D1
    description: "Tracer: both stacks synthesize offline; the check proves the run shape from the template: ml.g5.xlarge x1, MaxRuntimeInSeconds 21600, machine timeout 36000, digest-pinned train and pack images, exactly the data and base channels (S3Prefix; data a States.Format over $.dataset_prefix, base the exact base/laya-55cf4c4e/ prefix), LAYA_EXPECTED_INPUTS bound to $.inputs_sha256, ARM64 8192/16384 pack task, both Catches, start_time in both Finalize payloads, each image-digest env literal equal to its step's image digest"
    requirement: "D-02"
    verification:
      - kind: integration
        ref: "just laya-trainer-synth dev -> TRAINER STACK CHECK OK trainer=205 images=105 assertions, TRAINER SYNTH OK env=dev; just laya-trainer-synth staging -> exit 2 REFUSED env; just synth-training dev unchanged (synth OK); LAYA_GATES_ONLY=trainer-synth just laya-gates-selftest -> DRIFT CHECK OK, ROW trainer-synth GREEN, AWS CALLS: 0, LAYA GATES ROWS OK 1 of 46"
        status: pass
    human_judgment: false
  - id: D2
    description: "Least privilege per principal (SageMaker, Fargate task, finalizer: exact S3 grants including PutObjectTagging on runs/*; no Action *, s3:*, iam:* or sagemaker:*; PassRole only on specific role ARNs), the D-19 lifecycle (exactly three rules, runs/ tag-filtered, nothing on datasets/ or base/), bucket RETAIN/BLOCK_ALL/TLS, ECR untagged-only expiry and IMMUTABLE tags"
    requirement: "D-19"
    verification:
      - kind: unit
        ref: "python3 -I scripts/laya_deploy/laya_trainer_stack_check.py --selftest -> 36 mutation cases, 0 failure(s), STACK CHECK SELFTEST OK (the plan's 31 plus five)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The request function gets nothing at deploy time: RequestLambdaPolicy is an output only (DynamoDB item actions, StartExecution, DescribeExecution/StopExecution on the machine's executions, GetObject on datasets/* and runs/*, ListBucket conditioned on datasets/*, no s3:Put*); the abort and recovery path is declared (status rule for FAILED/TIMED_OUT/ABORTED, rate(10 minutes) reconcile rule with {source: reconcile}, retry 4 and an SQS DLQ on both targets, FinalizerErrors alarm)"
    requirement: "D-10"
    verification:
      - kind: unit
        ref: "selftest cases: the request policy granting s3:PutObject; ListBucket without the datasets/* condition; the request policy without states:DescribeExecution; the EventBridge rule without ABORTED; the reconcile schedule rule removed; an EventBridge target without a deadLetter queue; a policy attached to a role outside the stack"
        status: pass
    human_judgment: false
  - id: D4
    description: "Two source-level mutations (not template edits) turn the check red on a fresh synth: SageMaker runs/* write without PutObjectTagging -> REFUSED sagemaker-put-tagging and sagemaker-s3-scope; TRAIN_MAX_RUNTIME_SECS 43200 -> REFUSED train-runtime. Both restored (cmp-identical to the committed file)"
    requirement: "D-12"
    verification:
      - kind: other
        ref: "manual source mutation, cdk synth into the scratchpad, check script; file restored from a copy and git status clean"
        status: pass
    human_judgment: false

duration: 70min
completed: 2026-10-29
status: complete
---

# Phase 14 Plan 08: Laya trainer CDK stacks Summary

**All of the Laya training server's AWS infrastructure is declared in two offline-synthesizable CDK stacks (images; trainer with the Train -> PackVerify -> Finalize machine), and every spend limit, image pin, S3 right and retention rule is asserted from the synthesized template by a check script whose 36-case mutation table proves each assertion can fail.**

## Performance

- **Duration:** about 70 min
- **Tasks:** 2 (one tracer, one auto/tdd)
- **Files:** 3 created, 5 modified

## Accomplishments

- **Images stack** `aprender-laya-trainer-images-<env>`: two ECR repositories (IMMUTABLE tags, scan on push, one lifecycle rule expiring UNTAGGED images after 14 days and no count rule, RETAIN), a build-context bucket (BLOCK_ALL, TLS, 7-day expiry) and two privileged CodeBuild projects (LINUX_CONTAINER and ARM_CONTAINER, both LARGE). The shared buildspec refuses a `GIT_COMMIT` that is not 40 hex (an immutable tag is permanent), logs in, builds `-f $DOCKERFILE --build-arg GIT_COMMIT -t $REPO_URI:$GIT_COMMIT`, pushes, and prints `IMAGE DIGEST sha256:<64 hex>`.
- **Trainer stack** `aprender-laya-trainer-<env>`: the RETAINed bucket with exactly the three D-19 rules, the SetFit-schema task table, a no-NAT two-public-subnet VPC with a no-ingress security group, the cluster, the ARM64 8 vCPU / 16 GiB / 60 GiB-ephemeral pack task running `<pack repo>@sha256` with command `pack`, the finalizer Lambda (arm64, provided.al2023, 512 MB, 300 s, the five env vars with the machine ARN derived from the fixed name), and the machine `aprender-laya-train-<env>` (Train via `sagemaker:createTrainingJob.sync`, PackVerify via `ecs:runTask.sync`, Finalize and CatchFinalize via `lambda:invoke`, Catch States.ALL on Train and PackVerify, 36,000 s). Instance, runtime and both images are literals; the data channel is `States.Format('s3://<bucket>/{}', $.dataset_prefix)` and nothing else of the input reaches a limit.
- **Drivers of the finalizer:** an EventBridge status rule (FAILED, TIMED_OUT, ABORTED, this machine), a `rate(10 minutes)` rule sending `{"source":"reconcile"}`, retry 4 and the SQS DLQ `aprender-laya-train-finalizer-dlq-<env>` (14-day retention) on both targets, and the `FinalizerErrors` alarm.
- **Four IAM principals, four rights**, stated once: SageMaker (read `datasets/*` and `base/laya-55cf4c4e/*`; write `runs/*` with PutObject AND PutObjectTagging, and `sagemaker-output/*`), Fargate task role (read `runs/*` and `base/*`, write `runs/*` with both), finalizer (the 14-05 list, ECS conditioned on the cluster, SageMaker limited to `training-job/laya-*`), and the request function (nothing; `RequestLambdaPolicy` is an output only).
- **The check** (`laya_trainer_stack_check.py`): 205 assertions on the trainer template and 105 on the images template, flattening CloudFormation intrinsics to placeholders so the ASL definition can be parsed. `--selftest` mutates copies of the real templates: 36 cases, each required to be refused under its intended assertion name, plus the unmutated templates passing.
- **Recipe and gate row:** `just laya-trainer-synth` cross-builds the finalizer (aarch64, glibc floor <= 2.34), installs it as `bootstrap`, synthesizes both stacks credential-free, runs the check and prints `TRAINER SYNTH OK env=<e>`; `row_trainer_synth` makes it a registered gate (unknown env -> exit 2, a single digest -> exit 2, the real synth, and the check selftest), drift check passing.
- **README:** the ORDER MATTERS section (images -> builds -> trainer -> staging -> probes -> request function -> OAuth -> 401 check -> grant -> smoke), why the grant is last, what each principal may do, retention, and the recovery model.

## Task Commits

1. **Task 1 (tracer): both stacks, the check, the recipe and the gate row** - `ed40bad4e` (feat)
2. **Task 2: README order, grant-last rule, retention and recovery** - `1e20885aa` (docs)

Task 2's code (the roles, lifecycle rules, abort path, post-401 policy output, DLQ, alarm and the 31-plus case mutation table) was written in the same pass as Task 1 because the stack file and the check script are single files whose Task 1 assertions depend on them; only the README was left for Task 2. The tracer gate (`<verify>` re-run end to end: synth OK, staging refused with exit 2, `synth-training` unchanged, gate row green) passed before expansion; Task 2's verify passed on the final tree.

## Decisions Made

See `key-decisions`. The one that shaped the most code: the CDK's own SageMaker location helpers grant S3 rights the plan does not allow, so channels and output use a grant-free `LiteralLocation`, and the role's S3 rights are the explicit statements the check compares for equality.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] The first template rendered `InstanceType: ml.ml.g5.xlarge`**
- **Found during:** Task 1, inspecting the synthesized ASL
- **Issue:** the CDK prefixes `ml.` itself, so the literal `ml.g5.xlarge` became `ml.ml.g5.xlarge`, which SageMaker would have rejected at the first run.
- **Fix:** the stack strips the prefix before the CDK adds it; the check asserts the rendered literal `ml.g5.xlarge`.
- **Files modified:** `deploy-extensions/lib/laya-trainer-stack.ts`. **Commit:** ed40bad4e

**2. [Rule 2 - Missing critical] CDK helpers would have broadened the training role's S3 rights**
- **Found during:** Task 1 (reading the CDK source for `S3Location`)
- **Issue:** `S3Location.fromBucket` / `fromJsonExpression` add S3 grants (`s3:GetBucket*`, `s3:List*`, `*` resources) to the training role, contradicting the plan's exact per-principal rights and T-14-30.
- **Fix:** `LiteralLocation`, which supplies the URI and no grant (the plan named `fromJsonExpression` for the data channel; the rendered `S3Uri.$` is the same `States.Format` it specified).
- **Commit:** ed40bad4e

**3. [Rule 2 - Missing critical] No state-machine logging**
- **Issue:** enabling logging made the CDK add `logs:PutResourcePolicy` (and five other log-delivery actions) on `*` to the machine's role.
- **Fix:** no log group on the machine; Step Functions retains execution history. (The remaining `*` resources in the machine role are the CDK's own `ecs:DescribeTasks`/`StopTask` and `sagemaker:ListTags`, which accept no narrower resource; no wildcard action.)

**4. [Rule 3 - Blocking / correctness] The offline synth reached for ambient credentials**
- **Found during:** Task 1. The first manual synth (before the recipe existed) ran with the shell's default AWS profile: the CDK CLI resolved account 672915487120 and region us-west-2, put the literal account into the template, and (via the VPC's availability-zone validation) wrote a `cdk.context.json` from a read-only lookup. No mutating call was made, but it contradicted "no AWS call (synth only)".
- **Fix:** `laya-trainer-synth` runs the CLI with no profile, no keys, `AWS_CONFIG_FILE` and `AWS_SHARED_CREDENTIALS_FILE` at `/dev/null`, IMDS disabled, `--no-notices --no-version-reporting`. The template then holds `AWS::AccountId` references, is machine-independent, and no `cdk.context.json` appears. The stray file was deleted; `cdk.context.json` is now in `deploy-extensions/.gitignore` (it embeds the account id), because a real `cdk diff`/`deploy` by the owner makes one AZ lookup. Disclosed here because those first calls (STS identity, EC2 availability zones) were made against the ambient account.
- **Files modified:** `justfile`, `deploy-extensions/.gitignore` (not in the plan's file list).

### Differences from the plan text (no behaviour lost)

- 36 mutation cases, not 31: the plan's 31 plus the digest literal on the pack side, the missing `start_time`, a machine timeout of 36001, a widened finalizer S3 right, and a policy attached to a role outside the stack.
- The check also asserts per-principal S3 grants for EQUALITY (not only the named PutObjectTagging), the finalizer's ECS cluster condition and SageMaker `training-job/laya-*` scope, the DLQ's 14-day retention and the FinalizerErrors alarm shape.
- The images stack uses `LinuxBuildImage.STANDARD_7_0` (x86_64) and `LinuxArmBuildImage.AMAZON_LINUX_2023_STANDARD_3_0` (aarch64); the plan said only "standard image" and "aarch64 standard image".
- `VolumeSizeInGB 100` for the training job was not specified in the plan; chosen as above.
- The recipe's tool-missing and digest refusals are `REFUSED tool:` and `REFUSED digest:` (exit 2), beyond the plan's `REFUSED env:`.
- Literal `cargo clippy` was not part of this plan (no Rust changed).

**Total deviations:** 4 auto-fixed (1 bug, 2 missing-critical, 1 blocking), 5 differences. **Impact:** none weakens a must-have; deviation 4 is disclosed because it touched the ambient account read-only.

## Auth Gates

None. No `cdk deploy`, no AWS mutating call, no push. See deviation 4 for the two read-only calls made before the synth was isolated.

## Issues Encountered

- The rtk hook rewrites `npx cdk ...` arguments in the Bash tool; the node_modules `cdk` binary was called directly for manual runs. The `just` recipe uses `npx` (the hook does not apply inside just's shell).
- `just` and the CDK app both print "AI agent detected" and the unconfigured-feature-flags notice on stderr; neither is a failure.

## Known Stubs

None. (The all-zero placeholder digests are announced `PLACEHOLDER DIGESTS (synth only)`; 14-09's deploy refuses them.)

## Not Proven Here (belongs to 14-09, 14-12, 14-14)

- No deploy: IAM trust and the account's service quotas (an `ml.g5.xlarge` training-job quota in the target region is not asserted anywhere) are first exercised at the owner-run `diff`/`deploy` in 14-12.
- The buildspec's `docker inspect ... RepoDigests` extraction and the two CodeBuild builders have not run (the pack Dockerfile was smoked natively in 14-07; the train image only `docker build --check` locally).
- The state machine's `Environment` and `Parameters` shapes are what the CDK renders for the SDK integration; whether SageMaker accepts them (for example an empty `LAYA_EPOCHS`) is first shown by a real probe job.
- Recovery behavior (lost events, a live ECS task after cancel) is failure-injected in 14-05 and observed live in 14-14; this plan proves only the declarations those depend on.

## Threat Flags

None. T-14-30, T-14-31, T-14-58, T-14-32, T-14-33, T-14-34, T-14-67, T-14-68 and T-14-69 are each covered by named assertions and mutation cases (principal scoping and wildcard refusals, the literals and pins, the digest-literal pair, bucket posture, the lifecycle trio, IMMUTABLE tags, untagged-only ECR expiry, the reconcile/DLQ/alarm trio, PutObjectTagging x2).

## User Setup Required

None for this plan. Owner-run, later: `diff`/`deploy` of both stacks in 14-12 (a bootstrapped account is assumed; the AZ lookup uses the CDK lookup role).

## Next Phase Readiness

- **14-09** (recipes): the stack contexts are `env`, `stack`, `trainImageDigest`, `packImageDigest`; the images stack outputs are `TrainRepoUri`, `PackRepoUri`, `TrainProjectName`, `PackProjectName`, `ContextBucketName`; the trainer outputs are listed in the README. The CodeBuild projects take their source per build (`--source-type-override S3 --source-location-override`) and require `GIT_COMMIT` (40 hex) as an environment override; they build `$DOCKERFILE` (set per project) with the context root as the working directory, so the context zip must contain the Dockerfile path as well as the nine context paths. A deploy must refuse the all-zero placeholder digests. Synth the stacks with the isolated-credential form used in the `laya-trainer-synth` recipe when no account is meant to be contacted.
- **14-13:** `RequestLambdaPolicy` is the stack output to attach after the 401 check; its ARNs are the real machine, table and bucket.
- **14-14:** the finalizer log group is `/aws/lambda/aprender-laya-train-finalizer-<env>`; the alarm is `aprender-laya-train-FinalizerErrors-<env>` and has no notification action.

## Self-Check: PASSED

Verified: the three created files and the five modified files exist; commits `ed40bad4e` and `1e20885aa` are ancestors of HEAD; `just laya-trainer-synth dev` prints `TRAINER STACK CHECK OK` and `TRAINER SYNTH OK env=dev`; `staging` exits 2 with `REFUSED env`; `just synth-training dev` still prints `synth OK`; `--selftest` ends `STACK CHECK SELFTEST OK` with 36 mutation cases (37 `ok` lines including the base case) and 0 FAIL, every required case family present; `LAYA_GATES_ONLY=trainer-synth just laya-gates-selftest` prints `DRIFT CHECK OK`, `ROW trainer-synth: GREEN`, `AWS CALLS: 0` and `LAYA GATES ROWS OK 1 of 46`; the two source-level mutations turned the check red and were restored; the README section is present; no commit deleted a file.
