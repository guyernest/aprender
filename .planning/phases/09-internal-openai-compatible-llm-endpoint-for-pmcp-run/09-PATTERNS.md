# Phase 9: Internal OpenAI-Compatible LLM Endpoint for pmcp.run - Pattern Map

**Mapped:** 2026-10-15
**Files analyzed:** 19 (14 in this repo, 2 in `~/Development/mcp/sdk/pmcp-run`, 1 in `~/Development/mcp/sdk/rust-mcp-sdk`, 2 phase records)
**Analogs found:** 17 / 19 (2 are data/record files with a shape precedent only)

Every analog path below was checked with `git ls-files` in its own repo (tracked source, no gitignored mirrors). Line numbers are from the working tree at `gsd/phase-2-contract-gate` (this repo), `d0348451` (rust-mcp-sdk) and `a27d46b8e` (pmcp-run) on 2026-10-15.

Two corrections the planner must carry (both from RESEARCH.md, confirmed by reading the files):
1. **D-04's fix site is `crates/apr-cli/src/commands/serve/server.rs::serve_router` (lines 279-290)**, not `routes.rs::create_router`. `routes.rs` is the SafeTensors inspection router and is already gated.
2. **D-03's preferred edit keeps `content: String`** and makes only the deserializer tolerant (`#[serde(default, deserialize_with = ...)]`), which is exactly how `temperature` is already guarded two fields up in the same struct (`mod_create_demo.rs:70-80`). Byte-identical serialization (PMAT-801) then holds by construction.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/apr-cli/src/commands/serve/server.rs` (modify: `build_gguf_router` seam + wire `AuthGate::from_env()`) | route/router builder | request-response | `crates/apr-cli/src/commands/serve/handlers.rs::build_apr_cpu_router` (1594-1612, call site 1502) + `auth.rs::layer` (217-225) | exact |
| `crates/apr-cli/src/commands/serve/server.rs` `#[cfg(test)]` (new: `gguf_router_refuses_without_bearer`) | test | request-response (tower `oneshot`) | `crates/apr-cli/src/commands/serve/auth.rs` tests `public_ops` (302-360); `crates/apr-cli/tests/falsify_auth_001.rs` (28-70) | exact |
| `crates/aprender-serve/src/api/mod_create_demo.rs` (modify: `ChatMessage.content` deserializer, `max_tokens` alias) | model (wire struct) | transform (serde) | same file, `ChatCompletionRequest.temperature` (70-80, `deserialize_with = "crate::api::types::deserialize_temperature_f32"`) | exact |
| `crates/aprender-serve/src/api/tests/<new>.rs` (new: parse tests for D-03, alias, verdict-JSON reader) | test | transform (serde, no model) | `crates/aprender-serve/src/api/qwen35_chat_backend_tests.rs::chat_template_kwargs_parse_and_refuse_unknown_keys_3723` (769-806) | exact |
| `contracts/llm-endpoint-openai-conformance-v1.yaml` (new) | config (provable contract) | batch (pv validate + cargo-filter test binding) | `contracts/decide-tool-boundary-v1.yaml` (kind: kernel, `valid_under`, `falsification_tests[]`) with the auth-contract's `qa_gate.run` shape (`contracts/apr-serve-api-key-auth-v1.yaml:95-135`) | exact |
| `ci/explicit-test-commands.d/NNN-<slug>.cmd` (new, one line) | config (CI binding) | batch | `ci/explicit-test-commands.d/250-aprender-contracts-apr-serve-api-key-auth-contract.cmd` | exact |
| `deploy/llm-endpoint/Dockerfile` (new) | config (container image) | file-I/O | `.claude/skills/spike-findings-aprender/sources/031-openai-endpoint-aws-hosts/deploy/Dockerfile` | exact (hardening only) |
| `deploy/llm-endpoint/entrypoint.sh` (new) | utility (process entry) | file-I/O + exec | `.../031-openai-endpoint-aws-hosts/deploy/entrypoint.sh` + the `refuse()` ladder of `justfile::laya-deploy` (1796-1810) | exact |
| `deploy/llm-endpoint/pin.env` (new) | config | — | `crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template` (tracked template with the pin, generated file gitignored) | role-match |
| `deploy/llm-endpoint/probe/probe_openai.py`, `shapes.py`, `bench.py` (new, copied) | test harness | request-response (HTTP / lambda:Invoke) | `.../029-apr-serve-openai-tool-loop/tools/probe_openai.py`, `auth_probe.sh`; `.../031-.../tools/remote_bench.py`, `shapes.py` | exact (copy + KNOWN LIMITATION verdict) |
| `justfile` `llm-endpoint-image` / `-weights` / `-up` (new recipes) | utility (deploy recipe) | file-I/O + AWS CLI | `justfile::laya-weights-bucket` (2354-2382); `.../031/tools/build_image.sh`, `fetch_weights_codebuild.sh`, `aws_up.sh` | exact |
| `justfile` `llm-endpoint-deploy` (new, `DRY_RUN=1`) | utility (deploy recipe, fail-closed) | AWS CLI | `justfile::laya-deploy` (1796-1880) | exact |
| `justfile` `llm-endpoint-prove-401` / `-url` / `-probe` / `-measure` (new) | utility | request-response (lambda:Invoke, HTTPS) | `justfile::laya-deploy-verify` (2599); `.../031/tools/remote_bench.py`; `.../029/tools/auth_probe.sh` | role-match |
| `justfile` `llm-endpoint-teardown` (new) | utility | AWS CLI | `justfile::laya-teardown` (2742-2790) + `.../031/tools/teardown.sh` | exact |
| `.gitignore` (modify: `/deploy/llm-endpoint/bin/`, `/deploy/llm-endpoint/.ids/`) | config | — | `.gitignore:70-72` (`/deploy/node_modules/` etc., root-anchored) | exact |
| `.planning/phases/09-*/09-LIVE-DEPLOY-EVIDENCE.json` (new record) | config (evidence) | — | `.planning/phases/08-laya-.../08-LIVE-REDEPLOY-EVIDENCE.json` (key shape) | role-match |
| `.planning/phases/09-*/09-PMCP-RUN-BRIEF.md` (new) | doc | — | no analog (first cross-team brief); shape dictated by D-11 | none |
| **[sibling repo]** `~/Development/mcp/sdk/rust-mcp-sdk/crates/pmcp-agent/src/sources/openai_compat.rs` (modify: `message_to_openai` ToolUse arm + test) | service (wire transform) | transform | same file: `ToolResult` arm (184-190) writes `content`; test `tool_use_message_becomes_tool_calls` (364-382) | exact |
| **[sibling repo]** `~/Development/mcp/sdk/pmcp-run/amplify/functions/durable-agent-lambda/src/config/loader.rs` (modify: optional `timeoutSecs` in `map_provider_row_to_config`) + `handler/iteration.rs` tests | service (config loader) | CRUD (DDB row → struct) | same function: `enabled`/`authHeaderPrefix` optional-field reads (925-944); invariant tests `iteration.rs:2949-2984` | exact |
| **[sibling repo, data only]** pmcp.run `LLMProvider` + `LLMModel` rows (no code) | data | — | `amplify/data/resource.ts:1450-1470` (field list) | n/a (D-08: row, not code) |

## Pattern Assignments

### `crates/apr-cli/src/commands/serve/server.rs` — `build_gguf_router` seam (route builder, request-response)

**Analog:** `crates/apr-cli/src/commands/serve/handlers.rs` (`build_apr_cpu_router`, the gated precedent) and `auth.rs::layer`.

**The ungated site today** (`server.rs:279-290`):
```rust
#[cfg(feature = "inference")]
fn serve_router(state: realizar::api::AppState, config: &ServerConfig) -> Result<()> {
    use realizar::api::create_router_with_config;
    // ... #3991: read from the STATE the router is built with, before it moves.
    let endpoints = realizar::api::advertised_routes(&config.router_config(), &state);
    // Create realizar's full inference router (Ollama-parity endpoints).
    let app = create_router_with_config(state, config.router_config());   // <-- no AuthGate
```
Reached via `handler_gpu_completion.rs:366 start_gguf_server` → `server.rs:111 start_qwen35_server` → `serve_router`; non-hybrid GGUFs reach it via `run_cpu_server` (67-98).

**Gated precedent to copy — signature + doc comment** (`handlers.rs:1588-1594`):
```rust
/// HELIX-IDEA-009 / FALSIFY-AUTH-001: every route on the returned router
/// is gated by `auth_gate` via `super::auth::layer`. Pass
/// `AuthGate::disabled()` to bypass.
#[cfg(feature = "inference")]
#[allow(clippy::disallowed_methods)] // serde_json::json!() macro uses infallible unwrap
fn build_apr_cpu_router(state: AprServerState, auth_gate: super::auth::AuthGate) -> axum::Router {
```
**Production call site** (`handlers.rs:1502`):
```rust
let app = build_apr_cpu_router(state, super::auth::AuthGate::from_env());
```
**The generic layer** (`auth.rs:217-225`) — state-type agnostic, so it wraps realizar's router unchanged:
```rust
pub fn layer<S>(gate: AuthGate, router: axum::Router<S>) -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.layer(axum::middleware::from_fn_with_state(
        std::sync::Arc::new(gate),
        apply,
    ))
}
```
**What `from_env` does when the var is missing** (`auth.rs:49-70`) — the reason the entrypoint and the deploy recipe must refuse a missing `APR_API_KEY_HASH` (the server itself only warns):
```rust
pub fn from_env() -> Self {
    if let Ok(hex) = std::env::var("APR_API_KEY_HASH") {
        match decode_hex_32(&hex) {
            Ok(bytes) => return Self::from_hash(bytes),
            Err(reason) => { eprintln!("[apr serve] APR_API_KEY_HASH set but {reason}; ignoring (auth disabled)"); return Self::disabled(); }
        }
    }
    if let Ok(plain) = std::env::var("APR_API_KEY") { if !plain.is_empty() { return Self::from_plain_key(&plain); } }
    eprintln!("[apr serve] WARNING: no APR_API_KEY or APR_API_KEY_HASH set; HTTP routes are unauthenticated");
    Self::disabled()
}
```
**Target shape** (from RESEARCH Pattern 1, consistent with both analogs):
```rust
#[cfg(feature = "inference")]
fn build_gguf_router(state: realizar::api::AppState, config: &ServerConfig, auth_gate: super::auth::AuthGate) -> axum::Router {
    let app = realizar::api::create_router_with_config(state, config.router_config());
    super::auth::layer(auth_gate, app)   // every route, /health included (FALSIFY-AUTH-001); layer_public_ops is the one-word alternative
}
// serve_router: let app = build_gguf_router(state, config, super::auth::AuthGate::from_env());
```
**/health decision:** use `layer` (gate all), per `contracts/apr-serve-api-key-auth-v1.yaml:103-104` ("Health/metrics endpoints are NOT exempt"). `auth.rs::layer_public_ops` (193-201) exists if a probe exemption is ever needed; its rationale (WR-07) is in the doc comment at 177-192.

**Error envelope emitted by the gate** (`auth.rs:161-168`) — this is the contracted `{"error": "<string>"}` shape; do NOT reshape it (KNOWN LIMITATION per D-05):
```rust
let body = axum::Json(serde_json::json!({
    "error": "unauthorized",
    "message": "Missing or invalid Authorization: Bearer <key> header"
}));
let mut resp = (StatusCode::UNAUTHORIZED, body).into_response();
resp.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
```

---

### `crates/apr-cli/src/commands/serve/server.rs` `#[cfg(test)]` — `gguf_router_refuses_without_bearer` (test, tower oneshot)

**Analog:** `crates/apr-cli/src/commands/serve/auth.rs` tests, module `public_ops` (302-360). Same file, same crate, drives the REAL middleware through a real router.

**Imports + fixture** (`auth.rs:309-321`):
```rust
mod public_ops {
    use super::super::{layer_public_ops, AuthGate};
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use tower::ServiceExt as _;

    fn gated_router() -> axum::Router {
        let router = axum::Router::new()
            .route("/v1/classify", axum::routing::post(|| async { "ok" }))
            .route("/health", axum::routing::get(|| async { "ok" }))
            .route("/health/ready", axum::routing::get(|| async { "ok" }));
        layer_public_ops(AuthGate::from_hash([7_u8; 32]), router)
    }
```
**Status helper + assertion style** (`auth.rs:323-345`):
```rust
    async fn status(method: Method, path: &str) -> StatusCode {
        let request = Request::builder().method(method).uri(path).body(Body::empty()).expect("request builds");
        gated_router().oneshot(request).await.expect("router answers").status()
    }

    #[tokio::test]
    async fn an_unauthenticated_classify_is_still_refused() {
        assert_eq!(status(Method::POST, "/v1/classify").await, StatusCode::UNAUTHORIZED,
            "the only route that runs a model must stay gated — ...");
    }
```
**Body-reading variant with the header** (`crates/apr-cli/tests/falsify_auth_001.rs:46-61`) — use this when asserting `body["error"] == "unauthorized"`:
```rust
async fn fire(method: &str, path: &str, header: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(value) = header { builder = builder.header(axum::http::header::AUTHORIZATION, value); }
    let req = builder.body(Body::empty()).unwrap();
    let resp = protected_router().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
    let body: Value = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) };
    (status, body)
}
```
(`falsify_auth_001.rs:15` opens with `#![allow(clippy::unwrap_used, clippy::disallowed_methods)]` — tests may; production code may not.)

**Model-free state:** `build_apr_cpu_router`'s own test seams construct a demo `AprServerState` (`handlers.rs:1695-1705`, `build_demo_streaming_apr_cpu_router_for_test` 1718-1738) and pass `AuthGate::disabled()`. For the GGUF router use realizar's test fixture (`create_test_app_shared()` as used by `crates/aprender-serve/src/api/tests/openai_chat.rs:4`) and `AuthGate::from_plain_key("k")`. Assert: `POST /v1/chat/completions` and `GET /health` with no header → 401 + `error == "unauthorized"` + `WWW-Authenticate: Bearer`; with `Bearer k` → not 401.

---

### `crates/aprender-serve/src/api/mod_create_demo.rs` — `ChatMessage.content` + `max_tokens` alias (wire struct, serde transform)

**Analog:** the same file's `ChatCompletionRequest.temperature` guard (lines 70-80) — a field-level `deserialize_with` that keeps the Rust type and changes only acceptance.

**Existing per-field deserializer precedent** (`mod_create_demo.rs:70-80`):
```rust
    /// Maximum tokens to generate
    #[serde(default)]
    pub max_tokens: Option<usize>,
    /// Sampling temperature.
    ///
    /// A temperature outside `[0, ∞)` finite is rejected at deserialization
    /// (aprender#2375) — see `types::deserialize_temperature_f32`. ...
    #[serde(default, deserialize_with = "crate::api::types::deserialize_temperature_f32")]
    pub temperature: Option<f32>,
```
**The field to change and the comment that promises it** (`mod_create_demo.rs:205-218`):
```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    /// PMAT-801: kept REQUIRED on deserialize (a request message missing
    /// `content` is rejected 422, preserving the pre-PMAT-801 contract). ...
    /// (Optional/null content for assistant tool-call history on the REQUEST side is a
    /// follow-up — it would require relaxing this field to Option.)
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ResponseToolCall>>,
```
**Edits** (RESEARCH Pattern 2; rewrite the PMAT-801 comment to say the follow-up landed):
```rust
#[serde(default, deserialize_with = "deserialize_null_as_empty_string")]
pub content: String,

fn deserialize_null_as_empty_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}
// and, on max_tokens:
#[serde(default, alias = "max_completion_tokens")]
pub max_tokens: Option<usize>,
```
Serialization is untouched, so the response side (`content: String::new()` on the assistant tool-call message) stays byte-identical. `tool_choice` mapping lives at 291-301 (`OpenAiToolChoice::to_grammar`: `"none"` → `ToolChoice::None`, `"required"` → `ToolChoice::Required`) — `required` is mapped but not enforced (KNOWN LIMITATION).

---

### `crates/aprender-serve/src/api/tests/<new>.rs` — parse tests (test, serde transform, no model)

**Analog:** `crates/aprender-serve/src/api/qwen35_chat_backend_tests.rs:769-806` (`chat_template_kwargs_parse_and_refuse_unknown_keys_3723`). Register the new file in `api/tests/mod.rs` as the others are.

**Pattern to copy verbatim** (769-786):
```rust
/// #3723: `chat_template_kwargs` carries only what apr honours; any other key is refused at
/// deserialization, never silently dropped.
#[test]
fn chat_template_kwargs_parse_and_refuse_unknown_keys_3723() {
    let parse = |v: serde_json::Value| serde_json::from_value::<ChatCompletionRequest>(v);
    let base = serde_json::json!({"model": "m", "messages": [{"role": "user", "content": "x"}]});
    let with = |k: &str, v: serde_json::Value| { let mut b = base.clone(); b[k] = v; b };
    assert_eq!(parse(base.clone()).expect("parses").thinking(), None);
    ...
    assert!(parse(with("chat_template_kwargs", serde_json::json!({"reasoning_effort": "high"}))).is_err(), "an unknown kwarg is refused");
}
```
**For D-03** the fixture is the exact request #3 body from `.claude/skills/spike-findings-aprender/sources/029-apr-serve-openai-tool-loop/results/wire-0.8b-plain.jsonl` (assistant message `{"role":"assistant","tool_calls":[...]}` with no `content`) plus a `content: null` variant; assert `messages[1].content == ""`. Add a round-trip: serialize a `ChatMessage { content: String::new(), tool_calls: Some(..) }` and assert the bytes equal the pre-change output (PMAT-801). For the alias: `{"max_completion_tokens": 12}` → `max_tokens == Some(12)`. Expected names (RESEARCH Validation Architecture): `assistant_message_without_content_parses_pmat801_followup`, `max_completion_tokens_aliases_max_tokens`, `conformance_record_matches_contract` (the verdict-JSON reader — see the contract section).

---

### `contracts/llm-endpoint-openai-conformance-v1.yaml` (contract, pv-validated)

**Analog:** `contracts/decide-tool-boundary-v1.yaml` for the header and `falsification_tests[]` row shape; `contracts/apr-serve-api-key-auth-v1.yaml` for `qa_gate.run` and the CI fragment binding.

**Header** (`decide-tool-boundary-v1.yaml:1-11`):
```yaml
contract: decide-tool-boundary
metadata:
  version: 9.0.0
  created: '2026-09-25'
  author: PAIML Engineering
  kind: kernel
  # ONT-7 (plan 08-33): the boundary holds in the committed tree. ...
  valid_under:
    world: committed
  description: >
```
**Falsification row — `test:` is a `cargo test` filter string that `scripts/check_contract_test_binding.sh` resolves** (`decide-tool-boundary-v1.yaml`, FALSIFY-DECIDE-TOOL-007):
```yaml
  - id: FALSIFY-DECIDE-TOOL-007
    rule: 'Every Rust boundary constant, and the deployed Lambda memory, equals this file'
    prediction: 'The contract-mirror test reads every bound key via test_support::constant_u64 ... changing one YAML value alone turns a test red naming the key.'
    implemented_by: 'plan 08-06 (contract-mirror test); the tier test by the v2.0.0 amendment (plan 08-17)'
    test: 'cargo test -p aprender-mcp-decide --lib tests::bounds_match_contract && cargo test -p aprender-mcp-decide-lambda --lib tests::deploy_memory_is_the_contract_tier'
    if_fails: 'A bound changed in exactly one place: the published boundary and the served one disagree, ...'
```
Rows proven live (FALSIFY-DECIDE-TOOL-009) name the `just` recipe in `implemented_by` and the evidence JSON by path — copy that for the probe rows: `implemented_by: 'just llm-endpoint-probe ...'`, `test:` = the Rust reader `conformance_record_matches_contract` over the committed verdict JSON, so PV-VER-002 resolves.

**`qa_gate` shape** (`apr-serve-api-key-auth-v1.yaml:126-135`):
```yaml
qa_gate:
  id: F-AUTH-GATE
  name: "apr serve API key auth contract"
  pass_criteria: "All 3 FALSIFY-AUTH-* tests listed above pass"
  run: "cargo test -p apr-cli --test falsify_auth_001 --test falsify_auth_002 --test falsify_auth_003 --no-fail-fast"
```
**CI binding** — one line per file, sorted ordinal, free number between neighbours (`ci/explicit-test-commands.d/250-aprender-contracts-apr-serve-api-key-auth-contract.cmd`):
```
cargo test -p aprender-contracts --test apr_serve_api_key_auth_contract
```
Highest today is `520-aprender-mcp-decide-e2e-stdio.cmd`; take e.g. `530-`. Never renumber.

**Binding gate rules** (`scripts/check_contract_test_binding.sh:36-53`): new contracts start at zero findings (not in the baseline ⇒ any finding fails); `MIN_REFS=250` vacuity floor; run `bash scripts/check_contract_test_binding.sh` after adding rows. Validate with the justfile's pv resolution (`justfile:3442`): `if command -v pv > /dev/null 2>&1; then PV=pv; else PV="cargo run --release -q -p aprender-contracts-cli --bin pv --"; fi` — never a bare `pv` (not on PATH here).

---

### `deploy/llm-endpoint/Dockerfile` (container image config)

**Analog:** `.claude/skills/spike-findings-aprender/sources/031-openai-endpoint-aws-hosts/deploy/Dockerfile` (tracked). Copy whole; harden per RESEARCH Q6.
```dockerfile
FROM public.ecr.aws/awsguru/aws-lambda-adapter:0.9.1 AS lwa
FROM public.ecr.aws/amazonlinux/amazonlinux:2023-minimal
RUN microdnf install -y awscli-2 procps-ng && microdnf clean all
COPY --from=lwa /lambda-adapter /opt/extensions/lambda-adapter
COPY apr /usr/local/bin/apr
COPY entrypoint.sh /entrypoint.sh
RUN chmod 0755 /usr/local/bin/apr /entrypoint.sh && /usr/local/bin/apr --version
ENV AWS_LWA_PORT=8080 AWS_LWA_READINESS_CHECK_PATH=/health AWS_LWA_READINESS_CHECK_PROTOCOL=http \
    AWS_LWA_ASYNC_INIT=true AWS_LWA_INVOKE_MODE=buffered PORT=8080 MODEL_LOCAL=/tmp/model.gguf \
    RAYON_NUM_THREADS=6 HOME=/tmp
EXPOSE 8080
ENTRYPOINT ["/entrypoint.sh"]
```
Changes: `ARG APR_TAG` + `LABEL` (tag, tarball sha256); pin the LWA image by digest as well as tag; no secret in the image (`APR_API_KEY_HASH` is a function env var). `/health` answering 401 still counts as ready for LWA 0.9.1 (any status 100-499).

---

### `deploy/llm-endpoint/entrypoint.sh` (process entry, file-I/O + exec)

**Analog:** `.../031-openai-endpoint-aws-hosts/deploy/entrypoint.sh` (timeline + cpu_part logging + S3 pull + `exec`), with the refusal idiom from `justfile::laya-deploy`.

**Keep** (entrypoint.sh lines 5-11, 13-23, 26-27):
```bash
set -uo pipefail
T0=$(date +%s%N)
ms() { echo $(( ($(date +%s%N) - T0) / 1000000 )); }
log() { echo "[entry] t=$(ms)ms $*"; }
log "start pid=$$ apr=$(/usr/local/bin/apr --version 2>&1 | head -1)"
log "host: nproc=$(nproc) threads=${RAYON_NUM_THREADS:-unset} cpu_part=$(awk -F: '/CPU part/{gsub(/ /,"",$2); print $2; exit}' /proc/cpuinfo 2>/dev/null) ..."
if [ -s "$MODEL_LOCAL" ]; then log "model already present bytes=$(stat -c %s "$MODEL_LOCAL")"
else
  if [ -z "${MODEL_S3_URI:-}" ]; then log "FATAL: MODEL_S3_URI unset"; exit 2; fi
  if aws s3 cp "$MODEL_S3_URI" "$MODEL_LOCAL" --only-show-errors; then log "s3 pull done bytes=..."; else log "FATAL: s3 pull failed rc=$?"; exit 3; fi
fi
exec /usr/local/bin/apr serve run "$MODEL_LOCAL" --host 0.0.0.0 --port "$PORT" --backend cpu ${APR_SERVE_EXTRA:-}
```
**Add, before the S3 pull** (the refusal idiom, `justfile:1806` + the 64-hex case table from RESEARCH Pattern 3):
```bash
refuse() { echo "[entry] REFUSED $1: $2" >&2; exit "${3:-2}"; }
case "${APR_API_KEY_HASH:-}" in ????????????????????????????????????????????????????????????????) ;; *) refuse auth "APR_API_KEY_HASH must be 64 hex chars (server would start OPEN with only a warning)" ;; esac
# after the pull: sha256 of $MODEL_LOCAL must equal $MODEL_SHA256 (ladder pin) before exec
```
Notes: `log "FATAL: s3 pull failed rc=$?"` inside the `else` reads the `if`'s status correctly, but keep `rc=$?` discipline for any new pipeline; `exec` must stay (LWA needs the server as the process); `bashrs lint` the file (0 errors today, DET004 warnings on `nproc`); `scripts/check_apr_bin_pinned.sh` does not scan `deploy/` — document the pin in the file header.

---

### `justfile` — `llm-endpoint-deploy` (fail-closed recipe, `DRY_RUN=1`)

**Analog:** `justfile::laya-deploy` (1796-1880). Copy the preamble, the `refuse()` ladder, the `set +e … rc=$? … set -e` capture, the python `-I`/`-c` parse-and-compare step, and the `DRY_RUN` exit.

**Preamble + refusal helper** (1796-1808):
```make
laya-deploy apr run data base server="aprender-mcp-decide" env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    APR="{{apr}}"
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    # shasum directly: the rtk hook never rewrites a command inside a recipe ...
    sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
    case "$ENV" in dev|prod) ;; *) refuse env "'$ENV' is not a known environment (expected dev or prod)" ;; esac
```
**Pin check: config names THIS file by content** (1821-1830):
```bash
    H="$(sha256 "$APR")"
    FIELDS="$(python3 -c 'import sys, tomllib; c = tomllib.load(open(sys.argv[1], "rb")); ...' "$CFG" 2>/dev/null)" \
        || refuse config "$CFG does not parse as the decide deploy config"
    read -r CFG_NAME CFG_URI CFG_SHA <<< "$FIELDS"
    [ "$CFG_SHA" = "$H" ] || refuse sha-pin "APRENDER_DECIDE_SHA256 in $CFG is $CFG_SHA but $APR hashes to $H"
```
**Status capture without `$?` through a pipe, then DRY_RUN exit** (1843-1866):
```bash
    set +e
    just laya-verify "$APR" "{{run}}" "{{data}}" "{{base}}" > "$VLOG" 2>&1
    vrc=$?
    set -e
    if [ "$vrc" -ne 0 ]; then refuse eligibility "$(grep -m 1 '^REFUSED' "$VLOG" || echo "laya-verify exited $vrc (log: $VLOG)")" "$vrc"; fi
    ...
    if [ "${DRY_RUN:-0}" = "1" ]; then
        echo "DRY-RUN: skipping the head-object check of s3://$BUCKET/$KEY (expected ContentLength $SIZE)"
        echo "DRY-RUN OK $SERVER (sha256 $H; nothing deployed)"
        exit 0
    fi
```
**Env/account guard before the first write** (1870-1876):
```bash
    ACCOUNT="$(aws sts get-caller-identity --profile "$PROFILE" --query Account --output text)" || refuse env "cannot read the AWS account for profile $PROFILE"
    WANT_BUCKET="aprender-decide-weights-${ACCOUNT}-${ENV}"
    [ "$BUCKET" = "$WANT_BUCKET" ] || refuse env "the config's weights bucket is $BUCKET but env=$ENV expects $WANT_BUCKET ..."
```
For this phase the ladder is: `pin.env` present → tarball `.sha256` check (`shasum -a 256 -c`, as `build_image.sh:10`) → ladder sha256 read from `contracts/model-capability-ladder-v1.yaml` (the 0.8B pin is at line 113; read it with `python3 -I`, never typed twice) → `APR_API_KEY_HASH` 64-hex → S3 `head-object` `Metadata.sha256` equals the ladder → image digest equals the one `llm-endpoint-image` recorded → `DRY_RUN` exit → `aws lambda create-function --package-type Image --architectures arm64 --memory-size 10240 --timeout 900 --ephemeral-storage Size=10240 ...` (from `.../031/tools/lambda_up.py`'s boto3 args, re-expressed in aws-cli).

---

### `justfile` — `llm-endpoint-weights` / `-up` (bucket + ECR + role)

**Analog:** `justfile::laya-weights-bucket` (2354-2382) for the idempotent, private, encrypted, tagged bucket; `.../031/tools/fetch_weights_codebuild.sh` for the CodeBuild job (fix its 2 bashrs errors, SC1078 lines 14/16, by moving the inline JSON policy to a file under `deploy/llm-endpoint/`).
```bash
    ACCOUNT="$(aws sts get-caller-identity --profile "$P" --query Account --output text)"
    BUCKET="aprender-decide-weights-${ACCOUNT}-${ENV}"
    if aws s3api head-bucket --profile "$P" --bucket "$BUCKET" 2>/dev/null; then echo "  exists:  $BUCKET"
    else aws s3api create-bucket --profile "$P" --region us-east-1 --bucket "$BUCKET" > /dev/null; echo "  created: $BUCKET"; fi
    aws s3api put-public-access-block --profile "$P" --bucket "$BUCKET" --public-access-block-configuration BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
    aws s3api put-bucket-encryption --profile "$P" --bucket "$BUCKET" --server-side-encryption-configuration '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"}}]}'
    aws s3api put-bucket-tagging --profile "$P" --bucket "$BUCKET" --tagging "TagSet=[{Key=project,Value=aprender},{Key=component,Value=decide-weights},{Key=env,Value=$ENV}]"
```
Weights are uploaded with `--metadata sha256=<ladder pin>` (as `fetch_weights_codebuild.sh` does) so `llm-endpoint-deploy` can compare `head-object` metadata to the ladder. A D-15 option: server-side `aws s3 cp s3://llm-spike-031-816069129666/Qwen3.5-0.8B-Q4_K_M.gguf s3://<new>/...` (already carries the metadata), no CodeBuild run.

---

### `justfile` — `llm-endpoint-image` (release tarball → ECR)

**Analog:** `.../031/tools/build_image.sh` (lines 5-13):
```bash
set -uo pipefail
TAR=bin/apr-v0.70.3-aarch64-unknown-linux-gnu-cpu.tar.gz
(cd bin && shasum -a 256 -c apr-v0.70.3-aarch64-unknown-linux-gnu-cpu.tar.gz.sha256) || exit 1
tar xzf "$TAR" -C deploy --strip-components=1 apr-v0.70.3-aarch64-unknown-linux-gnu-cpu/apr || exit 1
aws ecr get-login-password --profile "$P" --region "$R" | docker login --username AWS --password-stdin "${ECR%%/*}" || exit 1
docker buildx build --platform linux/arm64 --provenance=false -t "$ECR:cpu-v0.70.3" --push deploy
```
Changes: read `APR_TAG` from `pin.env`; fetch with `gh release download "$APR_TAG" --repo paiml/aprender -p "apr-${APR_TAG}-aarch64-unknown-linux-gnu-cpu.tar.gz*"` into the gitignored `deploy/llm-endpoint/bin/`; `refuse` on sha mismatch; tag `cpu-${APR_TAG}`; record the pushed image digest into `.ids/` for `llm-endpoint-deploy` to compare. (`v0.71.4` assets: `apr-v0.71.4-aarch64-unknown-linux-gnu-cpu.tar.gz` 25,051,867 B + `.sha256`.)

---

### `justfile` — `llm-endpoint-prove-401` / `-url` / `-probe` / `-measure`

**Analog (bench path):** `.../031/tools/remote_bench.py:27-32` builds the API-GW v2 event with an `authorization: Bearer ...` header and invokes with `LogType=Tail`, parsing `REPORT` (Duration, Init Duration, Max Memory, Billed) and pricing `mem/1024 × billed_ms/1000 × 0.0000133334`. Shapes from `shapes.py` (`shape_short`, `shape_tools2`, `shape_tools10`, `shape_turn2`, with nonces).
**Analog (local auth table):** `.../029/tools/auth_probe.sh` — bash, not zsh:
```bash
probe() { local label="$1"; shift
  local code; code=$(curl -s -m 180 -o /tmp/auth-body.json -w '%{http_code}' -H 'Content-Type: application/json' "$@" -d "$B" "http://127.0.0.1:$PORT/v1/chat/completions")
  echo "| $label | $code | $(head -c 120 /tmp/auth-body.json | tr -d '\n') |"; }
probe "no Authorization header"
probe "Bearer wrong-key" -H "Authorization: Bearer wrong-key"
probe "Bearer <correct key>" -H "Authorization: Bearer $KEY"
```
**Ordering the recipes must encode** (RESEARCH Pattern 4): `prove-401` invokes with NO header and `refuse`s unless `.statusCode == 401`; only then `url` runs `create-function-url-config --auth-type NONE --invoke-mode BUFFERED` plus the two `add-permission` statements (`lambda:InvokeFunctionUrl` and `lambda:InvokeFunction`, October-2025 rule); `probe` runs `python3 -I deploy/llm-endpoint/probe/probe_openai.py --base <url>/v1 --model Qwen3.5-0.8B-Q4_K_M --out <json>` (stdlib only, `probe_openai.py:12-17`) and the verdict table gains a `KNOWN LIMITATION` symbol; `measure` writes `09-LIVE-DEPLOY-EVIDENCE.json` with wall AND billed durations and `cpu_part`.

---

### `justfile` — `llm-endpoint-teardown`

**Analog:** `justfile::laya-teardown` (2742-2790) for contain-first + status-checked deletes + "NOT RUN — the human's call" tail; `.../031/tools/teardown.sh` for the resource list (fix its bashrs SC2105 at line 33).
```bash
    aws lambda put-function-concurrency --profile "$P" --function-name "$SERVER" --reserved-concurrent-executions 0 > /dev/null
    GOT="$(aws lambda get-function-concurrency --profile "$P" --function-name "$SERVER" --query ReservedConcurrentExecutions --output text)"
    [ "$GOT" = "0" ] || { echo "ERROR: reserved concurrency of $SERVER reads '$GOT', not 0 -- it may still be serving" >&2; exit 1; }
    echo "  CONTAINED: $SERVER reserved concurrency 0 (no invocation runs, warm instances included)"
    ...
    set +e
    DERR="$(aws iam delete-role-policy ... 2>&1 > /dev/null)"
    drc=$?
    set -e
    case "$DERR" in *NoSuchEntity*) echo "  no legacy policy ... (expected)" ;; *) echo "ERROR: ... (exit $drc): $DERR" >&2; exit 1 ;; esac
    echo "  NOT RUN -- the human's call:"
```
Phase-specific additions: `delete-function-url-config` AND `remove-permission` for BOTH statement ids (deleting the URL leaves the policy behind); then function, ECR image, log group, bucket (unless `KEEP_BUCKET=1`), role. The spike-031 leftovers (`llm-spike-031-lambda-0-8b`, `-2b`, ECR `llm-spike-031`, bucket `llm-spike-031-816069129666`, 4 roles, SG, ECS cluster, CodeBuild project) are a D-15 decision: reuse or tear down, never silently.

---

### `.gitignore` (root-anchored ignores)

**Analog:** `.gitignore:70-72`:
```
/deploy/node_modules/
/deploy/cdk.out/
/deploy/.build/
```
Add `/deploy/llm-endpoint/bin/` and `/deploy/llm-endpoint/.ids/`. `*.gguf` (line 52) already covers a stray model file. Never a bare `bin/` (CB-510). After editing, run `bash scripts/check_include_files.sh` and `git check-ignore -v deploy/llm-endpoint/entrypoint.sh` (must exit 1).

---

### `deploy/llm-endpoint/pin.env` + `.ids/` (tracked pin, generated ids)

**Analog:** `crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template` — a TRACKED template whose generated sibling (`deploy.toml`, embeds the account id) is gitignored and refused while it still holds `UNSET-run-just-laya-deploy-config` (`laya-deploy:1817-1820`). Its memory/timeout comment is the model for documenting the tier:
```toml
# THE TIER. contracts/decide-tool-boundary-v1.yaml prices its token budget for ONE
# memory size, `lambda_memory_mb`, and a test asserts this value equals it (FALSIFY-DECIDE-TOOL-007) ...
memory_mb = 10240
timeout_seconds = 30
[environment]
APRENDER_DECIDE_S3_URI = "UNSET-run-just-laya-deploy-config"
APRENDER_DECIDE_SHA256 = "UNSET-run-just-laya-deploy-config"
```
For this phase: `pin.env` holds `APR_TAG=v0.71.4` (or v0.70.3 if the re-probe fails), `APR_TARBALL_SHA256=<from the release .sha256>`, `GGUF_SHA256` read from the ladder at recipe time (not typed). `.ids/` holds account/bucket/function/URL and is gitignored (the URL must never be committed — cost amplification, RESEARCH Q1).

---

### `.planning/phases/09-*/09-LIVE-DEPLOY-EVIDENCE.json` (evidence record)

**Analog:** `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-LIVE-REDEPLOY-EVIDENCE.json` (tracked). Top-level keys to mirror: `schema`, `server`, `artifact_sha256`, `offline_dry_run{ok, commit, command, rc, lines[], aws_calls}`, then (per RESEARCH Q6) `outcome`, `decided_by`, `decided_on`, `option`, `auth{enabled, provider, mechanism, risk_acceptance}`, `function_configuration{memory_size_mb, timeout_s, architecture, runtime, ephemeral_storage_mb, sha256_pin_equals_artifact, log_group}`, `precondition_gates`, `cold_samples`, `warm`, `identity_matches`, `teardown_commands_not_run`. Opening lines of the analog:
```json
{
  "schema": "laya-live-redeploy-v1",
  "server": "aprender-mcp-decide",
  "artifact_sha256": "24a44d7e...",
  "offline_dry_run": {
    "ok": true,
    "commit": "e0be0262b6e31a3e55d6ada7a79f39d8d3a555be",
    "command": "lockf -k /tmp/aprender-laya-real-weights.lock env DRY_RUN=1 just laya-deploy-selftest (MAIN checkout)",
    "rc": 0,
    "lines": ["DRY-RUN OK aprender-mcp-decide (sha256 ...; nothing deployed)", "AWS CALLS: 0"],
    "aws_calls": 0
  },
```
Identity for this phase = three values: the `apr --version` line from the `[entry]` log (tag + git sha), the image digest, the GGUF sha256; plus `cpu_part` per sample and both `client_wall_ms` and `billed_ms`. The hold shape (`08-DEPLOY-EVIDENCE.json`: `outcome: hold`, `option: hold-no-aws`, `readiness_open[]`) is the D-15 "no-go" record.

---

### [sibling repo] `rust-mcp-sdk/crates/pmcp-agent/src/sources/openai_compat.rs` — `message_to_openai` (D-09)

**Outside this repo; feature branch + PR, never a commit to main (D-11).** Tracked in rust-mcp-sdk.

**Edit site** (167-183) — the `ToolUse` arm omits `content`; the sibling `ToolResult` arm (184-190) shows the field placement:
```rust
fn message_to_openai(msg: &SamplingMessage) -> Value {
    match &msg.content {
        SamplingMessageContent::Text { text, .. } => json!({ "role": role_str(msg.role), "content": text }),
        SamplingMessageContent::ToolUse { id, name, input, .. } => json!({
            "role": "assistant",
            // ADD: "content": "",   (the platform's openai_v1 transformer already writes "" here)
            "tool_calls": [{ "id": id, "type": "function", "function": { "name": name, "arguments": input.to_string() } }],
        }),
        SamplingMessageContent::ToolResult { tool_use_id, content, .. } => json!({
            "role": "tool", "tool_call_id": tool_use_id, "content": tool_result_text(content),
        }),
```
**Test to extend** (364-382):
```rust
    #[test]
    fn tool_use_message_becomes_tool_calls() {
        let msg = SamplingMessage::new(Role::Assistant, SamplingMessageContent::ToolUse { name: "search".to_string(), id: "tc-1".to_string(), input: json!({"q": "rust"}), meta: None });
        let v = message_to_openai(&msg);
        assert_eq!(v["role"], "assistant");
        assert_eq!(v["tool_calls"][0]["id"], "tc-1");
        // arguments must be a JSON *string*.
        assert!(v["tool_calls"][0]["function"]["arguments"].is_string());
        // ADD: assert_eq!(v["content"], "");
    }
```
Run: `cargo test -p pmcp-agent --features openai-compat sources::openai_compat::tests::tool_use_message_becomes_tool_calls`. Proof against the server: the spike-029 `agent-loop` driver depends on pmcp-agent by path, so check the PR branch out in `~/Development/mcp/sdk/rust-mcp-sdk` and rebuild.

---

### [sibling repo] `pmcp-run/.../src/config/loader.rs` — optional `timeoutSecs` (D-10 proposal)

**Outside this repo; brief + PR for the pmcp.run team (D-11, `autonomous: false`).** Tracked in pmcp-run.

**Optional-field read pattern already in `map_provider_row_to_config`** (925-944):
```rust
    let enabled = item.get("enabled").and_then(|v| v.as_bool().ok()).copied().unwrap_or(true); // absent → default-enabled
    if !enabled { return Err(ConfigError::DisabledProvider(provider_id.to_string())); }
    let transformer_id = get_string(item, "transformerId")?;
    let endpoint_template = get_string(item, "endpointTemplate")?;   // A3: verbatim, {model} NOT resolved here
    let auth_header_prefix = item.get("authHeaderPrefix").and_then(|v| v.as_s().ok()).filter(|s| !s.is_empty()).map(|s| s.to_string());
    let secret_key_name = get_string(item, "secretKeyName")?;         // the SM key NAME only (T-107-01 / R2-10)
```
**The line the proposal changes** (965):
```rust
        timeout: crate::handler::iteration::LLM_HTTP_TIMEOUT_SECS, // single source of truth (the staleness-invariant test guards this const)
```
→ `timeout: row_timeout_secs.unwrap_or(LLM_HTTP_TIMEOUT_SECS)` with `row_timeout_secs` read as `item.get("timeoutSecs").and_then(|v| v.as_n().ok()).and_then(|n| n.parse::<u64>().ok())` and clamped.
**The invariant that bounds the clamp** (`handler/iteration.rs:62`, tests 2949-2984):
```rust
pub(crate) const LLM_HTTP_TIMEOUT_SECS: u64 = 120;
...
    fn test_staleness_invariant_class1_worst_case() {
        const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 300;
        let class1_worst_case_secs = LLM_HTTP_TIMEOUT_SECS * LLM_MAX_CLASS1_ATTEMPTS as u64
            + LLM_CLASS1_BASE_BACKOFF_SECS * (LLM_MAX_CLASS1_ATTEMPTS.saturating_sub(1)) as u64;
        assert!(class1_worst_case_secs < DEFAULT_STALENESS_THRESHOLD_SECS, "STALENESS INVARIANT VIOLATED (Class-1): ...");
    }
```
So `2×T + 2 < 300` ⇒ `T ≤ 148`; the brief offers (a) clamp ≤ 148 with two attempts [recommended for the demo], (b) a row-set `maxClass1Attempts = 1`, (c) raise `TeamLimits.max_sub_task_staleness_seconds`. Extend both invariant tests with the row-bound maximum; existing assertions at `loader.rs:1246, :1291` (`assert_eq!(config.timeout, LLM_HTTP_TIMEOUT_SECS)`) stay true for rows without the field.

**Row field list the brief quotes** (`amplify/data/resource.ts:1450-1466`): `id`, `displayName`, `transformerId`, `endpointTemplate` (verbatim; `{model}` resolved only in `service.rs::resolve_endpoint`), `authHeaderName`, `authHeaderPrefix` (optional), `secretKeyName` (NAME only, R2-10), `enabled` (default true), `availableBuiltInTools`. Request shape the row drives (`llm/transformers/openai.rs:28-44`): `model`, `messages`, `max_tokens`/`top_p`/`stream` only when `Some`, `tools[]`; no `temperature`.

## Shared Patterns

### Refusal ladder before any side effect
**Source:** `justfile::laya-deploy:1806` (`refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }`) and `laya-weights-bucket`'s `case "$ENV" in dev|prod) ;; *) ... exit 2`.
**Apply to:** every new `llm-endpoint-*` recipe and `entrypoint.sh`. Each check prints `REFUSED <check>: <why>` and exits non-zero before the first AWS call; `DRY_RUN=1` ends at `DRY-RUN OK ... (nothing deployed)`. Each refusal must be shown to fire (mutation: wrong hash, wrong sha) — `laya-deploy-selftest` (2014) is the self-test precedent.

### Status capture without `$?` through a pipe
**Source:** `justfile:1843-1848` (`set +e; cmd > "$LOG" 2>&1; rc=$?; set -e`); `scripts/check_contract_test_binding.sh` uses `set -uo pipefail` (no `-e`) for the same reason.
**Apply to:** every recipe body (`#!/usr/bin/env bash`, never zsh — the Bash tool here is zsh and has no `PIPESTATUS`), `entrypoint.sh`, and the probe wrapper.

### Auth gate wiring + the contracted 401 envelope
**Source:** `auth.rs::layer` (217-225), `apply` (142-168); contract `contracts/apr-serve-api-key-auth-v1.yaml` FALSIFY-AUTH-001..003; CI fragments `250-aprender-contracts-apr-serve-api-key-auth-contract.cmd`, `260-apr-cli-falsify-auth-001-002-003.cmd`.
**Apply to:** `server.rs::build_gguf_router` and its test. Do not touch the `{"error":"unauthorized","message":...}` shape (it is asserted by FALSIFY-AUTH-001 and by `contract_post_error_envelope_preservation!` in `routes.rs:321`).

### Model-free serde tests
**Source:** `qwen35_chat_backend_tests.rs:772` (`serde_json::from_value::<ChatCompletionRequest>`), `openai_handlers.rs:1287-1340` (`pmat801_tool_calling_tests`).
**Apply to:** every D-03/D-05 unit test; fixtures from `.../029-apr-serve-openai-tool-loop/results/wire-*.jsonl`.

### Contract-row ↔ test binding
**Source:** `decide-tool-boundary-v1.yaml` `falsification_tests[].test` (cargo filter strings); `scripts/check_contract_test_binding.sh` (PV-VER-002, zero-finding rule for new contracts, `MIN_REFS=250`); `justfile:3442` pv resolution.
**Apply to:** every row in `llm-endpoint-openai-conformance-v1.yaml`; probe-proven rows bind to the Rust verdict-JSON reader, with the `just` recipe in `implemented_by`.

### Identity proof, never label-by-intent
**Source:** `entrypoint.sh` (`[entry] ... apr=$(apr --version) ... cpu_part=...`), `laya-build-bootstrap` (1759: output newer than a stamp, aarch64 ELF, names the crate), `08-LIVE-REDEPLOY-EVIDENCE.json` `identity_both`.
**Apply to:** image build (`apr --version` recorded), every live sample (`cpu_part`, tag+sha, image digest, GGUF sha256), the evidence record.

### Root-anchored ignores
**Source:** `.gitignore:70-72`; CLAUDE.md "Publishing Safety".
**Apply to:** `/deploy/llm-endpoint/bin/`, `/deploy/llm-endpoint/.ids/`.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `.planning/phases/09-*/09-PMCP-RUN-BRIEF.md` | doc (cross-team brief) | — | First brief addressed to another team; D-11 fixes its contents (row values, wire fixtures, conformance table, timeout proposal with the three options, PR list). Nearest shape precedent is PR `#4941`'s body (What is in / not in / Known gap / Verification). |
| `deploy/llm-endpoint/probe/` verdict-JSON Rust reader (`conformance_record_matches_contract`) | test (reader over committed JSON) | file-I/O | No in-tree test reads a committed verdict JSON against contract expectations; Phase 8 asserted constants via `test_support::constant_u64` (`FALSIFY-DECIDE-TOOL-007`), the closest idea. Keep it a plain `serde_json` read of `results/probe-<pin>.json` asserting each gap id's verdict in {`✓`, `KNOWN LIMITATION`}. |

## Metadata

**Analog search scope:** `crates/apr-cli/src/commands/serve/`, `crates/apr-cli/tests/`, `crates/aprender-serve/src/api/`, `crates/aprender-mcp-decide-lambda/`, `contracts/`, `scripts/`, `ci/explicit-test-commands.d/`, `justfile`, `.gitignore`, `.claude/skills/spike-findings-aprender/sources/{029,031}-*/`, `.planning/phases/08-*/`; sibling repos `~/Development/mcp/sdk/rust-mcp-sdk/crates/pmcp-agent/src/sources/` and `~/Development/mcp/sdk/pmcp-run/amplify/{data,functions/durable-agent-lambda/src/{config,handler,llm}}`.
**Files scanned:** 24 read (targeted ranges), all confirmed git-tracked in their repo.
**Pattern extraction date:** 2026-10-15
