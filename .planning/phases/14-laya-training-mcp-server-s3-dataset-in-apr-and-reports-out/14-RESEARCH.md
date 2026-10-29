# Phase 14: Laya training MCP server: S3 dataset in, .apr and reports out - Research

**Researched:** 2026-10-28
**Domain:** thin MCP training server (pmcp, MCP Tasks) orchestrating a SageMaker GPU training job (Python/torch, x86_64 CUDA) plus an arm64 Rust pack-and-verify step, on AWS (ze-kasher-dev)
**Confidence:** MEDIUM. The scaffold, trainer, packer and register facts are HIGH (read this session). The SageMaker/CUDA/host-driver facts are MEDIUM or LOW and are gated behind a probe job the plan must run first. No AWS call was made.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Carried in from the owner (2026-10-28, ROADMAP.md)**
- **D-01:** Tenant and account: the dev environment, **ze-kasher-dev**, first. augury-dev is out of this phase.
- **D-02:** Compute: **SageMaker Training Jobs first**. Switch to **ECS** only if SageMaker shows a limitation beyond a reasonable range on Rust tooling, x86_64, latency or cost. ECS's one real advantage is that Graviton GPU hosts (g5g) keep aarch64. — **Reversibility:** costly — the dispatcher, finalizer, IAM and container contract are SageMaker-shaped. A move to ECS rewrites them, though not the request Lambda or the task store.
- **D-03:** Dataset access: **a single server-owned bucket read by the server's role**. Upload-through-MCP (the SetFit server's presigned-PUT slot) is deferred, not rejected.
- **D-04:** Output: **full**. The `.apr` plus every report the run writes, readable and usable by the caller.
- **D-05:** Training stays **Python/torch on Laya's own code** (Phase 8 D-01: "A Rust trainer is not planned"). Training rows are built by `Agent._encode_state`, so they equal inference rows byte for byte. This phase does not reopen that.

**Caller surface**
- **D-06:** The `train` tool takes **the dataset URI and a model name only**. Seeds (13/17/23, median-ECE selection), the stopping rule, epochs and the gate bars all come from `contracts/laya-finetune-gate-v1.yaml` at run time. Every run is a declared gate run, comparable with Phases 10 and 12. The shift probe runs when the prefix holds `shift.jsonl`, exactly as the CLI does today. Unknown arguments are refused, not ignored.
- **D-07:** Dataset shape: **a prepared data dir only**. That is an S3 prefix holding `task.json`, `train.jsonl` and `eval.jsonl`, with `shift.jsonl` optional: the trainer's generic contract (`scripts/laya_train/data.py`). Preparing data from partner CSVs stays local with the `prepare_*` scripts.
- **D-08:** **A synchronous `validate_dataset` tool**, and the same check run first inside `train`. It applies data.py's refusals (task schema and unknown keys, label membership, UTF-8 encoding, eval or shift overlap with train, conflicting train labels) in the request path and answers in seconds, before any GPU job is paid for. The job still re-runs the trainer's own validation. The request-side check is a pre-filter, never the authority, and a disagreement between the two is a defect to surface. The planner decides whether this is a Rust port with a case table shared with data.py, or Python in the request path. It must be one rule set proven equal by a shared table, not two drifting copies.
- **D-09:** Naming: **a caller-given model name plus the task id**. Output lands under `runs/<name>/<task_id>/`, so reruns of one model sit together and never overwrite. The name is validated, for example as a lowercase slug, and refused otherwise.

**Access and spend limits**
- **D-10:** Auth: **pmcp.run OAuth is required**, as on the augury motivation server since Phase 13. Every call can start a paid GPU job.
- **D-11:** Concurrency: **one job at a time per deployment** (single-flight, as the SetFit server does). A second `train` while one runs is refused with a typed error naming the running task.
- **D-12:** Ceiling: **6 hours per job** (SageMaker `MaxRuntimeInSeconds` = 21600). It covers Phase 10's 4.7 h MPS run with margin. A job that hits it ends failed with whatever reports exist.
- **D-13:** Instance: **`ml.g5.xlarge` (A10G, 24 GB)** for the first jobs. The planner confirms price and service quota in ze-kasher-dev with the owner before any job runs.

**x86_64 and CUDA acceptance**
- **D-14:** **Pack and verify run on arm64 after training**, never in the x86 GPU container. The GPU job writes its run dir to S3. `pack_laya pack` then `verify` run on Graviton CPU, as a Fargate arm64 task or an equivalent the planner picks. That is the architecture where every parity and pack-tolerance bar was measured and where the `.apr` is served. x86_64 touches only torch. `pack_laya` is a cargo example today, so the planner decides whether it ships as a prebuilt example binary or is promoted to a bin; either way it needs a FALSIFY-MONO-011 decision.
- **D-15:** **A failed pack verify is a failed run, and the owner decides what comes next.** Examples are a noise bound over the ceiling or an argmax disagreement. The task ends failed with every report attached and no `.apr`. Any amendment of `contracts/laya-parity-v1.yaml` for CUDA-trained checkpoints needs the owner's explicit decision, as amendment A1 did in Phase 8. It is never made by an executor. — **Reversibility:** one-way — a contract amendment changes what every later pack accepts.
- **D-16:** **Trust bar: reproduce motivation-v1.** Before the server's first real gate run is trusted, retrain motivation-v1 on SageMaker from the same data dir as Phase 12. It must pass the gate and land within a band the planner proposes and the owner approves, around Phase 12's numbers: margin 0.3780, post-calibration ECE 0.0936, wall 752 s on MPS. This is one paid run. Its evidence goes to a phase evidence JSON like the earlier gate runs.
- **D-17:** Reproducibility is claimed **at the gate-outcome level**. The same data and recipe give the same gate verdict, with metrics in a stated band; bytes may differ. This matches Phases 10 and 12 (spike 027: MPS training is not bitwise reproducible). No `torch.use_deterministic_algorithms`.

**Result hand-off**
- **D-18:** `train_status` returns **the verdict inline plus a manifest**. Inline: pass or fail, margin, ECE, the per-seed rows, the shipped seed and the `.apr` sha256 when there is one. The manifest lists every output file with its sha256 and a presigned GET link. Links are minted when the status is read and never stored, as in the SetFit server, so the verdict in the task store stays true for the task's TTL. `tasks/result` carries the verdict and manifest without links.
- **D-19:** Retention: **GATE PASS runs are kept indefinitely. GATE FAIL and failed runs expire after 30 days**, by an S3 lifecycle rule. The planner chooses how a run is marked so the rule can tell them apart (tag or prefix).
- **D-20:** **A GATE PASS stops at S3, plus a fetch recipe.** A `just` recipe pulls a run into `models/decide/<name>/` and verifies every sha256 against the manifest. The existing owner-run `laya-deploy` path can then use it unchanged. No decide server changes.

### Claude's Discretion
- The server id and crate names. One server per algorithm: proposed `aprender-mcp-laya-train` plus its Lambda crate. Its `bootstrap` bin adds one FALSIFY-MONO-011 allowlist entry (baseline 27), with the Lambda-ABI rationale the register already carries.
- How the base snapshot (`convaiinnovations/laya` @ 55cf4c4e, 0.84 GB, sha-pinned) reaches the job: an S3 input channel, never baked into the image (spike finding: baking weights in costs Lambda 800 s, Fargate 4×). The sha256 is checked in the job.
- Where the container image is built and stored (ECR in ze-kasher-dev), and how its digest is pinned and recorded in the run.
- Cancel semantics: `tasks/cancel` maps to `StopTrainingJob`, plus the guarded terminal write from the SetFit task-store model.
- The finalizer mechanism: an EventBridge SageMaker job-state rule to a finalizer Lambda, or an equivalent. It moves the task to its terminal state and triggers the arm64 pack-and-verify step on success.
- The task TTL and the exact S3 key layout under `runs/<name>/<task_id>/`.

### Deferred Ideas (OUT OF SCOPE)
- Upload-through-MCP (a presigned-PUT dataset slot, as the SetFit server has): deferred by the owner on 2026-10-28, not rejected.
- Server-side preparation of raw partner CSVs with named prepare rules.
- Caller-set epochs or stopping rule.
- Auto-deploy of a GATE PASS artifact to a decide server.
- ECS on Graviton GPU (g5g) as the compute tier: the D-02 fallback, triggered only by evidence.
- CUDA-specific parity bars in `laya-parity-v1`: only by owner decision after a failed verify (D-15).
</user_constraints>

<phase_requirements>
## Phase Requirements

No requirement IDs are mapped (ROADMAP: "none mapped; the plans implement CONTEXT decisions"). The planner maps plans to D-01..D-20.

| Decision | Research Support |
|----|------------------|
| D-06 / D-08 | Section "The epochs conflict" (Open Question 1) and "Request-side validation" |
| D-02 / D-12 / D-13 | "SageMaker Training Job shape", "CUDA 13 versus the host driver" |
| D-10 / D-11 | "Single-flight is NOT in the Lambda scaffold", "Auth-first ordering" |
| D-14 / D-15 | "The arm64 pack-and-verify step", "pack_laya bakes a build path" |
| D-16 | "Reproduction band risk" |
| D-18 / D-19 / D-20 | "Result manifest", "Retention", "Fetch recipe needs a full run dir" |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

- **Branch protection**: `main` is protected; work on a branch, PR, CI `ci / gate` + `workspace-test` must pass. Modifying `.github/workflows/*.yml` is a check-in item.
- **Justfile over scripts**: user global rule - project scripts go in `justfile`. The repo already follows it for every Laya and deploy recipe.
- **Code search**: use `pmat query`, not grep, for code search (CLAUDE.md "Code Search Policy"). Executors should follow this.
- **Pin the binary**: never invoke a bare `apr`; source `scripts/apr_bin.sh`. Never read `$?` through a pipe. The Bash tool runs zsh (no word splitting, no PIPESTATUS); guard case tables go silently vacuous under zsh.
- **No `unwrap()`**: banned by `.clippy.toml`; use `expect()` or `ok_or_else(|| ...)?`. Existing crates use `#![allow(clippy::disallowed_methods)]` only around `serde_json::json!`/`JsonSchema` expansions.
- **Shell scripts**: `set -euo pipefail`, quoted variables, `bashrs lint`. A sourced library must be option-neutral.
- **Contracts**: dogfood `pv` for contract YAML; never a bash re-implementation. New contract (if any) goes through `pv validate`.
- **Verification discipline**: prove the mechanism engaged (a trace line, a version+SHA, a behaviour delta), vary a failing input before naming a cause, a guard extended to a new scope must be re-mutated in the new scope, a guard that does not scan the decision surface is theater.
- **FALSIFY-MONO-011**: a new `[[bin]]` crate is a policy decision, never a same-PR edit (see "Register correction" below).
- **Autonomy limits**: AWS calls, paid jobs, quota changes, CI workflow edits, and contract amendments need the owner (project memory and CONTEXT specifics). Autonomous mode applies to PRs, builds, tests, evidence capture.
- **Spike findings** (`.claude/skills/spike-findings-aprender/`): Laya on default Lambda is S3-bandwidth bound (~80-95 MB/s per environment), Fargate pulls S3 at 675-790 MB/s, never bake weights into images, build images inside AWS (laptop uplink ~3.7 MB/s), thin one-model-per-server, weights in S3 pulled at start. [VERIFIED: .claude/skills/spike-findings-aprender/references/aws-mcp-model-hosting.md:35-50, 78-80]

## Summary

The scaffold reuse is real but narrower than the ROADMAP implies. `AprenderTaskStore`, `TaskBackend`, `InMemoryTaskBackend`, `StoredTask`, `mint_id` are `pub` and generic, and `DynamoDbTaskBackend::new(client, table)` is `pub` and reusable. The tool surface (`build_server`, `TrainArgs`, `Dispatcher`, `run_payload`, `submit`, `status`) is SetFit-shaped (config blob, `dataset_upload`, one `.apr` artifact) and cannot be reused; the Laya crate writes its own ~300-line equivalent against the same store. **Single-flight does not exist in the deployed SetFit scaffold**: `RunningJobs` is per-process and only the local dispatcher uses it, so D-11 needs a new durable lock. Cancel in the SetFit Lambda is a record flip plus a 15 s poll inside the worker; for SageMaker it must call `StopTrainingJob` (or stop the state-machine execution).

Four findings change the plan's shape. (1) **D-06 conflicts with the trainer**: above 16 shots/class `--epochs` is REQUIRED in [4,12] and below it is REFUSED; Phases 10 and 12 used 4 and 12. A "dataset URI and name only" `train` cannot satisfy the trainer without a server-owned or caller-supplied epoch rule (owner decision). (2) **The x86 CUDA container has two unproven links**: the committed `uv.lock` is macOS-arm64 only, and PyPI `torch==2.14.0` on Linux x86_64 depends on CUDA 13 wheels, which need driver 580-class support, while the only SageMaker driver table found lists `ml.g5` at driver 470. A cheap probe job (nvidia-smi, `torch.cuda.is_available()`, one matmul, one real seed on the tiny fixture) must precede everything. (3) **`pack_laya` resolves its contracts from a compile-time absolute path** (`env!("CARGO_MANIFEST_DIR")/../../contracts`), so a prebuilt binary only works where that path exists; keep it an example and build and run it at one fixed path inside the arm64 image. (4) **The FALSIFY-MONO-011 reference in CONTEXT is to the wrong register**: baseline 27 is `allowed_bins` (shrink-only migration debt); a Lambda deployment unit goes in `deployment_unit_bins`, currently exactly 10 of 10, and growing it needs a recorded CONTEXT decision with a blocking-human checkpoint.

The stage count (train, pack/verify, finalize) is exactly the point at which the scaffold's own comment says a state machine earns its keep. Recommended orchestration: one Step Functions Standard state machine using `sagemaker:createTrainingJob.sync` then `ecs:runTask.sync` (arm64 Fargate) then a finalizer Lambda, with a Catch on every state routed to the same finalizer. It removes the EventBridge duplicate-event handling and gives one place that defines the run. This departs from CONTEXT's "EventBridge ... or an equivalent" only within the discretion CONTEXT grants.

**Primary recommendation:** Build `aprender-mcp-laya-train` (lib only, no bin) and `aprender-mcp-laya-train-lambda` (bin `bootstrap` + a finalizer bin) reusing `AprenderTaskStore` and `DynamoDbTaskBackend::new`; add a DynamoDB single-flight lock; orchestrate with Step Functions (SageMaker `.sync` then Fargate arm64 pack/verify then finalizer); gate everything on a CUDA/driver/lock probe job and put the epochs rule and the register change to the owner before any code that depends on them.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| MCP tool surface, OAuth, owner scoping | API / Backend (request Lambda via pmcp.run) | - | Transport only; pmcp.run fronts it with API Gateway (30 s cap). Proven for SetFit. |
| Request-side dataset validation (D-08) | API / Backend (request Lambda, Rust) | Database / Storage (S3 GET of 3-4 small files) | Pre-filter in milliseconds-seconds; the job's trainer stays the authority. |
| Task record, envelope, single-flight lock | Database / Storage (DynamoDB) | API / Backend | Durable and shared across Lambda containers; per-process state is wrong scope (lib.rs:42-48 of the SetFit crate says so for its local case). |
| Orchestration of train -> pack/verify -> finalize | Orchestrator (Step Functions Standard) | - | Three stages with different compute tiers; `.sync` integrations hold hours-long waits without a polling Lambda. |
| GPU training (torch, 3 seeds, gate, noise record, shift probe) | SageMaker Training Job (x86_64 CUDA) | - | D-02, D-05. Touches only torch (D-14). |
| Pack + Rust verify (`pack_laya`) | ECS Fargate arm64 task | - | Architecture where every parity and pack bar was measured (D-14); 21+22 min on Apple silicon for 2,129 rows rules out Lambda's 900 s cap. |
| Terminal write, manifest verdict | Finalizer Lambda (arm64) | Database / Storage | Reads two small S3 objects, performs the guarded terminal write; hashes nothing large. |
| Artifact + report storage, lifecycle | Database / Storage (one S3 bucket) | CDN: none | D-03, D-19. Presigned GET minted at read time. |
| Image build (x86 CUDA and arm64) | CI-in-AWS (CodeBuild) | ECR | Laptop uplink ~3.7 MB/s; precedent in spike 021. |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| pmcp | 2.19 (crate pin) | MCP server, MCP Tasks, streamable HTTP | Already the org SDK; pinned `pmcp = { version = "2.19", features = ["streamable-http", "schema-generation"] }` [VERIFIED: crates/aprender-mcp-setfit-train/Cargo.toml dependencies block] |
| `aprender-mcp-setfit-train` (path dep) | workspace | `AprenderTaskStore`, `TaskBackend`, `InMemoryTaskBackend`, `StoredTask`, `mint_id`, `CancelSink` | Reuse, do not copy (OPS-03). All are `pub` [VERIFIED: crates/aprender-mcp-setfit-train/src/lib.rs:68-71] |
| `aprender-setfit-train-lambda` (path dep) | workspace | `DynamoDbTaskBackend::new(client, table)` | `pub fn new` is table-name agnostic; only `from_env` is hard-wired to `APRENDER_SETFIT_TASKS_TABLE` [VERIFIED: crates/aprender-setfit-train-lambda/src/dynamodb.rs:234-250] |
| aws-sdk-dynamodb / -lambda / -s3 | "1" (lock: 1.116.0 / 1.131.0 / 1.137.0) | task store, S3 reads and presign | Already in Cargo.lock [VERIFIED: Cargo.lock grep this session] |
| aws-sdk-sfn | latest 1.124.0 | `StartExecution`, `StopExecution`, `DescribeExecution` | Official AWS SDK; legitimacy check returned OK (see audit). Not yet in the lock. |
| aprender-decide (`task`, `verify::normalize_text`, `normalized_sha256`) | workspace | Rust half of the shared validation table | `Task::from_slice`, `normalize_text`, `normalized_sha256` are `pub` [VERIFIED: crates/aprender-decide/src/task.rs:172, verify.rs:1787-1796] |
| uv + `scripts/laya_train` | torch 2.14.0, transformers 5.17.0, numpy 2.5.3, safetensors 0.8.0, huggingface-hub 1.32.0, laya @ 4066d5d5 | the trainer | Pinned, hash-locked project; run with `--frozen` [VERIFIED: scripts/laya_train/pyproject.toml] |
| AWS CDK (`deploy-extensions/`) | aws-cdk-lib ^2.150.0 | the async infra | Existing separate app; `bin/app.ts` already switches stacks by `--context stack=` [VERIFIED: deploy-extensions/bin/app.ts:34-39] |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `aprender-decide` example `pack_laya` | workspace | `pack` / `verify` / `inspect` | Built as a release example inside the arm64 image; do not promote to a bin |
| boto3 / awscli v2 | image-pinned | container-side S3 up/down in the x86 wrapper and the arm64 step | Python wrapper in the GPU image (boto3 already installable under the pinned stack only if added to the lock; prefer `awscli`/`boto3` in a separate layer, not in `uv.lock`) |
| `just` | 1.46.0 | recipes | User rule |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Step Functions `.sync` | EventBridge `SageMaker Training Job State Change` rule -> finalizer Lambda | CONTEXT's named option. Works, but events can arrive more than once ("SageMaker AI may send multiple events ... for each state change" [CITED: docs.aws.amazon.com/sagemaker/latest/dg/automating-sagemaker-with-eventbridge.html]), and chaining the arm64 step needs a second rule plus a second correlation key. The guarded `finish` makes duplicates safe, so this is viable if the owner prefers fewer AWS resource types. |
| Fargate arm64 for pack/verify | Lambda container image arm64 | Lambda caps at 900 s; Phase 10 pack was ~21 min and verify ~22 min on Apple silicon for 2,129 eval rows [VERIFIED: 10-GATE-RUN-EVIDENCE.json `wall_note`]. Fits only small eval sets. |
| Rust port of validation | Python `data.py` in the request path | Python in a Rust provided.al2023 Lambda means a second runtime/package ceiling (pmcp.run package limit 250 MB, an oversized bootstrap already OOM-killed the platform backend once [VERIFIED: deploy-extensions/lib/setfit-training-stack.ts:17-20]). Rust port is the recommendation. |

**Installation (new crates):**
```bash
cargo add aws-sdk-sfn@1 -p aprender-mcp-laya-train-lambda   # after the crate exists
# if rustc 1.93 rejects the newest transitive crates:
CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update
```
[VERIFIED: aws-sdk-s3 > 1.137 needs rustc 1.94.1 per .claude/skills/spike-findings-aprender/references/aws-mcp-model-hosting.md:100-101; local `rustc --version` = 1.93.0]. Whether `aws-sdk-sfn` 1.124.0 builds on 1.93 is [ASSUMED]; the first task must run `cargo check`.

**Version verification:** `cargo search aws-sdk-sfn` returned `aws-sdk-sfn = "1.124.0"` this session; legitimacy seam returned OK, 48,973 weekly downloads, repo github.com/awslabs/aws-sdk-rust.

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| aws-sdk-sfn | crates.io | published 2021-05-08 | 48,973/wk | github.com/awslabs/aws-sdk-rust | OK | Approved |
| aws-sdk-sagemaker | crates.io | published 2021-05-08 | 1,203/wk | github.com/awslabs/aws-sdk-rust | OK | Approved but not needed if Step Functions makes the SageMaker call |
| torch 2.14.0 Linux x86_64 transitive set (`cuda-toolkit[...]==13.0.3`, `cuda-bindings`, `nvidia-cudnn-cu13==9.24.0.43`, `nvidia-cusparselt-cu13`, `nvidia-nccl-cu13`, `nvidia-nvshmem-cu13`, `triton~=3.8.0`) | PyPI | n/a | n/a | pytorch/NVIDIA | **NOT RUN** | `[ASSUMED]` - discovered from the PyPI JSON of torch (a non-authoritative path for the *names*), never put through the legitimacy seam, never in the lock. The Phase 8 README states every pin "was verified by a human ... before the first `uv lock`" [VERIFIED: scripts/laya_train/README.md:30-32]; the Linux additions need the same human `checkpoint:human-verify` before `uv lock` is re-run. |

**Packages removed due to SLOP verdict:** none
**Packages flagged SUS:** none by the seam. The Linux torch set is flagged by process (unverified), not by verdict: the planner must insert a `checkpoint:human-verify` task before re-locking.

## Architecture Patterns

### System Architecture Diagram

```
 MCP client (OAuth, pmcp.run)
      |  validate_dataset / train / train_status / tasks/*
      v
 +--------------------------------------------------------------+
 | request Lambda `bootstrap` (arm64, 1 GB, 30 s API-GW cap)    |
 |  1 parse+refuse args (deny_unknown_fields; name slug)        |
 |  2 verify dataset_uri is inside <bucket>/datasets/<prefix>/  |
 |  3 GET task.json,train.jsonl,eval.jsonl[,shift.jsonl] (caps) |
 |  4 Rust validation (shared case table with data.py)          |
 |  5 single-flight: DynamoDB conditional lock  --refuse--> typed error naming running task
 |  6 mint task, put envelope, sfn:StartExecution(task_id,owner)|
 +----------+---------------------------------------------------+
            | DynamoDB (tasks + lock)            ^ presign at read time (train_status)
            v                                    |
 +----------------------- Step Functions Standard -----------------------+
 | TrainingJob (.sync) --exit 0/3 mapped to job success--> PackVerify     |
 |  ml.g5.xlarge x86_64   channels: data, base          (.sync Fargate    |
 |  image@sha256 (ECR)    wrapper uploads run dir to    arm64, 8 vCPU)   |
 |  MaxRuntime 21600      runs/<name>/<task_id>/        pack, then verify |
 |        | Catch(States.ALL) --------------------------+ writes .apr,    |
 |        |                                              | manifest.json   |
 |        +--> Finalize <---------------------------------+ Catch ------->|
 +-------------------------------+---------------------------------------+
                                 v
              finalizer Lambda: read gate-report + manifest from S3,
              AprenderTaskStore::finish (guarded), tag objects pass/fail,
              release the single-flight lock

 S3 (ONE bucket)  datasets/<prefix>/ (owner-written)   base/laya-55cf4c4e/ (staged once)
                  runs/<name>/<task_id>/{run/, <name>.apr, manifest.json}
```

### Recommended Project Structure
```
crates/aprender-mcp-laya-train/          # lib only (NO [[bin]]): args, tools, payload, name slug,
  src/{lib.rs,args.rs,validate.rs,payload.rs,dispatcher.rs}   # validation port, Dispatcher trait
  tests/validation_cases.rs              # replays scripts/laya_train/data_cases.json
crates/aprender-mcp-laya-train-lambda/   # name MUST be <server-id>-lambda (cargo-pmcp lookup)
  src/bin/bootstrap.rs                   # request fn (loopback proxy, as setfit's bootstrap.rs)
  src/bin/aprender-laya-train-finalizer.rs   # renamed to bootstrap by CDK, as setfit trainer
  src/{lib.rs,sfn.rs,dataset.rs,lock.rs,presign.rs}
scripts/laya_train/data_cases.json       # the shared table (Python + Rust replay)
scripts/laya_train/container/            # Dockerfile.train (x86 CUDA), entrypoint.py, smoke
deploy-extensions/lib/laya-train-stack.ts  # new stack, selected by --context stack=laya-train
deploy-extensions/container/pack-arm64/Dockerfile
```
`deploy-extensions/bin/app.ts` already guards stack names (`['training','embed','embed-probe']`), so add `laya-train` there [VERIFIED: deploy-extensions/bin/app.ts:34-39].

### Pattern 1: mint, envelope, dispatch, compensate (reuse verbatim)
**What:** the handler mints the task via `mint_for_request` (arms the handoff), writes run inputs to the envelope (not the dispatch payload), dispatches, and compensates the task to `failed` if dispatch errs. [VERIFIED: crates/aprender-mcp-setfit-train/src/lib.rs:376-443] The envelope is owner-scoped and TTL-bounded; the dispatch payload carries only `task_id` and `owner` and `deny_unknown_fields` refuses anything else [VERIFIED: crates/aprender-setfit-train-lambda/src/lib.rs:89-96]. The Step Functions execution input must follow the same rule (execution history is readable by anyone with `states:GetExecutionHistory`, the reason chess records, quoted at task_store.rs:62-64).
**Laya envelope contents:** `dataset_uri` (a handle), `name`, `output_prefix`. No dataset content, no tokens.

### Pattern 2: guarded terminal write, first writer wins
`AprenderTaskStore::finish` turns `Working -> terminal` once and silently no-ops a second terminal write [VERIFIED: task_store.rs:552-569]. This makes a duplicate finalizer invocation, a Catch-path finalizer after a success-path one, and a cancel racing a completion all safe. The finalizer must call `finish`, never `update_status`/`set_result` separately.

### Pattern 3: presigned links minted at read time
`status()` re-reads the stored terminal payload and fills in `artifact_url` only when `phase == "completed"`, never stored [VERIFIED: lib.rs:451-508; dispatch.rs:177-194 mints the GET with a 1 h TTL]. For Laya the same applies per manifest file. `DOWNLOAD_URL_TTL` is 1 h; a 0.84 GB `.apr` over a slow link needs a longer TTL for `.apr` and checkpoint links (state the choice; S3 presigned maximum is 7 days with IAM user creds but is bounded by the role session lifetime for role credentials [ASSUMED]).

### Pattern 4: Step Functions Standard with Catch-to-finalizer (new)
```json
{
  "StartAt": "Train",
  "States": {
    "Train": { "Type": "Task", "Resource": "arn:aws:states:::sagemaker:createTrainingJob.sync",
      "Arguments": { "TrainingJobName": "{% 'laya-' & $states.input.task_id %}",
        "ResourceConfig": {"InstanceType":"ml.g5.xlarge","InstanceCount":1,"VolumeSizeInGB":60},
        "StoppingCondition": {"MaxRuntimeInSeconds": 21600},
        "AlgorithmSpecification": {"TrainingImage":"<ecr>@sha256:<digest>","TrainingInputMode":"File"},
        "...": "channels data + base; env from the execution input" },
      "Catch": [{"ErrorEquals":["States.ALL"],"Next":"Finalize"}], "Next":"PackVerify" },
    "PackVerify": { "Type":"Task","Resource":"arn:aws:states:::ecs:runTask.sync",
      "Catch":[{"ErrorEquals":["States.ALL"],"Next":"Finalize"}],"Next":"Finalize" },
    "Finalize": { "Type":"Task","Resource":"arn:aws:states:::lambda:invoke","End":true }
  }
}
```
Shape follows the docs example for `createTrainingJob.sync` and `ecs:runTask.sync` [CITED: docs.aws.amazon.com/step-functions/latest/dg/connect-sagemaker.html, connect-ecs.html]; the exact JSONata/JSONPath form is the planner's. The documented IAM templates for both `.sync` integrations include `StopTrainingJob` and `StopTask`, which is why abort-propagation is plausible; that **stopping an execution stops the training job** was not read as a sentence this session and is [ASSUMED] - verify in the probe by stopping a probe execution. `sfn:StopExecution` is therefore the candidate for `tasks/cancel` (D-"Cancel semantics"), with a direct `StopTrainingJob` as the fallback if the assumption fails.

### Pattern 5: single-flight as a DynamoDB lock (new; the scaffold has none)
Reuse `TaskBackend` only: a reserved owner id (for example `"\u0000laya-single-flight"`; any string no auth subject can equal) with `task_id = "lock"`. Acquire = `get`, if absent `put_new` (conditional insert -> `Conflict` means someone else won), if present and terminal or expired `cas` takeover. Release = the finalizer `cas`es it to a finished state; expiry is MaxRuntime + pack budget + margin (about 8 h) so a lost finalizer cannot wedge the deployment. The refusal names the holder's `task_id` (D-11). Because pmcp serializes requests per container but Lambda scales containers, the CAS, not the in-process mutex, is the guard.

### Anti-Patterns to Avoid
- **Assuming the SetFit server already single-flights.** It does not on Lambda; `RunningJobs` is only referenced from `local.rs` and lib.rs docs [VERIFIED: grep this session, only lib.rs:44 and :64 mention it].
- **Putting the dataset or tokens in the Step Functions input or SageMaker hyperparameters.** Both are retained and readable by other principals.
- **Letting the container's exit code carry the gate result to SageMaker.** `train.py` exits 0 PASS, 3 GATE FAIL, 2 refused [VERIFIED: scripts/laya_train/train.py:66 `EXIT_PASS, EXIT_REFUSED, EXIT_GATE_FAIL = 0, 2, 3`]. SageMaker marks any non-zero container exit as a failed job, so the entrypoint must map 0 and 3 to "job succeeded, outcome written to S3" and let 2 and crashes fail the job. Otherwise every GATE FAIL looks like an infrastructure failure and the reports are not collected.
- **Treating the validation port as the authority.** It is a pre-filter (D-08). A request-side accept followed by a trainer refusal is surfaced as a defect, not retried.
- **Adding `aws-sdk-sagemaker` to the request Lambda.** Not needed and it would force `iam:PassRole` for the SageMaker role onto the platform-owned execution role; Step Functions holds that permission instead.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Task store, mint handoff, guarded terminal write | a second store | `AprenderTaskStore` + `DynamoDbTaskBackend::new` | Tested CAS/handoff logic; the DynamoDB alias lesson (`#ver`, unused-alias rejection) is already paid for [VERIFIED: dynamodb.rs:72-98] |
| Hours-long wait for a training job | a polling Lambda or EventBridge dedup logic | Step Functions `.sync` | Waits without compute; failures route to one Catch |
| UTF-8, NFC, `split_whitespace` normalization | a new normalizer | `aprender_decide::verify::normalize_text` / `normalized_sha256` | Already mirrors `data.py` `nfc-trim-ws-v1` [VERIFIED: verify.rs:1784-1790] |
| `task.json` parsing | a new parser | `aprender_decide::task::Task::from_slice` | Same refusals as the contract, order-preserving criteria [VERIFIED: task.rs:15-19] |
| Gate decision and median selection | any Rust or server-side re-derivation | the trainer's `gate.py` + `pack_laya verify` | The Rust verifier already recomputes the gate from hash-bound probabilities (D-15 depends on this) |
| tar/zip dataset handling | an extractor | none: D-07 reads a prefix of plain files, so no archive path exists. This also removes the SetFit `tar` traversal surface (dataset.rs:101-120) | |
| Presigned URLs | hand-signed URLs | `aws_sdk_s3::presigning::PresigningConfig` as in dispatch.rs:158-193 | |
| HF cache population | custom downloader at job start | stage the exact cache subtree once and extract it (see "Base snapshot staging") | `snapshot_download` offline needs the `trees/<sha>.json` cache, not just the files |
| Container-side S3 transfer | bespoke code | boto3 `upload_file`/`download_file` (multipart, retries) or `aws s3 cp --recursive` | Spike finding: retry each ranged part, a 32-part run failed mid-body [VERIFIED: aws-mcp-model-hosting.md:36-37] |

**Key insight:** every numeric decision (gate bars, seeds, tolerances) already lives in two verifiers (`gate.py`, `pack_laya verify`). The server's job is transport and orchestration; a server that grew its own numerics would be the drift CLAUDE.md warns about for the forecast servers.

## Runtime State Inventory

Not a rename/refactor/migration phase. Omitted. One adjacent fact the planner needs: the deploy root `crates/` is shared between servers, and a third server needs the same swap treatment the decide servers get; see Pitfall 9.

## Common Pitfalls

### Pitfall 1: The epochs conflict with D-06 (blocking, owner decision)
**What goes wrong:** `train.py` calls `contract.resolve_epochs(args.variant, shots_per_class, args.epochs)`. At <= 16 shots/class `--epochs` is *refused*; above 16 it is *required* in [4, 12] [VERIFIED: scripts/laya_train/contract.py:223-248, quoted: `"REFUSED epochs: above 16 shots/class (%d) --epochs is required, in [%d, %d]"`]. The contract text is `epoch_rule: 'shots_per_class <= 16 -> 12 epochs; above 16 -> an epoch count in [4, 12] declared in recipe.json before training'` [VERIFIED: contracts/laya-finetune-gate-v1.yaml:287]. Phase 10 declared 4 (`just laya-train ... --seeds 3 --epochs 4`) and Phase 12 declared 12 [VERIFIED: 10-GATE-RUN-EVIDENCE.json `command`, 12-GATE-RUN-EVIDENCE.json `epochs`]. The motivation-v1 data dir has 295 train rows over 3 classes, so it is above 16 shots/class and the reproduction (D-16) needs `--epochs 12`.
**Why it happens:** the discuss session assumed epochs come from the contract; only the below-16 case does.
**How to avoid:** put it to the owner before planning the tool schema. Options: (a) `train` gains an optional `epochs` argument, accepted only in [4,12] and only above 16 shots/class, with `validate_dataset` reporting `shots_per_class` and whether epochs are required (this is the trainer's own rule exposed 1:1, not an extra knob, but it amends D-06's literal text); (b) the server pins 12 for every above-16 dataset (12 is the declared maximum, early stopping decides the real count, Phase 12's seeds stopped after 5 epochs with best epoch 2 [VERIFIED: 12-GATE-RUN-EVIDENCE.json `seeds.per_seed`]) but a 7,016 shots/class set at 12 epochs is roughly 3x Phase 10's 4.7 h MPS run and breaks the 6 h cap (D-12) unless the A10G is much faster; (c) refuse above-16 datasets (kills D-16). Recommendation: (a), decided by the owner.
**Warning signs:** a `train` that works on the tiny fixture and every few-shot set but refuses with `REFUSED epochs` on the first real dataset.

### Pitfall 2: CUDA 13 wheels versus the SageMaker host driver (blocking probe)
**What goes wrong:** the only torch 2.14.0 Linux x86_64 wheels are the PyPI ones, and they require CUDA 13: `cuda-toolkit[...]==13.0.3`, `nvidia-cudnn-cu13==9.24.0.43`, and so on [VERIFIED: https://pypi.org/pypi/torch/2.14.0/json requires_dist, fetched this session]. `download.pytorch.org/whl/{cu126,cu128,cu130}/torch/` listed nothing newer than 2.10.0 for cp313 manylinux x86_64 when grepped this session [VERIFIED: curl listings this session], so there is no older-CUDA build of the pinned version to fall back to. AWS documents driver 580 as CUDA 13.0 and lists `ml.g5.*` under driver 470 (CUDA 11.4) in the inference driver table [CITED: docs.aws.amazon.com/sagemaker/latest/dg/inference-gpu-drivers.html]. That page is for inference; no training-job driver table was found. If training hosts match, native CUDA 13 does not run, and only NVIDIA's CUDA compatibility package (data-center GPUs, A10G qualifies [ASSUMED]) could bridge a major-version gap [CITED: same page, "If it is a major version difference, the CUDA Compatibility Package will need to be installed"].
**How to avoid:** the first paid action is a cheap probe job on `ml.g5.xlarge`: `nvidia-smi`, `/proc/driver/nvidia/version`, `torch.cuda.is_available()`, one matmul, and the tiny-fixture lifecycle on `--device cuda`. Record the driver. If it fails: add the compat package in the image (the doc's startup snippet switches it on only when the host driver is older than the compat library), or change the torch pin (a lock change = a new numerical artifact, an owner decision, and every committed fixture is "a numerical artifact of these versions" [VERIFIED: scripts/laya_train/README.md:50-51]), or fall to ECS g5g (D-02's escape hatch).
**Warning signs:** `Failed to initialize NVML: Driver/library version mismatch`, `CUDA driver version is insufficient`, or - worse - `torch.cuda.is_available()` False and the trainer silently running on CPU. `request_device` falls through to `cpu` [VERIFIED: train.py:186-196] and `gate-report.json` records `device_used` / `device_is_cpu` [VERIFIED: train.py:906-907]. The wrapper must assert `device_used` starts with `cuda` and fail the job otherwise (CPU would burn the 6 h cap).

### Pitfall 3: the committed `uv.lock` has no Linux resolution
**What goes wrong:** `[tool.uv] environments = ["sys_platform == 'darwin' and platform_machine == 'arm64'"]` and the lock header `supported-markers = ["platform_machine == 'arm64' and sys_platform == 'darwin'"]`; the lock has 0 `nvidia` entries [VERIFIED: pyproject.toml `[tool.uv]`, uv.lock:1-8, grep count 0]. `uv sync --frozen` on Linux x86_64 cannot resolve from it. The README says a Linux/x86 split "would be a resolution nobody verified" [VERIFIED: README.md:47-48].
**How to avoid:** extend `environments` with `sys_platform == 'linux' and platform_machine == 'x86_64'` and re-run `uv lock` (uv keeps existing locked versions where possible [ASSUMED]); prove the darwin entries are unchanged by diffing the lock; put the new package set through a human checkpoint; the Dockerfile then runs `uv sync --frozen --no-dev`. `laya @ git+https://github.com/NandhaKishorM/laya@4066d5d5...` needs network at build time only. Local uv is 0.9.5; the container build should pin the same uv.
**Warning signs:** `uv sync --frozen` failing "does not support platform"; a lock diff that touches darwin hashes.

### Pitfall 4: HF offline load needs the tree cache, not just the files
**What goes wrong:** `Agent("convaiinnovations/laya", revision=<sha>)` calls `snapshot_download` *without* `local_files_only`. With `HF_HUB_OFFLINE=1` and only `snapshots/<sha>/` populated it raised `OfflineModeIsEnabled` at `list_repo_tree` [VERIFIED: experiment this session, hub 1.32.0, stub cache]. The same call against the real Mac cache (which has `trees/<sha>.json`, `blobs/`, `snapshots/<sha>/`) returned the snapshot path [VERIFIED: experiment this session, printed `OK .../snapshots/55cf4c4e...`]. `train.py` `Base.directory()` separately uses `local_files_only=True` [VERIFIED: train.py:259-261]. Laya's own docs say offline caches are supported [VERIFIED: site-packages/laya/revisions.py docstring "including HF_HUB_OFFLINE=1 deployments"].
**How to avoid:** stage the cache subtree exactly: `models--convaiinnovations--laya/{blobs, snapshots/55cf4c4e..., trees/55cf4c4e....json}` (the Mac cache snapshot also holds `typed-decisions/` -> 1.6 GB total vs 0.84 GB for the root checkpoint; exclude it and its blobs), as one S3 object or prefix, with its own sha256 recorded; extract under `HF_HOME` in the entrypoint, set `HF_HUB_OFFLINE=1`, and let `Base`/`Agent` verify `model.safetensors` against the contract digest `891102d372688fc2a094dac56a384bc537b87c63f21f9f3dac0be2b7cbc8d86c` [VERIFIED: contracts/laya-finetune-gate-v1.yaml:355]. A local test (extract the staged tar into a temp `HF_HOME`, run the offline `snapshot_download` and `Agent` load) must pass before any AWS job. Symlinks inside the cache (snapshot -> blob) must survive tar or the stager must dereference and the loader must be re-proved.
**Warning signs:** `OfflineModeIsEnabled`, `LocalEntryNotFoundError`, or the job trying to reach huggingface.co (SageMaker training has internet by default, so a missing cache would *silently download* the base unless `HF_HUB_OFFLINE=1` is set).

### Pitfall 5: `pack_laya` bakes a compile-time contracts path
**What goes wrong:** `fn contract` joins `env!("CARGO_MANIFEST_DIR")` with `../../contracts` and reads the YAML at run time; "The CLI accepts only the path arguments above and reads no environment variable" [VERIFIED: crates/aprender-decide/examples/pack_laya.rs:61-67 and module doc]. A binary built at `/src/crates/aprender-decide` only works where `/src/crates/aprender-decide/../../contracts/{laya-finetune-gate-v1,laya-parity-v1}.yaml` exist. Copying the binary into a slim image fails with `No such file`.
**How to avoid:** build and run at one fixed path in the arm64 image (for example `/src`), copy `contracts/` there, and create the `crates/aprender-decide` directory so `..` resolves. Keep it an example. Promoting it to a bin changes nothing about this and needs a register entry (see Pitfall 6). Record the repo commit and the two contract file sha256s in the run manifest so the policy that judged the run is identified. Do not add an env override: "THE POLICY IS NOT AN ARGUMENT" is a stated design rule.
**Warning signs:** `REFUSED Usage ...` / `No such file or directory` on the first arm64 task.

### Pitfall 6: register correction - the 27 baseline is the wrong register
**What goes wrong:** CONTEXT says the `bootstrap` bin "adds one FALSIFY-MONO-011 allowlist entry (baseline 27)". `ALLOWLIST_BASELINE: usize = 27` guards `allowed_bins`, the shrink-only migration-debt register [VERIFIED: monorepo_invariants.rs:263 and :420]. Thin MCP deployment units go in `deployment_unit_bins` (line 312), guarded by `const DEPLOYMENT_UNIT_BASELINE: usize = 10;` and that register already has exactly 10 entries [VERIFIED: monorepo_invariants.rs:312-323, :448; count = 10 listed names]. The test message: "a new publish = false MCP server binary requires a recorded CONTEXT decision (a `gate="blocking-human"` checkpoint), never a same-PR edit to this list" [VERIFIED: monorepo_invariants.rs:451-457].
**How to avoid:** the new Lambda crate (`publish = false`, required by the same test) takes entry 11 and the baseline moves 10 -> 11 with a comment line in the same style as the 9 -> 10 note. Keep `aprender-mcp-laya-train` as a *library only* (no `src/main.rs`, no `[[bin]]`): an auto-discovered `main.rs` also counts as a bin. The server and the Lambda crate then cost one entry, and the finalizer is a second `[[bin]]` in the same package (SetFit does the same with its trainer bin). The plan needs a blocking-human task that records the owner's go for 10 -> 11. Three further edits are forced by the same new `bootstrap` bin: add the package to the `bootstrap` line of `scripts/duplicate_bin_names_allowlist.txt` (currently five claimants), update the `laya-gates-selftest` `row_dup_bin_names` expectation from 5 to 6 (justfile ~5584; Phase 13 plan 13-10 fixed this same count) and extend the explanatory comment, per [VERIFIED: scripts/duplicate_bin_names_allowlist.txt and 13-HANDOFF.md section 9].
**Warning signs:** `test_no_unauthorized_binaries` red, `check_duplicate_bin_names.sh` red, `laya-gates-selftest` dup-bin-names row red.

### Pitfall 7: the unauthenticated window (D-10)
**What goes wrong:** cargo-pmcp cannot render an auth-enabled descriptor for the pmcp-run target; Phase 13 published `--no-oauth` and the owner enabled OAuth afterwards, measuring an open interval; an unauthenticated `initialize` answers 200 by platform design, `tools/list` and `tools/call` answer 401 once OAuth is on [VERIFIED: 13-HANDOFF.md section 5]. For a server that starts paid jobs, an open `train` is a spend hole.
**How to avoid:** exploit the existing ordering fact that the request function has *no access* to the table or the state machine until the grant step runs ("Between steps 5 and 6 ... every `train` call compensates to `failed` with an AccessDenied" [VERIFIED: deploy-extensions/README.md, step 6 paragraph]). So: deploy the function, owner enables OAuth, `_laya-auth-refusal-check` confirms the 401 [VERIFIED: justfile:4339 recipe exists], and only then run the equivalent of `pmcp-train-grant`. Re-run the check after every redeploy (13-HANDOFF section 9). This ordering is derived, not yet observed for a new server: [ASSUMED] until the first deploy shows AccessDenied before the grant.
**Warning signs:** a `train` that succeeds before the 401 check.

### Pitfall 8: reproduction band versus a razor-thin ECE margin (D-16)
**What goes wrong:** Phase 12's shipped seed (13) had ECE 0.09355 against a bar of 0.10 (headroom 0.0064); seed 17 had 0.11968 and **failed** the gate; seed 23 had 0.09143 [VERIFIED: 12-GATE-RUN-EVIDENCE.json `seeds.per_seed`; thresholds `max_ece 0.1`]. Margins were 0.378 / 0.350 / 0.371. The per-seed spread of ECE (sd about 0.016) is larger than the headroom, and the median-of-three on a different device is a fresh draw. A correct server can legitimately GATE FAIL the reproduction. Spike 027 measured the same flip on MPS replicates [VERIFIED: scripts/laya_train/README.md:136-137].
**How to avoid:** propose the band and the retry policy together before the paid run, and have the owner approve both. A defensible shape: PASS on reproduction means margin >= 0.05 and ECE <= 0.10; "within band" means margin in about [0.30, 0.45] and ECE in about [0.07, 0.12] around 0.3780 / 0.0936 (these two intervals are the researcher's proposal, [ASSUMED], for the owner to set); a GATE FAIL inside the band is *inconclusive*, not a server defect, and Phase 12 already used `attempt_cap 2` [VERIFIED: 12-GATE-RUN-EVIDENCE.json `attempt_cap`]; a margin or ECE far outside the band, or a CPU fallback, is a defect. The first x86 noise record is the more informative result: compare `rescore-noise.json` max_abs/bound with Phase 12's fine-tuned 1.82e-05 / 7.28e-05 and zero-shot 1.25e-05 / 5.01e-05 [VERIFIED: 12-GATE-RUN-EVIDENCE.json `noise`].

### Pitfall 9: the shared deploy root `crates/`
**What goes wrong:** cargo-pmcp resolves the package from `<deploy-root>/{server_name}-lambda` and `crates/.pmcp/deploy.toml` currently names `aprender-setfit-train`; the decide servers get their config installed by `_laya-crates-root-swap`, which backs up and restores `.pmcp/*` and `deploy/` byte-identically [VERIFIED: justfile:2057-2130, crates/.pmcp/deploy.toml.template, deploy-extensions/README.md]. A third server on the same root needs the same swap, or it deploys the wrong crate silently (the SetFit README documents the exact silent failure).
**How to avoid:** server id `aprender-mcp-laya-train` and crate directory `aprender-mcp-laya-train-lambda`, deployed through `_laya-crates-root-swap crates <cfg> <snap> cargo pmcp deploy --manifest-path crates ...`, with the tell `aprender-mcp-laya-train-lambda` in the compile log. Alternatively name the server id `aprender-laya-train` with crate `aprender-laya-train-lambda` (matching the SetFit `aprender-setfit-train` style); both satisfy the lookup, pick one and test it with the existing `laya-resolver-proof`-style check [recipe exists, justfile:1629].

### Pitfall 10: S3 prefix reads need `s3:ListBucket` or a missing key is 403
Known lesson from SetFit [VERIFIED: setfit-training-stack.ts:277-283 and dispatch.rs:225-228]. The policy needs `s3:ListBucket` on the bucket with `Condition: {StringLike: {'s3:prefix': ['datasets/*']}}`; without it `HeadObject`/`GetObject` on a missing `task.json` answers 403 and the "your prefix is missing task.json" message is unreachable. Scope `ListBucket` to `datasets/*` so it discloses nothing about `runs/`.

### Pitfall 11: pack/verify needs the full run dir with the checkpoint
**What goes wrong:** `verify_path` calls `PackInputs::from_run_dir(run_dir, data_dir)` [VERIFIED: crates/aprender-decide/src/verify.rs:3701], which reads the checkpoint safetensors; `laya-deploy apr run data base server` re-verifies from the run dir. So D-20's "existing `laya-deploy` path can then use it unchanged" holds only if the fetched `models/decide/<name>/` contains the **complete run dir including `checkpoint/` (about 0.84 GB, F16)**, the `.apr`, and the data dir. The manifest therefore includes `checkpoint/`, and the fetch recipe restores three things, not one. Checkpoint is in "every report the run writes" only if the planner reads D-04 that way; state it explicitly so a GATE FAIL run's retention cost (0.84 GB x up to 3 seeds is deleted by the trainer except the shipped one [VERIFIED: train.py:865-868 `shutil.rmtree` of the other two]) is understood: one checkpoint per run.

### Pitfall 12: in-flight loss on MaxRuntime / stop
D-12 says "a job that hits it ends failed with whatever reports exist". SageMaker stops the container with SIGTERM and then kills it [ASSUMED: grace period; verify in the probe]. The wrapper must trap SIGTERM and upload the run dir within the grace window, and additionally sync the small JSON files periodically (every few minutes) so an OOM kill or hard stop still leaves `recipe.json`, `seeds/seed-*/eval-probs.json` and partial reports in S3. Per-seed checkpoints are ~0.84 GB each and should not be synced mid-run.

### Pitfall 13: `tasks/cancel` plumbing
`CancelSink::cancel(&self, task_id)` is a *synchronous* trait method that fires only after the record is durably Cancelled [VERIFIED: task_store.rs:376-379, :659-679]. The Lambda request function installs no sink (bootstrap.rs:62-64 comment) and relied on the worker polling. For Laya the sink must start an async `StopExecution` (spawn a task; Lambda freezes on return, so await it inside the request or use a DynamoDB-stream/second path), and the finalizer's guarded `finish` ignores the later terminal write. Test: cancel a probe execution and observe the training job reach `Stopping`/`Stopped`.

### Pitfall 14: TTL versus runtime
SetFit's task TTL is 1 h (`TASK_TTL_MS = 3_600_000`, sized to a 127 s run) [VERIFIED: lib.rs:124]. A Laya run can last 6 h of training plus pack/verify, and the lock must outlive it. A record that expires mid-run makes the finalizer's `finish` return NotFound and the verdict is lost although the S3 outputs exist. Set the TTL to at least MaxRuntime + pack budget + a retrieval window (for example 72 h, the planner's choice), and make the finalizer also write a `verdict.json` next to the manifest in S3 as the durable record.

## Code Examples

### The shared validation table (D-08)
```text
scripts/laya_train/data_cases.json   # NEW: [{id, files:{task.json,train.jsonl,eval.jsonl,shift.jsonl?},
                                     #        expect:{ok:true,...} | {refused:"<rule>"}}]
python data.py --selftest            # existing selftest at data.py:593; add a replay loop
cargo test -p aprender-mcp-laya-train --test validation_cases   # replays the same file
```
Rule names to cover, taken from data.py: `task-missing`, `task-schema`, `task-unknown-key`, `task-type`, `task-too-few-criteria`, `task-duplicate-criterion`, `train-row-label`, `eval-row-label`, `*-row-unknown-key`, `*-row-schema`, `*-row-encoding`, `eval-missing`, `eval-class-coverage`, `eval-train-overlap`, `shift-train-overlap`, `train-conflicting-labels`, `train-class-too-small`, `*-empty` [VERIFIED: scripts/laya_train/data.py:93-219, :222-228 and README.md refusals table lines 143-160]. Two details are easy to miss in a port and are already documented as past defects: `jsonl_lines` splits on `\n` only and strips one `\r` before `\n` (WR-05), and a lone-surrogate escape decodes in Python but is refused by serde_json (`refuse_unencodable`) [VERIFIED: data.py:138-169]. The table must include those two inputs. `eval-class-coverage` (every criterion needs at least one eval row) is NOT in CONTEXT's D-08 list and is a real refusal [VERIFIED: data.py:210-218]; `train-class-too-small` needs the calibration split arithmetic from `contract.constant("calibration_slice_fraction"/"calibration_slice_min_per_class")`, so the Rust side must read those two constants from `contracts/laya-finetune-gate-v1.yaml` (not literals) or be listed as a case the pre-filter intentionally defers to the job; decide explicitly, because "the request-side check never refuses what the trainer accepts, and surfaces any disagreement" is the D-08 requirement. Also the request-side result should return `shots_per_class` and class counts (needed for Pitfall 1).

### Container entrypoint contract (x86 GPU job)
```text
entrypoint.py (the only thing SageMaker runs; ENTRYPOINT in exec form):
 1 assert nvidia-smi + torch.cuda.is_available(); print driver; set HF_HUB_OFFLINE=1
 2 extract staged base cache (channel `base`) under $HF_HOME; verify tar/blobs sha
 3 train.py --data /opt/ml/input/data/data --out /opt/ml/work/run --device cuda [--epochs N]   # no other flags
 4 map exit: 0 PASS and 3 GATE FAIL -> write outcome.json, continue; 2 REFUSED/crash -> outcome.json{refused}, exit 1
 5 assert gate-report device_used startswith "cuda" else fail (CPU fallback is flagged, not silent)
 6 compute sha256 of every file -> training-manifest.json; upload run dir with
   tag outcome=pending to s3://<bucket>/runs/<name>/<task_id>/run/ ; then exit 0
 7 trap SIGTERM -> best-effort upload; periodic small-file sync
```
Reads of `name`, `task_id`, bucket and prefix come from SageMaker `Environment`/hyperparameters (not secrets). The trainer is invoked with no seeds/stopping/base flags: `--seeds` defaults to the contract's 3 and any other is refused [VERIFIED: scripts/laya_train/README.md:16].

### Shape of the D-18 status payload (reuse `run_payload`'s one-shape rule)
```text
{ schema_version, task_id, phase: working|completed|gate_failed|failed|cancelled,
  verdict: { pass, margin, ece_post, seeds:{shipped, per_seed:[{seed,macro_f1,ece_post,margin,pass,rank_key}]},
             device_used, torch_version, recipe_id, noise:{...}, shift_probe?:{...}, apr_sha256? },
  manifest: [{path, size, sha256, url?}],   // url only in train_status, null in the stored record
  error? }
```
`gate_failed` is a *completed run with a negative verdict*, not an error; model it as a non-error tool result with `pass:false` so clients can read reports (D-04), while infrastructure failures use `is_error` as in `terminal_result` [VERIFIED: lib.rs:340-349]. The stored item must stay far below the 380,000-byte ceiling [VERIFIED: dynamodb.rs:62]; never embed `slice_ids` or probability arrays, only the verdict and the manifest.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| SetFit: one Lambda worker per run (127 s) | SageMaker job + arm64 step + finalizer | this phase | State machine now justified [VERIFIED: dispatch.rs:4-14 states the trigger: "When training grows a second stage ... that is the moment the state machine earns its keep"] |
| Laya training on a laptop (`just laya-train`, MPS) | SageMaker CUDA | this phase | First x86 noise record; parity bars unmeasured on x86_64 [VERIFIED: contracts/laya-parity-v1.yaml:322, "x86_64: UNMEASURED (risk)"] |
| `torch` from per-CUDA PyTorch indexes | PyPI wheels with CUDA 13 pulled as `nvidia-*-cu13` | torch 2.14 | Pin forces CUDA 13 (Pitfall 2) |

**Deprecated/outdated in CONTEXT:** the "baseline 27" register reference (Pitfall 6); the implicit assumption that epochs are contract-determined (Pitfall 1); "EventBridge job-state rule" as the only finalizer trigger (Alternatives).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Stopping a Step Functions execution stops the SageMaker training job and the ECS task (the IAM templates grant `StopTrainingJob`/`StopTask`; the abort sentence was not read) | Pattern 4, Pitfall 13 | Cancel leaves a paid GPU running; fallback is direct `StopTrainingJob` from the cancel sink |
| A2 | `aws-sdk-sfn` 1.124.0 builds on rustc 1.93 | Standard Stack | One `cargo update` with the resolver fallback; low |
| A3 | ml.g5 *training* hosts run a driver older than 580 | Pitfall 2 | If 580+, no compat work needed; if older, compat package or pin change; probe decides |
| A4 | NVIDIA's CUDA compat package supports A10G (data-center GPU) for CUDA 13 on an older driver | Pitfall 2 | If not, ECS g5g (D-02) or an older torch pin |
| A5 | `uv lock` preserves existing darwin pins when a Linux environment is added | Pitfall 3 | Re-lock churns darwin hashes; every committed fixture is a numerical artifact (README.md:50-51) |
| A6 | SageMaker sends SIGTERM and a bounded grace before SIGKILL at MaxRuntime/stop | Pitfall 12 | Partial reports lost on timeout; mitigated by periodic small-file sync |
| A7 | Deploying OAuth-off with no IAM grant means `train` compensates to `failed` until the grant | Pitfall 7 | The open window is a real spend window; the 13-style watchdog would be needed |
| A8 | S3 `PutObject` with `x-amz-tagging` needs `s3:PutObjectTagging`; lifecycle filters cannot negate a tag | Retention | Different lifecycle design needed |
| A9 | Presigned URL lifetime is bounded by the signing credential's session when signed by an assumed role | Pattern 3 | Long `.apr` downloads fail mid-way; choose TTL below the session |
| A10 | g5.xlarge on-demand training rate is about $1.41/h (third-party aggregator, not AWS) | Cost | Confirm with the owner/AWS pricing page before D-13 sign-off |
| A11 | The reproduction band proposal (margin about [0.30,0.45], ECE about [0.07,0.12]) | Pitfall 8 | Owner sets it; this is only a starting point |
| A12 | The A10G with 16 GB host RAM holds ModernBERT-large 421M fp32 AdamW at the recipe batch size | Container | OOM on first run; the probe on the real shape measures it |
| A13 | Model-name slug rule `^[a-z0-9][a-z0-9-]{0,47}$` is acceptable | Security | Owner may want another alphabet or length |

## Open Questions

1. **How is the epoch count decided above 16 shots/class? (blocking, owner)**
   - What we know: the trainer requires `--epochs` in [4,12] there and refuses it below; Phases 10 and 12 used 4 and 12.
   - What's unclear: whether D-06 should be amended to allow an optional `epochs` (recommended), or the server pins a value.
   - Recommendation: option (a) in Pitfall 1; the plan's first decision checkpoint.

2. **Does the ml.g5 training host run CUDA 13 torch? (blocking, probe)**
   - What we know: wheels are cu13 only; the only found AWS driver table puts g5 at 470.
   - What's unclear: the training-job driver; compat package behaviour.
   - Recommendation: a probe job before the lock work is merged.

3. **Register: owner go for deployment-unit 10 -> 11 (blocking-human).**
   - Recommendation: lib-only server crate plus one Lambda crate; record the decision in CONTEXT or a DECISION file, then edit the test and the two allowlists in the same PR as the crate.

4. **Where do the owner's datasets get into the bucket?** D-03 has the owner or a recipe write `datasets/<prefix>/`. There is no upload tool. A `just laya-dataset-put <dir> <prefix>` recipe (aws CLI, owner-run) is the obvious companion; the motivation-v1 dir (395 lines, 92 KB) is local and gitignored (`/data/` root-anchored) and would be the first upload. Needs the owner (AWS call).

5. **Service quota for `ml.g5.xlarge` for training job usage in ze-kasher-dev.** Quotas for training, endpoint and processing are separate; nothing could be checked without an AWS call. Owner action; likely a Service Quotas request.

6. **Image digest pinning and records.** Recommended: build in CodeBuild, push to ECR, reference `repo@sha256:<digest>` in `TrainingImage` and in the Fargate task definition revision, and write both digests into `manifest.json`. Who runs CodeBuild is an owner/AWS action.

7. **Retention of GATE PASS runs includes a 0.84 GB checkpoint per run.** "Indefinitely" is the owner's word; flag the storage line item once.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| rustc / cargo | all Rust crates | yes | 1.93.0 | - |
| cargo-zigbuild | arm64 Lambda cross-build | yes | installed | - |
| cargo-pmcp | request fn deploy | yes | `~/.cargo/bin/cargo-pmcp` | - |
| just | recipes | yes | 1.46.0 | - |
| uv | lock/sync | yes | 0.9.5 | - |
| Python | selftests | yes | 3.14.6 system; project pins 3.13.7 via uv | uv manages 3.13 |
| Node / CDK | `deploy-extensions` | yes | v24.19.0, `aws-cdk` in node_modules | - |
| Docker | local image checks | yes | 29.9.1 (Apple silicon: x86 image is emulated, slow) | build in CodeBuild |
| aws CLI | owner-run recipes | yes | installed (no calls made) | - |
| pmcp.run credentials | deploy | present but single-slot | `~/.pmcp/credentials.toml` holds one session (13-HANDOFF section 9) | owner logs in for ze-kasher-dev |
| NVIDIA GPU, CUDA | local CUDA check | no (Mac) | - | none; probe job is the only CUDA check |
| `ml.g5.xlarge` quota | D-13 | unknown | - | Service Quotas request (owner) |
| ECR / CodeBuild / Fargate arm64 in ze-kasher-dev | images, pack step | unknown | - | owner confirms; spike 021-023 used CodeBuild ARM_CONTAINER + Fargate arm64 successfully |

**Missing dependencies with no fallback:** a local CUDA device (so CUDA behaviour can only be observed on AWS, hence the probe), the g5 quota.
**Missing dependencies with fallback:** local x86 build (CodeBuild).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust: `cargo test` / `cargo nextest` (CI `workspace-test`); Python: `data.py --selftest`, `gate.py --selftest`, lifecycle (local-only, torch) |
| Config file | workspace `Cargo.toml`; `ci/explicit-test-commands.d/NNN-*.cmd` for integration targets |
| Quick run command | `cargo test -p aprender-mcp-laya-train --lib` and `cargo test -p aprender-mcp-laya-train-lambda --lib` |
| Full suite command | `cargo nextest run --profile ci --workspace --lib --exclude aprender-gpu --exclude aprender-cuda-edge --exclude aprender-compute` plus `cargo test -p aprender-core --test monorepo_invariants` |

New crates' `--lib` tests join `workspace-test` automatically; a new `tests/*.rs` is dark until a fragment names it (take a free ordinal between two others, e.g. 535; never renumber), and `ci_run_explicit_test_commands.sh --list` exits 2 on macOS (GNU find), so assert fragments directly in local verifies [VERIFIED: CLAUDE.md Testing section; project memory].

### Phase Decision -> Test Map
| Decision | Behavior | Test Type | Automated Command | File Exists? |
|---------|----------|-----------|-------------------|-------------|
| D-06 | unknown `train` args refused; name slug refused/accepted | unit | `cargo test -p aprender-mcp-laya-train --lib args` | Wave 0 |
| D-08 | Rust validation equals data.py on one shared table | unit + python | `cargo test -p aprender-mcp-laya-train --test validation_cases` and `uv run --project scripts/laya_train --frozen python scripts/laya_train/data.py --selftest` | Wave 0 (table is new) |
| D-11 | second `train` refused naming running task; lock takeover after expiry; release by finalizer | unit (in-memory backend) | `cargo test -p aprender-mcp-laya-train-lambda --lib lock` | Wave 0 |
| D-10 | no tool callable without auth (platform) | live check | `just _laya-auth-refusal-check <url> 0` (owner) | exists |
| D-18 | one payload shape; links absent in stored record | unit | `cargo test -p aprender-mcp-laya-train --lib payload` | Wave 0 |
| D-19 | lifecycle rules by tag; pending default expires | cdk synth assertion | `just synth-laya-train` + a jest-free assertion script on the template | Wave 0 |
| D-20 | fetch recipe verifies every sha256, refuses a tampered file, restores run+data+apr | recipe selftest (offline fixture) | `just laya-fetch-selftest` (new, mutation-proven) | Wave 0 |
| D-14 | pack/verify step runs at the fixed contracts path | local docker arm64 smoke | `docker run --platform linux/arm64 <img> pack_laya inspect <fixture.apr>` | Wave 0 |
| D-02/12/13 | job shape (instance, MaxRuntime 21600, channels) | ASL/CDK synth assertion | `just synth-laya-train` and a JSON assertion on the state machine | Wave 0 |
| D-15 | verify refusal -> failed task, no `.apr`, reports attached | finalizer unit with fixture outputs | `cargo test -p aprender-mcp-laya-train-lambda --lib finalize` | Wave 0 |
| D-16/17 | reproduction inside the approved band | live, owner-run, evidence JSON | probe + one paid run | n/a |
| MONO-011 | register 11, dup-bin allowlist, selftest row | existing tests | `cargo test -p aprender-core --test monorepo_invariants`; `bash scripts/check_duplicate_bin_names.sh`; `just laya-gates-selftest` | exists, edit |
| container | the pinned stack imports and the lifecycle passes | local-only | `just laya-train-lifecycle` locally; the same inside the image on CPU as a CodeBuild smoke step | exists (local) |

### Sampling Rate
- **Per task commit:** the quick run commands above for the touched crate.
- **Per wave merge:** `cargo test -p aprender-core --test monorepo_invariants` and `cargo clippy -p <crate> --lib -- -D warnings` for each new crate; `bashrs lint` on new shell.
- **Phase gate:** full suite green, `readme_contract` green (README crate-count is a *ratchet*: it may lag, never overstate; currently 94 packages vs README 93 [VERIFIED: cargo metadata this session; readme_contract.rs:131-140]), then `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `scripts/laya_train/data_cases.json` + replay in `data.py --selftest` + `crates/aprender-mcp-laya-train/tests/validation_cases.rs`
- [ ] `crates/aprender-mcp-laya-train/` and `crates/aprender-mcp-laya-train-lambda/` skeletons with in-memory backend tests
- [ ] CI fragment for any new `tests/*.rs` (ordinal free between two others)
- [ ] CDK stack + synth recipe; ASL JSON assertion
- [ ] Dockerfiles and a CodeBuild buildspec (owner runs); local arm64 smoke recipe
- [ ] fetch recipe + selftest with a tampered-file negative control
- [ ] the probe plan (CUDA/driver/stop-execution/SIGTERM) as an owner-gated checklist

## Security Domain

`security_enforcement` is enabled (absent/true in `.planning/config.json`; `security_asvs_level` 1, `security_block_on` high).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes | pmcp.run OAuth required (D-10); verify 401 on `tools/list`/`tools/call` after every deploy |
| V3 Session Management | partial | stateless streamable HTTP; no server sessions (`StreamableHttpServerConfig::stateless()`) |
| V4 Access Control | yes | single bucket, so access control is by prefix: `dataset_uri` must resolve inside `s3://<bucket>/datasets/`; reject `..`, empty segments, other buckets, percent-encoded traversal; owner-scoped task reads; runs readable by any authenticated caller is a stated single-tenant posture to confirm with the owner |
| V5 Input Validation | yes | `deny_unknown_fields` args; name slug; per-file and total size caps and a row-count cap before parsing; UTF-8 and JSON refusal in the Rust port; never log row text (the existing scripts log counts and sha256 only [VERIFIED: README.md:62]) |
| V6 Cryptography | partial | sha2 for manifests (already a dependency); SSE-S3 at rest, `enforceSSL` on the bucket as in the SetFit stack; presigned URL TTLs bounded |
| V7 Error handling/logging | yes | named refusals; no credentials, presigned URLs or dataset text in logs or in the DynamoDB item |
| V13 API | yes | typed errors; 413 before parse via `max_request_bytes` |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Confused deputy: caller names another bucket/prefix as `dataset_uri` and the server role reads it | Elevation of privilege / Info disclosure | verify bucket == server bucket and key under `datasets/` in code AND scope the role's `s3:GetObject`/`ListBucket` to that prefix (defence in depth, as `artifact_download_url` does at dispatch.rs:181-184) |
| Cost abuse: repeated or concurrent `train` | DoS (financial) | OAuth, single-flight lock with a typed refusal, MaxRuntime 21600, fixed instance type in the ASL (not caller-set), account service quota |
| Open window before OAuth is enabled | Spoofing | grant-after-401 ordering (Pitfall 7) |
| Secrets in Step Functions input / SageMaker env | Info disclosure | pass only task id, owner, name and S3 handles |
| Tampered base snapshot or image | Tampering | contract digest check in `Agent(... expected_sha256=...)`; image by `@sha256` digest; staged base sha recorded |
| Presigned link leakage | Info disclosure | links never stored; TTL bounded; `tasks/result` carries no links |
| Log/path injection through the model name | Tampering | slug allowlist; never interpolate `name` into shell; build S3 keys from validated parts only |
| Hostile dataset (JSON bomb, giant lines, billion-row file) in the request path | DoS | `Content-Length`/HeadObject cap before GET, streaming parse with a row cap, request Lambda timeout 30 s |
| Role over-breadth on the SageMaker execution role | Elevation | read `datasets/*` and `base/*`, write `runs/*` only; no `iam:*`; no network-isolation flag (it would block the in-container S3 upload) |
| Orphaned paid job | DoS (financial) | execution-level timeout, `MaxRuntimeInSeconds`, a periodic reconciler is NOT required if SFN owns the job; alarm on `ListTrainingJobs InProgress` count > 1 |

## Sources

### Primary (HIGH confidence)
- In-repo, read this session: `crates/aprender-mcp-setfit-train/src/{lib.rs,task_store.rs}`, `crates/aprender-setfit-train-lambda/src/{lib.rs,dispatch.rs,dynamodb.rs,dataset.rs,bin/bootstrap.rs,bin/aprender-setfit-trainer.rs}`, `deploy-extensions/{bin/app.ts,lib/setfit-training-stack.ts,README.md}`, `justfile` (60-330, 1880-2130), `scripts/laya_train/{README.md,train.py,data.py,contract.py,pyproject.toml,uv.lock}`, `crates/aprender-decide/examples/pack_laya.rs`, `crates/aprender-decide/src/{task.rs,verify.rs (portions)}`, `crates/aprender-core/tests/monorepo_invariants.rs`, `scripts/duplicate_bin_names_allowlist.txt`, `contracts/laya-finetune-gate-v1.yaml` (portions), `contracts/laya-parity-v1.yaml` (grep), the Phase 10/12 evidence JSONs, `13-HANDOFF.md`, the spike skill and `aws-mcp-model-hosting.md`.
- Experiments this session: offline `snapshot_download` with a stub cache (fails) and with the real cache (succeeds) under huggingface_hub 1.32.0.
- https://pypi.org/pypi/torch/2.14.0/json (requires_dist, cu13)
- https://download.pytorch.org/whl/{cu126,cu128,cu130}/torch/ (listing, nothing above 2.10.0 for cp313 manylinux_2_28_x86_64)
- https://docs.aws.amazon.com/sagemaker/latest/dg/automating-sagemaker-with-eventbridge.html (training state-change event; duplicate-event note)
- https://docs.aws.amazon.com/sagemaker/latest/APIReference/API_OutputDataConfig.html (`CompressionType` GZIP|NONE)
- https://docs.aws.amazon.com/step-functions/latest/dg/connect-sagemaker.html and connect-ecs.html (`.sync` patterns, IAM templates incl. StopTrainingJob/StopTask)

### Secondary (MEDIUM confidence)
- https://docs.aws.amazon.com/sagemaker/latest/dg/inference-gpu-drivers.html (inference driver table; ml.g5 = 470; applicability to training not stated)
- gsd package-legitimacy seam (`aws-sdk-sfn`, `aws-sdk-sagemaker`: OK)

### Tertiary (LOW confidence)
- calculator.holori.com/aws/sagemaker/ml.g5.xlarge (about $1.41/h, third-party aggregator, last updated 2026-02-11)
- Training-knowledge claims tagged `[ASSUMED]` above (A1-A13)

## Metadata

**Confidence breakdown:**
- Standard stack and scaffold reuse: HIGH - every cited type and seam was opened.
- Architecture (SFN + Fargate + finalizer): MEDIUM - sound against the docs read, abort-propagation and SIGTERM grace unverified.
- Container/CUDA: LOW to MEDIUM - cu13 wheels are certain; the host driver is unknown until a probe.
- Pitfalls: HIGH for the six that are in-repo facts (1, 3, 5, 6, 10, 11); MEDIUM for AWS-behaviour ones.

**Research date:** 2026-10-28
**Valid until:** 2026-11-11 for the AWS behaviour claims (fast-moving: SageMaker driver tables, torch CUDA targets); the in-repo facts are valid until the touched files change.
