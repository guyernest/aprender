# Deferred items — Phase 5

Out-of-scope discoveries logged during execution. NOT fixed by the plan that found them.

## 05-06: 24 pre-existing failures in `cargo test -p aprender-train --lib`

Found while running the full `aprender-train` lib suite for regression coverage on
plan 05-06 (the plan's own scoped verify — `--lib classify_trainer` (330 passed) and
`apr-cli --lib --features setfit finetune` (81 passed) — is green).

Failing modules, none of which plan 05-06 touches:

| Module | Count | Example |
|--------|-------|---------|
| `gpu::ledger::tests` | 12 | `test_reserve_and_release` — `assertion left == right failed: left: 0, right: 8000` |
| `gpu::guard::tests` | 8 | `test_guard_update_actual` |
| `gpu::wait::tests` | 1 | `test_timeout_when_full` |
| `prune::snapshot_tests::tests` | 3 | `snapshot_all_prune_methods` |

Evidence they are not caused by 05-06:

- 05-06 modifies `finetune/classify_trainer.rs` (+ its tests) and apr-cli command files.
  `gpu/ledger.rs`, `gpu/guard.rs`, `gpu/wait.rs` and `prune/snapshot_tests.rs` are
  untouched and reference neither `TrainResult` nor `TrainingConfig`.
- `test_reserve_and_release` fails in ISOLATION (`cargo test -p aprender-train --lib
  test_reserve_and_release` → 0 passed; 1 failed), so it is not a parallel-execution
  interaction with the new tests either.
- The assertion is a VRAM reservation figure (0 vs 8000 MB) on a host with no reservable
  GPU ledger state — an environment dependency, not a logic regression.

Action: needs its own ticket (GPU-ledger tests assume a reservable GPU / writable ledger
directory; the prune snapshot tests need their snapshots reviewed). Not Phase 5 work.
