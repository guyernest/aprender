---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 07
subsystem: pack-container
tags: [fargate, arm64, docker, pack-laya, verify, s3, laya, run-step, pilot-waiver]

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: "14-06 run_step.py (Runner seam, Uploader, stage_channel_base, selftest printer), stage_base.py, base-stage.json; 14-01 golden fixtures (pack-outcome-v1.json, training-manifest-v1.json, outcome-v1.json)"
provides:
  - "scripts/laya_train/container/run_step.py: mode `pack` (download, re-hash, pack, verify, upload only what verify accepted), 154 selftest cases"
  - "deploy-extensions/container/pack-arm64/Dockerfile: the linux/arm64 pack-and-verify image (412 MB), pack_laya built as an example at /src"
  - "deploy-extensions/container/pack-arm64/context-paths.txt: the pack image's whole build context (nine paths, read from the git index)"
  - "deploy-extensions/container/pack-arm64/smoke.sh: the native arm64 image smoke (SMOKE OK)"
affects: [14-08, 14-09, 14-12, 14-15]

requirements-completed: [D-14, D-15, D-04, D-19, D-20]

plan_head_before: c888508686583ed5452ee9f1900b462188994ce5
plan_head_after: a195c8e4adb2ecf3105f3ca612bcdc9394e499f3

commits: 2
actuals:
  tokens: 16500      # chars/4 over the added lines of run_step.py and pack-arm64/* (65,958 chars)
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "the acceptance decision is a single boolean computed from the children's own outputs and a local re-hash, before any upload; the .apr upload is the only call guarded by it"
    - "the pack children get no environment but PATH, HOME, LANG and TMPDIR and path arguments only; the policy path is a constant a selftest parameter alone can change"
    - "a failed smoke control matters as much as the pass: the same verify with /src/contracts hidden must fail as `REFUSED Usage ... contracts`"
    - "the build context is the git index restricted to a checked-in path list, extracted to a scratch directory and built with -f pointing outside it"

key-files:
  created:
    - deploy-extensions/container/pack-arm64/Dockerfile
    - deploy-extensions/container/pack-arm64/context-paths.txt
    - deploy-extensions/container/pack-arm64/smoke.sh
  modified:
    - scripts/laya_train/container/run_step.py

key-decisions:
  - "Skip is decided on the outcome alone wherever possible (not pass/gate_fail, or device_ok false) before any manifest is read; only a gate_fail needs the manifest, to find and re-hash run/gate-report.json for the pilot_waiver claim"
  - "Refusal text is kept where the finalizer shows it: pack-outcome packed_line carries pack's REFUSED line when pack refused, verify_line carries verify's REFUSED line; exit_code is the first child that did not exit 0"
  - "A failed .apr upload un-accepts the run (accepted false, apr_sha256 null, exit 1) so pack-outcome never claims a model that is not in the bucket"
  - "The base tar key is derived from base-stage.json (base/laya-<8 of revision>/<tar_name>); the selftest compares it with layout::BASE_KEY in the Rust crate when that source is present"
  - "RUSTUP_TOOLCHAIN=1.93.0 in the build stage names the compiler rust-toolchain.toml pins without rustup fetching rustfmt and clippy"

duration: 18 min
completed: 2026-10-29
status: complete

coverage:
  - id: D1
    description: "run_step pack: exactly the manifest's run/ and data/ entries are downloaded and re-hashed; the base tar is pinned before safe_extract; pack then (only on 0) verify with path arguments only and a stripped child env; accepted needs pack 0, verify 0, deploy_eligible, PACKED sha equal to the local hash, and agreeing waived values"
    verification:
      - kind: unit
        ref: "python3 -I scripts/laya_train/container/run_step.py --selftest (pack-accepted, pack-gate-failed, pack-refused, verify-refused, verify-not-eligible, sha-mismatch, pack-argv, pack-integrity, pack-base-pin)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Pilot-waiver policy: pack runs for pass, and for gate_fail only with a pilot_waiver claim in the re-hashed gate-report.json; any other outcome (or device_ok false) is recorded ran false with no base download"
    verification:
      - kind: unit
        ref: "run_step selftest pack-skip (7 variants), pack-waiver-claim, pack-waiver-refused, pack-waiver-disagree (two directions)"
        status: pass
    human_judgment: false
  - id: D3
    description: "pack-outcome.json and pack-manifest.json have exactly the 14-01 fixture key sets; image digest, both contract hashes and the two wall times are recorded on every outcome, skips included; every upload is tagged laya-retention=expire with pack-outcome.json last"
    verification:
      - kind: unit
        ref: "run_step selftest pack-keys, pack-contracts, pack-timings, pack-image-env"
        status: pass
    human_judgment: false
  - id: D4
    description: "The pack image builds natively for linux/arm64 from the index list, pins both bases by digest, and inside it pack-fixture reproduces 37d65159..., inspect accepts, verify refuses with a real refusal, the hidden-contracts control fails as a usage error, and the selftest passes with no mount"
    verification:
      - kind: other
        ref: "smoke.sh -> SMOKE OK image sha256:d5b0036a... pack_laya 7451008 bytes; FIXTURE SHA256 37d65159b2be0fa091aa840cd56c1a84b73c0bcd9e2df5906d1f8218f5448561; verify `REFUSED SyntheticNotDeployable`; negative control `REFUSED Usage /src/crates/aprender-decide/../../contracts/laya-finetune-gate-v1.yaml: No such file`; in-image selftest 154 ok, 0 FAIL"
        status: pass
    human_judgment: false
---

# Phase 14 Plan 07: arm64 pack and verify Summary

**`run_step.py pack` downloads a finished run, re-hashes every file, runs `pack_laya pack` then `verify` with path arguments and a stripped environment, and uploads the `.apr` only when verify accepted exactly the bytes pack wrote; the linux/arm64 image that carries `pack_laya` at the fixed path its contracts resolve from builds natively, reproduces the golden fixture hash and passes its own selftest with no mount.**

## Performance

- **Duration:** 18 min (07:27Z to 07:46Z)
- **Tasks:** 2 (one tracer, one auto)
- **Files:** 1 modified (`run_step.py`, +826 lines), 3 created under `deploy-extensions/container/pack-arm64/`

## Accomplishments

- **Pack mode.** `LAYA_STEP=pack` (or the argument `pack`, the image's CMD). Env is `LAYA_TASK_ID`, `LAYA_NAME`, `LAYA_BUCKET`, `LAYA_RUN_PREFIX` plus `LAYA_PACK_IMAGE_DIGEST`; anything missing or malformed is `REFUSED env:` exit 2 before a single download (six variants tested, the digest never echoed).
- **Skip rule.** `pass` packs. `gate_fail` packs only when `run/gate-report.json` (downloaded, re-hashed against `training-manifest.json`) carries a `pilot_waiver` object with an id; train.py's contract is that the claim never changes `pass` or the exit status. A `gate_fail` without a claim, `refused`, `crash`, `timeout`, or `device_ok` false is recorded `{ran false, skipped <exit_class>}`, exit 0, with no base download. Seven skip variants are tested, including a claim with `device_ok` false and a partial manifest that lacks the report.
- **Inputs.** Exactly the manifest's `run/` and `data/` entries are fetched with `aws s3api get-object`, size and sha256 checked (`ERROR integrity:` exit 1; a manifest path with `..`, an absolute path or a repeat is refused the same way). The base tar is fetched at the key derived from `base-stage.json`, its sha256 compared with the pin before `safe_extract`, and the extracted tar deleted to free disk.
- **Children.** `pack_laya pack --run R --data D --base B --out <name>.apr` then `pack_laya verify <name>.apr --run R --data D --base B`, each timed, stdout and stderr to `pack.log` / `verify.log`, environment cut to `PATH`, `HOME`, `LANG`, `TMPDIR`. The selftest runs with `LAYA_POLICY_PROBE`, `PACK_POLICY_OVERRIDE` and `AWS_ACCESS_KEY_ID` set in the parent and proves the fake `pack_laya` sees none of them.
- **Acceptance.** `accepted` = pack 0, verify 0, `deploy_eligible` true, the PACKED sha256 equals the local hash of the written file, and PACKED `waived=` equals the verify JSON `waived`. Only then is the `.apr` uploaded. Five refusal shapes (pack exit 3, pack exit 2, verify exit 2, `deploy_eligible` false, sha mismatch) plus both waiver-disagreement directions each give `accepted false`, no `.apr` anywhere in the bucket, reports untouched, exit 0.
- **Records.** `pack-outcome.json` has exactly the 14-01 fixture key set and carries `pack_image_digest`, `gate_contract_sha256` and `parity_contract_sha256` (hashed from `/src/contracts`, the files `pack_laya` reads) and `pack_seconds` / `verify_seconds` (null when that child did not run), skips included. `pack-manifest.json` is the fixture's JSON array of `{path,size,sha256}` and lists the `.apr`, both logs and `pack-outcome.json`. Upload order: `.apr`, `pack.log`, `verify.log`, `pack-manifest.json`, `pack-outcome.json` last, every call tagged `laya-retention=expire`.
- **Image.** `rust:1.93.0-bookworm@sha256:d0a4aa3c...` builds `pack_laya` as an example (`cargo build --release --locked -p aprender-decide --example pack_laya`, 34.6 s) at `/src`; `debian:bookworm-slim@sha256:99c0b25c...` runtime carries python3, ca-certificates, curl, unzip, the aws CLI v2 aarch64 zip (2.39.1, sha256 `5adf0b50...` checked in the RUN), the binary at `/src/target/release/examples/pack_laya`, three contracts at `/src/contracts/`, an empty `/src/crates/aprender-decide`, `run_step.py` and friends under `/opt/laya`, the 14-01 fixtures at the path `run_step`'s ROOT resolves, and `/opt/laya/contracts -> /src/contracts`. 412 MB, arm64, label `org.opencontainers.image.revision`. `docker build --check`: no warnings.
- **Smoke.** Build context is the git index restricted to `context-paths.txt` (295,420 KiB, 17,869 files, nine paths; no `.planning/`, `docs/`, `models/`, `.claude/`). In the image: `ldd` clean; `pack-fixture` prints and writes `37d65159b2be0fa091aa840cd56c1a84b73c0bcd9e2df5906d1f8218f5448561`; `inspect` exit 0; `verify` exit 2 with `REFUSED SyntheticNotDeployable` (asserted not to be `REFUSED Usage`); the same `verify` with `--tmpfs /src/contracts` prints `REFUSED Usage /src/crates/aprender-decide/../../contracts/laya-finetune-gate-v1.yaml: No such file` (the negative control that the fixed path is what makes the refusal real); `--selftest` with no mount: 154 ok, 0 FAIL, `RUN STEP SELFTEST OK`. Final line `SMOKE OK image sha256:d5b0036a... pack_laya 7451008 bytes`.

## Task Commits

1. **Task 1 (tracer): run_step pack mode against a fake pack_laya and a fake S3 runner** - `f6a5eec5a`
2. **Task 2: the linux/arm64 pack image, its context list and its native smoke** - `a195c8e4a`

Tracer gate: Task 1's `<verify>` (selftest green, last line `RUN STEP SELFTEST OK`, 0 FAIL, all 17 named cases present, 154 ok against the 47 required) passed on the final tree before the commit, and Task 2 expansion followed. Separate RED commits were not made for the `tdd="true"` tracer: the selftest lives in the same file as the code, as in 14-01 and 14-06.

Mutation check (not in the plan): seven one-line mutations of the pack logic each turned the selftest red and were restored byte for byte (`cmp`): gate_fail always packing (4 FAIL), sha comparison removed (2), `.apr` uploaded unconditionally (8), child env not stripped (2), waived values not compared (2), manifest re-hash removed (3), `device_ok` ignored (4).

## Deviations from Plan

**1. [Rule 3 - Blocking] `docker build` is called with `DOCKER_DEFAULT_PLATFORM=linux/arm64`, not `--platform linux/arm64`**
- **Found during:** Task 2 (`bashrs lint smoke.sh`)
- **Issue:** bashrs 6.66.3 reports `IDEM002 Non-idempotent rm` on the substring `rm` inside `--platform` on a `docker build` line (reproduced on a one-line file), which makes the plan's `bashrs lint` gate exit 1. The `docker run --platform` lines are not flagged.
- **Fix:** the build is driven through the environment variable; `docker run` keeps `--platform`; the smoke asserts the built image's `Os/Architecture` is `linux/arm64`. The lint also rejected nested-quote command substitutions (SC1078, 6 errors), so the smoke runs containers through one `run_to <log> <args>` helper and reads results from log files.
- **Files:** `deploy-extensions/container/pack-arm64/smoke.sh`. **Commit:** `a195c8e4a`

**2. [Plan-detail resolution] The smoke extracts the index subset into a scratch directory and builds with `-f` outside it**
- **Issue:** the plan pipes `tar -cf -` to the build; a stdin tar context cannot take the Dockerfile from outside the context, and the Dockerfile (a `deploy-extensions/` file) is not in the nine-path list.
- **Fix:** `git ls-files -z --cached -- <paths> | tar --null -T - -cf - | tar -xf - -C <scratch>`, then `docker build -f <repo>/deploy-extensions/container/pack-arm64/Dockerfile <scratch>`. 14-09's recipe must do the same (stated in the Dockerfile header).

**3. [Rule 2 - Missing critical] Refusals the plan did not enumerate**
- A `.apr` upload that fails records `accepted false` (and exits 1) instead of an accepted record for a model that is not in the bucket (case added).
- A manifest path with `..`, absolute, repeated, or an entry set that is not exactly `{path,size,sha256}`, a foreign `task_id`, an incomplete `training-manifest.json` for a packable run, and an absent or foreign `outcome.json` are each exit 1 (cases added).
- **Commit:** `f6a5eec5a`

**4. [Design choice inside the plan's latitude]** `packed_line` / `verify_line` hold the child's `REFUSED ...` line when that child refused, and `exit_code` is the first non-zero child code (else 0), because 14-05's finalizer builds its failure text from `verify_line` then `packed_line` and from `exit_code`; with the plain PACKED/JSON-only reading a refusal would read "no detail recorded".

**5. [Environment] The first image build failed in `apt-get update` with "At least one invalid signature"** (`deb.debian.org`, 0.5 s in). A direct `apt-get update` in the same pinned image then succeeded and the identical build passed; treated as a transient mirror fault, no change made. The apt packages are unpinned by design (stated in the Dockerfile header).

**Total deviations:** 5 (1 blocking, 1 resolution, 1 missing-critical, 1 design choice, 1 environment). **Impact:** none changes a must-have.

## Issues Encountered

- The rust stage needed nothing beyond Cargo.toml, Cargo.lock, rust-toolchain.toml, `src/`, `crates/` and `contracts/`: no apt package, no `.cargo/config.toml` (gitignored, absent from the index subset), no extra path. The build took 34.6 s; the plan's 10 to 30 minute estimate does not hold for this crate (`aprender-decide` has light dependencies), so 14-09's CodeBuild budget can be small.
- Only two contracts are read by `pack_laya` at run time (the laya gate and the laya parity, both through `policy()`; the library's non-test code reads none, `grep CARGO_MANIFEST_DIR crates/aprender-decide/src`). `decide-apr-v1.yaml` is copied into `/src/contracts/` as the plan says but nothing in this plan proves it is needed; it is harmless and is not a finding.
- Through the rtk hook, `ls`/`docker` output is condensed; digests and hashes recorded here were taken from commands that print them in full (`shasum -a 256`, `imagetools inspect`, the smoke log).

## Not Proven Here (belongs to 14-08, 14-12 and the first real run)

- A real pack and verify of a real trained run: the smoke refuses a synthetic artifact on purpose; the accept path ran only against a fake `pack_laya`. Phase 10 measured pack at about 21 min and verify at about 22 min on Apple silicon for 2,129 eval rows; Graviton Fargate numbers arrive in `pack_seconds` / `verify_seconds` on every run.
- The aws CLI against real S3 (`get-object`, `put-object --tagging`): the selftest and smoke make no AWS call; the zip is trust-on-first-use from `awscli.amazonaws.com` over TLS, as in 14-06.
- Fargate on Graviton itself (task size, ephemeral storage, network): see below.

## Known Stubs

None.

## Threat Flags

None. T-14-24 (pack policy), T-14-25 (run files across S3), T-14-26 (unearned .apr), T-14-66 (build context) and T-14-SC (image inputs) are each covered by the cases and smoke steps named above. No AWS call, push or deploy was made; the only network use was Docker pulling the two pinned bases, cargo and apt during the image build, and one download of the aws CLI zip to hash it.

## Next Phase Readiness

- **14-08 (state machine / IAM):** the pack task definition needs `LAYA_STEP` unset or `pack`, `LAYA_TASK_ID`, `LAYA_NAME`, `LAYA_BUCKET`, `LAYA_RUN_PREFIX` (exactly `runs/<name>/<task_id>/`) and `LAYA_PACK_IMAGE_DIGEST` set to the literal digest of the task definition's own image; the task role needs `s3:GetObject` on `runs/*` and `base/*` and `s3:PutObject` plus `s3:PutObjectTagging` on `runs/*`. Ephemeral storage: the run's three seeds' checkpoints (about 0.8 GB each), the 0.85 GB base tar (deleted after extraction) and its 0.85 GB extraction, and the packed `.apr` come to roughly 6 GB, inside Fargate's default 20 GiB; keep `taskTimeout` 3 h. The working directory is `/work` (the image creates it). Exit 0 means an outcome was recorded (read `accepted`); exit 1 means an infrastructure error or a failed upload and leaves no `pack-outcome.json`, which 14-05's finalizer already reads as "the pack stage left no record".
- **14-09 / 14-12 (image recipe, CodeBuild):** build with `context-paths.txt` extracted from the git index into a scratch directory, `docker build -f deploy-extensions/container/pack-arm64/Dockerfile <scratch>` on an arm64 builder, `--build-arg GIT_COMMIT=<sha>`; scope the clean-tree check to those nine paths. The first CodeBuild run proves the arm64 builder; the local smoke already proved the Dockerfile.
- **14-15:** `smoke.sh` is a re-runnable local gate (`bash deploy-extensions/container/pack-arm64/smoke.sh`, needs Docker Desktop; it starts it and waits up to 120 s). No `just` recipe was added (not in this plan's files).

## Self-Check: PASSED

Verified: `run_step.py` and the three `pack-arm64/` files exist; commits `f6a5eec5a` and `a195c8e4a` are ancestors of HEAD; Task 1's `<verify>` passes on the final tree (154 ok, 0 FAIL, last line `RUN STEP SELFTEST OK`, all 17 named cases present); Task 2's `<verify>` (`bashrs lint` exit 0, both FROM lines digest-pinned, `--example pack_laya` present, `Cargo.lock` listed and no `.planning`/`docs`/`models`, fixtures copied, `SMOKE OK image ...`, golden sha256 in the log) ended `P14-07 T2 VERIFY PASS`; `docker build --check` clean; neither banned determinism name appears in `run_step.py`; the image's `--selftest` with no mount ran 154 ok, 0 FAIL.
