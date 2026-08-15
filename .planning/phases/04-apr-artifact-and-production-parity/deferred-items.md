# Phase 4 — deferred items

Out-of-scope discoveries logged during execution. Not fixed; recorded so they are
not rediscovered as if new.

---

## D-04-04-A — `cargo clippy -p aprender-core ... -D warnings` cannot pass, even with `--no-deps`

Discovered by plan 04-04, wave 4.

Orchestrator note F-03 established that `--no-deps` rescues the clippy gate for
`aprender-train` by excluding `aprender-compute`'s pre-existing debt. **That does not
extend to `aprender-core`**, which carries one of its own:

```
error: unreachable expression
   --> crates/aprender-core/src/demo/reliable/performance.rs:126:5
124 |         return "NEON".to_string();
126 |     "Scalar".to_string()
```

Measured, not assumed: `rtk proxy cargo clippy -p aprender-core --features setfit --lib
--no-deps -- -D warnings` exits **101** with exactly **one** error, the one above. The
20 `aprender-compute` entries in the same output are `warning`, not `error` — `--no-deps`
is working; the crate under test simply has debt of its own.

Consequence for any plan that states `cargo clippy -p aprender-core ... --no-deps --
-D warnings` as a gate: it fails today and would fail identically on an empty diff, so it
cannot distinguish "my code is clean" from "never linted" — the exact F-03 defect, one
crate over.

**What 04-04 did instead** (and what a Make target should do): run the same command and
assert **zero diagnostics whose path is under the plan's own files**. Verified at 04-04's
final state: zero lines matching `setfit/` in the clippy output.

**Action for 04-10:** either fix `demo/reliable/performance.rs:126` (a `cfg`-shaped
`return` followed by a fallback — the same pattern `aprender-compute` has four of), or
scope the core clippy leg by path. Do not "fix" it by dropping `-D warnings`.

---

## D-04-04-B — 24 `aprender-train --lib` tests fail in this worktree, in modules that link nothing this phase touches

Discovered by plan 04-04, wave 4. **Cause not diagnosed. Not claimed pre-existing** —
that was not measured.

`cargo test -p aprender-train --features setfit --lib` → `7865 passed; 24 failed`. The
failures are entirely:

- `gpu::guard::tests::*` (8)
- `gpu::ledger::tests::*` (12)
- `gpu::wait::tests::test_timeout_when_full` (1)
- `prune::snapshot_tests::tests::*` (3)

Sample: `gpu::ledger::tests::test_reserve_and_release` asserts `total_reserved() == 8000`
immediately after a successful `try_reserve(8000, …)` and observes `0`. Re-running with
`--test-threads=1` still fails 21 of them, so intra-binary parallelism is not the cause.
The ledger path is per-process (`temp_dir()/entrenar-ledger-test/test-ledger-{n}-{pid}.json`),
so cross-process contention with the parallel wave-4 agent is not the cause either.

**Proven NOT caused by this plan**, by import graph rather than by argument:

- `grep -rn "predict_proba\|predict_logits\|MultinomialLogisticRegression\|softmax"
  crates/aprender-train/src/gpu/ crates/aprender-train/src/prune/` → **zero matches**.
- `gpu/ledger.rs` imports `std`, `chrono`, `fs4`, `serde`, `super::{error, profiler}` and
  `crate::trace` — **nothing from `aprender-core`**.
- `prune/snapshot_tests.rs` imports only `crate::prune::…`.

The one control that would have been decisive — reverting `classification/multinomial.rs`
to the base blob and re-running — does not compile, because 04-04's `classify` calls the
`predict_logits` that refactor introduces. Reverting the whole task to run it was judged a
worse trade than recording the import-level proof.

Positive evidence that the refactor is behaviour-preserving where it matters:
`cargo test -p aprender-train --features setfit --lib setfit::` → **256 passed**, and
`cargo test -p aprender-core --features setfit --lib` → **14419 passed**. Those suites
include the probe-replay comparisons that check head logits and probabilities against
recorded artifact values.

**Action:** someone should run these four modules on a clean checkout of `main` to
establish whether they are pre-existing or environmental to this machine.
