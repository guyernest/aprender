# 14 IAM fix note: ListBucket condition (2026-10-29)

Status: code and offline proof DONE (commit 65e1b754a). Live redeploy NOT done: held at the diff
because it shows one change beyond IAM policy (see "Diff").

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

## Next

Owner decision: either accept the finalizer code swap (same sources, rebuilt) and run
`just laya-trainer-infra deploy dev ze-kasher-dev`, or restore the previously deployed asset first. The
live 404-versus-403 behaviour is first observed in plans 14-13/14-14.
