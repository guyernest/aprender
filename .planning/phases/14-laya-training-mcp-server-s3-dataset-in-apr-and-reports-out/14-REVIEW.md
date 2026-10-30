---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
reviewed: 2026-10-29T00:00:00Z
depth: standard
files_reviewed: 36
files_reviewed_list:
  - Cargo.toml
  - crates/aprender-core/tests/monorepo_invariants.rs
  - crates/aprender-mcp-laya-train-lambda/.pmcp/deploy.toml.template
  - crates/aprender-mcp-laya-train-lambda/Cargo.toml
  - crates/aprender-mcp-laya-train-lambda/src/bin/aprender-laya-train-finalizer.rs
  - crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs
  - crates/aprender-mcp-laya-train-lambda/src/compute.rs
  - crates/aprender-mcp-laya-train-lambda/src/dataset.rs
  - crates/aprender-mcp-laya-train-lambda/src/finalizer.rs
  - crates/aprender-mcp-laya-train-lambda/src/lib.rs
  - crates/aprender-mcp-laya-train-lambda/src/objects.rs
  - crates/aprender-mcp-laya-train-lambda/src/sfn.rs
  - crates/aprender-mcp-laya-train/Cargo.toml
  - crates/aprender-mcp-laya-train/src/finalize.rs
  - crates/aprender-mcp-laya-train/src/layout.rs
  - crates/aprender-mcp-laya-train/src/lib.rs
  - crates/aprender-mcp-laya-train/src/lock.rs
  - crates/aprender-mcp-laya-train/src/validate.rs
  - deploy-extensions/bin/app.ts
  - deploy-extensions/container/pack-arm64/Dockerfile
  - deploy-extensions/container/pack-arm64/smoke.sh
  - deploy-extensions/lib/laya-trainer-images-stack.ts
  - deploy-extensions/lib/laya-trainer-stack.ts
  - justfile
  - scripts/laya_deploy/laya_fetch.py
  - scripts/laya_deploy/laya_repro_eval.py
  - scripts/laya_deploy/laya_trainer_ops.py
  - scripts/laya_deploy/laya_trainer_stack_check.py
  - scripts/laya_deploy/train_call.py
  - scripts/laya_train/container/Dockerfile.train
  - scripts/laya_train/container/run_step.py
  - scripts/laya_train/container/stage_base.py
  - scripts/laya_train/data.py
  - scripts/laya_train/data_cases_gen.py
  - scripts/laya_train/pyproject.toml
  - scripts/laya_train/train.py
findings:
  critical: 0
  warning: 4
  info: 3
  total: 7
status: issues_found
---

# Phase 14: Code Review Report

**Reviewed:** 2026-10-29
**Depth:** standard
**Files Reviewed:** 36
**Status:** issues_found

## Summary

Scope was the Phase 14 Laya training server: the server library (validate, lock, finalize decision, layout), the Lambda
deployment unit (S3 dataset source, Step Functions dispatcher and cancel sink, compute gate, finalizer), the two CDK
stacks, the two container programs (`run_step.py` train and pack modes), and the owner-run tooling (`laya_trainer_ops.py`,
`laya_fetch.py`, `train_call.py`, `laya_repro_eval.py`).

I found no injection, path-traversal, secret-handling or authorization defect. The path guards are consistent end to end:
`parse_dataset_uri` and `segment_ok` on the request side, `guard_link_key` on presigning, `check_entry_path` and
`dest_for` on fetch, and `parse_training_manifest` on the pack side. The wire shapes also agree across the three
languages. I checked `Outcome`, `TrainingManifest`, `PackOutcome` and `ExecutionInput`, which are
`deny_unknown_fields` in Rust, against what `run_step.py` and the CDK payloads write, and the field sets match. The IAM
statements match the code paths that need them. The `train.py` to `data.validate_data_dir` refactor preserves the check
order and still binds `seed`.

The defects that remain are behavioural. The most consequential is that the finalizer's refused, crash and timeout
classification (including the D-08 `prefilter_disagreement` flag) is unreachable on the normal path. Next are a
finalization wedge on a manifest entry whose object does not exist, client-visible AWS error text carrying account
identifiers, and a finalizer asset deployed without provenance checks.

Out of scope and not re-reported, per the brief: the cancel-sink runtime-per-call concern, the deferred efficiency
items, the justfile preamble duplication, the Mac-vs-Graviton parity bound (D-15), the untracked `.env.m2m`, and the
Python/Rust validator mirror.

Review depth note: `data_cases_gen.py` and `laya_trainer_stack_check.py` were skimmed (a generator and a mutation-tested
template checker) and the `run_step.py` probe modes and selftest were not read line by line.

## Warnings

### WR-01: the refused / crash / timeout / D-08 classification in `finalize_run` is unreachable on the normal path

**File:** `crates/aprender-mcp-laya-train/src/finalize.rs:430` (with `scripts/laya_train/container/run_step.py:193-195` and `deploy-extensions/lib/laya-trainer-stack.ts:508-509`)
**Issue:** `run_step.py` exits 1 for every exit class except `pass` and `gate_fail` (`job_exit_code`), so a refused, crashed
or timed-out trainer makes the SageMaker job `Failed`. The `Train` state then fails, and
`train.addCatch(catchFinalize, { errors: [ALL], resultPath: '$.error' })` always hands the finalizer a populated `error`.
`finalize_run` returns at `if let Some(error) = &input.stage_error { return b.failed(error, None, false); }` (line 430)
before it reads `outcome.json`. Consequences:
- A trainer refusal the pre-filter accepted never sets `prefilter_disagreement: true`. That is exactly the D-08 "defect to surface" row of the decision table.
- The `REFUSED <rule>: ...` line the trainer recorded in `outcome.refused_line` never reaches the caller. They get the SFN Cause text instead.
- The `Refused`, `Crash` and `Timeout` arms (lines 442-470) are reachable only through the Event and reconcile paths, which pass `stage_error: None`. The machine catches every Train failure and ends `SUCCEEDED`, so those paths do not see such runs.
- The live gate run exercised the pack-refusal path, which exits 0, so no live evidence covers this.

**Fix:** When `outcome.json` parsed and names this task, let it decide the class and use `stage_error` only as the
fallback when the outcome is absent (reorder the checks in `finalize_run`):
```rust
if input.outcome.is_none() {
    if let Some(error) = &input.stage_error {
        return b.failed(error, None, false);
    }
    return b.failed("outcome.json is missing: the training stage left no record", None, false);
}
// then the existing `match outcome.exit_class { ... }`; attach stage_error as extra detail on the Crash arm
```
Add a test with `stage_error: Some(..)` and an outcome `refused` (not `inputs-changed`) that asserts
`prefilter_disagreement == true`. The existing tests almost certainly pass `stage_error: None` for these rows.

### WR-02: finalization wedges permanently when a manifest lists an object that does not exist

**File:** `crates/aprender-mcp-laya-train-lambda/src/finalizer.rs:874-905` (with `scripts/laya_train/container/run_step.py:759` and `:1313`)
**Issue:** `run_step.finish` appends an `outcome.json` entry to `training-manifest.json` before it uploads `outcome.json`
(line 759, upload at line 764). `publish_pack` does the same for `pack-outcome.json` (line 1313). If that last upload
fails after its retries, the job exits 1 and the manifest still lists an object that is not in S3.

The finalizer then builds `targets` from every manifest entry (line 874) and calls `get_tag` on each. S3
`GetObjectTagging` on a missing key returns NoSuchKey, which becomes `store_err` and returns `Err` from `publish`.

The record was already settled at step 2, so the lock stays held. Every 10-minute sweep re-enters `finalize_task`, fails
the same way and trips the FinalizerErrors alarm. This continues until the lease runs out (11 h), because
`reconcile` filters on `!released`, not on liveness, so it keeps retrying after the lease expires too.

Trigger probability is low (3 failed attempts at 900 s on the last upload), but the failure mode is an unbounded alarm
storm plus up to 11 h of single-flight lockout, and nothing in the code can clear it.

**Fix:** Tolerate a listed-but-absent object in the tagging pass. The `list(prefix)` result already computed above gives
the set of existing keys:
```rust
let existing: BTreeSet<&str> = listed.iter().map(|(k, _)| k.as_str()).collect(); // hoist `listed` out of the manifest-building branch
targets.retain(|key| existing.contains(key.as_str()));
```
Report the dropped paths in `FinalizeReport` (or bump `unlisted`) instead of failing. Alternatively have the containers
write the manifest entry for `outcome.json` and `pack-outcome.json` only after the upload returned success, and rewrite
the manifest if needed. That conflicts with the "outcome LAST is the completion marker" rule, so the finalizer-side fix
is the safer one.

### WR-03: raw AWS error text reaches API callers and the stored status payload, including account identifiers

**File:** `crates/aprender-mcp-laya-train-lambda/src/sfn.rs:142-145`, `crates/aprender-mcp-laya-train/src/lib.rs:595`, `crates/aprender-mcp-laya-train-lambda/src/finalizer.rs:263-269` (stored at `finalize.rs:291`)
**Issue:** Two paths pass infrastructure error text through to the client unmodified.

1. **Dispatch.** `SfnExecutions::start` sets `StartError.message = DisplayErrorContext(&e).to_string()`. `submit` returns
   `pmcp::Error::validation("REFUSED dispatch: {reason}")` with that text and also stores it in the compensated
   payload's `error`. An AccessDenied on `states:StartExecution` carries the caller role ARN and the state machine ARN
   (both embed the 12-digit account id). So do `ExecutionLimitExceeded`, throttling and DynamoDB or lock backend errors
   (`AcquireError::Backend`).
2. **Catch.** `bounded_error` forwards the SFN Catch `Error: Cause` text, truncated to 2048 and then 1024 characters.
   For a failed `.sync` SageMaker job the `Cause` is the `DescribeTrainingJob` JSON, whose leading fields include
   `AlgorithmSpecification.TrainingImage`. That is the ECR URI with the account id and, on the finalizer side, role and
   resource ARNs. `train_status` serves it to the caller.

The same module is careful elsewhere: `dataset.rs` documents "names the bucket and prefix, never the SDK text". The repo's
own tooling is careful to print no 12-digit number. These two paths leak what that discipline protects.

**Fix:** Log the full text server-side with `tracing::error!`. Return a fixed, caller-safe string built from the error
name or kind only. For example, store `format!("dispatch failed ({kind:?})")`, and from the Catch keep only `Error`
(for example `States.TaskFailed`) plus, where available, the SageMaker `FailureReason` line after a redaction pass that
drops `arn:` tokens and 12-digit runs.

### WR-04: the finalizer Lambda asset has no provenance or freshness check at deploy time

**File:** `scripts/laya_deploy/laya_trainer_ops.py:670-675` (asset path at `:136`), `deploy-extensions/bin/app.ts` (finalizerAssetPath check)
**Issue:** The two container images are built from `git archive HEAD` of a clean context and are tagged by commit. The
finalizer is the "one writer of the terminal record, verdict, manifest, tags and lock release", yet `infra diff|deploy`
and `cdk` only require that `deploy-extensions/assets/laya-finalizer/bootstrap` exists. That file is an untracked
artifact written by `just laya-trainer-synth` from whatever the working tree held at the time. If the finalizer sources
change after the last synth (or the tree was dirty or on another branch), `just laya-trainer-infra deploy` silently ships
a stale or unreviewed binary. The CDK asset hash only changes when the binary does, so there is no signal.

**Fix:** Have `laya-trainer-synth` write a sidecar next to the asset (`bootstrap.build.json` with `git rev-parse HEAD`, a
clean-tree flag scoped to `crates/aprender-mcp-laya-train*` and `Cargo.lock`, and the binary sha256). Make `cmd_infra`
refuse a deploy when the sidecar's commit is not `HEAD`, the tree for those paths is dirty, or the sha256 does not match
the file. This mirrors the existing `image` recipe's clean-tree check.

## Info

### IN-01: `training-manifest.json`, `partial-manifest.json` and `pack-manifest.json` expire in 30 days even for a kept run

**File:** `crates/aprender-mcp-laya-train-lambda/src/finalizer.rs:874-905` (with `SELF_LISTING` at `:67-73`; lifecycle rule at `deploy-extensions/lib/laya-trainer-stack.ts:127-134`)
**Issue:** The retention tag is applied only to manifest entries plus `manifest.json`. The three manifests that cannot list
themselves are in `SELF_LISTING`, so they keep the upload-time `laya-retention=expire` tag. For a `keep` run they are
deleted after 30 days while the `.apr`, the reports and `verdict.json` stay. `manifest.json` carries the image digests
and contract hashes, so provenance survives. The loss is only the per-stage inventories and the training-side
`gate_contract_sha256` in their original form. If that is intended, say so in the `SELF_LISTING` doc comment. If not,
add the three keys (when they exist) to `targets`.

### IN-02: an immutable commit tag makes a failed or partial image build for a commit impossible to repeat

**File:** `deploy-extensions/lib/laya-trainer-images-stack.ts:46-49` (buildspec) and `:84` (`IMMUTABLE`)
**Issue:** The buildspec pushes `$REPO_URI:$GIT_COMMIT` into an IMMUTABLE repository. If a build pushed and then failed
later (digest extraction, a timeout while polling), re-running `just laya-trainer-image` for the same `HEAD` fails at
`docker push` ("tag ... cannot be overwritten"). The only way out is a new commit. Consider checking
`aws ecr describe-images --image-ids imageTag=$GIT_COMMIT` first and printing that digest instead of rebuilding, which is
also the correct behaviour since a rebuild would not be byte-identical.

### IN-03: pre-filter and trainer gate contracts can skew with no signal

**File:** `crates/aprender-mcp-laya-train/src/finalize.rs:212-232` (the `Contracts` assembly in `Builder::manifest`)
**Issue:** `Manifest.contracts` records `prefilter_gate` (the request function's embedded contract hash) and `train_gate`
(the trainer image's). Neither `finalize_run` nor the finalizer compares them. The request function and the training
image are deployed by different recipes, so they can sit on different contract versions, and the epoch rule or
calibration constants the pre-filter judged by would then differ from the trainer's. Only a human diffing the manifest
would notice. Given WR-01, the `prefilter_disagreement` flag is also not set on that path. A one-line comparison that
adds `error`/a note or sets a manifest flag when both are present and differ would make the skew observable.

---

_Reviewed: 2026-10-29_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
