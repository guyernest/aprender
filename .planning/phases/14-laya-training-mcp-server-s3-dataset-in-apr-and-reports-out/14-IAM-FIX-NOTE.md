# 14 IAM fix note: ListBucket condition (2026-10-29)

Status: DONE. Code and offline proof in commit 65e1b754a; trainer stack redeployed once by the owner
(own shell) and verified read-only.

## Defect

Every `s3:ListBucket` grant in `deploy-extensions/lib/laya-trainer-stack.ts` was conditioned on
`StringLike` `s3:prefix`. GetObject/HeadObject carry no `s3:prefix` key, so the condition fails and S3
answers 403 AccessDenied where it should answer 404 NoSuchKey. The finalizer's `ObjectStore::get`
treats only NoSuchKey as absent, so the first finalization of a run (no `verdict.json` yet) or a
crashed job (no `outcome.json`) would error and keep the single-flight lock held until the 11 h lease.

## Change

`StringLikeIfExists` on all three ListBucket statements (SageMaker role, finalizer role, request-Lambda
policy output). Prefix lists unchanged. `scripts/laya_deploy/laya_trainer_stack_check.py` now asserts the
new operator; its selftest gained five cases (old `StringLike` on request policy, finalizer and SageMaker
rejected; prefix restriction dropped on finalizer and SageMaker rejected): 41 mutation cases, 0 failures.
README note updated.

## Offline proof

- `just laya-trainer-synth dev`: `TRAINER STACK CHECK OK trainer=205 images=105 assertions`, `TRAINER SYNTH OK env=dev`
- `--selftest`: `41 mutation cases, 0 failure(s)`, `STACK CHECK SELFTEST OK`
- `LAYA_GATES_ONLY=trainer-synth just laya-gates-selftest`: `ROW trainer-synth: GREEN`, `AWS CALLS: 0`

## Diff (`just laya-trainer-infra diff dev ze-kasher-dev`, read-only change set, rc 0)

Expected: SageMaker and finalizer role default policies (StringLike -> StringLikeIfExists), the
RequestLambdaPolicy output.

NOT expected: `AWS::Lambda::Function Finalizer` Code.S3Key changes
`ec55d7ac...zip` -> `7288da6d...zip`. The finalizer crate sources have not changed since before the
14-12 deploys (last touching commit e3a3aea73, 08:28 local; deploys 10:32 and 11:11), so the new asset hash
comes from rebuilding the bootstrap during `laya-trainer-synth` (a non-reproducible rebuild or a build
environment difference), not from a source change. A deploy would therefore also replace the live
finalizer's code with an unproven rebuild, outside the approved IAM-only scope. Stopped here.

## Finalizer asset swap (accepted)

The orchestrator accepted the `Finalizer` `Code.S3Key` change (`ec55d7ac...zip` -> `7288da6d...zip`) as a
same-source rebuild: sources unchanged since e3a3aea73, before the 14-12 deploys, tests 60/60 on that source.
The rebuild is not byte-reproducible across synths, so any future trainer-stack deploy that follows a fresh
`laya-trainer-synth` will re-upload the finalizer even with no source change.

## Deploy (run by the owner, `assume ze-kasher-dev && just laya-trainer-infra deploy dev ze-kasher-dev`)

`INFRA OK deploy`; stack `aprender-laya-trainer-dev` updated in 39.55 s. The `cdk diff` it ran showed exactly the
four changes listed above (SageMaker and finalizer ListBucket operators, the RequestLambdaPolicy output, the
Finalizer code key) and nothing else. Image digests unchanged (train b49b3fe9..., pack 504084d0...).
`just laya-trainer-infra outputs dev ze-kasher-dev` then refreshed the state file
(`wrote outputs-dev.json (12 trainer outputs, 5 images outputs)`, `INFRA OK outputs`).

## Verification (read-only, profile ze-kasher-dev, us-east-1)

- `cloudformation get-template` of the deployed stack: SageMaker role default policy ListBucket condition
  `StringLikeIfExists s3:prefix [datasets/*, base/laya-55cf4c4e/*]`; finalizer default policy
  `StringLikeIfExists s3:prefix [runs/*]`; `RequestLambdaPolicy` output `StringLikeIfExists s3:prefix [datasets/*]`.
- `iam get-role-policy` on the live roles: SageMaker `StringLikeIfExists [datasets/*, base/laya-55cf4c4e/*]`,
  finalizer `StringLikeIfExists [runs/*]`. Prefix lists unchanged.

The live 404-versus-403 behaviour is first observed in plans 14-13/14-14 (not a policy question, so not simulated).
