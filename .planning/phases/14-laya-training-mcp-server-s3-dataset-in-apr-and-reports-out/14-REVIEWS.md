---
phase: 14
reviewers: [codex]
reviewer_instances: [fable]
reviewed_at: 2026-10-29T03:17:26Z
plans_reviewed: [14-01-PLAN.md, 14-02-PLAN.md, 14-03-PLAN.md, 14-04-PLAN.md, 14-05-PLAN.md, 14-06-PLAN.md, 14-07-PLAN.md, 14-08-PLAN.md, 14-09-PLAN.md, 14-10-PLAN.md, 14-11-PLAN.md, 14-12-PLAN.md, 14-13-PLAN.md, 14-14-PLAN.md, 14-15-PLAN.md]
models:
  codex: "gpt-6.1-sol (reasoning=high)"
  fable: "claude-fable-5-1 (reasoning=high)"
model_sources:
  codex: "banner"
  fable: "pinned"
---

# Cross-AI Plan Review — Phase 14

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

> `fable` is a reviewer instance (Fable 5.1) resolved through the `claude` adapter, a different model from the orchestrator. Codex is the non-Anthropic lane. Both lanes ran source-grounded with repo read access, cited `file:line` evidence, and covered all 15 plan ids.

## Codex Review

Review basis: repository source, committed fixtures, and all 15 plans. No files were modified and no AWS or pmcp.run calls were made. Plan filename citations refer to `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/`.

## 14-01

**Summary:** The shared library is a useful foundation, but its proposed interfaces and report mapping contain blockers that would propagate into downstream plans.

**Strengths**

- Reusing the existing task store preserves its guarded terminal transition and read-time link pattern. Those mechanisms exist in `crates/aprender-mcp-setfit-train/src/task_store.rs:552` and `crates/aprender-mcp-setfit-train/src/lib.rs:451`.

**Concerns**

- **HIGH — The lock cannot access its backend through the specified server interface.** `build_server(store, dispatcher, source, version)` receives only the store, while acquisition requires `TaskBackend`. The store’s backend is private and has no public accessor. Evidence: `14-01-PLAN.md:263`; [task_store.rs:383](/Users/guy/Development/machine-learning/aprender/crates/aprender-mcp-setfit-train/src/task_store.rs:383).
- **HIGH — `validate_dataset` cannot validate above-16 datasets as specified.** Its arguments contain only a URI, but `validate_files` also enforces epoch resolution. Passing `None` above 16 causes the trainer’s mandatory-epochs refusal. Evidence: `14-01-PLAN.md:248`, `14-01-PLAN.md:307`; [contract.py:242](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/contract.py:242).
- **HIGH — ECE is copied from the wrong location.** The plan names `calibration.ece_post`; the writer puts ECE under `fine_tuned`, while `calibration` contains temperature and slice metadata. The committed tiny fixture confirms this distinction. Evidence: `14-01-PLAN.md:237`; [train.py:900](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/train.py:900).

**Suggestions**

- Introduce an explicit shared context containing both backend and store.
- Separate dataset inspection from submit-time epoch enforcement.
- Test payload extraction against the existing committed gate report, including nested field locations.

**Risk Assessment: HIGH.** These defects affect compilation, validation, and every terminal status response.

## 14-02

**Summary:** The shared replay table is well motivated, but the equality claim needs a precise input domain and one mutation test is currently unsatisfiable.

**Strengths**

- The cases address real parser differences: duplicate keys, surrogate handling, line splitting, and refusal ordering are visible in `scripts/laya_train/data.py:138`, `data.py:185`, and `data.py:197`.
- Deferring the order-dependent calibration refusal has a concrete basis: whole groups are drawn using a seeded permutation in `scripts/laya_train/data.py:283`.

**Concerns**

- **MEDIUM — P3 will survive the proposed mutation test.** Weakening the replay from exact rule equality to “a refusal occurred” does not change the result when the unmodified validator already returns every expected rule. Ordering cases alone cannot kill that mutation. Evidence: `14-02-PLAN.md:222`.
- **MEDIUM — Reusing `Task::from_slice` introduces another semantic difference.** Rust refuses more than 510 criteria during parsing; Python’s `load_task` has no equivalent maximum. The table does not cover that boundary, despite the “no false refusal” claim. Evidence: [task.rs:138](/Users/guy/Development/machine-learning/aprender/crates/aprender-decide/src/task.rs:138), `scripts/laya_train/data.py:116`.

**Suggestions**

- Test the replay comparator directly with deliberately mismatched expected and actual refusal rules.
- Add criteria-count boundaries and explicitly distinguish transport/serving limits from trainer-validation equality.
- Keep the declared calibration deferral visible in public validation responses.

**Risk Assessment: MEDIUM.** The approach is sound, but its current evidence would overstate equivalence.

## 14-03

**Summary:** This is a careful lock-extension plan with good protection against approving one resolution and installing another.

**Strengths**

- The platform gap is real: `scripts/laya_train/uv.lock:4` supports only Darwin arm64, matching `scripts/laya_train/pyproject.toml:44`.
- Preserving the reviewed candidate byte for byte respects the repository’s numerical-artifact policy in `scripts/laya_train/README.md:30` and `README.md:50`.

**Concerns**

- **MEDIUM — The verification proves Darwin stability, not Linux installability.** Marker inspection and a Darwin freeze comparison do not establish that the Linux wheels install and import successfully. The existing lock’s dependencies themselves contain platform markers, so wheel presence alone is insufficient. Evidence: `scripts/laya_train/uv.lock:25`; `14-03-PLAN.md:158`.
- **LOW — The evidence schema relies heavily on recorded booleans.** `freeze_equal: true` is checked later, but the before/after freeze contents or hashes are not retained in the declared evidence. Evidence: `14-03-PLAN.md:172`.

**Suggestions**

- Add a frozen Linux installation/import check before pushing the training image.
- Record hashes of both freeze outputs and compare dependency edges for the Darwin marker.

**Risk Assessment: MEDIUM.** Package approval and application are controlled well; platform execution remains deferred.

## 14-04

**Summary:** The deployment-unit changes and AWS seams are appropriate, but cancellation and dispatch uncertainty need durable handling.

**Strengths**

- D-22 targets the correct register. The source separately ratchets deployment packages and migration debt in `crates/aprender-core/tests/monorepo_invariants.rs:300` and `monorepo_invariants.rs:448`.
- Reusing the DynamoDB backend preserves conditional insertion and consistent reads: `crates/aprender-setfit-train-lambda/src/dynamodb.rs:101` and `dynamodb.rs:301`.

**Concerns**

- **HIGH — Cancellation can silently fail while training continues.** The sink logs a failed or timed-out `StopExecution` and returns after the task has already become Cancelled. No durable retry is specified; if stopping fails, there may be no abort event until the original execution eventually finishes. Evidence: `14-04-PLAN.md:222`; [task_store.rs:669](/Users/guy/Development/machine-learning/aprender/crates/aprender-mcp-setfit-train/src/task_store.rs:669).
- **HIGH — An ambiguous dispatch error can release the lock around an execution that actually started.** Any `StartExecution` error becomes dispatch failure, and 14-01 releases the lease. A lost response after successful acceptance needs reconciliation, rather than immediate compensation. Evidence: `14-04-PLAN.md:217`, `14-01-PLAN.md:269`.
- **HIGH — Validation is not bound to the bytes SageMaker later reads.** The request computes hashes, but `ExecutionInput` carries only a mutable dataset prefix. The trainer’s change check protects its own local data directory, not the interval between request validation and channel download. Evidence: `14-04-PLAN.md:203`; `scripts/laya_train/train.py:759`, `train.py:925`.

**Suggestions**

- Persist cancellation intent and retry until compute termination is confirmed.
- Reconcile uncertain starts using the deterministic execution name before releasing the lease.
- Bind submission to immutable dataset versions or verify the request’s expected hashes inside the job.

**Risk Assessment: HIGH.** Current failure paths can undermine single-flight and preflight validation guarantees.

## 14-05

**Summary:** The cancelled-result attachment correctly addresses the existing store behavior, but finalization currently happens too early to guarantee complete outputs.

**Strengths**

- The plan correctly recognizes that cancellation stores no result and that `finish` will not attach one afterward. Evidence: `crates/aprender-mcp-setfit-train/src/task_store.rs:659`, `task_store.rs:689`.
- Re-reading the winning result after a completion/cancel race is consistent with the store’s CAS model at `task_store.rs:478`.

**Concerns**

- **HIGH — The finalizer releases single-flight before compute is necessarily finished.** It calls `StopTrainingJob` and releases the lease without waiting for a terminal job state. Its seams also lack an ECS stop/reconciliation path for cancellation during packing. Evidence: `14-05-PLAN.md:114`, `14-05-PLAN.md:130`. AWS documents `.sync` cancellation as [best effort](https://docs.aws.amazon.com/step-functions/latest/dg/connect-to-resource.html).
- **HIGH — “Every report” is not recoverable from the proposed inventory.** The finalizer merges writer manifests, but interrupted training may leave reports without a completed training manifest. An early abort invocation can then freeze an empty or partial manifest before the SIGTERM upload finishes. Evidence: `14-05-PLAN.md:118`; `14-06-PLAN.md:136`, `14-06-PLAN.md:175`.
- **HIGH — Terminal publication precedes durable S3 completion.** A task becomes Completed before verdict, manifest, and retention updates succeed. Recovery depends on later invocations, but there is no explicit durable reconciliation protocol. Evidence: `14-05-PLAN.md:124`.

**Suggestions**

- Treat stop requests, compute termination, output collection, and publication as distinct states.
- Maintain incremental inventories for interrupted runs.
- Add crash tests between the DynamoDB write and each S3/tagging operation, plus cancellation during packing.

**Risk Assessment: HIGH.** Successful terminal records can currently precede complete, retained, downloadable results.

## 14-06

**Summary:** The wrapper has sensible boundaries around the existing trainer, but its interruption guarantees exceed the specified implementation.

**Strengths**

- Keeping production numerics in the existing CLI preserves its three-seed selection and noise lifecycle. Evidence: `scripts/laya_train/train.py:827`, `train.py:886`.
- The offline base check complements the trainer’s existing digest verification in `scripts/laya_train/train.py:262`.

**Concerns**

- **HIGH — Periodic report upload lacks a corresponding recoverable inventory.** The final training manifest is uploaded after the trainer exits, while periodic sync uploads individual files. A hard kill can therefore preserve bytes that the finalizer never exposes. Evidence: `14-06-PLAN.md:136`, `14-06-PLAN.md:175`; the trainer writes useful per-seed outputs before the final report at `scripts/laya_train/train.py:836`.
- **MEDIUM — SIGTERM timing is internally inconsistent.** The test requires completion within ten seconds, but the implementation may first wait twenty seconds for the child, then perform retried uploads. Evidence: `14-06-PLAN.md:166`, `14-06-PLAN.md:174`.
- **LOW — “Every external input pinned” is stronger than the Dockerfile instructions.** Image and ZIP digests are pinned, but apt packages remain resolved from mutable repositories. Evidence: `14-06-PLAN.md:229`.

**Suggestions**

- Upload incremental, hash-bearing inventories alongside periodic files.
- Define an absolute shutdown deadline covering child termination and uploads; test slow and failed transfers.
- Narrow the reproducibility claim or pin the remaining build inputs.

**Risk Assessment: HIGH.** Normal completion is well covered; timeout and hard-kill report delivery remain incomplete.

## 14-07

**Summary:** The arm64 image addresses the packer’s actual runtime requirements, but its skip policy contradicts later waiver acceptance plans.

**Strengths**

- Preserving `/src/contracts` and the manifest-directory path is necessary: `pack_laya` reads contracts through `env!("CARGO_MANIFEST_DIR")` at runtime. Evidence: [pack_laya.rs:62](/Users/guy/Development/machine-learning/aprender/crates/aprender-decide/examples/pack_laya.rs:62).
- Requiring both pack and verify follows the existing eligibility mechanism at `crates/aprender-decide/examples/pack_laya.rs:175`.

**Concerns**

- **HIGH — Waived acceptance is unreachable.** Every trainer `gate_fail` skips packing. However, the trainer deliberately leaves `pass` and its exit status unchanged when attaching a waiver claim; Rust decides whether that failure is waived. Plans 14-14 and 14-15 explicitly expect that path. Evidence: `14-07-PLAN.md:97`; [train.py:926](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/train.py:926); `crates/aprender-decide/src/verify.rs:3528`.
- **MEDIUM — The image smoke lacks its selftest fixtures.** The runtime image copies three Python files, while `run_step --selftest` must read the new golden fixtures. The smoke only specifies mounting `laya_tiny`. Evidence: `14-07-PLAN.md:142`, `14-07-PLAN.md:151`; `14-06-PLAN.md:54`.

**Suggestions**

- Resolve the raw gate verdict versus waived deploy eligibility policy explicitly across all affected plans.
- Copy or mount every selftest dependency at its expected path, and run the smoke with an otherwise empty runtime filesystem.

**Risk Assessment: HIGH.** The normal pack path is credible, but one declared reproduction outcome cannot occur.

## 14-08

**Summary:** The infrastructure assertions are valuable, but the recovery architecture needs stronger delivery guarantees.

**Strengths**

- Keeping async infrastructure separate preserves the existing app’s explicit stack selection and default behavior in `deploy-extensions/bin/app.ts:34`.
- The proposed table schema matches the backend’s owner/task keys and expiry behavior: `deploy-extensions/lib/setfit-training-stack.ts:102`.

**Concerns**

- **HIGH — EventBridge is the sole abort-finalization recovery path.** The plan relies on a status event to attach cancelled results, stop residual compute, and repair tagging. Step Functions status events are delivered on a [best-effort basis](https://docs.aws.amazon.com/step-functions/latest/dg/eventbridge-integration.html); a missing event can leave those promises unfulfilled. Evidence: `14-08-PLAN.md:178`.
- **MEDIUM — Keeping only twenty images can delete a deployed digest.** The rule does not protect images referenced by the current state machine or task definition. Digest pinning prevents substitution, but does not preserve availability. Evidence: `14-08-PLAN.md:120`; [ECR lifecycle behavior](https://docs.aws.amazon.com/AmazonECR/latest/userguide/LifecyclePolicies.html).
- **MEDIUM — Template mutations do not prove operational recovery.** They check resource shape, but not lost abort events, exhausted finalizer retries, or an ECS task remaining alive after cancellation. Evidence: `14-08-PLAN.md:170`.

**Suggestions**

- Add durable reconciliation for cancellation and incomplete finalization, with failure destinations and alarms.
- Protect currently deployed and reproduction-referenced image digests from cleanup.
- Add failure-injection acceptance cases beyond template mutations.

**Risk Assessment: HIGH.** Resource declarations are thorough; guaranteed cleanup and finalization are not yet established.

## 14-09

**Summary:** Centralized owner-run recipes are useful, but account identity, initial bootstrap state, and dataset replacement need clarification.

**Strengths**

- Clean-context builds and recorded image digests support provenance, while the existing repository already supplies account verification machinery in `justfile:1792`.

**Concerns**

- **HIGH — A profile name is not an account identity check.** Restricting the string to `ze-kasher-dev` still permits credentials under that profile to resolve elsewhere. The existing `_laya-account-check` explicitly detects this, but the new operations do not require it. Evidence: `14-09-PLAN.md:23`; [justfile:1811](/Users/guy/Development/machine-learning/aprender/justfile:1811).
- **HIGH — First-deployment output discovery is underspecified.** The live sequence builds images before running `infra outputs`, although image builds need the images stack’s bucket, repositories, and project names. The outputs command describes both stacks, including the trainer stack that cannot yet exist. Evidence: `14-09-PLAN.md:99`, `14-09-PLAN.md:114`; `14-12-PLAN.md:101`.
- **MEDIUM — Restaging can leave a stale optional shift file.** Uploading three files over a prefix that previously contained four does not remove `shift.jsonl`. The trainer will run the shift probe whenever that file exists. Evidence: `14-09-PLAN.md:141`; `scripts/laya_train/train.py:753`.

**Suggestions**

- Reuse `_laya-account-check` before authorized live operations.
- Add an explicit images-only outputs/bootstrap step.
- Make dataset prefixes immutable or refuse/reconcile existing contents, including optional-file removal.

**Risk Assessment: HIGH.** These gaps affect deployment correctness and which dataset the job actually consumes.

## 14-10

**Summary:** Grant-after-auth and token handling are good choices, but the auth proof must be bound to the function receiving the grant.

**Strengths**

- The existing auth helper checks both `tools/list` and `tools/call`, rather than relying on unauthenticated `initialize`: `justfile:4378` and `justfile:4389`.
- Reusing `m2m_token.py` retains its protected file handling and environment-only token transfer: `scripts/laya_deploy/m2m_token.py:12`, `m2m_token.py:23`.

**Concerns**

- **HIGH — A 401 from an unrelated endpoint can authorize the training function’s grant.** `grant ENV PROFILE URL` checks the supplied URL and then grants the fixed training role. The existing helper validates URL syntax, not its relationship to that deployment. Evidence: `14-10-PLAN.md:156`; [justfile:4345](/Users/guy/Development/machine-learning/aprender/justfile:4345).
- **MEDIUM — The authenticated HTTP client omits redirect refusal.** The existing token helper explicitly blocks redirects because credentials could be forwarded. The new bearer client should retain that property rather than merely requiring an initial HTTPS URL. Evidence: `14-10-PLAN.md:161`; `scripts/laya_deploy/m2m_token.py:15`.
- **MEDIUM — Redeploy revocation needs fail-closed error classification.** An absent policy and an inability to inspect/delete it must not both permit an OAuth-off redeploy. Evidence: `14-10-PLAN.md:119`.

**Suggestions**

- Resolve the endpoint from verified deployment state and bind it to the function before checking OAuth or granting.
- Add redirect tests and explicit policy-inspection/deletion failure cases.

**Risk Assessment: HIGH.** An endpoint mix-up could defeat the intended protection against unauthenticated spend.

## 14-11

**Summary:** The staging-and-verify hand-off is appropriate, but the local path mapping needs a single explicit contract.

**Strengths**

- Fetching the complete checkpoint and data directories is necessary: `verify_path` reconstructs `PackInputs` from both. Evidence: [verify.rs:3701](/Users/guy/Development/machine-learning/aprender/crates/aprender-decide/src/verify.rs:3701).
- Keeping deployment manual preserves the existing `laya-verify` interface in `justfile:1532`.

**Concerns**

- **MEDIUM — The mapping contradicts the promised root layout.** “Everything else → logs/” includes `verdict.json`, while the promised result and later smoke require root-level `verdict.json` and `manifest.json`. Their special handling is not specified consistently. Evidence: `14-11-PLAN.md:101`, `14-11-PLAN.md:103`; `14-14-PLAN.md:106`.
- **MEDIUM — Safe individual paths do not establish a valid file tree.** Duplicate destinations and file/directory conflicts such as `run/a` versus `run/a/b` need rejection before downloads begin. The current negative cases cover traversal but not these conflicts. Evidence: `14-11-PLAN.md:89`.

**Suggestions**

- Define explicit mappings for both metadata files, run/data trees, artifact, and logs.
- Validate the entire destination tree before writing, and test it through the actual fetch consumer used by 14-14.

**Risk Assessment: MEDIUM.** The integrity design is sound; layout inconsistencies can break the hand-off.

## 14-12

**Summary:** The owner-gated probe sequence is sensible, but the evidence requirements currently contradict themselves and the probe does not establish production training capacity.

**Strengths**

- Measuring CUDA execution before trusting a gate run addresses a real distinction between requesting a device and observing the parameters’ device. The trainer records the latter in `scripts/laya_train/train.py:906`.

**Concerns**

- **HIGH — Required image evidence fails the account-ID prohibition.** `images.train/pack.repo_uri` must contain real ECR repository URIs, while the final verifier rejects every twelve-digit account ID anywhere in the evidence. The later smoke plan itself recognizes that repository URIs carry the account ID. Evidence: `14-12-PLAN.md:184`, `14-12-PLAN.md:213`; `14-14-PLAN.md:107`.
- **MEDIUM — Forward inference plus tiny-model training does not prove real-model training fits.** Production training allocates AdamW state and performs backward passes; the probe performs those operations only on the tiny fixture. Evidence: `14-06-PLAN.md:183`; [train.py:330](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/train.py:330), `train.py:362`.

**Suggestions**

- Store repository identifiers in gitignored state and retain digest/commit/build references in committed evidence.
- Add one bounded real-model forward/backward/optimizer step and record GPU and host-memory peaks before authorizing the full smoke.

**Risk Assessment: HIGH.** The evidence verifier cannot pass as written, and capacity risk remains deferred to a paid production run.

## 14-13

**Summary:** This is a useful live acceptance stage, but some claimed proofs are stronger than the observations and client capabilities permit.

**Strengths**

- Testing the same authenticated validation call before and after the grant isolates the grant’s effect. The underlying auth helper checks real HTTP refusal codes in `justfile:4398`.

**Concerns**

- **HIGH — The expected above-16 validation success is blocked by the current shared API.** Motivation-v1 validation supplies no epochs, but the proposed shared validator applies the mandatory-epochs rule. Evidence: `14-13-PLAN.md:172`; `scripts/laya_train/contract.py:242`.
- **MEDIUM — S3 denial does not prove absence of Step Functions or DynamoDB permissions.** Those are independent IAM statements even in the existing scaffold. Observing `dataset-access` proves the S3 call failed, not that no other policy grants execution rights. Evidence: `14-13-PLAN.md:28`; `deploy-extensions/lib/setfit-training-stack.ts:255`, `setfit-training-stack.ts:266`.
- **MEDIUM — The client cannot send the required unknown argument.** Its declared train syntax accepts only URI, name, and optional epochs; no facility sends `seeds`. Also, an empty execution list does not prove no task was minted and compensated. Evidence: `14-13-PLAN.md:175`; `14-10-PLAN.md:161`; `crates/aprender-mcp-setfit-train/src/lib.rs:399`.

**Suggestions**

- Fix the inspection/submission distinction before this stage.
- Inspect or simulate the request role’s relevant permissions.
- Provide a protected negative-test RPC path and compare task-store state as well as executions.

**Risk Assessment: HIGH.** Several mandatory live assertions are currently either unreachable or insufficiently demonstrated.

## 14-14

**Summary:** The smoke covers valuable external behavior, but it can pass without exercising successful packing and does not fully test cancellation safety.

**Strengths**

- Declaring the reproduction band and retry policy before measurement is justified by the recorded seed spread: Phase 12 seed 17 failed ECE while the shipped seed passed. Evidence: `.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-GATE-RUN-EVIDENCE.json:69` and `:82`.
- Local verification of fetched bytes uses the real existing eligibility command in `justfile:1532`.

**Concerns**

- **HIGH — The cancel test permits lock release while the job is still Stopping.** It does not attempt a replacement submission during that interval or cancel during packing. Consequently, it can pass while the implementation violates deployment-wide single-flight. Evidence: `14-14-PLAN.md:146`, `14-14-PLAN.md:157`; the store itself marks cancellation before stopping work at `task_store.rs:669`.
- **MEDIUM — A gate-failed smoke can satisfy acceptance without any production pack/verify execution.** The smoke accepts `gate_failed`, while 14-07 skips that outcome. Evidence: `14-14-PLAN.md:115`; `14-07-PLAN.md:97`.
- **MEDIUM — The declaration check proves only that some version was committed.** `git log` can succeed while the declaration’s working copy differs from the committed bytes. Evidence: `14-14-PLAN.md:196`.

**Suggestions**

- Require a completed production pack/verify path before treating the pipeline smoke as complete.
- Test cancellation during both stages and reject new work until compute is terminal.
- Record the declaration blob/commit and require byte equality before paid execution.

**Risk Assessment: HIGH.** The current tests can pass without proving the strongest lifecycle guarantees.

## 14-15

**Summary:** The reproduction record has the right intent, but its extraction and verdict verification need correction before supporting a trust claim.

**Strengths**

- Reading the fetched run’s files and re-verifying the artifact preserves traceability to the real verifier rather than trusting a copied status alone. Evidence: `crates/aprender-decide/examples/pack_laya.rs:175`.
- The reference recipe identity is genuinely device-independent: `scripts/laya_train/contract.py:292` lists the recipe fields and excludes device.

**Concerns**

- **HIGH — Exact input-map equality rejects real reports.** The verifier accepts only a three-key dictionary, but the actual gate report also includes `base_model` and `tokenizer_json`. Directly copying the required source field therefore fails acceptance. Evidence: `14-15-PLAN.md:102`; [train.py:910](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/train.py:910). The plan also repeats the incorrect `calibration.ece_post` location at `14-15-PLAN.md:84`.
- **HIGH — The reproduction verifier allows vacuous success.** `all(...values())` accepts empty `must` and `band` dictionaries. It neither requires the declared clause names nor recomputes band membership from recorded metrics. Evidence: `14-15-PLAN.md:146`.
- **MEDIUM — Retry enforcement is incomplete.** The action implements only attempt 2 despite allowing owner-selected caps above two; the verifier checks count but not whether a preceding defect prohibited retry. Evidence: `14-15-PLAN.md:129`, `14-15-PLAN.md:144`.

**Suggestions**

- Compare the required dataset-hash subset while retaining all provenance fields; read ECE from `fine_tuned`.
- Implement a pure reproduction evaluator with mandatory clause keys and tests for empty clauses, altered metrics, waived outcomes, and forbidden retries.
- Bind execution to the exact committed declaration and either support the declared cap or restrict it explicitly.

**Risk Assessment: HIGH.** Valid runs can fail verification, while malformed evidence can falsely satisfy it.

## Overall assessment

**Coverage: 15/15 plans reviewed. Recommendation: request changes before execution. Overall risk: HIGH.**

The architecture is credible, and the plans show strong attention to existing contracts, owner authorization, and native verification. The principal weaknesses are integration contracts and failure recovery, rather than the choice of compute services.

The highest-priority corrections are:

1. Fix backend access, validation-versus-epoch semantics, and real report-field mappings.
2. Make cancellation, uncertain dispatch, and finalization durably recoverable; retain single-flight until compute terminates.
3. Preserve discoverable inventories for interrupted runs and define metadata publication consistently.
4. Reconcile gate failure, waiver acceptance, artifact publication, and reproduction policy.
5. Bind grants and AWS operations to verified deployment/account identities.
6. Replace contradictory or vacuous evidence checks with executable comparisons against actual writer outputs.

A single offline integration fixture exercising **request → wrapper outputs → pack outcome → finalizer → fetch → reproduction evaluator** would catch several of these gaps before the live plans spend money.

---

## Claude Review (fable)

### Phase 14 Implementation Plan Review

**Verification scope.** I read the SetFit scaffold (`task_store.rs`, `dynamodb.rs`, `lib.rs`, `bootstrap.rs`), the trainer (`train.py`, `data.py`, `contract.py`, `pyproject.toml`, `uv.lock`), `pack_laya.rs`, the FALSIFY-MONO-011 test, the dup-bin allowlist and guard, the justfile recipes the plans extend, the CDK app and the installed `aws-cdk-lib`, pmcp 2.19.3's `tasks/result` path, the Phase 10/12 evidence files, the three local datasets, the local HF cache, and the installed toolchain. I ran `data.py --selftest` (117 `ok` lines, `DATA SELFTEST OK`) to confirm the 14-02 baseline. No AWS call was made and no file was modified.

**Headline.** The plans are unusually well grounded: every RESEARCH pitfall I could check against source is reflected correctly (register 10 to 11 not 27, the epochs rule, `pack_laya`'s compile-time contracts path, `put_new` blocking on an expired row, pmcp serving `tasks/result` store-first). The design that matters most, one guarded terminal writer plus a CAS lease with no backend TTL, is consistent with what the scaffold actually does. The defects I found are concrete and fixable before execution: a repo-root Docker build context over a 26 GB `models/` tree with no `.dockerignore`, a clean-tree check that will refuse in the current working tree, an undefined state-dir parameter between 14-09 and 14-12, an unstated `s3:PutObjectTagging` grant, and a cancel-sink runtime pattern that needs one explicit test.

---

## 14-01

**Summary.** Creates the library crate with the tool surface, the shared layout vocabulary, the single-flight lease, the one status payload and the pure finalize decision, proven on `InMemoryTaskBackend`. The design decisions are traceable to the scaffold's real behavior rather than to the ROADMAP's summary of it, which is the plan's main strength.

**Strengths.**
- The lock design is correct against the backend as written. `TaskBackend::get` hides expired rows (`task_store.rs:178-180`, `dynamodb.rs:313`) while the DynamoDB `put_new` condition is `attribute_not_exists(#owner) AND attribute_not_exists(#tid)` (`dynamodb.rs:101`), so an expired lock row would block `put_new` forever. The plan's "lock record carries NO backend TTL" rule is exactly the escape, and the backend omits `expires_at` when `None` (`dynamodb.rs:170-172`, asserted at `:457-458`), so DynamoDB TTL can never delete the lock.
- The cancelled-task rule is derived from real code: `cancel` CASes Working to Cancelled and never sets `result` (`task_store.rs:659-677`), `finish` returns early on a terminal record (`:552-568`), `set_result` overwrites unconditionally (`:689-701`), and pmcp 2.19.3 serves `tasks/result` store-first (`pmcp-2.19.3/src/server/task_dispatch.rs:1631-1643`). `attach_result_if_absent` is the only safe primitive, and its `version + 1` matches `cas_update` (`task_store.rs:478-492`).
- The epochs rule mirrors `contract.resolve_epochs` (`contract.py:223-248`) and the three numbers exist under `recipe` (`laya-finetune-gate-v1.yaml:280-282`).
- Terminal-without-result status already has a precedent in the scaffold (`crates/aprender-mcp-setfit-train/src/lib.rs` status(), the "Terminal with no readable result" branch), so the plan extends rather than invents.
- README gate is handled in the creating commit (`readme_contract.rs:265, 290`, CI fragment 040).

**Concerns.**
- MEDIUM: the contract is embedded with `include_str!`, so the request Lambda carries a build-time copy of `laya-finetune-gate-v1.yaml`. D-06's wording is "at run time". A contract amendment after deploy leaves the pre-filter judging with the old numbers until a redeploy, and nothing records which contract bytes judged a run. The pack image (14-07) also copies the contracts at build time. This is acceptable if recorded, but the manifest only records image digests.
- MEDIUM: the request path normalizes and hashes every train and eval row for `eval-train-overlap` within the API Gateway 30 s window. With `MAX_ROWS_PER_FILE` 200,000 and 32 MiB files, Rust should finish in seconds, but no test bounds it. A 1 GB Lambda cold start plus a 64 MiB total fetch is the real budget.
- LOW: the `ExecutionInput` carries `owner` (the Cognito sub) into Step Functions history. The scaffold does the same in its dispatch payload, and the envelope rule is respected, but it should be stated as deliberate since `task_store.rs:60-66` quotes the retention concern.
- LOW: step (e) peeks the lock before mint and (g) acquires after; a race between two requests both passing (e) is resolved by `put_new`/`cas` Conflict and compensation. Fine, but the "mints nothing on the pre-check path" test only covers the non-racing case.

**Suggestions.**
- Record `contract_sha256` for the embedded gate contract in `Manifest` beside `images`, and have the pack step record the two contract hashes it read; then a run names its policy as well as its images.
- Add a `payload_size`-style timing test: 200,000 synthetic rows through `validate_files` under a stated bound.
- Document the `owner` field in `ExecutionInput`'s doc comment as an identifier, not a secret, with the retention note.

**Risk: MEDIUM.** Large foundational surface, but every mechanism it relies on exists and behaves as the plan states.

---

## 14-02

**Summary.** Factors the trainer's step-1 checks into `validate_data_dir`, writes one generated case table, and replays it through Python and Rust with six planted mutations. This is the right shape for D-08 and the one place where a refactor touches the trainer.

**Strengths.**
- The step-1 order the plan cites matches `train.py:745-770` exactly (task, train, eval, overlap, optional shift, hashes, `calibration_split`, `shots_per_class = max(...)` at `:767`, `resolve_epochs`). Moving that block into `data.py` and calling it is a pure extraction.
- The `train-class-too-small` analysis is right: `calibration_split` refuses on `took < need or took >= counts[c]` (`data.py:298-301`), so the order-independent case is exactly `counts[c] <= max(min_per_class, ceil(fraction * counts[c]))`, and the order-dependent overshoot comes from multi-row text groups drawn by `rng.permutation` (`:292-297`). Deferring only that rule is well bounded.
- Baseline confirmed: 117 `ok` lines and `DATA SELFTEST OK` at HEAD, so the "117 prior cases plus one per table case" assertion is checkable.
- The mutation step honors CLAUDE.md's "re-mutate in the new scope" and includes a replay-weakening mutation (P3), which is the one most case tables forget.

**Concerns.**
- MEDIUM: `validate_data_dir` imports `contract` and calls `contract.declared_seed()` and `contract.constant(...)`. `data.py` today is torch-free and runs in CI's torch-free selftest set (`justfile:1446-1449` comment). `contract.py` reads YAML from `REPO/contracts`, so this stays torch-free, but the plan should assert the CI selftest invocation still works from the repo root with no venv beyond pyyaml.
- MEDIUM: the generator must emit invalid UTF-8 and lone surrogates through base64 and must be deterministic; the table is compared byte for byte. Any `dict` ordering or float formatting drift between Python versions (system 3.14 for `python3 -I`, project 3.13 for `uv run`) would fail `cmp`. The plan runs the generator under `python3 -I` (3.14) in both verifies, so it is internally consistent, but say so.
- LOW: the Rust replay uses `include_str!("../../../scripts/laya_train/data_cases.json")`; the path resolves correctly from `crates/aprender-mcp-laya-train/src/`. A 45-plus case table with base64 bodies is small, fine.

**Suggestions.**
- Pin the generator to `python3 -I` in a comment and have the Python replay recompute the table in-process and compare to the committed file, so a Python-version drift shows as a named case rather than a `cmp` failure.
- Add one table case where `shift.jsonl` is present and `epochs` is required, so the D-21 path and the shift path are exercised together (14-14's smoke dataset is exactly that shape).

**Risk: MEDIUM.** The trainer refactor is mechanical and guarded by `laya-train-selftest`, but it is the only change to production training code in the phase.

---

## 14-03

**Summary.** Adds a Linux x86_64 resolution to the trainer lock in a scratch copy, puts every new package to the owner, then installs the reviewed file by copy and checks it with `uv lock --check`. The two-step "resolve, review, copy" is the correct way to honor the Phase 8 human-verification rule.

**Strengths.**
- The premise is verified: `pyproject.toml:44` restricts environments to darwin arm64, `uv.lock` has zero `nvidia` entries, and `README.md:47-48` carries the "nobody verified" sentence the plan replaces.
- Installing the exact reviewed bytes and refusing any writing `uv lock` after approval closes the window where a wheel published between review and apply would change the set.
- The darwin-unchanged proof is per package and per wheel hash, plus a `uv pip freeze` equality on the real venv.

**Concerns.**
- MEDIUM: `uv lock --check` needs to validate the lock against the pyproject and may consult the network or the uv cache for the `laya` git source and index metadata. If the owner's review takes days and the cache is cold, `--check` could need network at Task 3 time. Not a correctness problem, but the task says "stop for the owner" on non-zero, so a transient network failure reads as a resolution mismatch.
- LOW: the candidate's `cuda-toolkit`, `nvidia-*-cu13` and `triton` set is several GB of wheels. 14-06's image size follows from this choice (see 14-06). Nothing to change here, but the owner should see the size at the checkpoint.
- LOW: the plan asserts `uv_version` starts with `uv 0.9.5`; the installed uv is 0.9.5 (checked). If the owner's reply lands after a uv upgrade, Task 3 still copies the candidate, so the pin is on the file, not the tool. Good, but the evidence's `uv_version` would then be stale for Task 3; record both.

**Suggestions.**
- Run `uv lock --check` once in Task 1 on the scratch copy immediately after resolution, so Task 3's `--check` has a known-good reference and a failure there is attributable to the pyproject edit, not to resolution.
- Add the summed wheel size of `new_packages` to the evidence for the owner's checkpoint.

**Risk: LOW to MEDIUM.** Offline, reversible, owner-gated; the only live dependency is PyPI availability at two points.

---

## 14-04

**Summary.** Creates the Lambda crate (request `bootstrap`, S3 dataset source, Step Functions dispatcher, cancel sink, presign guard) and grows the deployment-unit register 10 to 11 in the same commit as the bin. The register handling is exactly right; the cancel sink needs one more explicit proof.

**Strengths.**
- Register facts verified: `deployment_unit_bins` has ten entries (`monorepo_invariants.rs:312-323`), `DEPLOYMENT_UNIT_BASELINE = 10` at `:448` with the 9 to 10 note above it, `ALLOWLIST_BASELINE = 27` at `:420`, and the publishability check (`:346-372`) requires `publish = false` for register entries. The test counts packages, so the finalizer's second bin (14-05) costs nothing.
- The dup-bin facts are verified: five `bootstrap` claimants at `scripts/duplicate_bin_names_allowlist.txt:16`, the row expectation `declared by 5 packages` at `justfile:5592`, and the guard's PASS and self-test lines at `check_duplicate_bin_names.sh:167, 141`.
- Creating the crate lib-only first so no intermediate commit leaves FALSIFY-MONO-011 red is careful and correct.
- `DynamoDbTaskBackend::new(client, table)` is public and table-agnostic (`dynamodb.rs:234`), while `from_env` (`:246`) is SetFit-wired; the plan uses the right one.
- The "Cargo.lock only grew" proof against `git show HEAD:Cargo.lock` is a good invariant for an SDK addition on rustc 1.93.

**Concerns.**
- MEDIUM: `SfnCancelSink::cancel` is synchronous (`CancelSink::cancel(&self, task_id)` at `task_store.rs:376-379`) and is called from inside the async `cancel` (`:672-674`). The plan spawns a `std::thread` with its own current-thread runtime and joins it with a 10 s bound. That works only if the new runtime has timers enabled (`enable_all`) and if the aws-sdk client, built on the main runtime, is callable from another runtime. In practice the smithy hyper connector spawns on the current runtime at call time, so it works, but a current-thread runtime built without `enable_time` panics on the SDK's timeouts. The `cancel_sink` tests use a fake stopper, so this is never exercised against the real client shape.
- MEDIUM: the request function's main runtime is blocked for up to 10 s during the join. Under the 30 s API Gateway cap that is tolerable, but `tasks/cancel` is the one call a client may retry aggressively.
- LOW: depending on `aprender-setfit-train-lambda` as a library pulls `tar`, `flate2` and `aws-sdk-lambda` into the new binary. The glibc floor check covers the linking consequence; size is checked at 100 MB.

**Suggestions.**
- Add a `cancel_sink_real_runtime` test that builds an actual `aws_sdk_sfn::Client` from a static credentials provider pointed at an unreachable loopback endpoint, calls `cancel` from inside a multi-thread tokio runtime, and asserts it returns with the logged timeout or connection error and no panic. This proves the cross-runtime pattern once.
- Build the inner runtime with `tokio::runtime::Builder::new_current_thread().enable_all()` and say so in the plan text.
- State the join bound relative to the 30 s request cap in a code comment, so the two numbers move together.

**Risk: MEDIUM.** Register and dispatcher are low risk; the cancel sink is the one unproven mechanism.

---

## 14-05

**Summary.** Builds the finalizer: one idempotent writer for the terminal record, `verdict.json`, `manifest.json`, retention tags and lock release, on every path including client cancel and foreign or duplicate events. This is the plan that makes D-18 and D-19 true.

**Strengths.**
- "Record before S3" plus "first writer wins" is consistent with `finish`'s no-op on terminal (`task_store.rs:558-561`) and with pmcp serving the stored result (`task_dispatch.rs:1634-1640`). The three-way decision (stored result, Working, terminal-without-result) covers every record shape the scaffold can leave.
- The `fin_cancel_race` test, where a cancel lands between the finalizer's read and its `finish`, is the one most designs miss; the re-read-after-finish rule handles it.
- Tagging failure after record and lock work, returning Err so the execution fails and EventBridge retries, keeps retries idempotent.
- The `StopTrainingJob` fallback makes cancel independent of RESEARCH A1.

**Concerns.**
- MEDIUM: the finalizer's `TrainingJobs` seam uses `aws-sdk-sagemaker`, a very large generated crate. Compile time for the lambda crate and the aarch64 binary size both grow materially; the plan records size but sets no bound. Not a correctness issue.
- MEDIUM: the EventBridge path reads the `ExecutionInput` via `DescribeExecution`. For a state-machine `Finalize` payload the input comes from `$$.Execution.Input`. The plan validates `run_prefix == run_prefix(name, task_id)` on both, good, but nothing checks that the `owner` in the input matches the task record's owner before `finish`. Since `finish(task_id, owner, ...)` is owner-keyed, a wrong owner answers NotFound and the plan then treats NotFound as "expired, not fatal", writing `verdict.json` and releasing the lock for a task that is still Working. The only writer of execution inputs is the request function, so this is defense in depth, but the NotFound branch conflates two cases.
- LOW: `fin_record_expired` relies on `TASK_TTL_MS` 72 h and the 10 h machine timeout; consistent.

**Suggestions.**
- Split the NotFound handling: before treating the record as expired, `get` the record under the lock holder's owner from the lock envelope (the lock stores `holder`); if a live Working record exists under a different owner, log `Integrity` and return Err rather than writing.
- Add a size budget for the finalizer binary in the summary and consider feature-gating `aws-sdk-sagemaker` to the two operations (the SDK crate supports no feature trimming, so the alternative is a tiny signed REST call; note the trade-off rather than act on it).

**Risk: MEDIUM.** Tested offline on every enumerated path; the remaining risk is the AWS behaviors (duplicate events, abort propagation) that 14-14 observes.

---

## 14-06

**Summary.** Writes the GPU container's one program (`run_step.py` train, probe, probe-sigterm), deterministic base staging with an offline-load proof, and `Dockerfile.train`. The container contract is thorough. The local verify command has a build-context hazard that must be fixed before execution.

**Strengths.**
- The exit-code mapping is correct against `train.py:93` (`0, 2, 3`), and the CPU-fallback kill keys on the real `DEVICE seed=... used=...` line (`train.py:314`) which prints before any training step in `train_seed`.
- Base staging matches the verified RESEARCH experiment: the local cache has `blobs/`, `snapshots/55cf4c4e.../` and `trees/` (checked), and the contract pin is at `laya-finetune-gate-v1.yaml:353-355`.
- Probe mode measures exactly what RESEARCH Open Question 2 and A6 need, and the compat switch is decided by the host driver and recorded rather than forced.
- `outcome.json` is uploaded last as the completion marker, and small-file periodic sync bounds loss on a hard kill.

**Concerns.**
- HIGH: Task 3's verify runs `docker build --check -f scripts/laya_train/container/Dockerfile.train .` with the repository root as context. There is no `.dockerignore` at the root (checked) and `models/` is 26 GB. Even in check mode BuildKit resolves the context; a real build with this command would ship 26 GB to the daemon. The image recipe (14-09) assembles the context from `context-paths.txt`, so production is safe, but the local lint command as written is not.
- MEDIUM: the image carries both the `nvidia/cuda:13.0.x-base` layers and the PyPI `nvidia-*-cu13` wheels that torch 2.14 bundles. The CUDA runtime is present twice; only the compat libraries are needed from the base image. Image size drives SageMaker's pull time (billable) on every job and CodeBuild time on every build.
- MEDIUM: `run_step.py` uploads with `aws s3api put-object --tagging laya-retention=expire`. That header requires `s3:PutObjectTagging` on the SageMaker execution role, not only `s3:PutObject`. 14-08's truths say "writes runs/*" without naming the tagging action (see 14-08).
- LOW: the SIGTERM handler waits 20 s for the trainer then uploads. If SageMaker's grace is 120 s this is fine; if the probe measures under 20 s the design relies on the 300 s sync. The plan records this as a finding, which is the right posture.
- LOW: probe mode passes `--stopping fixed_epochs --epochs 1 --variant synthetic-fixture --base ... --base-sha256 ...`; these flags exist (`train.py:724-734`) and `resolve_epochs` accepts epochs in [0, 12] for the fixture variant (`contract.py:235-238`).

**Suggestions.**
- Change the lint verify to assemble the same subset tree the smoke already builds (`context-paths.txt` into a temp dir) and run `docker build --check` with that directory as context; or add a root `.dockerignore` that excludes `models/`, `data/`, `target/`, `.planning/`, `.claude/` as part of this plan's `files_modified`.
- Consider `FROM ubuntu:24.04` plus the `cuda-compat-13-0` apt package only, since the torch wheels bring the runtime; record the resulting image size in the summary either way.
- Name `s3:PutObjectTagging` in this plan's threat register row for uploads so 14-08 cannot miss it.

**Risk: MEDIUM.** Offline-proven and carefully specified; the context hazard is a one-line fix and the CUDA unknown is deliberately deferred to 14-12.

---

## 14-07

**Summary.** Adds `run_step.py pack` and the linux/arm64 pack image that builds `pack_laya` as an example at `/src` so its compile-time contracts path resolves. The decision to keep it an example and fix the path is correct and verified.

**Strengths.**
- `pack_laya.rs:62-67` joins `env!("CARGO_MANIFEST_DIR")` with `../../contracts` at run time and reads `laya-finetune-gate-v1.yaml` and `laya-parity-v1.yaml` (`:73-74`). Building at `/src` makes that `/src/contracts`; the plan creates `/src/crates/aprender-decide` so `..` resolves. Correct.
- The fixture sha is real: `crates/aprender-decide/tests/fixtures/laya_tiny.apr.sha256` holds `37d65159...`. The smoke's "verify refuses the synthetic artifact with exit 2" matches the module doc (`pack_laya.rs:33-34`).
- The five refusal cases that block an `.apr` upload (pack 3, pack 2, verify 2, not eligible, sha mismatch) close every path RESEARCH named for D-15.
- Child environment stripped to PATH, HOME, LANG, TMPDIR enforces "the policy is not an argument" at the process boundary.

**Concerns.**
- MEDIUM: the pack image only copies two contracts plus `decide-apr-v1.yaml`. `pack_laya` reads exactly two by name, but `aprender_decide::verify` may read other contract files through the library (the plan says "if the smoke shows another contract file is read, add it"). The smoke runs `pack-fixture`, `inspect` and `verify` on the tiny fixture, which should exercise every read; acceptable.
- MEDIUM: `cargo build --release --locked -p aprender-decide --example pack_laya` loads the whole workspace. On an ARM CodeBuild LARGE this is likely 15 to 30 minutes per image build, and the local smoke is budgeted at 10 to 30 minutes. Fine, but every pack-image rebuild for a contract change pays it.
- LOW: the pack task downloads the checkpoint and the base (about 1.7 GB) and then runs verify. Phase 10's wall note (`10-GATE-RUN-EVIDENCE.json:211`) gives 21 plus 22 minutes on Apple silicon for 2,129 eval rows; a Graviton Fargate task is slower per core. The 3 h `taskTimeout` in 14-08 holds for the phase's eval sizes (100 to a few hundred rows), but a Phase 10-sized eval set is near the budget.

**Suggestions.**
- Record pack and verify wall time per run in `pack-outcome.json` (two fields) so the Fargate budget is measured by the smoke, not assumed.
- Cache the cargo build stage in CodeBuild (ECR-backed BuildKit cache) so contract-only changes rebuild in minutes.

**Risk: MEDIUM.** Mechanism verified; the remaining risk is build and verify wall time on Fargate, which the smoke will measure.

---

## 14-08

**Summary.** Declares both CDK stacks (images; trainer with bucket, table, state machine, Fargate pack task, finalizer, abort rule, post-401 request policy) with a template check script and a 21-case mutation table. It is the right place for every spend limit to become machine-checked.

**Strengths.**
- The installed `aws-cdk-lib` is 2.268.0 and exposes everything the plan uses: `SageMakerCreateTrainingJob.environment` (`create-training-job.d.ts:81`), `S3Location.fromJsonExpression` (`base-types.d.ts:268`), `taskTimeout` (`task-base.d.ts:31`), `ephemeralStorageGiB` (`fargate-task-definition.d.ts:65`), `runtimePlatform` and lifecycle `tagFilters` (both present).
- The SetFit stack's ListBucket condition pattern (`setfit-training-stack.ts:278-287`) and `RequestLambdaPolicy` output (`:247`) are reused, and the grant-after-401 rationale is already documented at `deploy-extensions/README.md:117-129`.
- `app.ts:34-39` is the exact guard the plan extends, with `training` kept as default.
- Deriving the state machine ARN with `Stack.formatArn` from a fixed name avoids the Lambda-to-machine cycle.

**Concerns.**
- MEDIUM: the SageMaker execution role must hold `s3:PutObjectTagging` on `runs/*` for `run_step`'s tagged uploads, and the Fargate task role the same. The truths list only "writes runs/*". The check script's mutation table has no case for a missing `PutObjectTagging`, so a least-privilege pass could silently drop it and every upload would fail with AccessDenied at the first object.
- MEDIUM: the `data` channel is described as `s3://<bucket>/<$.dataset_prefix>`. `ExecutionInput.dataset_prefix` is a prefix, not a URI, so the state machine must build the URI with `JsonPath.format` (an intrinsic), and the channel must be `S3DataType.S3_PREFIX`. The plan does not say which; the check script should assert the rendered `S3Uri` shape.
- MEDIUM: the request policy grants `s3:GetObject` on `runs/*`, but a presigned GET is signed by the request function's role; its session lifetime bounds the link (RESEARCH A9). `DOWNLOAD_URL_TTL_SECS` is 3,600; Lambda role sessions are typically long enough, but the 0.84 GB checkpoint link over a slow client could outlive the signing session. Record the session duration once in 14-13.
- LOW: `removalPolicy RETAIN in every env` contradicts the SetFit stack's env-dependent policy (`:95`); that is deliberate here and stated.

**Suggestions.**
- Add mutation cases: SageMaker role without `s3:PutObjectTagging`; Fargate task role without it; a `data` channel whose `S3Uri` is not `States.Format`-built from `dataset_prefix`.
- Add to the check script an assertion that the `data` channel `S3DataType` is `S3Prefix` and the `base` channel points at exactly `base/laya-55cf4c4e/`.

**Risk: MEDIUM.** Synth-only and well asserted; the two IAM and ASL details above are the kind of thing the mutation table exists to catch, so add them to it.

---

## 14-09

**Summary.** Gives the owner one guarded, DRY_RUN-provable recipe per AWS action (images, infra, staging, probes). The guard design is sound. The clean-tree check for the pack image is stricter than the working tree can satisfy today.

**Strengths.**
- Profile and env refusals before any call, exact-name `LAYA_TRAINER_APPROVED_SPEND`, and `cdk diff` in the same invocation as `deploy` all close real spend holes.
- `--expect motivation-v1` binds the staged dataset to Phase 12's `inputs_sha256` (`12-GATE-RUN-EVIDENCE.json:36-40`), and I confirmed the local files exist under `data/decide/motivation-v1/`.
- State under gitignored `/models` (`.gitignore:62-63`) with a `git check-ignore` assertion keeps account ids out of the tree.

**Concerns.**
- MEDIUM: `image pack` refuses when `git diff --quiet HEAD -- <paths>` fails, and for the pack image the paths are every tracked file. At HEAD the working tree already has `.planning/config.json` and `.planning/state.json` modified (git status at review time). GSD execution keeps those files dirty between commits, so `laya-trainer-image pack` will refuse during 14-12 unless the executor commits planning state first or the context excludes `.planning/`.
- MEDIUM: 14-12 Task 1 relies on "the script's state-dir parameter" to generate placeholder outputs in a temp dir. This plan defines the state dir as `models/laya-trainer/` with no override. The interface is missing on this side.
- MEDIUM: `image` polls `batch-get-builds` for up to 90 minutes. The executor's Bash tool caps a foreground command at 10 minutes. 14-07 explicitly uses `run_in_background` for its long verify; this plan and 14-12 do not say so.
- LOW: the pack-image zip is the whole index (about 386 MB per RESEARCH). The cargo build only needs `Cargo.toml`, `Cargo.lock`, `crates/`, `contracts/`, `rust-toolchain.toml` and whatever `build.rs` files read. Shipping `.planning/`, `docs/`, and scripts adds upload time and widens the clean-tree check for no benefit.

**Suggestions.**
- Give the pack image its own `context-paths.txt` (the cargo workspace inputs), and run the clean-tree check only over those paths. That fixes both the dirty-planning refusal and the upload size.
- Add a `--state-dir DIR` option (default `models/laya-trainer`) to the ops script, used by 14-12's dry runs and by the selftest.
- State in both 14-09 and 14-12 that `image` and `probe` must be started with `run_in_background` and judged by their last line.

**Risk: MEDIUM.** The recipes are offline-provable; the clean-tree and state-dir gaps would surface as refusals in 14-12, not as wrong actions.

---

## 14-10

**Summary.** Deploy template without IAM and with auth off at publish, swap-guarded deploy, grant gated on an in-invocation 401 check, and an authenticated client with a local selftest. D-10's ordering becomes mechanical, which is the plan's purpose.

**Strengths.**
- The package-resolution trap is real: `laya-deploy` sets `EXPECT_PKG="crates/$SERVER-lambda"` (`justfile:2191`) and `_laya-crates-root-swap` backs up and restores `.pmcp/*` and `deploy/` with a leftover-backup refusal (`justfile:2057-2070`). Routing the new server through the same swap is correct.
- `_laya-auth-refusal-check` is `[positional-arguments]` with `url wait_s` and hardcodes `classify` in `CALL` (`justfile:4338, 4363`); an optional third positional with default `classify` leaves every existing caller unchanged, as the plan states.
- Revoking the inline grant before a `--no-oauth` redeploy is the right response to Phase 13's unobserved OAuth-reset risk (`13-HANDOFF.md` section 9).
- `m2m_token.py` exists at `scripts/laya_deploy/m2m_token.py` and `laya-probe-auth` already enforces "token only from LAYA_M2M_FILE, never argv" (`justfile:4291-4296`), which `train_call.py` mirrors.

**Concerns.**
- MEDIUM: `train_status` with `task_id: "auth-refusal-probe"` is the 401 probe. On an open server after the grant this call reaches DynamoDB and answers NotFound; before the grant it answers AccessDenied. Both are harmless, but the recipe's success criterion is "401", and a transient platform 5xx would read as `REFUSED auth-open` and block the grant. That is fail-closed, acceptable.
- MEDIUM: the deploy template sets `[auth] enabled = false`. If cargo-pmcp ever renders auth on for pmcp-run, the open window disappears, but a redeploy would then not need the revoke. The plan's revoke-first rule stays correct in both worlds; note it.
- LOW: `train_call.py` falls back to `/etc/ssl/cert.pem` when the default context has no CA bundle (F-13-08-3, `13-HANDOFF.md` section 9). Good, but the helper itself (`m2m_token.py`) is still unfixed per the handoff; the recipe sets `SSL_CERT_FILE` explicitly in 14-13, so it is handled by the caller.

**Suggestions.**
- Have the grant subcommand distinguish "401 observed" from "transport error" in its refusal text so the owner knows whether to retry or to check OAuth.
- Add a `train_call.py` selftest case for a 401 response (prints the error line, exits non-zero, no token echoed), since 14-13 will observe one.

**Risk: MEDIUM.** Offline-proven; the live ordering it enforces is observed in 14-13.

---

## 14-11

**Summary.** The D-20 hand-off: a manifest-verified fetch into `models/decide/<name>/` with an atomic rename and a tampered-file gate row. Small, correct and complete.

**Strengths.**
- Staging-then-rename, every entry's size and sha256 checked before anything appears at DEST, and a non-empty DEST refused: the three properties D-20 needs.
- The local layout restores `run/` with `checkpoint/`, `data/` and the `.apr`, which is what `laya-verify apr run data base` (`justfile:1532`) needs (RESEARCH Pitfall 11).
- Gate registration is required by the drift check: any `laya*` recipe whose body contains `exec`, `FAIL`, `exit 1|2|3` or a verdict token must be a row (`justfile` drift check, the `GATE` regex and `exec` scan in the `laya-gates-selftest` Python block). The plan registers `fetch` and a `row_fetch`.
- Baseline confirmed: no `laya-fetch` recipe and no `fetch` row at HEAD.

**Concerns.**
- LOW: the printed `laya-verify` command points at `~/.cache/huggingface/hub/.../snapshots/55cf4c4e...`, which exists on this machine, but the recipe does not check it exists before printing. It is "owner-run, not executed", so this is cosmetic.
- LOW: TASK_ID is validated as a 36-char lower-hex UUID; `mint_id` produces exactly that shape (`task_store.rs:584-607`). Fine, but the validator should be shared with 14-01's `execution_arn` test fixtures to avoid two UUID grammars.
- LOW: `depends_on` lists 14-04, but nothing in the plan uses the Lambda crate; 14-01's `manifest-v1.json` is the only cross-plan input. The extra edge delays nothing (both are earlier waves) but is inaccurate.

**Suggestions.**
- Print a one-line note when the local base snapshot path is absent, naming `just laya-fetch-base` or the HF download the owner would run.
- Drop 14-04 from `depends_on` or state why it is there.

**Risk: LOW.**

---

## 14-12

**Summary.** The first live plan: image builds in AWS, both stacks, staging, and the CUDA and SIGTERM probes, all behind an owner go with price and quota on record. It answers RESEARCH Open Question 2 by measurement, which is the right order.

**Strengths.**
- Nothing paid happens before a verbatim owner reply with price and quota, and `quota-pending` lets infrastructure proceed while probes wait.
- The preflight dry-runs every live command with a counting fake aws and asserts `aws_calls 0` per recipe, and the 12-digit account-id scan runs on every evidence verify.
- The probe verdict is computed from the measured fields, and `deploy-go` on a failed probe is refused by the verify itself.
- Motivation-v1's three files are hashed against Phase 12's `inputs_sha256` before staging; I confirmed the local dir exists with 295 train rows (66, 41, 188).

**Concerns.**
- HIGH (inherited): the plan's `dry_runs` need placeholder `outputs-dev.json` and `images-dev.json` through a state-dir parameter that 14-09 does not define. As written, Task 1 cannot run the probe and infra dry runs without faking the real `models/laya-trainer/` state, which the plan forbids.
- MEDIUM: `laya-trainer-image pack` will refuse on the dirty `.planning/` files (see 14-09) unless they are committed first; the preflight's "context_clean true" check should include that condition explicitly so it fails in Task 1, not in Task 3.
- MEDIUM: long-running live commands (two CodeBuild builds of tens of minutes, two probe jobs of up to 30 minutes each) exceed the Bash tool's foreground cap; the plan does not say to background them, while 14-07 does.
- MEDIUM: the verdict requires `cuda.final_status in ("Completed", "Failed")`. A probe job that hits its 1,800 s `MaxRuntime` ends `Stopped` with `MaxRuntimeExceeded`, which the verify would reject as "unexpected status" even though it is informative. Allow `Stopped` with the failure reason recorded.
- LOW: the spend estimate is stated as [ASSUMED]; the owner's reply records the real price. Good.

**Suggestions.**
- Resolve the state-dir interface in 14-09 (add `--state-dir`) before this plan runs; have Task 1 assert the parameter exists.
- Add "commit or stash `.planning/` changes" to the preflight checklist and assert `git status --porcelain` is empty over the pack context paths.
- Mark `laya-trainer-image` and `laya-trainer-probe` as background invocations judged by last line, as 14-07 Task 2 does.

**Risk: HIGH.** First paid actions and the one unknown (CUDA 13 on the g5 host driver) that can redirect the phase; the plan handles the unknown well, but the two interface gaps would stall it at the preflight.

---

## 14-13

**Summary.** Publishes the request function with no access, observes it serving the right tools and refusing the bucket, has the owner enable OAuth and create the M2M client, measures the open window, grants, and exercises every free path live. D-10's ordering is observed rather than argued.

**Strengths.**
- The pre-grant refusal chain is sound: with no IAM, `HeadObject` on the dataset answers AccessDenied, mapped to `dataset-access` by 14-04, so the open window cannot mint or spend. Then the same authenticated call before and after the grant is the cleanest proof of "the grant is the only thing standing between a caller and the bucket".
- The live assertions are checked against real data: `shots_per_class` is `max(class_counts)` (`train.py:767`), motivation-v1's counts are 66, 41, 188, so 188 is right; tweet-stance-64 has `shift.jsonl` and tweet-stance-16 has 16 per class, so `shift_present true` and `epochs_fixed 12` are right.
- The `auth-refusal ok: tools/list 401, tools/call 401` prefix matches the recipe's printed line (`justfile:4333-4336`), and the endpoint regex matches the ze-kasher-dev host shape seen in `13-HANDOFF.md` (`aprender-mcp-decide-motivation.us-east.true-mcp.com`).
- Every refusal check (`seeds` unknown field, `Bad` name, epochs 3) is done with `ListExecutions` read afterwards and required empty.

**Concerns.**
- MEDIUM: "the unauthenticated validate_dataset is refused `dataset-access`" assumes the platform forwards an unauthenticated `tools/call` to the function while OAuth is off. Phase 13 observed that posture on augury, and ze-kasher-dev's decide servers run auth off, so it holds; but if the platform's pre-OAuth behavior differs for a new server, the evidence field `unauth_validate.rule` will be something else and Task 1's verify fails hard. Treat a non-`dataset-access` answer as a finding to record, not only as a verify failure.
- MEDIUM: one pmcp-run credential slot (`13-HANDOFF.md` section 9). The precondition says `deploy-go` "also confirms the ze-kasher-dev pmcp.run login", but a login that expired between 14-12 and 14-13 shows up as an auth gate mid-deploy. The execute workflow handles auth gates, so this is a known interaction, not a defect.
- LOW: the absent-prefix check expects `task-missing`, which depends on the prefix-scoped `ListBucket` in the granted policy (`setfit-training-stack.ts:278-287` lesson). If 14-08's policy output lacks the condition, the live answer is `dataset-access` and this is the first place it shows. Good placement.

**Suggestions.**
- Let Task 1 record `unauth_validate.rule` verbatim and only require that it is not `valid`, with a separate assertion that no task was minted; then an unexpected platform posture is evidence, not a stop.
- Record the Lambda role's session duration (from `get-function` plus `get-role`) in `post_grant`, for 14-08's presigned-link TTL question.

**Risk: MEDIUM.** Nothing paid; the observations are the point, and the plan is built to record them.

---

## 14-14

**Summary.** First end-to-end run on AWS with a small production dataset, the live single-flight refusal, read-time links, fetch and local verify, retention tags, then the live cancel test, then the owner's D-16 band and retry declaration committed before the paid reproduction. The ordering (pipeline check before trust bar) is exactly right.

**Strengths.**
- tweet-stance-64 is a real three-class set (64 per class, checked) with `shift.jsonl`, so one run exercises epochs-required, the shift probe and three seeds at modest cost.
- The cancel test observes A1 directly and records which mechanism stopped the job (`stopped_by`), with the lock record and `list-training-jobs InProgress` checked afterwards. That closes the financial DoS path end to end.
- The reproduction declaration addresses the pilot waiver explicitly: `recipe_id_for` takes variant, shots, epochs, seed, base, stopping and selection (`contract.py:311-320`), not the device, so Phase 12's `motivation-v1-pilot-ece` waiver can bind a SageMaker run; the plan's "waived counts as inconclusive" default is the honest choice given Phase 12 passed unwaived (`12-GATE-RUN-EVIDENCE.json:121, 160`).
- The band proposal is grounded in Phase 12's per-seed spread (seed 17 ECE 0.1197 failed; `12-GATE-RUN-EVIDENCE.json:84-90`).

**Concerns.**
- MEDIUM: the smoke's status poll caps at 3 h. The run is training (three seeds, 64 shots, 4 epochs, likely under an hour) plus a Fargate pack and verify whose wall is unmeasured on Graviton (14-07 concern). If pack plus verify runs long, the poll ends while the execution is still in PackVerify; the plan says "stop and report", which is right, but the lock stays held until the finalizer runs, so Task 2's cancel test precondition ("the smoke finished, so the lock is free") may need a wait.
- MEDIUM: Task 2 cancels "once the training job is InProgress". InProgress begins at `Starting`/`Downloading`, before the container runs. A cancel there proves A1 and the finalizer path, but does not exercise `run_step`'s SIGTERM handler. That is the probe's job (14-12), so no gap, but the evidence should record the secondary status at cancel time.
- MEDIUM: D-19 live check samples tags on the smoke run only. The `keep` re-tag happens only for `completed`; if the smoke gate-fails (plausible on a 64-shot stance set), the `keep` path is not observed until 14-15. Say so in the evidence rather than let the verify's `want = "expire"` branch make it look covered.
- LOW: the verify computes the image-digest check against `14-INFRA-EVIDENCE.json` by path substitution; fine.

**Suggestions.**
- Make the status poll cap "6 h training plus the Fargate `taskTimeout`" and have Task 2 wait for `lock_released` via a read-only `get-item` before cancelling.
- Record `SecondaryStatus` at the moment of cancel and whether the container had started.
- If the smoke gate-fails, add a `keep_tag_observed: false` field so 14-15 is known to be the first observation of the keep path.

**Risk: HIGH.** It is the first paid end-to-end run; every pipeline defect from waves 1 to 6 surfaces here. The plan is built to absorb that (it is explicitly a pipeline check, never a gate claim).

---

## 14-15

**Summary.** The trust bar: motivation-v1 retrained through the live server with `epochs 12`, judged mechanically by the committed declaration, recorded in the Phase 12 evidence shape with the first x86 noise record beside the MPS values.

**Strengths.**
- Every MUST clause is read from the fetched run's own files, never from the status payload alone, and `recipe_id_written` must equal Phase 12's `f88f5b71...` (`12-GATE-RUN-EVIDENCE.json:25, 62`), which is the strongest same-recipe proof available.
- The verdict is forced to follow the declaration (attempts bounded by `attempt_cap`, defect classes never retried, `next_trusted` only on `reproduced`), and the verify asserts the two Laya contracts equal HEAD, which is D-15 made executable.
- The comparison block pins Phase 12's 752 s and both noise records, so the x86 noise measurement RESEARCH asked for lands in one place.

**Concerns.**
- MEDIUM: Phase 12's shipped ECE had 0.0064 headroom and one of three seeds failed; a fresh draw on CUDA has a real chance of an in-band GATE FAIL, and with `attempt_cap 2` the phase can end `inconclusive`. The plan handles it honestly, but the ROADMAP's phase goal reads as if the server is trusted at the end of the phase. The phase summary should state up front that `inconclusive` is a possible and legitimate outcome.
- MEDIUM: `wall_seconds` is `TrainingTimeInSeconds`, which includes image pull and base download, so it is not comparable with Phase 12's 752 s (trainer wall). The gate-report's own `SEEDS total_seconds`-style figure is the comparable one; record both.
- LOW: the verify accepts two spellings of the `inputs_sha256` keys; pick one (the trainer writes `task.json`-style keys in gate-report, Phase 12's evidence used `task_json`). Fine as written, but say which is canonical for Phase 14.

**Suggestions.**
- Add `train_wall_seconds` from the run's gate-report beside `billable_seconds` and `TrainingTimeInSeconds`, and compare that to 752 s.
- In the evidence, add `outcome_meaning` text for `inconclusive` ("the server behaved; the draw landed in band and failed the gate; a third attempt is an owner decision"), so a reader of the summary alone understands it.

**Risk: HIGH.** Not because of design: the run is one paid attempt at a razor-thin bar, and the result may be inconclusive by construction. The plan is honest about that.

---

## Cross-plan assessment

**Dependency ordering.** The wave graph is sound. The one owner-gated plan in wave 1 (14-03) blocks only 14-06, and 14-02 and 14-04 proceed in parallel. The `coupling_justified` notes for 14-02 and 14-04 and for 14-10 and 14-12 are accurate: both pairs touch disjoint parts of a shared file or compile against a fixed signature. 14-11's dependency on 14-04 is unnecessary but harmless.

**Interface gaps to close before execution.**
- 14-09 must define `--state-dir` for the ops script; 14-12 depends on it.
- 14-09's pack-image clean-tree check must be scoped to the cargo workspace inputs, or the dirty `.planning/` files at HEAD will refuse it.
- 14-06's local lint must not use the repo root as Docker context (no `.dockerignore`, `models/` is 26 GB).
- 14-08 must name `s3:PutObjectTagging` for the SageMaker and Fargate roles and add it to the mutation table.
- 14-04's cancel sink needs one test against a real SDK client across runtimes.

**Security.** The threat registers are consistent and most mitigations map to a named test. The strongest property is that the request function holds no IAM until a 401 is observed in the same invocation as the grant, which 14-13 observes with the same authenticated call before and after. The confused-deputy guards (bucket equality, `datasets/` prefix grammar, link keys confined to the run prefix) are tested at the unit level and scoped again in IAM.

**Scope.** No scope creep. The plans implement D-01 to D-22 and nothing deferred (no upload tool, no auto-deploy, no contract amendment, no ECS). The one place the plans go beyond CONTEXT is recording image digests in the manifest (RESEARCH Open Question 6), which is provenance, not a feature.

**Does the phase achieve its goal?** Yes, with the caveat that the trust-bar outcome may be `inconclusive`. The server, container, pack step, finalizer, retention, fetch and deploy order are each specified with an offline proof, and the live plans observe rather than assume the four AWS behaviors research could not verify (CUDA on the g5 driver, SIGTERM grace, abort propagation, duplicate events).

**Overall risk: MEDIUM** for waves 1 to 5 (offline, well tested) and **HIGH** for waves 6 to 9 (paid, first contact with CUDA on SageMaker, a thin reproduction margin). Fix the five interface gaps above before wave 2 starts; none of them changes a decision.

---

## Orchestrator Verification of Single-Reviewer HIGHs

Reviewer agreement is not evidence, so the HIGHs that only one lane raised were spot-checked against source before this summary was written.

| Finding | Raised by | Check | Verdict |
|---|---|---|---|
| 14-01 copies ECE from `calibration.ece_post` | codex | `train.py:895-904`: the `calibration` block holds `bucket/t_fitted/t_applied/clamp_hit/slice_*`. `ece_post` lives under `fine_tuned` (see `bakeoff.py:52,247`). The plan text at `14-01-PLAN.md:237` says "calibration ece_post" | CONFIRMED (also repeated in 14-15) |
| 14-01 `build_server(store, …)` cannot reach the lock backend | codex | `AprenderTaskStore.backend` is private (`task_store.rs:383`) and there is no `pub fn backend`. `14-01-PLAN.md:263` passes only the store | CONFIRMED (signature fix) |
| 14-01 `validate_dataset` refuses datasets above 16 shots because no epochs are given | codex | `contract.py:242-244` refuses `None` above 16. `validate_files(…, epochs: Option<u32>, …)` applies "the epochs rule", but `DatasetSummary` also carries `epochs_required/epochs_range`, which suggests validate was meant to report rather than refuse. The plan never says which | PLAUSIBLE: ambiguity to resolve explicitly (14-13 expects motivation-v1, 188 shots, to validate) |
| 14-07 waived acceptance is unreachable | codex | `train.py:926-928`: the waiver claim "never changes `pass` or the exit status; pack and verify decide". `14-07-PLAN.md:97` `pack-skip` skips every `gate_fail` | CONFIRMED |
| 14-12 `repo_uri` evidence vs the 12-digit account-id ban | codex | `14-12-PLAN.md:184` requires `images.{train,pack}.repo_uri`. An ECR URI starts with the account id, and `:124`/`:213` assert no `\b\d{12}\b` | CONFIRMED |
| 14-15 exact `inputs_sha256` dict equality | codex | `14-15-PLAN.md:102` accepts only 3-key dicts. The gate report writes 5 keys (`train.py:909-913`: also `base_model`, `tokenizer_json`) | CONFIRMED if the field is copied from the report |
| 14-06 local `docker build --check` uses the repo root as context | fable | no `.dockerignore` at root, `models/` = 26 G | CONFIRMED |

Not re-verified here (judgment-class or AWS-behaviour claims; treat as reviewer findings): codex's durable cancel/dispatch reconciliation and EventBridge best-effort concerns (14-04/05/08), the 14-10 grant-URL binding, and fable's dirty-`.planning/` clean-tree refusal, missing `--state-dir`, and `s3:PutObjectTagging` gaps (14-09/12/08).

---

## Consensus Summary

The two lanes reach different overall verdicts. Codex says **request changes, HIGH**. Fable says **MEDIUM for waves 1-5, HIGH for the paid waves 6-9, with five interface gaps to fix before wave 2**. Both endorse the architecture: grant-after-401, a CAS lease with no backend TTL, one guarded terminal writer, and an owner gate on every spend. The defects they found mostly do not overlap. Codex found contract/field-mapping errors against the real writer outputs and failure-recovery gaps. Fable found build/ops interface gaps. Orchestrator spot-checks confirmed most of codex's single-reviewer HIGHs (table above), so the phase should be replanned before execution.

### Agreed Strengths
- The lock design is correct against the scaffold: no TTL on the lock row escapes the `put_new`/expired-row trap, and `attach_result_if_absent` is the only safe way to attach a result to a cancelled task (`task_store.rs:552-568, 659-701`).
- 14-04 targets the D-22 register correctly (10 → 11, not the allowlist's 27).
- Deploy/auth ordering in 14-10 and 14-13: an in-invocation 401 before the grant, the same call made before and after the grant, and `m2m_token.py` reused.
- 14-07 builds `pack_laya` at `/src` so the compile-time `CARGO_MANIFEST_DIR` contracts path resolves (`pack_laya.rs:62-74`).
- 14-14 declares the reproduction band from Phase 12's real per-seed spread (seed 17 failed ECE) before the paid run.

### Agreed Concerns
- **Cancel sink (14-04) is the weakest mechanism.** Codex: a failed or timed-out `StopExecution` is logged and lost after the task is already Cancelled, with no durable retry. Fable: the cross-runtime sync-in-async pattern has never been exercised against a real SDK client. Fix: persist cancel intent, retry until compute is terminal, and add a test against a real client across runtimes.
- **The finalizer's lock release can happen too early or on the wrong path (14-05).** Codex: it releases single-flight before the training job reaches a terminal state. Fable: the NotFound branch conflates "expired" with "owner mismatch" and can release the lock for a live Working task.
- **SIGTERM timing (14-06).** The 20 s child wait plus retried uploads does not fit the test's 10 s expectation or a short SageMaker grace. Define one absolute shutdown deadline.
- **The 14-14 smoke can pass without exercising pack/verify or the `keep` retention path** if it gate-fails. Require or record which paths were actually observed.
- **The 14-15 outcome can be `inconclusive` by construction** (thin ECE headroom, attempt cap). The retry rules and the verifier need tightening (codex: vacuous `all()` over empty dicts and a cap above 2 left unimplemented; fable: say up front that `inconclusive` is legitimate and compare train wall time, not `TrainingTimeInSeconds`).

### Divergent Views
- **14-01 risk:** codex HIGH (three blockers, two confirmed and one plausible). Fable MEDIUM ("every mechanism exists and behaves as stated"). Fable missed the field-mapping and signature defects.
- **14-09 / 14-12 ops:** codex focuses on account identity (profile name ≠ account, so reuse `_laya-account-check`), first-deploy output discovery, and a stale `shift.jsonl` on restage. Fable focuses on the dirty-tree refusal, the missing `--state-dir`, and backgrounding long commands. These sets are complementary, not contradictory.
- **14-08:** codex says EventBridge is best-effort and the only abort-recovery path, and that the ECR keep-20 rule can delete a deployed digest. Fable says `s3:PutObjectTagging` is missing from the roles and the mutation table, and the `data` channel `S3Uri` construction is unspecified.
- **14-12 probe sufficiency:** codex wants one real-model forward/backward/optimizer step to prove memory fits. Fable wants a `Stopped`/MaxRuntimeExceeded outcome accepted as informative.
