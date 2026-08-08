# Phase 1 — Deferred Items

Out-of-scope discoveries logged during plan execution. **Not fixed** — they are
pre-existing and unrelated to the changes that surfaced them.

Plans 01-01 and 01-02 executed in parallel and independently surfaced D1 and D2.
Two agents reaching the same finding from different code paths raises confidence
that these are real and reproducible, not artifacts of one agent's environment.

## From plan 01-01 (2026-08-08)

### D1. `scripts/check_include_files.sh` is a no-op on macOS

*Independently confirmed by plan 01-02.*

The script uses `grep -P` / `grep -oP` (PCRE), which BSD grep rejects:

```
grep: invalid option -- P
OK: All 0 include!() files are tracked by git
```

It then reports success over **zero** files, exiting 0. CB-510 exists because a
gitignore pattern silently hid `include!()` sources from git and crates.io; on
macOS this guard cannot detect that recurrence — it is theater there. It
presumably works on the CI Linux runner, so the drift is platform-split rather
than total. CLAUDE.md documents the repo as having 562 `include!()` files; the
guard sees 0 of them on darwin.

Discovered while adding four new `include!()` files under
`crates/aprender-core/src/autograd/ops/`. Those were verified by hand instead
(`git check-ignore -v` exits 1 for each; `git ls-files` lists all four).

Fix direction: `grep -Eo` with a POSIX-ERE equivalent, or `rg` if it is an
accepted dependency. Whichever is chosen, re-run the must-match / must-not-match
case table on macOS **and** Linux (CLAUDE.md verification rule 7) — the pattern
is exactly the kind that has been wrong five times before.

### D2. `cargo clippy -p aprender-core -- -D warnings` fails on `aprender-compute`

*Independently confirmed by plan 01-02.*

`aprender-compute` is a workspace path dependency, so command-line `-D warnings`
applies to it too. It carries ~20 pre-existing findings (unreachable
expressions, unused imports/constants, unused variables, dead functions in
cfg-gated NEON/AVX paths inactive on this target), so the invocation named in
the 01-01 and 01-02 plans' `<verification>` blocks exits 101 regardless of the
state of `aprender-core`.

`aprender-core` itself is clean: `cargo clippy -p aprender-core --lib --tests`
exits 0 and reports zero findings in the touched files of either plan.

Fix direction: clean `aprender-compute`, or scope the phase gate to
`--lib --tests` on the crate under change. Do NOT paper over it with a
workspace-level allow — that would hide future regressions in compute kernels.

### D3. `generated_contracts.rs` had drifted badly from `contracts/`

Re-running the sanctioned regeneration command
(`pv codegen contracts/ -o crates/aprender-core/src/generated_contracts.rs`)
produced a ~31k-line diff **before** this plan's contract was accounted for:
2415 macros at `HEAD` versus 2903 after regeneration, with 18 macros dropped
entirely. So `pv codegen` had not been re-run after a long run of contract
edits.

No consumer was broken — all 18 dropped macros were verified to have zero call
sites, `cargo check -p aprender-core` exits 0, and the full lib suite is green
(13,972 passing before this plan's tests were added). But nothing in the repo
detects the drift.

Fix direction: a tier3/CI check that regenerates into a temp file and fails on
any diff, so "the generated file matches the contracts" becomes falsifiable
rather than assumed.

### D4. Equation-name collision: `mse_loss`

`loss-functions-v1` and `setfit-encoder-conformance-v1` both declare an equation
named `mse_loss`. `pv codegen` derives macro names from the equation name alone,
so both emit `contract_pre_mse_loss!` / `contract_inv_mse_loss!` into one
`#[macro_use]` module and the later definition shadows the earlier.

Contained for now: the 01-01 contract's preconditions and invariants were chosen
so the emitted macros are **byte-identical** to the `loss-functions-v1` ones,
making the shadowing a semantic no-op (verified at
`generated_contracts.rs:20334` and `:29825`). A YAML comment on the equation
records the constraint.

This is a latent trap for any future contract, not a defect in either contract.

Fix direction: have `pv codegen` either namespace macros by contract stem or
reject duplicate equation names across contracts outright. The current behaviour
lets one contract silently redefine another's assertions.

## From plan 01-02 (2026-08-08)

### D5. `cargo check --workspace` fails on darwin (intentional platform gate)

`crates/aprender-profile` hard-stops via
`#[cfg(not(target_os = "linux"))] compile_error!("renacer requires Linux (ptrace syscall tracing)")`.

This is an intentional platform gate, not a defect — but it means the
`cargo check --workspace` command named in plan verification blocks can never
pass on macOS. Verified instead with
`cargo check --workspace --exclude aprender-profile` (exit 0, all other 77
crates clean).

Fix direction: phase verification blocks targeting macOS developers should name
the `--exclude aprender-profile` form, or the repo should provide a
`just check` recipe that applies the exclusion per-platform.

### D6. Pre-existing warnings in `aprender-core` test builds

- `f16_first_u16` never used — `serialization/safetensors_tests_core.rs:571`
- unused `#[must_use]` return — `models/bert/embeddings.rs:128`

Unrelated files, not caused by either plan's changes.
