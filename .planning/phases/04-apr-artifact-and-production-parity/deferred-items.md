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

---

## D-04-09-A — `cargo check -p apr-cli --no-default-features` does not compile (pre-existing)

**Found by:** plan 04-09, while verifying the Cargo.toml note plan 04-09 Task 1 step 1
requires, which names that command as the leg SAFE-02's gating evidence is read from.

**Measured twice** — on 04-09's tree, and again with the BASE manifest restored
(`git checkout f2824611a -- crates/apr-cli/Cargo.toml`), which is a true base measurement for
a lib-only check because 04-09's diff touches no `src/` file at all. Identical result both
times, same four errors:

```
$ cargo check -p apr-cli --no-default-features > log 2>&1; echo "rc=$?"
rc=101   error: could not compile `apr-cli` (lib) due to 4 previous errors
```

The four are `inference`-gated code that is not `cfg`-gated:

| file | error |
| ---- | ----- |
| `src/commands/explain.rs:231` | `realizar::safetensors::find_sibling_file` — unlinked crate |
| `src/commands/explain.rs:344` | same |
| `src/commands/diff_05_aprt_stage.rs:100` | `realizar::inference_trace::save_tensor::read_tensor_file` |
| `src/lib.rs:63` | re-exports `commands::serve::auth::apply`, which is `#[cfg(feature = "inference")]` |

**Not fixed here.** All four are in files 04-09 does not own, the fix is a `cfg` decision on
the `explain`/`diff` command surface, and 04-15 is editing `crates/apr-cli/` in the same wave.

**Action for 04-10 (gates) and 04-11 (audit): do NOT wire `--no-default-features` as a leg.**
It cannot distinguish a regression from this standing red. The leg that is green, and the one
04-06 and 04-07 actually used, is `cargo check -p apr-cli --all-targets` with DEFAULT features
(`setfit` OFF, `inference` ON) — the ungated build is what proves the `setfit` gating.
04-09's Cargo.toml carries the same note beside the dev-dependency block.

---

## D-04-10-A — `aprender-serve`'s minimal TEST build is red; the LIBRARY check is green

**Found by:** plan 04-10, while measuring the SAFE-02 run legs for the feature matrix. The
CHECK cells for `aprender-serve` at profiles (a) and (b) are both rc=0, so the discrepancy is
specific to the test target and would not have surfaced from a build-only matrix.

**Measured** (status captured directly off cargo, never through a pipe):

```
$ cargo check -p aprender-serve --no-default-features                        rc=0
$ cargo check -p aprender-serve --no-default-features --features setfit      rc=0
$ cargo test  -p aprender-serve --no-default-features --lib setfit           rc=101
$ cargo test  -p aprender-serve --no-default-features --features setfit --lib setfit  rc=101
```

**Cause:** `#[cfg(test)]` code imports feature-gated items unconditionally —
`crate::gguf::OwnedQuantizedModelCached`, `crate::gguf::OwnedQuantizedModelCachedSync`,
`crate::gguf::DequantizedFFNWeights`, `crate::gguf::DequantizedWeightCache`, `crate::gpu`, and
the `crate::api` GPU request/response types (`GpuBatchRequest`, `GpuStatusResponse`, ...). The
library compiles without `server`/`gpu`; only the test target needs them.

**Pre-existing and unrelated to setfit.** The two runs above produce the SAME first errors with
the feature OFF and ON, and the first four errors are `gguf`/`gpu` imports that setfit does not
touch. This is the same class as D-04-09-A one crate over: gated code that is not `cfg`-gated,
here inside `#[cfg(test)]` rather than in `src/`.

**Action taken by 04-10:** `setfit-feature-matrix` runs `aprender-serve` at profile (c) only,
and says so in the recipe. The (a)/(b) CHECK cells are still wired — they are green and they
are what the SAFE-02 build claim needs. `setfit-serve-tests` is likewise scoped to
`--features setfit --lib setfit` (10 passed / 0 failed).

**Action for whoever fixes it:** the fix is a `cfg` decision on `aprender-serve`'s test module
imports, not on setfit. Once green, add the two missing RUN cells to `setfit-feature-matrix`
beside the profile-(c) leg. Do NOT wire the whole-crate `-p aprender-serve --lib` suite in the
process — that is a separate standing red (D-04-08-A, 51 failures from one overflow at
`contract_gate.rs:428`).

---

## D-04-11-A — two PRODUCTION mutation survivors in `api/setfit_handlers.rs`, one of them surprising

**Found by:** plan 04-11's mutation gate. `crates/aprender-serve/src/api/setfit_handlers.rs` is
04-08's file, not 04-11's, so neither survivor was fixed here.

Invocation (note the two corrections — see 04-11-SUMMARY deviations 4 and 5):

```
cargo mutants --no-times --timeout 180 --package aprender-serve \
  --features setfit --cargo-arg=--lib \
  -f crates/aprender-serve/src/api/setfit_handlers.rs -- setfit
```

Baseline `ok`. The run was INTERRUPTED at ~68 min before finishing all 101 mutants, so there is
**no score** — but the survivors it did report are real and are recorded here rather than lost.

**Nine survivors reported; SEVEN are proven-equivalent by construction** — they mutate code inside
the file's own `#[cfg(test)]` module (five `tests::<fn> -> ()` and two in `fixture::Filler::next`).
A test suite cannot detect the deletion or alteration of one of its own tests, so these are not
coverage gaps. This is the F-05 self-scan class at the mutation tier: a `-f <file>` glob mutates
the file's tests along with its production code, and those survivors must be triaged out rather
than counted against the score.

**The two that matter:**

| # | Mutation | Source | Assessment |
|---|----------|--------|------------|
| A | `replace AppState::has_setfit_model -> bool with false` | `:70-72`, body is `self.setfit_model.is_some()` | **Real gap.** Under the `setfit`-filtered suite — the same filter `make setfit-serve-tests` uses — nothing pins this returning `true`. A readiness path that always reported "no classifier resident" would pass. |
| B | `replace > with == in setfit_classify_handler` | `:158`, `if request.texts.len() > MAX_BATCH_TEXTS` | **Real, and it should not have survived.** A co-located test is *named* `setfit_classify_refuses_a_batch_one_over_the_contract_bound`, i.e. `len == MAX+1` — the exact input that distinguishes `>` from `==`. Either that test does not exercise the branch its name claims, or it is not reached under this filter. **Not diagnosed here; one failing input is an anecdote (CLAUDE.md rule 6).** |

B is the more valuable finding and is precisely what mutation testing is for: a test whose NAME
asserts boundary coverage, beside a boundary mutant that lives. That is "labelling by intent"
(CLAUDE.md rule 2) one level down — at the test name rather than at the run.

**Action for the owner of `api/setfit_handlers.rs` (04-08's surface, or Phase 5):**
1. Re-run B's mutant alone and read why the named test does not kill it.
2. Add an assertion pinning `has_setfit_model() == true` on a state that has a resident model.
3. When re-running, exclude `#[cfg(test)]` functions from the mutation set so the score is not
   diluted by seven survivors that are equivalent by construction.

---

## D-04-11-B — the full mutation gate does not fit a single session: MEASURED projection

**Found by:** plan 04-11, which was asked to run a scoped mutation gate over all four crates.

Denominators, enumerated with `cargo mutants --list` (not estimated):

| Crate | Files | Mutants |
| ----- | ----- | ------- |
| aprender-core | `setfit/artifact.rs` (311), `setfit/classify.rs` (69) | 380 |
| aprender-train | `bundle.rs` (188), `lock.rs` (81), `config.rs` (63), `apr_reload.rs` (14), `apr_codec.rs` (12) | 358 |
| aprender-serve | `api/setfit_handlers.rs` | 101 |
| apr-cli | `setfit_train.rs` (25), `predict.rs` (20), `setfit_io.rs` (6) | 51 |
| **total** | | **890** |

Two timing measurements on the SMALLEST crate:

- `--shard 1/25` (4 mutants + baseline): **453 s**, 4/4 caught.
- full 101 mutants: **interrupted at 4,094 s (68 min) without finishing**.

From the second: the average cost per mutant, baseline included, is **> 40.5 s** — a lower bound,
since the run had not completed. `aprender-core` and `aprender-train` carry far heavier builds and
test suites than `aprender-serve`, so their per-mutant cost is strictly worse.

**Projection: ≥ 10 hours of wall clock for all 890 mutants**, ≥ 8.9 h for the 789 not yet
attempted, plus four baselines. That is beyond a single execution session, and the run that was
attempted was killed rather than completing.

**Reported and stopped rather than shrinking scope silently**, per the plan's own instruction and
the Phase 3 compute-budget precedent. **Recommendations for whoever runs it:**
1. Run it as a scheduled/nightly job per crate, not inside a plan execution.
2. Pass `--features` / `--cargo-arg`, never `-- --features` (deviation 4) — otherwise the whole
   run happens with `setfit` compiled out.
3. Pass `--cargo-arg=--lib` (deviation 5) or the baseline cannot build on `aprender-serve`.
4. Exclude `#[cfg(test)]` functions, or ~7 of every 9 survivors will be equivalent by construction.

## D-04-11-A — `setfit-api-boundary` excluded from CI (user ruling, 2026-08-15)

**Status:** deferred by explicit user decision at the 04-11 checkpoint. Excluded on QUOTING
grounds, not value grounds.

`setfit-api-boundary` is a `cargo tree` dependency-direction gate implemented as a Make `for` loop
with `$$`-escaped variables and single-quoted patterns. It cannot be embedded in the CI step's
single-quoted `bash -c '...'` without rewriting the quoting — and that rewrite is precisely the
drift the gate exists to detect. It continues to run locally via `make setfit-api-boundary`
(orchestrator-measured rc=0).

**Risk explicitly accepted by the user:** a Linux-only dependency-closure regression — one
introduced via `cfg(target_os)` so that the Linux closure differs from macOS — would NOT be caught,
because the gate now only ever runs on developer machines. This is the one class of regression a
local-only run cannot cover, and it is the counter-argument the executor recorded in the patch
beside the exclusion.

**If revisited:** the clean fix is to extract the loop into `scripts/setfit_api_boundary.sh` and
have both the Make target and a CI step invoke that script, so no quoting rewrite is needed. Note
`bashrs` (which CLAUDE.md mandates over shellcheck) is NOT installed on this host, so any new
script would need linting elsewhere.

## D-04-12-A — 04-REVIEW.md's non-Critical findings are open and now TRACKED (W-03)

The phase-04 code review returned **2 Critical, 6 Warning, 5 Info**. Both Criticals were fixed and
committed at `0fb47958f`:
- **CR-02 (security)** `POST /v1/classify` was mounted WITHOUT the AuthGate, so with `APR_API_KEY`
  set the classifier server was unauthenticated behind `CorsLayer::permissive()` — and since
  `AuthGate::from_env()` was never called, its "routes are unauthenticated" warning never printed
  either. Fixed with the same `auth::layer` the APR path already used.
- **CR-01 (correctness)** `apr eval --split test` scored artifact-head indices against dataset
  indices with no label-map gate, so a mismatched corpus produced a confidently wrong accuracy
  rather than an error — the exact failure the validation evaluator refuses by name
  (`apr_evaluate.rs`, `LabelMapMismatch`). Gate added in `run_test`, with a source guard asserting
  ORDER (not mere presence) plus non-vacuity on both sides, shown red by deleting the gate.

**The remaining 6 Warning + 5 Info are NOT fixed.** They were previously untracked, which is how
review findings quietly die. The two worth doing first, both write-path defects:

- **WR-02** (verifier re-confirmed present at `eval/setfit.rs:620-636`): `write_lock`'s temp file is
  predictable, symlink-following, and opened with `.create(true)` rather than `create_new` — while
  this repo's own precedent (`setfit_train::temp_path` + `create_new`) does the opposite.
- **WR-01**: `atomic_write` / `write_lock` both clobber through a check -> `fs::rename` window that
  their own doc comments claim to have closed.

Full findings with file:line and mechanism are in
`.planning/phases/04-apr-artifact-and-production-parity/04-REVIEW.md`.

## D-04-12-B — 04-11's mutation gate never produced a score (verifier gap, not F-10)

The only phase-04 gap NOT caused by F-10. One crate attempted, interrupted at 68 minutes against a
measured >=10 h projection for the lightest crate alone; cargo-mutants prints its summary only at
the end, so **no per-crate or aggregate score exists**. Two real production survivors in
`setfit_handlers.rs` remain undiagnosed — the notable one is `>` -> `==` at
`texts.len() > MAX_BATCH_TEXTS` surviving beside a test named
`..._refuses_a_batch_one_over_the_contract_bound`, i.e. the exact distinguishing input.

Two recipe defects were found and fixed en route, and both invalidate any earlier confidence at
this tier: `-- --features setfit` never reached cargo (so setfit was compiled OUT — F-04 vacuity),
and cargo-mutants had no baseline at all for `aprender-serve` (fixed with `--cargo-arg=--lib`).
