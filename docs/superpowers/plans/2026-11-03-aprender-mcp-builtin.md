# `aprender-mcp-builtin` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The core crate of the pmcp.run `aprender` built-in server type: one library, `crates/aprender-mcp-builtin`, that parses an embedded `config.toml`, serves `classify`/`status`/`verify` over a lazily loaded `.apr` decide model, and refuses an ineligible model at build time through `bootstrap precheck`, so the platform's template can pin it by git revision.

**Architecture:** Five modules behind one entry point. `config` (strict TOML, every rule a named error), `store` (an `ObjectStore` trait with an in-memory and an S3 implementor), `precheck` (the §5.2 rule table over the store, ported from the platform's spike 036 with its 45 cases), `load` (the once-per-container model load through the fork's `LoadOnce` and `resolve_model`), `tools` (`classify`, `status`, `verify` as pmcp `SimpleTool`s, the `verify` door reading `auth_context`), `server` (builds the `pmcp::Server`), and `main_with` (argv dispatch: serve, `--serve-local`, `precheck`, usage 64). The crate never depends on the platform's `mcp-lambda-proxy`; the template passes the serving closure.

**Tech Stack:** Rust 2021 on the fork's 1.93.0 pin (must also be warning-free on 1.97.1); `aprender-decide`, `aprender-mcp-decide`, `aprender-mcp-decide-lambda` (path); pmcp 2.19 (`streamable-http`, `schema-generation`); lambda_http 0.13; aws-sdk-s3 1 (locked 1.137.0); toml 0.8; serde/serde_json; schemars 1.0; sha2; chrono; rand 0.9 + rand_chacha 0.9; tokio; futures. Dev: reqwest 0.12 (rustls), tempfile.

**Spec:** `docs/superpowers/specs/2026-11-03-aprender-built-in-server-type-design.md` (the revised draft of 2026-11-03 evening). Platform-side facts and the two prototypes being ported: `~/Development/mcp/sdk/pmcp-run/.claude/skills/spike-findings-pmcp-aprender-builtin-type/` (`references/*.md`, `sources/036-precheck-rules-offline/precheck/src/*.rs`, `sources/033-closure-on-platform-toolchain/harness/antisemitism-spike-lambda/src/main.rs`). The contract the platform compiles against: `~/Development/mcp/sdk/pmcp-run/.planning/notes/aprender-team-reply-core-crate-rev-2026-11-03.md` §2.

## Global Constraints

- Crate path `crates/aprender-mcp-builtin`, package `aprender-mcp-builtin`, lib `aprender_mcp_builtin`, `publish = false`, `[lints] workspace = true`, `version/edition/license/repository/authors/rust-version.workspace = true`. Registered in the root `Cargo.toml` `members` list after `crates/aprender-mcp-decide-lambda`.
- `unwrap()` is banned (`.clippy.toml`); use `expect` in tests only, `ok_or_else`/`?` in library code. Clippy pedantic is the workspace baseline; `cargo clippy -p aprender-mcp-builtin --lib --tests -- -D warnings` must be clean.
- Never run the `cargo pmcp` generator, never deploy, never touch AWS from this plan. Every test is offline: `MemStore` for records, the packed `laya_tiny` fixture for the model (`aprender_decide::pack_run_dir`, as `crates/aprender-mcp-decide/src/tests.rs:49-60` does).
- The binary contract (spec §5.1, §5.2, §6.2): argv `["bootstrap"]` and `["bootstrap", "--serve-local"]` serve through the closure; `["bootstrap", "precheck"]` prints and exits 0/2/3/4; anything else exits 64. Precheck output grammar, stdout only: zero or more `WARN <rule> skipped: <why>` lines, then exactly one of `ERROR: REFUSED <code>: <detail>` (exit 2 or 3), `ERROR: REFUSED s3-unreachable: <op> <uri>: <cause>` (exit 4), `ELIGIBLE {json}` (exit 0). No line ever contains a record's contents.
- Configuration grammar (spec §3.2): `id` `^[a-z][a-z0-9-]{0,62}$` and equal to `expected_id`; `schema = "aprender-server/v1"`; `environment` ∈ {`staging`, `prod`}; `[model].kind = "decide"`; `model_id` `^[a-z0-9][a-z0-9._-]{0,63}$`; `artifact_uri` `s3://bucket/key.apr`; `artifact_sha256` 64 lowercase hex; `description` ≤ 500 chars; every `[records]` URI `s3://bucket/key`, `verification_prefix` ends with `/`; `[publish].kind` ∈ {`promote`, `rollback`} default `promote`; `reason` ≤ 1000 chars; `baseline_sha256` 64 hex or `first_model`, required when `environment = "prod"`; `[verify].tolerance_abs` ∈ (0, 1.0e-3], default 5.0e-5; `[verify].default_rows` ∈ 1..=8, default 8; `deny_unknown_fields` on every section.
- Caps (spec §5.2): artifact 1,342,177,280 bytes; pack and comparison and verification records 1,048,576 bytes; eval files 32 MiB each. Pack record schema `laya-train-pack-outcome-v1`.
- Environment inputs (spec §5.3): `APRENDER_PREVIOUS_ARTIFACT_SHA256`, `APRENDER_PUBLISHED_SHA256_HISTORY` (comma-joined, oldest first), `APRENDER_BUILD_ID`; an **empty string is absent** (the buildspec exports empty strings on a first build). Runtime build id: `PMCP_BUILD_ID`.
- `verify` refusal (spec §4.5): a JSON-RPC error raised as `pmcp::Error::internal`, message exactly `Permission denied: verify requires '<role>' role`, role = `[tools].verify_required_role` or `admin`. Door order (spec §4.6): no `auth_context` → deny; any group equals the role → allow; `auth_context.subject` equals an entry of `verify_allowed_principals` → allow; else deny.
- The model loads only inside `classify` and `verify`, once per process, through `aprender_mcp_decide_lambda::LoadOnce` and `resolve_model` (fixed `s3::DOWNLOAD_DEADLINE`); `initialize`, `tools/list` and `status` never load. Bounds are parsed before the load (`parse_args` first).
- Contract bounds are `aprender_mcp_decide::ClassifyLimits::CONTRACTED`, never literals.

## Review Focus

1. **Empty environment strings.** The buildspec exports `APRENDER_PREVIOUS_ARTIFACT_SHA256=""` and `APRENDER_PUBLISHED_SHA256_HISTORY=""` on a first build; a reader that maps `""` to `Some("")` would refuse `baseline-mismatch` on every first prod publish and treat `""` as a one-entry history. Test: Task 3, `env_from_lookup_treats_empty_as_absent`.
2. **`eval.jsonl` with a trailing blank line or CRLF endings.** The trainer writes Unix lines, a hand copy may not; a blank last line must not become a row and `\r` must not enter the text sha. Test: Task 6, `eval_rows_skip_blank_lines_and_strip_cr`.
3. **A `verify` row longer than the classify door allows.** A 20,000-byte eval text would be refused by `parse_args` as `tool_rejected`; `verify` must surface that refusal, not panic or return a half record. Test: Task 6, `verify_surfaces_classify_bound_refusal`.
4. **`rows` larger than the eval set.** A 5-row eval with `rows = 8` must refuse in-band naming both numbers, never index out of range. Test: Task 6, `verify_refuses_rows_over_eval_len`.
5. **A HEAD 403 on the artifact.** Without `ListBucket` S3 answers 403 for a missing key; the detail must name both causes and exit 4, never read as a clean miss. Test: Task 2, `classify_s3_error_maps_403_to_failure_naming_both_causes`.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/aprender-mcp-builtin/Cargo.toml` | the manifest (deps above) |
| `crates/aprender-mcp-builtin/src/lib.rs` | module list, re-exports (`pub use pmcp; pub use lambda_http;`), `main_with`, argv dispatch, exit constants |
| `crates/aprender-mcp-builtin/src/config.rs` | `Config` and sections, `parse`, `ConfigError`, `config_sha256`, `PublishKind` |
| `crates/aprender-mcp-builtin/src/store.rs` | `ObjectStore` trait, `StoreError`, `MemStore`, `S3Store`, `classify_s3_error` |
| `crates/aprender-mcp-builtin/src/precheck.rs` | `Env`, `Outcome`, `run`, the rule table, caps |
| `crates/aprender-mcp-builtin/src/load.rs` | `Loaded`, `LoadFailure`, `source_from_config`, `load`, the process-wide `LoadOnce` |
| `crates/aprender-mcp-builtin/src/tools.rs` | `classify_tool`, `status_tool`, `verify_tool`, `verify_allowed`, `VerifyArgs`, `select_rows`, `EvalRow`, `read_eval_rows` |
| `crates/aprender-mcp-builtin/src/server.rs` | `build` and `build_with` |
| `crates/aprender-mcp-builtin/src/tests/mod.rs` + `cases.rs` | lib tests; `cases.rs` is the ported 45-case table |
| `crates/aprender-mcp-builtin/examples/local_server.rs` | `just builtin-smoke`'s binary: the template shape over a fixture config |
| `crates/aprender-mcp-builtin/fixtures/{smoke-valid.toml,smoke-invalid.toml}` | the two configs the smoke uses |
| `contracts/aprender-builtin-v1.yaml` | the grammar, refusal names, record schemas, falsification tests |
| `justfile` (`builtin-smoke`, `builtin-check`) | the local proof mirroring the platform's `aprender-local-smoke` |
| `crates/aprender-core/build.rs`, `crates/aprender-compute/build.rs`, `crates/aprender-compute/src/brick/quant_ops/mod.rs` | fork hygiene the platform asked for |

---

### Task 1: Crate scaffold and the `config` module

**Files:**
- Create: `crates/aprender-mcp-builtin/Cargo.toml`, `src/lib.rs`, `src/config.rs`, `src/tests/mod.rs`
- Modify: `Cargo.toml` (root) `members`, after the line `"crates/aprender-mcp-decide-lambda",`

**Interfaces:**
- Produces: `config::Config { id: String, schema: String, environment: Environment, model: ModelSection, records: RecordsSection, publish: PublishSection, hold: Option<HoldSection>, tools: ToolsSection, verify: VerifySection }` with the field names of the 036 prototype `config.rs` (`ModelSection { kind, model_id, artifact_uri, artifact_sha256, labels: Option<Vec<String>>, description: Option<String> }`, `RecordsSection { pack_outcome_uri, eval_jsonl_uri, eval_probs_uri, rescore_noise_uri, comparison_uri, staging_verification_uri, verification_prefix }`, `PublishSection { kind: PublishKind, reason, baseline_sha256: Option<String>, clears_hold: bool }`, `HoldSection { reason }`, `ToolsSection { verify_required_role: Option<String>, verify_allowed_principals: Vec<String>, status_required_role: Option<String> }`, `VerifySection { tolerance_abs: Option<f64>, default_rows: Option<u32> }`).
- `pub enum Environment { Staging, Prod }` (serde `rename_all = "lowercase"`), `pub enum PublishKind { Promote, Rollback }` (serde lowercase, `#[default] Promote`).
- `pub struct ConfigError { pub key: String, pub detail: String }` whose `Display` is `config-invalid: {key} {detail}` (for a serde error `key` is `toml` and `detail` is `e.message()`, which names the unknown field itself).
- `pub fn parse(toml_text: &str, expected_id: &str) -> Result<Config, ConfigError>`
- `pub fn config_sha256(toml_text: &str) -> String` (via `aprender_mcp_decide_lambda::sha256_hex`)
- `impl Config { pub fn is_prod(&self) -> bool; pub fn tolerance_abs(&self) -> f64 /* default 5.0e-5 */; pub fn default_rows(&self) -> u32 /* default 8 */; pub fn verify_role(&self) -> &str /* default "admin" */ }`
- `pub const SCHEMA: &str = "aprender-server/v1"; pub const FIRST_MODEL: &str = "first_model"; pub fn is_sha256_hex(s: &str) -> bool`

- [ ] **Step 1: Write the manifest and register the member**

`Cargo.toml` of the crate: package fields as in `crates/aprender-mcp-decide-lambda/Cargo.toml` (workspace inheritance, `publish = false`, `[lints] workspace = true`), description `The pmcp.run aprender built-in server type, model side: config, lazy load, classify/status/verify, bootstrap precheck`. Dependencies: `aprender-decide`, `aprender-mcp-decide`, `aprender-mcp-decide-lambda` (path); `pmcp = { version = "2.19", features = ["streamable-http", "schema-generation"] }`; `lambda_http = "0.13"`; `aws-config = "1"`; `aws-sdk-s3 = "1"`; `schemars = "1.0"`; `tracing = "0.1"`; and `{ workspace = true }` for `serde`, `serde_json`, `toml`, `tokio`, `futures`, `sha2`, `chrono`, `rand`, `rand_chacha`. Dev-dependencies: `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }`, `tempfile = { workspace = true }`, `tokio-util = "0.7"` (pmcp's `CancellationToken`, already in the closure), `serde_yaml = { workspace = true }` (Task 10's constants test). Copy the comment from the lambda crate's manifest about never `cargo update`-ing the aws crates.

`src/lib.rs` for now: `pub mod config;` and `#[cfg(test)] mod tests;`. `src/tests/mod.rs`: `mod config;` with the tests below in `src/tests/config.rs`.

- [ ] **Step 2: Write the failing config tests** (`src/tests/config.rs`)

Fixture text: port `staging_config()` and `prod_config(baseline, kind, artifact)` from the 036 `cases.rs` verbatim (ids `antisemitism-staging` / `antisemitism-prod`, shas `A`/`B`/`C`, the two prefixes) into `src/tests/cases.rs` (`pub` within the tests module); this task writes only the config-related part of that file, Task 3 completes it.

```rust
#[test] fn parses_the_staging_example() { let c = parse(&staging_config(), SERVER_ID).expect("parses"); assert_eq!(c.environment, Environment::Staging); assert_eq!(c.publish.kind, PublishKind::Promote); assert_eq!(c.tolerance_abs(), 5.0e-5); assert_eq!(c.default_rows(), 8); assert_eq!(c.verify_role(), "admin"); }
#[test] fn parses_the_prod_rollback_example() { let c = parse(&prod_config(A, "rollback", B), PROD_ID).expect("parses"); assert!(c.is_prod()); assert_eq!(c.publish.kind, PublishKind::Rollback); assert_eq!(c.tools.verify_allowed_principals, vec!["3k9f2m2mclientid"]); }
#[test] fn unknown_key_names_the_field() { let e = parse(&staging_config().replace("[model]\n", "[model]\nmemory_mb = 10240\n"), SERVER_ID).expect_err("refused"); assert!(e.to_string().starts_with("config-invalid: "), "{e}"); assert!(e.to_string().contains("memory_mb"), "{e}"); }
#[test] fn unknown_section_is_refused() { assert!(parse(&(staging_config() + "\n[bounds]\nmax_texts = 4\n"), SERVER_ID).is_err()); }
#[test] fn id_must_be_lowercase_and_match_expected() { let upper = staging_config().replace("id = \"antisemitism-staging\"", "id = \"Antisemitism-staging\""); assert_eq!(parse(&upper, "Antisemitism-staging").expect_err("refused").key, "id"); assert_eq!(parse(&staging_config(), "antisemitism-other").expect_err("refused").key, "id"); }
#[test] fn every_named_rule_names_its_key() { for (edit, key) in [ (("aprender-server/v1", "aprender-server/v2"), "schema"), (("environment = \"staging\"", "environment = \"test\""), "environment"), (("kind = \"decide\"", "kind = \"setfit\""), "[model].kind"), ] { let text = staging_config().replace(edit.0, edit.1); let e = parse(&text, SERVER_ID).expect_err(key); assert!(e.to_string().contains(key), "{e} should name {key}"); } let e = parse(&staging_config().replace(&format!("artifact_sha256 = \"{A}\""), &format!("artifact_sha256 = \"{}\"", A.to_uppercase())), SERVER_ID).expect_err("sha"); assert_eq!(e.key, "[model].artifact_sha256"); }
#[test] fn prod_requires_baseline() { let e = parse(&prod_config("first_model", "promote", A).replace("baseline_sha256 = \"first_model\"\n", ""), PROD_ID).expect_err("refused"); assert_eq!(e.key, "[publish].baseline_sha256"); }
#[test] fn verify_bounds_are_checked() { assert_eq!(parse(&(staging_config() + "\n[verify]\ntolerance_abs = 2.0e-3\n"), SERVER_ID).expect_err("tol").key, "[verify].tolerance_abs"); assert_eq!(parse(&(staging_config() + "\n[verify]\ndefault_rows = 9\n"), SERVER_ID).expect_err("rows").key, "[verify].default_rows"); }
#[test] fn verification_prefix_needs_trailing_slash() { let e = parse(&staging_config().replace("verifications/\"", "verifications\""), SERVER_ID).expect_err("refused"); assert_eq!(e.key, "[records].verification_prefix"); }
#[test] fn config_sha256_is_the_sha_of_the_bytes() { assert_eq!(config_sha256("x"), aprender_mcp_decide_lambda::sha256_hex(b"x")); }
```

Note on `environment` and `kind` keys: serde refuses an unknown enum variant before our rules run, so those two assertions check that the serde message (which contains the key name, e.g. `environment`) is carried into `ConfigError.detail`; the test passes on `contains`.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p aprender-mcp-builtin --lib config`
Expected: compile error, `config` module missing.

- [ ] **Step 4: Implement `config.rs`**

Port the 036 `config.rs`: the structs with `#[serde(deny_unknown_fields)]`; `Environment` and `PublishKind` as serde enums (`#[serde(rename_all = "lowercase")]`, `PublishKind` with `#[serde(default)]` on the field and `#[default] Promote`); `parse` applying the Global Constraints grammar in the prototype's order, each failure a `ConfigError { key, detail }` with the key spelled as in the tests (`id`, `schema`, `[model].kind`, `[model].model_id`, `[model].artifact_uri`, `[model].artifact_sha256`, `[model].description`, `[records].<name>`, `[records].verification_prefix`, `[publish].reason`, `[publish].baseline_sha256`, `[verify].tolerance_abs`, `[verify].default_rows`); the id rule is `^[a-z][a-z0-9-]{0,62}$`. Serde failures: `ConfigError { key: "toml".into(), detail: e.message().to_string() }`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p aprender-mcp-builtin --lib config`
Expected: all PASS.

- [ ] **Step 6: Lint and commit**

Run: `cargo fmt --all && cargo clippy -p aprender-mcp-builtin --lib --tests -- -D warnings`
Expected: clean.

```bash
git add Cargo.toml Cargo.lock crates/aprender-mcp-builtin
git commit -m "feat(builtin): the aprender-mcp-builtin crate with the strict config.toml parser (aprender-server/v1)"
```

---

### Task 2: The `store` module: `ObjectStore`, `MemStore`, `S3Store`

**Files:**
- Create: `src/store.rs`, `src/tests/store.rs`
- Modify: `src/lib.rs` (`pub mod store;`), `src/tests/mod.rs`

**Interfaces:**
- `pub struct StoreError(pub String)` (Display = the string)
- `pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StoreError>> + Send + 'a>>`
- `pub trait ObjectStore: Send + Sync { fn head<'a>(&'a self, uri: &'a str) -> StoreFuture<'a, Option<u64>>; fn get<'a>(&'a self, uri: &'a str, cap: u64) -> StoreFuture<'a, Option<Vec<u8>>>; fn put_if_absent<'a>(&'a self, uri: &'a str, bytes: Vec<u8>) -> StoreFuture<'a, PutOutcome>; }`
- `pub enum PutOutcome { Written, AlreadyExists }`
- `pub struct MemStore { .. }` with `pub fn put(&self, uri, bytes)`, `remove`, `declare_size`, `fail` (interior `std::sync::Mutex` so a shared `Arc<MemStore>` can be mutated by tests), `pub fn get_sync(&self, uri) -> Option<Vec<u8>>`, `pub fn keys_under(&self, prefix: &str) -> Vec<String>`.
- `pub struct S3Store { client: aws_sdk_s3::Client }` with `pub fn new(client) -> Self`, `pub async fn from_default_config() -> Self`.
- `pub enum SdkOutcome { Absent, Failure(String) }` and `pub fn classify_s3_error(op: &str, uri: &str, status: Option<u16>, code: Option<&str>, display: &str) -> SdkOutcome`: `code` ∈ {`NotFound`, `NoSuchKey`} or `status == Some(404)` → `Absent`; `status == Some(403)` → `Failure("{op} {uri}: 403 Forbidden (the object is missing, or the builder role cannot read it)")`; else `Failure("{op} {uri}: {display}")`.
- `pub fn parse_s3_uri(uri: &str) -> Result<(String, String), StoreError>` (re-use `aprender_mcp_decide_lambda::parse_s3_uri`, mapping its error to `StoreError`).

- [ ] **Step 1: Write the failing tests** (`src/tests/store.rs`)

```rust
#[tokio::test] async fn mem_store_head_get_and_declared_size() { let s = MemStore::default(); s.put("s3://b/k", b"abc".to_vec()); assert_eq!(s.head("s3://b/k").await.expect("ok"), Some(3)); assert_eq!(s.get("s3://b/k", 10).await.expect("ok"), Some(b"abc".to_vec())); assert_eq!(s.get("s3://b/k", 2).await.expect("ok"), None, "over the cap reads as absent"); s.declare_size("s3://b/k", 1_342_177_281); assert_eq!(s.head("s3://b/k").await.expect("ok"), Some(1_342_177_281)); assert_eq!(s.head("s3://b/none").await.expect("ok"), None); }
#[tokio::test] async fn mem_store_failures_look_like_network_errors() { let s = MemStore::default(); s.fail("s3://b/k"); let e = s.head("s3://b/k").await.expect_err("fails"); assert!(e.0.starts_with("HeadObject s3://b/k: "), "{}", e.0); let e = s.get("s3://b/k", 10).await.expect_err("fails"); assert!(e.0.starts_with("GetObject s3://b/k: "), "{}", e.0); }
#[tokio::test] async fn put_if_absent_is_create_only() { let s = MemStore::default(); assert!(matches!(s.put_if_absent("s3://b/p/x.json", b"1".to_vec()).await.expect("ok"), PutOutcome::Written)); assert!(matches!(s.put_if_absent("s3://b/p/x.json", b"2".to_vec()).await.expect("ok"), PutOutcome::AlreadyExists)); assert_eq!(s.get_sync("s3://b/p/x.json"), Some(b"1".to_vec())); assert_eq!(s.keys_under("s3://b/p/"), vec!["s3://b/p/x.json".to_string()]); }
#[test] fn classify_s3_error_maps_404_and_codes_to_absent() { for (status, code) in [(Some(404), None), (None, Some("NotFound")), (None, Some("NoSuchKey"))] { assert!(matches!(classify_s3_error("HeadObject", "s3://b/k", status, code, "x"), SdkOutcome::Absent)); } }
#[test] fn classify_s3_error_maps_403_to_failure_naming_both_causes() { let SdkOutcome::Failure(d) = classify_s3_error("HeadObject", "s3://b/k", Some(403), None, "service error") else { panic!("403 is a failure") }; assert_eq!(d, "HeadObject s3://b/k: 403 Forbidden (the object is missing, or the builder role cannot read it)"); }
#[test] fn classify_s3_error_other_failures_carry_the_display() { let SdkOutcome::Failure(d) = classify_s3_error("GetObject", "s3://b/k", Some(500), None, "dispatch failure: timeout") else { panic!() }; assert_eq!(d, "GetObject s3://b/k: dispatch failure: timeout"); }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p aprender-mcp-builtin --lib store`
Expected: compile error, `store` missing.

- [ ] **Step 3: Implement `store.rs`**

`MemStore` ports the 036 `store.rs` behind a `Mutex<Inner>`; `get` returns `None` when the declared or real size exceeds `cap`. `S3Store::head`: `head_object` → `content_length` as `u64`; on `SdkError::ServiceError` classify with `e.raw().status().as_u16()` and `e.err().meta().code()`; on other `SdkError`s, `Failure` with `DisplayErrorContext`. `S3Store::get`: `head` first (absent or over cap → `Ok(None)`), then `get_object` and collect the body with `body.collect().await` into `Vec<u8>` (the size is already known to be ≤ cap). `put_if_absent`: `put_object().if_none_match("*")`; a service error with status 412 or code `PreconditionFailed` → `AlreadyExists`. Every S3 call's error string goes through `classify_s3_error`.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p aprender-mcp-builtin --lib store`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy -p aprender-mcp-builtin --lib --tests -- -D warnings
git add crates/aprender-mcp-builtin && git commit -m "feat(builtin): ObjectStore with an in-memory store and the S3 store (HEAD, bounded GET, create-only PUT)"
```

---

### Task 3: The `precheck` module and its 45 cases

**Files:**
- Create: `src/precheck.rs`, `src/tests/precheck.rs`; complete `src/tests/cases.rs`
- Modify: `src/lib.rs`, `src/tests/mod.rs`

**Interfaces:**
- `#[derive(Debug, Clone, Default)] pub struct Env { pub previous_artifact_sha256: Option<String>, pub published_sha256_history: Option<Vec<String>>, pub build_id: Option<String> }` with `pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Env` (reads `APRENDER_PREVIOUS_ARTIFACT_SHA256`, `APRENDER_PUBLISHED_SHA256_HISTORY`, `APRENDER_BUILD_ID`; a value that is empty after `trim()` is `None`; the history is split on `,`, trimmed, empty parts dropped) and `pub fn from_env() -> Env`.
- `pub struct Outcome { pub exit: i32, pub code: Option<String>, pub warns: Vec<String>, pub last_line: String }` with `pub fn render(&self) -> String` (the WARN lines, then `last_line`, each `\n`-terminated).
- `pub const EXIT_ELIGIBLE: i32 = 0; EXIT_REFUSED = 2; EXIT_CONFIG_INVALID = 3; EXIT_STORE_UNREACHABLE = 4;`
- `pub const ARTIFACT_CAP: u64 = 1_342_177_280; RECORD_CAP: u64 = 1_048_576; EVAL_CAP: u64 = 32 * 1024 * 1024; PACK_SCHEMA: &str = "laya-train-pack-outcome-v1";`
- `pub async fn run(toml_text: &str, expected_id: &str, env: &Env, store: &dyn ObjectStore) -> Outcome`

- [ ] **Step 1: Complete `cases.rs` and write the matrix test**

Port the 036 `cases.rs` in full (`pack_record`, `EVAL_JSONL`, `EVAL_PROBS`, `comparison`, `verification`, `base_store` over the Task 2 `MemStore`, `Case`, `env`, `all()` with P1–P8 and N01–N37). Then in `src/tests/precheck.rs`:

```rust
#[tokio::test] async fn the_case_table_holds() { for case in cases::all() { let out = precheck::run(&case.config, case.expected_id, &case.env, &case.store).await; assert_eq!(out.exit, case.expect_exit, "{}: {}\n{}", case.name, case.what, out.render()); assert_eq!(out.code.as_deref(), case.expect_code, "{}", case.name); let last = out.render(); let last = last.lines().last().expect("a verdict line"); match case.expect_exit { 0 => assert!(last.starts_with("ELIGIBLE {"), "{}: {last}", case.name), 4 => assert!(last.starts_with("ERROR: REFUSED s3-unreachable: "), "{}: {last}", case.name), _ => assert!(last.starts_with(&format!("ERROR: REFUSED {}: ", case.expect_code.expect("code"))), "{}: {last}", case.name), } assert!(!last.contains("ERROR: ERROR"), "{}", case.name); } }
#[tokio::test] async fn warns_precede_the_verdict_and_are_echoed_in_eligible() { let case = cases::all().into_iter().find(|c| c.name == "P5").expect("P5"); let out = precheck::run(&case.config, case.expected_id, &case.env, &case.store).await; assert_eq!(out.warns, vec!["rollback-unknown-sha skipped: no published-sha history was passed"]); let r = out.render(); assert!(r.starts_with("WARN rollback-unknown-sha skipped: ")); let json: serde_json::Value = serde_json::from_str(r.lines().last().expect("line").trim_start_matches("ELIGIBLE ")).expect("json"); assert_eq!(json["warns"][0], "rollback-unknown-sha skipped: no published-sha history was passed"); assert_eq!(json["publish_kind"], "rollback"); }
#[tokio::test] async fn no_output_line_contains_a_record_body() { let case = cases::all().into_iter().find(|c| c.name == "P3").expect("P3"); let r = precheck::run(&case.config, case.expected_id, &case.env, &case.store).await.render(); assert!(!r.contains("PACKED waived=none") && !r.contains("difference_interval_90") && !r.contains("verified_by\":\"analyst"), "{r}"); let json: serde_json::Value = serde_json::from_str(r.lines().last().expect("line").trim_start_matches("ELIGIBLE ")).expect("json"); assert_eq!(json["comparison"]["verdict"], "clearly_better"); assert_eq!(json["staging_verification"]["max_abs_diff"], 2.03e-6); assert_eq!(json["eval_jsonl_sha256"], aprender_mcp_decide_lambda::sha256_hex(cases::EVAL_JSONL.as_bytes())); }
#[test] fn env_from_lookup_treats_empty_as_absent() { let e = Env::from_lookup(|k| match k { "APRENDER_PREVIOUS_ARTIFACT_SHA256" => Some(String::new()), "APRENDER_PUBLISHED_SHA256_HISTORY" => Some(" ".into()), "APRENDER_BUILD_ID" => Some("codebuild:1".into()), _ => None }); assert_eq!(e.previous_artifact_sha256, None); assert_eq!(e.published_sha256_history, None); assert_eq!(e.build_id.as_deref(), Some("codebuild:1")); let e = Env::from_lookup(|k| (k == "APRENDER_PUBLISHED_SHA256_HISTORY").then(|| format!("{},{}", cases::B, cases::A))); assert_eq!(e.published_sha256_history, Some(vec![cases::B.to_string(), cases::A.to_string()])); }
#[test] fn config_invalid_line_names_the_key() { let out = futures::executor::block_on(precheck::run(&cases::staging_config().replace("[model]\n", "[model]\nmemory_mb = 10240\n"), cases::SERVER_ID, &Env::default(), &MemStore::default())); assert_eq!(out.exit, 3); assert!(out.last_line.starts_with("ERROR: REFUSED config-invalid: ") && out.last_line.contains("memory_mb"), "{}", out.last_line); }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p aprender-mcp-builtin --lib precheck`
Expected: compile error.

- [ ] **Step 3: Implement `precheck.rs`**

Port the 036 `precheck.rs` rule by rule and in its order, with these differences only: the store is awaited; `Outcome::refused` renders `ERROR: REFUSED {code}: {detail}`; a config failure renders `ERROR: REFUSED {ConfigError}` (its Display already starts with `config-invalid: `); a store failure renders `ERROR: REFUSED s3-unreachable: {StoreError}` (the store's string already reads `<op> <uri>: <cause>`); the artifact-missing detail is `HeadObject {uri}: no such object`; `config_sha256` comes from `config::config_sha256`; the `hold`/`baseline`/`rollback` block reads `cfg.publish.kind` as the enum. The `ELIGIBLE` json keys are the prototype's (`config_sha256, server, environment, kind, model_id, artifact_sha256, publish_kind, baseline_sha256, previous_artifact_sha256, build_id, artifact_bytes, pack{task_id, pack_image_digest, parity_contract_sha256, verify_seconds}, eval_jsonl_bytes, eval_jsonl_sha256, eval_probs_bytes, eval_probs_sha256, hold?, comparison?, staging_verification?, warns`).

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p aprender-mcp-builtin --lib precheck`
Expected: PASS, 45/45 in the matrix.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy -p aprender-mcp-builtin --lib --tests -- -D warnings
git add crates/aprender-mcp-builtin && git commit -m "feat(builtin): bootstrap precheck rule table over the object store, 45 cases, the ERROR: REFUSED grammar"
```

---

### Task 4: The `load` module: once-per-process model load from the configuration

**Files:**
- Create: `src/load.rs`, `src/tests/load.rs`, `src/tests/fixture.rs`
- Modify: `src/lib.rs`, `src/tests/mod.rs`

**Interfaces:**
- `pub struct Loaded { pub service: aprender_mcp_decide::ClassifyService, pub labels: Vec<String>, pub timeline: aprender_mcp_decide_lambda::LoadTimeline, pub performed_at_unix: u64 }`
- `pub struct LoadFailure { pub kind: &'static str, pub detail: String }` (`kind` ∈ the `ResolveError::kind()` values, plus `labels_mismatch`, `config_error`)
- `pub fn source_from_config(model: &config::ModelSection) -> Result<ModelSource, LoadFailure>` (via `ModelSource::from_lookup` mapping `ENV_S3_URI` → `artifact_uri`, `ENV_SHA256` → `artifact_sha256`)
- `pub async fn load(source: &ModelSource, expected_labels: Option<&[String]>) -> Result<Loaded, LoadFailure>`: `resolve_model(source)` → `Arc<Model>` → `check_served_task_fits(&model, &ClassifyLimits::CONTRACTED)` (failure kind `config_error`) → labels = `model.task().labels().to_vec()`; if `expected_labels` is `Some` and differs → `labels_mismatch` with detail `config labels [..] are not the artifact's [..]` → `Loaded { service: ClassifyService::served(model), .. }`.
- `pub struct ModelCell(LoadOnce<Loaded>)` with `pub const fn new()`, `pub fn get(&self) -> Option<Arc<Loaded>>`, `pub async fn get_or_load(&self, source: &ModelSource, expected_labels: Option<&[String]>) -> Result<(Arc<Loaded>, bool), LoadFailure>`; `pub static MODEL: ModelCell` is the process-wide cell `server::build` uses, and tests build servers over their own `&'static ModelCell` (a `Box::leak`), so one test process can hold several servers.
- `src/tests/fixture.rs`: `pub fn tiny_apr() -> (tempfile::TempDir, PathBuf, String /* sha256 */)` packing `crates/aprender-decide/tests/fixtures/laya_tiny` with `aprender_decide::pack_run_dir(&dir, &dir.join("data"))` (as `aprender-mcp-decide/src/tests.rs:49-60`), writing `tiny.apr` into a temp dir; `pub fn tiny_labels() -> Vec<String>` read from the fixture's `eval-probs.json` `labels`; `pub fn local_source(path, sha) -> ModelSource` (`ModelSource::from_lookup` with `ENV_MODEL` and `ENV_SHA256`).

- [ ] **Step 1: Write the failing tests** (`src/tests/load.rs`)

```rust
#[tokio::test] async fn loads_the_tiny_fixture_once() { let (_d, path, sha) = fixture::tiny_apr(); let cell: &'static ModelCell = Box::leak(Box::new(ModelCell::new())); assert!(cell.get().is_none()); let (first, performed) = cell.get_or_load(&fixture::local_source(&path, &sha), Some(&fixture::tiny_labels())).await.expect("loads"); assert!(performed); assert_eq!(first.labels, fixture::tiny_labels()); assert_eq!(first.timeline.artifact_sha256, sha); let (_again, performed) = cell.get_or_load(&fixture::local_source(&path, &sha), None).await.expect("cached"); assert!(!performed); }
#[tokio::test] async fn labels_mismatch_refuses_before_serving() { let (_d, path, sha) = fixture::tiny_apr(); let wrong = vec!["a".to_string(), "b".to_string(), "c".to_string()]; let e = load(&fixture::local_source(&path, &sha), Some(&wrong)).await.expect_err("refused"); assert_eq!(e.kind, "labels_mismatch"); assert!(e.detail.contains("shipping"), "{}", e.detail); }
#[tokio::test] async fn sha_mismatch_is_refused_before_the_ladder() { let (_d, path, _sha) = fixture::tiny_apr(); let e = load(&fixture::local_source(&path, &"0".repeat(64)), None).await.expect_err("refused"); assert_eq!(e.kind, "hash_mismatch"); }
#[test] fn source_from_config_maps_the_uri_and_pin() { let c = config::parse(&cases::staging_config(), cases::SERVER_ID).expect("parses"); let s = source_from_config(&c.model).expect("source"); assert!(matches!(s, ModelSource::S3 { .. })); assert_eq!(s.kind(), "s3"); }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p aprender-mcp-builtin --lib load`
Expected: compile error.

- [ ] **Step 3: Implement `load.rs` and `tests/fixture.rs`**

As the Interfaces block; `performed_at_unix` from `SystemTime::now().duration_since(UNIX_EPOCH)`, `0` on error. Mirror the 033 harness `load` (`references/lazy-load-runtime.md` §1), adding the labels check.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p aprender-mcp-builtin --lib load`
Expected: PASS (the tiny pack takes a few seconds).

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy -p aprender-mcp-builtin --lib --tests -- -D warnings
git add crates/aprender-mcp-builtin && git commit -m "feat(builtin): once-per-process model load from the configuration with the labels check"
```

---

### Task 5: `classify`, `status` and `server::build`

**Files:**
- Create: `src/tools.rs`, `src/server.rs`, `src/tests/server.rs`, `src/tests/rpc.rs`
- Modify: `src/lib.rs`, `src/tests/mod.rs`

**Interfaces:**
- `pub struct Runtime { pub config: Arc<config::Config>, pub source: ModelSource, pub store: Arc<dyn ObjectStore>, pub cell: &'static ModelCell, pub config_sha256: String, pub build_id: Option<String> }` (Clone)
- `tools::classify_tool(rt: Runtime) -> impl pmcp::ToolHandler + 'static` (name `aprender_mcp_decide::TOOL_NAME`; description `tools::classify_description(&config.model)` = `[model].description` + one space, then the 033 harness sentence with `ClassifyLimits::CONTRACTED` values and the labels; `with_schema_from::<ClassifyArgs>()`); body: `parse_args(args)?` first, then `rt.cell.get_or_load(&rt.source, rt.config.model.labels.as_deref()).await` mapped to `pmcp::Error::internal(format!("model load failed ({kind}): {detail}; the next call retries"))`, `tracing::info!` of `load_log_line(performed, None, load_ms, performed.then_some(&loaded.timeline))`, then `loaded.service.call(parsed).await?` serialised to JSON.
- `tools::status_tool(rt: Runtime) -> impl ToolHandler + 'static` (name `status`, `StatusArgs {}` with `deny_unknown_fields`): the 033 harness JSON with these keys: `schema: "aprender-status/v1"`, `server`, `environment` (`"staging"`/`"prod"`), `model {kind, model_id, artifact_sha256, labels}`, `loaded`, `load` (null, or `{performed_at_unix, download_ms (= timeline.fetch_ms), sha_ms, build_ms, bytes, rss_mb, peak_rss_mb, graviton, source, artifact_sha256}`), `bounds {max_texts, max_text_bytes, max_total_tokens, max_in_flight, max_pending}`, `publish {kind, reason, baseline_sha256}`, `hold` (null or `{reason}`), `records {pack_outcome_uri, eval_jsonl_uri, eval_probs_uri, verification_prefix}`, `identity` (null, or `{subject, email, groups}` from `extra.auth_context`), `build {build_id, config_sha256, crate_version}`. If `[tools].status_required_role` is set, apply `verify_allowed` with that role (Task 6 defines it; this task stubs the call as always-open and Task 6 wires it).
- `server::build_with(rt: Runtime, server_name: &str, version: &str) -> pmcp::Result<pmcp::Server>` registering the three tools (`verify` arrives in Task 6; this task registers two) with `ServerCapabilities::tools_only()`.
- `pub fn build(toml_text: &str, config: Config, server_name: &str, version: &str, build_id: Option<String>, store: Arc<dyn ObjectStore>) -> Result<Server, BuildError>` with `pub enum BuildError { Source(LoadFailure), Server(pmcp::Error) }` (Display, std Error): derives `source_from_config(&config.model)`, uses the process-wide `load::MODEL` cell and `config_sha256(toml_text)`, then calls `build_with`.
- `src/tests/rpc.rs`: `pub async fn serve(server: pmcp::Server) -> (String /* http://127.0.0.1:port/mcp */, JoinHandle<()>)` via `aprender_mcp_decide_lambda::start_loopback(server, "127.0.0.1:0")`; `pub async fn call(url: &str, method: &str, params: serde_json::Value, headers: &[(&str, &str)]) -> serde_json::Value` posting a JSON-RPC 2.0 request with `Accept: application/json, text/event-stream` and returning the parsed response object; `pub async fn tool(url, name, args, headers) -> serde_json::Value` (the `result` or the `error` object of a `tools/call`).

- [ ] **Step 1: Write the failing tests** (`src/tests/server.rs`)

```rust
fn runtime_over(path: &Path, sha: &str, store: Arc<MemStore>) -> (Runtime, String) { let text = cases::staging_config().replace(cases::A, sha); let config = config::parse(&text, cases::SERVER_ID).expect("parses"); let cell: &'static ModelCell = Box::leak(Box::new(ModelCell::new())); (Runtime { config: Arc::new(config), source: fixture::local_source(path, sha), store, cell, config_sha256: config::config_sha256(&text), build_id: Some("codebuild:test".into()) }, text) }
#[tokio::test] async fn tools_list_describes_classify_from_config_without_loading() { let (_d, path, sha) = fixture::tiny_apr(); let (rt, _) = runtime_over(&path, &sha, Arc::new(MemStore::default())); let (url, _h) = rpc::serve(server::build_with(rt.clone(), "antisemitism-staging", "0.0.0").expect("server")).await; let list = rpc::call(&url, "tools/list", json!({}), &[]).await; let names: Vec<&str> = list["result"]["tools"].as_array().expect("tools").iter().map(|t| t["name"].as_str().expect("name")).collect(); assert!(names.contains(&"classify") && names.contains(&"status")); let desc = list["result"]["tools"].as_array().expect("tools").iter().find(|t| t["name"] == "classify").expect("classify")["description"].as_str().expect("desc").to_string(); assert!(desc.starts_with("Antisemitism decider, GoldStandard2024"), "{desc}"); assert!(desc.contains("1..8 texts") && desc.contains("800 built tokens"), "{desc}"); assert!(rt.cell.get().is_none(), "tools/list must not load"); }
#[tokio::test] async fn status_reports_not_loaded_then_loaded_after_classify() { let (_d, path, sha) = fixture::tiny_apr(); let (rt, text) = runtime_over(&path, &sha, Arc::new(MemStore::default())); let (url, _h) = rpc::serve(server::build_with(rt.clone(), "antisemitism-staging", "0.0.0").expect("server")).await; let before = rpc::tool(&url, "status", json!({}), &[]).await; let s: serde_json::Value = serde_json::from_str(before["content"][0]["text"].as_str().expect("text")).expect("json"); assert_eq!(s["schema"], "aprender-status/v1"); assert_eq!(s["loaded"], false); assert!(s["load"].is_null()); assert_eq!(s["bounds"]["max_texts"], 8); assert_eq!(s["build"]["config_sha256"], config::config_sha256(&text)); assert_eq!(s["build"]["build_id"], "codebuild:test"); assert!(s["identity"].is_null()); let r = rpc::tool(&url, "classify", json!({"texts": ["The box is two weeks late and nobody answers."]}), &[]).await; let c: serde_json::Value = serde_json::from_str(r["content"][0]["text"].as_str().expect("text")).expect("json"); assert_eq!(c["model"]["artifact_sha256"], sha); assert_eq!(c["results"].as_array().expect("results").len(), 1); let after = rpc::tool(&url, "status", json!({}), &[]).await; let s: serde_json::Value = serde_json::from_str(after["content"][0]["text"].as_str().expect("text")).expect("json"); assert_eq!(s["loaded"], true); assert_eq!(s["load"]["artifact_sha256"], sha); assert_eq!(s["load"]["source"], "local"); }
#[tokio::test] async fn status_echoes_the_proxy_identity() { let (_d, path, sha) = fixture::tiny_apr(); let (rt, _) = runtime_over(&path, &sha, Arc::new(MemStore::default())); let (url, _h) = rpc::serve(server::build_with(rt, "antisemitism-staging", "0.0.0").expect("server")).await; let r = rpc::tool(&url, "status", json!({}), &[("x-pmcp-user-id", "sub-1"), ("x-pmcp-user-groups", "admin,analysts"), ("x-pmcp-user-email", "a@example.org")]).await; let s: serde_json::Value = serde_json::from_str(r["content"][0]["text"].as_str().expect("text")).expect("json"); assert_eq!(s["identity"]["subject"], "sub-1"); assert_eq!(s["identity"]["groups"], json!(["admin", "analysts"])); assert_eq!(s["identity"]["email"], "a@example.org"); }
#[tokio::test] async fn an_oversized_request_is_refused_before_the_load() { let (_d, path, sha) = fixture::tiny_apr(); let (rt, _) = runtime_over(&path, &sha, Arc::new(MemStore::default())); let (url, _h) = rpc::serve(server::build_with(rt.clone(), "antisemitism-staging", "0.0.0").expect("server")).await; let r = rpc::tool(&url, "classify", json!({"texts": ["x"; 9]}), &[]).await; assert_eq!(r["isError"], true, "{r}"); assert!(r["content"][0]["text"].as_str().expect("text").contains("classify_max_texts"), "{r}"); assert!(rt.cell.get().is_none(), "a refused request never loads"); }
#[test] fn build_derives_the_s3_source_from_config() { let text = cases::staging_config(); let c = config::parse(&text, cases::SERVER_ID).expect("parses"); assert!(server::build(&text, c, "antisemitism-staging", "0.0.0", None, Arc::new(MemStore::default())).is_ok()); }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p aprender-mcp-builtin --lib server`
Expected: compile error.

- [ ] **Step 3: Implement `tools.rs` (classify, status), `server.rs`, `tests/rpc.rs`**

Port the 033 harness handlers (`references/lazy-load-runtime.md` §1 and `harness/.../main.rs:135-309`): `SimpleTool::new(name, move |args, extra| -> ToolFuture { .. })` with `SimpleToolExt::with_schema_from`. The identity map in `status` reads `extra.auth_context` only (never `extra.metadata`). The `pmcp 2.19` stateless server fills `auth_context` from `x-pmcp-user-id`, `x-pmcp-user-email`, `x-pmcp-user-name`, `x-pmcp-user-groups` request headers, which is what `rpc::call` sends. `rpc::call` uses a `reqwest::Client` as `aprender-mcp-decide-lambda/src/tests.rs:87` does.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p aprender-mcp-builtin --lib server`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy -p aprender-mcp-builtin --lib --tests -- -D warnings
git add crates/aprender-mcp-builtin && git commit -m "feat(builtin): classify and status tools over the lazy load; server::build from the configuration"
```

---

### Task 6: `verify` and its door

**Files:**
- Modify: `src/tools.rs`, `src/server.rs` (register `verify`; wire `status_required_role`), `src/tests/server.rs`, `src/tests/mod.rs`
- Create: `src/tests/verify.rs`

**Interfaces:**
- `pub fn verify_allowed(extra: &pmcp::RequestHandlerExtra, role: &str, allowed_principals: &[String]) -> bool` (spec §4.6 order).
- `pub fn permission_denied(role: &str) -> pmcp::Error` = `pmcp::Error::internal(format!("Permission denied: verify requires '{role}' role"))`.
- `#[derive(Deserialize, JsonSchema)] #[serde(deny_unknown_fields)] pub struct VerifyArgs { pub rows: Option<u32>, pub seed: Option<u64>, pub write_record: Option<bool> }` (defaults: `config.default_rows()`, `0`, `true`).
- `pub fn select_rows(n_eval: usize, rows: usize, seed: u64) -> Vec<usize>`: indices `0..n_eval` shuffled by `rand_chacha::ChaCha8Rng::seed_from_u64(seed)` with `rand::seq::SliceRandom::shuffle`, the first `rows`, then sorted ascending.
- `pub struct EvalRow { pub text: String, pub label: String }` and `pub fn read_eval_rows(bytes: &[u8]) -> Result<Vec<EvalRow>, String>`: one JSON object per line, lines trimmed of `\r`, blank lines skipped, error names the 1-based line.
- `pub fn eval_to_data_rows(rows: &[EvalRow], labels: &[String]) -> Result<Vec<aprender_decide::verify::DataRow>, String>` (label → index in `labels`; an unknown label is an error naming it), so `aprender_decide::verify::validate_probs(ProbsWhich::FineTuned, probs_bytes, &data_rows, labels)` is the one probs reader (text sha and shape checks included).
- `pub fn max_abs_diff(a: &[f32], b: &[f32]) -> f64` (NaN-visible: a NaN on either side yields `f64::NAN`, which fails `<= tol`).
- `tools::verify_tool(rt: Runtime) -> impl ToolHandler + 'static` (name `verify`, description `Re-score frozen evaluation rows against the training run's stored probabilities. Requires the '<role>' role.`); body, in order: door (`permission_denied`) → `VerifyArgs` parse (a serde failure is `pmcp::Error::validation`) → `rows` ∈ 1..=`ClassifyLimits::CONTRACTED.max_texts` else `tool_rejected`-style in-band error text `verify rows must be 1..=8 (got N)` → read `eval_jsonl_uri` and `eval_probs_uri` through `rt.store.get(uri, EVAL_CAP)` (absent/over cap or a store failure → `pmcp::Error::internal("verify: <uri> absent or over 33554432 bytes")` / the store string) → `read_eval_rows`, `eval_to_data_rows`, `validate_probs` → `rows > eval.len()` → in-band error `verify rows N exceeds the eval set's M rows` → load (as classify) → `select_rows` → `ClassifyArgs { texts }` through `parse_args(json)` so the door's bounds apply (its refusal is returned as-is, in-band) → `service.call` → per selected row `max_abs_diff(result.probabilities, probs[row])`, overall max → record JSON `{schema: "aprender-verification/v1", server, environment, artifact_sha256, model_id, eval_jsonl_sha256, eval_probs_sha256, rows_scored, row_indices, max_abs_diff, tolerance_abs, pass, verified_at (RFC 3339 UTC, seconds), verified_by (auth_context.email() else subject), build_id}` → if `write_record` and `verification_prefix` is set: key `{prefix}{YYYYMMDDTHHMMSSZ}-{sha12}.json`, `store.put_if_absent` → response `{record, written: bool, record_uri: Option<String>, reason: Option<String>}` where `reason` is `"no verification_prefix configured"`, `"write_record = false"`, `"record already exists"` or a store failure string.
- `server::build_with` registers `verify`, and `status` applies the door when `status_required_role` is set.

- [ ] **Step 1: Write the failing unit tests** (`src/tests/verify.rs`)

```rust
fn extra_with(subject: Option<&str>, groups: &[&str]) -> RequestHandlerExtra {
    // pmcp 2.19.3: RequestHandlerExtra::new(request_id, CancellationToken).with_auth_context(..);
    // AuthContext::new(subject) with the public `claims` map; groups() reads the `groups` claim.
    let ctx = subject.map(|s| { let mut c = pmcp::server::auth::AuthContext::new(s); c.claims.insert("groups".into(), serde_json::json!(groups)); c });
    RequestHandlerExtra::new("req-1".into(), tokio_util::sync::CancellationToken::new()).with_auth_context(ctx)
}
#[test] fn door_denies_without_identity() { assert!(!verify_allowed(&extra_with(None, &[]), "admin", &["m2m".into()])); }
#[test] fn door_admits_the_role_group() { assert!(verify_allowed(&extra_with(Some("sub"), &["analysts", "admin"]), "admin", &[])); assert!(!verify_allowed(&extra_with(Some("sub"), &["analysts"]), "admin", &[])); }
#[test] fn door_admits_an_allow_listed_principal_without_groups() { assert!(verify_allowed(&extra_with(Some("3k9f2m2mclientid"), &[]), "admin", &["3k9f2m2mclientid".into()])); assert!(!verify_allowed(&extra_with(Some("3k9f2m2mclientid-x"), &[]), "admin", &["3k9f2m2mclientid".into()]), "exact equality only"); }
#[test] fn denial_text_names_the_configured_role() { assert_eq!(permission_denied("decider-admin").to_string().contains("Permission denied: verify requires 'decider-admin' role"), true); }
#[test] fn select_rows_is_deterministic_sorted_and_bounded() { let a = select_rows(2129, 8, 0); let b = select_rows(2129, 8, 0); assert_eq!(a, b); assert_eq!(a.len(), 8); assert!(a.windows(2).all(|w| w[0] < w[1])); assert!(a.iter().all(|&i| i < 2129)); assert_ne!(select_rows(2129, 8, 1), a); assert_eq!(select_rows(3, 3, 7), vec![0, 1, 2]); }
#[test] fn eval_rows_skip_blank_lines_and_strip_cr() { let rows = read_eval_rows(b"{\"text\":\"a\",\"label\":\"x\"}\r\n\n{\"text\":\"b\",\"label\":\"y\"}\n\n").expect("rows"); assert_eq!(rows.len(), 2); assert_eq!(rows[0].text, "a"); let e = read_eval_rows(b"{\"text\":\"a\",\"label\":\"x\"}\nnot json\n").expect_err("line 2"); assert!(e.contains("line 2"), "{e}"); }
#[test] fn max_abs_diff_is_nan_visible() { assert_eq!(max_abs_diff(&[0.5, 0.5], &[0.5, 0.4]), 0.10000000149011612_f64.min(0.1 + 1e-7)); assert!(max_abs_diff(&[f32::NAN, 0.5], &[0.5, 0.5]).is_nan()); assert!(max_abs_diff(&[0.5], &[0.5, 0.5]).is_nan(), "length mismatch is NaN"); }
```

(For `max_abs_diff` use `assert!((max_abs_diff(&[0.5, 0.5], &[0.5, 0.4]) - 0.1).abs() < 1e-6)` rather than the exact f32 literal.)

- [ ] **Step 2: Write the failing end-to-end tests** (append to `src/tests/server.rs`)

```rust
fn store_with_tiny_records(text: &str) -> Arc<MemStore> { let s = MemStore::default(); let dir = fixture::dir(); let cfg = config::parse(text, cases::SERVER_ID).expect("parses"); s.put(&cfg.records.eval_jsonl_uri, std::fs::read(dir.join("data/eval.jsonl")).expect("eval")); s.put(&cfg.records.eval_probs_uri, std::fs::read(dir.join("eval-probs.json")).expect("probs")); Arc::new(s) }
#[tokio::test] async fn verify_passes_on_the_tiny_fixture_and_writes_a_record_once() { let (_d, path, sha) = fixture::tiny_apr(); let text = cases::staging_config().replace(cases::A, &sha).replace("labels = [\"not_antisemitic\", \"antisemitic\"]", &format!("labels = {:?}", fixture::tiny_labels())); let store = store_with_tiny_records(&text); let (rt, _) = runtime_over_text(&text, &path, &sha, store.clone()); let (url, _h) = rpc::serve(server::build_with(rt, "antisemitism-staging", "0.0.0").expect("server")).await; let admin = [("x-pmcp-user-id", "sub-1"), ("x-pmcp-user-groups", "admin"), ("x-pmcp-user-email", "ops@example.org")]; let r = rpc::tool(&url, "verify", json!({"rows": 3, "seed": 0}), &admin).await; let v: serde_json::Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{r}"))).expect("json"); assert_eq!(v["record"]["schema"], "aprender-verification/v1"); assert_eq!(v["record"]["pass"], true, "{v}"); assert_eq!(v["record"]["rows_scored"], 3); assert_eq!(v["record"]["tolerance_abs"], 5.0e-5); assert_eq!(v["record"]["verified_by"], "ops@example.org"); assert_eq!(v["record"]["artifact_sha256"], sha); assert_eq!(v["written"], true); let uri = v["record_uri"].as_str().expect("uri").to_string(); assert!(uri.starts_with("s3://aprender-laya-train-dev-123456789012/runs/antisemitism-v2/task-01jbx/verifications/") && uri.ends_with(&format!("-{}.json", &sha[..12])), "{uri}"); let stored: serde_json::Value = serde_json::from_slice(&store.get_sync(&uri).expect("stored")).expect("json"); assert_eq!(stored["pass"], true); assert_eq!(stored["environment"], "staging"); }
#[tokio::test] async fn verify_without_identity_is_the_documented_json_rpc_error() { let (_d, path, sha) = fixture::tiny_apr(); let text = cases::staging_config().replace(cases::A, &sha); let (rt, _) = runtime_over_text(&text, &path, &sha, Arc::new(MemStore::default())); let (url, _h) = rpc::serve(server::build_with(rt.clone(), "antisemitism-staging", "0.0.0").expect("server")).await; let resp = rpc::call(&url, "tools/call", json!({"name": "verify", "arguments": {"rows": 8, "seed": 0}}), &[]).await; let err = &resp["error"]; assert!(!err.is_null(), "a JSON-RPC error, not a result: {resp}"); assert_ne!(err["code"], -32602); assert!(err["message"].as_str().expect("msg").contains("Permission denied: verify requires 'admin' role"), "{err}"); assert!(rt.cell.get().is_none(), "a denied verify never loads"); }
/// The tiny fixture's eval set has fewer than 8 rows (read the count from the file, never assume it).
#[tokio::test] async fn verify_refuses_rows_over_eval_len() { let (_d, path, sha) = fixture::tiny_apr(); let text = tiny_text(&sha); let store = store_with_tiny_records(&text); let n = tools::read_eval_rows(&std::fs::read(fixture::dir().join("data/eval.jsonl")).expect("eval")).expect("rows").len(); assert!(n < 8, "the fixture has {n} rows; pick rows > n"); let (rt, _) = runtime_over_text(&text, &path, &sha, store); let (url, _h) = rpc::serve(server::build_with(rt.clone(), "antisemitism-staging", "0.0.0").expect("server")).await; let r = rpc::tool(&url, "verify", json!({"rows": 8, "seed": 0}), &ADMIN).await; assert_eq!(r["isError"], true, "{r}"); let msg = r["content"][0]["text"].as_str().expect("text"); assert!(msg.contains(&format!("verify rows 8 exceeds the eval set's {n} rows")), "{msg}"); assert!(rt.cell.get().is_none(), "refused before the load"); }
/// One eval row whose text is over classify_max_text_bytes; the probs file names that row's sha.
#[tokio::test] async fn verify_surfaces_classify_bound_refusal() { let (_d, path, sha) = fixture::tiny_apr(); let text = tiny_text(&sha); let cfg = config::parse(&text, cases::SERVER_ID).expect("parses"); let long = "x".repeat(ClassifyLimits::CONTRACTED.max_text_bytes + 1); let store = Arc::new(MemStore::default()); store.put(&cfg.records.eval_jsonl_uri, format!("{{\"text\":{:?},\"label\":\"shipping\"}}\n", long).into_bytes()); store.put(&cfg.records.eval_probs_uri, serde_json::to_vec(&json!({"labels": fixture::tiny_labels(), "rows": [{"row": 0, "text_sha256": aprender_mcp_decide_lambda::sha256_hex(long.as_bytes()), "probabilities": [0.34, 0.33, 0.33]}]})).expect("json")); let (rt, _) = runtime_over_text(&text, &path, &sha, store); let (url, _h) = rpc::serve(server::build_with(rt, "antisemitism-staging", "0.0.0").expect("server")).await; let r = rpc::tool(&url, "verify", json!({"rows": 1, "seed": 0}), &ADMIN).await; assert_eq!(r["isError"], true, "{r}"); assert!(r["content"][0]["text"].as_str().expect("text").contains("classify_max_text_bytes"), "{r}"); }
#[tokio::test] async fn verify_write_record_false_returns_the_record_unwritten() { let (_d, path, sha) = fixture::tiny_apr(); let text = tiny_text(&sha); let store = store_with_tiny_records(&text); let prefix = config::parse(&text, cases::SERVER_ID).expect("parses").records.verification_prefix.expect("prefix"); let (rt, _) = runtime_over_text(&text, &path, &sha, store.clone()); let (url, _h) = rpc::serve(server::build_with(rt, "antisemitism-staging", "0.0.0").expect("server")).await; let r = rpc::tool(&url, "verify", json!({"rows": 2, "seed": 0, "write_record": false}), &ADMIN).await; let v: serde_json::Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap_or_else(|| panic!("{r}"))).expect("json"); assert_eq!(v["record"]["rows_scored"], 2); assert_eq!(v["written"], false); assert_eq!(v["reason"], "write_record = false"); assert!(v["record_uri"].is_null()); assert!(store.keys_under(&prefix).is_empty()); }
```

Helpers shared by these tests (same file): `const ADMIN: [(&str, &str); 3] = [("x-pmcp-user-id", "sub-1"), ("x-pmcp-user-groups", "admin"), ("x-pmcp-user-email", "ops@example.org")];` and `fn tiny_text(sha: &str) -> String` = the staging example with `A` replaced by `sha` and the labels line replaced by the fixture's labels (as the pass test builds it).

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p aprender-mcp-builtin --lib verify`
Expected: compile error.

- [ ] **Step 4: Implement `verify` in `tools.rs`, register it, wire `status_required_role`**

As the Interfaces block. `verified_at` from `chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)`; the key stamp from `Utc::now().format("%Y%m%dT%H%M%SZ")`. The eval files are read **before** the load so a missing record never costs a cold load. Fixture helper `fixture::dir() -> PathBuf` (the `laya_tiny` directory) is added to `tests/fixture.rs`; `runtime_over_text` is `runtime_over` taking the text.

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p aprender-mcp-builtin --lib`
Expected: all PASS.

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all && cargo clippy -p aprender-mcp-builtin --lib --tests -- -D warnings
git add crates/aprender-mcp-builtin && git commit -m "feat(builtin): verify tool with the auth_context door, seeded rows, the create-only verification record"
```

---

### Task 7: `main_with`, argv dispatch, exit codes

**Files:**
- Modify: `src/lib.rs`, `src/tests/mod.rs`
- Create: `src/tests/dispatch.rs`

**Interfaces:**
- `pub enum Mode { Serve, ServeLocal, Precheck, Usage }` and `pub fn dispatch(args: &[String]) -> Mode` (`[_]` → Serve; `[_, "--serve-local"]` → ServeLocal; `[_, "precheck"]` → Precheck; anything else → Usage).
- `pub const EXIT_USAGE: i32 = 64;`
- `pub async fn run_precheck(toml_text: &str, expected_id: &str, env: &precheck::Env, store: &dyn ObjectStore, out: &mut dyn std::io::Write) -> i32` (writes `outcome.render()`, returns `outcome.exit`).
- `pub type ServeFuture = Pin<Box<dyn Future<Output = Result<(), lambda_http::Error>> + Send + 'static>>;`
- `pub async fn main_with(args: Vec<String>, toml_text: &'static str, expected_id: &str, serve: impl FnOnce(pmcp::Server) -> ServeFuture) -> Result<(), lambda_http::Error>`: `Usage` → `eprintln!("usage: bootstrap [--serve-local | precheck]")`, `std::process::exit(EXIT_USAGE)`; `Precheck` → `S3Store::from_default_config().await`, `Env::from_env()`, `run_precheck(.., &mut std::io::stdout())`, flush, `std::process::exit(code)`; `Serve`/`ServeLocal` → `config::parse` (an error becomes `lambda_http::Error::from(e.to_string())`), `server::build(toml_text, config, expected_id, env!("CARGO_PKG_VERSION"), std::env::var("PMCP_BUILD_ID").ok().filter(|s| !s.is_empty()), Arc::new(S3Store::from_default_config().await))`, then `serve(server).await`.
- `lib.rs` re-exports: `pub use lambda_http; pub use pmcp;`.

- [ ] **Step 1: Write the failing tests** (`src/tests/dispatch.rs`)

```rust
fn argv(a: &[&str]) -> Vec<String> { a.iter().map(|s| s.to_string()).collect() }
#[test] fn dispatch_table() { assert!(matches!(dispatch(&argv(&["bootstrap"])), Mode::Serve)); assert!(matches!(dispatch(&argv(&["bootstrap", "--serve-local"])), Mode::ServeLocal)); assert!(matches!(dispatch(&argv(&["bootstrap", "precheck"])), Mode::Precheck)); assert!(matches!(dispatch(&argv(&["bootstrap", "--help"])), Mode::Usage)); assert!(matches!(dispatch(&argv(&["bootstrap", "precheck", "--case", "P1"])), Mode::Usage)); assert!(matches!(dispatch(&argv(&[])), Mode::Usage)); assert_eq!(EXIT_USAGE, 64); }
#[tokio::test] async fn run_precheck_writes_the_grammar_and_returns_the_exit() { let mut out = Vec::new(); let code = run_precheck(&cases::staging_config().replace("[model]\n", "[model]\nmemory_mb = 10240\n"), cases::SERVER_ID, &Env::default(), &MemStore::default(), &mut out).await; assert_eq!(code, 3); let s = String::from_utf8(out).expect("utf8"); assert!(s.starts_with("ERROR: REFUSED config-invalid: ") && s.ends_with('\n') && s.lines().count() == 1, "{s}"); let case = cases::all().into_iter().find(|c| c.name == "P1").expect("P1"); let mut out = Vec::new(); assert_eq!(run_precheck(&case.config, case.expected_id, &case.env, &case.store, &mut out).await, 0); assert!(String::from_utf8(out).expect("utf8").starts_with("ELIGIBLE {\"config_sha256\":")); }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p aprender-mcp-builtin --lib dispatch`
Expected: compile error.

- [ ] **Step 3: Implement in `lib.rs`**

As the Interfaces block. The crate-level doc comment states the binary contract verbatim from Global Constraints.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p aprender-mcp-builtin --lib`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy -p aprender-mcp-builtin --lib --tests -- -D warnings
git add crates/aprender-mcp-builtin && git commit -m "feat(builtin): main_with entry point: serve, --serve-local, precheck, usage 64"
```

---

### Task 8: The local smoke (`examples/local_server.rs`, fixtures, `just builtin-smoke`)

**Files:**
- Create: `crates/aprender-mcp-builtin/examples/local_server.rs`, `crates/aprender-mcp-builtin/fixtures/smoke-valid.toml`, `crates/aprender-mcp-builtin/fixtures/smoke-invalid.toml`
- Modify: `justfile` (new recipes `builtin-check`, `builtin-smoke` next to the `laya-*` recipes)

**Interfaces:**
- `examples/local_server.rs` is the platform template's `main.rs` shape with the serve closure replaced by `aprender_mcp_decide_lambda::start_loopback(server, "127.0.0.1:8081")` for both modes (there is no Lambda here): `const CONFIG_TOML: &str = include_str!(env!("APRENDER_SMOKE_CONFIG"))` is not possible with `include_str!`, so the example reads the config path from `APRENDER_SMOKE_CONFIG` at runtime (`std::fs::read_to_string`), leaks it to `&'static str`, and calls `main_with(args, text, "aprender-seed", closure)`.
- `fixtures/smoke-valid.toml`: `id = "aprender-seed"`, staging, the §3.1 staging example with the bucket names the platform's smoke uses (`aprender-decide-weights-000000000000-dev`, `aprender-laya-train-dev-000000000000`), a 64-hex sha; header comment: compile and offline serving only, the objects do not exist. `smoke-invalid.toml`: the same plus `memory_mb = 10240` under `[model]`.
- `just builtin-check`: `cargo fmt --all -- --check`, `cargo clippy -p aprender-mcp-builtin --lib --tests --examples -- -D warnings`, `cargo test -p aprender-mcp-builtin --lib`.
- `just builtin-smoke` (bash, `set -euo pipefail`): build the example (`cargo build --release -p aprender-mcp-builtin --example local_server`); (1) `APRENDER_SMOKE_CONFIG=<invalid> target/release/examples/local_server precheck`; pass only if the exit status is 3 and stdout's single line starts with `ERROR: REFUSED config-invalid:` and mentions `memory_mb` (capture `rc=$?` directly, never through a pipe); (2) with the valid config, start `local_server --serve-local` in the background (trap kills it), wait for the port, POST `initialize`, `tools/list`, `tools/call status`, `tools/call verify {"rows":8,"seed":0}` to `http://127.0.0.1:8081/mcp` with `Content-Type: application/json` and `Accept: application/json, text/event-stream`; pass only if `tools/list` names `classify`, `status` and `verify`, `status` reports `"loaded":false`, and the `verify` response is a JSON-RPC `error` whose `message` contains `Permission denied: verify requires 'admin' role` and whose `code` is not `-32602`; print `PASS builtin-smoke` last.

- [ ] **Step 1: Write the fixtures and the example**

- [ ] **Step 2: Run the example's precheck by hand to see it fail closed**

Run: `APRENDER_SMOKE_CONFIG=crates/aprender-mcp-builtin/fixtures/smoke-invalid.toml cargo run --release -p aprender-mcp-builtin --example local_server -- precheck; echo "rc=$?"`
Expected: one line `ERROR: REFUSED config-invalid: ... memory_mb ...`, `rc=3`.

- [ ] **Step 3: Write the two recipes**

- [ ] **Step 4: Run them**

Run: `just builtin-check && just builtin-smoke`
Expected: both exit 0; the last line of the second is `PASS builtin-smoke`.

- [ ] **Step 5: Commit**

```bash
git add crates/aprender-mcp-builtin justfile
git commit -m "feat(builtin): local smoke mirroring the platform's aprender-local-smoke (precheck grammar, serve-local, verify door)"
```

---

### Task 9: Fork hygiene the platform asked for

**Files:**
- Modify: `crates/aprender-core/build.rs:150-160`, `crates/aprender-compute/build.rs:60-70`, `crates/aprender-compute/src/brick/quant_ops/mod.rs:219,318`

- [ ] **Step 1: Gate the `rerun-if-changed` lines on the file's existence**

In both build scripts, emit `cargo:rerun-if-changed=<binding_path>` only when `binding_path.exists()`; otherwise emit nothing for that path (the `cargo:warning` note stays). Keep the `contracts/model-families` line in `aprender-core/build.rs:331` as it is (it exists in-tree).

- [ ] **Step 2: Rename the two unused parameters to `_backend`**

- [ ] **Step 3: Verify warning-free on 1.97.1 and no spurious rebuild**

Run: `cargo +1.97.1 check -p aprender-mcp-builtin -p aprender-compute -p aprender-core --lib 2>&1 | grep -c '^warning: unused'`
Expected: `0`.
Run: `cargo check -p aprender-compute --lib && cargo check -p aprender-compute --lib 2>&1 | tail -1`
Expected: the second run prints only `Finished` (no `Compiling aprender-compute`).

- [ ] **Step 4: Run the affected crates' lib tests**

Run: `cargo test -p aprender-compute --lib && cargo test -p aprender-core --lib format::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/aprender-core/build.rs crates/aprender-compute/build.rs crates/aprender-compute/src/brick/quant_ops/mod.rs
git commit -m "build: gate the binding.yaml rerun-if-changed on existence; silence the two unused backend params (platform ask)"
```

---

### Task 10: The contract file and the repository gates

**Files:**
- Create: `contracts/aprender-builtin-v1.yaml`
- Modify: `README.md` (via `make readme-sync`), `CLAUDE.md` (one sentence in the Decision-model row naming the crate), `docs/superpowers/specs/2026-11-03-aprender-built-in-server-type-design.md` (status line: built on this side at this commit)

- [ ] **Step 1: Write the contract**

Mirror `contracts/laya-parity-v1.yaml`'s skeleton (`contract: aprender-builtin`, `metadata` with `version: 1.0.0`, `created: '2026-11-03'`, `author`, `kind: kernel`, `valid_under: { world: committed }`, `description`), then `constants` (the caps and exit codes of Global Constraints, each with its Rust name), `equations` (one per rule family: config grammar, precheck order, verify door order, record key), `proof_obligations`, `falsification_tests` FALSIFY-BUILTIN-001..006 each naming its `cargo test -p aprender-mcp-builtin --lib <test>` (the case table, the empty-env test, the door tests, the grammar test, the verify pass test, the dispatch table), `qa_gate`. A `constants` test in `src/tests/contract.rs` asserts `ARTIFACT_CAP`, `RECORD_CAP`, `EVAL_CAP` and the exit codes EQUAL the file's values (parse with `serde_yaml`, a workspace dev-dependency, as `aprender-mcp-decide-lambda` does).

- [ ] **Step 2: Validate it**

Run: `pv validate contracts/aprender-builtin-v1.yaml && pv lint contracts/aprender-builtin-v1.yaml`
Expected: valid; lint without errors.

- [ ] **Step 3: Regenerate the derived counts and run the doc gates**

Run: `make contracts && make readme-sync && bash scripts/check_readme_claims.sh`
Expected: exit 0 (the crate count ratchets 93 → 94, the contract count follows the census).
Run: `cargo test -p aprender-core --test readme_contract && cargo test -p aprender-core --test monorepo_invariants`
Expected: PASS.

- [ ] **Step 4: The whole-crate gate once more**

Run: `just builtin-check && just builtin-smoke && cargo test -p aprender-mcp-decide-lambda --lib thin_server_crates_are_shims`
Expected: PASS everywhere.

- [ ] **Step 5: Commit**

```bash
git add contracts/aprender-builtin-v1.yaml contracts/census.json README.md CLAUDE.md docs/superpowers/specs/2026-11-03-aprender-built-in-server-type-design.md crates/aprender-mcp-builtin
git commit -m "contracts(builtin): aprender-builtin-v1 with the grammar, refusal names and record schemas; README counts regenerated"
```

---

## Handover (not a task: the owner's step)

The revision the platform pins is the head of `gsd/phase-2-contract-gate` after Task 10, pushed by the owner (`git push origin gsd/phase-2-contract-gate`). The line for the platform's plan 209-07 resume is `rev <40-hex sha>` followed by §2 of `aprender-team-reply-core-crate-rev-2026-11-03.md`. The platform's three acceptance checks and `just aprender-local-smoke` then run on their side; any observed line that differs from the reply becomes a test here before the next revision.
