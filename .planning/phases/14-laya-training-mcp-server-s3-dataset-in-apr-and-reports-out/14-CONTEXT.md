# Phase 14: Laya training MCP server: S3 dataset in, .apr and reports out - Context

**Gathered:** 2026-10-28 (interactive discuss-phase; the five owner decisions of 2026-10-28 recorded in ROADMAP.md are carried in as D-01..D-05)
**Status:** Ready for planning

<domain>
## Phase Boundary

A thin training MCP server for ONE algorithm, Laya. It takes a prepared dataset by S3 URI, runs the full
`laya-finetune-gate-v1` lifecycle on a SageMaker GPU training job, packs and Rust-verifies the `.apr` on arm64,
and returns the full output (the `.apr` on GATE PASS plus every report, readable on GATE FAIL too) as an MCP
Task. It is deployed to the dev environment, ze-kasher-dev, on pmcp.run.

In scope:
- The server: a request Lambda with `validate_dataset`, `train` and `train_status` tools; the MCP Task surface; the
  DynamoDB task store; single-flight.
- The training container: the pinned `scripts/laya_train` uv project on a PyTorch CUDA image, plus base-snapshot
  staging from S3.
- The arm64 pack-and-verify step.
- The finalizer that writes the terminal task record from job state.
- The S3 layout and its lifecycle rule.
- A `just` fetch recipe that pulls a run into `models/decide/<name>/` with sha256 checks.
- The motivation-v1 reproduction probe.
- The deployment-unit register entry for the new Lambda crate.

Not in this phase:
- Any change to a decide server.
- Auto-deploy of a trained artifact.
- A Rust trainer.
- Upload-through-MCP.
- Server-side preparation of raw partner CSVs.
- Caller-set training knobs.
- Any CUDA-specific amendment of `laya-parity-v1` without the owner (D-15).
- augury-dev.

</domain>

<decisions>
## Implementation Decisions

### Carried in from the owner (2026-10-28, ROADMAP.md)
- **D-01:** Tenant and account: the dev environment, **ze-kasher-dev**, first. augury-dev is out of this phase.
- **D-02:** Compute: **SageMaker Training Jobs first**. Switch to **ECS** only if SageMaker shows a limitation
  beyond a reasonable range on Rust tooling, x86_64, latency or cost. ECS's one real advantage is that Graviton
  GPU hosts (g5g) keep aarch64.
  — **Reversibility:** costly — the dispatcher, finalizer, IAM and container contract are SageMaker-shaped. A move
  to ECS rewrites them, though not the request Lambda or the task store.
- **D-03:** Dataset access: **a single server-owned bucket read by the server's role**. Upload-through-MCP (the
  SetFit server's presigned-PUT slot) is deferred, not rejected.
- **D-04:** Output: **full**. The `.apr` plus every report the run writes, readable and usable by the caller.
- **D-05:** Training stays **Python/torch on Laya's own code** (Phase 8 D-01: "A Rust trainer is not planned").
  Training rows are built by `Agent._encode_state`, so they equal inference rows byte for byte. This phase does not
  reopen that.

### Caller surface
- **D-06:** The `train` tool takes **the dataset URI, a model name and an optional `epochs`** (amended
  2026-10-28 by D-21). Seeds (13/17/23, median-ECE
  selection), the stopping rule, the epochs rule and the gate bars all come from `contracts/laya-finetune-gate-v1.yaml` at
  run time. Every run is a declared gate run, comparable with Phases 10 and 12. The shift probe runs when the
  prefix holds `shift.jsonl`, exactly as the CLI does today. Unknown arguments are refused, not ignored.
- **D-07:** Dataset shape: **a prepared data dir only**. That is an S3 prefix holding `task.json`, `train.jsonl`
  and `eval.jsonl`, with `shift.jsonl` optional: the trainer's generic contract (`scripts/laya_train/data.py`).
  Preparing data from partner CSVs stays local with the `prepare_*` scripts.
- **D-08:** **A synchronous `validate_dataset` tool**, and the same check run first inside `train`. It applies
  data.py's refusals (task schema and unknown keys, label membership, UTF-8 encoding, eval or shift overlap with
  train, conflicting train labels) in the request path and answers in seconds, before any GPU job is paid for. The
  job still re-runs the trainer's own validation. The request-side check is a pre-filter, never the authority, and
  a disagreement between the two is a defect to surface. The planner decides whether this is a Rust port with a
  case table shared with data.py, or Python in the request path. It must be one rule set proven equal by a shared
  table, not two drifting copies.
- **D-09:** Naming: **a caller-given model name plus the task id**. Output lands under `runs/<name>/<task_id>/`,
  so reruns of one model sit together and never overwrite. The name is validated, for example as a lowercase slug,
  and refused otherwise.

### Access and spend limits
- **D-10:** Auth: **pmcp.run OAuth is required**, as on the augury motivation server since Phase 13. Every call
  can start a paid GPU job.
- **D-11:** Concurrency: **one job at a time per deployment** (single-flight, as the SetFit server does). A second
  `train` while one runs is refused with a typed error naming the running task.
- **D-12:** Ceiling: **6 hours per job** (SageMaker `MaxRuntimeInSeconds` = 21600). It covers Phase 10's 4.7 h MPS
  run with margin. A job that hits it ends failed with whatever reports exist.
- **D-13:** Instance: **`ml.g5.xlarge` (A10G, 24 GB)** for the first jobs. The planner confirms price and service
  quota in ze-kasher-dev with the owner before any job runs.

### x86_64 and CUDA acceptance
- **D-14:** **Pack and verify run on arm64 after training**, never in the x86 GPU container. The GPU job writes its
  run dir to S3. `pack_laya pack` then `verify` run on Graviton CPU, as a Fargate arm64 task or an equivalent the
  planner picks. That is the architecture where every parity and pack-tolerance bar was measured and where the
  `.apr` is served. x86_64 touches only torch. `pack_laya` is a cargo example today, so the planner decides whether
  it ships as a prebuilt example binary or is promoted to a bin; either way it needs a FALSIFY-MONO-011 decision.
- **D-15:** **A failed pack verify is a failed run, and the owner decides what comes next.** Examples are a noise
  bound over the ceiling or an argmax disagreement. The task ends failed with every report attached and no `.apr`.
  Any amendment of `contracts/laya-parity-v1.yaml` for CUDA-trained checkpoints needs the owner's explicit
  decision, as amendment A1 did in Phase 8. It is never made by an executor.
  — **Reversibility:** one-way — a contract amendment changes what every later pack accepts.
- **D-16:** **Trust bar: reproduce motivation-v1.** Before the server's first real gate run is trusted, retrain
  motivation-v1 on SageMaker from the same data dir as Phase 12. It must pass the gate and land within a band the
  planner proposes and the owner approves, around Phase 12's numbers: margin 0.3780, post-calibration ECE 0.0936,
  wall 752 s on MPS. This is one paid run. Its evidence goes to a phase evidence JSON like the earlier gate runs.
- **D-17:** Reproducibility is claimed **at the gate-outcome level**. The same data and recipe give the same gate
  verdict, with metrics in a stated band; bytes may differ. This matches Phases 10 and 12 (spike 027: MPS training
  is not bitwise reproducible). No `torch.use_deterministic_algorithms`.

### Result hand-off
- **D-18:** `train_status` returns **the verdict inline plus a manifest**. Inline: pass or fail, margin, ECE, the
  per-seed rows, the shipped seed and the `.apr` sha256 when there is one. The manifest lists every output file with
  its sha256 and a presigned GET link. Links are minted when the status is read and never stored, as in the SetFit
  server, so the verdict in the task store stays true for the task's TTL. `tasks/result` carries the verdict and
  manifest without links.
- **D-19:** Retention: **GATE PASS runs are kept indefinitely. GATE FAIL and failed runs expire after 30 days**, by
  an S3 lifecycle rule. The planner chooses how a run is marked so the rule can tell them apart (tag or prefix).
- **D-20:** **A GATE PASS stops at S3, plus a fetch recipe.** A `just` recipe pulls a run into
  `models/decide/<name>/` and verifies every sha256 against the manifest. The existing owner-run `laya-deploy` path
  can then use it unchanged. No decide server changes.

### Amendments from research (owner, 2026-10-28)
- **D-21:** **`train` takes an optional `epochs`**, amending D-06. The trainer's `contract.resolve_epochs`
  (`scripts/laya_train/contract.py:223-248`) refuses `--epochs` at 16 or fewer shots per class and requires a
  value in [4,12] above 16, with no default, so a two-argument tool could not run an above-16 dataset. The
  server accepts `epochs` only in [4,12] and only above 16 shots per class, refusing it otherwise with a typed
  error. An above-16 dataset sent without `epochs` is refused too, never defaulted. `validate_dataset` reports
  `shots_per_class` so the caller knows whether `epochs` is required. The D-16 reproduction passes `epochs` 12,
  as Phase 12 did. Every other knob stays contract-owned.
- **D-22:** **The FALSIFY-MONO-011 deployment-unit register grows from 10 to 11.** The Lambda crate's
  `bootstrap` bin is a deployment unit, so it goes in `deployment_unit_bins`
  (`DEPLOYMENT_UNIT_BASELINE` 10 to 11, `crates/aprender-core/tests/monorepo_invariants.rs`), not the
  shrink-only `allowed_bins` debt list (27) that the discretion note below first named. `aprender-mcp-laya-train`
  stays a library with no bin, so there is exactly one new entry. The same PR updates
  `scripts/duplicate_bin_names_allowlist.txt` and the `laya-gates-selftest` duplicate-bin-names row (5 to 6
  `bootstrap` claimants).

### Claude's Discretion
- The server id and crate names. One server per algorithm: proposed `aprender-mcp-laya-train` plus its Lambda
  crate. Its `bootstrap` bin adds one FALSIFY-MONO-011 deployment-unit entry (register 10 to 11, D-22; not the 27-entry `allowed_bins` list), with the Lambda-ABI rationale
  the register already carries.
- How the base snapshot (`convaiinnovations/laya` @ 55cf4c4e, 0.84 GB, sha-pinned) reaches the job: an S3 input
  channel, never baked into the image (spike finding: baking weights in costs Lambda 800 s, Fargate 4×). The sha256
  is checked in the job.
- Where the container image is built and stored (ECR in ze-kasher-dev), and how its digest is pinned and recorded
  in the run.
- Cancel semantics: `tasks/cancel` maps to `StopTrainingJob`, plus the guarded terminal write from the SetFit
  task-store model.
- The finalizer mechanism: an EventBridge SageMaker job-state rule to a finalizer Lambda, or an equivalent. It
  moves the task to its terminal state and triggers the arm64 pack-and-verify step on success.
- The task TTL and the exact S3 key layout under `runs/<name>/<task_id>/`.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### The training lifecycle and its contracts
- `contracts/laya-finetune-gate-v1.yaml` — the gate, the seed policy (three seeds, median-ECE), the stopping rule,
  epochs, the base pin, `run_dir_layout`. It is the source of every value the server does NOT let a caller set
  (D-06).
- `contracts/laya-parity-v1.yaml` — the fixture bar and the pack/verify re-score bar (A1), all measured on aarch64.
  D-14 and D-15 hinge on it.
- `contracts/decide-apr-v1.yaml` — the `.apr` schema and the `task_json_schema` that `task.json` must meet.
- `scripts/laya_train/README.md` — the Training section (the 11 steps, the run-dir layout, exit codes 0/3/2, device
  read-back).
- `scripts/laya_train/train.py`, `scripts/laya_train/data.py`, `scripts/laya_train/gate.py`,
  `scripts/laya_train/contract.py` — the trainer, the data refusals D-08 mirrors, the gate decision, and the
  contract reader.
- `scripts/laya_train/pyproject.toml`, `scripts/laya_train/uv.lock` — the pinned torch stack the container must
  run frozen.
- `crates/aprender-decide/examples/pack_laya.rs` — `pack`, `verify`, `inspect`: the arm64 step (D-14).
- `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-CONTEXT.md` — D-01 (Python
  training), D-02 (no `apr` subcommand), D-03 (device read-back).

### The scaffold to reuse (SetFit training server, live 2026-09-04)
- `crates/aprender-mcp-setfit-train/src/lib.rs`, `crates/aprender-mcp-setfit-train/src/task_store.rs`,
  `crates/aprender-mcp-setfit-train/src/local.rs` — tools, MCP Tasks, the `RequestMintHandoff` and `TaskBackend`
  seams, single-flight, guarded terminal writes.
- `crates/aprender-setfit-train-lambda/src/dispatch.rs`, `crates/aprender-setfit-train-lambda/src/dynamodb.rs`,
  `crates/aprender-setfit-train-lambda/src/dataset.rs`, `crates/aprender-setfit-train-lambda/src/bin/bootstrap.rs`
  — the DynamoDB backend (CAS with an aliased `#ver`, consistent reads), dataset-URI verification (bucket and prefix
  check, HeadObject), and presigned links minted when the status is read.
- `deploy-extensions/` — the separate CDK app for the async infra (table, bucket), next to the platform-rendered
  stack.
- `justfile` recipes `build-trainer-asset` and `pmcp-train-config` — cross-compile and glibc-floor checks, and
  filling the deploy config from SSM.

### Deployment, auth and register
- `justfile` recipes `laya-deploy-config`, `laya-deploy`, `_laya-auth-refusal-check` — the OAuth-on deploy path
  and the 401 check. Phase 13 CR-01 is deferred, so a redeploy guard for auth-on servers does not exist yet; this
  server is auth-on from the start (D-10).
- `.planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-HANDOFF.md` — section 9,
  the manual auth-on redeploy procedure, and the Cognito/M2M facts.
- `crates/aprender-core/tests/monorepo_invariants.rs` — FALSIFY-MONO-011, the `[[bin]]` allowlist (baseline 27).
- `.claude/skills/spike-findings-aprender/references/aws-mcp-model-hosting.md` — measured S3 bandwidth, why weights
  are never baked into images, Fargate pull rates.

### Evidence the trust bar compares against
- `.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-GATE-RUN-EVIDENCE.json` —
  motivation-v1: margin 0.3780, ECE 0.0936, 752 s on mps:0 (D-16).
- `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-GATE-RUN-EVIDENCE.json` — 16,822 s on
  mps:0 for three seeds: the sizing input for D-12.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **SetFit training server** (`crates/aprender-mcp-setfit-train` plus `crates/aprender-setfit-train-lambda`):
  - MCP Tasks via pmcp core's `TaskStore` (not pmcp-tasks'), the DynamoDB backend, single-flight, dataset-URI
    verification, and presigned links minted when the status is read.
  - Its Lambda worker is replaced here by a SageMaker dispatcher and a job-state finalizer. The memory notes say a
    SetFit run is one 127 s unit, so it needed no state machine. A Laya run is also one unit, just a long one.
- **`pack_laya`:** pack, verify and inspect already exist and decide the gate from the packed bytes.
- **`data.py`:** the refusal rules and their selftest are the case table D-08 must match.

### Established Patterns
- One thin server per algorithm; transport only; numerics live in the library or trainer.
- Fail closed with named refusals (`REFUSED <rule>:`), and exit codes 0 / 3 / 2 for pass / fail / refused.
- Async infra in `deploy-extensions/`, never in the platform-rendered stack. Do not hand-edit the rendered stack.ts
  without pinning logical IDs.
- Device read back from the parameters and recorded; a silent CPU fallback is flagged.
- Evidence JSON per gate run in the phase directory.

### Integration Points
- pmcp.run deploy on ze-kasher-dev with OAuth on.
- ECR plus SageMaker plus EventBridge plus S3 in ze-kasher-dev.
- The fetch recipe feeds `models/decide/<name>/`, which the existing `laya-deploy` consumes.

</code_context>

<specifics>
## Specific Ideas

- The motivation-v1 reproduction (D-16) is the acceptance test of the whole server, not only of the container.
- The trainer already writes the noise record per run. On CUDA its value is the first x86 noise measurement and
  should be reported next to the Phase 12 MPS value.
- Every AWS or pmcp.run call, every paid job, and the quota check need the owner (project constraint).

</specifics>

<deferred>
## Deferred Ideas

- Upload-through-MCP (a presigned-PUT dataset slot, as the SetFit server has): deferred by the owner on 2026-10-28,
  not rejected.
- Server-side preparation of raw partner CSVs with named prepare rules.
- Caller-set epochs or stopping rule.
- Auto-deploy of a GATE PASS artifact to a decide server.
- ECS on Graviton GPU (g5g) as the compute tier: the D-02 fallback, triggered only by evidence.
- CUDA-specific parity bars in `laya-parity-v1`: only by owner decision after a failed verify (D-15).

### Reviewed Todos (not folded)
These matched on keywords only and are outside the server's scope; the owner chose "fold none":
- `aprender-train-gpu-ledger-tests-red.md` — aprender-train's Rust GPU ledger tests; not the torch trainer.
- `qwen35-9b-hybrid-forward-unimplemented.md` — Kev / Qwen3.5-9B; unrelated.
- `spike-laya-calibration-slice-and-temperature-cap.md` — concerns the gate contract the server runs, not the
  server itself.
- `workspace-test-gate-blocked-by-renacer-validate.md` — the macOS workspace build blocker; separate.

</deferred>

---

*Phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out*
*Context gathered: 2026-10-28*
