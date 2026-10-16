Title: apr-serve-api-key-auth-v1 describes a flag and router coverage that the code does not have

## Summary

`contracts/apr-serve-api-key-auth-v1.yaml` and the code under
`crates/apr-cli/src/commands/serve/` disagree in three places. Observed on `main` at abf66f908
with `rg`/`grep`; nothing here is a runtime measurement.

1. **`--auth-disabled` does not exist.** The contract's `description`, `metadata.description` and
   the `jidoka` text say "Disabling auth requires an explicit `--auth-disabled` flag with a visible
   startup warning, not a silent default". No clap argument by that name exists in `apr-cli`; the
   only mention in source is a doc comment at `routes.rs:339`. What actually happens is
   `AuthGate::from_env()` returning a disabled gate plus one stderr line when neither
   `APR_API_KEY_HASH` nor `APR_API_KEY` is set (or when the hash is malformed), which is the
   "silent default" the jidoka text says does not exist.
2. **`references` names the wrong router.** It lists `routes.rs::create_router`, which is
   `create_router_with_auth(state, AuthGate::disabled())` and has no production caller (the only
   in-tree users are `tests_e2e_http.rs` and an unused import in `safetensors.rs`). It does not list
   `server.rs::serve_router` (the GGUF / Qwen3.5 / embedding serve path, which until the
   `build_gguf_router` PR was served without any gate), nor the five CUDA-gated
   `create_router_with_config` call sites (see the separate issue).
3. **`build_gpu_router` has no caller.** The contract lists it as an authoritative router
   builder, but `grep -rn 'build_gpu_router(' crates/apr-cli/src` finds only the definition in
   `handlers_include_01.rs`. The only production use of `AuthGate::from_env()` in `serve/` is the
   `.apr` CPU path at `handlers.rs:1519`.

Together with the contract's `scope` ("every route on every router builder honours the same auth
gate"), a reader would conclude that setting `APR_API_KEY_HASH` protects every `apr serve`
configuration. It protected one.

## Suggested direction

- Either implement `--auth-disabled` and make "no key configured" an error unless it is passed, or
  rewrite the contract text to describe the env-var-only behaviour that exists.
- Replace `references` with the builders that exist and are called, and state the CUDA exception (or
  close it).
- Decide whether `build_gpu_router` should be wired to a caller or removed from the contract.

I have not touched the contract in the `build_gguf_router` PR, to keep that change to one file.
