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

---

## D-04-08-A — 51 `aprender-serve --lib` tests fail on ONE arithmetic overflow in `contract_gate.rs:428`

Discovered by plan 04-08, wave 7. **Not fixed** — `contract_gate.rs` is outside this
plan's files and the fix needs a decision about the intended width.

`cargo test -p aprender-serve --features setfit --lib` → `15389 passed; 51 failed`. Every
one of the 51 panics at the SAME line, and the message is the same:

```
thread '...' panicked at crates/aprender-serve/src/contract_gate.rs:428:21:
attempt to multiply with overflow
```

The failing tests are `apr_transformer::tests::{q4k_bytes_q6k, tests_08, tests_10}::*` (49),
`contract_gate::tests::test_small_model_passes_resource_check` (1) and
`convert::convert_tests_q4k_converter::test_q4k_convert_roundtrip_loadable` (1). They are
one defect with 51 witnesses, not 51 defects.

**Not caused by this plan.** This plan's diff touches `api/` (the `AppState` slot, the
route, `HealthResponse`), `Cargo.toml` and test files. It does not touch `contract_gate.rs`,
`apr_transformer.rs` or `convert/`, and none of those modules names `AppState`,
`HealthResponse` or anything under `setfit`. The panic is an integer multiply in a
resource-estimation path reached from model construction — a code path with no edge to the
HTTP surface.

**Also pre-existing and unrelated:** `cargo check -p aprender-serve --tests` is red at the
base commit for two integration targets that have drifted from their types —
`tests/ffn_coverage.rs` (5 × missing `OwnedQuantizedLayer::{post_attn_norm_weight,
post_ffw_norm_weight}`) and `tests/gguf_extended_coverage.rs` (14 × missing
`GGUFConfig::query_pre_attn_scalar`). 04-08 fixed only the `HealthResponse` initializers its
own change broke (8 sites, 3 files) and left these 19 alone.

**Action for 04-10 (gates):** an `aprender-serve` test leg must either fix
`contract_gate.rs:428` first or be scoped by filter, because a whole-crate `cargo test
-p aprender-serve --lib` cannot go green today and so cannot distinguish a regression from
the standing red. 04-08's own leg is scoped: `--lib setfit` (see its SUMMARY).
