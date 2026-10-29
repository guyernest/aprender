# Phase 14 deferred items

## 14-01

- **`cargo clippy -p <crate> ... -- -D warnings` fails on two pre-existing warnings in `aprender-compute`.**
  `crates/aprender-compute/src/brick/quant_ops/mod.rs:219` and `:318` carry an unused `backend` parameter.
  The `-D warnings` after `--` is applied to every workspace member clippy-driver compiles, not only to the
  `-p` crate, so the literal command in 14-01 Task 3's `<verify>` exits 101 for a reason unrelated to the new crate.
  14-01 used `cargo clippy -p aprender-mcp-laya-train --lib --tests --no-deps -- -D warnings` (clean). Later Phase 14
  plans that copy the literal command will hit the same failure; the fix (prefix the parameter with `_`, or an
  `#[allow(unused_variables)]` on those two `execute` impls) is out of scope here because the file belongs to a crate
  this phase does not touch.
