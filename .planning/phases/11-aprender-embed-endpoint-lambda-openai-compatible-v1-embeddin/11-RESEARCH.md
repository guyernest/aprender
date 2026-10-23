# Phase 11: aprender-embed-endpoint Lambda - Research

**Researched:** 2026-10-23
**Domain:** IAM-invoke-only AWS Lambda serving an OpenAI-shaped `POST /v1/embeddings` over the in-tree `aprender-core::setfit` MiniLM encoder
**Confidence:** HIGH on tree facts and the local (M4 Pro) measurements; MEDIUM on Lambda behaviour; LOW on anything that depends on the requester's 13 checks (not in hand)

<user_constraints>
## User Constraints (from CONTEXT.md)

Copied verbatim from `11-CONTEXT.md`. CONTEXT.md has no `## Decisions` / `## Claude's Discretion` / `## Deferred Ideas` headings, so the nearest sections are reproduced under those roles. The planner must treat "Locked requirements" as locked decisions.

### Locked Decisions (CONTEXT "Locked requirements (from the requester; verbatim intent)")
- R1 Lambda `aprender-embed-endpoint-<stage>` serving POST /v1/embeddings: API Gateway v2 event in, OpenAI-shaped response out, for sentence-transformers/all-MiniLM-L6-v2 (384-d, mean pooling, L2).
- R2 Server-side truncation: full encoded sequence <= 256 tokens incl. [CLS]/[SEP]. A 1,600+ wordpiece chunk returns 200, never panics, never 5xx. Build-time and query-time vectors identical.
- R3 Release identity bundle per release: release tag, deployed CodeSha256, sha256 of .apr, safetensors, tokenizer.json, vocab.txt; HF revision; encoder config; `aprender-embed-reference-vectors.json` computed with sentence-transformers at max_seq_length=256.
- R4 No public ingress: no Function URL, API Gateway or load balancer; resource policy has no `*` principal; access by IAM lambda:InvokeFunction only.
- R5 Never an unhandled FunctionError: malformed input gets the OpenAI error envelope with 400, 413, 429 or 500.
- R6 Measure and REPORT (not promise): memory size and timeout (timeout >= 30 s), cold and warm latency for 1 input and for 64 realistic chunks, whether provisioned concurrency is needed. Cold start must answer a one-input canary within 30 s.
- R7 Acceptance: requester runs `just aprender-embed-conformance lambda://<function-name>/v1/embeddings <reference-vectors.json>` -> `CONFORMANCE RESULT PASS 13/13`. We run the equivalent on our own build first.
- Deliverables per environment: function name + region, release bundle, reference-vectors file + sha256, deployment-limit numbers.
- Out of scope: multilingual/larger encoders, Cohere, re-embedding tooling. Nothing changes in the pmcp.run repo. English-only, 384-d accepted.

### Findings that shape the plan (CONTEXT, "verified in tree 2026-10-23") - treated as constraints
- Engine exists: `aprender-core::setfit` = native all-MiniLM-L6-v2, pinned HF revision (setfit/import.rs:61), MAX_SEQUENCE_LENGTH=256 (setfit/tokenizer.rs:52) with TruncationFact, no-panic `SetFitArtifact::embed` (artifact.rs:1875).
- aprender-serve `/v1/embeddings` is NOT a zeros stub any more but is the decoder/GGUF path: not reusable.
- `aprender-mcp-setfit-lambda` is a pmcp loopback proxy, not an API-GW-v2 handler. Write a direct lambda_runtime handler; reuse model resolution only.
- R4 conflicts with the `cargo pmcp deploy` path (creates ingress). Deploy via CDK/aws under `deploy-extensions/` (training stack style), IAM-invoke only.
- Phase 10 pattern to follow: thin per-server crate over shared library, fail-closed deploy recipes in justfile, owner-run live deploy, identity probe, evidence JSON, claims ledger.

### Claude's Discretion (CONTEXT "Open / blocked inputs" and "Project rules to honor")
- The requester's brief with the thirteen conformance checks, and the `aprender-embed-conformance` tool (in the pmcp.run repo): not in hand. Plans must include a checkpoint to obtain them and must not invent the 13 checks.
- Live deploy needs owner AWS credentials (profile ze-kasher-dev); live steps are owner-run or checkpointed.
- Reference vectors need Python sentence-transformers + HF download (back-office, not served).
- Target date: not yet set; to be set after the cold-start measurement.
- Project rules: justfile for scripts; strict clippy per touched crate; `pv validate` for any contract (new contract for the endpoint boundary); no unwrap; no new .sh without need; pin binaries; measure before reporting; report timings, do not gate on them unless the requester's numbers say so.

### Deferred Ideas (OUT OF SCOPE)
- Multilingual/larger encoders, Cohere compatibility, re-embedding tooling (CONTEXT "Out of scope"). Nothing changes in the pmcp.run repo.
</user_constraints>

<phase_requirements>
## Phase Requirements

REQUIREMENTS.md has no Phase 11 IDs. The planner should use the CONTEXT labels R1-R7 as the requirement IDs.

| ID | Description | Research Support |
|----|-------------|------------------|
| R1 | Lambda `aprender-embed-endpoint-<stage>`, API-GW-v2 event in, OpenAI shape out | Direct `lambda_runtime` handler on `serde_json::Value` (Pattern 1); OpenAI request/response fields (Common Pitfalls 8-10); platform event fixture `deploy/llm-endpoint/events/apigw-v2-no-auth.json` |
| R2 | <= 256 tokens incl. specials, never 5xx on a 1,600+ wordpiece chunk, build/query identical | `MiniLmTokenizer` truncation (tokenizer.rs:292-301), measured: 1,472- and 147,002-wordpiece inputs truncated to seq 256, delta vs sentence-transformers <= 3.3e-7 |
| R3 | Release identity bundle incl. vocab.txt hash and reference vectors | sha256s derived this session (Identity table); vocab.txt is NOT emitted by any Rust/packer path, derive from the pinned HF snapshot; reference-vector recipe (Pattern 5) |
| R4 | No public ingress, no `*` principal | CDK with no `FunctionUrl`/API/LB; readback recipe pattern from `llm-endpoint-readback` (Pattern 4) |
| R5 | Never an unhandled FunctionError | `lambda_runtime` 0.13 turns a panic into `function-error-type: unhandled`; catch panics ourselves; response-size and OOM/timeout are the other FunctionError sources (Pitfalls 2-5) |
| R6 | Measure and report cold/warm latency, memory, timeout, PC need | Local measurements (Measured Facts); the Lambda numbers are an Open Question that Wave 0 must close with a real invoke |
| R7 | 13/13 on the requester's conformance run | 13 checks are an OPEN INPUT, not guessed here (Open Questions 1) |
</phase_requirements>

## Summary

The engine is `aprender::setfit::SetFitMiniLm` (alias of `SetFitBert`), loaded from a directory with `from_pretrained_dir`, and it already gives an unconditional 256-token cut, mean pooling and L2 normalisation. Measured on this M4 Pro against sentence-transformers 5.7.0 / torch 2.13.0 at `max_seq_length=256`, the Rust vectors differ by at most 3.3e-7 per element across 23 inputs (empty string, unicode, 233-, 1,472- and 147,002-wordpiece texts), against a contract tolerance of 3.73762473e-05 for real-weight embeddings. The statement "SetFitArtifact::embed" in CONTEXT is imprecise: the method is `VerifiedSetFitModel::embed` (artifact.rs:1864), and it needs a `setfit-apr-v1` artifact, which **requires a classifier head with at least two labels and six head probes**. A head-less MiniLM cannot be a `setfit-apr-v1` artifact. The right shape for this endpoint is the directory loader (`full_model.apr` produced by `apr import --arch bert`, plus `config.json`, `modules.json`, `1_Pooling/config.json`, `tokenizer.json`), which is exactly what `just build-trainer-asset` already copies into a Lambda zip.

Two defects the planner must design around, both measured here. (1) **Memory leak by construction**: `encode_texts`, `embed` and `classify` record every op on a thread-local autograd tape because the loaded parameters have `requires_grad` set. 64 sequential single-text encodes at 256 tokens reached **26.9 GB peak RSS**; a single 64-wide call was killed at 10.3 GB RSS / 76 GB footprint. Wrapping the call in `aprender::autograd::no_grad` held the tape at 0 ops and peak RSS at **3.47 GB**, with output **bit-identical** to the grad-mode output. A warm Lambda environment that served many requests in grad mode would therefore OOM; the endpoint must run under `no_grad`. (2) **CPU time**: 64 inputs at the 256 cap took **5.1-5.5 s** on 14 M4 Pro cores (no_grad, micro-batch 4-16). Lambda gives at most 6 vCPU, and the Phase 8 decide server measured 12 ms/token for a model ~37x larger, so the Lambda figure is bracketed from about 6 s to about 35 s and **cannot be settled without a real Graviton invoke**. The 30 s timeout requirement in R6 and the 64-chunk measurement are the schedule's main risk.

Four plan-shaping decisions need the owner: the deployment-unit register in `monorepo_invariants.rs` grows 9 to 10 for any new crate with a bin (a `blocking-human` checkpoint by that test's own text); a small additive public API in `aprender-core` is needed because token counts and truncation flags are not reachable from outside the crate without the test-only `conformance-fixtures` feature; the platform's `lambda://` allowlist has been documented as accepting only `aprender-llm-` and `pmcp-llm-` names, and `aprender-embed-endpoint-` is neither; and the 13 conformance checks are still unknown.

**Primary recommendation:** Ship one new `publish = false` crate (library plus `bootstrap` bin) that loads `SetFitMiniLm::from_pretrained_dir("/var/task/model")` at start, answers every call, including a failed model load, with an API-GW-v2 response, runs encodes in micro-batches of 8 inside `spawn_blocking` + `no_grad`, deploys through a new CDK stack in `deploy-extensions/` with no URL/API/LB and no resource policy, and treats the Graviton latency measurement as the first Wave-0 gate.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Invocation auth (who may call) | IAM (caller identity policy, `lambda:InvokeFunction`) | - | R4: no Function URL, no API Gateway, no resource policy; same-account callers need only an identity policy |
| Event parsing, routing, OpenAI envelope, bounds | Lambda handler (our crate) | - | The platform sends an API-GW-v2 shaped event itself (Phase 9 notes); nothing in front of the function parses it |
| Tokenization and 256-token cut | `aprender-core::setfit` (`MiniLmTokenizer`) | - | One implementation per operation (OPS-03); the tokenizer is sealed on purpose |
| Encoder forward, pooling, L2 | `aprender-core::setfit` (`BertSentenceEncoder`) | `aprender-compute` GEMM | Only parity-proven implementation; serving a second port through `aprender-serve` is forbidden by the CLAUDE.md exception rows |
| Model bytes at runtime | Lambda zip package (`/var/task/model`) | - | 90.9 MB `.apr` plus 0.5 MB of JSON fits the 250 MB unzipped quota; no S3 read, no S3 IAM grant, and CodeSha256 covers the model |
| Release identity and reference vectors | Back-office (justfile + uv project under `scripts/`) | Owner-run AWS readback | Python never runs in the served path |
| Infrastructure definition | CDK app `deploy-extensions/` | `aws` CLI readback recipes | Matches `deploy-training`; `cargo pmcp deploy` creates ingress and is excluded |
| Acceptance | Requester's `aprender-embed-conformance` (pmcp.run repo) | Our own equivalent run first | Checks are unknown; an equivalent can only be built after the brief arrives |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `aprender-core` (lib `aprender`) feature `setfit` | workspace 0.69.3 | Tokenizer, encoder, pooling, L2 | The only parity-proven MiniLM path in the tree [VERIFIED: crates/aprender-core/Cargo.toml features, `crates/aprender-mcp-setfit/Cargo.toml:29`] |
| `lambda_runtime` | 0.13.0 (already locked) | Direct Lambda custom-runtime handler | Already used directly by `aprender-setfit-train-lambda` (`use lambda_runtime::{service_fn, Error, LambdaEvent};` at `src/bin/aprender-setfit-trainer.rs:47`) [VERIFIED: Cargo.lock `name = "lambda_runtime"` / `version = "0.13.0"`] |
| `serde_json` | workspace "1.0" | Event is `serde_json::Value`, never a typed deserialization that can fail | Removes the "event does not deserialize" FunctionError class |
| `tokio` | workspace 1 | `spawn_blocking` for CPU work | Same pattern as `aprender-mcp-setfit/src/lib.rs:214` |
| `sha2`, `base64` | workspace (`base64 = "0.22"`) | Identity check, `encoding_format=base64` | Already in the dependency graph |
| `tokenizers` | 0.23.1 (workspace pin, via `setfit`) | Reached only through `SetFitMiniLm` | Rust is the pinned 0.23.1; the Python reference env has 0.22.2 (see Pitfall 9) |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `sentence-transformers` | 5.7.0 (exact, `scripts/setfit_fixtures/uv.lock`) | Reference vectors | Back-office only; reuse the committed lock, do not float |
| `torch` | 2.13.0 (exact) | Backend for the above | Same |
| `transformers` | 4.57.6 installed, `>=4.41.0,<5` | Constraint | Same |
| `aws-cdk-lib` | `^2.150.0` (package.json) | New stack | Same app as the training stack |
| `cargo-zigbuild` | 0.23.4 installed | aarch64 cross build, glibc floor `.2.34` | `just build-trainer-arm64` uses it; decide recipes pin `{{target}}.2.34` |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Directory loader (`full_model.apr` + JSON) | `setfit-apr-v1` artifact with a dummy 2-label head | Needs `aprender-train`'s writer, six head probes and a fake head that the bundle would then have to explain; adds a train-side dependency for no served benefit |
| Model in the zip | S3 pull at cold start (decide-lambda `s3.rs`) | S3 adds a GetObject grant, a hash-pin env contract, a download deadline derived from live samples (10,739 ms for 0.85 GB) and ~80-95 MB/s per environment. For 91 MB it buys nothing and a zip keeps CodeSha256 = model identity |
| New direct handler | `lambda_http` `Request` type | `lambda_http` 0.13 deserializes only API-GW/ALB shapes; an event it cannot recognise becomes a runtime error, i.e. a FunctionError. Use `serde_json::Value` |
| Embedded `include_bytes!` | Files beside `bootstrap` | `from_pretrained_dir` is path based (mod.rs:313); the `apr import` container is read from a file. Files in `/var/task/model` avoid a 91 MB copy to `/tmp` |

**Installation (no new third-party crate is required):**
```bash
# Rust: lambda_runtime 0.13, sha2, base64, serde_json, tokio are already in the workspace lock.
# Python (back-office), exact pins already committed:
cd scripts/setfit_fixtures && uv sync --frozen        # python 3.13.7, sentence-transformers 5.7.0, torch 2.13.0
cd deploy-extensions && npm ci                         # node_modules is absent in this checkout
```

**Version verification:** `lambda_runtime` 0.13.0 and `base64` are in `Cargo.lock`; `gsd-tools query package-legitimacy check` returned OK for both. `sentence-transformers` is flagged SUS (see audit): the flag is about the package's newest release (published 2026-10-21), not the pinned 5.7.0 that the repo already trusts for committed fixtures.

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| lambda_runtime | crates.io | since 2018-11-29 | 308,542/wk | github.com/aws/aws-lambda-rust-runtime | OK | Approved; already locked at 0.13.0 |
| base64 | crates.io | since 2015-12-04 | 33,613,683/wk | github.com/marshallpierce/rust-base64 | OK | Approved; workspace `base64 = "0.22"` |
| sentence-transformers | PyPI | years; latest release 2026-10-21 | unknown (null) | https://www.SBERT.net | SUS (reasons: `too-new`, `unknown-downloads`) | Keep. The repo already pins `5.7.0` in `scripts/setfit_fixtures/uv.lock` and the committed Phase 1 fixtures were generated with it. The flag is driven by the newest release date. Do not upgrade; planner adds no new install gate beyond `uv sync --frozen` |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** `sentence-transformers` (already a pinned, in-tree dependency; no new install, so no `checkpoint:human-verify` for installation is required, but the reference-vector task must use `--frozen`)

No other external package is introduced. `serde_json`, `tokio`, `sha2`, `aws-cdk-lib`, `constructs` are existing workspace/package.json dependencies.

## Architecture Patterns

### System Architecture Diagram

```
 requester (pmcp.run graph-rag)                         back-office (not served)
 aws lambda invoke / lambda://<fn>/v1/embeddings         uv run scripts/embed_reference/*.py
        |  IAM lambda:InvokeFunction (caller identity)         |  sentence-transformers 5.7.0, max_seq_length=256
        v                                                      v
 +------------------------ Lambda (provided.al2023, arm64) ----------------+     aprender-embed-reference-vectors.json
 | bootstrap (lambda_runtime)                                              |     (+ sha256, HF revision, uv.lock sha)
 |   init: load /var/task/model ONCE, verify sha256 pins, never exit on    |
 |         failure (record failure, answer 500 envelope per call)          |
 |   per invoke: serde_json::Value event                                   |
 |     -> route (POST + path == /v1/embeddings) else 404/405 envelope      |
 |     -> body: isBase64Encoded? decode; size > cap -> 413                 |
 |     -> JSON parse; model / input / encoding_format / dimensions checks  |
 |        -> 400 envelope on any refusal                                   |
 |     -> bounds BEFORE tokenization: count, bytes per input, total bytes  |
 |     -> spawn_blocking { no_grad { micro-batches of 8 }  + clear_graph } |
 |          SetFitMiniLm: tokenize (cut at 256 incl [CLS]/[SEP])           |
 |                        encoder x6 -> masked mean pool -> L2             |
 |     -> catch panic / join error -> 500 envelope (never propagate)       |
 |     -> {object:list,data:[{object,index,embedding}],model,usage}        |
 |     -> APIGW v2 response {statusCode,headers,body,isBase64Encoded}      |
 +-------------------------------------------------------------------------+
        ^ code package: bootstrap + model/ (full_model.apr 90.9 MB, tokenizer.json,
          config.json, modules.json, 1_Pooling/config.json, release-identity.json)
        ^ deployed by CDK (deploy-extensions), no FunctionUrl / API / LB / Permission
```

### Recommended Project Structure
```
crates/aprender-embed-endpoint/            # NEW, publish = false, lib + [[bin]] bootstrap (ONE package)
  src/lib.rs                               # pure: handle(event: Value, &Engine, now) -> Value  (testable without Lambda)
  src/envelope.rs                          # OpenAI request/response/error types
  src/bounds.rs                            # constants asserted EQUAL to the contract (test_support::constant_u64 pattern)
  src/identity.rs                          # sha256 pins, release-identity.json
  src/main.rs                              # lambda_runtime::run(service_fn(...)); builds its own runtime
  tests/                                   # hostile-event table, real-weight parity (env gated)
contracts/embed-endpoint-boundary-v1.yaml  # NEW pv contract: constants, untrusted_input_bounds, served_fields
deploy-extensions/lib/embed-endpoint-stack.ts      # NEW stack; bin/app.ts selects stack by --context
scripts/embed_reference/                   # NEW uv project (or extend scripts/setfit_fixtures) - reference vectors + bundle
deploy/embed-endpoint/                     # events/, tests, ids dir (gitignored .ids), as deploy/llm-endpoint/
justfile                                   # embed-* recipes (build asset, synth, diff, deploy, readback, bundle, selftest)
```

### Pattern 1: Direct handler that cannot produce a FunctionError from input
**What:** `lambda_runtime::run(service_fn(f))` with `f(LambdaEvent<serde_json::Value>) -> Result<serde_json::Value, Infallible>`; every outcome, including a failed model load, is an `Ok(api_gateway_v2_response)`.
**When to use:** always for this endpoint.
**Evidence:** `lambda_runtime` 0.13.0 wraps the handler with a panic layer, but its own test asserts a panic is reported with `function-error-type: unhandled` ([VERIFIED: ~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/lambda_runtime-0.13.0/src/runtime.rs:422-460, mock `when.method(POST).path(.../error).header("lambda-runtime-function-error-type", "unhandled")`]). So a panic is caught by the runtime and still surfaces as a FunctionError; our code must catch it first.
**Example:**
```rust
// Sketch (not compiled). Source for service_fn/LambdaEvent shape: lambda_runtime 0.13 lib.rs docs.
use lambda_runtime::{service_fn, Error, LambdaEvent};
use serde_json::Value;

async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
    let engine = ENGINE.get();                       // Option<&Engine>; None => model_unavailable envelope
    let deadline_ms = event.context.deadline;        // epoch ms, used to refuse work that cannot finish
    let out = tokio::task::spawn_blocking(move || {
        aprender::autograd::no_grad(|| handle(&event.payload, engine, deadline_ms))
    })
    .await;                                          // JoinError (panic) is data, not an Err we return
    Ok(match out { Ok(resp) => resp, Err(_) => error_response(500, "server_error", "internal error") })
}
```

### Pattern 2: Load once, fail soft
**What:** Load and hash-verify at process start (init phase). If anything fails, do **not** `exit`; store the failure and answer each call with a 500 envelope. An init-phase crash is a FunctionError for the first invocation.
**Why:** The model load measured 92-145 ms from a hot page cache; the init cap is about 10 s (`bootstrap.rs` module doc, "default Lambda caps init near 10 s"), so eager load fits and removes the cold-start load from the request path. The decide server loads lazily only because its 0.85 GB S3 download did not fit init [VERIFIED: crates/aprender-mcp-decide-lambda/src/bootstrap.rs:9-14].

### Pattern 3: Micro-batch under `no_grad` in one blocking thread
**What:** Split the request into chunks of 8 texts in request order; for each chunk call `SetFitMiniLm::encode_texts` inside `aprender::autograd::no_grad`; call `clear_graph()` afterwards as a belt.
**Why:** measured (Measured Facts). `no_grad` and `GRAD_ENABLED` are **thread-local** (`thread_local! { static GRAPH ...; static GRAD_ENABLED ... }`, autograd/mod.rs:63-69), so the `no_grad` call must be on the thread that runs the encode, i.e. inside the `spawn_blocking` closure, not around the `.await`.

### Pattern 4: No-ingress CDK stack and fail-closed readback
**What:** A `lambda.Function` (PROVIDED_AL2023, ARM_64, `handler: 'bootstrap'`, `Code.fromAsset`) in a new stack, named `aprender-embed-endpoint-${env}` and logged to an explicit LogGroup, with **no** `addFunctionUrl`, no `apigatewayv2`/`apigateway`/`elbv2` construct, no `addPermission`. Publish a caller policy as a `CfnOutput` (precedent: `RequestLambdaPolicy` in `setfit-training-stack.ts`) and an SSM parameter. Proof of R4 has two halves: (a) an offline scan of the synthesised template for forbidden resource types and `"*"` principals; (b) an owner-run readback recipe copied from `llm-endpoint-readback` (justfile:4897-4953) that records `function_url: absent` (via `get-function-url-config` returning `ResourceNotFoundException`), `reserved_concurrency`, `code_sha256`, env var key names only, and adds `get-policy` (expect `ResourceNotFoundException` or no statement with `"Principal":"*"`/`{"AWS":"*"}`).
**Deploy recipe style to copy:** `refuse <check> "<msg>"` ladder with `DRY_RUN=1` (offline, zero AWS calls) and `PREFLIGHT=1` (read-only) modes, an `aws` shim self-test with a call recorder, content-pinned inputs, and containment (`put-function-concurrency ... 0`) on identity failure [VERIFIED: justfile:4157-4346 `llm-endpoint-deploy`, justfile:1923-2090 `laya-deploy`].

### Pattern 5: Reference vectors
**What:** A uv project pinned to the committed lock; load the snapshot directory at the pinned revision offline; set `max_seq_length = 256`; `encode(..., normalize_embeddings=False)` (the model already has a `Normalize` module); store inputs, vectors (float64 JSON), original token lengths, and the identities. Measured working this session: `HF_HUB_OFFLINE=1 .venv/bin/python -I ref_st.py <snapshot> in.json out.json` printed `max_seq_length 256 modules ['Transformer', 'Pooling', 'Normalize']`.
**Acceptance bar grounding:** see "Tolerance" below.

### Anti-Patterns to Avoid
- **Calling `VerifiedSetFitModel::embed` / `classify` / `encode_texts` outside `no_grad` in a warm process.** Tape growth, measured at 26.9 GB.
- **Encoding all 64 inputs in one call.** Even under `no_grad` the grad-mode attempt was killed; under `no_grad` micro-batches of 4-16 had identical peak RSS (3.47 GB) and 1-8% different time, so the batch width is not worth the memory risk.
- **Reaching tokenizer facts via `--features conformance-fixtures`.** That feature unlocks the test-only slice constructor (mod.rs:17-20 comment). Add a public API in core instead (Open Question 4).
- **`exit` on model-load failure.** Becomes a FunctionError (Pattern 2).
- **Returning `Err` from the handler for anything derived from input.**
- **A `setfit-apr-v1` artifact with a fake head just to reuse `embed`.**
- **Logging request text or the `user` field.** `bootstrap.rs` module doc: "Request text is never logged."

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| WordPiece tokenization and truncation | A second tokenizer or manual slicing | `SetFitMiniLm` (sealed, `LongestFirst`, library reserves room for specials) | tokenizer.rs:288-291: "Hand-rolling would silently drop [SEP]" |
| Mean pool + L2 | Own pooling | `BertSentenceEncoder::encode` (`masked_mean_pool` + `l2_normalize_rows`, encoder.rs:941-945) | Parity-tested to 7.6e-6 on the pooling family |
| Truncation facts / token counts | Re-tokenizing for `usage` | The batch's `truncation()` and attention mask (what `classify` reads, classify.rs:739-780) | A second pass reports facts about a different call |
| Lambda panic handling | Relying on the runtime | `spawn_blocking` + `JoinError` mapping | Runtime marks a panic `unhandled` |
| S3 ranged download | A new downloader | Not needed (zip package). If a future model is too large, reuse `aprender-mcp-decide-lambda/src/s3.rs` | Parts, retries, deadlines are already derived there |
| Contract/boundary validation | Bash/yq scripts | `pv validate` / `pv lint` | CLAUDE.md "DOGFOOD pv, NEVER bash" |
| sha256 | Own implementation | `sha2` (workspace) | Existing |
| CDK-template forbidden-resource scan | Grepping text | Parse `cdk.out/*.template.json` with Python `json` (or a Rust test reading it) | Structural check, with a must-match control |

**Key insight:** every hard part (tokenizer cut, pooling, normalisation, hashing, deploy refusals) already exists; the new code is the event/envelope layer, the bounds, the `no_grad` discipline and the proof tooling.

## Runtime State Inventory

Not a rename/refactor/migration phase. Omitted.

## Common Pitfalls

### Pitfall 1: Autograd tape growth (memory leak) in a warm environment
**What goes wrong:** Peak RSS grows by roughly 0.4 GB per 256-token single-text encode; a 64-text request in grad mode reached 26.9-36.1 GB (micro-batch 1 to 16) and a single 64-wide call was killed at 10.3 GB RSS / 76 GB footprint.
**Why it happens:** parameters from `from_pretrained_dir` are all-trainable (mod.rs:260 "Default is all-trainable (D-20)"), so every op appends to the thread-local `GRAPH`; nothing in `embed`/`classify`/`encode_texts` calls `no_grad` or `clear_graph` (grep over `setfit/classify.rs`, `setfit/artifact.rs`, `aprender-mcp-setfit/src`, `aprender-serve/src/api/setfit_handlers.rs` found neither). Whether the same growth affects the already-deployed SetFit classify servers is unverified [ASSUMED].
**How to avoid:** `no_grad` inside the blocking closure, `clear_graph()` after, and a test asserting `graph_tape_len() == 0` after a request.
**Warning signs:** Lambda "Max Memory Used" rising across invocations of one environment; `graph_tape_len()` non-zero after a call.

### Pitfall 2: A panic is a FunctionError even though the runtime catches it
See Pattern 1. Also `catch_unwind` needs `panic = "unwind"`; the workspace has no `[profile.release]` override and `Cargo.toml` mentions no `panic = "abort"` (grep found only unrelated `panic` comments), so unwinding is the default [VERIFIED: grep of Cargo.toml].

### Pitfall 3: Response over 6 MB is a service-side FunctionError
**What goes wrong:** Lambda caps synchronous request and response at 6 MB each [CITED: docs.aws.amazon.com/lambda/latest/dg/gettingstarted-limits.html, "Invocation payload (request and response) | 6 MB each for request and response (synchronous)"]. OpenAI allows 2,048 inputs per request [CITED: developers.openai.com/api/reference/resources/embeddings/methods/create]. 2,048 float vectors of 384 values would be roughly 12 MB of JSON (estimate: about 6 KB per vector, [ASSUMED], measure in a test).
**How to avoid:** cap `max_inputs` far below 2,048 (the CPU budget will bind first) and measure `body.len()` of the maximal response in a unit test; fail with a 400/413 envelope before computing.

### Pitfall 4: Out-of-memory and timeout cannot be caught
**How to avoid:** bounds before tokenization (count, bytes per input, total bytes), a token budget derived from measured ms/token, and a deadline check against `LambdaEvent.context.deadline` before each micro-batch that returns a 500/504-style envelope with time to spare. Measured: one 5,560,000-byte single input cost 1.74 s and 1.47 GB RSS under `no_grad`, so a per-input byte cap is necessary even though the encoder reads only 256 tokens.

### Pitfall 5: The model is not "ready" at the first call
Init-phase failure surfaces as a FunctionError for the invoke that triggered init. See Pattern 2. Also record `first_call_in_process` and load timings in a log line (precedent `decide.load ... load_ms=`), never request text.

### Pitfall 6: `setfit-apr-v1` cannot hold an encoder without a head
`write_setfit_apr` refuses `num_labels < 2` ("a classifier head needs at least two labels", artifact.rs:917-920), and the loader refuses the same (artifact.rs:2447-2451) [VERIFIED: Read of artifact.rs]. Use the directory loader.

### Pitfall 7: `tokenizer.json` ships its own truncation=128
The pinned `tokenizer.json` carries `truncation: {'max_length': 128, ...}` and `padding: {'strategy': {'Fixed': 128}, ...}` (measured by loading it in Python). Rust overrides both (tokenizer.rs:292-312). A requester computing build-time vectors by loading `tokenizer.json` with default settings would cut at 128 and disagree with the endpoint. State the 256 policy in the bundle and in the reference-vector file.

### Pitfall 8: OpenAI `input` forms we cannot honour
OpenAI accepts a string, an array of strings, an array of integers, or an array of integer arrays [CITED: developers.openai.com embeddings create]. Token-id inputs are token ids of OpenAI's tokenizers; `SetFitMiniLm` has no id-input door (it is sealed to text). Refuse integer inputs with a 400 envelope unless the brief says otherwise [ASSUMED: policy].

### Pitfall 9: Tokenizer library skew in the reference environment
Rust uses `tokenizers` 0.23.1 (tokenizer.rs docs); the committed Python lock has `tokenizers 0.22.2` [VERIFIED: `.venv/bin/python -c ...` printed `5.7.0 2.13.0 4.57.6 0.22.2`]. Measured token counts agree on all 23 probe inputs (9, 10, 2, 23, 233, 1472, 147002, ...). Keep the reference corpus containing long and unicode inputs so a future skew shows up.

### Pitfall 10: OpenAI fields that do not apply
`dimensions` is "Only supported in `text-embedding-3` and later models" [CITED]. For this 384-d model accept absent or `384` and refuse anything else with 400; never truncate-and-renormalise silently. `user` is accepted and ignored. `encoding_format` `float` (default) or `base64`; base64 is the little-endian f32 bytes of the vector, which is what the official Python SDK decodes with numpy [ASSUMED].

### Pitfall 11: The platform's function-name allowlist
The 2026-10-18 pmcp.run change request says "the platform only invokes names that start with aprender-llm- or pmcp-llm-" [VERIFIED: `.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/2026-10-18-pmcp-run-multi-size-change-request.md:13`]. `aprender-embed-endpoint-<stage>` matches neither. Whether the embed caller uses the same allowlist is unknown; ask in the brief (Open Question 1).

### Pitfall 12: Adding a crate trips three repo gates
`monorepo_invariants.rs` FALSIFY-MONO-011 (deployment-unit register), `scripts/duplicate_bin_names_allowlist.txt` if the bin is called `bootstrap`, and `readme_contract.rs` FALSIFY-README-005/007 (README crate and contract counts are equalities). Details in "Registries a New Crate Must Touch".

## Measured Facts (this session, M4 Pro 14 cores, aarch64 macOS, `--release`, f32)

Method: a throwaway example inside `aprender-core` (feature `setfit,conformance-fixtures`), built and run, then deleted (`git status` shows no change under `crates/`). Raw outputs: `/private/tmp/claude-501/-Users-guy-Development-machine-learning-aprender/616c454e-f1c8-4d02-b77e-c34809419d19/scratchpad/` (`probe.out`, `p_mb{1,4,8,16}.out`, `q_mb{1,4,8,16}.out`, `delta.out`, `st_out.json`, `rust_out.json`, `texts.json`). Peak RSS from `/usr/bin/time -l`. Model dir `~/.cache/aprender/minilm-l6-v2-1110a243`. Workload for the 64-input runs: texts of 234, 444 or 654 wordpieces (every one at or above the 256 cap).

| Measurement | Result | Tag |
|---|---|---|
| Model load from directory | 92.8-144.6 ms (hot page cache) | [VERIFIED: run] |
| One 233-token input, grad mode | 125.1-126.7 ms (3 runs) | [VERIFIED: run] |
| 64 inputs, `no_grad`, micro-batch 16 / 8 / 4 / 1 | 5.15 / 5.37 / 5.45 / 6.76 s (first run); second runs 5.13 / 5.37 / 5.52 / 8.21 s | [VERIFIED: run] |
| Peak RSS, `no_grad`, any micro-batch 1-16 | 3.47-3.48 GB | [VERIFIED: run]. Why it is constant across micro-batch sizes is not explained; likely per-thread GEMM buffers on 14 threads; re-measure with 6 threads |
| 64 inputs, grad mode, micro-batch 1 / 4 / 8 / 16 | 27.7 / 22.1 / 20.8 / 18.2 s wall; peak RSS 26.9 / 30.4 / 36.1 / 36.1 GB | [VERIFIED: run] |
| 64 inputs in ONE call, grad mode | 8.4 s then 11.2 s; process terminated abnormally (maximum resident 10.28 GB, peak footprint 76 GB) | [VERIFIED: run] |
| `no_grad` output vs grad output, first 8 rows | bit-identical (`identical=true`, max_abs 0) | [VERIFIED: run] |
| Autograd tape after `no_grad` encodes | `tape_len=0` | [VERIFIED: run] |
| Micro-batch composition (1, 4, 16 vs 8) on 64 real 256-token inputs | max_abs delta 0.000e0 for all | [VERIFIED: run, aarch64 macOS only] |
| Same long input twice | identical bytes | [VERIFIED: run] |
| Truncation | 1,472-wordpiece input: `TruncationFact { truncated: true, original_len: 1472 }`, batch `seq=256` | [VERIFIED: run] |
| Degenerate inputs `""`, `" "`, NUL, two emoji, U+200B, U+FFFF | all return Ok with L2 norm 1.0000, no error | [VERIFIED: run] |
| One 5,560,000-byte input | Ok `[1,384]`, 1.74 s, peak RSS 1.47 GB | [VERIFIED: run] |
| Rust vs sentence-transformers 5.7.0 / torch 2.13.0, `max_seq_length=256`, 23 inputs | max abs element delta between 1.0e-7 and 3.3e-7 (max 3.278e-07, on the empty string); token lengths identical for all 23 (e.g. 233, 1472, 147002) | [VERIFIED: run] |

Existing recorded SetFit numbers (same hardware class, short TweetEval texts, different workload): cold single classify 765.5 ms, warm single-text median 38.5 ms, throughput 78.7 rows/s at batch 32, inference peak RSS 1.55 GB [VERIFIED: benchmarks/tweeteval-stance/rows/setfit-s32-seed43.json, fields `cold_latency_ms`, `warm_latency_ms_median`, `throughput_rows_per_sec`, `inference_peak_rss_bytes`]. The skill notes "SetFit 91 MB at ~32 ms warm" on Lambda [CITED: spike-findings-aprender `llm-classifier-lambda-deployment.md`].

### What this implies for Lambda (estimates, NOT measurements)
- Bracket 1 (scale from M4 Pro): 5.2 s for 16,384 tokens x (14 / 6 cores) x (2.5-4x Graviton core ratio quoted in the skill: "An M4 P-core is roughly 4x a Graviton2 core") gives about 30-50 s. [ASSUMED]
- Bracket 2 (scale from Phase 8): Laya priced at 12 ms/token at the 10,240 MB tier for ~395 M non-embedding parameters [VERIFIED: contracts/decide-tool-boundary-v1.yaml v7.0.0 note, `tier_ms_per_token_at_512 40 -> 12`]; MiniLM has about 10.6 M non-embedding parameters (6 x (4 x 384^2 + 2 x 384 x 1536)), so about 0.32 ms/token or about 5.3 s for 16,384 tokens, if the SetFit path reaches the Laya path's efficiency. [ASSUMED]
- The two brackets disagree by 6x. **A real Graviton invoke at 10,240 MB with 6 rayon threads is the first Wave-0 gate**, and the 64-chunk timeout answer should be reported, not promised (R6).
- Memory sizing: pick 10,240 MB for vCPU, not for RSS. The skill measured that Laya at 4 GB "cost the same per decision and was 2.4x slower" [CITED: aws-mcp-model-hosting.md]. Peak RSS is about 3.5 GB on 14 threads; expect lower on 6.
- Set `RAYON_NUM_THREADS` explicitly (skill: "Set RAYON_NUM_THREADS from the function's vCPU setting"). Whether `trueno::Matrix::matmul` takes the parallel BLIS path for these shapes is size-dependent (encoder.rs:197-201) and unmeasured on Graviton; build with and without `aprender-core` default features only if the first Graviton run is slow.

## Answers to the Research Questions

### Q1. Embedding API, loading, truncation, packaging, vocab.txt, concurrency

**Public API surface (all in `aprender::setfit`, feature `setfit`):**
- `SetFitMiniLm` = `pub type SetFitMiniLm = SetFitBert;` [VERIFIED: mod.rs:278].
- Load: `pub fn from_pretrained_dir(dir: &Path, root_seed: u64) -> Result<Self, SetFitError>` (mod.rs:313). Returns eval mode. Requires in `dir`: `config.json`, `modules.json`, `1_Pooling/config.json`, `tokenizer.json` (hash must equal `PINNED_TOKENIZER_SHA256`), and an APR weight file named `full_model.apr` or `model.apr` (`const WEIGHT_FILE_CANDIDATES: [&str; 2] = ["full_model.apr", "model.apr"];`, import.rs:1123). `sentence_bert_config.json` is validated only when present (import.rs:468-477). Safetensors is **not** read directly; it is converted by `apr import model.safetensors -o full_model.apr --arch bert` (`scripts/setfit_fixtures/fetch_full_weights.py`).
- Rebuild from bytes: `from_bundle_parts(tokenizer_bytes, &EncoderArchitecture, tensors, root_seed)` (mod.rs:442) - the reload surface used by `setfit-apr-v1`; not needed here.
- Encode: `pub fn encode_texts(&self, texts: &[&str]) -> Result<Tensor, SetFitError>` returning `[B, H]` unit-norm rows (mod.rs:582). `Tensor::data()` and `shape()` give the floats (used in the probe and in `split_embedding_rows`, artifact.rs:1891-1925).
- `VerifiedSetFitModel::embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, SetFitArtifactError>` (artifact.rs:1864) exists, with `EmptyEmbedBatch` and `EncodeFailed`, but `VerifiedSetFitModel` is mintable only by `load_setfit_apr` (artifact.rs:1791 docs) and needs a head.
- **Tokenizer facts are not reachable from outside the crate** without `conformance-fixtures`: `tokenize_batch` is `pub(crate)` (mod.rs:616), `tokenize` is `#[cfg(feature = "conformance-fixtures")]` (mod.rs:692-695). `TruncationFact { truncated: bool, original_len: usize }` (tokenizer.rs:56-61; `original_len` counts "special tokens included") and `SentenceBatch` accessors are public types.

**Truncation behaviour [VERIFIED: tokenizer.rs:292-312 + run]:** `TruncationParams { max_length: 256, strategy: LongestFirst, stride: 0, direction: Right }`; `PaddingStrategy::BatchLongest`. `MAX_SEQUENCE_LENGTH = 256` (tokenizer.rs:52) counts the post-processor's `[CLS]`/`[SEP]`. Truncated rows are re-tokenized without truncation only to recover `original_len` (module docs, "STRATEGY IN FORCE"). Measured: a 1,472-wordpiece input gives `truncated: true, original_len: 1472`, batch seq 256.

**Packed artifact for an encoder-only model:** use the directory form. The tree already produces and consumes it: `full_model.apr` is 90,865,988 bytes, sha256 `980becb4b853b5043b8c87f001eb3d4b5bc3a12d724b8ca2c04423bce7c59831`, from source safetensors `53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db` [VERIFIED: `~/.cache/aprender/minilm-l6-v2-1110a243/full_manifest.json`, re-hashed this session]. `just build-trainer-asset` copies `full_model.apr tokenizer.json config.json modules.json full_manifest.json` plus `1_Pooling/config.json` (justfile:95-101). `apr_sha256` depends on the `apr` that wrote the file; produce it once, store it, and bind the release to that hash rather than rebuilding per release [ASSUMED: `apr import` output determinism across apr versions is unverified].

**Identity values (derived this session, [VERIFIED: shasum -a 256 of the pinned HF snapshot `1110a243fdf4706b3f48f1d95db1a4f5529b4d41`]):**

| File | sha256 | Note |
|---|---|---|
| model.safetensors | 53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db | equals `upstream_manifest.json` |
| tokenizer.json | be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037 | equals `PINNED_TOKENIZER_SHA256` (import.rs:74-75) |
| config.json | 953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41 | equals manifest |
| vocab.txt | 07eced375cec144d27c900241f3e339478dec958f92fddbc551f295c992038a3 | **not in `upstream_manifest.json`**, not emitted by any Rust/Python script in the tree |
| sentence_bert_config.json | fc1993fde0a95c24ec6c022539d41cf6e2f7c9721e5415d6fb6897472a9cd4b7 | `{"max_seq_length": 256, "do_lower_case": false}` |
| full_model.apr | 980becb4b853b5043b8c87f001eb3d4b5bc3a12d724b8ca2c04423bce7c59831 | `full_manifest.json` |

**vocab.txt:** `grep` over `crates contracts scripts docs justfile` finds `vocab.txt` only in `apr-cli` `embed.rs`/`rerank.rs` (readers) and the command registry; nothing writes or hashes it. The Rust path never reads it (the vocabulary is inside `tokenizer.json`). Measured: `tokenizer.json` `model.vocab` has 30,522 entries and every `vocab.txt` line index equals the id in `tokenizer.json` (`True`). So the bundle script should hash `vocab.txt` from the pinned snapshot and add a consistency check (line i == id i), and `upstream_manifest.json` should be extended or paralleled, not silently edited (it is covered by `manifest.sha256`).

**Thread-safety and batching:** `Arc<VerifiedSetFitModel>` is moved into `tokio::task::spawn_blocking` in `aprender-mcp-setfit` (src/lib.rs:199-214), so the model is `Send + Sync` [VERIFIED: that code compiles in-tree]; `encode_texts(&self)` takes a shared reference. The autograd tape and `GRAD_ENABLED` are thread-local. A Lambda environment serves one invocation at a time, so concurrency inside an environment is 1 and parallelism is rayon inside GEMM. Existing bounds to reuse as precedent: `MAX_BATCH_TEXTS = 256` (classify.rs:49) and `MAX_REQUEST_BODY_BYTES = 1_048_576` (classify.rs:76, "a TRANSPORT bound" the reading surface must apply). Do not inherit them; derive this endpoint's from the measured ms/token (decide-tool-boundary pattern).

**Expected CPU latency:** see Measured Facts. Local: 125 ms per 256-token text, about 5.2 s per 64 texts at the cap. Lambda: unmeasured.

### Q2. Direct handler and invocation differences

**What to reuse from `aprender-mcp-setfit-lambda` / `aprender-mcp-decide-lambda`:** the build-asset idea (`include_bytes!` staging in `build.rs`, but see Alternatives), `once_cell`/`OnceCell`-style one-time load, the glibc `.2.34` zigbuild target and floor check, `tracing` init, the "log load timeline, never request text" convention, and the identity-in-response convention (`x-decide-load` header; precedent `artifact_sha256` in every response). Do **not** reuse the pmcp loopback proxy, `lambda_http`, S3 loader, or the 503-on-load-failure behaviour (503 is a status, not a FunctionError, but a failed load should still be a stable envelope).

**Env contract (decide precedent, for contrast):** `APRENDER_DECIDE_S3_URI` + `APRENDER_DECIDE_SHA256` (mandatory pin) or `APRENDER_DECIDE_MODEL` path; both set is `Ambiguous` [VERIFIED: crates/aprender-mcp-decide-lambda/src/lib.rs:259-380]. For this endpoint: `APRENDER_EMBED_MODEL_DIR` (default `/var/task/model`), `APRENDER_EMBED_IDENTITY` (path to `release-identity.json`, default beside it), `RAYON_NUM_THREADS=6`, `RUST_LOG`. No secrets.

**Event shape.** The platform sends an API Gateway v2 payload; a real fixture is in the tree [VERIFIED: deploy/llm-endpoint/events/apigw-v2-no-auth.json]:
```json
{"version":"2.0","routeKey":"$default","rawPath":"/v1/chat/completions","rawQueryString":"",
 "headers":{"content-type":"application/json","accept":"application/json"},
 "requestContext":{"http":{"method":"POST","path":"/v1/chat/completions","protocol":"HTTP/1.1","sourceIp":"127.0.0.1","userAgent":"..."},"requestId":"prove-401","stage":"$default"},
 "body":"{...}","isBase64Encoded":false}
```
and the reference proxy reads `requestContext.http.method`, `rawPath`, `body`, `isBase64Encoded` and returns `{"statusCode","headers","body","isBase64Encoded"}` [VERIFIED: deploy/llm-endpoint/gpu-host/proxy/lambda_function.py:20-24, 29-33, 18-19]. Route on method plus `rawPath` (fall back to `requestContext.http.path`), tolerate a stage prefix only if the brief shows the platform sends one.

**`aws lambda invoke` (and `lambda://`) vs HTTP events:** there is no HTTP layer. The caller (or the platform's harness) builds the JSON payload; Lambda requires the payload to be JSON ("Could not parse request body into json" is an `InvalidRequestContentException` raised by the service before the function runs, [CITED: docs.aws.amazon.com/lambda/latest/dg/invocation-sync.html]), and returns HTTP 200 for an invoke "even if the function returned an error" (same page). Hence: errors must be encoded inside our response (`statusCode`), and a FunctionError only appears for crash/timeout/oversize. A handler that takes `serde_json::Value` accepts any valid JSON, including a CLI test that sends a bare `{}`, and answers it with a 400/404 envelope. CLI note: AWS CLI v2 needs `--cli-binary-format raw-in-base64-out` for a literal payload (same page).

**Guaranteeing no FunctionError - the full list of causes and the control for each**
| Cause | Control |
|---|---|
| Panic in tokenizer/encoder/serialisation | `spawn_blocking` + map `JoinError` to a 500 envelope; no `unwrap` (`.clippy.toml` disallows it) |
| Event cannot be parsed | `Value`, then manual field access with `get`; unknown shape => 400/404 envelope |
| Handler returns `Err` | signature `Result<Value, Infallible>` |
| Init failure | load in `main` but never `exit`; store `Result` |
| Request body over limit | own cap => 413 envelope. Lambda itself rejects >6 MB request with `RequestEntityTooLargeException` before the function (service error, not a FunctionError and not ours) [ASSUMED: exception name; limit is CITED] |
| Response over 6 MB | cap inputs, measure max response size in a test |
| Timeout | set function timeout >= 60 s (R6 says >= 30 s), keep an internal deadline of `context.deadline - margin`, refuse oversized work up front |
| OOM | `no_grad`, byte/count/token bounds, 10,240 MB |
| Non-UTF-8 body after base64 | `from_utf8` error => 400 |

**413 mapping:** the in-function 413 is for **our** cap on the decoded body (decide a number below the 6 MB service cap, e.g. derived from `max_inputs x max_input_bytes`); the service's own >6 MB rejection never reaches us. **429:** nothing in a single-concurrency Lambda produces one; Lambda throttling surfaces as `TooManyRequestsException` at the caller. If the 13 checks demand an in-function 429, it needs a defined trigger (Open Question 5).

### Q3. Deploy with no public ingress, IAM-invoke-only, CDK style

**Existing structure [VERIFIED: deploy-extensions/bin/app.ts, lib/setfit-training-stack.ts, justfile:129-160]:** `bin/app.ts` instantiates one stack, `env` context `dev|prod` (default `dev`, rejects others), fails `cdk synth` when the asset's `bootstrap` is missing (`Build it first: just build-trainer-asset`), region default `us-east-1`, account from `CDK_DEFAULT_ACCOUNT`. The stack uses `lambda.Runtime.PROVIDED_AL2023`, `Architecture.ARM_64`, `handler: 'bootstrap'`, `Code.fromAsset(path)`, explicit `LogGroup`, `retryAttempts: 0`, `removalPolicy` RETAIN in prod/DESTROY in dev, SSM params `/aprender/setfit-train/${env}/...` and a `CfnOutput` policy for the caller. Recipes: `synth-training`, `diff-training`, `deploy-training`, `destroy-training` take positional `env` and `profile="ze-kasher-dev"`; `deploy-training` passes `--require-approval never` because `diff-training` is the review gate.

**What to add:** `EmbedEndpointStack` (+ app wiring by a second `--context stack=embed` or a second app entry), `build-embed-asset` (cross-compile `bootstrap`, assemble `model/`, glibc floor check like `build-trainer-asset` (the `CEILING = 34` block, justfile ~104-120), write `release-identity.json`), `synth-embed`, `diff-embed`, `deploy-embed`, `readback-embed`, `destroy-embed`, `embed-bundle`, `embed-selftest`. Function name `aprender-embed-endpoint-${env}` (the CONTEXT `<stage>` = `dev|prod`, open: confirm).

**Resource policy with no `*` principal:** a CDK `lambda.Function` has **no** resource policy unless something calls `addPermission`/creates a trigger. The strongest form of R4 is therefore: no `AWS::Lambda::Permission` at all, no `AWS::Lambda::Url`, none of `AWS::ApiGatewayV2::*`, `AWS::ApiGateway::*`, `AWS::ElasticLoadBalancingV2::*` [ASSUMED: CloudFormation type names from memory; confirm in a test against a synthesised template]. A same-account caller then needs only an identity-based `lambda:InvokeFunction` on the function ARN; publish that statement as a `CfnOutput` (as `RequestLambdaPolicy` does) for the requester to attach. If the requester is cross-account, add exactly one `AWS::Lambda::Permission` whose `Principal` is the named role/account ARN, never `*` (Open Question 2). The function resource-based policy limit is 20 KB [CITED: gettingstarted-limits].

**Fail-closed checks to copy [VERIFIED: justfile]:**
- `refuse <check> "<msg>"` helper with distinct exit codes; env whitelist `dev|prod`; reject `variant=` mis-positioning (justfile:4179-4195).
- Content pins: local sha256 equals the pin in the generated config (`laya-deploy` rung (3), justfile:1950-1960), `shasum -a 256` called directly because "the rtk hook never rewrites a command inside a recipe" (justfile:1936-1938).
- Account guard: `aws sts get-caller-identity` account equals the recorded account before any write (justfile:4253-4257).
- Read-only `PREFLIGHT=1` and offline `DRY_RUN=1` modes; self-test with an `aws` shim whose recorder must stay empty (`AWS CALLS: 0`) plus a negative control (README "The recipes' own test").
- Containment on identity failure: `put-function-concurrency --reserved-concurrent-executions 0` (justfile:2020-2025, 4325-4331).
- Readback evidence JSON with env var **key names only**, `function_url` absent/present, `code_sha256` (justfile:4929-4953).
- Never read `$?` through a pipe (CLAUDE.md Verification 1): every recipe above captures status after a redirect.

**Offline R4 gate to add:** after `cdk synth`, parse `cdk.out/aprender-embed-endpoint-<env>.template.json`; fail if any forbidden resource type exists or if any `Principal` equals `"*"` or `{"AWS": "*"}`; run the scan once against a planted-violation copy to prove it can fail (CLAUDE.md Verification 4/7).

**Package size:** `bootstrap` (an earlier build in `target/lambda/bootstrap` is 17.2 MB) + `full_model.apr` 90.9 MB + 0.5 MB JSON is about 110 MB unzipped, against `AWS_LAMBDA_ZIP_LIMIT_BYTES: u64 = 262_144_000` (artifact.rs:1573) and "250 MB ... (unzipped)" [CITED: gettingstarted-limits]. The zipped upload exceeds the 50 MB direct limit, so the asset goes through the CDK bootstrap bucket (the training stack already ships ~159 MB this way per its README).

### Q4. Reference vectors and the acceptance bar

**Reproducible computation.** Reuse the existing pinned uv project (`scripts/setfit_fixtures`: `requires-python >=3.13,<3.14`, `.python-version` 3.13.7, `setfit==1.1.3`, `sentence-transformers==5.7.0`, `torch==2.13.0`, `transformers>=4.41.0,<5`) or clone it to `scripts/embed_reference/` with a copied `uv.lock` (sibling keeps fixture regeneration untouched). Steps:
1. `uv sync --frozen` (never `uv lock` in this task).
2. Resolve the snapshot at the immutable revision (`huggingface_hub.snapshot_download(repo_id, revision=PINNED_REVISION)`), assert each file sha256 against the table above, **including `vocab.txt`** (new pin).
3. `SentenceTransformer(snapshot_dir, device="cpu")`; assert `m.max_seq_length == 256`; set it explicitly anyway.
4. `m.encode(texts, batch_size=8, convert_to_numpy=True, normalize_embeddings=False)` (the model has a `Normalize` module; observed `modules ['Transformer', 'Pooling', 'Normalize']`).
5. Record per text: input, `len(tokenizer(text, truncation=False)["input_ids"])`, the vector as float64, plus top-level `revision`, package versions, `uv.lock` sha256, `max_seq_length`.
6. Corpus must include: short, empty, unicode/CJK/accented/mixed case, 233-wordpiece, a >=1,600-wordpiece chunk, an extreme (100k+ wordpiece) input, and a 64-item realistic batch.
The requester's file is named `aprender-embed-reference-vectors.json`; its schema (field names) is **not** in the tree; get it from the brief before fixing the format (Open Question 1).

**Tolerance grounded in the repo [VERIFIED: contracts/setfit-encoder-conformance-v1.yaml]:** tokenizer family is EXACT (integer equality, SEC-EQ-001); the real-weight embedding family is `tolerance: 3.73762473e-05` (SEC-EQ-011, yaml:570-585; `floor(W)=8*sqrt(W)*EPS_F32` with W=1536) and the pooled-output family is `7.62939453e-06`; `PROBE_EMBEDDING_ABS_TOLERANCE: f64 = 7.629_394_53e-06` (artifact.rs:1594). Gap in the repo's own evidence: `full_model_reference.json` has **three short sentences** (`full_model_trio`) and no truncated or long input; the only truncation fixture (`truncation_long`) covers the tokenizer, and per-layer forward parity runs on the 2-layer slice. So the 256-token truncated real-weight case was not grounded until now. This session's measurement (max 3.3e-7 over 23 inputs including truncated ones) is about 110x below 3.74e-5. Recommend: the acceptance bar for our own run is max abs <= 3.73762473e-05 (existing, citable), report the measured delta alongside, and add a committed long-input fixture so the claim is repo-owned rather than session-owned. All deltas here are aarch64-macOS; Graviton is unmeasured and x86_64 is documented as unmeasured in CLAUDE.md.

### Q5. Registries a new crate must touch

| Registry | What to do | Evidence |
|---|---|---|
| Root `Cargo.toml` `members` | add the crate path (existing lambda crates at lines 60 and 64) | [VERIFIED: grep] |
| `monorepo_invariants.rs` FALSIFY-MONO-011 | The register is keyed by PACKAGE. Any new package with a bin must be added to `deployment_unit_bins` and `DEPLOYMENT_UNIT_BASELINE` raised 9 to 10. The test's own text: "a new publish = false MCP server binary requires a recorded CONTEXT decision (a `gate=\"blocking-human\"` checkpoint), never a same-PR edit to this list." Not present in CONTEXT.md: needs a decision. Crate must be `publish = false` (checked) | [VERIFIED: crates/aprender-core/tests/monorepo_invariants.rs:312-323, 428-454] |
| `scripts/duplicate_bin_names_allowlist.txt` | The `bootstrap` line lists four packages; a fifth package with a bin named `bootstrap` must be added to that exact line, and the declared set must equal the observed set (no stale entries). Avoidable by naming the bin `aprender-embed-endpoint` and renaming to `bootstrap` in the asset recipe, as `build-trainer-asset` does for `aprender-setfit-trainer` | [VERIFIED: allowlist file; justfile:111-114] |
| `readme_contract.rs` | FALSIFY-README-005 (README "Workspace crates" row equals `cargo metadata` count) and FALSIFY-README-007 (contract count equals `find contracts/ -name '*.yaml'`): +1 crate, +1 contract (or more) means README edits in the same change. CLAUDE.md's sample row also drifts but is derived, not gated | [VERIFIED: tests listing, readme_contract.rs:80-170] |
| `contracts/embed-endpoint-boundary-v1.yaml` | New `pv` contract. Template top-level keys (both templates): `contract, metadata, constants, (served_fields, untrusted_input_bounds) \| door_surface, equations, proof_obligations, falsification_tests, kani_harnesses, qa_gate`. `decide-tool-boundary-v1` constants: `classify_max_texts, classify_max_text_bytes, classify_max_total_tokens, lambda_memory_mb, api_gateway_timeout_ms, cold_start_budget_ms, margin_ms, tier_ms_per_token_at_512` - assert Rust constants EQUAL to the YAML in a test (`test_support::constant_u64`), assert the CDK `memorySize` equals `lambda_memory_mb` (FALSIFY-DECIDE-TOOL-007 pattern), and run `pv validate`/`pv lint`. `pv` is not on PATH here; `target/release/pv` exists, pin it | [VERIFIED: python yaml key dump of both contracts; `command -v pv` MISSING; `ls target/release` shows `pv`] |
| Claims ledger | `scripts/laya_claims.tsv` has columns `claim_id file anchor kind owner test finding`; `just laya-claims-check` fails on a row whose anchor text is in no file. Any new CLAUDE.md sentence about the endpoint (e.g. a fourth Realizar-first exception row) needs a row naming its enforcing test. Also `scripts/laya_gates.tsv` (`gate_id target must_fail must_pass finding`) for new recipe gates. Whether this phase gets its own ledger or reuses these is a design choice | [VERIFIED: head of both files; justfile:3581, 3684] |
| CLAUDE.md "Realizar-First" table | Phase 8 (SetFit D-09, Forecast D-07, Decision D-16) each added an exception row with an argued paragraph. The embed endpoint serves a MiniLM encoder through `aprender-core`, not `realizar`; it needs the same: a row and a stated reason (performance argument does not apply; only parity-proven implementation; OPS-03) | [VERIFIED: CLAUDE.md text] |
| `ci/explicit-test-commands.d/NNN-<slug>.cmd` | A new `tests/*.rs` integration target is dark until a fragment names it; take a free ordinal. Unit tests (`--lib`) run automatically in `workspace-test` | [VERIFIED: CLAUDE.md Testing section] |
| `.github/workflows/*.yml` | Not required for this phase; modifying CI is a check-in item in CLAUDE.md | [VERIFIED: CLAUDE.md] |
| `.gitignore` | `deploy/embed-endpoint/.ids/`, generated assets under `deploy-extensions/assets/embed/` (the training asset directory pattern) | [ASSUMED: mirror `deploy/llm-endpoint/.ids`, `deploy-extensions/assets/trainer`] |
| Publishing safety | `publish = false`; run `scripts/check_include_files.sh` if any `include!()` is added (CLAUDE.md CB-510) | [VERIFIED: CLAUDE.md] |

### Q6. Risks, OpenAI shape and error envelopes

**Risk table**
| Risk | Severity | Evidence / handling |
|---|---|---|
| 64 x 256-token request does not finish in 30 s on 6 Graviton vCPU | HIGH | Brackets 5-50 s (Measured Facts). Measure first; fallback levers: lower `max_inputs` per call (requester chunks), `RAYON_NUM_THREADS` tuning, length-bucketed micro-batches, f16 is NOT a lever (parity), Lambda Managed Instances up to 16 vCPU (skill: LMI has no request-path cold start but is an always-on host; one model costs a `c9g.8xlarge`) |
| Tape growth OOM | HIGH | Pitfall 1; `no_grad` + test |
| Platform name allowlist rejects the function name | HIGH | Pitfall 11 |
| 13 checks unknown | HIGH | cannot plan acceptance work until the brief arrives |
| Cold start | LOW-MED | Load 0.1 s local. Lambda zip fetch of ~110 MB and Graviton disk reads unmeasured. For comparison: Chronos tiny 24 MB embedded had 52 ms cold start [CITED: spike skill chronos-mcp-server.md]; default Lambda S3 reads run ~80-95 MB/s [CITED: aws-mcp-model-hosting.md], so even a worst-case 110 MB fetch is about 1.2-1.4 s. Cold start target 30 s is not near |
| Graviton generation lottery | MED | skill: default Lambda gave Graviton2 and Graviton3 in the same function; record `/proc/cpuinfo` CPU part per response |
| Numeric difference across arch | MED | all parity numbers are aarch64 macOS; ship arm64 only, state x86_64 unmeasured |
| Deployment-unit register / allowlist gates fail CI | MED | Pitfall 12 |
| Public-API gap in core | MED | Open Question 4 |
| `apr import` non-determinism changes `.apr` sha | LOW-MED | store the one artifact; bundle pins its hash |
| Account Lambda memory ceiling | LOW | memory note: 10 GB tier available (reported 2026-09-28); the training stack README documents the 3,008 MB trap |

**Memory size and timeout recommendation (to be confirmed by measurement):** 10,240 MB (6 vCPU), `ephemeralStorage` default (no `/tmp` use), timeout 60 s (R6 requires >= 30 s; a 30 s timeout would equal the client's own budget and leave nothing for the envelope), internal deadline margin as `margin_ms` in the contract, arm64, no provisioned concurrency until the cold-start measure says otherwise. These are recommendations, not promises (R6).

**OpenAI request fields [CITED: developers.openai.com/api/reference/resources/embeddings/methods/create]:**
- `input`: "a string or array of tokens"; arrays "can hold strings, integers, or arrays of integers"; "The input must not exceed the max input tokens for the model (8192 tokens for all embedding models)"; "any array must be 2048 dimensions or less"; "300,000 tokens summed across all inputs in a single request".
- `model`: required string ("ID of the model to use"). Echo it. Unknown id: 404 with `error.code = "model_not_found"` - precedent in-tree: `"type": "invalid_request_error", "code": "model_not_found"` with `StatusCode::NOT_FOUND` [VERIFIED: crates/apr-cli/src/commands/serve/handler_apr_cpu_completion.rs:80-92]. Accept exactly `sentence-transformers/all-MiniLM-L6-v2` (and any alias the brief names).
- `encoding_format`: `float` (default) | `base64`.
- `dimensions`: only for `text-embedding-3`+; here absent or 384.
- `user`: ignore.

**OpenAI response:** `{"object":"list","data":[{"object":"embedding","index":<i>,"embedding":[...]}],"model":"<model>","usage":{"prompt_tokens":N,"total_tokens":N}}` [CITED: same page: `object` always "list", `data[].object` always "embedding", `index`, `embedding` "a list of floats when `encoding_format` is `float`" / base64 string, `model`, `usage.prompt_tokens`, `usage.total_tokens`]. `index` is the position in the input array. Define `prompt_tokens` explicitly (Open Question 5); the natural definition from the batch is the sum of attention-mask-kept positions, i.e. post-truncation and including `[CLS]`/`[SEP]` (classify's `consumed_positions`, classify.rs:762), so a 1,600-wordpiece chunk contributes 256.

**Error envelope** (OpenAI style; only `type` and `code` for `model_not_found` are repo-verified): `{"error":{"message":"...","type":"invalid_request_error","param":null,"code":null}}`. Suggested statuses/types: 400 `invalid_request_error` (malformed JSON, wrong types, empty `input` array, unsupported input form, bad `dimensions`/`encoding_format`, bound exceeded), 404 unknown route/model, 405 wrong method, 413 body over own cap (`type` `invalid_request_error`, `code` `request_too_large`), 429 `rate_limit_error`/`rate_limit_exceeded` (no natural trigger), 500 `server_error` (panic, model unavailable, deadline) [ASSUMED: type/code strings except the model_not_found precedent]. Messages must never echo caller text (decide-tool-boundary's `refusal_names_bound` pattern names the bound, not the data).

**Batch limits and degenerate input:** the encoder accepts `""` (norm 1.0, measured), but OpenAI rejects an empty string/array with 400 [ASSUMED]; decide which (Open Question 5). Per-input byte cap before tokenization is mandatory (5.56 MB single input cost 1.74 s and 1.47 GB). Suggested derivation (to fill with measured numbers): `max_inputs`, `max_input_bytes`, `max_request_bytes`, and a token budget `sum(min(original_len,256)) <= B` with `B = (timeout_budget - cold_budget - margin) / ms_per_token_at_tier`, exactly the structure of `decide-tool-boundary-v1` (`classify_max_total_tokens`, `tier_ms_per_token_at_512`, `margin_ms`, `lambda_memory_mb`).

## Code Examples

### Run the encoder correctly (probe-verified calls)
```rust
// Verified in this session's throwaway probe (since deleted): public API only.
use aprender::setfit::SetFitMiniLm;

let model = SetFitMiniLm::from_pretrained_dir(std::path::Path::new("/var/task/model"), 0)?;
// Inside spawn_blocking, on the encoding thread:
let rows: Vec<f32> = aprender::autograd::no_grad(|| -> Result<Vec<f32>, _> {
    let mut out = Vec::new();
    for chunk in texts.chunks(8) {                       // micro-batch 8, request order
        let t = model.encode_texts(chunk)?;              // [B,384], unit norm
        out.extend_from_slice(t.data());
    }
    Ok(out)
})?;
aprender::autograd::clear_graph();                       // belt; tape_len stayed 0 under no_grad
```
Source: public API listed in Q1 (mod.rs:313, 582; autograd/mod.rs:89, 117, 142).

### Reference vectors
```python
# scripts/embed_reference/make_reference.py (sketch; run with: uv run --frozen python -I make_reference.py)
import os, json, hashlib
os.environ["HF_HUB_OFFLINE"] = "1"                       # snapshot already pinned and hash-checked
from sentence_transformers import SentenceTransformer
m = SentenceTransformer(SNAPSHOT_DIR, device="cpu")
m.max_seq_length = 256
emb = m.encode(texts, batch_size=8, convert_to_numpy=True, normalize_embeddings=False)
lens = [len(m.tokenizer(t, truncation=False)["input_ids"]) for t in texts]
```
Source: pattern run successfully this session (`ref_st.py`).

### Template scan for R4 (offline)
```python
import json, sys
FORBIDDEN = ("AWS::Lambda::Url", "AWS::ApiGatewayV2::", "AWS::ApiGateway::", "AWS::ElasticLoadBalancingV2::")
t = json.load(open(sys.argv[1]))
bad = [k for k, r in t["Resources"].items() if r["Type"].startswith(FORBIDDEN)]
star = [k for k, r in t["Resources"].items() if r["Type"] == "AWS::Lambda::Permission"
        and r["Properties"].get("Principal") in ("*", {"AWS": "*"})]
sys.exit(1 if bad or star else 0)
```
Source: structure of CDK templates in `deploy-extensions/cdk.out/*.template.json` [ASSUMED: type names]. Add a planted-violation control.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Premise: `aprender-serve /v1/embeddings` is a zeros stub | Real decoder/GGUF hidden-state pooling, not an encoder | before 2026-10-23 (quick task 261023-aq0) | Not reusable for MiniLM |
| `cargo pmcp deploy` for every Lambda | CDK in `deploy-extensions/` when ingress is unwanted | Phase 5-7 (training stack) | Required for R4 |
| Weights in S3 pulled at cold start | Weights in the zip when <= ~100 MB | Chronos/SetFit lambdas embedded their weights | 91 MB MiniLM fits; no S3 path |
| `lambda_http` loopback proxy | Direct `lambda_runtime` on `Value` | this phase | Removes a FunctionError class |

**Deprecated/outdated:** the "decide" S3 loader's derived 10,739 ms download deadline applies to a 0.85 GB artifact only; do not copy its numbers.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The same autograd-tape growth affects `load_setfit_apr`/`classify` in the already-deployed SetFit servers | Pitfall 1 | An existing deployed server has a latent OOM; worth a separate finding to the owner, not a blocker here |
| A2 | `apr import` output is deterministic across `apr` versions | Q1 packaging | The `.apr` hash in a later bundle might differ; mitigated by storing one artifact |
| A3 | Graviton per-token cost: bracket 5-50 s for 64 x 256 tokens | Measured Facts | The 30 s requirement and the 64-chunk batch size are decided by a real invoke |
| A4 | CloudFormation type strings (`AWS::Lambda::Url`, `AWS::ApiGatewayV2::*`, `AWS::ApiGateway::*`, `AWS::ElasticLoadBalancingV2::*`, `AWS::Lambda::Permission`) | Q3, Code Examples | Scan misses a type; fix by checking the synthesised template |
| A5 | OpenAI error envelope fields and `type`/`code` strings other than `invalid_request_error`/`model_not_found` | Q6 | The requester's checks may assert exact strings; resolve from the brief |
| A6 | OpenAI rejects empty string/array input with 400; SDK decodes base64 as little-endian float32 | Q6, Pitfall 10 | Wrong policy on empty input or base64 |
| A7 | In-function 429 has no natural trigger on single-concurrency Lambda | Q2 | The brief may require a trigger |
| A8 | Lambda rejects an over-6 MB request with `RequestEntityTooLargeException` before the function | Q2 | Naming only; limit itself is cited |
| A9 | Platform sends an event shaped like `apigw-v2-no-auth.json` for the embed route and no stage prefix | Q2 | Routing mismatch; resolve from the brief or a captured event |
| A10 | `.gitignore` additions mirror the llm-endpoint/trainer directories | Q5 | Cosmetic |
| A11 | Peak RSS 3.47 GB would be lower with 6 threads | Measured Facts | Memory sizing; irrelevant at 10,240 MB |
| A12 | Float JSON for one 384-d vector is about 6 KB | Pitfall 3 | Response cap derivation; measure in a test |

## Open Questions

1. **The 13 conformance checks, the `aprender-embed-conformance` tool, the reference-vectors JSON schema, a captured platform event, and the function-name allowlist.**
   - What we know: they live in the pmcp.run repo; CONTEXT says do not invent them; the platform has been documented as invoking only `aprender-llm-*` / `pmcp-llm-*` names.
   - Unclear: the checks, whether `aprender-embed-endpoint-<stage>` is allowlisted, the exact event/route shape, the JSON schema, any 429 trigger, `usage` semantics, empty-input policy.
   - Recommendation: plan a first `checkpoint:human-action` (obtain brief + tool + a sample event); no acceptance-harness work before it. Do not list or guess the checks.
2. **Same-account or cross-account caller?** Decides identity-policy-only versus one explicit-principal resource policy. Recommend assuming same-account and publishing a caller policy output, as the training stack does.
3. **Deployment-unit register growth 9 to 10** (`monorepo_invariants.rs`). Needs a recorded CONTEXT decision with a `blocking-human` checkpoint by that test's own text. Alternative that avoids growth: none clean (the register is per package); adding a bin to an existing deployment-unit package would break its "thin" doctrine. Recommend raising it as a checkpoint at plan time.
4. **Core API for tokenizer facts.** Add an additive public method on `SetFitBert` (e.g. one returning the `[B,H]` tensor plus per-input `truncated`, `original_len`, consumed positions) that internally runs under `no_grad` and clears the tape; or use `conformance-fixtures` (not recommended). Touching `aprender-core` triggers its contract/test surface (`setfit-encoder-conformance-v1` is additive-safe, but run its suite). Do not change `embed`/`classify` behaviour in this phase; report the tape-growth finding separately.
5. **Envelope policy details the brief may fix:** `usage.prompt_tokens` definition, empty string/array, integer-token inputs, `dimensions`, unknown fields (ignore vs refuse), what produces 429, `max_inputs`. Defaults proposed in Q6.
6. **Graviton latency for 64 x 256 tokens.** The one number that can reshape the batch limit or force another host. Wave 0: build the arm64 bootstrap, deploy to the dev account (owner-run), invoke 1 and 64 inputs cold and warm, record CPU part and `Max Memory Used`.
7. **Stage and region:** `<stage>` assumed `dev|prod`; region `us-east-1` default in `app.ts`; profile `ze-kasher-dev`.
8. **Where the contract's numbers come from before the Lambda measurement exists:** write the contract constants as "pending measurement" or land the contract after the Wave-0 gate; the decide contract went through 10 versions as measurements arrived.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| rustc / cargo | build | yes | 1.93.0 (`rust-toolchain.toml` pin) | - |
| aarch64-unknown-linux-gnu target | Lambda build | yes | installed | - |
| cargo-zigbuild | cross build | yes | 0.23.4 | - |
| just | recipes | yes | 1.46.0 | - |
| cargo-nextest | CI-equivalent runs | yes | 0.9.102 | `cargo test` |
| node / npx | CDK | yes | 24.19.0 / 11.17.0 | - |
| `deploy-extensions/node_modules` (aws-cdk) | synth/deploy | no | - | `npm ci` in `deploy-extensions` (network read) |
| aws CLI | readback, invoke | yes | 2.29.0 (x86_64 build on arm Mac) | - |
| AWS credentials / profile `ze-kasher-dev` | live deploy | not exercised | - | owner-run checkpoint (no AWS access in this task) |
| uv / Python 3.13.7 | reference vectors | yes (uv 0.9.5; `scripts/setfit_fixtures/.venv` has sentence-transformers 5.7.0, torch 2.13.0, transformers 4.57.6, tokenizers 0.22.2); system python is 3.14.6 | - |
| HF snapshot `1110a243...` | reference vectors, bundle | yes, in `~/.cache/huggingface/hub/...` (incl. `vocab.txt`) and `~/.cache/aprender/minilm-l6-v2-1110a243` (incl. `full_model.apr`) | - | `fetch_full_weights.py` (needs network) |
| `pv` | contract validation | not on PATH; `target/release/pv` present | - | pin `target/release/pv` or build |
| cargo-pmcp | not used (R4) | yes 0.28.1 | - | - |
| `aprender-embed-conformance` | acceptance | no (pmcp.run repo) | - | blocked on brief |

**Missing dependencies with no fallback:** requester's conformance tool and brief; owner AWS credentials for the live deploy.
**Missing dependencies with fallback:** `deploy-extensions/node_modules` (`npm ci`); `pv` on PATH (use the built binary).

## Validation Architecture

`workflow.nyquist_validation` is `true` in `.planning/config.json`.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in tests via `cargo test` / `cargo nextest` (CI uses nextest `--profile ci`); Python for the reference script; offline recipe self-tests |
| Config file | workspace `Cargo.toml`; `.config/nextest.toml` if present (unchanged); `ci/explicit-test-commands.d/` for integration targets |
| Quick run command | `cargo test -p aprender-embed-endpoint --lib` (crate name is a placeholder until the plan fixes it) |
| Full suite command | `cargo nextest run --profile ci --workspace --lib --exclude aprender-gpu --exclude aprender-cuda-edge --exclude aprender-compute` plus `cargo test -p aprender-core --test monorepo_invariants --test readme_contract` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| R1 | API-GW-v2 event in, OpenAI list out; routes, methods | unit (pure `handle`) | `cargo test -p aprender-embed-endpoint --lib handler::` | Wave 0 |
| R1 | response/field shape, `index`, `usage`, base64 vs float | unit | `cargo test -p aprender-embed-endpoint --lib envelope::` | Wave 0 |
| R2 | >= 1,600-wordpiece input: 200, seq 256, `truncated` | unit with real weights, env gated | `APRENDER_MINILM_DIR=... cargo test -p aprender-embed-endpoint --test real_weights` | Wave 0 |
| R2 | parity vs committed reference vectors <= 3.73762473e-05 | integration, env gated | same | Wave 0 (needs reference JSON) |
| R2 | micro-batch / repeat determinism | integration | same | Wave 0 |
| R2 | tape stays empty after a request (`graph_tape_len() == 0`) | unit (tiny fixture) | `cargo test -p aprender-embed-endpoint --lib no_grad::` | Wave 0 |
| R3 | bundle values equal recomputed hashes; vocab.txt line i == tokenizer id i | recipe + script | `just embed-bundle-check` | Wave 0 |
| R3 | reference vectors regenerate to the same sha | back-office | `just embed-reference` (uv, frozen) | Wave 0 |
| R4 | synthesised template has no forbidden type / no `*` principal, with planted-violation control | python on `cdk.out` | `just synth-embed dev && just embed-ingress-scan` | Wave 0 |
| R4 | live readback: `function_url: absent`, no `*` in `get-policy` | owner-run | `just readback-embed dev` | manual-only (needs AWS) |
| R5 | hostile event table (non-object, missing body, bad base64, bad UTF-8, huge body, wrong method/path, wrong types, unknown fields, empty input) -> envelope, never `Err` | unit table | `cargo test -p aprender-embed-endpoint --lib hostile::` | Wave 0 |
| R5 | injected panic returns 500 envelope | unit | `cargo test ... panic_is_data` | Wave 0 |
| R5 | maximal response < 6 MB | unit | `cargo test ... max_response_bytes` | Wave 0 |
| R6 | cold/warm latency 1 and 64 inputs, memory, timeout | live, owner-run | `just embed-measure dev` (driver modelled on `llm-endpoint-measure`) | manual-only (needs AWS) |
| R7 | requester's 13/13 | external tool | `just aprender-embed-conformance lambda://... <vectors.json>` | blocked on brief |
| Gates | contract valid; constants equal; memory equals contract tier | `pv` + unit | `target/release/pv validate contracts/embed-endpoint-boundary-v1.yaml`; `cargo test ... constants_match_contract` | Wave 0 |
| Gates | deployment-unit register, duplicate-bin allowlist, README counts | existing tests | `cargo test -p aprender-core --test monorepo_invariants --test readme_contract` | exists (edits needed) |
| Gates | recipes fail closed | offline self-test with `aws` shim and recorder | `just embed-selftest` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p <crate> --lib` plus `cargo clippy -p <crate> --lib -- -D warnings` (CLAUDE.md strict clippy per crate).
- **Per wave merge:** the full suite command above and the offline recipe self-test.
- **Phase gate:** full suite green, mutation check on the PR diff (`MUTANTS_MAX_MISSED` default 0), `pv lint`, template scan, then owner-run live steps and the requester's run.

### Wave 0 Gaps
- [ ] New crate with `tests/` and a hostile-event table; fixture events (copy `deploy/llm-endpoint/events/apigw-v2-no-auth.json` shape).
- [ ] Reference-vector script and a committed long-input fixture (so the truncated real-weight claim is repo-owned).
- [ ] `contracts/embed-endpoint-boundary-v1.yaml` (+ `pv validate`).
- [ ] CDK stack, `build-embed-asset`, template scan with control, readback and selftest recipes.
- [ ] Graviton measurement harness (modelled on `llm-endpoint-measure`, `.ids` evidence JSON, key names only).
- [ ] Core additive API (Open Question 4) with its own tests; run `setfit-encoder-conformance-v1` suites.
- [ ] `npm ci` in `deploy-extensions`.
- [ ] Obtain the brief and conformance tool (blocking for R7).

## Security Domain

`security_enforcement` is enabled (absent = enabled); `security_asvs_level` is 1.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no (no end-user auth) | caller identity is AWS IAM SigV4 on `lambda:Invoke` |
| V3 Session Management | no | stateless |
| V4 Access Control | yes | IAM identity policy on the function ARN; no resource policy / no `*` principal; scan + readback |
| V5 Input Validation | yes | bounds before tokenization (count, per-input bytes, total bytes), strict type checks on a `serde_json::Value`, token budget after tokenization; refusal messages name the bound, never echo text |
| V6 Cryptography | partial | `sha2` for identity pins (never hand-rolled); no secrets |
| V7 Error handling and logging | yes | stable envelopes, no panic text to callers, request text and `user` never logged |
| V8 Data protection | yes | inputs are not stored; response contains vectors only |
| V10/V14 Config and dependencies | yes | pinned bytes (revision, sha256s), `Cargo.lock`, frozen `uv.lock`, `publish = false` |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Oversized/hostile input for CPU or memory DoS (5.56 MB input cost 1.74 s / 1.47 GB) | Denial of service | per-input byte cap, count cap, total-bytes cap, token budget, deadline check |
| Memory exhaustion from autograd tape in a warm environment | Denial of service | `no_grad` + `clear_graph` + test on `graph_tape_len` |
| Public exposure by drift (someone adds a Function URL) | Elevation of privilege / Info disclosure | template scan in CI-able recipe, owner readback evidence, `destroy` + containment (reserved concurrency 0) |
| Model substitution (tampered weights) | Tampering | sha256 pins in `release-identity.json` checked at load, tokenizer hash pin in `from_pretrained_dir`, CodeSha256 in the bundle |
| Cost amplification by an authorised-but-buggy caller | Denial of service | reserved concurrency cap (optional), bounded request work |
| Sensitive text in logs | Information disclosure | never log bodies; log counts, timings, identity |
| Stale-binary deploy (wrong binary under the right name) | Spoofing | compile-log/bootstrap name check, identity header, post-deploy probe as in `laya-deploy` |
| Unhandled-error info leak | Information disclosure | generic 500 message, details to CloudWatch only |

## Project Constraints (from CLAUDE.md)

Actionable directives the plan must satisfy:
- Scripts and recipes in **`justfile`** (user global CLAUDE.md); repo shell scripts use `bashrs`, `set -euo pipefail`, quoted variables; no new `.sh` without need (CONTEXT).
- **Pin the binary**: `. scripts/apr_bin.sh || exit 1` -> `"$APR"`; never a bare `apr`; pin `pv` too (not on PATH here).
- **Code search via `pmat query`**, not bare grep, where practical; contracts through **`pv`**, never bash/yq reimplementation.
- **No `unwrap()`** (`.clippy.toml` disallowed-methods); `unsafe_code = "deny"` (a throwaway probe using `unsafe` failed to compile here, confirming it); strict clippy per crate: `cargo clippy -p <crate> --lib -- -D warnings`; also current-stable clippy ceiling gate.
- **Realizar-first**: inference/serving belongs to `realizar` except the documented exception rows; this endpoint is a new exception and needs a CLAUDE.md row, argued, with a claims-ledger row.
- **Derive the numbers; do not quote them** in docs; readme_contract gates counts and cited paths (every repo-relative path cited in CLAUDE.md must exist).
- **Verification discipline**: never read `$?` through a pipe; prove the mechanism engaged (trace line, version+SHA, behaviour delta); one failing input is an anecdote; a shadowed artifact is worse than a missing one; extending a guard's scope needs re-mutation; guards need a case table.
- **Measure before reporting a cause**; report timings, do not gate on them unless the requester's numbers say so.
- **Autonomous mode**: modifying `.github/workflows/*.yml`, destructive ops, compute spend > 1 hr, and anything contradicting a user instruction are check-ins; live AWS steps are owner-run (CONTEXT).
- Mutation testing on the PR diff (`MUTANTS_MAX_MISSED`), coverage floor 88%, `pmat comply`; coverage must co-evolve with contracts.
- New integration test targets need a fragment under `ci/explicit-test-commands.d/`.
- `publish = false`; root-anchored `.gitignore` patterns (CB-510).
- Main is protected: branch, PR, CI.

## Sources

### Primary (HIGH confidence) - read or run this session
- `crates/aprender-core/src/setfit/{tokenizer,mod,import,artifact,classify,encoder}.rs` (Read; line numbers cited)
- `crates/aprender-core/src/autograd/mod.rs:55-145` (`GRAPH`, `GRAD_ENABLED`, `no_grad`, `clear_graph`, `graph_tape_len`)
- `crates/aprender-core/tests/monorepo_invariants.rs:255-480`, `readme_contract.rs` (test list)
- `contracts/setfit-encoder-conformance-v1.yaml` (tolerance policy, SEC-EQ-011), `contracts/decide-tool-boundary-v1.yaml` (header + key structure), `contracts/forecast-tool-boundary-v1.yaml` (key structure)
- `crates/aprender-mcp-setfit{,-lambda}`, `crates/aprender-mcp-decide-lambda/src/{bootstrap,lib,s3}.rs`, `crates/aprender-setfit-train-lambda`
- `deploy-extensions/bin/app.ts`, `lib/setfit-training-stack.ts`, `justfile` (lines 1-282, 1885-2146, 3540-3600, 4140-4346, 4897-4963), `deploy/llm-endpoint/{README.md,events/,gpu-host/proxy/lambda_function.py,.ids/evidence/R1.json}`
- `scripts/setfit_fixtures/{pyproject.toml,README.md,fetch_full_weights.py,uv.lock}`, HF snapshot and `~/.cache/aprender/minilm-l6-v2-1110a243` (hashed)
- `~/.cargo/registry/src/.../lambda_runtime-0.13.0/src/{lib,runtime}.rs`
- `.planning/phases/09-.../2026-10-18-pmcp-run-multi-size-change-request.md`, `.planning/quick/261023-aq0-*/261023-aq0-SUMMARY.md`
- `.claude/skills/spike-findings-aprender/` (SKILL.md, `llm-classifier-lambda-deployment.md`, `aws-mcp-model-hosting.md`, `chronos-mcp-server.md`)
- Local runs: throwaway probe (deleted), sentence-transformers reference run, `gsd-tools query package-legitimacy check`

### Secondary (MEDIUM confidence) - official docs fetched
- https://docs.aws.amazon.com/lambda/latest/dg/gettingstarted-limits.html (payload, memory, timeout, package, ephemeral limits)
- https://docs.aws.amazon.com/lambda/latest/dg/invocation-sync.html (sync invoke, FunctionError semantics, invalid JSON payload)
- https://developers.openai.com/api/reference/resources/embeddings/methods/create (request/response fields and limits; `platform.openai.com` returned HTTP 403)

### Tertiary (LOW confidence)
- Training-memory items listed in the Assumptions Log (A4-A9).

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - every component already exists in the lock/tree and was exercised here
- Architecture: HIGH for the handler/deploy shape (precedents read), MEDIUM for the exact event shape the platform sends
- Pitfalls: HIGH for tape growth, truncation, head requirement, register gates (measured/read); MEDIUM for Lambda service behaviours
- Lambda latency/memory: LOW - bracketed only, one real invoke settles it
- Acceptance (R7): LOW - the 13 checks are not in hand

**Research date:** 2026-10-23
**Valid until:** 2026-11-22 for tree facts (30 days); the latency brackets are superseded by the first Graviton measurement
