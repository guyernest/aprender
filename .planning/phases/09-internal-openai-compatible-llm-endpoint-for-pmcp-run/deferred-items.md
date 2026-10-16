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
