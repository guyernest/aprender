Title: apr serve: CUDA-path router builders do not apply the Bearer gate (APR_API_KEY_HASH ignored)

## Summary

`contracts/apr-serve-api-key-auth-v1.yaml` says every route on every router builder in
`crates/apr-cli/src/commands/serve/` honours the same auth gate (FALSIFY-AUTH-001). On
`main` at abf66f908 the `.apr` CPU router (`build_apr_cpu_router`), the SafeTensors routers and
`build_gpu_router` are wrapped with `super::auth::layer`, but five call sites build realizar's
router with `create_router_with_config(...)` and serve it directly, so `APR_API_KEY_HASH` /
`APR_API_KEY` are silently ignored on them. All five are behind `feature = "cuda"`:

| File | Function | Line |
| ---- | -------- | ---- |
| `crates/apr-cli/src/commands/serve/handlers_include_01.rs` | `start_apr_server_gpu` | 86 |
| `crates/apr-cli/src/commands/serve/handlers_include_01.rs` | `start_apr_q4k_server_gpu` | 197 |
| `crates/apr-cli/src/commands/serve/handlers_include_01.rs` | `start_safetensors_server_gpu` | 283 |
| `crates/apr-cli/src/commands/serve/handler_gpu_completion.rs` | `start_gguf_server_cuda` | 753 |
| `crates/apr-cli/src/commands/serve/server.rs` | `start_gguf_server_gpu_batched` | 563 |

The CPU GGUF / Qwen3.5 path (`serve_router`) had the same gap and is fixed separately in the PR
that adds `build_gguf_router` (branch `fix/gguf-router-auth-gate`).

## Proposed fix

The same one-liner as `build_gguf_router`: wrap each router with
`super::auth::layer(super::auth::AuthGate::from_env(), router)` (or route them through
`build_gguf_router`). I could not test this: it needs a CUDA host, and the five functions are
`coverage(off)` and not reachable from a CPU build. A `oneshot` test on the router seam, as in
`gguf_router_auth_tests`, would cover the wiring without a GPU.

## Why it matters

A deployment that sets `APR_API_KEY_HASH` and starts `apr serve --gpu` on a GGUF, `.apr` or
SafeTensors model gets an open server with a one-line stderr warning at most, which contradicts the
contract's `scope`.
