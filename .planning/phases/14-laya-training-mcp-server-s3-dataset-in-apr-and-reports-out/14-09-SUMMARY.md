---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 09
subsystem: infra
tags: [aws, codebuild, cdk, sagemaker, s3, dry-run, gate-row, python-stdlib, laya]

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: 14-08's two stacks and their outputs; 14-06/14-07's context lists, base-stage.json and the probe modes of run_step.py; 14-11's gate-row helpers and REFUSED style
provides:
  - scripts/laya_deploy/laya_trainer_ops.py (image, infra, stage, probe subcommands behind one Runner seam; --state-dir; --write-placeholder-state; --selftest, 37 mutation-checked cases)
  - recipes laya-trainer-image, laya-trainer-infra, laya-trainer-stage, laya-trainer-probe
  - gate rows trainer-image, trainer-infra, trainer-stage, trainer-probe (DRY_RUN with AWS CALLS: 0, refusals as must_fail)
  - the gitignored state files images-<env>.json, outputs-images-<env>.json, outputs-<env>.json, datasets-<env>.json, probe-<id>.json under models/laya-trainer/
affects: [14-10, 14-12, 14-13, 14-14, 14-15]

plan_head_before: d31e17b5cc6cc0af53385461b1f66bcefc94249d
plan_head_after: 200a7ea1d057aee9b27c79deac62b5f54ae3ae7c

commits: 2
actuals:
  tokens: 29700     # chars/4 over the added lines of the realized diff (118,794 chars)
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "One Runner seam for every external command; DRY_RUN prints the command and executes only local read-only ones (git, the two just checks); a simulated world (git, just, aws, npx, SageMaker, CodeBuild) drives every selftest case on a fake clock"
    - "Every printed line passes redact() (12-digit runs bounded by non-alphanumerics), so ECR URIs, bucket names and role ARNs never reach stdout; the URIs live only in the gitignored state files"
    - "A dry run reports what a live run would refuse (a dirty context, an absent finalizer asset) as a DRY RUN NOTE instead of failing, so the gate rows stay hermetic"
    - "Deterministic last lines (IMAGE OK|FAILED, PROBE DONE|FAILED, INFRA OK|FAILED, STAGE OK|FAILED) so a backgrounded run is judged by its log, not the wrapper"

key-files:
  created:
    - scripts/laya_deploy/laya_trainer_ops.py
  modified:
    - justfile
    - scripts/laya_gates.tsv

key-decisions:
  - "The context zip is the image's context list PLUS its Dockerfile: CodeBuild runs `docker build -f $DOCKERFILE .` from the context root and the pack list names only cargo inputs, contracts and run_step files; 14-08 had already flagged this"
  - "A dry run notes (does not refuse) a dirty context path and an absent finalizer asset: a refusing dry run would turn the gate sweep red whenever a developer has uncommitted crate changes (every crates/ edit is under the pack context)"
  - "The sigterm probe waits for the container's sigterm-ready.json (at least 120 s after InProgress, capped 1,200 s more) before stopping the job: InProgress includes the 0.85 GB base download, and a stop before the container runs measures nothing"
  - "probe-<id>.json nests probe.json unchanged under `probe`; 14-12's evidence reads probe.<key>, plus job_name, final_status, failure_reason, billable_seconds, grace_seconds at the top level"
  - "Placeholder state is a tool feature (`--write-placeholder-state DIR`), written only into a state dir that passes the same ignore check and never over an existing file, so 14-12's dry runs never fake the real state"
  - "--stage-json is honoured only under DRY_RUN=1 (the live pin is the committed base-stage.json), mirroring LAYA_PMCP_CONFIG"

patterns-established:
  - "A mutation that survives names a missing case: the dataset size verification survived the first 14-mutation table and got its own case"
  - "A dry-run path that is never exercised hides a bug: the gate row (not the selftest) found describe-stacks answering no JSON under DRY_RUN; a case now pins it"

requirements-completed: [D-01, D-02, D-03, D-07, D-13, D-16]

coverage:
  - id: D1
    description: "Tracer: the image and infra subcommands end to end under DRY_RUN with zero aws/cdk executions; every recipe refuses profile != ze-kasher-dev and env outside dev|prod before any command; a live run's first command is `just _laya-account-check ze-kasher-dev us-east-1` and its refusal stops the run with exit 2 and no further call; state dirs inside the repo must be gitignored"
    requirement: "D-01"
    verification:
      - kind: integration
        ref: "python3 -I scripts/laya_deploy/laya_trainer_ops.py --selftest -> cases image-dry-run, account-check-first, profile-env (10 subcommand forms x live and dry, 0 commands run), state-dir, usage; LAYA_GATES_ONLY=trainer-image,trainer-infra just laya-gates-selftest -> DRIFT CHECK OK, LAYA GATES ROWS OK, AWS CALLS: 0"
        status: pass
    human_judgment: false
  - id: D2
    description: "Images: the zip is `git archive HEAD` of the image's own context list plus its Dockerfile (sorted, fixed timestamps, deterministic); the clean-tree check is scoped to exactly those paths (a dirty .planning/ file never refuses); the build is polled every 30 s to a 90-min cap; the digest is read from ECR by the commit tag; images-<env>.json gets {repo_uri, repo_name, digest, commit, build_id, image_bytes} and keeps the other image; stdout holds no 12-digit number; last line IMAGE OK/FAILED"
    requirement: "D-02"
    verification:
      - kind: unit
        ref: "selftest cases image-pack-context (the REAL repository at HEAD: 17,870 files = the pack list plus its Dockerfile, none under .planning/, docs/, models/), image-dirty, image-dirty-scope, image-dirty-dry, image-record, image-zip, image-polls, image-failed, image-timeout"
        status: pass
    human_judgment: false
  - id: D3
    description: "Infra: deploy refuses without real (non-zero, well-formed) digests for both images and runs cdk diff before cdk deploy --require-approval never with both digest contexts; outputs-images works from the images stack alone; outputs refuses before the trainer stack exists and writes the 12 trainer plus 5 images outputs mode 600"
    requirement: "D-02"
    verification:
      - kind: unit
        ref: "selftest cases infra-deploy-guard, infra-order, infra-diff-fails, infra-images-stack, infra-outputs-images, infra-outputs, infra-outputs-dry"
        status: pass
    human_judgment: false
  - id: D4
    description: "Stage: the base tar is uploaded only when its size and sha256 equal base-stage.json and the uploaded size is verified; datasets accept exactly task/train/eval/shift under the trainer's prefix grammar, --expect motivation-v1 binds the three files to Phase 12's inputs (f74a3ad3, f1d84f19, ee95d71f, confirmed against the real local data under DRY_RUN), and a prefix that already holds an object is refused with no put"
    requirement: "D-03"
    verification:
      - kind: unit
        ref: "selftest cases stage-base, stage-dataset (11 bad prefixes, extra/hidden/missing/symlinked/directory entries), stage-dataset-size, stage-expect, stage-immutable (the listing is the first aws call after the account check; a second stage of a staged prefix is refused)"
        status: pass
    human_judgment: false
  - id: D5
    description: "Probe: one ml.g5.xlarge job, MaxRuntime 1800, the base channel only, LAYA_STEP probe|probe-sigterm; a live run is refused with no call unless LAYA_TRAINER_APPROVED_SPEND equals probe-<mode>; sigterm grace is the max seconds_since_sigterm heartbeat (28 for heartbeats at 2..28 s); a cuda probe ending Stopped with MaxRuntimeExceeded is recorded (exit 0, last line PROBE DONE cuda Stopped) with the partial probe.json; probe.json fields are carried unchanged"
    requirement: "D-13"
    verification:
      - kind: unit
        ref: "selftest cases probe-request, probe-request-sigterm, probe-spend-guard, probe-cuda-completed, probe-train-step, probe-maxruntime, probe-sigterm-grace, probe-sigterm-noready, probe-failures; LAYA_GATES_ONLY=trainer-image,trainer-infra,trainer-stage,trainer-probe just laya-gates-selftest -> DRIFT CHECK OK, LAYA GATES ROWS OK 4 of 50"
        status: pass
    human_judgment: false
  - id: D6
    description: "The selftest and the rows can fail: 23 source mutations (account check removed, diff after deploy, unscoped status, no redaction, zero digest accepted, profile check removed, dirty ignored, pack entry dropped, state check removed, spend guard off or prefix-matched, immutability off, sha check off, allowlist off, 5 prefix segments, expect off, grace min, no ready gate, dropped probe field, MaxRuntime exit 1, two channels, MaxRuntime 3600, size verify off) each turned a named case red; with the spend guard removed the trainer-probe row turned RED; the file was restored byte-identical each time"
    requirement: "D-16"
    verification:
      - kind: other
        ref: "in-place mutation of laya_trainer_ops.py with a copy restored and cmp-checked after each run; one row-level mutation through LAYA_GATES_ONLY=trainer-probe"
        status: pass
    human_judgment: false

duration: 12min
completed: 2026-10-29
status: complete
---

# Phase 14 Plan 09: Owner-run AWS recipes Summary

**Every AWS action the Laya trainer needs before go-live (two image builds, both stacks, base and dataset staging, the two probe jobs) is one guarded recipe over `laya_trainer_ops.py`, each refusing foreign accounts, printing no account id, and provable offline with a DRY_RUN that makes zero aws or cdk calls, plus a 37-case simulated-world selftest whose cases were each shown able to fail.**

## Performance

- **Duration:** about 12 min wall clock from the first command (the start was recorded after reading the plan and the 14-xx hand-offs)
- **Tasks:** 2 (one tracer, one auto/tdd)
- **Files:** 1 created, 2 modified (`laya_trainer_ops.py` 1,891 lines, `justfile` +139, `laya_gates.tsv` +4)

## Accomplishments

- **One Runner seam, one account gate.** Every external command goes through `Runner.run`; DRY_RUN prints each as `+ ...` and executes only `local` ones (git reads and the two `just` checks). Each subcommand validates profile (`REFUSED profile:`) and env (`REFUSED env:`) first, then runs `just _laya-target-check` for the region and `just _laya-account-check <profile> <region>` before any other command; a refusal is re-emitted and exits 2.
- **image.** Zip from `git archive HEAD -- <context list + Dockerfile>` (sorted, 1980-01-01 timestamps, symlinks kept), `git status --porcelain -- <those paths>` as the clean check, `aws s3 cp` to the images stack's context bucket, `start-build` with DOCKERFILE, REPO_URI and GIT_COMMIT overrides, a 30 s poll to a 90-min cap, digest from `ecr describe-images --image-ids imageTag=<commit>`, merged into `images-<env>.json`. Repo names and digests only on stdout; last line `IMAGE OK <which> <digest>` or `IMAGE FAILED <which> <status>`.
- **infra.** `diff-images`, `deploy-images`, `outputs-images`, `diff`, `deploy`, `outputs`. Trainer `diff` and `deploy` need real digests for both images in `images-<env>.json`; `deploy` is `cdk diff` then `cdk deploy --require-approval never` in one invocation. `outputs-images` is the first-deployment bootstrap; `outputs` refuses before the trainer stack exists.
- **stage.** `base` checks size then sha256 against `base-stage.json` before any call, copies to `base/laya-55cf4c4e/laya-base.tar` and verifies the uploaded size. `dataset` allows exactly task/train/eval/shift under the trainer's prefix grammar, treats a prefix as immutable (a one-key listing is the first call after the account check), verifies each uploaded size and records the sha256s.
- **probe.** The request builder is a pure function (one `ml.g5.xlarge`, `MaxRuntimeInSeconds` 1800, one `base` channel, the image by `@digest`, `LAYA_STEP`, `LAYA_BUCKET`, `LAYA_PROBE_ID`); a live run needs `LAYA_TRAINER_APPROVED_SPEND=probe-<mode>`; results land in `probe-<id>.json`.
- **Gate rows.** `trainer-image`, `trainer-infra`, `trainer-stage`, `trainer-probe` each have must_fail refusals and must_pass DRY_RUN runs against placeholder state in a temp dir (`AWS CALLS: 0`, the global recorder never called). The full drift check passes.

## Task Commits

1. **Task 1 (tracer): image and infra recipes, rows and selftest** - `0b65f63ed` (feat)
2. **Task 2: stage and probe recipes, rows and selftest growth** - `200a7ea1d` (feat)

The tracer gate (the plan's Task 1 `<verify>` re-run end to end after the commit-ready tree: 23 cases ok, rows green, `models/decide` snapshot identical) passed before Task 2 began.

## Decisions Made

See `key-decisions`. The two that matter for 14-12: state shapes (below) and the dry-run-notes-not-refuses rule for a dirty context.

## State file shapes (for 14-10, 14-12, 14-13)

- `outputs-images-<env>.json`: `{schema, env, outputs: {ContextBucketName, TrainRepoUri, PackRepoUri, TrainProjectName, PackProjectName}}`
- `outputs-<env>.json`: `{schema, env, trainer: {BucketName, TableName, StateMachineArn, FinalizerName, SageMakerRoleArn, TrainRepoUri, PackTaskDefinitionArn, ClusterArn, SubnetIds, SecurityGroupId, FinalizerDlqUrl, RequestLambdaPolicy}, images: {the five above}}` (14-10's config render should read `trainer.BucketName`, `trainer.TableName`, `trainer.StateMachineArn`)
- `images-<env>.json`: `{schema, env, train: {repo_uri, repo_name, digest, commit, build_id, image_bytes}, pack: {...}}`
- `datasets-<env>.json`: `{schema, env, datasets: {<PREFIX>: {prefix: "datasets/<PREFIX>/", files: {name: sha256}, staged_at}}}`
- `probe-<id>.json` (`<id>` = `<mode>-<UTC yyyymmddhhmmss>`): `{schema, mode, probe_id, job_name, instance_type, max_runtime_seconds, final_status, failure_reason, secondary_status, training_time_seconds, billable_seconds, secondary_status_transitions, recorded_at, probe}` plus, for sigterm, `grace_seconds`, `heartbeats`, `sigterm_ready`, `stop_requested_after_in_progress_seconds`. `probe` is probe.json unchanged (null when the container wrote none).
- `python3 -I scripts/laya_deploy/laya_trainer_ops.py --write-placeholder-state DIR [--env dev|prod]` writes valid-shaped placeholder outputs-images, outputs and images files for 14-12's dry runs.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] `outputs-images` and `outputs` failed under DRY_RUN**
- **Found during:** Task 1, the first gate-row run (the selftest had not exercised the dry path)
- **Issue:** under DRY_RUN the runner answered the describe-stacks call with empty output, which the JSON parser rejected (`INFRA FAILED outputs DESCRIBE`).
- **Fix:** `describe_stack` supplies `[]` as the dry answer; the case `infra-outputs-dry` pins both dry commands (rc 0, `AWS CALLS: 0`, nothing written).
- **Files modified:** `scripts/laya_deploy/laya_trainer_ops.py`. **Commit:** `0b65f63ed`

**2. [Rule 2 - Missing critical] The CodeBuild context must contain the Dockerfile**
- **Found during:** Task 1 (14-08's hand-off, confirmed in the pack Dockerfile and the buildspec's `docker build -f "$DOCKERFILE" .`)
- **Issue:** the plan says the zip holds exactly the context list; the pack list does not name `deploy-extensions/container/pack-arm64/Dockerfile`, so the build could not start.
- **Fix:** the archive paths are the context list plus the image's Dockerfile when no listed path covers it (train's is already under `scripts/laya_train/`). The clean-tree check covers the same paths. `image-pack-context` asserts the listing equals the list at HEAD plus the Dockerfile.
- **Commit:** `0b65f63ed`

**3. [Rule 2 - Missing critical] Sigterm probe stop gated on the container's ready marker**
- **Issue:** the plan stops the job 120 s after InProgress; `InProgress` covers the 0.85 GB base-channel download, so a fixed 120 s can stop the job before the container runs, which measures no grace.
- **Fix:** the stop is sent at least 120 s after InProgress and as soon as `runs/probe/<id>/sigterm-ready.json` exists, or after 1,200 s more without it (recorded as `sigterm_ready: false`). Cases `probe-sigterm-grace` and `probe-sigterm-noready`.
- **Commit:** `200a7ea1d`

**4. [Rule 2 - Missing critical] A job that outlasts the poll cap is stopped**
- A probe not terminal after 3,600 s (create to terminal, start-up included) gets a `stop-training-job` and ends `PROBE FAILED <mode> POLL_TIMEOUT`, so a forgotten poll cannot leave a billed GPU job running (T-14-36). An image build past 90 min is abandoned and said not to be stopped (`IMAGE FAILED <which> POLL_TIMEOUT`).
- **Commit:** `200a7ea1d`

### Differences from the plan text (no behaviour lost)

- **A dirty context path or an absent finalizer asset is a `DRY RUN NOTE`, not a refusal, under DRY_RUN=1** (live runs refuse with `REFUSED image:` / `REFUSED infra:`). The plan states the refusal without a mode; refusing in dry runs would make the gate sweep red whenever a developer has uncommitted changes under `crates/`, which the pack context covers. Both modes are tested (`image-dirty`, `image-dirty-dry`, `infra-deploy-guard`).
- `infra diff` (not only `deploy`) requires real digests, and `deploy-images` also runs a diff first: the same review-then-apply rationale.
- The per-job `describe-training-job` failure policy (three consecutive failures end `PROBE FAILED probe DESCRIBE`) and the extra refusals (`REFUSED usage:`, an images file whose train repository differs from the stack's `TrainRepoUri`, a symlinked or non-regular dataset file, a hidden file such as `.DS_Store`) go beyond the plan's list.
- A dataset prefix nested under another dataset's prefix (`datasets/a/b/` when `datasets/a/` holds direct objects) is not detected: a correct check needs a delimiter listing that max-keys 1 cannot give. Use single-segment prefixes (the three planned ones are); 14-06's inputs check is the second line.
- `image_bytes` (from `ecr describe-images`) is recorded in `images-<env>.json` because 14-12's evidence asks for it "when CodeBuild reports them".
- `--stage-json`, `--write-placeholder-state` and `--list-files` are additions to the plan's option set (the first two for hermetic dry runs, the third for the pack-context case).

**Total deviations:** 4 auto-fixed (1 bug, 3 missing-critical), 5 differences. **Impact:** none weakens a must-have.

## Auth Gates

None. No AWS call, `cdk deploy` or push was made. Every selftest, row and manual run used DRY_RUN or the simulated world; the live (DRY_RUN=0) refusal row runs with the recorder `aws` first on PATH and the spend guard refuses before any command. No ambient credential or profile was read: the manual dry runs used a fixture pmcp config and the script strips `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` and `AWS_SESSION_TOKEN` from every child environment.

## Issues Encountered

- The rtk hook is not involved in the script's subprocess calls (`npx cdk` runs inside Python), so the 14-08 hook hazard does not apply to the live recipes.
- The first selftest build passed all 22 cases at once; the nine-then-fourteen mutation tables were run to prove that was not vacuous. One mutation (account check removed) crashed the selftest with `StopIteration` instead of failing a named case; the case now uses `next(..., default)` so it fails by name.

## Known Stubs

None. The placeholder state files are announced as placeholders and exist only to let a dry run read well-formed input; every live path refuses an all-zero digest.

## Not Proven Here (belongs to 14-12 and the first real run)

- No live AWS call ran: the CodeBuild start/poll/`describe-images` shapes, `cloudformation describe-stacks` against a real stack, `sagemaker create-training-job` accepting this request (an empty `Environment` value, FILE input mode on a prefix channel, the 100 GB volume), `stop-training-job` timing and the real SIGTERM grace are first exercised by the owner-run recipes in 14-12.
- `cdk diff`/`cdk deploy` run `npx` from `deploy-extensions/`; whether the owner's `ze-kasher-dev` profile carries a CDK bootstrap and a quota for one `ml.g5.xlarge` training job is not asserted anywhere.
- The finalizer asset `deploy-extensions/assets/laya-finalizer/bootstrap` is not rebuilt by `infra diff|deploy`; run `just laya-trainer-synth <env>` after any change to the finalizer crate (a live run refuses when the file is absent but cannot tell that it is stale).

## Threat Flags

None. T-14-35 (profile/env refusals and the account check first), T-14-70 (immutable prefixes), T-14-36 (spend guard, MaxRuntime 1800, one instance, the poll-cap stop), T-14-37 (sha256 pins, the dataset allowlist, `--expect`), T-14-38 (clean-tree check, commit-tagged images, digest read back from ECR) and T-14-39 (state under the gitignored `/models`, `check-ignore` asserted and redaction of every printed line) are each covered by named cases above.

## User Setup Required

None for this plan. Owner-run, in 14-12 and with the owner's go: the commands in the module docstring, in the order of `deploy-extensions/README.md` ("Order of the first deployment"). `image` and `probe` must run in the background and be judged by their last line.

## Next Phase Readiness

- **14-10** extends the same script (`config`, `deploy`) and its selftest; read `outputs-<env>.json` as `trainer.<Key>` and reuse `state_dir`, `read_state`, `write_state`, `connect` and `Runner`. Add its cases to `selftest()` before the `placeholder-state` block; `go()`/`world()`/`Sim` are the scaffolding.
- **14-12** runs the recipes with `--state-dir <temp>` created by `--write-placeholder-state` for the dry runs, then live with the owner. Judge `image` by `IMAGE OK|FAILED`, `probe` by `PROBE DONE|FAILED`. `images.{train,pack}.repo_uri` stays only in the gitignored state file; the evidence takes `repo_name`, `digest`, `commit`, `build_id`.
- `python3 -I scripts/laya_deploy/laya_trainer_ops.py --selftest` runs offline in under 1 s (the pack-context case reads the real tree list).

## Self-Check: PASSED

Verified: `scripts/laya_deploy/laya_trainer_ops.py` exists; commits `0b65f63ed` and `200a7ea1d` are ancestors of HEAD (`commits: 2` measured with `git rev-list --count d31e17b5c..HEAD`); the selftest prints 37 ok, 0 FAIL, ends `LAYA TRAINER OPS SELFTEST OK`, and every case family the two `<verify>` scripts name is present; `LAYA_GATES_ONLY=trainer-image,trainer-infra,trainer-stage,trainer-probe just laya-gates-selftest` prints `DRIFT CHECK OK` and `LAYA GATES ROWS OK 4 of 50` with `AWS CALLS: 0`; the `models/decide` snapshot is cmp-identical before and after; `models/laya-trainer/images-dev.json` is gitignored and `models/laya-trainer/` was never created; no commit deleted a file.
