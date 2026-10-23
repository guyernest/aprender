---
phase: 11
reviewers: [codex]
reviewer_instances: [fable]
reviewed_at: 2026-10-23
plans_reviewed: [11-01-PLAN.md, 11-02-PLAN.md, 11-03-PLAN.md, 11-04-PLAN.md, 11-05-PLAN.md, 11-06-PLAN.md, 11-07-PLAN.md, 11-08-PLAN.md, 11-09-PLAN.md, 11-10-PLAN.md, 11-11-PLAN.md, 11-12-PLAN.md, 11-13-PLAN.md]
models:
  codex: "gpt-6.1-sol (reasoning=high)"
  fable: "claude-fable-5-1 (reasoning=high)"
model_sources:
  codex: "banner"
  fable: "pinned"
---

# Cross-AI Plan Review - Phase 11

## Codex Review

Source review completed at `1e1aa99a7`. I inspected the referenced implementation, contracts, repository guards, and pinned Lambda runtime. I did not run AWS operations or reproduce the research’s performance measurements.

Below, `P/` denotes `.planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/`. References to proposed endpoint files are assessed through their plan specifications; those files do not yet exist.

## 11-01

**Summary:** Good measurement-first approach, but the probe’s results need tighter qualification before they become production limits.

**Strengths:** The proposed `no_grad` placement matches the implementation: gradient state and the tape are thread-local, so the guard must execute on the encoding thread (`crates/aprender-core/src/autograd/mod.rs:63`, `:89`). The closed resource allowlist and planted violations provide stronger evidence than inspecting CDK source alone (`P/11-01-PLAN.md:247`).

**Concerns:**

- **MEDIUM — Dependency containment does not establish production equivalence.** The lock check compares package names and versions, without comparing sources, checksums, resolved features, or build settings (`P/11-01-PLAN.md:169`). The probe also enables `conformance-fixtures` and performs an additional tokenization before calling `encode_texts`, which itself tokenizes (`P/11-01-PLAN.md:144`; `crates/aprender-core/src/setfit/mod.rs:582`).
- **MEDIUM — Tokenizer pricing samples only ordinary prose.** The byte probe repeats the corpus, while truncation performs an additional untruncated pass for cut inputs. Byte-equivalent Unicode, punctuation, and control-character inputs can exercise different work (`P/11-01-PLAN.md:144`; `crates/aprender-core/src/setfit/tokenizer.rs:447`).
- **LOW — Invoke-count claims omit retries.** The nominal 15 invokes can increase through cold-start retries (`P/11-01-PLAN.md:249`).

**Suggestions:** Record resolved sources/features, distinguish probe timing components, add adversarial tokenizer workloads, and record actual attempts and billed time.

**Risk Assessment:** **MEDIUM.** Suitable for exploration; insufficient by itself to certify the maximum admitted production workload.

## 11-02

**Summary:** Explicit owner decisions are appropriate, but the approved alternatives need executable downstream consequences.

**Strengths:** The register really requires a prior CONTEXT decision, and its ratchet currently holds nine deployment units (`crates/aprender-core/tests/monorepo_invariants.rs:428`). The proposed combined embedding-and-facts API addresses a real visibility gap: production tokenization remains crate-private (`crates/aprender-core/src/setfit/mod.rs:616`).

**Concerns:**

- **MEDIUM — The register policy interpretation remains ambiguous.** The source says growth is “never a same-PR edit”; this plan records approval before creating the crate, but does not explicitly resolve whether both changes may share a PR (`crates/aprender-core/tests/monorepo_invariants.rs:450`; `P/11-02-PLAN.md:125`).
- **MEDIUM — Owner-amended names are accepted without consistently parameterizing later plans.** The decision records amended crate/bin names, while later verification commands hardcode `aprender-embed-endpoint` (`P/11-02-PLAN.md:125`; `P/11-06-PLAN.md:178`).

**Suggestions:** Record the owner’s explicit interpretation of the register rule. Either constrain supported names in the decision or make every downstream command consume the recorded names.

**Risk Assessment:** **MEDIUM.** Primarily execution and governance consistency risks.

## 11-03

**Summary:** Strong provenance-oriented intake, with gaps between the declared rules and their verification.

**Strengths:** Verbatim source capture, hashes, and quote matching directly support the requirement not to invent acceptance checks (`P/11-03-PLAN.md:117`, `:164`). That requirement is explicitly locked in CONTEXT (`P/11-CONTEXT.md:25`).

**Concerns:**

- **HIGH — “Complete” intake can omit essential acceptance inputs.** Completeness requires the brief, schema, and event, but not a usable conformance tool or a validated full check set (`P/11-03-PLAN.md:162`). The later harness accepts any positive check count (`P/11-11-PLAN.md:106`).
- **MEDIUM — Quote enforcement has exceptions.** The verifier excludes `caller` from quote checking and does not validate the function-name answer’s quote (`P/11-03-PLAN.md:200`). Thus an unsourced authorization assumption can pass.
- **MEDIUM — A documented decision branch cannot pass verification.** Keeping the original name despite an incompatible allowlist is offered as an option, but verification always requires the final name to match the required prefix (`P/11-03-PLAN.md:166`, `:204`).
- **LOW — Sanitized headers can still trigger rejection.** Redaction preserves header names, while the secret scanner rejects the literal `x-amz-security-token`, even when its value is `REDACTED` (`P/11-03-PLAN.md:117`, `:128`).

**Suggestions:** Separate material completeness from acceptance readiness. Require the governing check count, tool identity, and all security-relevant quotations before acceptance work. Make each supported decision branch verifiable.

**Risk Assessment:** **HIGH** for acceptance readiness; otherwise a sound intake design.

## 11-04

**Summary:** The owner-run measurement session is sensible, but a successful invocation is not necessarily a successful embedding.

**Strengths:** Separating offline harness construction from owner-run AWS work follows the locked deployment constraint (`P/11-CONTEXT.md:26`; `P/11-04-PLAN.md:103`). Recomputing derived numbers from the raw record is a useful reproducibility check (`P/11-04-PLAN.md:177`).

**Concerns:**

- **HIGH — Application failures can satisfy the measurement gate.** The probe intentionally returns load/encode failures as successful Lambda responses. The measured-outcome conditions reject FunctionErrors and check 64-input truncation, but do not consistently require successful one-input embeddings, expected rows/dimensions, and valid vectors (`P/11-01-PLAN.md:144`, `:248`; `P/11-04-PLAN.md:168`). A fast error response must not establish the cold canary.
- **HIGH — Local hashes are not bound to deployed code.** The evidence copies bootstrap/APR hashes from the pre-record, without comparing the deployed package against an expected package digest (`P/11-04-PLAN.md:148`, `:174`).
- **MEDIUM — The lower-memory fallback is not implemented coherently.** The session offers a lower-tier retry, while the probe recipe specification requires 10,240 MB and exposes no matching memory argument (`P/11-04-PLAN.md:107`; `P/11-01-PLAN.md:249`).

**Suggestions:** Require successful embeddings for every performance sample, correlate deployment identity with the pre-record, and implement or remove the lower-tier branch.

**Risk Assessment:** **HIGH.** These measurements become inputs to the safety contract.

## 11-05

**Summary:** The additive core API is well motivated and mostly well scoped.

**Strengths:** Reading facts from the encoded batch follows the existing classification mechanism, including typed refusal for missing facts and attention-mask token counts (`crates/aprender-core/src/setfit/classify.rs:739`, `:762`). Avoiding `clear_graph` inside a library API correctly preserves caller-owned state (`crates/aprender-core/src/autograd/mod.rs:117`).

**Concerns:**

- **MEDIUM — Panic safety is documented rather than provided.** Existing `no_grad` restores the flag only after a normal return; unwinding skips restoration (`crates/aprender-core/src/autograd/mod.rs:93`). The new public API knowingly inherits that behavior (`P/11-05-PLAN.md:99`). This weakens “safe by construction” for callers that catch panics.
- **MEDIUM — Caller-state preservation is not directly tested.** The tests start with an empty tape, so they do not prove an existing graph survives or that an initially disabled gradient flag remains disabled (`P/11-05-PLAN.md:134`).
- **MEDIUM — The real-weight comparison can contaminate its own tape assertion.** It calls ordinary `encode_texts` before asserting subsequent inference calls leave a zero tape, without specifying cleanup or a guarded comparator (`P/11-05-PLAN.md:192`). Ordinary `encode_texts` has no inference guard (`crates/aprender-core/src/setfit/mod.rs:582`).

**Suggestions:** Add nonempty-tape and initially-disabled-state tests. Specify panic restoration and run the numerical comparator under `no_grad`.

**Risk Assessment:** **MEDIUM.** The normal inference path is credible; public-library state preservation needs completion.

## 11-06

**Summary:** The tracer architecture is appropriate, but its principal R5 assumption is false in the pinned runtime.

**Strengths:** Delegating encoding to core preserves the existing masked-mean-pooling and L2 implementation (`crates/aprender-core/src/setfit/encoder.rs:941`). Testing the actual binary through the Runtime API is materially stronger than testing only `handle` (`P/11-06-PLAN.md:224`).

**Concerns:**

- **HIGH — `LambdaEvent<Value>` does not eliminate deserialization FunctionErrors.** The pinned runtime deserializes before calling the handler; failure constructs an invocation error request:
  - `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/lambda_runtime-0.13.0/src/layers/api_response.rs:105`
  - `.../lambda_runtime-0.13.0/src/requests.rs:175`

  That request carries `function-error-type: unhandled`. The pinned JSON deserializer has a depth limit of 128 (`.../serde_json-1.0.150/src/de.rs:63`). Deep outer event JSON can therefore bypass every planned envelope handler.
- **MEDIUM — The lockfile guard proves less than claimed.** It checks that no lines were removed and that the new package appears; unrelated package additions also pass (`P/11-06-PLAN.md:172`).

**Suggestions:** Capture the incoming payload through a verified raw-payload path and perform fallible bounded parsing inside the envelope boundary. Add raw outer-event depth and numeric-overflow cases through the mock runtime. Compare parsed lockfile package records.

**Risk Assessment:** **HIGH.** R5 is not achieved by the proposed runtime entry point.

## 11-07

**Summary:** Broad request coverage, but several tests can pass while the endpoint accepts invalid requests.

**Strengths:** Validation before model availability gives stable caller-error behavior during load failures (`P/11-07-PLAN.md:122`). Testing base64 through decoded f32 bits is a precise interoperability check (`P/11-07-PLAN.md:119`).

**Concerns:**

- **HIGH — The hostile oracle permits success.** Hostile cases accept any status in a set containing 200, rather than requiring the case’s expected rejection (`P/11-07-PLAN.md:119`). The generic wire checker similarly checks allowed statuses (`P/11-07-PLAN.md:178`). Removing a validator can remain green.
- **MEDIUM — Provenance labels are checked, policy values are not.** Verification establishes `"intake"` versus `"default"` but does not compare actual behavior with the normalized answer (`P/11-07-PLAN.md:148`).
- **MEDIUM — The status set expands locked R5 without a resolution step.** The plan introduces 404/405, while locked R5 names 400/413/429/500 (`P/11-07-PLAN.md:120`; `P/11-CONTEXT.md:11`).

**Suggestions:** Give every case an exact expected status/code/parameter and assert invalid requests never invoke the embedder. Compare policy values as well as provenance. Resolve the status-set interpretation explicitly.

**Risk Assessment:** **HIGH** until the rejection oracle is strengthened.

## 11-08

**Summary:** The contract structure is useful, but the proposed derivation does not justify its safety claims.

**Strengths:** Contract-to-Rust equality tests and real test bindings follow established repository mechanisms. The existing binding guard distinguishes schema validity from resolvable test references (`scripts/check_contract_test_binding.sh:11`, `:25`).

**Concerns:**

- **HIGH — Admission expands substantially beyond measurement.** The formula prices a larger count from the 64-input run (`P/11-08-PLAN.md:142`). Applying it with a 1,000 ms margin produces **961 inputs at 9 ms/input, 408 at 130 ms/input, and 114 at 700 ms/input**. No corresponding maximal-request Graviton test is required.
- **HIGH — The deadline guard is incomplete.** Its estimate includes input count and margin, but not the request’s tokenizer-byte cost (`P/11-08-PLAN.md:127`). The current encoder call has no cancellation/deadline parameter (`crates/aprender-core/src/setfit/encoder.rs:941`); checks between batches cannot interrupt an already-running tokenizer or forward pass.
- **MEDIUM — Startup panics remain outside the recovery boundary.** Identity verification and loading execute before the guarded request path (`P/11-08-PLAN.md:218`). Keeping ordinary `Result` failures does not catch panics.
- **MEDIUM — Supported policy branches conflict with verification.** Headers may be forbidden, yet verification unconditionally indexes all identity headers (`P/11-08-PLAN.md:235`). Successful identity tests also require the real pinned tokenizer, contrary to the “small stand-in files” specification (`P/11-08-PLAN.md:214`).

**Suggestions:** Initially cap admission at the measured count, then test maximal count/byte combinations on Graviton before expanding. Include tokenizer and serialization costs in deadline admission. Guard startup panics and make policy-dependent checks conditional.

**Risk Assessment:** **HIGH.** Arithmetic consistency is not yet evidence that admitted requests avoid timeout or OOM.

## 11-09

**Summary:** Strong numerical coverage and release metadata, with reproducibility and tooling gaps.

**Strengths:** Exact truncation boundaries, long inputs, and batch-versus-single comparisons improve substantially on the existing three-sentence full-weight fixture (`crates/aprender-core/tests/setfit_conformance/full_weight_parity.rs:109`; `P/11-09-PLAN.md:125`). The planned tolerances match existing generated constants (`crates/aprender-core/tests/setfit_conformance/tolerances_generated.rs:33`, `:45`).

**Concerns:**

- **MEDIUM — `--no-sync --frozen` does not establish installed-environment identity.** Only sentence-transformers and torch versions are checked, although transformers, tokenizers, and Python also affect the generated artifact (`P/11-09-PLAN.md:131`; `scripts/setfit_fixtures/pyproject.toml:9`, `:23`).
- **MEDIUM — Isolated Python execution needs an explicit import design.** The plan requires sibling-module sharing while invoking scripts with `python3 -I` (`P/11-09-PLAN.md:202`, `:208`). I reproduced that ordinary sibling imports fail under this invocation. The same issue affects later tooling unless bootstrapping is specified.
- **MEDIUM — Bundle checking records CodeSha256 syntax, not deployment identity.** Any valid 32-byte digest passes the checker; its relationship to the tested binary must be established elsewhere (`P/11-09-PLAN.md:210`).

**Suggestions:** Validate the full reference environment against the lock, define isolated-mode module loading with lazy reference-only imports, and distinguish offline bundle integrity from deployed-code verification.

**Risk Assessment:** **MEDIUM.** The numerical proof design is strong once reproducibility details are explicit.

## 11-10

**Summary:** Substantial deployment safeguards, but the live R4 guard is weaker than the offline guard.

**Strengths:** Template-versus-contract checks, account checking, and recorded containment calls are appropriate fail-closed mechanisms (`P/11-10-PLAN.md:130`, `:196`, `:259`). Comparing the existing training template protects a real shared entry point (`deploy-extensions/bin/app.ts:55`).

**Concerns:**

- **HIGH — Named service permissions pass live readback.** Offline scanning rejects every Lambda permission, but live readback accepts policies without wildcard principals (`P/11-10-PLAN.md:197`). An `apigateway.amazonaws.com` principal is not a wildcard and can permit precisely the ingress R4 forbids. AWS documents that API Gateway invocation permissions use Lambda resource policies. [AWS API Gateway integration documentation](https://docs.aws.amazon.com/lambda/latest/dg/services-apigateway.html).
- **HIGH — Readback does not cover qualified resources.** Function-only policy/URL checks do not establish alias/version state. AWS explicitly exposes qualified policies separately. [AWS GetPolicy API](https://docs.aws.amazon.com/lambda/latest/api/API_GetPolicy.html).
- **HIGH — The package identity chain remains incomplete.** A marker string plus matching model files does not establish that deployed code equals the tested package; readback records CodeSha256 without comparing it to the expected uploaded ZIP (`P/11-10-PLAN.md:196`).
- **MEDIUM — Canary failure is overinterpreted as a provisioned-concurrency requirement.** The flag is true whenever cold client wall exceeds 30 seconds, which does not isolate initialization as the cause (`P/11-10-PLAN.md:257`).

**Suggestions:** Require absent resource-policy statements for this same-account design, enumerate qualified exposure, add named-service-permission mutations, and compare the deployed ZIP digest with a precomputed package digest. Report provisioned concurrency as a measured recommendation requiring causal evidence.

**Risk Assessment:** **HIGH.** R4 and release identity remain deployment blockers.

## 11-11

**Summary:** Faithful check mapping is valuable, but transport and falsification requirements need adjustment.

**Strengths:** Exact IDs, titles, and quotes prevent acceptance drift, and distinguishing local results from requester acceptance avoids misleading certification (`P/11-11-PLAN.md:121`, `:124`).

**Concerns:**

- **MEDIUM — Persistent transport is not supplied by the earlier API.** The harness requires an interactive process retained across checks, while `invoke_many` is specified as a finite batch that kills the process before returning (`P/11-11-PLAN.md:122`; `P/11-06-PLAN.md:224`).
- **MEDIUM — Response mutation cannot falsify every possible check.** Infrastructure and timing checks require mutations of those inputs. Requiring `drop_usage` to fail also assumes a usage check before the requester’s checks are known (`P/11-11-PLAN.md:123`, `:212`).
- **MEDIUM — Ledger existence is overstated as enforcement.** A `just` ledger row verifies that a recipe exists; it does not execute it (`justfile:3759`). Likewise, digest/coverage tests do not run ignored numerical parity tests, although the planned documentation binds parity claims to them (`P/11-11-PLAN.md:240`).

**Suggestions:** Specify a persistent mock-runtime client or precompute check events. Match defect injection to each check’s actual dependency. Describe ledger bindings accurately and bind numerical claims to substantive parity evidence.

**Risk Assessment:** **MEDIUM.** Good acceptance discipline, with avoidable implementation and proof-strength problems.

## 11-12

**Summary:** The live session order is appropriate, but evidence validation can certify failed or unrelated measurements.

**Strengths:** Identity probing before conformance and explicit recording of a cold-canary miss are appropriate (`P/11-12-PLAN.md:123`, `:167`). Keeping code fixes outside evidence ingestion prevents silently weakening acceptance.

**Concerns:**

- **HIGH — Failed responses can become performance evidence.** Measurement records response status, but ingestion does not require every measured shape to return 200 with the expected embedding count and usage (`P/11-10-PLAN.md:257`; `P/11-12-PLAN.md:166`). A fast 500 must not count as a successful cold answer.
- **HIGH — Live conformance count is not validated.** Verification accepts a PASS-shaped line without checking numerator, denominator, or complete requester-ID coverage (`P/11-12-PLAN.md:183`).
- **MEDIUM — Records can be mixed across sessions.** Independently selecting newest readback, probe, conformance, and measurement files does not prove they describe the same deployment (`P/11-12-PLAN.md:143`, `:161`).
- **MEDIUM — The failure branch cannot satisfy its own verifier.** A canary miss stops before deployed-bundle construction, but verification still requires that bundle and release directory (`P/11-12-PLAN.md:167`, `:189`).

**Suggestions:** Carry one deployment/session identifier and expected CodeSha256 through every record. Require successful responses, complete check coverage, and proven-cold samples for both shapes. Define a separately verifiable failure-evidence schema.

**Risk Assessment:** **HIGH.** The current evidence gate can overstate live success.

## 11-13

**Summary:** Useful handoff discipline, but required verification is incomplete and scheduled too late.

**Strengths:** Recomputing handoff facts from owning records reduces transcription drift (`P/11-13-PLAN.md:137`). Reserving R7 closure for the requester’s own exact result line follows locked acceptance (`P/11-CONTEXT.md:13`; `P/11-13-PLAN.md:251`).

**Concerns:**

- **HIGH — Mutation coverage omits changed core implementation.** The command mutates only the endpoint crate, excluding the new core embedding door (`P/11-13-PLAN.md:195`). Repository policy applies mutation checking to the PR diff (`CLAUDE.md:599`).
- **MEDIUM — Required quality gates are absent.** The final gate list omits the documented coverage floor and `pmat comply` requirement carried into research (`P/11-13-PLAN.md:186`; `CLAUDE.md:111`; `P/11-RESEARCH.md`, “Project Constraints”).
- **MEDIUM — Final checks happen after deployment.** Discovering a substantive survivor then requires a separate code-fix plan while the acceptance target remains deployed (`P/11-13-PLAN.md:195`). These checks belong in deployment readiness.
- **MEDIUM — JSON exclusion weakens the identifier scan.** Most changed JSON is skipped, even though intake manifests and captured events are relevant disclosure surfaces (`P/11-13-PLAN.md:194`). Numeric embeddings need field-aware handling, not blanket exclusion.

**Suggestions:** Run quality gates before live deployment, include changed core code in mutation scope, add the required quality checks, and use schema-aware scanning for JSON. Keep post-deployment verification focused on deployed identity and acceptance.

**Risk Assessment:** **HIGH** for claiming phase completion; the handoff format itself is sound.

## Cross-plan assessment

**Recommendation: revise before execution.** All 13 plans were reviewed.

The architecture is credible: reuse the native encoder, expose a small core API, keep transport thin, measure on Graviton, deploy without ingress, and retain requester-owned acceptance. The main weaknesses are guards whose tests establish a weaker proposition than the plans claim.

Resolve these first:

1. **R5 runtime boundary:** handle outer-event deserialization failures inside the response boundary.
2. **R4 live exposure:** reject unexpected named service permissions and inspect qualified resources.
3. **Measured admission:** test the maximum admitted count/byte combinations before certifying limits.
4. **Deployment identity:** bind the tested ZIP to live CodeSha256 and correlate all evidence to that deployment.
5. **Acceptance oracles:** require exact rejection outcomes, successful measured embeddings, and complete check coverage.
6. **Predeployment quality:** include core mutations and required quality gates before live rollout.

There is also a scheduling contradiction: the roadmap blocks all Wave 2 work on Wave 1 completion, so delayed requester intake blocks the supposedly independent Graviton run and core API work (`.planning/ROADMAP.md:913`). Explicitly support dependency-based continuation or separate those tracks.

The plans contain substantial verification machinery. Tightening its assertions will improve confidence more than adding further recipes or evidence files.

---

## Claude Review (fable)

Verification is complete. I read the repo sources the plans cite (register test, tokenizer, autograd, lambda_runtime 0.13 source, justfile recipes, contracts, claims ledger, gates) and ran the tools the verifies depend on. Writing the review now.

# Cross-AI Plan Review: Phase 11 (aprender-embed-endpoint Lambda)

**Verification basis.** I read the referenced tree at HEAD `1e1aa99a7` on branch `gsd/phase-2-contract-gate` and ran the gates the plans lean on. Facts that hold up, cited once here and relied on below:

- `no_grad` only flips the thread-local flag and has no drop guard (`crates/aprender-core/src/autograd/mod.rs:63-68`, `:89-100`), so the plans' statements about the tape and about a panic leaving the flag off are accurate.
- `lambda_runtime` 0.13.0 is locked (`Cargo.lock:8222`), it accepts `Infallible` handler errors (`~/.cargo/registry/.../lambda_runtime-0.13.0/src/diagnostic.rs:85`), and its own `CatchPanicService` still reports a panic as `function-error-type: unhandled` (`src/runtime.rs:218`, `:336`, `:444`). Catching panics ourselves is therefore required, as plans 11-06 and 11-08 do.
- The runtime API env var and path live in `lambda_runtime_api_client-0.11.1/src/lib.rs:114` and `:144`, which is what the mock runtime of plan 11-06 must serve.
- The deployment-unit register and its "recorded CONTEXT decision, blocking-human" text are at `crates/aprender-core/tests/monorepo_invariants.rs:312-323` and `:445-454`; the stale-entry rule at `:476-486`.
- `FALSIFY-README-005` and `-007` are ratchets (lag allowed, overstatement red) at `crates/aprender-core/tests/readme_contract.rs:138` and `:200`; README claims 92 while `cargo metadata` reports 93 today. A crate README must contain `paiml/aprender` (`readme_contract.rs:302`).
- The strict test-binding gate classifies `LIVE-PENDING` and `just …` harnesses as Unbindable and skips them (`crates/aprender-contracts/src/lint/strict_test_binding.rs:114-141`), and only fails on a `cargo test` filter naming a test that does not exist. `just` is absent from its shell allowlist, which matters below.
- The in-tree measure driver already forces cold starts by `update-function-configuration` plus `wait function-updated-v2` and reads `Init Duration` from the REPORT line (`deploy/llm-endpoint/probe/bench.py:120`, `:204-212`). Plan 11-01's normalizer rule has precedent.
- Tolerances: real-weight `3.73762473e-05`, batch invariance `7.62939453e-06` (`crates/aprender-core/tests/setfit_conformance/tolerances_generated.rs:33`, `:45`).
- Environment: Python 3.14.6 with PyYAML 6.0.3 under `-I`, `just` 1.46 with working `--dump --dump-format json`, cargo-zigbuild 0.23.4, the aarch64 target, `target/release/pv` printing exactly `0 error(s), 0 warning(s)` / `Contract is valid.` and `Result: PASS`, the frozen venv at 5.7.0 / 2.13.0 / 4.57.6 / tokenizers 0.22.2, the HF snapshot with `vocab.txt`, and `deploy-extensions/node_modules` absent.
- The pinned model directory also holds `model.safetensors` (86.7 MB) next to `full_model.apr`, so plan 11-01's explicit exclusion is load-bearing.
- `.planning/config.json`: `use_worktrees: false`, `nyquist_validation: true`, `inline_plan_threshold` unset (default 2). The nine two-task plans will execute inline in the main context.

## 11-01

**Summary.** Builds the Graviton measurement harness offline: a standalone probe project seeded from the root lock, the arm64 asset recipe, the no-ingress CDK stack with probe and endpoint variants, an allowlist-first template scanner with planted violations, the REPORT-line normalizer, and the owner-run recipes proven with an `aws` shim.

**Strengths.**
- The standalone-project approach has an exact precedent with its own empty `[workspace]` table (`deploy/llm-endpoint/probe/agent-loop/Cargo.toml:15`), and the lock-containment gate is a real check rather than a promise.
- The probe runs the production call shape (load once, `no_grad` on the encoding thread, micro-batches) and reports `graph_tape_len`, matching the thread-local facts at `autograd/mod.rs:63-68`.
- The cold-forcing and `Init Duration` rule copies what `bench.py:204-212` already does live.
- `deploy-extensions/.gitignore` already ignores `assets/` and `cdk.out/`, so the new asset directory needs no ignore edit.
- The training-template `cmp` control is a strong byte-level guard against app.ts drift.

**Concerns.**
- MEDIUM. The probe's `read_first` sends the executor to `justfile` lines 5300-5383 for the cold-forcing pattern, but `llm-endpoint-measure` only shells out to `bench.py` (`justfile:5336`). The mechanism is in `deploy/llm-endpoint/probe/bench.py:204-212`. Point `read_first` there or the executor re-derives it.
- MEDIUM. `RAYON_NUM_THREADS=6` is asserted as the thread count but never proven engaged (CLAUDE.md verification rule 2). The BLIS path does honor rayon's global pool (`crates/aprender-compute/src/blis/parallel.rs:25`), but the probe only reports the env var and `available_parallelism`. Report the effective pool size, or at least `cpu_part` plus a wall-time delta between two thread settings in one owner run.
- LOW. The allowlist is "derived from the synthesized templates" and then used to gate those same templates. On first run the real guards are the denylist and the wildcard-principal scan. State that in the scanner's docs so a later reader does not mistake the allowlist for independent evidence.
- LOW. The app.ts `embed-probe` branch must skip the trainer-asset check at `deploy-extensions/bin/app.ts:32-38`, otherwise synthesizing the probe stack requires a trainer package. The plan implies this but does not say it.
- LOW. `cpu_part` from `/proc/cpuinfo` and `vmhwm_kb` are null on macOS, so the local run cannot exercise those parsers. Add a fixture string test for the parser.

**Suggestions.** Add the probe's own `init_ms` and a sha256 of the `.apr` at startup so the gate's cold number includes the identity hashing that plan 11-08 adds to the real binary (see 11-08). Add `cold_64_duration_ms_max` to the normalizer's derived set now, since 11-08 computes it from samples "when absent".

**Risk Assessment.** Medium. Nothing here touches the workspace, and the AWS-free proof is well designed. The main residual risk is that the number measured under-represents the production cold path.

## 11-02

**Summary.** Two owner decisions recorded as blocking-human checkpoints: D-01 grows the deployment-unit register 9 to 10, D-02 fixes the shape of the additive `embed_texts` door in aprender-core.

**Strengths.**
- Matches the register test's own requirement text at `monorepo_invariants.rs:448-453` and the Phase 10 precedent of recording the decision before the crate exists.
- The decision parser is real and parses the Phase 10 CONTEXT today (`~/.claude/gsd-core/bin/lib/decisions.cjs`, outcome `parsed`), so the verify is executable.
- The recommended D-02 option reads facts off the encoded batch, mirroring `classify.rs:739` and `:762`.

**Concerns.**
- MEDIUM. The register's defining prose calls the category "thin MCP servers whose capability IS a protocol surface" (`monorepo_invariants.rs:300-311`). The embed endpoint is a Lambda deployment unit but not an MCP server. D-01's record should say explicitly that the category is being read as "publish = false deployment-unit binaries", and plan 11-06 should amend the register comment in the same change, or the sentence above `allowed_bins` stops being literally true.
- LOW. D-02's context says a panic inside the closure leaves the grad flag off for the thread. With tokio's blocking pool that thread is reused, so the next request on it would silently run under `no_grad`. That is the safe direction here, but an RAII guard restoring the previous flag costs one small type and removes the footnote. Offer it as a variant of the recommended option rather than a fourth option.

**Suggestions.** Give the owner the two-line summary of the register text and the tape numbers at the top of the checkpoint; the rest is reference.

**Risk Assessment.** Low. Records only; the one cost is calendar time waiting on the owner, and the plan is correctly placed in wave 1 so it does not wait on the requester.

## 11-03

**Summary.** Requester intake: the thirteen checks, the conformance tool, the reference-vector schema, a captured platform event, the function-name allowlist answer, and the envelope-policy answers, saved verbatim and normalized under a quote-or-unspecified rule.

**Strengths.**
- The quote-must-be-a-verbatim-substring rule is machine-checked, which is the right defense against invented checks.
- The function-name branch is grounded: the 2026-10-18 request says the platform "only invokes names that start with aprender-llm- or pmcp-llm-" (`.planning/phases/09-.../2026-10-18-pmcp-run-multi-size-change-request.md:13`), and `aprender-embed-endpoint-<stage>` matches neither.
- Redaction of the captured event and the before/after `git status` of sibling checkouts are concrete.

**Concerns.**
- MEDIUM. The stated-count regex in Task 2's verify (`\b(?:all\s+)?(\d+|thirteen|twelve|fourteen)\s+(?:conformance\s+)?checks\b`) takes the first match in the brief. A brief that says "three checks cover truncation" before "13 checks" fails the plan on a false mismatch, and the common form "13/13" is not matched at all. Record `stated_count` explicitly with its own quote and compare that.
- MEDIUM. `intake_complete` requires the captured event, but routing in 11-07 can be decided from `tool.invocation` plus a fixture if the platform cannot capture one. Allow the event to be marked `unspecified` with the default event shape recorded, so one missing artifact does not hold 11-11.
- LOW. The 12-digit scrub will refuse a brief containing any 12-digit example value; the executor is told to ask for a sanitized copy, which is right, but say so in the checkpoint text so the requester sends one the first time.

**Suggestions.** Ask the requester in the same message whether their tool builds the API Gateway v2 event itself or whether pmcp.run does; that answer decides whether the captured event is even obtainable from them.

**Risk Assessment.** Medium, driven entirely by third-party latency. The plan is correct to isolate it in wave 1.

## 11-04

**Summary.** The owner runs the Graviton gate on the dev account: deploy the probe, 15 invokes cold and warm, read back no ingress, destroy, and normalize into `11-GRAVITON-EVIDENCE.json`.

**Strengths.**
- Every write is owner-run; the executor's single AWS call is a read-only `get-function` after the destroy.
- The memory-ceiling trap is handled with `embedMemoryMb`, matching the training stack's documented 3008 MB experience (`deploy-extensions/README.md:58-66`).
- Provenance (bootstrap and apr sha256, raw record sha256, re-normalization equality) is checked, not asserted.

**Concerns.**
- MEDIUM. A warm sample that lands on a recycled environment carries `Init Duration` and the normalizer marks the outcome `failed`. The recipe retries cold samples until they are cold but has no symmetric retry for warm samples. Add one or the gate can fail on a Lambda lottery.
- MEDIUM. The gate measures the probe, not the endpoint. Plan 11-08 adds identity hashing of a 91 MB file at init and the envelope work, so the live cold number in 11-12 will be larger than the gate's. The plan reports both, which is honest, but 11-08 prices bounds from the gate. See 11-08.
- LOW. `rtk proxy aws lambda get-function` must pass `--region`; the plan says "use the region the raw record states", good, but the command line shown omits it.

**Suggestions.** Record the Graviton CPU part per sample in the SUMMARY table, since two cold-64 samples on different generations would otherwise look like noise.

**Risk Assessment.** Medium. Cost is small; the schedule risk is the owner session and the account ceiling.

## 11-05

**Summary.** Adds the owner-confirmed `embed_texts` door to aprender-core test-first: facts read off the encoded batch, encode inside `no_grad`, no `clear_graph`, a positive control proving the tape test can see growth, and real-weight proof on 2,000-wordpiece inputs.

**Strengths.**
- Zero-deletion `numstat` over `src/setfit` is a crisp "nothing existing moved" proof, and the seal scan excludes `*_tests.rs` and scans only for sealed constructors and re-exports (`crates/aprender-core/src/setfit/model_tests.rs:39`, `:185-230`), so a new `embed.rs` that calls `self.tokenizer.encode_batch` passes it.
- The in-crate test module pattern (`#[cfg(all(test, feature = "setfit"))] #[path = ...]`) exists at `mod.rs:827-829`, and CI fragment 010 runs `--lib setfit`, so the new tests are not dark.
- `original_len` of the empty string is 2 by the attention-mask popcount at `tokenizer.rs:443-446`; the behavior table is right.
- The harness structural scans (`.tokenize(` single call site, no `e-` literals, no sealed constructors; `setfit_conformance.rs:792-937`) are respected by the test design.

**Concerns.**
- MEDIUM. `full_weight_embed_texts_matches_encode_texts_and_keeps_the_tape_empty` calls `encode_texts` in grad mode on 16 real-weight texts, three past the cap. RESEARCH measured roughly 0.4 GB of tape per 256-token text, so this single call can take several GB on a developer box. Compare on fewer texts, or wrap the control call in its own `no_grad` and keep the growth proof on the slice model.
- MEDIUM. The "mod.rs gains only a module line and a re-export line" rule is fragile under `cargo fmt`: if the executor adds names into the existing `pub use` block (`mod.rs:50-76`) rustfmt reflows it and `numstat` shows deletions. Spell out a separate `pub use embed::{...};` statement.
- LOW. The `full_weight_parity.rs` file is gated whole by `#![cfg(feature = "model-tests")]` (`:31`), so Task 1's listing of that file in `files_modified` is correct only for Task 2; fine, but Task 1's clippy run with `model-tests` absent will not compile the new tests until Task 2 runs it with the feature.

**Suggestions.** Have `facts_from_batch` return `consumed_positions` from the mask slice per row using `chunks(seq)`, and add a test that `consumed_positions == original_len` for every untruncated row, which is the identity `tokenizer.rs:443-446` guarantees.

**Risk Assessment.** Low to medium. The change is additive and well fenced; the real-weight control is the one place it can hurt a developer machine.

## 11-06

**Summary.** Creates the `aprender-embed-endpoint` crate, raises the register to 10 citing D-01, implements the engine over the core door, proves one event in and an OpenAI list out against the committed reference, and drives the real binary through a local mock Lambda runtime.

**Strengths.**
- The bin is named after the crate and renamed to `bootstrap` only in the asset, so `scripts/duplicate_bin_names_allowlist.txt` stays untouched and cargo-pmcp's first-bootstrap fallback (`deploy-extensions/README.md:95-99`) cannot select it.
- `cargo metadata` reports `publish = false` as an empty array, which the verify checks the same way the register test does (`monorepo_invariants.rs:366-371`).
- The mock runtime serves the exact endpoints the client uses (`lambda_runtime_api_client-0.11.1/src/lib.rs:144`), and the load-failure negative control exercises the one init path that otherwise becomes a FunctionError.
- The `Arc<model>` into `spawn_blocking` pattern is proven in-tree at `crates/aprender-mcp-setfit/src/lib.rs:214`.

**Concerns.**
- MEDIUM. The register prose issue from 11-02: amend the comment at `monorepo_invariants.rs:300-311` in this same change so the category description covers an IAM-invoke Lambda unit, not only MCP servers.
- MEDIUM. Floats serialized through `serde_json::Value` are stored as f64, so an f32 `0.1` prints as `0.10000000149011612`. This matters in 11-08's per-vector byte bound and bloats every response. Serialize the embedding array from `&[f32]` with a typed struct, not through `Value`. Put that rule in this plan's `list` builder so 11-07 and 11-08 inherit it.
- LOW. The `tracer` integration target is all-ignored and no fragment names it, so CI never compiles it except through `clippy --all-targets`. Acceptable, but say so, or fold `--test tracer` into the 540 fragment of plan 11-09.
- LOW. "Cargo.lock only gains the new package" is asserted in the must-haves but the verify checks only "no removed lines plus the name present". Lock additions from new feature combinations are possible; `matchers` for `env-filter` is already locked (`Cargo.lock:8610`) so it is unlikely here, but align the verify with the claim.

**Suggestions.** Install the panic hook in this plan rather than waiting for 11-08; until then a panic message, which can contain input text, reaches CloudWatch.

**Risk Assessment.** Medium. The registry edits are the sharp edge and are well gated; the serialization point is a correctness detail that is cheap now and costly later.

## 11-07

**Summary.** Replaces the tracer's generic refusal with full request validation and OpenAI envelopes, a sourced POLICY table with provenance per intake answer, a valid table, a 45-case hostile table, and the same table run through the real binary with and without a loadable model.

**Strengths.**
- Validation before engine availability is the right rule and is tested.
- Deep-nesting and lone-surrogate cases rely on serde_json's recursion limit and strict escape parsing, both of which return errors rather than panics.
- The marker-planting rule for echo detection is applied at unit level and on the wire, with the checker proven able to fail.
- The `model_not_found` envelope cites the in-tree precedent (`crates/apr-cli/src/commands/serve/handler_apr_cpu_completion.rs:80-92`).

**Concerns.**
- MEDIUM. The POLICY values are hand-transcribed from `11-BRIEF-INTAKE.json` into Rust constants, and the verify compares only provenance keys, not values. A mistyped model alias or 429 trigger string ships unnoticed. Add a dev-only test that reads the intake file when present (workspace-relative, skip with a message when absent) and compares each stated value.
- LOW. Method casing "decide from the captured event" is unsafe if the event is missing; default to exact `POST` and accept the captured casing as an alias only when the intake supplies one.
- LOW. The hostile case "array containing an empty string per POLICY.empty_input" can be a 200 when the policy embeds empty strings; the table's status set includes 200, so the assertion is fine, but the test name promises an envelope. Name the case by policy.

**Suggestions.** Include a `Content-Type` check case (missing or `text/plain`) and decide the policy; the OpenAI clients always send JSON, but the platform harness might not.

**Risk Assessment.** Low to medium. The surface is large but mechanical, and the wire run on the shipped binary is the right proof for R5.

## 11-08

**Summary.** Derives every bound from the Graviton evidence with a tested script, writes `contracts/embed-endpoint-boundary-v1.yaml`, mirrors it in Rust with equality tests, enforces bounds in cost order, and finishes the runtime shell: identity verification at start, tape guard, panics and load failures as data, a payload-free panic hook, identity headers.

**Strengths.**
- The derivation structure copies `decide-tool-boundary-v1` and the mirror test pattern at `crates/aprender-mcp-decide/src/tests.rs:28-41`, which the tree already trusts.
- The stop conditions (timeout over 900 s, `max_inputs` under 64, `max_input_bytes` under 64 KiB) prevent publishing a bound that contradicts R2 or R6.
- `LIVE-PENDING` rows and `just …` harnesses are skipped by the binding gate (`strict_test_binding.rs:120-122`), so the plan's "no `test:` for a test that does not exist yet" rule is exactly what the gate needs.
- The identity check pins `tokenizer.json` and the revision to compiled constants (`crates/aprender-core/src/setfit/import.rs:68-78`), independent of the identity file's own claims.

**Concerns.**
- HIGH. Internal inconsistency in the bound order. The must-haves say body bytes are checked before decoding, but the behavior block and the test name `bound_order_is_count_then_input_bytes_then_request_bytes` put the request-bytes check last. The body length is known before any parse and is the cheapest check; it must be first, then count, then per-input bytes. Fix the behavior table and the test name before execution, or the executor implements the wrong order and both tests cannot pass.
- MEDIUM. `vector_json_bytes_max = 384 * 16 + 128` holds only if floats are printed as f32 shortest round-trip. Through `serde_json::Value` they print as f64 and exceed 16 characters each, so `vector_json_bytes_bound_holds` fails or the derived `max_inputs` is wrong by about 1.5x. State the serialization rule in the contract description and the Rust builder (see 11-06).
- MEDIUM. The bounds are priced from the probe's numbers, but this plan adds a sha256 of the 91 MB `.apr` at init. Phase 10 measured about 3.5 s of hashing for 0.85 GB, so expect a few hundred milliseconds here. It does not threaten the 30 s canary, but the contract's `cold_64_duration_ms_max` input is the probe's, not the endpoint's. Record `identity_ms` and let 11-12 report the delta.
- LOW. `deadline_exceeded` as 500 is defensible under R5's status list, but a client retrying a 500 will re-run the same oversized request. Consider 400 with code `deadline_exceeded` when the refusal is computed before any work, and 500 only when work was interrupted.
- LOW. The readback of `/proc/cpuinfo` for the host header runs once per process; on an x86_64 fallback host the field name differs. The plan says `unknown` elsewhere; fine.

**Suggestions.** Add a `selftest` case to `derive_bounds.py` where the response bound binds before the time bound at a high per-input speed, to prove `response_cap_bytes` is the active clamp somewhere in the table.

**Risk Assessment.** Medium. This is the plan most likely to need a mid-execution correction because of the ordering contradiction; everything else is sound.

## 11-09

**Summary.** Produces the sentence-transformers reference corpus at `max_seq_length=256` from the frozen environment, the release identity bundle with an independent checker and eight tamper controls, and real-weight parity tests including the 255/256/257 boundary and a 1,600-plus-wordpiece chunk.

**Strengths.**
- Every input hash is checked before encoding, and `vocab.txt` is verified line-by-id against `tokenizer.json`, which closes the `upstream_manifest.json` gap (it lists five files and no `vocab.txt`).
- `--no-sync --frozen` with a versions precondition is the right posture given the SUS flag on the newest sentence-transformers release; the installed 5.7.0 / 2.13.0 are present.
- The committed digest sidecar plus a CI-visible test keeps the fixture repo-owned.
- The bundle's pre-deploy versus deployed state rules match Lambda's base64 32-byte `CodeSha256`.

**Concerns.**
- MEDIUM. Byte-identical regeneration is required by `cmp`, but torch CPU reductions can differ with thread count, and `sentence-transformers` batches by sorted length. Pin `torch.set_num_threads(1)` and `torch.manual_seed(0)` in `generate` so the determinism claim does not depend on the host's core count.
- MEDIUM. The at-limit item is exactly `max_input_bytes` bytes (1 MiB by the 11-08 ceiling) and the batch-64 items are 700 words each, so the committed corpus will be several MB. That is acceptable, but state the expected size and keep the float format to a fixed number of significant digits so the file does not grow further.
- LOW. The frozen venv's tokenizers is 0.22.2 against Rust 0.23.1; RESEARCH measured equal counts on 23 inputs. The boundary items at exactly 255/256/257 tokens are the right place to catch a skew; good.
- LOW. The 12-digit scan excludes JSON files for this plan; the corpus JSON could carry a 12-digit run inside a float string, which is why. Fine, but say so in the verify comment.

**Suggestions.** Add a `schema_source: requester` round-trip test: generate the deliverable with a fake `reference_vectors.fields` mapping and assert the mapped file re-parses to the canonical vectors, so the mapping code is not only exercised on the default path.

**Risk Assessment.** Medium. Reproducibility is the one claim that can break on another machine; pinning threads removes most of it.

## 11-10

**Summary.** The deploy path: the endpoint stack finalized from the contract and the intake, a verified arm64 package with a stale-binary tell, a template-versus-contract gate with nine planted defects, a ten-rung fail-closed deploy ladder with DRY_RUN and PREFLIGHT, a readback that contains on ingress, the identity probe, the measurement driver, and an offline selftest with four induced containments.

**Strengths.**
- The ladder mirrors `llm-endpoint-deploy` (`justfile:4179-4186` refuse helper and mode refusal) and `llm-endpoint-readback` (`justfile:4941-4945` URL config, env key names only), both of which exist and are selftested.
- Two independent offline gates (scanner and synth gate) plus a live readback with containment is the strongest R4 posture in the repo.
- Cross-account callers stop the plan for the owner rather than adding a permission.
- The stale-binary tell (the `embed.load` log literal, and the absence of the probe's package name) addresses the exact failure the training README warns about (`deploy-extensions/README.md:107-111`).

**Concerns.**
- HIGH for R4 posture. The live readback accepts `policy: no-wildcard`. The stack creates no permission at all, so any statement found live is drift, including a non-wildcard `Principal: {"Service": "apigateway.amazonaws.com"}`, which is API Gateway ingress without a `*`. Make the live rule the same as the offline one: a resource policy must be absent, and any statement is a finding that contains the function. If the intake later names an explicit same-account principal, allow exactly that one.
- MEDIUM. The `InvokePolicy` output is printed on the terminal only, but the requester needs the real function ARN to bind. Plan 11-13's hand-off uses template tokens, so the ARN must travel out of band. Say where (the owner's channel) so the executor does not paste it into a committed file to "help".
- LOW. `embed-ids` refuses a different account unless `EMBED_IDS_REWRITE=1`; good. Also refuse when `AWS_REGION` differs from the recorded region.
- LOW. The synth gate imports the scanner; make sure both are run with `python3 -I` from the repo root so the import resolves without the `scripts/embed_endpoint` directory on `sys.path` being a side effect of the cwd.

**Suggestions.** Add one more induced failure to the selftest: an environment variable set beyond the closed set, since a surprise `APRENDER_EMBED_MODEL_DIR` override is the most likely console drift after a URL.

**Risk Assessment.** Medium. The design is right; the live `no-wildcard` acceptance is the one hole and is a one-line fix.

## 11-11

**Summary.** The local equivalent of the requester's acceptance: one registered check per intake check, two transports, a defect catalog with a per-check must-fail matrix, the run on our own build recorded, and the fourth Realizar-first exception row in CLAUDE.md bound by eight ledger rows.

**Strengths.**
- Set equality on ids and string equality on quotes and titles is the strongest available guard against paraphrased checks.
- The defect matrix proves the checks are not vacuous, which is what the repo's verification rules ask for.
- The claims ledger supports exactly the kinds used (`justfile:3695` lists `rust, just, python, pv, evidence`), the `LAYA_CLAIMS_TSV` override exists (`justfile:3687`) so the planted-drift negative control runs, and `just --dump` is how `just` rows are checked.
- The CLAUDE.md numeric-leak verify is a real guard against the document drifting back into quoting measurements.

**Concerns.**
- MEDIUM. The plan hard-requires `intake_complete` and so does the chain after it. R1 through R6 do not depend on the thirteen checks, but 11-12 depends on 11-11. See the cross-plan note; the fix lives in 11-12.
- MEDIUM. The ledger row `embed.claude-md.parity` with owner `aprender-embed-endpoint:test=real_weights` makes `just laya-claims-check` run `cargo test -p aprender-embed-endpoint --test real_weights -- --list`, which compiles the crate's test target. The recipe already took over 120 s at planning time; budget for more and keep the 600 s timeout note.
- LOW. The CLAUDE.md check forbids the literal values of six contract constants. If `lambda_memory_mb` is 10240, the existing Decision-model paragraph already contains "10,240 MB", but the verify scans only added lines, so it is safe. Keep it that way.
- LOW. `sed -i.bak` is correct on macOS; the verify should delete the `.bak` it creates under `/tmp`, which is harmless either way.

**Suggestions.** Register each check with a `source` field (`brief` or the copied tool file name) so the harness output shows which text each assertion was built from.

**Risk Assessment.** Medium. The harness is only as available as the brief; the documentation work is low risk and well gated.

## 11-12

**Summary.** The owner's live session: record the account, preflight, deploy with readback, identity probe, live conformance, cold/warm measurement, then make the bundle `deployed` with the live `CodeSha256` and commit the per-environment release directory.

**Strengths.**
- The 30 s canary is the only timing that gates, matching R6 and the project rule to report other timings.
- The evidence is cross-checked against the intake's function name, the contract's memory and timeout, the pre-record's apr sha256, and the bundle's digests.
- The function is left deployed for the requester's acceptance, which is what R7 needs, and the executor makes no write.

**Concerns.**
- HIGH for schedule. `conformance_live.passed is True` is required for a `verified` outcome. If the brief has not arrived, nothing in this plan can run even though deploy, readback, identity, and measurement (R1, R3, R4, R6) are fully specified. Allow `conformance_live: pending` with the reason recorded, and have 11-13 refuse to close R7 only. That decouples the measurable deliverables from the requester's latency.
- MEDIUM. The warm-sample lottery from 11-04 applies here too: a warm sample that carries `Init Duration` fails the evidence. Retry warm samples in the driver.
- LOW. The verify's identifier scan also rejects the literal `aprender-decide-weights`; fine, but name the bucket pattern in a comment so a future reader knows why.

**Suggestions.** Record the `x-aprender-embed-load` header of the identity probe so the evidence shows whether the probe itself was the cold call.

**Risk Assessment.** Medium. Technically well prepared by 11-10; the risk is the dependency on 11-11.

## 11-13

**Summary.** The hand-off with a machine-checked facts block, the phase-wide static gates with mutation adjudication, and the requester's own acceptance line recorded verbatim.

**Strengths.**
- Every number in the facts block is recomputed from its owning record; promise words and proposed dates fail the verify.
- Findings outside the phase (tape growth in `encode_texts`, `embed`, `classify`; aarch64-only parity; tokenizers skew; the shipped `tokenizer.json` truncation of 128) are named and not fixed, which is the right scope discipline.
- The gate list includes the register, README, duplicate-bin, claims ledger, binding, `pv`, include-files, and every selftest; the `bash -n` reconstruction from `just --dump` is feasible given the dump's fragment shape I observed.

**Concerns.**
- MEDIUM. `cargo mutants --in-place` mutates the working tree; an interrupted run leaves mutated source behind. The repo's CI uses it, but for an owner-run tree add a `git status --porcelain` check before and after, and refuse to record the gate if the tree is dirty afterwards.
- MEDIUM. The acceptance's `passed` rule accepts `CONFORMANCE RESULT PASS n/n` with n equal to `check_count` when the intake gave no pass line. Also require that the requester's output names the function that was invoked, otherwise a pass against another deployment is indistinguishable.
- LOW. The hand-off's permission statement uses `<account-id>` tokens, correct for a public tree, but the owner must send the real ARN privately. State the channel.

**Suggestions.** Include a one-line "how to tear down" with `just embed-destroy` and the containment semantics, since the function stays deployed after acceptance.

**Risk Assessment.** Low. Documents and gates only; the mutation run is the only step that can disturb the tree.

## Cross-plan assessment

**Dependency ordering.** The waves are mostly right: the Graviton gate (11-01, 11-04) and the owner decisions (11-02) do not wait on the requester, and the crate (11-06) only needs the core door. The one structural problem is that the live delivery of R1 through R6 (11-12) is chained behind the requester's thirteen checks through 11-11. Fix by making live conformance a `pending` outcome in 11-12 and letting 11-13 hold only R7 open.

**Security.** No public ingress is enforced offline twice and live once, with containment. The single gap is the live readback's acceptance of a non-wildcard resource policy in 11-10; align it with the offline rule (absent only). Everything else, including secrets scrubbing, identity pinning to compiled constants, and never logging request text, is specified with tests.

**Performance and the 30 s canary.** The canary risk is low: load is around 0.1 s locally, the package fetch is around 1.5 s at Lambda's observed rates, and identity hashing adds a few hundred milliseconds. The real unknown is the 64-chunk call, which the plans correctly measure before pricing. Two things would make the measurement truer to production: add identity hashing to the probe (11-01) and record `identity_ms` live (11-08, 11-12).

**Edge cases and error handling.** Two inconsistencies need fixing before execution: the bound-check order contradiction in 11-08, and the f32-through-`Value` serialization that breaks 11-08's per-vector byte bound unless 11-06 serializes typed f32 arrays. The warm-sample lottery in 11-04 and 11-12 is the one missing retry path.

**Scope.** The plans stay inside CONTEXT's scope. They are large, but each piece maps to a requirement or a project rule (selftests, claims ledger, mutation adjudication). With `inline_plan_threshold` defaulting to 2, nine plans with 2 tasks execute inline in the main context at 10k to 65k estimated tokens each; that is workable but the executor should expect long single turns on 11-06 through 11-08.

**Does the set achieve the phase goal?** Yes, with the corrections above. R1 through R6 are fully specified with executable proofs, R3's bundle and reference are reproducible and independently checked, and R7 is correctly treated as the requester's own run with a non-vacuous local equivalent. The plans I could not verify beyond their text are the requester-side items (the thirteen checks, the tool, the platform event); those are open inputs by design and the plans refuse to invent them.

---

## Consensus Summary

Both reviewers read all 13 plans and cited source. Neither is a full-weight vote on facts they did not check; the items below are marked by how many raised them. None is independently re-verified by the orchestrator yet.

### Agreed Strengths
- Measure-first ordering: Graviton gate before any product code, owner-run AWS writes, executor limited to read-only calls.
- Quote-or-unspecified intake rule for the 13 checks; no invented acceptance checks.
- Planted-violation controls on the ingress scanner, synth gate, tamper tests and defect matrix.
- Thin crate over a small additive core door; `no_grad` placement and thread-local tape facts match the code.

### Agreed Concerns (both reviewers)
1. **R4 live policy check too weak (11-10, HIGH).** Live readback accepts `no-wildcard`; any resource-policy statement (for example an API Gateway service principal) is ingress drift. Make the live rule match the offline one: absent, or exactly the one intake-named principal.
2. **Deadline / admission bounds (11-08, HIGH/MEDIUM).** Bounds extrapolate from the 64-input probe (codex: 961 inputs at 9 ms/input); the deadline guard cannot interrupt a running tokenizer or forward pass; the endpoint adds identity hashing at init that the probe does not measure. Cap admission at the measured count until a maximal-request run exists.
3. **Live evidence must not count failures (11-04, 11-12).** Codex: fast 500s and application errors can become cold-canary evidence; conformance count and cross-session mixing unchecked. Fable: warm-sample retry missing (warm sample with Init Duration fails the gate).
4. **Panic-safety of `no_grad` (11-05).** Flag is not restored on unwind; both suggest an RAII guard.
5. **Mutation scope and pre-deploy placement (11-13).** Core `embed.rs` not mutated (same as checker W6); run before the live deploy. Interrupted `cargo mutants --in-place` can leave a dirty tree.
6. **Intake and decoupling (11-03, 11-11, 11-12).** Both note R1-R6 should not wait on the requester's brief: allow `conformance_live: pending` and hold only R7 open. Codex adds that ROADMAP wave 2 blocks on wave 1, so late intake would block the independent Graviton and core-API work.
7. **Unpinned reproducibility (11-09).** Python `-I` sibling-import problem (codex reproduced), thread/seed pinning, full environment identity.

### Single-reviewer findings worth checking first
- **11-06 R5 hole (codex HIGH):** `LambdaEvent<Value>` still deserializes the outer event in `lambda_runtime` 0.13 before the handler; a failure there is an unhandled FunctionError (api_response.rs:105, requests.rs:175; serde depth limit 128). Needs a raw-payload path with bounded parsing inside the envelope boundary. This directly threatens R5.
- **11-08 ordering contradiction (fable HIGH):** must-haves say request bytes first; behavior block and test name say last.
- **11-06/11-08 f32 serialization (fable):** floats through `serde_json::Value` print as f64 and break the per-vector byte bound; serialize from `&[f32]`.
- **11-07 hostile oracle (codex HIGH):** allowed-status sets containing 200 let a removed validator stay green; use exact expected status per case.
- **11-04/11-12 deployment identity (codex HIGH):** bind the tested ZIP to the live CodeSha256 and a session id.
- **11-04 memory fallback (codex):** offered lower-tier retry has no matching probe argument.
- **11-05 test memory (fable):** real-weight comparison with tape growth can take several GB; run it under `no_grad`.

### Divergent Views
- Overall risk: codex rates 11-04/06/07/08/12/13 HIGH; fable rates nothing above MEDIUM except that 11-08 and 11-10 each carry one HIGH item. Codex found the runtime-deserialization hole fable missed; fable found the ordering and serialization defects codex missed.
- Both recommend revising before execution (codex explicitly; fable "with the corrections above").
