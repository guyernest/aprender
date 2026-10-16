# Phase 9 deferred items

Out-of-scope discoveries logged by executors. Not fixed in the plan that found them.

## From plan 09-02

### D-09-02-1: `kernel_registry::parity::committed_parity_receipts_hold_on_this_host` fails on this host (pre-existing)

- **Found during:** Task 1, full `cargo test -p aprender-serve --lib` in upstream worktree A (`aprender-wt-pr-content`).
- **Symptom:** `cpu.matvec.q6_k: max_rel_err 0.0000016505971099429934 > receipt tolerance 0.0000004 (evidence/kreg/parity/cpu.matvec.q6_k.json)` at `crates/aprender-serve/src/kernel_registry_parity.rs:804`.
- **Why it is out of scope:** it fails identically on pristine `upstream/main` at `5b1fd7b08` (run in worktree B before any edit), and it passes on the phase branch. A host-specific numeric receipt (aarch64), unrelated to serde request parsing.
- **Evidence:** `/tmp/p09-02-b-pristine-parity.log`.

### D-09-02-2: `apr-cli` serve `ChatCompletionRequest` has no `max_completion_tokens` alias (observation)

- `crates/apr-cli/src/commands/serve/types.rs:479` declares `max_tokens: Option<u32>` with no `alias = "max_completion_tokens"`, and no `#[serde(default)]` on it beyond the `skip_serializing_if`. That is a separate request type from `aprender-serve`'s `ChatCompletionRequest`, which is the one PR #5412 changes.
- Whether any deployed route (the CPU GGUF / Qwen3.5 path this phase serves) deserializes that apr-cli type, rather than `aprender-serve`'s, was not traced in plan 09-02. Plan 09-05 or the deployed probe (`max_completion_tokens` row, FALSIFY-LLM-CONF-003 / row 015-class live probes) will show it: if the field is still ignored on the live endpoint, that type needs the same one-attribute alias as a follow-up.
- No pmcp.run client sends the field (D-05), so this never gates the end-to-end turn.

### D-09-02-3: pre-existing clippy error `unused macro definition: cpu_feature` (`crates/aprender-serve/src/isa.rs:171`)

- Present on pristine `upstream/main` (`5b1fd7b08`) under `cargo clippy -p aprender-serve --lib --no-deps -- -D warnings`; absent on the phase branch (count 0). Not introduced by either fix.

## From plan 09-03

### D-09-03-1: the two upstream issue drafts are saved but deliberately NOT filed

- The user answered `open-draft` at the 09-03 checkpoint: the PR only (https://github.com/paiml/aprender/pull/5451), no issues.
- Drafts (kept in this directory): `09-03-issue-gpu-router-auth.md` (CUDA-path routers ungated) and `09-03-issue-auth-contract-drift.md` (auth contract names a nonexistent `--auth-disabled` flag, wrong `references`, `build_gpu_router` has no caller).
- The PR body's "What is not in" says both will be filed as follow-ups. Until they are, the CUDA exception lives only in contract row FALSIFY-LLM-CONF-002 and the deployed 401 proofs.

### D-09-03-2: five CUDA-gated router call sites still ungated (scope exception, not fixed)

- `handlers_include_01.rs` (`start_apr_server_gpu`, `start_apr_q4k_server_gpu`, `start_safetensors_server_gpu`), `handler_gpu_completion.rs` (`start_gguf_server_cuda`) and `server.rs` (`start_gguf_server_gpu_batched`), all behind `feature = "cuda"`, build `create_router_with_config(...)` without an `AuthGate`. `APR_API_KEY_HASH` is ignored on them.
- Out of scope: this deployment is CPU-only (`--backend cpu`), and the change cannot be tested without a CUDA host. The fix is the same one-liner as `build_gguf_router`.
- Do not run a GPU-backed deployment of this endpoint until these are gated or the deployed 401 proof is repeated on that path.

### D-09-03-3: auth contract drift (observation)

- `contracts/apr-serve-api-key-auth-v1.yaml` is not edited by this phase. See `09-03-issue-auth-contract-drift.md` (D-09-03-1).
