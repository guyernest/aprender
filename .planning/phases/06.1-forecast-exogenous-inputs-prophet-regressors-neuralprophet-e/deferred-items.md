# Deferred items — phase 06.1

Out-of-scope discoveries logged rather than fixed, per the executor scope boundary: only
issues DIRECTLY caused by a task's own changes are auto-fixed. Each entry below was
MEASURED to pre-date this phase, not assumed to.

## D1 — `cargo fmt --all -- --check` is red on the committed tree

**Found during:** plan 06.1-01, Task 1 (running the Task 3 gate early).

**Measurement.** `cargo fmt --all -- --check` reports diffs in six files beyond this
phase's own. Three are the uncommitted SetFit work in the working tree
(`crates/aprender-core/src/setfit/encoder.rs`, `.../import_tests.rs`); the other three are
COMMITTED and byte-identical to `HEAD`:

```
crates/aprender-image/src/lib.rs
crates/aprender-image/src/spectral.rs
crates/aprender-image/src/tests.rs
crates/aprender-mcp-chronos/build.rs
crates/aprender-mcp-chronos/src/lib.rs
```

`git diff --quiet HEAD -- crates/aprender-image crates/aprender-mcp-chronos` returns 0, so
these are properties of the committed tree, not of any edit made here.

**Impact on this plan.** Plan 06.1-01 Task 3's `cargo fmt --all -- --check` acceptance
criterion CANNOT pass on this tree for reasons unrelated to the phase. Every file this
phase touched IS fmt-clean, verified with
`rustfmt --edition 2021 --check <the ten touched files>` (rc=0).

## D2 — `cargo clippy -p aprender-forecast -p aprender-mcp-forecast --all-targets -- -D warnings` is red on the committed tree

**Found during:** plan 06.1-01, Task 1.

**Measurement.** The command exits 101 with 18 errors, and EVERY ONE is in
`crates/aprender-compute/`:

```
backends/q4k/gemv/mod.rs, backends/q4k/gemv/scalar.rs, backends/q6k/gemv.rs,
blis/backend_selection.rs, blis/compute.rs, blis/elementwise.rs, blis/gemv.rs,
blis/packing.rs, brick/quant_ops/mod.rs, brick/simd_config/mod.rs, hardware/mod.rs,
vector/ops/rounding.rs
```

`-D warnings` propagates to workspace path dependencies, so pre-existing `unused_imports`,
`unreachable_code`, `unused_variables` and `dead_code` warnings in `aprender-compute`
become errors and the build fails BEFORE either selected crate is linted.
`git diff --quiet HEAD -- crates/aprender-compute` returns 0 — unchanged by this phase, and
`aprender-compute` does not depend on `aprender-forecast`, so no edit here can reach it.

**The scoped measurement that DOES work**, and which this phase used instead:

```
cargo clippy -p aprender-forecast -p aprender-mcp-forecast --all-targets --no-deps -- -D warnings
```

`--no-deps` lints only the selected packages. It exits 0, and it is not vacuous — it caught
two real defects in this plan's own code (five `clippy::disallowed_methods` hits from
`serde_json::json!`'s internal `unwrap`, and one `clippy::range_plus_one`), both fixed.

**Recommendation.** Either clean `aprender-compute`'s 18 warnings, or change the phase's
clippy acceptance criterion to the `--no-deps` form. The current criterion can never pass.

## D3 — the README drift gate is red on the committed tree

**Found during:** plan 06.1-01, Task 3.

**Measurement.** `cargo test -p aprender-core --test readme_contract` fails two tests:
`test_readme_contract_count_matches_workspace` and
`test_readme_crate_count_matches_workspace`.

| Claim | README says | Actual |
|---|---|---|
| provable contracts | 1790 | 1791 |
| workspace crates | 86 | 87 |

**Proof this phase did not cause it.** The contract count is IDENTICAL at the phase base and
at HEAD:

```
git ls-tree -r --name-only c850e62aaffa5e42e9ed644f9c1c81547cca562f contracts/ | grep -c '\.yaml$'  -> 1791
git ls-tree -r --name-only HEAD contracts/ | grep -c '\.yaml$'                                       -> 1791
```

This phase MODIFIED two contracts and ADDED none
(`git diff --name-status <base>..HEAD -- contracts/` shows two `M` lines, no `A`), and added
no crate. Both numbers drifted before the phase started.

**Not fixed here deliberately.** Editing the two README numbers would be a two-line change,
but it is out of scope and would also paper over whatever added the 87th crate without
updating the gate. It belongs to whoever landed that crate.

## D4 — `regressor_prior_scale_min` is a representability bound, not the usability bound its rationale claimed

**Found during:** plan 06.1-01, Task 2. **This one is a finding about the plan itself**, and
is written up in full in `06.1-01-SUMMARY.md`. Summarised here so the ledger is complete.

The plan justified the floor by asserting that `f64::MIN_POSITIVE` yields "a NaN that
propagates through L-BFGS into every response field and serialises as JSON null".
MEASURED across 18 configurations and three series shapes: no response field is ever
non-finite. `Model::objective`'s existing `if self.guard && !f.is_finite() { return 1e300 }`
absorbs the 0/0. The real failure is quieter — L-BFGS runs ZERO iterations and the regressor
contributes exactly nothing, while the door returns a normal-looking forecast.

The degenerate region is ~145 decades wider than the floor: every `prior_scale <= 1e-9`
measured 0-1 iterations with a zero contribution; `>= 1e-7` fits normally. The shipped
constant (1e-153, as the plan specifies) satisfies the plan's acceptance criterion (finite
objective and beta) but does not make the fit meaningful.

**Deliberately NOT changed here:** the value is a published contract constant that plan
06.1-03 is specified to build on, so moving it is a decision for a human. What WAS done: the
false rationale is corrected in both the contract and the Rust doc comment, and
`forecast::tests::the_prior_scale_floor_is_a_representability_bound_not_a_usability_bound`
now pins the measured behaviour so it cannot become folklore.
