---
phase: 9
reviewers: [codex]
reviewed_at: 2026-10-16T01:04:45Z
plans_reviewed: [09-01-PLAN.md, 09-02-PLAN.md, 09-03-PLAN.md, 09-04-PLAN.md, 09-05-PLAN.md, 09-06-PLAN.md, 09-07-PLAN.md, 09-08-PLAN.md, 09-09-PLAN.md, 09-10-PLAN.md, 09-11-PLAN.md, 09-12-PLAN.md]
models:
  codex: "gpt-5.6-sol (reasoning=high)"
model_sources:
  codex: "pinned"
failed_lanes:
  gemini: "no such declared lane (this GSD install has no gemini lane; use --agy for Google)"
---

# Cross-AI Plan Review — Phase 9

> Single-reviewer run. `--gemini` was requested but is not a lane in this install, so it did not run.
> One vendor is not a cross-AI review: treat every finding below as unverified until checked against source.
> Codex's configured default `gpt-6.1-sol` is rejected for this ChatGPT account (HTTP 400); the run was pinned to `gpt-5.6-sol` via `--model` with no config change.

## Codex Review

## 09-01

**Summary.** Establishes the conformance contract, committed baseline record, Rust record reader, CI fragment, and reusable probe harness.

**Strengths.**

- The contract is grounded in real request types: `max_tokens` currently has no alias and `ChatMessage.content` is required in `crates/aprender-serve/src/api/mod_create_demo.rs:62-71,205-230`.
- The reader’s non-recursive `conformance-*.json` boundary is well designed and explicitly mutation-tested in `09-01-PLAN.md:292-319`.
- CI and contract binding are wired into existing enforcement rather than inventing a parallel mechanism. New contracts must have zero binding findings according to `scripts/check_contract_test_binding.sh:37-43`, and compound `&&` test commands are supported by `crates/aprender-contracts/src/lint/strict_test_binding.rs:270-284`.

**Concerns.**

- **MEDIUM — CPU-part contract row is not actually enforced.** Rows 008–012 are said to bind to the reader because it verifies each as `KNOWN LIMITATION` (`09-01-PLAN.md:254-263`), but the reader’s allowed limitation set omits `cpu_part` (`09-01-PLAN.md:303-307`). The probe’s default list also omits it (`09-01-PLAN.md:375-383`). Row 012 can therefore cite the reader without that reader testing its assertion.
- **LOW — Baseline expectations are deliberately exact but brittle.** The reader hard-codes the four-cross set at `09-01-PLAN.md:302-305`; this is appropriate for a pinned baseline, but any release-specific drift must be represented as a different profile rather than editing the baseline.

**Suggestions.**

- Either add `cpu_part` to the fixed-profile limitation set and record it consistently, or bind row 012 to a separate evidence-reader test that checks the committed live record’s `cpu_part`.
- Keep the baseline immutable and add explicit tests that reject duplicate probe names and missing expected probes, not only unexpected crosses.

**Risk Assessment: MEDIUM.**

## 09-02

**Summary.** Implements tolerant assistant-content deserialization and the `max_completion_tokens` alias, with tests and two upstream PRs.

**Strengths.**

- The proposed `String` plus `deserialize_with` approach preserves serialization and existing readers. This directly addresses the required field at `crates/aprender-serve/src/api/mod_create_demo.rs:205-230`.
- Tests cover absent, null, invalid numeric content, byte identity, and alias collision semantics (`09-02-PLAN.md:136-140,204-207`).
- Treating the nested OpenAI error envelope as a known limitation is justified: the current response type is `error: String` in `crates/aprender-serve/src/api/types.rs:141-146`, and the auth contract explicitly pins that shape at `contracts/apr-serve-api-key-auth-v1.yaml:70-76`.

**Concerns.**

- **HIGH — The two upstream commits are not independently cherry-pickable as planned.** Both tasks edit the phase-only contract, and Task 2 also edits the test file created by Task 1 (`09-02-PLAN.md:150-175,195-215`). The checkpoint then proposes cherry-picking exactly one fork commit into each branch cut from `upstream/main` (`09-02-PLAN.md:242-245`). Since the new contract comes from 09-01 and is not on upstream main, those cherry-picks will conflict or cease to be self-contained. The alias commit likewise depends on the earlier-created shared test file.
- **MEDIUM — The deserializer broadens every role, not only assistant tool-use messages.** The plan explicitly changes missing/null content on “any message” to empty string (`09-02-PLAN.md:161-164`), while the requirement is specifically assistant tool-call history. Today the field is required for all roles (`mod_create_demo.rs:208-218`), so malformed user/system/tool messages would also become silently accepted.

**Suggestions.**

- Build each upstream PR independently inside its `upstream/main` worktree, with a dedicated upstream test file. Cherry-pick those upstream commits back onto the phase branch.
- Keep contract-binding changes in separate phase-local commits.
- Either implement role-aware validation or explicitly approve and test the broader all-role compatibility change.

**Risk Assessment: HIGH.**

## 09-03

**Summary.** Adds an authenticated GGUF-router builder seam and production-realistic `oneshot` tests.

**Strengths.**

- The corrected fix site is accurate. Both dense CPU GGUF and Qwen3.5 end in `serve_router` (`crates/apr-cli/src/commands/serve/server.rs:67-95,110-117`), where the realizar router is currently created without an auth layer (`server.rs:273-291`).
- It correctly avoids `routes.rs::create_router`, which is the separate SafeTensors router and already delegates to an authenticated builder (`crates/apr-cli/src/commands/serve/routes.rs:331-374`).
- The new test improves materially on the existing generic middleware test, which only constructs a synthetic router (`crates/apr-cli/tests/falsify_auth_001.rs:7-13,29-45`).
- Gating `/health` is consistent with the existing contract (`contracts/apr-serve-api-key-auth-v1.yaml:82-91`).

**Concerns.**

- **HIGH — The upstream cherry-pick has the same packaging defect as 09-02.** The planned commit modifies both `server.rs` and the phase-only conformance contract (`09-03-PLAN.md:164-190`), then is cherry-picked as a single commit onto `upstream/main` (`09-03-PLAN.md:250-254`). The contract file will not exist on that base.
- **MEDIUM — The scope remains narrower than the auth contract.** The plan intentionally leaves other direct realizar-router call sites ungated (`09-03-PLAN.md:175-178`), while the existing contract says every route on every router builder is gated (`contracts/apr-serve-api-key-auth-v1.yaml:42-51`). This is acceptable for the CPU/Qwen phase boundary only if the contract drift is explicitly resolved, not merely mentioned.
- **LOW — The existing contract contains stale claims.** It still names a nonexistent `--auth-disabled` path and incomplete router references (`contracts/apr-serve-api-key-auth-v1.yaml:26-34,53-61`).

**Suggestions.**

- Produce an upstream-only code-and-test commit from `upstream/main`; bind the phase contract in a separate local commit.
- Amend the auth contract’s scope or add explicit exception rows for deferred GPU paths.
- Preserve the deployment-level fail-closed check because `AuthGate::from_env()` disables auth on missing or malformed configuration (`crates/apr-cli/src/commands/serve/auth.rs:49-74`).

**Risk Assessment: HIGH.**

## 09-04

**Summary.** Builds the pinned release image and hardens the entrypoint against missing auth or incorrect model weights.

**Strengths.**

- The entrypoint refusals are necessary because malformed or absent auth configuration currently starts an open server (`crates/apr-cli/src/commands/serve/auth.rs:49-74`).
- Release tarball verification, an explicit tarball pin, image labels, runtime identity, and a digest-pinned Lambda Web Adapter provide strong supply-chain evidence (`09-04-PLAN.md:162-217`).
- Root-anchored ignores for downloaded binaries and live IDs are appropriate.

**Concerns.**

- **HIGH — The acceptance criteria are internally impossible.** The action requires `MODEL_LOCAL=/tmp/model.gguf` in the Dockerfile (`09-04-PLAN.md:172-183`), while acceptance says the Dockerfile must not contain `.gguf` (`09-04-PLAN.md:229-234`). A correct implementation necessarily fails its own gate.
- **LOW — The LWA digest is duplicated between `pin.env` and the Dockerfile.** The plan acknowledges this, but no automated comparison is specified between the two copies.

**Suggestions.**

- Replace “does not contain `.gguf`” with a precise check that no GGUF is copied or added to the image, such as rejecting `COPY/ADD .*\.gguf`.
- Add a test that the Dockerfile’s LWA digest equals `LWA_IMAGE_DIGEST` from `pin.env`.

**Risk Assessment: HIGH.**

## 09-05

**Summary.** Re-probes the release image and proves the three fixes together on an interim upstream-main build.

**Strengths.**

- Release and interim records are clearly separated by profile, label, version identity, and filenames (`09-05-PLAN.md:135-144,190-206`).
- The baseline is tested under the intended memory cap, and unexpected drift is saved outside the conformance glob instead of normalized away (`09-05-PLAN.md:120-144`).
- The interim auth proof exercises the actual server and correctly distinguishes 401/401/200 behavior (`09-05-PLAN.md:185-202`).

**Concerns.**

- **HIGH — It inherits the invalid upstream-branch construction from 09-02/09-03.** The interim worktree cherry-picks the three fix commits and assumes they were independently verified against upstream main (`09-05-PLAN.md:168-177`). As written, those commits include phase-contract edits that cannot cleanly apply to upstream main.
- **MEDIUM — Fixed-profile record validation inherits the missing `cpu_part` enforcement from 09-01.**
- **LOW — Retaining an entire release `target/` for 09-12 is operationally expensive, though cleanup is planned.**

**Suggestions.**

- Make 09-05 depend on corrected upstream-only commits, not the phase-branch commits.
- Record the upstream base SHA and each applied patch ID, then verify `git diff upstream/main...HEAD` contains only the intended source and test changes.
- Add a cleanup checkpoint if 09-12 is paused or skipped.

**Risk Assessment: HIGH until the upstream commit choreography is corrected.**

## 09-06

**Summary.** Adds the AWS lifecycle, proof, benchmark, URL, and teardown recipes.

**Strengths.**

- The most important security sequence is explicit: deployed no-key 401, digest freshness, URL creation, immediate HTTPS recheck, and automatic URL removal on failure (`09-06-PLAN.md:241-260`).
- The benchmark records both client wall time and Lambda billed time, avoiding the known timing confusion.
- Shell linting, AWS-call recording, fixture-driven failure cases, and URL/key redaction are unusually thorough.

**Concerns.**

- **HIGH — Teardown can destroy an image repository shared across environments.** `llm-endpoint-up` uses one ECR repository named `aprender-llm-endpoint` for every environment (`09-06-PLAN.md:219-226`), but teardown deletes that repository with `--force` (`09-06-PLAN.md:267-273`). Tearing down dev could delete prod tags or other retained pins. The existing laya precedent intentionally contains first and prints destructive cleanup for a human instead of executing it (`justfile:2738-2787`).
- **MEDIUM — The DRY_RUN contract is contradictory.** The deploy ladder says S3 metadata and pushed digest are checked before DRY_RUN (`09-06-PLAN.md:145-157`), while the self-test says those checks are skipped before the first AWS call (`09-06-PLAN.md:181-188`). Consequently `DRY-RUN OK` does not prove all advertised rungs.
- **MEDIUM — This is an oversized implementation unit.** Eleven recipes, an AWS fixture framework, benchmarking, URL lifecycle, teardown, and probe wrapping in one 95k-token plan raises defect and review risk.

**Suggestions.**

- Make ECR repositories environment-specific, or delete only the exact phase-owned digest/tag. Delete the repository only after proving it is empty and tagged for the chosen environment.
- Define two explicit modes: offline static validation with zero AWS calls, and connected read-only preflight that checks STS, S3 metadata, and ECR digest.
- Split benchmark/probe packaging from AWS resource lifecycle.

**Risk Assessment: HIGH.**

## 09-07

**Summary.** Human-gated live deployment, private `lambda:Invoke` proof, measurements, and evidence production.

**Strengths.**

- The D-15 checkpoint clearly enumerates account, cost, pin, weights source, concurrency, and destructive choices before AWS mutation (`09-07-PLAN.md:128-158`).
- No Function URL is created in this plan; the live function is measured first through IAM-authenticated invocation.
- Evidence requires both wall and billed duration, model identity, CPU part, cost, and explicit spike-context labeling (`09-07-PLAN.md:280-297`).

**Concerns.**

- **MEDIUM — Artifact naming is hard-coded to v0.71.4 despite an allowed fallback.** The plan allows the checkpoint to select a fallback tag (`09-07-PLAN.md:34,158`) but declares and verifies `conformance-v0.71.4-deployed-invoke.json` (`09-07-PLAN.md:8-10,286-295`).
- **MEDIUM — The checkpoint claims the SDK matrix is already complete, but this plan does not depend on 09-12.** The statement appears at `09-07-PLAN.md:131`; 09-12 is a separate wave-5 plan. The “everything offline-provable is done” claim is therefore not guaranteed by the dependency graph.
- **LOW — Cost enforcement is primarily post hoc.** The verifier checks `cost_spent_usd <= cost_ceiling_usd` after measurement (`09-07-PLAN.md:283-285`) rather than refusing before each sample when the remaining ceiling is insufficient.

**Suggestions.**

- Use `conformance-${APR_TAG}-deployed-invoke.json` throughout.
- Either depend on 09-12 or remove the SDK-matrix claim from the go/no-go prerequisites.
- Add a conservative pre-run estimate and per-sample budget guard.

**Risk Assessment: MEDIUM.**

## 09-08

**Summary.** Waits for a fixed upstream release, re-pins the deployment, exposes authenticated HTTPS, re-runs conformance, and proves the SDK path.

**Strengths.**

- The release precondition is appropriately strict: all three PR merge commits must be ancestors of the release, and partial releases halt (`09-08-PLAN.md:114-148`).
- URL creation remains gated by a fresh no-key 401 for the live image digest (`09-08-PLAN.md:157-175`).
- The HTTPS auth table checks no key, wrong key, right key, `/health`, and `/v1/models`, with tracked URL redaction (`09-08-PLAN.md:181-215`).

**Concerns.**

- **HIGH — The SDK driver cannot be built using `Cargo.branch.toml`.** The plan reuses a generated alternate manifest (`09-08-PLAN.md:229-237`). Cargo rejects `--manifest-path` unless the file is literally named `Cargo.toml`; a read-only probe returned `error: the manifest-path must be a path to a Cargo.toml file`. This originates in 09-12.
- **MEDIUM — Two separate one-tool runs do not prove one two-tool agent turn.** The weather and addition tasks are executed independently (`09-08-PLAN.md:237-245`), yet the done condition describes SC2 as proven. The driver does advertise both tools (`.claude/.../agent-loop/src/main.rs:25-67`), but the record does not prove both were exercised within one conversation.
- **MEDIUM — Function URL redaction does not cover platform runtime logs.** The pmcp.run service logs the complete resolved endpoint at `durable-agent-lambda/src/llm/service.rs:228-235`. That will place the Function URL in CloudWatch even though tracked evidence is redacted, increasing the cost-amplification exposure identified by the plan.

**Suggestions.**

- Generate a temporary driver directory whose manifest is actually named `Cargo.toml`, or use a normal Cargo workspace/patch mechanism.
- Add one combined SDK task that invokes both `get_weather` and `add` in the same `AgentEngine` run.
- Redact or hash endpoint hosts in generic platform logging, or explicitly accept the CloudWatch exposure and compensate with concurrency and invocation alarms.

**Risk Assessment: HIGH.**

## 09-09

**Summary.** Adds generic per-provider timeouts to pmcp.run and writes the cross-team implementation brief.

**Strengths.**

- The proposed field fits the existing data-driven model at `pmcp-run/amplify/data/resource.ts:1450-1469`.
- The loader integration is correctly generic: current provider mapping is centralized at `config/loader.rs:919-969`.
- The 148-second bound is correctly derived from the existing constants and staleness invariant (`handler/iteration.rs:52-72,2935-2982`).
- Human approval before pushing the two sibling-repository branches is appropriate.

**Concerns.**

- **HIGH — The full-suite verification command targets a nonexistent library.** The plan runs `cargo test --manifest-path .../Cargo.toml --lib` at `09-09-PLAN.md:163`. That package declares binary targets only (`durable-agent-lambda/Cargo.toml:1-26`) and has no `src/lib.rs`, so the command will fail regardless of implementation quality.
- **MEDIUM — Branches are cut from a local `main` without requiring it to match the intended remote base.** The precondition verifies only that `main` exists (`09-09-PLAN.md:132-134`), not that it is current or that its SHA is recorded.
- **LOW — Malformed `timeoutSecs` silently falls back to 120.** That is safe for liveness but hides bad operator data; at least a warning would improve auditability.

**Suggestions.**

- Run the whole primary binary suite with `cargo test --manifest-path .../Cargo.toml --bin durable-agent-lambda`, or omit `--lib`.
- Fetch and create the worktree from a recorded `origin/main` SHA.
- Test zero, negative/invalid, maximum, and overflow values; log a structured warning for malformed data.

**Risk Assessment: HIGH.**

## 09-10

**Summary.** Performs the central pmcp.run end-to-end turn and records the phase’s final evidence.

**Strengths.**

- The row verification aligns with the real model: the DDB row stores only `secretKeyName`, never the key value (`resource.ts:1448-1465`; `config/loader.rs:914-965`).
- The test task genuinely asks for weather and addition in one conversation (`09-10-PLAN.md:150-159`).
- Max-token bounds and configuration-only switch-back are explicitly covered.

**Concerns.**

- **HIGH — The verifier permits two calls to the same tool.** It accepts either both named tools or merely `len(tool_calls) >= 2` (`09-10-PLAN.md:174`). Two duplicate weather calls could therefore satisfy the final SC1 gate.
- **HIGH — The evidence schema omits core SC1 wire assertions.** The action says to collect `finish_reason: tool_calls`, parseable arguments, and accepted `role: tool` results (`09-10-PLAN.md:155-159`), but the JSON schema records only tool names, timing, and a final excerpt (`09-10-PLAN.md:162-168`). The automated verifier checks none of those protocol properties.
- **MEDIUM — Key claims are self-declared booleans.** `transformer_unchanged`, `loader_unchanged_for_this_row`, and `under_120s_each_call` are written into the record and trusted. The verifier does not derive timeout compliance from `platform_latency_ms`, nor verify source SHAs.
- **MEDIUM — REPORT correlation is weak.** The verification accepts any two REPORT lines after the start timestamp (`09-10-PLAN.md:176-177`), which can include pre-warm or unrelated calls.

**Suggestions.**

- Require `{'get_weather','add'} <= set(tool_calls)` without the fallback.
- Record per iteration: `finish_reason`, call ID, tool name, raw arguments, parsed-arguments success, matching tool-result ID, and final grounding assertions.
- Compute `under_120s_each_call` from recorded latency values.
- Record platform commit SHA and a clean-diff check for transformer/loader claims.
- Correlate endpoint logs using a narrow timestamp window and reserved concurrency one, or capture explicit Lambda request IDs where possible.

**Risk Assessment: HIGH.**

## 09-11

**Summary.** Creates the ladder reader, CodeBuild weight staging, IAM policy files, and operator runbook.

**Strengths.**

- The model pin really appears in two ladder locations—the `known_red` and inventory entries (`contracts/model-capability-ladder-v1.yaml:111-118,265-272`)—so collecting distinct hashes is the correct design.
- Moving IAM JSON out of shell quoting materially reduces script risk.
- The buildspec receives the hash from the environment and verifies before upload (`09-11-PLAN.md:153-157`).
- IAM intent is narrow: one weights bucket plus logging (`09-11-PLAN.md:143-151`).

**Concerns.**

- **MEDIUM — The advertised ambiguity guard is not mutation-tested.** The reader promises exit 4 when one rung has two distinct hashes (`09-11-PLAN.md:159-166`), but verification only changes all occurrences to the same replacement and tests an unknown rung (`09-11-PLAN.md:168-174`). `AMBIGUOUS PIN` could be broken and the plan would still pass.
- **LOW — PyYAML is an undeclared runtime dependency.** The plan relies on it being importable on this host (`09-11-PLAN.md:104-107,159`) rather than pinning or eliminating it.

**Suggestions.**

- Add a third mutation that changes only one of the two 0.8B occurrences and asserts exit 4 plus `AMBIGUOUS PIN`.
- Prefer an existing repository-owned parser or clearly document PyYAML as a required operator dependency with a version check.
- Split bucket and object actions into separate IAM statements for clearer least-privilege review.

**Risk Assessment: MEDIUM.**

## 09-12

**Summary.** Implements the pmcp-agent fix and builds a four-cell SDK/server compatibility matrix with a negative control.

**Strengths.**

- The SDK edit targets the real defect: `message_to_openai` currently omits content on `ToolUse` (`rust-mcp-sdk/.../openai_compat.rs:166-190`), while the platform transformer already emits empty content (`pmcp-run/.../openai.rs:220-270`).
- The matrix is strong experimental design: server-only fix, SDK-only fix, both fixes, and the unchanged 422 control (`09-12-PLAN.md:200-227`).
- The driver uses the real `OpenAiCompatSource` and `AgentEngine`, and advertises two concrete tools (`.claude/.../agent-loop/src/main.rs:1-6,25-67`).
- Loopback binding, cleanup, identity checks, and record redaction are explicit.

**Concerns.**

- **HIGH — `Cargo.branch.toml` is not a usable Cargo manifest path.** The plan copies the manifest to that name and invokes `cargo build --manifest-path .../Cargo.branch.toml` (`09-12-PLAN.md:151-162`). Cargo requires the manifest filename to be `Cargo.toml`; this blocks the branch-driver build and consequently 09-08.
- **MEDIUM — The driver falls back to a hard-coded key.** The plan preserves the spike literal when `APR_PROBE_KEY` is absent (`09-12-PLAN.md:151-153`); the current literal is at `.claude/.../agent-loop/src/main.rs:130`. A production-oriented probe should fail closed rather than silently use a predictable value.
- **MEDIUM — Matrix success exercises only `get_weather`.** Each matrix cell uses the weather-only task (`09-12-PLAN.md:200-210`). This proves the tool-result continuation defect but not a same-run two-tool workflow.

**Suggestions.**

- Create two generated driver directories, each containing a real `Cargo.toml`, or use a Cargo `[patch]`/workspace override.
- Require `APR_PROBE_KEY`; allow a literal only behind an explicit `--allow-test-key` switch restricted to loopback.
- Keep the weather matrix for isolating the 422 defect, then add one combined branch×fixed-server run invoking both advertised tools.

**Risk Assessment: HIGH.**

# Cross-plan assessment

**Overall verdict: REVISE BEFORE EXECUTION.**

The architecture is well researched, the security sequencing is strong, and the plans trace real code paths correctly. However, the plan set is not executable as written because of five blocking defects:

1. Reconstruct 09-02 and 09-03’s upstream commits independently from `upstream/main`; keep phase-contract edits separate.
2. Fix 09-04’s contradictory Dockerfile acceptance test.
3. Replace the invalid `Cargo.branch.toml` mechanism in 09-12 and 09-08.
4. Replace 09-09’s nonexistent `--lib` test target.
5. Strengthen 09-10 so the final SC1 record proves both distinct tools, finish reason, argument parsing, tool-result acceptance, and derived timeout compliance.

Before live deployment, also make ECR teardown environment-safe, make 09-07 artifact names tag-dependent, close the `cpu_part` contract gap, and address the Function URL being logged by pmcp.run’s current endpoint logging.

---

## Consensus Summary

Only one grounded reviewer ran, so there is no cross-reviewer consensus. Codex's verdict is **REVISE BEFORE EXECUTION**
(per-plan risk: HIGH on 09-02..09-06, 09-08..09-10, 09-12; MEDIUM on 09-01, 09-07, 09-11). Its five blocking claims:

1. 09-02/09-03 upstream commits must be rebuilt independently from `upstream/main`, separate from phase-contract edits.
2. 09-04's Dockerfile acceptance test contradicts itself.
3. 09-12 and 09-08 rely on an invalid `Cargo.branch.toml` mechanism.
4. 09-09 names a nonexistent `--lib` test target.
5. 09-10 does not prove both distinct tools, finish reason, argument parsing, tool-result acceptance and timeout compliance.

Also flagged before live deploy: environment-safe ECR teardown, tag-dependent artifact names in 09-07, the `cpu_part` contract gap, and the Function URL appearing in pmcp.run endpoint logs.

### Agreed Strengths / Agreed Concerns
None computable from one reviewer.

### Divergent Views
None. Re-run with `--agy` or `--claude`/fable for a second opinion, and verify the five blockers against source first.
