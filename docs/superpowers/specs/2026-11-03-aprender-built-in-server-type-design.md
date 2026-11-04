# The `aprender` built-in server type: configuration, crates and code samples

Status: approved for building by the owner on 2026-11-03 ("build the crate"); the core crate
`crates/aprender-mcp-builtin` is built on this side to the contract of §6.2 as revised below
(plan `docs/superpowers/plans/2026-11-03-aprender-mcp-builtin.md`; the contract file is
`contracts/aprender-builtin-v1.yaml`). The platform side pins it by git revision once the
owner pushes the branch. It is the type-level design the umbrella
(`docs/superpowers/specs/2026-11-02-decider-lifecycle-design.md`, §5.6, §7, §9 project 3)
promised. Where the two disagree, this document is the newer and names the deviation.

Revision, 2026-11-03 evening: the platform side built its half against the morning
draft (pmcp-run phase 209: admission, routing, buildspec with the precheck gate,
config-derived IAM, Admin UI, template, builder image, operator guide
`P/docs/guides/aprender-built-in-server.md`), on five spikes (033–037) whose findings
corrected this document in place: `main_with` takes four parameters (§6.2); the binary is
`target/lambda/bootstrap/bootstrap` and prints the `ERROR: REFUSED …` anchor itself
(§5.1, §5.2); `precheck::Env` carries the published-sha history and the predecessor comes
from the platform's S3 marker (§5.2, §5.3); `rollback-first-publish` refuses (§5.3); the
builder role holds `ListBucket` (§5.4, §8); the `verify` door reads the SDK's
`auth_context` and the `admin` group lives in a dedicated Cognito pool per server
(§4.5, §4.6); there are no `x-decide-*` headers (§4.7); ids are lowercase (§3.2, §6.2).
The platform's request and this side's reply are
`P/.planning/notes/aprender-team-{request,reply}-core-crate-rev-2026-11-03.md`.

Every fact below was read from the two repositories on 2026-11-03 and is cited as
`file:line`; `R/` is `~/Development/machine-learning/aprender` (the fork,
`github.com/guyernest/aprender`) and `P/` is `~/Development/mcp/sdk/pmcp-run`. "Decided"
marks the owner's decisions, "my call" the author's, "dev team's call" what this
document leaves to the platform side.

## 1. What this document is for, and who does what

The owner decided (2026-11-02) that deciders are served as **built-in servers** of a new
pmcp.run type, `aprender`, built from configuration only, and that the two live deciders
migrate to it later. The dev team owns the platform side; aprender owns the model side.
The line between them is drawn so that the dev team never touches ML code and the
aprender side never touches the builder, the Admin UI or the GraphQL API.

| Side | Owns | Delivered as |
|---|---|---|
| pmcp.run dev team | builder image, CodeBuild project and buildspec, state-machine branch, SSM parameters, the `serverType` value, trigger-build allowlist, Admin UI wizard/editor entries, GraphQL, the template directory, the CDK deploy app | changes in `P/` |
| aprender (this side) | the core crate `aprender-mcp-builtin` (configuration parsing, model loading, the three tools, the build-time check), the small changes it needs in the existing decide crates | a pinned git revision of `R/` |

The interface between the two sides is exactly: the configuration schema (§3), the core
crate's public API (§6.2), the deploy constants the buildspec writes (§7), the
environment variables the buildspec passes (§5.4, §7), and the IAM the CDK app derives
from the configuration (§8). Everything else is internal to one side.

## 2. The type in one paragraph, with its numbers

One built-in server serves one `.apr` model under one configuration. The configuration
is embedded in the binary at compile time, like every other built-in type
(`P/built-in/builder/templates-sql/main_athena.rs:13`). The weights are never in the
zip: they stream from S3 into memory at the first `classify` or `verify` call, the
sha256 is checked before the bytes are parsed, and the model stays loaded for the life
of the execution environment. Tools: `classify` (open; unchanged from today's servers),
`status` (open; configuration, load state, build id, never triggers a load) and `verify`
(admin; scores a few frozen eval rows through the served path against the training
run's stored probabilities and writes a verification record). A new model goes live by
publishing a configuration that names a new sha; the build runs a prerequisite check and
fails, naming the missing prerequisite, instead of deploying an ineligible model.
Staging and prod are two servers, possibly in two accounts.

| Fact | Value | Source |
|---|---|---|
| Artifact size (F16 weights, tokenizer inside) | 846,220,868 bytes (`antisemitism-v1`); 846,196,612 (`motivation-v1`) | `R/.planning/phases/10-*/10-RENAME-DEPLOY-EVIDENCE.json:594-599`, `12-*/12-DEPLOY-EVIDENCE.json:45` |
| Artifact cap | 1,342,177,280 bytes | `R/contracts/decide-apr-v1.yaml:78-82` |
| Peak RSS on load | 2,454–2,456 MB (download buffer + f32 model; buffer dropped after build) | `10-RENAME-DEPLOY-EVIDENCE.json:312-393` |
| Lambda memory / timeout | 10,240 MB / 30 s (pinned to the contract by FALSIFY-DECIDE-TOOL-007) | `R/crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template:62,68`; `tests.rs:367` |
| Cold load inside the function | 15,582 ms = download 9,226 + sha 3,484 + build 2,786 | `10-RENAME-DEPLOY-EVIDENCE.json:312` |
| Cold call seen by the client | 19,759–25,499 ms against the 30,000 ms gateway cap | same file `:306-387` |
| Warm call | 1.08–1.15 s | `R/CLAUDE.md` decision-model row |
| Per-call bounds | 1–8 texts, 16,384 bytes per text, 800 built tokens per call, 512 tokens per row | `R/crates/aprender-mcp-decide/src/lib.rs:124-131`; `R/contracts/decide-tool-boundary-v1.yaml:313-377` |
| Architecture | aarch64 only; every parity number is aarch64, x86_64 is unmeasured | `R/CLAUDE.md`; `R/justfile:30` |
| Platform builder and runtime | native `aarch64-unknown-linux-gnu` CodeBuild image, `cargo lambda build --release --arm64`, `provided.al2023` `ARM_64` | `P/built-in/builder/Dockerfile.toolchain:62`; `build-project.ts:1253`; `amplify/functions/upload-deployment/synthesis/synthesize.ts:373-376` |

The architectures match. That is the single fact that makes this type possible without
re-measuring parity.

## 3. The configuration

### 3.1 Two complete examples

A staging server, in the trainer's account, reading the artifact and the records
straight from the training run's prefix:

```toml
id = "antisemitism-staging"
schema = "aprender-server/v1"
environment = "staging"

[model]
kind = "decide"
model_id = "antisemitism-v2"
artifact_uri = "s3://aprender-laya-train-dev-123456789012/runs/antisemitism-v2/task-01jbx.../antisemitism-v2.apr"
artifact_sha256 = "22796ff94a0839f47c4308f0b88c73b43aaff1f21b760d78e434dac332909eed"
labels = ["not_antisemitic", "antisemitic"]
description = "Antisemitism decider, GoldStandard2024 plus reviewer feedback through 2026-11."

[records]
pack_outcome_uri = "s3://aprender-laya-train-dev-123456789012/runs/antisemitism-v2/task-01jbx.../pack-outcome.json"
eval_jsonl_uri   = "s3://aprender-laya-train-dev-123456789012/runs/antisemitism-v2/task-01jbx.../data/eval.jsonl"
eval_probs_uri   = "s3://aprender-laya-train-dev-123456789012/runs/antisemitism-v2/task-01jbx.../run/eval-probs.json"
verification_prefix = "s3://aprender-laya-train-dev-123456789012/runs/antisemitism-v2/task-01jbx.../verifications/"

[publish]
reason = "Phase 16 SageMaker control model; staging for the next experiment."
```

A prod server, in another account, after the operator copied the artifact and the
records next to each other:

```toml
id = "antisemitism-prod"
schema = "aprender-server/v1"
environment = "prod"

[model]
kind = "decide"
model_id = "antisemitism-v2"
artifact_uri = "s3://aprender-decide-weights-210987654321-prod/decide/antisemitism/22796ff9...9eed.apr"
artifact_sha256 = "22796ff94a0839f47c4308f0b88c73b43aaff1f21b760d78e434dac332909eed"
labels = ["not_antisemitic", "antisemitic"]
description = "Antisemitism decider, GoldStandard2024 plus reviewer feedback through 2026-11."

[records]
pack_outcome_uri = "s3://aprender-decide-weights-210987654321-prod/decide/antisemitism/22796ff9...9eed/pack-outcome.json"
eval_jsonl_uri   = "s3://aprender-decide-weights-210987654321-prod/decide/antisemitism/22796ff9...9eed/eval.jsonl"
eval_probs_uri   = "s3://aprender-decide-weights-210987654321-prod/decide/antisemitism/22796ff9...9eed/eval-probs.json"
staging_verification_uri = "s3://aprender-decide-weights-210987654321-prod/decide/antisemitism/22796ff9...9eed/staging-verification.json"
comparison_uri   = "s3://aprender-decide-weights-210987654321-prod/decide/antisemitism/22796ff9...9eed/comparison.json"
verification_prefix = "s3://aprender-decide-weights-210987654321-prod/decide/antisemitism/22796ff9...9eed/verifications/"

[publish]
kind = "promote"
reason = "Experiment 3 ended 2026-11-20; verdict clearly_better on recall at the 0.95 precision floor."
baseline_sha256 = "2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e"

[tools]
verify_required_role = "admin"
verify_allowed_principals = ["3k9f2...m2m-client-id"]   # the deploy-verify recipe's M2M client
```

The same prod server rolled back a week later, by restoring the previous configuration
and editing only `[publish]`:

```toml
[publish]
kind = "rollback"
reason = "Recall regression on the experiment-4 stream; back to 2293bebb until retrained."
baseline_sha256 = "22796ff94a0839f47c4308f0b88c73b43aaff1f21b760d78e434dac332909eed"
```

### 3.2 Fields

Parsing is strict: unknown keys and unknown sections are refused
(`#[serde(deny_unknown_fields)]`, the coach-family rule), so a typo cannot silently
become a no-op.

| Field | Required | Rule |
|---|---|---|
| `id` | yes | `^[a-z][a-z0-9-]{0,62}$`: the server id, lowercase because the platform slugifies ids (its buildspec grammar `^[a-zA-Z][-a-zA-Z0-9]*$` is the wider superset). The platform derives `SERVER_ID` from the first top-level `id = "..."` line of the configuration for every TOML type (`P/amplify/shared/server-config-link.ts:29-37`, used by the trigger-build handler `:150-166`) and falls back to the record's UUID, which fails the buildspec grammar (`build-project.ts:1146`) whenever it starts with a digit, so the key is required and must be the first line. It is the Lambda function name, the host name, `status.server` and the `server` of every record |
| `schema` | yes | exactly `aprender-server/v1` |
| `environment` | yes | `staging` or `prod`; selects the prerequisite set in §5 |
| `[model].kind` | yes | `decide` in v1. Reserved for later kinds (`setfit` is the next candidate, §11); any other value is refused at parse time |
| `[model].model_id` | yes | `^[a-z0-9][a-z0-9._-]{0,63}$` (the trainer's name grammar); free of meaning to the server, recorded in every record it writes |
| `[model].artifact_uri` | yes | `s3://bucket/key`, key ending in `.apr` |
| `[model].artifact_sha256` | yes | 64 lowercase hex; the pin checked before the bytes are parsed (`R/crates/aprender-mcp-decide-lambda/src/lib.rs:565-602`) |
| `[model].labels` | no | when present, must equal the manifest's `labels` in order at load time, else the server refuses to serve (§4.5). Recommended: it is what the analyst sees in the UI |
| `[model].description` | no | ≤ 500 characters; prepended to the `classify` tool description |
| `[records].pack_outcome_uri` | yes | the Graviton pack task's `pack-outcome.json` for this artifact (§5) |
| `[records].eval_jsonl_uri` | yes | the frozen eval rows the run was gated on (`data/eval.jsonl` under the run prefix) |
| `[records].eval_probs_uri` | yes | the run's stored fine-tuned probabilities (`run/eval-probs.json`) |
| `[records].rescore_noise_uri` | no | **refused in v1** (`config-invalid`): the strict parser never accepts a key it would ignore. The intent stands for a later version: when read, `verify`'s tolerance becomes `max(floor, 4 × noise)` as in `R/contracts/laya-parity-v1.yaml:138-157`; until then the noise-referenced bound is the pack task's and `verify` uses `[verify].tolerance_abs` (final review, 2026-11-03) |
| `[records].comparison_uri` | prod: yes unless `baseline_sha256 = "first_model"` | the trainer's comparison record (§5.2) |
| `[records].staging_verification_uri` | prod: yes | a verification record (§4.4) written by a staging server for the same sha |
| `[records].verification_prefix` | no | `s3://bucket/prefix/`; where this server writes verification records. Without it, `verify` scores and reports but writes nothing |
| `[publish].kind` | no | `promote` (default) or `rollback`. A rollback re-publishes a sha this server served before; it needs the pack record and a reason, not a comparison or a staging verification (§5.2) |
| `[publish].reason` | yes | non-empty, ≤ 1,000 characters; recorded in the platform's `ConfigVersion` and reported by `status` |
| `[publish].baseline_sha256` | prod: yes | 64 hex or the literal `first_model`: the sha this publish replaces. Must equal the previously published sha of this server when the platform tells the check what that was (§5.3); for a `rollback` it is the sha being rolled back from |
| `[publish].clears_hold` | no | `true` lets a publish that changes the artifact pass while `[hold]` is present; the reason says why |
| `[hold].reason` | no | while present, a publish that names a different `artifact_sha256` than the previous published one, promotion or rollback, is refused unless `clears_hold = true` (a rule, not a lock: umbrella §5.6) |
| `[tools].verify_required_role` | no | default `admin`; the group (in the server's own Cognito pool) a caller must carry in `auth_context.groups` to call `verify`; it is also the role named in the refusal text (§4.5, §4.6) |
| `[tools].verify_allowed_principals` | no | Cognito app client ids of the server's own pool, matched by exact equality against `auth_context.subject`, checked before the group test: the door for machine-to-machine tokens, which carry no groups (§4.6) |
| `[tools].status_required_role` | no | default none (open) |
| `[verify].tolerance_abs` | no | default `5.0e-5` (the fine-tuned floor of `laya-parity-v1` amendment A2); ceiling `1.0e-3` |
| `[verify].default_rows` | no | default 8, maximum 8 (the `classify` door) |

Deliberately **not** in the configuration:

- Memory, timeout, architecture and `/tmp` size. They are per-type constants the
  buildspec writes (§7), as for every other type, and the memory is pinned to the
  tool-boundary contract. The `[deployment]` fields the generic config form offers
  (`P/components/built-in-servers/config-form/types.ts:21-26`) are not read by any
  buildspec today and must not be for this type either.
- Per-call bounds. The umbrella sketched a `[bounds]` section; my call is to drop it in
  v1. The bounds are the kind's contract (`decide-tool-boundary-v1`), the only public
  door of the classify library serves `ClassifyLimits::CONTRACTED`
  (`R/crates/aprender-mcp-decide/src/lib.rs:779-781`), and a configurable bound would
  re-open that contract. `status` reports the bounds in force.
- Environment variables. The runtime reads none for the model; `RUST_LOG` and the build
  id are the only ones the buildspec sets (§7).
- Secrets. None.

### 3.3 Why explicit record URIs rather than a prefix

A staging server reads the records where the trainer wrote them,
`runs/<name>/<task_id>/` (`R/crates/aprender-mcp-laya-train/src/layout.rs:91-96`), and a
prod server reads copies in its own account, because no built-in type reads S3 across
accounts and none assumes a role (umbrella §7). Naming each object makes the
configuration account-agnostic and makes the IAM derivation (§8) a list of exact ARNs
with one prefix for writes. When the trainer's "datasets from parts" project lands and
writes `models/<model_id>/`, the URIs simply point there.

## 4. Runtime behaviour

### 4.1 Startup and lazy load

The template's `main` parses the embedded TOML, builds the pmcp `Server` with the three
tools, and hands it to the platform's `mcp_lambda_proxy::run_lambda(server, name)`
(`P/built-in/shared/mcp-lambda-proxy/src/lib.rs:34`), which starts the stateless
streamable-HTTP loop on `127.0.0.1:8080` and proxies Lambda events to it, stripping
inbound `x-pmcp-*` headers and re-stamping the verified identity and groups
(`lib.rs:116-121, 360, 397`). This replaces the decide crates' own `bootstrap` module
(`lambda_http` plus a reqwest loopback proxy), which does the same job without the
platform's identity headers. My call: the type uses the platform's proxy so that the
`verify` door reads the identity the proxy verified (§4.6). The same `pmcp::Server` is
also served directly on `127.0.0.1:8081` by `bootstrap --serve-local` (the template's
own closure, no Lambda, no identity), which is how the platform's local smoke drives
the handshake and the refusal grammar before any deploy (dev team, plan 209-07).

The model is loaded lazily by whichever of `classify` or `verify` is called first, in
the handler, through a process-wide `LoadOnce` (`R/crates/aprender-mcp-decide-lambda/src/lib.rs:758-813`):
`ModelSource` built from the configuration (`ModelSource::from_lookup`, `lib.rs:359`),
`resolve_model_within(source, budget)` (`lib.rs:725`), which streams the artifact in
64 MiB ranged GETs with 16 in flight and a fixed 10,739 ms download deadline
(`s3.rs:40-75`), hashes the bytes, refuses `HashMismatch` before the ladder, and builds
the `Decider` (`lib.rs:565-602`). `initialize` and `tools/list` do not load, so a
connector handshake from ChatGPT or Claude Desktop is instant; today's bootstrap loads
on any POST (`bootstrap.rs:130-252`). A failed load leaves the cell empty, the call
returns an error naming the kind (`config_error`, `download`, `hash_mismatch`,
`artifact`), and the next call retries. The Lambda init phase does nothing heavy, so
the 10 s init cap is never in play.

Why a fixed deadline is enough: today's bootstrap caps the download at
`min(10,739 ms, remaining − reserve)` using the invocation context
(`lib.rs:705-707`). Behind the proxy the handler has no context, but the load starts
within milliseconds of the first call, so the fixed deadline plus the measured
post-download reserve (15,261 + 4,000 ms, `s3.rs:55-63`) keeps the first call inside the
30 s cap by the same margin as today.

### 4.2 `classify`

Unchanged. Arguments `{ "texts": [..] }` with `deny_unknown_fields`
(`R/crates/aprender-mcp-decide/src/lib.rs:89-96`); response `{ model: {artifact_sha256,
recipe_id, method, base}, labels: [..], results: [{label, probabilities, tokens,
truncated}] }` (`lib.rs:431-465`); arrays in task order, never maps; bound refusals as
`tool_rejected` results naming the contract key and the observed value, never caller
text (`lib.rs:137-185`); one in flight and four pending per process (`lib.rs:124-131`).
The handler is the existing `ClassifyService` (`lib.rs:579`) constructed at load; the
tool description is `[model].description` followed by the labels and the bounds.

### 4.3 `status`

Open by default. Returns, without loading:

```json
{
  "schema": "aprender-status/v1",
  "server": "antisemitism-staging",
  "environment": "staging",
  "model": { "kind": "decide", "model_id": "antisemitism-v2",
             "artifact_sha256": "22796ff9...", "labels": ["not_antisemitic", "antisemitic"] },
  "loaded": true,
  "load": { "performed_at_unix": 1795255391, "download_ms": 8153, "sha_ms": 3472, "build_ms": 2857,
            "bytes": 846220804, "rss_mb": 2485, "peak_rss_mb": 2485, "graviton": "2",
            "source": "s3", "artifact_sha256": "22796ff9..." },
  "bounds": { "max_texts": 8, "max_text_bytes": 16384, "max_total_tokens": 800,
              "max_in_flight": 1, "max_pending": 4 },
  "publish": { "kind": "promote", "reason": "...", "baseline_sha256": null },
  "hold": null,
  "identity": { "subject": "...", "groups": ["admin"] },
  "build": { "build_id": "pmcp-aprender-builder:3f9a...", "config_sha256": "...", "crate_version": "0.69.3" },
  "records": { "pack_outcome_uri": "...", "eval_jsonl_uri": "...", "eval_probs_uri": "...",
               "verification_prefix": "..." }
}
```

`load` is null until the first load; `build.build_id` is the CodeBuild id the buildspec
passes (§7) and `config_sha256` the sha256 of the embedded TOML, so the chat can tell
which publish it is talking to. `identity` echoes the non-secret part of the verified
caller (`auth_context.subject` and `groups`, never a token), so an operator can see
from the chat what `verify` would see (the shape the platform's spike 033 harness
measured; `load` is that harness's measured shape too).

### 4.4 `verify`

`required_role = "admin"` by default. Arguments:

```json
{ "rows": 8, "seed": 0, "write_record": true }
```

Behaviour: read `eval_jsonl_uri` and `eval_probs_uri` (each capped at 32 MiB, the
trainer's per-file cap); choose `rows` row indices deterministically from `seed`
(a seeded shuffle of the row indices, the first `rows` of it, sorted, so a sample is
reproducible from its seed; a selected row the classify door refuses, by bytes or by the
800-token budget, is reported in-band as a verify refusal naming the door's bound, and
the caller picks another seed or fewer rows: pre-filtering by built tokens would need a
tokenizer door the classify crate does not expose; final review, 2026-11-03); score them through the
same `ClassifyService` as `classify`; compare each probability vector with the stored
one for that row (the `eval-probs.json` reader is the one in `aprender_decide::verify`,
made public for this crate); `max_abs_diff ≤ tolerance_abs` is `pass`. With
`write_record` and a `verification_prefix`, write
`<verification_prefix><utc-stamp>-<sha12>.json` create-only (`If-None-Match: *`), where
`sha12` is the first 12 hex of the artifact sha, and return the record:

```json
{
  "schema": "aprender-verification/v1",
  "server": "antisemitism-staging",
  "environment": "staging",
  "artifact_sha256": "22796ff9...",
  "model_id": "antisemitism-v2",
  "eval_jsonl_sha256": "...", "eval_probs_sha256": "...",
  "rows_scored": 8, "row_indices": [17, 203, 611, 940, 1288, 1402, 1777, 2090],
  "max_abs_diff": 2.03e-6, "tolerance_abs": 5.0e-5, "pass": true,
  "verified_at": "2026-11-20T10:05:40Z",
  "verified_by": "analyst@example.org",
  "build_id": "pmcp-aprender-builder:3f9a..."
}
```

`verified_by` is the verified caller's email when the proxy stamped one, else its
subject (the Cognito `sub`, which for a client-credentials caller is the app client
id): a real identity either way, unlike the caller-supplied `annotator` in
feedback-coach (umbrella §8). Who calls it, my call: `verify` is the
ML-Ops role's tool, and in the first version that is the owner, who is a platform
admin, from the chat; and the deploy-verify recipe through its M2M client, allow-listed
in `[tools].verify_allowed_principals`. Analysts do not verify; a staging verification is
the ML-Ops step before a prod publish (umbrella §4 step 8). What this proves: the
served path on this build, this architecture and this toolchain reproduces the
training run's probabilities on real rows. It is the deployed-binary guard: the
builder's rustc is 1.97.1 (`P/built-in/builder/Dockerfile.toolchain:32-33`) and the
fork's pin is 1.93.0 (`R/rust-toolchain.toml:2`), and a refactor that hashed identical
on the tiny fixture once moved the parity gate by one ULP on real weights (2026-10), so
"same source" is not "same numbers" until the deployed binary is measured. A full-set verification stays
the pack task's and `just laya-verify`'s job (87.5 s on Graviton, about 24 min on the
Mac; `R/crates/aprender-mcp-laya-train/fixtures/pack-outcome-v1.json`,
`10-LIVE-DEPLOY-EVIDENCE.json:467`).

### 4.5 Refusals and errors

| Situation | Wire form | Name |
|---|---|---|
| a `classify`/`verify` bound | `tools/call` result with `isError: true`, text names `contracts/decide-tool-boundary-v1.yaml`, the key and the observed value | as today (`lib.rs:137-185`) |
| configuration refused at startup | cannot happen on a deployed build: `precheck` parses the same bytes and exits 3 before anything deploys (§5). If it ever did, the process exits non-zero and every invocation errors; there is no post-deploy probe for template-mode built-ins | `aprender-config-invalid` |
| artifact sha mismatch at load | tool error, `kind = hash_mismatch`; nothing served | `aprender-artifact-hash-mismatch` |
| manifest labels ≠ `[model].labels` | tool error, `kind = config_error`; nothing served | `aprender-labels-mismatch` |
| manifest `model_type` ≠ `[model].kind` | tool error, `kind = artifact` (the ladder's rung 3) | `aprender-kind-mismatch` |
| download over deadline or S3 failure | tool error, `kind = download`; next call retries | `aprender-download-failed` |
| caller not admitted to `verify` | a JSON-RPC error (not a tool result): `pmcp::Error::internal(...)`, code −32603, message `Permission denied: verify requires '<role>' role` with the configured role (`admin` by default), the GraphRAG wording. The same error for a caller with no identity at all (an open server, or `--serve-local`). The platform's smoke and live checks match on this text and require a code other than −32602 | `permission-denied` |
| `verify` record already exists | the record is still returned; `written: false` with the reason | `verification-exists` |

### 4.6 Per-tool roles

Corrected 2026-11-03 by the platform side's spike 035 (what a tool actually receives
through `mcp-proxy → mcp-lambda-proxy → pmcp 2.19`, observed live for a user token):

- **Read the SDK's `extra.auth_context`, never `extra.metadata`.** With no auth provider
  configured, pmcp 2.19's stateless server fills `AuthContext` from the proxy's
  `x-pmcp-user-id`, `-user-email`, `-user-name`, `-user-groups` and `-tenant-id`
  headers (`streamable_http_server.rs:3062-3140`), while `RequestHandlerExtra.metadata`
  is constructed empty and nothing fills it (`core.rs:9425`). The GraphRAG guard this
  design first copied reads `metadata` and would deny every caller on this path.
- **Allow-list before the groups test.** A client-credentials token carries no groups
  claim, so the proxy stamps no `x-pmcp-user-groups` for it; its `auth_context.subject`
  is the app client id (Cognito sets `sub` to it). The door is: admitted if any group
  equals `[tools].verify_required_role` (default `admin`), else if `subject` equals an
  entry of `[tools].verify_allowed_principals` (exact equality on client ids), else
  denied; no `auth_context` at all (an open server, `--serve-local`) is denied the same
  way.
- **The `admin` group lives in the server's own Cognito pool**, a new dedicated pool
  per `aprender` server (platform requirement R9-3), created when the operator enables
  OAuth for the server in the Admin UI; the operator creates the group and its users
  there, and provisions the deploy-verify recipe's machine client there (Machine
  Principal in the Admin UI, which also appends it to the server's `m2mClientIds`).
  Platform administrators (the management pool's `admin`/`platform-admin`) are not
  decider administrators; the 2026-06-13 pool-migration incident is why. The
  operator steps are the platform's guide `docs/guides/aprender-built-in-server.md` §3.

The claim key is therefore not a configuration knob in v1: the SDK's `AuthContext` is
the one source. The deploy-verify recipe's M2M path is live only once the server has an
OAuth row; today's crate-deployed dev deciders run open.

### 4.7 Logs and headers

One log line per load, the existing `decide.load performed_load=… download_ms sha_ms
build_ms bytes rss_mb peak_rss_mb graviton source` (`lib.rs:835`). There is no
`x-decide-load` response header: a pmcp tool cannot set response headers (spike 034),
so `status.load` is the carrier of the cold fact, and the in-band tell is `status`
before (`loaded: false`), the timed `classify`, `status` after (`loaded: true` plus the
timeline). The `x-decide-probe-id` request header is not readable either (`metadata` is
empty on this path, §4.6), so the deploy-verify recipe moves to the in-band tell; the
server-side witness is the `decide.load performed_load=true` line (the function logs in
JSON format, so there is no `REPORT RequestId:` line; filter on `platform.report`).

## 5. The build-time check (`precheck`)

### 5.1 Where it runs

The type's binary has one subcommand: `bootstrap precheck`. The Lambda runtime starts
`bootstrap` with no arguments; `bootstrap --serve-local` serves on localhost (§4.1);
any other argument is a usage error, exit 64. The buildspec runs the freshly built
binary, `target/lambda/bootstrap/bootstrap` (cargo-lambda names the output directory
after the bin, not the package; corrected by the dev team), with `precheck` on the
builder (an aarch64 Linux host, so the Lambda binary runs natively, and nothing is
compiled twice) after `cargo lambda build` and before `cargo pmcp deploy`. GraphRAG already gates its `build` phase this way (intent-mismatch and
`snapshot-validate`, `P/built-in/builder/.../build-project.ts:2063-2101`) and both
GraphRAG and workbook guard `post_build` with `CODEBUILD_BUILD_SUCCEEDING`
(`:2279-2285`, `:1857-1863`); this type needs the same guard. My call, against the
alternative of a TypeScript check in the trigger-build Lambda: the rules read records
that only exist in Rust form (`pack-outcome.json`, the comparison record, the
verification record), they must stay identical to what `status` reports at runtime, and
the records are in the account the build runs in, not the platform's control plane.

### 5.2 The rules

The binary prints the platform's error anchor itself (corrected by the dev team's
spike 036: the build-error extractor only sees lines matching a CloudWatch filter, and
`ERROR:` is one of its terms, while a bare `REFUSED …` line is dropped before the
handler runs). One grammar for every outcome, on stdout:

```text
WARN <rule> skipped: <why>                            # zero or more, first
ERROR: REFUSED <code>: <detail>                       # exit 2 (refused) and exit 3 (config-invalid)
ERROR: REFUSED s3-unreachable: <op> <uri>: <cause>    # exit 4 (store unreachable)
ELIGIBLE {"config_sha256": …}                         # exit 0, the one verdict line last
```

The buildspec runs the bare command and never re-echoes (a re-echo produced
`ERROR: ERROR s3-unreachable`). No line ever contains an object body: the `ELIGIBLE`
json carries sizes, shas, the pack facts and the verdict summaries, never a record's
contents. `config-invalid` names the offending key (`[section].key`, or serde's own
`unknown field \`memory_mb\`, expected one of …` for a mis-keyed field). A `HeadObject`
403 is reported with both causes named, "the object is missing, or the builder role
cannot read it", as `s3-unreachable` (exit 4); with the `s3:ListBucket` the platform
grants (§5.4) a missing object is a 404 and reads as the `-missing` rule it belongs to.

| Code | Applies to | Rule |
|---|---|---|
| `config-invalid` | all | §3.2; exit 3 |
| `s3-unreachable` | all | any store error other than a clean 404: `<op> <uri>: <cause>`; exit 4 |
| `artifact-missing` | all | `HeadObject` on `artifact_uri` fails |
| `artifact-too-large` | all | `ContentLength` > 1,342,177,280 (`decide-apr-v1` cap) |
| `pack-record-missing` | all | `pack_outcome_uri` absent or > 1 MiB |
| `pack-record-schema` | all | `schema ≠ "laya-train-pack-outcome-v1"` |
| `pack-record-not-eligible` | all | `accepted ≠ true` or `deploy_eligible ≠ true` |
| `pack-record-sha-mismatch` | all | `apr_sha256 ≠ [model].artifact_sha256` |
| `eval-missing` | all | `eval_jsonl_uri` or `eval_probs_uri` absent, or either > 32 MiB |
| `reason-missing` | all | `[publish].reason` empty |
| `hold-active` | all | `[hold]` present, the previous published sha is known and differs from `[model].artifact_sha256`, and `clears_hold ≠ true` |
| `baseline-mismatch` | prod | the previous published sha is known and ≠ `[publish].baseline_sha256` (and `first_model` is given while a previous sha exists) |
| `comparison-missing` | prod, `promote`, unless `first_model` | `comparison_uri` absent |
| `comparison-sha-mismatch` | prod, `promote` | `candidate_sha256 ≠ artifact_sha256` or `baseline_sha256 ≠ [publish].baseline_sha256` |
| `staging-verification-missing` | prod, `promote` | `staging_verification_uri` absent |
| `staging-verification-failed` | prod, `promote` | record's `artifact_sha256 ≠ artifact_sha256` or `pass ≠ true` or `environment ≠ "staging"` |
| `rollback-unknown-sha` | prod, `rollback` | the platform passed this server's published-sha history and `[model].artifact_sha256` is not in it. Skipped with a `WARN` when no history is passed |
| `rollback-first-publish` | prod, `rollback` | no previous published sha exists (there is nothing to roll back from) |

**Rollback is its own publish kind** (my call, from the reviewer's finding that a
restored configuration cannot pass the promote rules: its baseline names the sha it
replaced back then, not the one it replaces now, and no comparison record "A against
B" exists for the way back). A rollback re-publishes a sha this server served before,
so the comparison and the staging verification it had when it was promoted still
stand; what it needs is the pack record (deploy eligibility never expires), the hold
rule (a hold stops any model change, forward or back), a baseline naming the sha being
rolled back from, and a reason. The platform's version restore alone is therefore not
a rollback: the operator restores the old version and edits `[publish]` to
`kind = "rollback"` with the current sha as baseline (§3.1, third example). The
`ELIGIBLE` line and `status` carry `kind`, so the history shows which publishes went
backwards.

"Previously published" means the last sha the platform's **published-sha marker** holds
for this server: `published/<SERVER_ID>/sha256-history.txt` in the builder artifact
bucket, one 64-hex sha per line, oldest first, appended only after a deploy whose live
function carries the build's id (the dev team's decision, replacing this design's
earlier `BuildRecord.configContentSnapshot`: that record is behind a closed data stack,
and `currentBuildId` is overwritten at trigger time, so a refused build followed by a
retry would have reset the predecessor). Either way the meaning is the same: a
configuration the precheck refused was saved and built but never served, so it is not
"previous". The check reads the marker's contents only through the two environment
variables of §5.3; the marker, its lock and its pending record are the platform's
(guide §6).

The pack-record rules are `laya_eligibility.check_record` as it is
(`R/scripts/laya_deploy/laya_eligibility.py:72-106`), moved into Rust. The comparison
verdict is **shown, not enforced**: a `not_clearly_better` candidate may be promoted with
a reason (umbrella §5.6); `worse` is reported in the `ELIGIBLE` line and in `status` so
the history shows it was knowingly published.

The comparison record the prod rule reads is the trainer's, from the "datasets from
parts" project (umbrella §5.4). Until that project writes it, `first_model` is the only
way to publish a prod server, which is the honest state: today no comparison record
exists for any model. Its minimum shape, fixed here so both sides can build against it:

```json
{ "schema": "laya-compare/v1",
  "candidate_sha256": "...", "baseline_sha256": "...", "eval_sha256": "...",
  "positive_label": "antisemitic", "cost_rule": "false_flag_worse",
  "verdict": "clearly_better", "metric": "recall_at_precision_floor",
  "candidate": 0.620, "baseline": 0.551, "difference_interval_90": [0.012, 0.126] }
```

### 5.3 What the platform passes in

Three environment variables, all optional, exported by the buildspec from the
published-sha marker it read and validated (§5.2; `build-project.ts`,
`aprenderMarkerReadCommands` / `aprenderExportHistoryCommands`):

- `APRENDER_PREVIOUS_ARTIFACT_SHA256`: the marker's last line. Empty on a first build
  (the buildspec refuses to call it a first build while a function of that name is
  live, so an absent marker cannot bypass the rules). When absent, `hold-active` and
  `baseline-mismatch` are skipped with a `WARN` line, so the first publish of a server
  can never be blocked by a rule about its predecessor. `rollback-first-publish` is
  **not** skipped: it fires exactly then (a rollback with nothing to roll back from);
  an earlier revision of this section said otherwise and was wrong.
- `APRENDER_PUBLISHED_SHA256_HISTORY`: the marker's lines, comma-joined, oldest first.
  Adopted in `precheck::Env` (§6.2): it tightens `rollback-unknown-sha`, which is
  `WARN`-skipped only when the variable is unset.
- `APRENDER_BUILD_ID`: `$CODEBUILD_BUILD_ID`, recorded by the check and passed on to
  the runtime as `PMCP_BUILD_ID` in `[environment]` (§7).

The output grammar is §5.2's: the binary prints the `ERROR:` anchor itself and the
buildspec never re-echoes.

### 5.4 What the check needs

The CodeBuild role must be able to `HeadObject`/`GetObject` the URIs the configuration
names. The role is static while configurations vary, so the grant is on the bucket
families that hold artifacts and records, not derived per server:
`aprender-laya-train-<env>-<account>` and `aprender-decide-weights-<account>-<env>` in
the deploying account (the platform refuses any other bucket at synth as
`config-invalid`). The builder role also holds `s3:ListBucket` on those buckets
(the dev team's decision, correcting this section's earlier "no ListBucket"): without
it S3 answers a `HeadObject` on a missing key with 403, indistinguishable from a
denied read, and the admin would see `s3-unreachable` instead of
`pack-record-missing`. The check performs no writes. Both roles assume SSE-S3; a
KMS-encrypted bucket would need `kms:Decrypt` on both (known gap, guide §5).

## 6. Crates and code

### 6.1 The crate map and how the builder obtains it

What exists today (all in `R/`, all `publish = false`, none on crates.io; checked
2026-11-03 with `cargo search`):

| Crate | Role | Lines |
|---|---|---|
| `aprender-decide` | the `.apr` ladder, `Decider`, `verify`, `pack` (back office only) | ~15 k incl. tests |
| `aprender-mcp-decide` | `ClassifyArgs`/`ClassifyResponse`/`ClassifyLimits`, `ClassifyService`, `parse_args`, `build_server` | 822 + tests |
| `aprender-mcp-decide-lambda` | library: `ModelSource`, S3 streaming download, `resolve_model_within`, `LoadOnce`, `LoadTimeline`, the old `bootstrap` | 866 + `s3.rs` 914 |
| `aprender-mcp-decide-antisemite-lambda`, `-motivation-lambda` | 12-line shims; differ only in package name | 12, 15 |

Dependency closure of a decide Lambda binary: 485 crates (`cargo tree --edges normal`,
2026-11-03), of which the aprender ones are `aprender-core` (default features, so the
ModernBERT encoder and `parallel` are on), `aprender-compute`, `aprender-quant`,
`apr-format`, `aprender-common`, two proc-macro crates and the four above. Pinned
elsewhere and part of the parity claim: `tokenizers 0.23.1` (`R/Cargo.toml:256`, the pin
Laya's Python side runs), `pmcp 2.19` (locked 2.19.3), `lambda_http 0.13`,
`aws-sdk-s3` locked 1.137.0.

New, in `R/` (aprender side): **`aprender-mcp-builtin`**, the core crate of the type, the
analogue of `mcp-sql-server-core`. It depends on the three library crates above plus
`toml`, `serde`, `aws-sdk-s3`, `pmcp`, and exposes the API in §6.2. It does not depend on
`mcp-lambda-proxy`, so it builds and tests inside the fork with the fork's contracts and
fixtures; only the template calls the proxy.

How the builder obtains it, my call, with the alternative stated: a **git dependency
pinned by `rev`** on the public fork. Reasons: the crates are not published and the
`aprender-*` names on crates.io belong to upstream; the platform already caches
`/root/.cargo/git/**/*` (`build-project.ts:1308-1314`) and the GraphRAG image already
pre-compiles its dependencies into image layers (`Dockerfile.graph-rag:70-72,129-146`),
so a `Dockerfile.aprender` that runs one seed build at image-build time pays the clone
(the fork's `.git` is 567 MB) and the 485-crate compile once per image, not per server.
The alternative the platform uses for its one private crate is vendoring the sources
under the type workspace (edgequake, `templates-graph-rag/Cargo.toml.tmpl:30-35`);
it works, costs a copy step per aprender revision, and is the fallback if the dev team
prefers no network at build time. Either way the **lock is part of the pin**: seed
`P/built-in/aprender-api/Cargo.lock` from `R/Cargo.lock` at the same revision so the
builder resolves the same `tokenizers`, `aws-sdk-s3` and `pmcp` the parity numbers were
measured with; the generated workspace refuses to resolve without a lock
(`build-project.ts:1043-1046`), which is the right behaviour.

Toolchain: the builder asserts rustc 1.97.1 (`Dockerfile.toolchain:84-107`); the fork
pins 1.93.0 and its ceiling gate keeps the tree clean on current stable (`R/CLAUDE.md`,
Linting). The fork must build warning-free on 1.97.1 before the revision is pinned; the
`verify` tool then proves the numerics on the deployed binary.

### 6.2 The core crate's public API (the contract with the template)

```rust
//! aprender-mcp-builtin — the pmcp.run `aprender` built-in server type, model side.
//! The template is the only caller. Everything it needs is here; nothing it does not
//! need is public.

pub mod config {
    /// The parsed, validated `config.toml` (§3). `deny_unknown_fields` throughout.
    #[derive(Debug, Clone, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Config {
        pub id: ServerId,                   // the server id (§3.2); must equal the template's SERVER_ID
        pub schema: Schema,                 // exactly "aprender-server/v1"
        pub environment: Environment,       // Staging | Prod
        pub model: ModelSection,
        pub records: RecordsSection,
        pub publish: PublishSection,
        #[serde(default)] pub hold: Option<HoldSection>,
        #[serde(default)] pub tools: ToolsSection,
        #[serde(default)] pub verify: VerifySection,
    }

    /// Parse and validate. Every rule in §3.2 is a named error that names the key
    /// (`config-invalid: [model].artifact_sha256 must be 64 lowercase hex`; a
    /// mis-keyed field carries serde's own `unknown field` message); no defaults for
    /// required fields. `id` is `^[a-z][a-z0-9-]{0,62}$` (lowercase: the platform
    /// slugifies ids). `expected_id` is the template's substituted SERVER_ID; a
    /// configuration whose `id` differs is `config-invalid` (the two are derived from
    /// the same line by the platform, so a difference means a hand edit).
    pub fn parse(toml_text: &str, expected_id: &str) -> Result<Config, ConfigError>;

    pub enum PublishKind { Promote, Rollback }   // `[publish].kind`, default Promote

    /// sha256 of the exact TOML bytes, reported by `status` and the precheck.
    pub fn config_sha256(toml_text: &str) -> String;
}

pub mod server {
    /// Build the pmcp server with `classify`, `status` and `verify`. Nothing is loaded
    /// here; the model loads on the first `classify` or `verify` call.
    pub fn build(config: config::Config, server_name: &str, version: &str,
                 build_id: Option<String>) -> pmcp::Result<pmcp::Server>;
}

pub mod precheck {
    /// §5.3. All optional; read by `main_with` from `APRENDER_PREVIOUS_ARTIFACT_SHA256`,
    /// `APRENDER_PUBLISHED_SHA256_HISTORY` (comma-joined, oldest first) and
    /// `APRENDER_BUILD_ID`.
    pub struct Env {
        pub previous_artifact_sha256: Option<String>,
        pub published_sha256_history: Option<Vec<String>>,
        pub build_id: Option<String>,
    }

    /// §5. Prints the WARN lines and the one verdict line (`ERROR: REFUSED <code>:
    /// <detail>` or `ELIGIBLE {json}`) to stdout and returns the exit status the
    /// binary must use: 0 eligible, 2 refused, 3 config-invalid, 4 s3-unreachable.
    pub async fn run(toml_text: &str, expected_id: &str, env: Env) -> i32;
}

/// The one binary entry point the template calls. `args` are the process arguments:
/// `["bootstrap"]` and `["bootstrap", "--serve-local"]` build the server and hand it
/// to `serve` (the template chooses the closure: the platform proxy, or a localhost
/// listener); `["bootstrap", "precheck"]` runs the check and exits with its status;
/// anything else is a usage error, exit 64. `expected_id` is the template's SERVER_ID.
pub async fn main_with(
    args: Vec<String>,
    toml_text: &'static str,
    expected_id: &str,
    serve: impl FnOnce(pmcp::Server)
        -> Pin<Box<dyn Future<Output = Result<(), lambda_http::Error>> + Send + 'static>>,
) -> Result<(), lambda_http::Error>;
```

Four parameters (the platform's template passes four; an earlier revision of this
section showed three and §6.3 four). `main_with` takes the serving function from the
template so the core crate never depends on `mcp-lambda-proxy`; the template passes
`mcp_lambda_proxy::run_lambda` for Lambda and its own loopback listener for
`--serve-local`. The crate re-exports `pmcp` and `lambda_http` at the versions it was
built with, so the template's closure type and the crate's agree on one `pmcp::Server`.

The platform's spike sources are the direct templates for two of the three modules and
this side ports them rather than re-deriving: the offline precheck prototype
(`P/.claude/skills/spike-findings-pmcp-aprender-builtin-type/sources/036-precheck-rules-offline/precheck/`,
`config.rs`/`store.rs`/`precheck.rs`/`cases.rs`, 45 cases over a fake object store,
the whole §5.2 table) and the 033 harness `main.rs` (the lazy `classify` and `status`
handlers measured live, `sources/033-closure-on-platform-toolchain/harness/`). The
production `ObjectStore` is an `aws-sdk-s3` implementor of the prototype's trait
(`HeadObject` for `head`, a ranged `GetObject` for `get` that never reads past the cap);
the 45 cases become the crate's tests. `verify` and its door are the only parts without
a spike.

### 6.3 The template (`P/built-in/builder/templates-aprender/`)

Four files plus the CDK app, following `templates-sql/` exactly
(`Cargo.toml.tmpl`, `lambda-Cargo.toml.tmpl`, `main.rs`, `deploy/`). Placeholders are the
same `${SERVER_ID}` / `${SERVER_NAME}` the builder substitutes
(`build-project.ts:1192`).

`Cargo.toml.tmpl`:

```toml
# Auto-generated Cargo.toml for ${SERVER_ID}
# Generated by PMCP Builder — do not edit manually.

# The default release profile, deliberately: today's cold-start and parity numbers were
# measured on default-release binaries (`cargo zigbuild --release`, R/justfile:2395; the
# fork has no [profile.release]). Not the other types' `opt-level = "z"` (the encoder's
# GEMM is the cold-start floor) and no `lto`/`codegen-units = 1`: fat LTO is slow,
# uncacheable and would run twice per build (§7). Tune only against the §9 bars.
[profile.release]
opt-level = 3
strip = true

[workspace]
members = ["${SERVER_ID}-lambda"]
resolver = "2"

[workspace.dependencies]
# The type's core crate, model side. Pinned by revision; the lock is seeded from the
# fork at the same revision (§6.1). Bump both together.
aprender-mcp-builtin = { git = "https://github.com/guyernest/aprender.git", rev = "${APRENDER_REV}" }

# Shared platform runtime (identity headers, loopback proxy).
mcp-lambda-proxy = { path = "../../../shared/mcp-lambda-proxy" }

pmcp = { version = "2.19", features = ["streamable-http", "schema-generation"] }
lambda_http = "0.13"     # the proxy's pin; aprender-mcp-decide-lambda uses the same
tokio = { version = "1", features = ["full"] }
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

[workspace.package]
authors = ["pmcp.run"]
edition = "2024"
license = "MIT OR Apache-2.0"
version = "0.1.0"
```

`APRENDER_REV` is a builder-image constant (the pinned revision), not a per-server
input; a server never chooses its code version.

`lambda-Cargo.toml.tmpl`:

```toml
[package]
name = "${SERVER_ID}-lambda"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
description = "${SERVER_NAME} - aprender decider"

[[bin]]
name = "bootstrap"
path = "src/main.rs"

[dependencies]
aprender-mcp-builtin.workspace = true
mcp-lambda-proxy.workspace = true
lambda_http.workspace = true
tokio.workspace = true
tracing-subscriber.workspace = true
```

`main.rs` (the platform's file, `P/built-in/builder/templates-aprender/main.rs`, is
canonical; this is its shape):

```rust
//! ${SERVER_NAME} - aprender decider (pmcp.run built-in type `aprender`).
//! Configuration is embedded at compile time from config.toml. Generated by PMCP Builder.

const CONFIG_TOML: &str = include_str!("../config.toml");
const SERVER_ID: &str = "${SERVER_ID}";
const LOCAL_ADDR: &str = "127.0.0.1:8081";

/// `bootstrap --serve-local`: the same server over plain streamable HTTP on localhost.
async fn serve_local(server: pmcp::Server) -> Result<(), lambda_http::Error> { /* StreamableHttpServer::with_config(LOCAL_ADDR, stateless) */ }

#[tokio::main]
async fn main() -> Result<(), lambda_http::Error> {
    tracing_subscriber::fmt().json().init();
    let args: Vec<String> = std::env::args().collect();
    let local = args.get(1).map(String::as_str) == Some("--serve-local");
    aprender_mcp_builtin::main_with(args, CONFIG_TOML, SERVER_ID, move |server| {
        if local { Box::pin(serve_local(server)) }
        else { Box::pin(mcp_lambda_proxy::run_lambda(server, SERVER_ID)) }
    })
    .await
}
```

`deploy/bin/app.ts`, the part that differs from the SQL template (`templates-sql/deploy/bin/app.ts:136-147` passes `additionalPolicies`):

```ts
// Derive IAM from config.toml: exact objects for reads, one prefix for writes (§8).
import { parse } from 'smol-toml';            // or the parser graph-rag's stack.ts uses
import * as iam from 'aws-cdk-lib/aws-iam';

const cfg = parse(readFileSync('../config.toml', 'utf8')) as any;
const arnOf = (uri: string) => {
  const m = /^s3:\/\/([^/]+)\/(.+)$/.exec(uri);
  if (!m) throw new Error(`not an s3 uri: ${uri}`);
  return `arn:aws:s3:::${m[1]}/${m[2]}`;
};
const readUris = [cfg.model.artifact_uri, cfg.records.eval_jsonl_uri, cfg.records.eval_probs_uri,
                  cfg.records.rescore_noise_uri].filter(Boolean);
const additionalPolicies = [
  new iam.PolicyStatement({ actions: ['s3:GetObject'], resources: readUris.map(arnOf) }),
];
if (cfg.records.verification_prefix) {
  additionalPolicies.push(new iam.PolicyStatement({
    actions: ['s3:PutObject'], resources: [arnOf(cfg.records.verification_prefix) + '*'],
  }));
}
new McpServerConstruct(stack, 'Server', {
  serverId, memorySize: 10240, timeout: Duration.seconds(30),
  compositionEnabled: false,                 // this server calls nothing
  additionalPolicies,
});
```

The `memorySize`/`timeout` literals are belt and braces: cargo-pmcp overrides them
from `.pmcp/deploy.toml` after synth (`cargo-pmcp/src/deployment/targets/pmcp_run/deploy.rs:966-980`),
and the buildspec writes 10,240 / 30 there (§7).

### 6.4 aprender-side changes the core crate needs (this side's work)

- `aprender-mcp-decide`: make the tool description builder public, or give the new
  crate a `classify_tool(service, description)` constructor, so the boundary logic stays
  in one place (OPS-03); `ClassifyService`, `parse_args`, `check_served_task_fits` are
  public already (`lib.rs:579, 727, 751`). `check_served_task_fits` moves from build
  time to load time for this type (the model is not known at build time); its refusal
  becomes a `config_error` load failure.
- `aprender-mcp-decide-lambda`: nothing in the library; the `bootstrap` module is
  simply not used by the type. The S3 fetcher gains a small `get_object_bounded(uri,
  cap)` for the records and a `put_object_if_none_match` for the verification record,
  or those live in the new crate.
- `aprender-decide::verify`: expose the `eval-probs.json` reader and the row matching.
- Fork hygiene the platform asked for, in the same change: `aprender-core/build.rs` and
  `aprender-compute/build.rs` print `rerun-if-changed` on a `provable-contracts/…/binding.yaml`
  that does not exist outside the sibling checkout, and cargo treats a missing path as
  always changed, so a *path* dependency on the fork recompiles everything downstream
  on every build (measured: 56 s no-op rebuild against 0.27 s for the git dependency).
  Gate the line on the file's existence. And the two `unused variable: backend`
  warnings in `aprender-compute/src/brick/quant_ops/mod.rs:219,318`, the only code
  warnings rustc 1.97.1 reports on the decide closure (re-measured here 2026-11-03,
  `cargo +1.97.1 check` of the three decide crates: 2 code warnings, both those, plus
  the 2 build-script notes above; cargo hides git-dependency warnings from the
  template build either way, so this is hygiene, not a blocker).
- Contracts: a new `contracts/aprender-builtin-v1.yaml` holding the configuration
  grammar, the refusal names in §4.5 and §5.2, the record schemas (`aprender-status/v1`,
  `aprender-verification/v1`, the `laya-compare/v1` shape) and their case tables,
  validated with `pv` like the rest.
- The new crate's tests: a tiny-model fixture already exists
  (`R/crates/aprender-decide/tests/fixtures/laya_tiny/`), with a `gate-report.json` and
  a pack-outcome fixture (`R/crates/aprender-mcp-laya-train/fixtures/pack-outcome-v1.json`),
  so the precheck rules and the three tools are testable offline against a fake S3.

## 7. Platform touchpoints (the dev team's list)

In the order a build flows, with the existing file for each pattern:

1. **Type id.** `serverType` enum gains `aprender` (`P/amplify/data/resource.ts:980`);
   the trigger-build allowlist too (`amplify/functions/trigger-build/handler.ts:177-178`).
   The `-api`-suffixed `BuiltInServerType.typeId` space (`resource.ts:919-923`) gets
   `aprender-api` if that registry is used for it.
2. **Trigger-build.** Upload the TOML as today (`handler.ts:165-172`). The previous
   published sha and the history come from the buildspec's own read of the
   published-sha marker (§5.2, §5.3), not from the execution input (built, 209-03).
3. **State machine.** A `ChooseBuilderProject` branch for `aprender` and a
   `ReadAprenderBuilderProjectName` state reading `/pmcp/builder/aprender/codebuild-project-name`
   (`trigger-build-workflow/resource.ts:104-203`); the role's project-name pattern
   (`amplify/modules/deployment.ts:1742-1752`).
4. **Builder image.** `Dockerfile.aprender` extending `pmcp-toolchain`: `git clone` of
   the fork at `APRENDER_REV`, the seeded lock, one seed server build so the dependency
   compile lives in the image layers (the GraphRAG pattern,
   `Dockerfile.graph-rag:70-72,129-146`). Justfile `build/create-repo/push/start-build`
   entries (`built-in/builder/justfile:70-75,157-162,182-187,236-240`); the toolchain
   header's type count (`Dockerfile.toolchain:3,20-26`).
5. **CodeBuild project.** `createAprenderBuildSpec` in `build-project.ts`, project plus
   the three SSM parameters (`:600-612` is the GraphQL example). Compute `LARGE` (8 vCPU,
   16 GB ARM), timeout to be set from the first measured build (§9). Two platform facts
   the reviewer established shape the cache strategy: every buildspec runs
   `cargo lambda build` after `unset CARGO_TARGET_DIR` into the workspace-local
   `target/` (`build-project.ts:1253`, GraphRAG `:2220`), so an image-warmed target
   directory is not what a server build links against and sccache over S3
   (`:1053-1066`, region hardcoded `us-east-1`) is the real cross-build cache; and
   `cargo pmcp deploy` rebuilds natively in `post_build` (workbook `:1853-1856`), so the
   shipped binary is a second build of the same source and configuration, not the one
   `precheck` ran (the check reads records, not the binary, so its verdict holds).
   Dev team's call which mechanism carries the 485-crate closure: sccache, an
   image-layer precompile wired to the workspace target as GraphRAG does, or both; §9
   measures the result. The release profile is the platform's to tune (§6.3): the
   parity claim does not depend on it, the §9 cold bar and `verify` are the re-proof.
   Buildspec steps:
   - `pre_build`: as today (`SERVER_ID` grammar, fetch config).
   - `build`: read and validate the published-sha marker, export the three variables
     (§5.3); `cargo lambda build --release --arm64 -p "${SERVER_ID}-lambda" --bin bootstrap`;
     then the bare `"$WORKSPACE_DIR/target/lambda/bootstrap/bootstrap" precheck`; a
     non-zero exit fails the build and the binary's own `ERROR: REFUSED …` line is what
     `ExtractBuildError` surfaces (§5.2; built, 209-03).
   - `post_build`: guarded by `CODEBUILD_BUILD_SUCCEEDING` (`:2279-2285`); write
     `.pmcp/deploy.toml` with `[server] memory_mb = 10240`, `timeout_seconds = 30`,
     `[environment] RUST_LOG = "info"`, `PMCP_BUILD_ID = "$CODEBUILD_BUILD_ID"`,
     `[observability] enable_xray = true`, `create_dashboard = true`,
     `log_retention_days = 30`, the error/latency alarms today's servers carry
     (`R/crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template:97-106`:
     errors 10, latency 20,000 ms), `[auth] enabled = false` as the other types; then
     `cargo pmcp deploy --target pmcp-run --no-oauth`.
6. **Builder role.** `GetObject` and `ListBucket` on the artifact and record bucket
   families (§5.4), plus read/write of its own marker keys in the builder artifact
   bucket (§5.2).
7. **Admin UI.** A selector option (`components/built-in-servers/wizard/server-type-selector.tsx:16-47`),
   an edit-page registry entry with `EditorKind = 'toml'`
   (`components/built-in-servers/lib/server-type-registry.ts:45-55`), and a starter TOML
   (the staging example in §3.1 with blanks). The wizard forms are hand-written per
   type (`wizard-page.tsx:1034-1500`); my call is that v1 ships the TOML editor with the
   starter and no form, because every field is a URI or a sha the analyst copies from
   the trainer's report, and a form can follow once the trainer's "datasets from parts"
   project fixes where the records live.
8. **Version history.** Nothing new in the platform: restore re-saves as a new
   `ConfigVersion` and rebuilds (`hooks/use-server-config.ts:724-734`). A rollback is a
   restore plus an edit of `[publish]` to `kind = "rollback"` with the current sha as
   baseline (§5.2); the starter text for the editor can say so.
9. **`uploadDeployment` constants.** cargo-pmcp sends `memorySize: 512, timeout: 30`
   in its GraphQL input regardless of the manifest
   (`cargo-pmcp/src/deployment/targets/pmcp_run/graphql.rs:463-464`). Settled by the
   dev team's spike 037: the only reader of that input is the descriptor-synthesis
   path, which a CLI-synthesized template never enters; the handler range-checks
   `MemorySize` and the CloudFormation values win (live readback 10,240 MB / 30 s).

## 8. IAM derived from the configuration

| Principal | Action | Resource | When |
|---|---|---|---|
| the server's function role | `s3:GetObject` | the exact artifact object | always |
| the server's function role | `s3:GetObject` | the exact `eval_jsonl_uri`, `eval_probs_uri`, `rescore_noise_uri` objects | always (for `verify`) |
| the server's function role | `s3:PutObject` | `verification_prefix*` | when `verification_prefix` is set |
| the server's function role | nothing else | | `compositionEnabled: false`; no DynamoDB, Secrets Manager, SSM, Lambda invoke |
| the CodeBuild role | `s3:GetObject`, `s3:ListBucket` | the artifact and record bucket families | the precheck (§5.4) |
| the CodeBuild role | `s3:GetObject`, `s3:PutObject`, `s3:DeleteObject` | `published/<server>/*` in the builder artifact bucket | the published-sha marker, lock and pending record (§5.2) |

The weights bucket today is SSE-S3 (`just laya-weights-bucket`, `R/justfile:4110`), so
no KMS grant; if a bucket is SSE-KMS, the function role needs `kms:Decrypt` on that key,
derived the same way. Today's hand-written grant is one `s3:GetObject` on
`decide/<server>/*` and a check refuses anything broader (`R/justfile:2261-2267`); the
derived policy above is narrower (exact objects) and the `_laya-iam-check` recipe can
be retired with the per-model crates.

## 9. First proof (the spike the umbrella asked for)

Done on `ze-kasher-dev` (`us-east-1`), the account that hosts the dev platform instance
and the trainer bucket, with the model already verified on the dev endpoint (sha
`22796ff9…9eed`, the Phase 16 SageMaker control; `laya-deploy-verify` passed there
2026-11-02, cold 18.6–27.3 s, served probabilities within 2e-6 of the stored ones).

1. Dev team: image, project, state-machine branch, UI entry (§7), with `APRENDER_REV`
   set to the revision this side hands over once the core crate exists.
2. This side: hand over the revision, a staging `config.toml` for a server named
   `antisemitism-staging` pointing at that artifact and its run records, and the
   deploy-verify recipe pointed at the new URL.
3. Measure and record, as a `BuildRecord` plus a JSON evidence file next to today's:
   - CodeBuild wall time of the first and of a second build of the same server (the
     second is the number that matters; the first includes any cache miss).
   - Four proven-cold calls of the maximal request (the in-band tell of §4.7 plus the
     `performed_load=true` log line; the recipe `R/justfile:4420-4610` adapts from the
     probe-id header to the tell), against the 30 s cap; the bar is today's worst,
     27.3 s, with slack ≥ 2.7 s. The dev team's spike 034 already measured this path
     with a harness built on the decide crates behind the proxy at 10,240 MB:
     client-visible cold 16.8–21.3 s (load 12.7–14.6 s, of which download ~8.15 s),
     slack 8.7–13.2 s, peak 2,485 MB; the core crate must hold that, not improve it.
   - `status` before and after the first load; `verify` with `rows = 8`, `seed = 0`,
     writing a record, called once by the owner's admin user from a chat and once by
     the deploy-verify recipe's allow-listed M2M client; the bar is
     `max_abs_diff ≤ 5.0e-5` (expected ~2e-6).
   - A `verify` call by a user token without the `admin` group and not allow-listed,
     refused with the permission wording.
   - A publish naming a sha with no pack record, refused by the precheck with
     `pack-record-missing` and the build marked failed with that line.
   - The prod rule set, on the dev platform instance: a second server published with
     `environment = "prod"`, `first_model`, copied records; then a promote with a
     comparison record (a hand-written `laya-compare/v1` fixture until the trainer
     writes one) and the staging server's verification record; then a rollback with
     `kind = "rollback"`; then the three refusals `baseline-mismatch`,
     `comparison-missing`, `staging-verification-missing`. This proves the prod rules
     before any second environment exists.
   - Build time of the second build with the chosen cache mechanism, against the
     project timeout.
4. Only then: `motivation-v1` on a second staging server (its own records), and the
   migration plan in §10. The prod platform environment (`augury-dev`, `eu-west-1`: a
   second builder image and project, grants on that account's buckets) is part of §10,
   not of this proof (my call, from the standing rule that prod changes only after the
   running experiment ends and only on a clear improvement).

If the cold call misses the bar, the levers in order are: confirm the profile is the
measured one (`opt-level = 3`, no size optimization); try `lto = "thin"`, then `"fat"`,
each re-measured against both the cold bar and the build timeout, since a profile change
is a build-time cost paid twice per build (§7, touchpoint 5); and if neither reaches it, the load
itself is the floor (download 9.2 s, sha 3.5 s, build 2.8 s) and the question goes back
to the owner. SnapStart does not exist for `provided.al2023`, so there is no platform
lever; what today's servers do is keep the number known: the deploy-verify recipe forces
a cold environment on every verify.

## 10. Migration of the two live deciders (later, decided)

The per-model crates keep running until the type is proven on a new server (§9). Then,
per decider: a staging built-in server in `ze-kasher-dev` on the current prod sha
(`antisemitism-v1` `2293bebb…`, `motivation-v1` `d202f495…`), `verify` passes, a prod
built-in server in `augury-dev` on the same sha with `baseline_sha256 = "first_model"`
(the first publish of that server) and the copied records, the chat connectors
re-pointed, then `just laya-teardown` of the crate-deployed servers. The prod swap of
the antisemitism model itself stays under the standing rule: not during the running
experiment, and only on a clear improvement, which is what the prod precheck shows.
The `laya-deploy`, `laya-upload`, `laya-grant` and `_laya-iam-check` recipes retire with
the crates; `laya-pack`, `laya-verify`, `laya-fetch` and `laya-deploy-verify` stay.

## 11. Known limits and open questions

- **Promotion from the chat.** A publish is a `ConfigVersion` save plus the admin
  `triggerBuild` mutation (`handler.ts:124-178`). The dev team now owns the GraphQL and
  Admin UI side; whether a chat can drive it is their call, and this type does not
  depend on it: v1 promotion is a UI or `cargo pmcp` publish, as the umbrella says.
- **A hold is a rule, not a lock** (umbrella §5.6); and the hold and baseline rules
  depend on the platform passing the previous sha (§5.3); without it they are skipped,
  loudly, while `rollback-first-publish` fires. `rollback-unknown-sha` is skipped only
  when no history is passed; the platform passes it.
- **`verify` callers are the `admin` group of the server's own pool or allow-listed
  machine clients** in v1 (§4.6). The dedicated pool is an operator-enforced condition:
  the Admin UI also offers shared and existing pools, and nothing stops an operator
  choosing one; the platform's live check (plan 209-09, R4) is what proves the pool is
  new and not the management pool.
- **One platform environment in v1.** The prod rules are proven on the dev instance
  (§9); the `augury-dev` rollout is the first step of the migration (§10).
- **The comparison record does not exist yet** for any model; prod publishes use
  `first_model` until the trainer project writes it (§5.2).
- **Prod reads copies.** The operator copies the artifact and the five records into the
  prod account by hand (`aws s3 cp --sse AES256`, as `laya-upload` does today,
  `R/justfile:4160-4245`) until the trainer project or the platform owns the copy.
- **Build time is unmeasured** in CodeBuild for a crate with this closure; the image
  seed build is the mitigation and §9 measures it.
- **The 10,240 MB tier** passes the platform validator and the dev team's spike 034
  harness ran on it behind `mcp-lambda-proxy` (§9); the core crate's own number is
  still owed by §9.
- **One identity source.** The SDK's `AuthContext`, filled from the proxy's headers
  (§4.6); a configurable claim key can be added later without changing the schema
  version (it is additive).
- **Other kinds.** `kind = "setfit"` is the next candidate: a SetFit `.apr` is detected
  by `model_type = "setfit"` and loaded by `aprender::setfit::load_setfit_apr`
  (`R/crates/aprender-mcp-setfit/src/lib.rs:38-48, 126-143`; `R/contracts/setfit-apr-v1.yaml:122-131`),
  its artifact cap is 1 GiB and its door is `contracts/setfit-tool-boundary` rather than
  decide's. Adding it is a second loader behind `kind` and a second bounds table in
  `status`, with memory and timeout per kind in the buildspec (a 22M-parameter encoder
  does not need the large tier). Not in v1.

## 12. Left to the builders

- Dev team (what their phase 209 left open, in their own words): the CodeBuild timeout
  after the first measured build; whether the starter TOML becomes a form later; the
  decommission path for a server's marker keys; the KMS gap (§5.4). Settled by them
  and recorded above: the marker as the predecessor source, `ListBucket`, the dedicated
  pool, the binary path, the anchor grammar, touchpoint 9.
- This side: the seeded-shuffle definition for `verify` rows; the exact error type the
  load failure carries through pmcp (`tool_rejected` versus `internal`); the contract
  file's case tables; the deploy-verify recipe's move from the probe-id header to the
  in-band tell (§4.7).

## Appendix: facts this design rests on

- The thin per-model crates are shims: each `main.rs` calls
  `aprender_mcp_decide_lambda::bootstrap::run(env!("CARGO_PKG_NAME"))`; a test refuses a
  thin crate that grows a second dependency (`R/crates/aprender-mcp-decide-lambda/src/tests.rs:623`).
  The model is chosen by `APRENDER_DECIDE_S3_URI` and `APRENDER_DECIDE_SHA256`
  (`lib.rs:38-43, 283-300`) in a generated deploy TOML; the sha pin is mandatory for S3
  sources (`lib.rs:262-279`).
- Weights are never bundled: "/tmp is 512 MB and cargo-pmcp cannot raise it"
  (`deploy.toml.template:45-47`); no ephemeral-storage field exists in cargo-pmcp's
  `ServerConfig` (`cargo-pmcp/src/deployment/config.rs:629-696`) or in the platform's
  synthesis (`synthesize.ts`, `mcp-server-construct.ts`).
- The `.apr` is an APR v2 container with `model_type = "decide"` and one custom key
  `decide` holding the manifest (schema, method, base, labels, agent, blobs, recipe_id,
  calibration, gate, inputs_sha256, probes); tokenizer and task JSON are blobs inside
  (`R/contracts/decide-apr-v1.yaml:138-155, 310-317`). Identity: `artifact_sha256` of the
  whole file, `recipe_id` of the recipe blob (`:325-329`).
- Deploy eligibility today is the pack task's `pack-outcome.json` bound to the sha:
  `schema laya-train-pack-outcome-v1`, `accepted`, `deploy_eligible`, `apr_sha256`
  (`R/scripts/laya_deploy/laya_eligibility.py:72-106`); the run layout is
  `runs/<name>/<task_id>/{<name>.apr, run/, data/, pack-outcome.json, manifest.json, …}`
  (`R/crates/aprender-mcp-laya-train/src/layout.rs:66-102`).
- pmcp.run built-in types: templates are thin shells over a per-type core crate
  (`P/built-in/builder/templates-sql/main_athena.rs`, 39 lines); shared
  `mcp-lambda-proxy` 0.1.0 on `pmcp 2.19.0` and `lambda_http 0.13`
  (`P/built-in/shared/mcp-lambda-proxy/Cargo.toml:11,14`); no per-type config
  validator exists, parsing happens at Lambda start; memory and timeout come from the
  buildspec's `.pmcp/deploy.toml` and cargo-pmcp merges them into the function after
  synth; the only private crate is vendored (edgequake); CodeBuild caches
  `/root/.cargo/{registry,git}` and the target dir in S3; `ConfigVersion` restore is a
  re-save plus rebuild; `triggerBuild` is admin-only; no document for adding a type
  exists (`P/built-in/README.md:235-244` points at a missing `TEMPLATE_GUIDE.md`).
- GraphRAG's per-tool role guard reads `extra.metadata["x-pmcp-user-groups"]`, denies on
  no groups, allows `admin` or the required role
  (`P/built-in/graph-rag/crates/mcp-rag-server-core/src/pmcp_server.rs:441-489`).
- Facts from the builder check of 2026-11-03 (a reviewer reading as the platform
  team): `SERVER_ID` is derived from the first top-level `id = "..."` line of the
  configuration (`P/amplify/shared/server-config-link.ts:29-37`); groups are stamped
  only from token claims and client-credentials tokens carry none
  (`P/amplify/functions/mcp-proxy-rust/src/main.rs:2472-2485, 2717-2724`;
  `P/amplify/auth/resource.ts:14`); buildspecs `unset CARGO_TARGET_DIR` and
  `cargo pmcp deploy` rebuilds natively in `post_build`; the template validator admits
  `MemorySize` up to 10,240 and nothing in the deployment workflow reads the GraphQL
  `memorySize` input (`P/amplify/functions/upload-deployment/handler.ts:1797, 1006`);
  IAM statements from the CDK app reach the function in template mode (`:1831-1857`);
  `ExtractBuildError` anchors on `ERROR:`; the fork is public, has no `[patch]`
  sections, and `aprender-core`'s default features are just `parallel`; cold-start
  evidence spans Graviton 2 and 3 and `aprender-compute` has no runtime CPU dispatch.
