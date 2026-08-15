# Phase 4 — deferred items

Out-of-scope discoveries logged rather than fixed. Each was observed while executing a plan,
is NOT caused by that plan's changes, and touches files the plan does not own.

---

## D-04-03-A — `aprender-core` cannot pass `clippy -D warnings` on arm64 (pre-existing)

Found during: plan 04-03, Task 1 verification.

```
cargo clippy -p aprender-core --features setfit --lib --tests --no-deps -- -D warnings
rc=101, 1 error:
  error: unreachable expression
     --> crates/aprender-core/src/demo/reliable/performance.rs:126:5
  124 |         return "NEON".to_string();
  126 |     "Scalar".to_string()
```

`#[cfg(target_arch = "aarch64")] { return "NEON".to_string(); }` makes the trailing
`"Scalar".to_string()` unreachable on arm64 only. Zero findings in `setfit/artifact.rs`
(`grep -c "setfit/artifact.rs" <clippy log>` = 0), and `git diff --name-only 17938b1a7..HEAD`
does not list `demo/`, so this is the known-red arm64 baseline (D-ITEM-02), not a regression.

Fix belongs to whoever owns `demo/reliable/` — either an `#[allow(unreachable_code)]` with a
comment or a `#[cfg(not(target_arch = "aarch64"))]` on the trailing expression. **Do not fix by
dropping `-D warnings`.**

---

## D-04-03-B — three `aprender-train` insta snapshots are stale (pre-existing)

Found during: plan 04-03, Task 3 commit (the active pre-commit hook ran the crate's lib tests).

```
cargo test -p aprender-train --lib prune::snapshot
rc=101 — 14 passed, 3 failed:
  prune::snapshot_tests::tests::snapshot_all_prune_methods
  prune::snapshot_tests::tests::snapshot_pipeline_stages
  prune::snapshot_tests::tests::snapshot_schedule_validation_errors
```

Each run writes a `.snap.new` beside the committed `.snap`. `crates/aprender-train/src/prune/`
is not in 04-03's `files_modified` and `--lib` does not compile `tests/`, so the two ui files
this plan added cannot be the cause. Whoever owns `prune/` should review the three `.snap.new`
diffs and either bless them or fix the drift — a blind `INSTA_UPDATE=always` would bless
whatever regressed.

---

## D-04-03-C — the active pre-commit hook fails without blocking, and has a broken line

Found during: plan 04-03, Task 3 commit. Observed in the hook's own output:

```
error: test failed, to rerun pass `-p aprender-train --lib`
(eval):2: command not found: --features
ok worktre
[exited with code 0]
```

Two separate problems, both in the class this phase exists to catch:

1. The hook ran a test suite, that suite FAILED (D-04-03-B), and the commit still succeeded.
   A gate that cannot block is theater.
2. `(eval):2: command not found: --features` — a command was split across lines so its flags
   became a command. That leg has never run.

The repo-tracked `.githooks/pre-commit` is NOT the hook that ran: `git config --get
core.hooksPath` exits 1 (unset), and `.githooks/pre-commit` contains no `cargo test` and no
`--features`. So the active hook is installed outside the repo and is not visible from a
worktree — CLAUDE.md rule 8's shadowed-artifact class. Locating and repairing it is out of
scope for a plan that owns two source files.
