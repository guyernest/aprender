# Deferred Items — Phase 01

Out-of-scope discoveries logged during execution. NOT fixed (scope boundary: only
issues directly caused by the current task's changes are auto-fixed).

| # | Found in | Item | Why deferred |
|---|----------|------|--------------|
| 1 | Plan 01-02 | `scripts/check_include_files.sh` is vacuous on macOS. It uses `grep -oP` (GNU PCRE), unsupported by BSD grep, so the script prints a usage error to stderr, counts zero files, and exits 0 — reporting `OK: All 0 include!() files are tracked by git` on a repo CLAUDE.md documents as having 562. The CB-510 guard therefore provides no protection for any developer on darwin. | Pre-existing; in `scripts/`, untouched by this plan. This is a Verification Discipline #5 "guard that does not scan the decision surface is theater" instance and deserves its own ticket. |
| 2 | Plan 01-02 | `cargo clippy -p aprender-core -- -D warnings` exits 101 on darwin due to 20 warnings-as-errors in the **`aprender-compute` dependency** (unused imports/variables/dead code in cfg-gated NEON/AVX paths that are inactive on this target). No diagnostic points at `aprender-core`. | Pre-existing; `crates/aprender-compute/` was not touched by this plan (`git diff --name-only` confirms). Plan 01-02's clippy criterion was verified instead by `cargo clippy -p aprender-core --lib --tests` (exit 0) plus confirming zero warnings in all seven touched files. |
| 3 | Plan 01-02 | `cargo check --workspace` fails on darwin: `crates/aprender-profile` hard-stops via `#[cfg(not(target_os = "linux"))] compile_error!("renacer requires Linux (ptrace syscall tracing)")`. | Intentional platform gate, not a defect. Verified with `cargo check --workspace --exclude aprender-profile` (exit 0, all other 77 crates clean). |
| 4 | Plan 01-02 | Pre-existing warnings in `aprender-core` test builds: `f16_first_u16` never used (`serialization/safetensors_tests_core.rs:571`), and unused `#[must_use]` return in `models/bert/embeddings.rs:128`. | Unrelated files, not caused by this plan's changes. |
