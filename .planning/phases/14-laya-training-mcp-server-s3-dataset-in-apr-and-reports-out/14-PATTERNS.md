# Phase 14: Laya training MCP server - Pattern Map

**Mapped:** 2026-10-28
**Files analyzed:** 22 new or modified
**Analogs found:** 19 / 22 (3 have no in-repo analog: Step Functions ASL, x86 CUDA container, CodeBuild)

All analog paths were checked against `git ls-files` (tracked). No gitignored mirror is named. `crates/.pmcp/deploy.toml` is gitignored; its tracked origin is `crates/.pmcp/deploy.toml.template`.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `crates/aprender-mcp-laya-train/src/lib.rs` (args inline in `lib.rs`; status payload structs in `layout.rs`; + `dispatcher.rs`) — revised per plan 14-01 | service (MCP tool surface) | request-response + event-driven (Tasks) | `crates/aprender-mcp-setfit-train/src/lib.rs` | role-match (SetFit-shaped, rewrite ~300 lines) |
| `crates/aprender-mcp-laya-train/src/validate.rs` | utility (validation port) | transform | `scripts/laya_train/data.py` + `aprender_decide::task`/`verify::normalize_text` | exact (rule source) |
| `crates/aprender-mcp-laya-train/tests/validation_cases.rs` | test | transform | `scripts/laya_train/data.py::selftest` (line 593) | role-match |
| `scripts/laya_train/data_cases.json` | test fixture (shared table) | transform | `scripts/laya_train/numeric_cases.json` | exact (shared-table convention) |
| `crates/aprender-mcp-laya-train/Cargo.toml` | config (lib, NO bin) | - | `crates/aprender-mcp-setfit-train/Cargo.toml` (minus main.rs) | role-match |
| `crates/aprender-mcp-laya-train-lambda/Cargo.toml` | config | - | `crates/aprender-setfit-train-lambda/Cargo.toml` | exact |
| `crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs` | controller (Lambda loopback proxy) | request-response | `crates/aprender-setfit-train-lambda/src/bin/bootstrap.rs` | exact |
| `crates/aprender-mcp-laya-train-lambda/src/bin/aprender-laya-train-finalizer.rs` | service (finalizer) | event-driven | `crates/aprender-setfit-train-lambda/src/bin/aprender-setfit-trainer.rs` | role-match |
| `.../src/{sfn.rs,presign.rs}` | service (dispatcher) | event-driven | `crates/aprender-setfit-train-lambda/src/dispatch.rs` | role-match |
| `.../src/dataset.rs` | utility (URI/prefix verify) | request-response | `crates/aprender-setfit-train-lambda/src/dataset.rs` | exact |
| `.../src/lock.rs` (single-flight) | service | CRUD (CAS) | `TaskBackend` seam in `task_store.rs:124-150` | partial (no in-repo single-flight on Lambda) |
| `deploy-extensions/lib/laya-train-stack.ts` | config (CDK) | - | `deploy-extensions/lib/setfit-training-stack.ts` | role-match |
| `deploy-extensions/bin/app.ts` (modify) | config | - | itself, lines 29-39 | exact |
| `scripts/laya_train/container/{Dockerfile.train,entrypoint.py}` | config + utility | file-I/O, batch | none (closest: `scripts/laya_train/train.py` CLI and exit codes) | no analog |
| `deploy-extensions/container/pack-arm64/Dockerfile` | config | batch | none (`pack_laya.rs` is the program) | no analog |
| Step Functions ASL (in the CDK stack) | orchestration | event-driven | none | no analog (comment at `dispatch.rs:4-14` states the trigger) |
| `justfile` recipes (`synth-laya-train`, `laya-fetch`, `laya-fetch-selftest`, `laya-dataset-put`, deploy/grant) | config (recipes) | file-I/O | `synth-training` (129), `pmcp-train-config` (173), `laya-deploy` (2177), `_laya-auth-refusal-check` (4339) | role-match |
| `justfile` `laya-gates-selftest` row `row_dup_bin_names` (modify, ~5584) | test | - | itself | exact |
| `scripts/duplicate_bin_names_allowlist.txt` (modify) | config | - | its own `bootstrap` line | exact |
| `crates/aprender-core/tests/monorepo_invariants.rs` (modify) | test | - | its own 9->10 growth | exact |
| `ci/explicit-test-commands.d/535-*.cmd` (only if a new `tests/*.rs` is added) | config | - | `530-aprender-serve-llm-endpoint-conformance.cmd` | exact |
| Workspace `Cargo.toml` members, `crates/.pmcp/deploy.toml.template` | config | - | existing entries | exact |

## Pattern Assignments

### `crates/aprender-mcp-laya-train-lambda/Cargo.toml` (config)

**Analog:** `crates/aprender-setfit-train-lambda/Cargo.toml` (lines 1-76). Copy:
- Workspace inheritance block (lines 2-8), `publish = false` (line 10).
- The `bootstrap` bin must be the request function; the finalizer is a second `[[bin]]` with a descriptive name that the CDK renames:
```toml
[[bin]]
name = "bootstrap"
path = "src/bin/bootstrap.rs"
[[bin]]
name = "aprender-laya-train-finalizer"
path = "src/bin/aprender-laya-train-finalizer.rs"
```
- Deps pins to reuse verbatim: `pmcp = { version = "2.19", features = ["streamable-http", "schema-generation"] }`, `aws-config/aws-sdk-dynamodb/aws-sdk-s3 = "1"`, `lambda_http = "0.13"`, `lambda_runtime = "0.13"`, reqwest rustls, `once_cell`, tracing. Drop `tar`, `flate2`, `aws-sdk-lambda`. Add `aws-sdk-sfn = "1"` (research A2: verify with `cargo check` on rustc 1.93).
- Path dep `aprender-mcp-setfit-train = { path = "../aprender-mcp-setfit-train" }` for `AprenderTaskStore`, `TaskBackend`, `StoredTask`, `mint_id`, `CancelSink`; path dep `aprender-setfit-train-lambda` for `DynamoDbTaskBackend::new`.
- Crate dir name MUST be `<server-id>-lambda` (cargo-pmcp lookup). Server id `aprender-mcp-laya-train` gives `aprender-mcp-laya-train-lambda`.
- `[lints] workspace = true`.

**Thin-crate comparison:** `crates/aprender-mcp-decide-motivation-lambda/Cargo.toml` lines 10-26 show the publish=false + FALSIFY-MONO-011 comment style to copy for the rationale text.

---

### `crates/aprender-mcp-laya-train-lambda/src/bin/bootstrap.rs` (controller, request-response)

**Analog:** `crates/aprender-setfit-train-lambda/src/bin/bootstrap.rs` (215 lines). Copy whole file; change only the `build()` wiring and the health message.

**Allow for json! (line 17-18):** `#![allow(clippy::disallowed_methods)]`

**Stateless config narrowed to the tool's body bound (lines 50-55):**
```rust
fn server_config() -> StreamableHttpServerConfig {
    StreamableHttpServerConfig {
        max_request_bytes: MAX_CONFIG_BYTES,
        ..StreamableHttpServerConfig::stateless()
    }
}
```
Laya has no large config blob, so use a much smaller constant (args are URI, name, epochs).

**Wiring seam (lines 58-66)** is the only substantive change:
```rust
let backend = DynamoDbTaskBackend::from_env().await?;
let store = Arc::new(AprenderTaskStore::new(Arc::new(backend)));
let dispatcher: Arc<dyn Dispatcher> = Arc::new(LambdaDispatcher::from_env().await?);
// No cancel sink: ...
build_server(store, dispatcher, SERVER_NAME, env!("CARGO_PKG_VERSION"))
```
Pitfall 13 in RESEARCH: Laya must install a `CancelSink` that starts `StopExecution`; `from_env` of the SetFit backend reads `APRENDER_SETFIT_TASKS_TABLE`, so build `DynamoDbTaskBackend::new(client, table)` (dynamodb.rs:234-241) from a Laya-specific env var instead.

**Loopback proxy, CORS, GET health, once-per-container server start** (lines 100-203): copy unchanged.

---

### `crates/aprender-mcp-laya-train/src/lib.rs` (service, request-response + Tasks)

**Analog:** `crates/aprender-mcp-setfit-train/src/lib.rs` (723 lines). The tool surface is SetFit-shaped; copy the structure, not the types.

**Re-exports to reuse (lines 68-71):** `mint_id, AprenderTaskStore, BackendError, CancelSink, InMemoryTaskBackend, StoredTask, TaskBackend`. Per RESEARCH these are `pub`.

**Imports (lines 73-79):**
```rust
use pmcp::server::typed_tool::TypedTool;
use pmcp::types::capabilities::ServerCapabilities;
use pmcp::types::{CallToolResult, Content, TaskStatus, TaskSupport, ToolExecution};
use pmcp::RequestHandlerExtra;
use pmcp::Server;
```

**Args pattern (lines 143-154):** a `mod args` with `#![allow(clippy::disallowed_methods)]`, `#[derive(Debug, Deserialize, JsonSchema)]`, `#[serde(deny_unknown_fields)]`. Laya `TrainArgs { dataset_uri: String, name: String, epochs: Option<u32> }` (D-06/D-21). `validate_dataset` takes `dataset_uri` only. Name slug rule per RESEARCH A13: `^[a-z0-9][a-z0-9-]{0,47}$`.

**Mint, envelope, dispatch, compensate (lines 356-443), reuse the sequence verbatim:**
```rust
store.clear_handoff();                                   // (a)
let owner = resolve_owner(extra);
let task = store.mint_for_request(&owner, Some(TASK_TTL_MS)).await ...;   // (c) mint FIRST
store.put_envelope(&task_id, &owner, run_envelope(...)).await ...;        // (d) handles only
dispatcher.dispatch(&task_id, &owner, ...).await  // (e) Err => compensate(...)
// (f) value.insert("taskId", ...); value.insert("status","working"); value.insert("ttl", ...)
```
Insert the validation pre-filter (D-08) and the single-flight acquire between (b) and (c). Envelope carries `dataset_uri`, `name`, `output_prefix` only (RESEARCH Pattern 1). Raise `TASK_TTL_MS` from 3_600_000 (line 124) to cover 6 h + pack + retrieval (Pitfall 14, for example 72 h).

**Terminal result wrapper (lines 340-349):** reuse `terminal_result(payload, failed)`. `gate_failed` is a completed run with `pass:false` (non-error result), infrastructure failure uses `is_error`.

**Status read with link-minting at read time (lines 451-508):** copy the shape. The change is that `mint` is per manifest entry (D-18), and the stored record holds no URLs.

**Dispatcher trait:** SetFit's `Dispatcher` (artifact_uri, dataset_upload, artifact_download_url, dispatch) is SetFit-shaped. Define a Laya `Dispatcher` trait with `dispatch`, `manifest_links`, and `cancel`; keep the `Arc<dyn Dispatcher>` seam so a local fake drives unit tests (compare `local.rs`).

**Guarded terminal write** (`task_store.rs:552-569`), the finalizer must call only this:
```rust
pub async fn finish(&self, task_id, owner_id, status: TaskStatus, result: CallToolResult)
    -> Result<(), TaskStoreError>   // no-op if already terminal; first writer wins
```

---

### `crates/aprender-mcp-laya-train-lambda/src/lock.rs` (service, CAS single-flight, D-11)

**Analog:** the `TaskBackend` seam (`task_store.rs:124-150`) with `put_new` (conditional insert, `Conflict` on loss) and `cas` (expected version). No existing single-flight lock exists on Lambda: SetFit's `RunningJobs` is per-process (`lib.rs:42-48`, referenced only from `local.rs`). Build on the same backend with a reserved owner id, as RESEARCH Pattern 5 specifies.

Error handling analog: `BackendError::Conflict` at `task_store.rs:100-105` is the lost-race signal. Test with `InMemoryTaskBackend` (task_store.rs:~160-210), as the SetFit tests do.

---

### `crates/aprender-mcp-laya-train-lambda/src/dispatch/sfn.rs` and presign (service, event-driven)

**Analog:** `crates/aprender-setfit-train-lambda/src/dispatch.rs`. Replace `aws_sdk_lambda` invoke with `aws_sdk_sfn` `StartExecution`; keep the rest.

**Presign pattern (lines 177-194), copy for each manifest entry:**
```rust
let (bucket, key) = crate::parse_s3_uri(artifact_uri)?;
if bucket != self.bucket || !key.starts_with(ARTIFACT_PREFIX) { return None; }   // confused-deputy guard
let presign = PresigningConfig::expires_in(DOWNLOAD_URL_TTL).ok()?;
self.s3.get_object().bucket(bucket).key(key).presigned(presign).await.ok().map(|s| s.uri().to_string())
```
Use a longer TTL for the 0.84 GB checkpoint and `.apr` (RESEARCH Pattern 3, A9).

**Error formatting (line 90):** `DisplayErrorContext(&e)`, never `{e}`.

**HeadObject / not-found message (lines 217-237):** `e.as_service_error().is_some_and(|se| se.is_not_found())`, valid only if the role holds `s3:ListBucket` on the prefix (Pitfall 10).

**`parse_s3_uri` (lines 60-64):** reuse as-is.

**Dispatch payload rule:** only `task_id` and `owner` travel; `deny_unknown_fields` at `crates/aprender-setfit-train-lambda/src/lib.rs:89-96`. Apply the same to the Step Functions execution input.

---

### `.../src/dataset.rs` (utility, request-response)

**Analog:** `crates/aprender-setfit-train-lambda/src/dataset.rs` (`dataset_key_in(bucket, uri)` used at `dispatch.rs:209`). Copy the bucket and prefix verification, reject `..`, empty segments, other buckets and percent-encoded traversal. Drop the tar extraction path (lines 101-120): D-07 reads plain files from a prefix. Prefix becomes `datasets/<prefix>/`; fetch `task.json`, `train.jsonl`, `eval.jsonl`, optional `shift.jsonl` with HeadObject size caps before GET.

---

### `crates/aprender-mcp-laya-train-lambda/src/bin/aprender-laya-train-finalizer.rs` (service, event-driven)

**Analog:** `crates/aprender-setfit-train-lambda/src/bin/aprender-setfit-trainer.rs` (303 lines) for the "second `[[bin]]` renamed to `bootstrap` by CDK" shape, the `lambda_runtime` handler skeleton, and reading the envelope by `task_id` and `owner`. Behavior differs: read `gate-report.json` and `manifest.json` from S3, call `store.finish(...)`, write `verdict.json`, tag objects pass/fail (D-19), release the lock. Must be idempotent (duplicate invocation safe via guarded `finish`).

---

### `crates/aprender-mcp-laya-train/src/validate.rs` and `scripts/laya_train/data_cases.json` (D-08)

**Rule source:** `scripts/laya_train/data.py`. Every refusal is `DataError("REFUSED <rule>: ...")` (lines 49-55) with a stable `.rule`. Rules to mirror: `task-missing`, `task-schema`, `task-unknown-key`, `task-type`, `task-too-few-criteria`, `task-duplicate-criterion`, `train-row-label`, `eval-row-label`, `*-row-unknown-key`, `*-row-schema`, `*-row-encoding`, `eval-missing`, `eval-class-coverage`, `eval-train-overlap`, `shift-train-overlap`, `train-conflicting-labels`, `train-class-too-small`, `*-empty`.

**Normalization (data.py lines 58-73):** NFC, then split on Unicode White_Space, join with one U+0020, sha256 of utf-8. The Rust side must reuse `aprender_decide::verify::normalize_text` / `normalized_sha256` (verify.rs:1784-1796) and `aprender_decide::task::Task::from_slice` (task.rs:172), not a new normalizer. The Rust port must read `calibration_slice_fraction` and `calibration_slice_min_per_class` from `contracts/laya-finetune-gate-v1.yaml` for `train-class-too-small`; `data.py` does it via `contract.constant(...)` (selftest lines 606-607).

**Shared-table pattern:** the selftest `expect(name, rule, fn)` helper at data.py:~612 compares `e.rule == rule`. Add a loop that replays `data_cases.json` there, and a Rust `tests/validation_cases.rs` that replays the same file. Include the two documented past defects: `\r\n` handling (WR-05) and lone-surrogate escapes (data.py:138-169). Convention analog for a JSON case table beside the trainer: `scripts/laya_train/numeric_cases.json`.

Note: a new `tests/validation_cases.rs` is dark in CI until a fragment names it; copy the style of `ci/explicit-test-commands.d/530-aprender-serve-llm-endpoint-conformance.cmd` and take ordinal 535 (between 530 and the next; never renumber).

---

### `deploy-extensions/lib/laya-train-stack.ts` (config, CDK)

**Analog:** `deploy-extensions/lib/setfit-training-stack.ts`.

**Table (lines 106-117), reuse the key schema, since `DynamoDbTaskBackend` expects it:**
```ts
partitionKey: { name: 'owner_id', type: dynamodb.AttributeType.STRING },
sortKey: { name: 'task_id', type: dynamodb.AttributeType.STRING },
billingMode: dynamodb.BillingMode.PAY_PER_REQUEST,
timeToLiveAttribute: 'expires_at',
```
**Bucket (lines 122-139):** `S3_MANAGED`, `BLOCK_ALL`, `enforceSSL: true`, env-dependent `removalPolicy`. Replace the single `expire-training-artifacts` rule with tag-filtered rules (D-19): pending/fail/failed expire after 30 days, pass tag keeps. Lifecycle filters cannot negate a tag (RESEARCH A8), so use a default-expire-until-tagged-pass design or a prefix split; the planner picks.
**ListBucket scoping (lines 278-287):**
```ts
Effect: 'Allow', Action: ['s3:ListBucket'], Resource: this.artifactBucket.bucketArn,
Condition: { StringLike: { 's3:prefix': ['datasets/*'] } },
```
Request role: `GetObject` on `datasets/*` and `runs/*`, `ListBucket` on `datasets/*`. SageMaker role: read `datasets/*`, `base/*`; write `runs/*`. No `iam:*`.
**Naming and stack docstring:** the "why a separate CDK app" block (lines 21-31) applies verbatim: never hand-edit the platform-rendered `deploy/lib/stack.ts`.

**`deploy-extensions/bin/app.ts` (modify):** line 30-39 gate the stack list; extend it:
```ts
const stack = String(app.node.tryGetContext('stack') ?? 'training');
if (!['training', 'embed', 'embed-probe'].includes(stack)) {   // add 'laya-train'
```
Keep default `training` so existing recipes are unchanged. Keep `env` defaulting to `dev` (lines 20-27).

---

### `justfile` recipes (config)

| New recipe | Analog (justfile line) | What to copy |
|---|---|---|
| `synth-laya-train` | `synth-training` (129-132) | `cd deploy-extensions && npx cdk synth --context env=... --context stack=laya-train --quiet`; `trim_start_match(env, "env=")` idiom |
| `diff-/deploy-/destroy-laya-train` | 135-165 | `--profile {{profile}}`, `--require-approval never` justified by the diff gate |
| config fill from SSM/stack outputs | `pmcp-train-config` (173) | gitignored output from a tracked template; never embed account id in tracked files |
| IAM grant after OAuth | `pmcp-train-grant` (231) | grant-after-401 ordering (Pitfall 7) |
| deploy of the request fn | `laya-deploy` (2177-2200), `_laya-crates-root-swap` (2057-2090) | `refuse()` helper (`REFUSED <rule>: ...`, exit 2), `EXPECT_PKG="crates/$SERVER-lambda"`, swap+restore of `.pmcp/*` and `deploy`, backup dir guard |
| auth check | `_laya-auth-refusal-check url wait_s` (4339-4372) | URL regex fail-closed, `post()` curl helper, `tools/list` expecting 401. NOTE: its `CALL` payload hardcodes `classify`; Laya needs `train` with a harmless refused argument |
| `laya-fetch` + `laya-fetch-selftest` (D-20) | `laya-train` data-dir `test -f` guard (1352-1357); selftest style of `laya-gates-selftest` (4976) | `set -euo pipefail`, `ERROR:` exit 2, sha256 via `shasum -a 256`, tampered-file negative control, restore run + data + `.apr` (Pitfall 11) |
| `laya-dataset-put` | none; use `laya-train` arg checks | owner-run `aws s3 cp --recursive`, validates `task.json` exists first |

Shell conventions (CLAUDE.md): `#!/usr/bin/env bash`, `set -euo pipefail`, quoted vars, `bashrs lint`. The Bash tool is zsh; recipes run bash through the shebang, but local verification commands do not.

Register the recipe in `scripts/laya_gates.tsv` if the new check is a gate (tracked file; the `laya-gates-selftest` row loop reads it, lines 4976-5000).

---

### FALSIFY-MONO-011 register edits (D-22)

**`crates/aprender-core/tests/monorepo_invariants.rs`:** add one entry to `deployment_unit_bins` (line 312-323, format: `"name", // rationale`) and move the baseline.
```rust
"aprender-mcp-decide-motivation-lambda", // server id ...: ... (Phase 12 D-02)
// add:
"aprender-mcp-laya-train-lambda", // server id `aprender-mcp-laya-train`: the Laya training server's request-function bootstrap (Phase 14 D-22)
```
```rust
// 9 -> 10: GROWN (Phase 12 D-02 ...)   <- extend with a 10 -> 11 note in the same style
const DEPLOYMENT_UNIT_BASELINE: usize = 10;   // -> 11 (line 448)
```
`aprender-mcp-laya-train` must stay a library with NO `src/main.rs` and no `[[bin]]` (cargo auto-discovers `main.rs`). `aprender-mcp-laya-train-lambda` must be `publish = false`. The test message (lines 451-457) requires a recorded CONTEXT decision, which D-22 is. The finalizer is a second bin in the same package: confirm the test counts by package, not by bin (SetFit has the same two-bin shape and one entry, `aprender-setfit-train-lambda`).

**`scripts/duplicate_bin_names_allowlist.txt`:** the `bootstrap` line currently lists five packages sorted alphabetically:
`bootstrap aprender-mcp-chronos-lambda aprender-mcp-decide-antisemite-lambda aprender-mcp-decide-motivation-lambda aprender-mcp-setfit-lambda aprender-setfit-train-lambda   # ...`
Insert `aprender-mcp-laya-train-lambda` in sorted position (between `...decide-motivation-lambda` and `...setfit-lambda`), change "none of the five" and "All five" to six, and extend the history sentence (Phase 14 D-22).

**`justfile` `row_dup_bin_names` (~5584-5596):** the `must_fail no-bootstrap-line '...declared by 5 packages'` expectation (line ~5592) becomes 6. It greps the real message text, so confirm by running `just laya-gates-selftest` and `bash scripts/check_duplicate_bin_names.sh` (and its `--self-test`).

---

### `scripts/laya_train/container/entrypoint.py` and `Dockerfile.train` (no in-repo analog)

Use RESEARCH "Container entrypoint contract" as the spec. In-repo contracts to honor:
- `scripts/laya_train/train.py:66` `EXIT_PASS, EXIT_REFUSED, EXIT_GATE_FAIL = 0, 2, 3`; the wrapper maps 0 and 3 to job success with outcome in S3, 2 and crash to job failure.
- `scripts/laya_train/train.py:186-196` `request_device` falls through to CPU; assert `gate-report.json` `device_used` starts with `cuda` (train.py:906-907 records it).
- `scripts/laya_train/train.py:259-261` `Base.directory()` uses `local_files_only=True`; set `HF_HUB_OFFLINE=1` and stage the tree cache (Pitfall 4).
- Invocation, from `justfile:1352-1357`: `uv run --project scripts/laya_train --frozen python scripts/laya_train/train.py --data D --out O {{args}}`; in the container use `uv sync --frozen --no-dev`, no seeds or stopping flags.
- `scripts/laya_train/uv.lock` is darwin-arm64 only: human checkpoint before relocking (Pitfall 3, `pyproject.toml [tool.uv] environments`).

### `deploy-extensions/container/pack-arm64/Dockerfile` (no in-repo analog)

Spec: `crates/aprender-decide/examples/pack_laya.rs`. Constraint: line 61-67 resolves contracts from `env!("CARGO_MANIFEST_DIR")/../../contracts` at compile time, so build and run at one fixed path (copy `contracts/` plus an empty `crates/aprender-decide` dir). CLI forms to call: `pack_laya pack --run DIR --data DIR --base DIR --out FILE`, then `pack_laya verify FILE --run DIR --data DIR --base DIR`. Exit codes 0 accepted, 3 recomputed gate failed, 2 refusal. It reads no env var and the policy is not an argument; do not add an override. Verified `PACKED ... sha256=... shipped_seed=...` line and the `REFUSED <Variant> ...` line are what the finalizer parses.

---

## Shared Patterns

### Fail-closed named refusals
**Source:** `scripts/laya_train/data.py:49-55`; `justfile` `refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }` (laya-deploy, ~2190); `pack_laya` one-line `REFUSED <Variant> <detail>`.
**Apply to:** validation port, entrypoint, fetch recipe, all new recipes. Exit codes 0/3/2 = pass/gate fail/refused.

### Owner-scoped, handle-only envelope
**Source:** `crates/aprender-mcp-setfit-train/src/lib.rs:304-333` (`run_envelope`) and `crates/aprender-setfit-train-lambda/src/lib.rs:89-96` (`deny_unknown_fields` dispatch payload).
**Apply to:** request Lambda, Step Functions input, finalizer. Never put dataset content, tokens or presigned URLs in the envelope, SFN input, SageMaker env or DynamoDB item (380,000-byte ceiling at `dynamodb.rs:62`).

### Guarded first-writer-wins terminal write
**Source:** `task_store.rs:552-569` (`finish`).
**Apply to:** finalizer (success and Catch paths), cancel sink, compensate.

### Presigned links minted at read time, never stored
**Source:** `lib.rs:451-508` (`status`) and `dispatch.rs:177-194`.
**Apply to:** `train_status` only; `tasks/result` carries verdict and manifest without links (D-18).

### Confused-deputy guard on S3 URIs
**Source:** `dispatch.rs:181-184` plus `dataset.rs` `dataset_key_in`.
**Apply to:** `validate_dataset`, `train`, and every presign path.

### Lambda loopback proxy, stateless HTTP
**Source:** `bootstrap.rs:36-55`. Stateful/SSE config answers 503 once deployed.

### No `unwrap()`
**Source:** `.clippy.toml` ban; only `#![allow(clippy::disallowed_methods)]` around `serde_json::json!` / `JsonSchema` expansions (`bootstrap.rs:17-18`, `lib.rs:143-146`).

### Auth-on from the start
**Source:** `justfile:4339` `_laya-auth-refusal-check`, `13-HANDOFF.md` section 9. cargo-pmcp cannot render auth-on for pmcp-run: deploy, owner enables OAuth, run the check, then grant IAM (RESEARCH Pitfall 7). Re-run after every redeploy.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| Step Functions ASL (Train .sync, PackVerify ecs .sync, Finalize, Catch-to-finalizer) | orchestration | event-driven | No state machine in the repo; `dispatch.rs:4-14` documents this as the planned trigger. Use RESEARCH Pattern 4. |
| `scripts/laya_train/container/Dockerfile.train`, `entrypoint.py` | container | batch, file-I/O | No x86 or CUDA image exists. Spike 021-023 used CodeBuild ARM_CONTAINER and Fargate arm64 (see `.claude/skills/spike-findings-aprender/references/aws-mcp-model-hosting.md`) for the arm64 side only. |
| CodeBuild buildspec, ECR, Fargate task definition | infra | batch | None tracked in `deploy-extensions/` (only `embed-endpoint-stack.ts` and the SetFit stack). Check `embed-endpoint-stack.ts` for any ECR or image-asset idiom before inventing one. |
| Single-flight on Lambda | service | CAS | Built from the `TaskBackend` `put_new`/`cas` seam; no existing implementation. |

## Gotchas the planner must carry (from the analogs, verified here)

1. `DynamoDbTaskBackend::from_env` is hard-wired to the SetFit table env var; use `::new(client, table)` (`dynamodb.rs:234-241`).
2. The SetFit `Dispatcher` trait and `TrainArgs` are not reusable; the Laya crate defines its own over the same `AprenderTaskStore`.
3. `justfile` `_laya-auth-refusal-check` posts a `classify` call; reuse the URL, retry and 401 logic but change the tool name.
4. `crates/.pmcp/deploy.toml` is gitignored; edit `deploy.toml.template` and route deploys through `_laya-crates-root-swap` to avoid cargo-pmcp deploying the wrong crate silently.
5. `aws-sdk-s3` above 1.137 needs rustc 1.94.1; local rustc is 1.93. Use `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update` if needed.
6. Any new `tests/*.rs` needs a CI fragment in `ci/explicit-test-commands.d/`; `ci_run_explicit_test_commands.sh --list` exits 2 on macOS, so assert the fragment file directly.

## Metadata

**Analog search scope:** `crates/aprender-mcp-setfit-train`, `crates/aprender-setfit-train-lambda`, `crates/aprender-mcp-decide-motivation-lambda`, `crates/aprender-mcp-decide-lambda` (listing only), `deploy-extensions/`, `scripts/laya_train/`, `crates/aprender-decide/examples/pack_laya.rs`, `crates/aprender-core/tests/monorepo_invariants.rs`, `scripts/duplicate_bin_names_allowlist.txt`, `justfile`.
**Files read:** about 14 (large files read by range).
**Pattern extraction date:** 2026-10-28
**No AWS calls made. No source files modified.**
