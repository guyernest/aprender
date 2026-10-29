---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 06
subsystem: training-container
tags: [sagemaker, docker, cuda, sigterm, s3, laya, run-step, base-staging, offline-load]

requires:
  - phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
    provides: "14-01 golden fixtures (outcome-v1.json, training-manifest-v1.json) and layout vocabulary; 14-03 owner-approved Linux x86_64 uv.lock"
provides:
  - "scripts/laya_train/container/run_step.py: the one program SageMaker runs (train, probe, probe-sigterm), stdlib only, 85 selftest cases"
  - "scripts/laya_train/container/stage_base.py: deterministic laya-base.tar build, safe_extract, offline-load check"
  - "scripts/laya_train/container/base-stage.json: the pinned staged-base identity (laya-base-stage-v1)"
  - "scripts/laya_train/container/Dockerfile.train: the x86_64 CUDA training image definition (lints clean)"
  - "scripts/laya_train/container/context-paths.txt: the Docker build context subset"
affects: [14-07, 14-08, 14-09, 14-12, 14-15]

requirements-completed: [D-05, D-12, D-13, D-14, D-17, D-02]

plan_head_before: 646826c9e0a8c988413354ae17648979fda52158
plan_head_after: ffe1fcc4fc5c75d9c68e5446b1b1e798dc74ac44

actuals:
  tokens: 29300      # chars/4 over scripts/laya_train/container/* (117,178 chars)
  tasks: 3
  commits: 3

tech-stack:
  added: []
  patterns:
    - "one Runner callable (argv, timeout) is the only S3 seam; selftests inject a fake that copies into a temp bucket"
    - "the SIGTERM handler does the whole bounded shutdown itself and os._exit()s; the selftest drives the real handler in child processes"
    - "a trainer child is waited on by ONE reaper thread; handlers and the main loop read an Event, never Popen.poll/wait"
    - "a manifest lists a file only after its own upload returned success (Uploader.uploaded)"

key-files:
  created:
    - scripts/laya_train/container/run_step.py
    - scripts/laya_train/container/stage_base.py
    - scripts/laya_train/container/base-stage.json
    - scripts/laya_train/container/Dockerfile.train
    - scripts/laya_train/container/context-paths.txt
  modified: []

key-decisions:
  - "The staged tar resolves the weights through the hub-level blob store: this box's xet-backed HF cache keeps the 803.6 MB model blob as a symlink into <hub>/blobs/, outside the model dir, so stage_base accepts that one extra root and refuses every other escape"
  - "SIGTERM: the handler runs the shutdown (SIGTERM group, SIGKILL at t0+grace, small uploads, partial-manifest, outcome timeout) and exits; the reserve is shared so a slow runner cannot starve outcome.json (the manifest may use the first half of the reserve)"
  - "outcome.json is written locally first and listed in training-manifest.json (the golden fixture lists it), then uploaded LAST; any upload failure makes the job exit 1 even for pass and gate_fail"
  - "No `# syntax=` directive in Dockerfile.train: it would pull the floating docker/dockerfile:1 frontend, an input the file could not pin"
  - "CUDA base is 13.0.3-base-ubuntu24.04 (digest-pinned index); the compat libraries (libcuda.so.580.126.20) ship with it and the build RUN fails when absent"

duration: 22 min
completed: 2026-10-29
status: complete

coverage:
  - id: D1
    description: "run_step train mode: the pinned trainer runs unchanged; exit 0/3/2/other map to pass/gate_fail/refused/crash; outcome.json last; files, training-manifest (complete true, image digest, contract hash) tagged laya-retention=expire"
    verification:
      - kind: unit
        ref: "python3 -I scripts/laya_train/container/run_step.py --selftest (train-pass, gate_fail, refused, crash, argv, keys, image-digest)"
        status: pass
    human_judgment: false
  - id: D2
    description: "No silent CPU fallback: the trainer is killed at the first non-cuda DEVICE line and device_ok is false without a cuda gate-report"
    verification:
      - kind: unit
        ref: "run_step selftest cpu-fallback (killed in 0.02 s, crash, device_ok false)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Validation bound to the bytes trained on: LAYA_EXPECTED_INPUTS re-checked before the trainer starts (changed byte, added or removed shift.jsonl)"
    verification:
      - kind: unit
        ref: "run_step selftest inputs-changed, env refusal"
        status: pass
    human_judgment: false
  - id: D4
    description: "SIGTERM leaves reports: one 25 s absolute deadline, partial-manifest then outcome timeout last, no checkpoints, slow and failing runners bounded, 300 s sync writes a hash-bearing partial manifest listing only uploaded files"
    verification:
      - kind: unit
        ref: "run_step selftest sigterm, sigterm-slow, sigterm-failing, periodic-sync (real handler in child processes)"
        status: pass
    human_judgment: false
  - id: D5
    description: "Compat switch (driver major below 580), GPU sampler, probe and probe-sigterm modes, no determinism flag"
    verification:
      - kind: unit
        ref: "run_step selftest compat, gpu sampler, probe, probe-sigterm, no-determinism"
        status: pass
      - kind: other
        ref: "PROBE_HELPER_PY run on CPU against the tiny fixture checkpoint: matmul, offline load, forward, one backward and AdamW step all ran (CUDA itself untested here)"
        status: pass
    human_judgment: true
    rationale: "The CUDA numbers (peak bytes, step seconds, real SIGTERM grace) can only come from the paid probe job in 14-12"
  - id: D6
    description: "Deterministic base tar (two builds byte-identical, sha256 15600ec3a856), safe extraction, offline load through Laya's own Agent with the contract digest"
    verification:
      - kind: unit
        ref: "stage_base.py --selftest (17 cases); build x2 cmp; uv run ... stage_base.py check -> OFFLINE LOAD OK model.safetensors sha256=891102d3..."
        status: pass
    human_judgment: false
  - id: D7
    description: "Dockerfile.train pins base, uv and aws CLI, syncs --frozen --no-dev, imports the frozen stack at build time, copies only context-paths; lints clean on the subset assembled from the git index"
    verification:
      - kind: other
        ref: "docker build --check against the context-paths subset: Check complete, no warnings found; lifecycle.py from the subset prints LIFECYCLE OK"
        status: pass
    human_judgment: true
    rationale: "Whether the image BUILDS and the lock imports on Linux x86_64 is proven only by CodeBuild (14-12)"
---

# Phase 14 Plan 06: GPU training container Summary

**A stdlib-only `run_step.py` that runs the pinned trainer unchanged on CUDA, kills a silent CPU fallback, survives SIGTERM inside one 25 s deadline with a hash-bearing partial inventory, plus a reproducible 846 MB base tar that loads offline through Laya's own Agent and a digest-pinned `Dockerfile.train` that lints clean.**

## Performance

- **Duration:** 22 min (06:28:41Z to about 06:50Z)
- **Tasks:** 3 (one tracer, two auto/tdd)
- **Files:** 5 created under `scripts/laya_train/container/`

## Accomplishments

- `run_step.py train` runs `train.py --data <work>/data --out <work>/run --device cuda [--epochs N]` and nothing else, maps the exit (0 pass, 3 gate_fail with job exit 0, 2 refused, other or signal crash with job exit 1), and uploads every file, `training-manifest.json` (complete true, `train_image_digest`, `gate_contract_sha256`), then `outcome.json` last, each as a `put-object` tagged `laya-retention=expire`. Written key sets equal the 14-01 golden fixtures.
- A DEVICE line with a non-cuda `used=` kills the trainer's process group (0.02 s in the selftest); after any exit `device_ok` needs `gate-report.json device_used` to start with `cuda`.
- `LAYA_EXPECTED_INPUTS` is parsed by a mirror of `layout::parse_inputs_string` and checked against the data channel before the trainer starts: a changed byte, an added `shift.jsonl` or a removed one is `REFUSED inputs-changed: <file>`, exit 1, trainer never started.
- SIGTERM: one absolute deadline (`SHUTDOWN_DEADLINE_S = 25`, grace 5, reserve 5). Measured with the real handler in child processes: exits 1.07 s after the signal when the trainer ignores SIGTERM (test deadline 6 s), 8.05 s with a 2 s-per-upload runner (deadline 9), 1.07 s with a runner that fails every small upload; `outcome.json {timeout}` is always the last call and the partial manifest lists exactly the files whose upload returned.
- The 300 s sync (1 s in the test) uploads changed small files then a `partial-manifest.json` (complete false); files over 5 MiB and anything under a checkpoint directory never go mid-run.
- Compat: driver major below 580 puts `/usr/local/cuda/compat` first on the child's `LD_LIBRARY_PATH`, recorded in `run-env.json`; an unreadable driver is `unknown` and no compat.
- Probe: `probe.json` (the 20-key `laya-train-probe-v1` set) carries nvidia-smi, driver, compat, the CUDA helper's fields (including the bounded real-model training step: `train_step_ok/seconds/peak_bytes`, `host_rss_peak_bytes`, batch 8 x 1024 tokens) and the synthetic-fixture run's device. `probe-sigterm` uploads a heartbeat every 2 s after SIGTERM (0.0, 2.009, 4.007 s measured).
- `stage_base.py build` gives two byte-identical tars from the local cache (`15600ec3a856...`, 846,223,360 bytes, 18 entries, `typed-decisions/` excluded); `check` extracts into a fresh HF_HOME with `HF_HUB_OFFLINE=1` and prints `OFFLINE LOAD OK model.safetensors sha256=891102d3...` (contract digest). `run_step` verifies the channel tar against `base-stage.json` before `safe_extract` (flipped byte refused before any extraction; a `..` member with the right sha refused by `safe_extract`).
- `Dockerfile.train` pins the CUDA base (`nvidia/cuda:13.0.3-base-ubuntu24.04@sha256:7c7413a5...`), uv 0.9.5 (`@sha256:f459f6f7...`) and the aws CLI v2.39.1 zip (sha256 `5b6c10e0...`), runs `uv sync --frozen --no-dev`, has the build-time `IMPORT OK` check and states that apt is unpinned. `docker build --check` against the index-assembled subset: "Check complete, no warnings found".

## Task Commits

1. **Task 1 (tracer): run_step.py train mode against a fake trainer and fake S3 runner** - `4e9593c19`
2. **Task 2: SIGTERM, periodic sync, GPU sampler, compat switch, probe modes** - `e1a874962`
3. **Task 3: base staging with offline-load proof, Dockerfile.train, base channel wiring** - `ffe1fcc4f`

Tracer gate: the Task 1 `<verify>` was re-run after the commit (43 cases then, 18 required), passed, and expansion continued.
Separate RED commits were not made for the two `tdd="true"` tasks: the selftest lives in the same file as the code, as in 14-01.

## Deviations from Plan

**1. [Rule 1 - Bug in the plan's premise] The real HF cache cannot be staged under "refuse a symlink resolving outside the cache"**
- **Found during:** Task 3 (inspecting the cache before writing `stage_base.py`)
- **Issue:** this box's cache is xet-backed: `snapshots/<rev>/model.safetensors -> ../../blobs/891102d3...`, and that blob is itself a symlink to `<hub>/blobs/dd/dd8562...`, outside `models--convaiinnovations--laya/`. A strict "inside the model dir" rule would refuse the 803.6 MB weights.
- **Fix:** `stage_base.collect` accepts two roots, the model dir and the hub-level `<hub>/blobs/` store, and refuses everything else (selftest: a link to a file outside both is refused). In the tar every blob is a regular file and every snapshot entry a relative symlink.
- **Files:** `scripts/laya_train/container/stage_base.py`. **Commit:** `ffe1fcc4f`

**2. [Rule 1 - Bug] SIGTERM handler could not see a dead trainer**
- **Found during:** Task 2 (`sigterm-slow` failed: the first upload started 1 s late)
- **Issue:** the handler interrupts the main thread inside `Popen.wait()`, which holds Popen's waitpid lock, so `poll()` and a nested `wait()` inside the handler return "alive" and burn their full timeout.
- **Fix:** one reaper thread is the only waiter; `Trainer.exited` (an Event) is what the main loop and the handler read. After the fix the 6 s-deadline case exits in 1.07 s.
- **Files:** `run_step.py`. **Commit:** `e1a874962`

**3. [Rule 2 - Missing critical] A slow runner could starve `outcome.json`**
- **Issue:** with one shared reserve the partial-manifest call can consume all of it and leave `outcome.json` (the completion marker) no time.
- **Fix:** the manifest may use the first half of the reserve (`end - reserve/2`); `outcome.json` always keeps the second half. The slow case (2 s per upload, deadline 9) uploads both and exits in 8.05 s.
- **Commit:** `e1a874962`

**4. [Rule 2 - Missing critical] Upload failures must fail the job**
- A `pass` or `gate_fail` run whose files did not all upload would exit 0 and let the pack stage run on a missing checkpoint. Any failed upload now exits 1 (outcome.json is still attempted). **Commit:** `4e9593c19`

**5. [Plan-detail resolution] outcome.json is in the training manifest**
- The golden `training-manifest-v1.json` lists `outcome.json`, which the plan also uploads last. It is written locally first, hashed, listed, and uploaded after the manifest. A failed outcome upload would leave a listed-but-absent file; the finalizer treats a missing `outcome.json` as the failure it is. **Commit:** `4e9593c19`

**6. [Rule 3 - Blocking] `# syntax=docker/dockerfile:1` removed from Dockerfile.train**
- It makes BuildKit pull a floating frontend image, an input the file cannot pin. The built-in frontend lints the file ("no warnings found"). **Commit:** `ffe1fcc4f`

**7. [Environment] The plan's verify scripts need bash, and Docker had to be started**
- The Bash tool runs zsh, where the unquoted `$(cat context-paths.txt)` in Task 3's verify does not word-split. The three verify scripts were run as written under `bash`. Docker Desktop was not running; `open -a Docker` brought the daemon up in 6 s (29.9.1).

**8. CUDA base is 13.0.3, not 13.0.2.** The plan says "13.0.x"; 13.0.3 is the newest published (13.0.4 is not found). The linux/amd64 child was run once to confirm `/usr/local/cuda/compat/libcuda.so.580.126.20` ships with it.

**Total deviations:** 8 (3 bug/premise, 2 missing-critical, 1 blocking, 1 resolution, 1 environment). **Impact:** none changes a must-have; 1 to 3 change implementation details later plans rely on (listed below).

## Issues Encountered

- `ls`/`curl` output through the rtk hook is condensed; hashes and digests recorded here were taken from commands that print them in full (`shasum -a 256`, `imagetools inspect`, the build logs).
- The aws CLI zip sha256 is trust-on-first-use from `awscli.amazonaws.com` over TLS (no vendor-published digest file); it is pinned and checked in the build RUN from now on.

## Not Proven Here (belongs to 14-12 and CodeBuild)

- The image has not been built: Apple silicon would only emulate x86, and the lock's 2.4 GB of Linux wheels were not installed. Linux install and import of the frozen stack (the `IMPORT OK` RUN) is first proven by CodeBuild.
- Nothing touched CUDA. `PROBE_HELPER_PY` was run on CPU against the tiny fixture checkpoint (load, forward, backward, AdamW step all executed, `train_step_ok` true); the GPU numbers, the compat decision on a real ml.g5 host and SageMaker's real SIGTERM grace (25 s is `[ASSUMED]`) are the paid probe's.
- Image size and pull time are recorded when 14-12 builds it; the nvidia/cuda base plus torch's cu13 wheels carry two CUDA runtimes (the slimmer base is a recorded follow-up).

## Known Stubs

None.

## Threat Flags

None. The plan's register covers the new surfaces: the base channel tar (T-14-21, pin check plus `safe_extract`), silent CPU fallback (T-14-20), silent downloads (T-14-22, offline env plus the offline-load proof), lost reports on stop (T-14-23), changed dataset bytes (T-14-65) and the image inputs (T-14-SC). No network call, AWS call or push was made.

## Next Phase Readiness

- **14-08 (state machine / IAM):** the SageMaker role needs `s3:PutObjectTagging` on `runs/*` (every upload is tagged); the job must set `LAYA_STEP` (unset = train), `LAYA_TASK_ID`, `LAYA_NAME`, `LAYA_BUCKET`, `LAYA_RUN_PREFIX` (exactly `runs/<name>/<task_id>/`), `LAYA_EPOCHS`, `LAYA_TRAIN_IMAGE_DIGEST` (literal equal to the TrainingImage digest) and `LAYA_EXPECTED_INPUTS` (14-01's canonical string); channels `data` and `base` (`base/laya-55cf4c4e/laya-base.tar`).
- **14-09 / 14-12:** build with the context-paths subset from the git index, `docker build --check` first; upload the base tar built by `stage_base.py build` (sha256 must equal `base-stage.json`: `15600ec3a8565ee7046909fdc644f56779823fc27e7d54938d2a9184eb244dfc`, 846,223,360 bytes). The tar is NOT committed. The probe requires `train_step_ok` true in `probe.json` and the heartbeat files from `probe-sigterm` give the real grace.
- **14-05 (finalizer):** `partial-manifest.json` (complete false) is the fallback inventory when `training-manifest.json` is absent; `outcome.json {exit_class timeout}` is written by the SIGTERM path with `train_rc` null; a refused run's `device_used` is `none` and `device_ok` false.
- **14-07 (pack):** `run_step.py` has no `pack` mode yet; `LAYA_STEP` other than train/probe/probe-sigterm is `REFUSED env:`.
- No `just` recipe was added (not in this plan's files); the selftests run as `python3 -I scripts/laya_train/container/{run_step,stage_base}.py --selftest`.

## Self-Check: PASSED

Verified: all five created files exist; commits `4e9593c19`, `e1a874962`, `ffe1fcc4f` are ancestors of HEAD; the three `<verify>` scripts pass when re-run on the final tree (run_step selftest 85 ok, 0 FAIL, last line `RUN STEP SELFTEST OK`; stage_base selftest 17 ok; two base builds `cmp` identical and equal to the pin; `OFFLINE LOAD OK` with the contract digest; subset `lifecycle.py` prints `LIFECYCLE OK`; `docker build --check` clean; run_step imports no boto3, botocore, requests, torch, numpy or yaml); neither banned determinism name appears in `run_step.py`, `stage_base.py` or `Dockerfile.train`.
