# Phase 5 — Deferred Items (out-of-scope discoveries)

Discoveries made while executing Phase 5 plans that are NOT caused by this phase's
changes. Logged rather than fixed, per the executor scope boundary.

> Merge note: plans 05-04 and 05-06 each created this file independently in their own
> worktrees (an add/add conflict at wave-1 merge-back). Both sets of entries are kept and
> renumbered; nothing was dropped in favour of one side.

---

## D-ITEM-05-01: `aprender-serve` test target `driver_cpu` does not compile

**Found during:** plan 05-04, Task 3 (`cargo check --workspace --all-targets` run to prove
the new `AprenderError::ZeroVarianceDifferences` variant broke no downstream match).

**Symptom:** 12 × `E0063` in `crates/aprender-serve/tests/driver_cpu.rs`:

```
error[E0063]: missing field `query_pre_attn_scalar` in initializer of `GGUFConfig`   (×10)
error[E0063]: missing fields `post_attn_norm_weight` and `post_ffw_norm_weight`
              in initializer of `OwnedQuantizedLayer`                                (×2)
error: could not compile `aprender-serve` (test "driver_cpu") due to 12 previous errors
```

**Proven pre-existing, not caused by 05-04:**

- `query_pre_attn_scalar` was added to `GGUFConfig` on 2026-06-19 in `366f3c275`
  (*fix(serve): batched-GPU path crashed on every GQA model*, PMAT-841, #2125), which is
  an ancestor of this plan's base `350b08575`. The test target has been red since then.
- The workspace check output contains **zero** occurrences of `AprenderError`,
  `non-exhaustive` or `ZeroVarianceDifferences`, so the added error variant is not
  implicated. Adding it broke nothing: only `error.rs`'s own `Display` match is
  exhaustive over that enum.

**Why not fixed here:** it is in a different crate, on a surface plan 05-04 does not
touch, and the fix is to update a fixture-construction site for two struct fields added
by a GQA dispatch change — an `aprender-serve` concern with its own review context.

**Suggested owner:** an `aprender-serve` ticket. Note that `make tier2`/`tier3` and the
`workspace-test` required status check may already be masking this by not building
`--all-targets` for that crate; if so, that gap is the more important half of the fix
(CR-02: a gate that runs nothing passes).

---

## D-ITEM-05-02: 24 pre-existing failures in `cargo test -p aprender-train --lib`

**Found during:** plan 05-06, while running the full `aprender-train` lib suite for
regression coverage. The plan's own scoped verify is green — `--lib classify_trainer`
(330 passed) and `apr-cli --lib --features setfit finetune` (81 passed).

Failing modules, none of which plan 05-06 touches:

| Module | Count | Example |
|--------|-------|---------|
| `gpu::ledger::tests` | 12 | `test_reserve_and_release` — `assertion left == right failed: left: 0, right: 8000` |
| `gpu::guard::tests` | 8 | `test_guard_update_actual` |
| `gpu::wait::tests` | 1 | `test_timeout_when_full` |
| `prune::snapshot_tests::tests` | 3 | `snapshot_all_prune_methods` |

**Evidence they are not caused by 05-06:**

- 05-06 modifies `finetune/classify_trainer.rs` (+ its tests) and apr-cli command files.
  `gpu/ledger.rs`, `gpu/guard.rs`, `gpu/wait.rs` and `prune/snapshot_tests.rs` are
  untouched and reference neither `TrainResult` nor `TrainingConfig`.
- `test_reserve_and_release` fails in ISOLATION (`cargo test -p aprender-train --lib
  test_reserve_and_release` → 0 passed; 1 failed), so it is not a parallel-execution
  interaction with the new tests either.
- The assertion is a VRAM reservation figure (0 vs 8000 MB) on a host with no reservable
  GPU ledger state — an environment dependency, not a logic regression.

**Action:** needs its own ticket (GPU-ledger tests assume a reservable GPU / writable
ledger directory; the prune snapshot tests need their snapshots reviewed). Not Phase 5
work.

**Gate relevance:** these 24 failures are the baseline for Phase 5's post-merge test
gate. A wave that reports exactly these 24 has introduced no regression; any *additional*
failure is merge-induced and must be treated as such. Recording the count here so a later
wave cannot quietly absorb a new failure into "the usual 24".

---

## D-ITEM-05-14-A — `apr_reload.rs` is unformatted at HEAD (PRE-EXISTING, out of scope)

Surfaced by plan 05-14's `cargo fmt -p aprender-train -- --check` gate.

`crates/aprender-train/src/train/setfit/apr_reload.rs:331` produces a rustfmt diff (a
`let recorded: ProvenanceRecord = …` binding rustfmt wants on one line). It is
**pre-existing**, not caused by 05-14: the working tree during that plan contained only
`evidence.rs`, and `apr_reload.rs` is byte-identical to `b3386b063`.

Left alone per the scope boundary — only issues DIRECTLY caused by the plan's own changes
are auto-fixed. `rustfmt --check` on `evidence.rs` alone is clean, which is what 05-14's
acceptance criterion asks for ("clean for this file").

**Action:** whoever next runs a crate-wide `cargo fmt` on `aprender-train` should land the
reformat as its own commit rather than folding it into a feature diff. Worth confirming
first whether it is rustfmt-version drift on this host rather than a genuine omission —
if it is drift, reformatting here would make CI red instead.

## D-ITEM-05-14-B — `cargo test` output is rewritten on this host, and the plan's greps assume it is not

Measured during 05-14, recorded because it will bite every later plan in this phase whose
verify block reads a libtest count.

The `rtk` hook rewrites `cargo test` output into a one-line summary
(`cargo test: 313 passed, 3 ignored, 7661 filtered out (1 suite, 37.59s)`). There is **no
`test result: ok. N passed` line anywhere in the captured log** — `grep -c "test result"`
returned **0** on a run that exited **0**. Any verify block asserting that line fails on a
GREEN run, and any count floor fed from it reads 0.

**Workaround in use:** run through `rtk proxy`, which yields the raw libtest stream. 05-14
used it for every `cargo test`, and for `git status` / `git diff` / `git log` in its
checks — the same standing Phase 2 ruling that porcelain-emptiness assertions must go
through `rtk proxy`.

**Action:** plans 05-03, 05-07, 05-09..05-13 should be read as the `rtk proxy` form
wherever they grep a libtest line. Worth deciding once, at phase level, rather than
rediscovering per plan.

## D-ITEM-05-14-C — `make test` cannot compile the workspace on macOS (PRE-EXISTING, blocks the post-merge gate)

Surfaced by the wave-1 post-merge test gate, which is `make test`
(`cargo nextest run --workspace -j 2`). It exited non-zero having run **zero** tests: the
`cargo test --no-run --workspace` compile aborted. Two independent, unrelated breaks:

1. `crates/aprender-profile/examples/{validate_golden_trace,process_tracer_demo}.rs` import
   `renacer::{validate, process_tracer}`, which are `#[cfg(target_os = "linux")]` in
   `crates/aprender-profile/src/lib.rs:86-87`. On macOS the imports do not resolve
   (`E0432`), plus an `E0282` behind them.
2. `crates/aprender-serve/tests/{gguf_config_coverage,gguf_kv_cache_coverage}.rs` build
   `GGUFConfig { .. }` literals that omit the `query_pre_attn_scalar` field (`E0063`, 9
   sites). The field exists in the struct; the test initializers were never updated.

**Proven pre-existing, not caused by 05-14** — three independent controls:
- The identical `aprender-profile` errors reproduce from a detached worktree at the base
  commit `b3386b063` (`cargo check -p aprender-profile --examples` → exit 101).
- All four failing files are byte-identical between `b3386b063` and the merge commit
  (`git rev-parse <rev>:<path>` matches on each).
- The merge changed exactly 4 files: 3 planning docs and
  `crates/aprender-train/src/train/setfit/evidence.rs`. Neither `aprender-profile` nor
  `aprender-serve` declares an `aprender-train` dependency, so no path exists by which the
  change could reach them.

**Why the wave still closed:** the merged tree is byte-identical to the tree the executor
tested in its worktree (`git diff c69a5fcba HEAD` is empty — the merge added no content
beyond the executor's own commits), and wave 1 ran a single plan, so there is no cross-plan
integration surface for the gate to detect. The gate's purpose was satisfied by that
equivalence, not by waiving it.

**Action:** these two breaks make the project's standard test gate unrunnable on a macOS dev
host — every later wave in this phase inherits it, and any plan whose verify block shells
out to `make test` will read a non-zero exit that has nothing to do with its own work. Fix
by feature-gating the two `aprender-profile` examples to Linux (`required-features` or a
`#![cfg]` guard) and adding the missing field to the 9 `GGUFConfig` literals. Until then,
scope post-merge gates to the crates a plan touches.

## D-ITEM-05-14-D — the dev host is at 95% disk with a 117 GB `target/debug`

Surfaced when a full-workspace control compile exhausted the volume mid-run (`ENOSPC`,
which also killed the tool harness's own output writes until space was reclaimed).

`df -h /` reports 926Gi total, ~760Mi free. `target/debug` alone is 117 GB (`target/release`
is 1.6 GB).

**Action:** this phase's remaining waves compile heavily and waves 05-11/05-12 execute 80
benchmark cells that write per-row artifacts under `benchmarks/tweeteval-stance/`. At
~760Mi headroom those will fail on write, and an ENOSPC failure mid-benchmark is
indistinguishable from a measurement failure — exactly the confusion the claims gate exists
to prevent. Reclaim space before dispatching wave 2.

## D-ITEM-05-05-A — 24 `aprender-train` lib tests are red before this plan touched anything

Surfaced by plan 05-05's post-implementation control run of the FULL crate suite
(`CARGO_INCREMENTAL=0 cargo test -p aprender-train --lib --features setfit`, status captured
directly: rc=101, `7969 passed; 24 failed; 17 ignored`). None is reachable from this plan's
change: `bench_row` is a new leaf module under `train::setfit`, and `git diff` against this
plan's base is EMPTY for both `crates/aprender-compute/` and every failing file's crate path.
Two independent pre-existing causes, not one:

**(a) 21 non-hermetic GPU tests** — `gpu::guard::*` (8), `gpu::ledger::*` (12), `gpu::wait::*`
(1). `default_ledger_path()` (`crates/aprender-train/src/gpu/ledger.rs:33-38`) resolves to
`~/.cache/entrenar/gpu-ledger.json` — a MACHINE-GLOBAL path, not a per-test temp dir. Every
process on the box shares one ledger file, so two concurrent `cargo test` runs (this phase
dispatches parallel executors in separate worktrees, on one machine) contend on it. Verified
not to be an ordering interaction with the new tests: running `gpu::ledger` ALONE still fails
(`rc=101, 25 passed; 12 failed`). Failures are assertion mismatches on reserved/available MB
(`left: 17000, right: 10000`), which is the signature of state another process wrote.

**(b) 3 stale insta snapshots** — `prune::snapshot_tests::{snapshot_all_prune_methods,
snapshot_pipeline_stages, snapshot_schedule_validation_errors}`. The strongest evidence that
these predate this plan is that their `.snap.new` rejection artifacts are already TRACKED AND
COMMITTED in git (`git ls-files crates/aprender-train/src/prune/snapshots/ | grep snap.new`
returns three paths) and are UNMODIFIED by this plan's run. A committed `.snap.new` is a
recorded, unresolved snapshot disagreement.

**Action:** neither is this plan's to fix (SCOPE BOUNDARY — auto-fix only what the current
task's changes caused). (a) needs the ledger tests to take a per-test `with_path(tempdir)`,
which the type already supports (`AccessLedger::with_path`, `:139-141`); leaving it means any
plan in this phase that runs the full crate suite reads a red exit that has nothing to do with
its work, and any plan that runs it CONCURRENTLY with a sibling executor makes it worse. (b)
needs someone to review the three diffs and either accept (`cargo insta accept`) or fix the
regression, then delete the committed `.snap.new` files — a committed rejection file makes the
next reader think the disagreement is expected.

**Consequence for gating:** until (a) and (b) are fixed, a Phase 5 plan touching
`aprender-train` must scope its verification to a test FILTER over its own module and quote the
matched count, not to the whole-crate suite. 05-05 did exactly that
(`... --features setfit bench_row` -> rc=0, 24 passed).

## D-ITEM-05-05-B — `cargo clippy -- -D warnings` cannot pass on this workspace

Measured by plan 05-05 (`CARGO_INCREMENTAL=0 cargo clippy -p aprender-train --lib --features
setfit -- -D warnings`, status captured directly: rc=101). Every finding is in
`crates/aprender-compute/` — dead code (`compute_chunk_scalar`, `pack_a_block_generic`,
`pack_b_block_nr16`, `extract_q6k_values`, `NT_STORE_THRESHOLD_BYTES`, `PREFETCH_DISTANCE`,
...), unused imports (`NeonBackend`, `SUPER_BLOCK_BYTES`/`SUPER_BLOCK_SIZE`, `MR_512V2`/
`NR_512V2`) and unused variables (`mr_block`, `nr_block`) in the BLIS/SIMD kernels. `git diff`
against this plan's base for `crates/aprender-compute/` is EMPTY, so none of it is this plan's.
Findings attributable to `crates/aprender-train/src` in that run: **zero** (grep count 0).

**Action:** not this plan's to fix, and it is a genuine hole rather than noise — CLAUDE.md
lists `cargo clippy -- -D warnings` as a standing gate and `ci.yml` runs a lint job, so a gate
this red is either not actually running with `-D warnings` on this path or is scoped narrower
than the docs claim. Worth resolving as its own ticket: either clear the kernel crate or
record explicitly which crates the `-D warnings` gate covers. A plan-level clippy check in this
phase should scope to `crates/aprender-train/src` and read the finding count for its own files.
