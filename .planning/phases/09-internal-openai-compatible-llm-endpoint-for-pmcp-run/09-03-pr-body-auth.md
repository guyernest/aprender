## What is in

`apr serve` on a GGUF model (including the Qwen3.5 hybrid, and the embedding server) never applied the Bearer gate: `serve_router` built realizar's router with `create_router_with_config` and served it directly, so `APR_API_KEY_HASH` / `APR_API_KEY` were read by nothing on this path. The server answered 200 on every route without a key (observed at v0.70.3 and v0.71.4). `build_apr_cpu_router` already gates the `.apr` CPU path; this is the same wiring for the GGUF path.

- `build_gguf_router(state, config, auth_gate)` in `crates/apr-cli/src/commands/serve/server.rs`: realizar's router wrapped with `super::auth::layer`. It is a seam so the wiring is testable without a socket or model weights.
- `serve_router` now builds through it with `AuthGate::from_env()`. The `advertised_routes` banner logic is untouched.
- `/health` is gated like every other route (the auth contract says health and metrics are not exempt). Behind the AWS Lambda Web Adapter this is still fine: its readiness check treats any 100-499 status as ready, so a 401 on `/health` still marks the function ready. `layer_public_ops` is the one-word alternative if a probe needs `/health` open.
- Four `oneshot` tests in `gguf_router_auth_tests` on a model-free `AppState::demo_mock()`, driving the real realizar router: 401 + `{"error":"unauthorized"}` + `WWW-Authenticate: Bearer` on `POST /v1/chat/completions`, `GET /health` and `GET /v1/models` without a header; 401 with a wrong key; not 401 with the right key; and an unconfigured (`AuthGate::disabled()`) gate leaves the router open.

The 401 envelope is unchanged (FALSIFY-AUTH-001 asserts it).

## What is not in

- The CUDA-gated call sites that also call `create_router_with_config` directly: `start_apr_server_gpu`, `start_apr_q4k_server_gpu`, `start_safetensors_server_gpu` (`handlers_include_01.rs`), `start_gguf_server_cuda` (`handler_gpu_completion.rs`) and `start_gguf_server_gpu_batched` (`server.rs`). The fix is the same one-liner, but I have no CUDA host to test it on. I will file that as a follow-up issue.
- `contracts/apr-serve-api-key-auth-v1.yaml` is not edited. Its text names an `--auth-disabled` flag that does not exist in `apr-cli` and lists `routes.rs::create_router`, which has no production caller. I will file that as a separate issue rather than widen a one-file PR.

## Known gap

`AuthGate::from_env()` returns a disabled gate, with one stderr line, when neither `APR_API_KEY_HASH` nor `APR_API_KEY` is set (or the hash is malformed). This PR keeps that existing semantics, so the server still starts open when no key is configured. Deployments that need fail-closed must refuse to start without the variable; my own deployment handles that in its container entrypoint, outside this PR.

## Verification

Run in a worktree cut from `upstream/main` (abf66f908), one commit on top, only `server.rs` changed:

- RED first: `cargo test -p apr-cli --lib gguf_router_auth_tests` failed to compile with `error[E0425]: cannot find function build_gguf_router in this scope`.
- GREEN: `cargo test -p apr-cli --lib gguf_router_auth_tests` -> 4 passed, 0 failed.
- `cargo test -p apr-cli --test falsify_auth_001 --test falsify_auth_002 --test falsify_auth_003 --no-fail-fast` -> `test result: ok` x3 (4, 3 and 4 passed).
- `cargo fmt --all -- --check` clean.
- `cargo clippy -p apr-cli --lib --no-deps -- -D warnings` -> 0 errors, same as the pristine baseline of 0.
