# Phase 4 — Orchestrator Notes

Cross-plan findings surfaced during execution that no single plan owns. Each is verified,
not reported second-hand. Wave-8 (04-10) and wave-9 (04-11) executors MUST read this.

---

## F-01 — `contracts/aprender/binding.yaml` is an ORPHAN file (BLOCKING for 04-10/04-11)

**No plan declares it in `files_modified`; no plan even mentions it.** Verified by scanning the
frontmatter of all 16 plans. Yet:

- 04-01 had to edit it (undeclared, but measured as required: rc=1 with 15 `BIND-001` errors
  without the entries, rc=0 with 15 `BIND-004` warnings after).
- 04-02 deferred a status flip to it because it is shared across concurrently-running agents.

All 15 equations are `status: pending`. `make contract-audit-phase4` currently exits 0 with 15
`BIND-004` warnings (pending is a WARNING, not an error). As plans land, these statuses drift
further from reality, and 04-11's closing requirements audit would report 15 pending equations
for work that is largely done.

**Do NOT blind-flip these.** Verified as of wave 2 (`f91d79279`):

| equation | bound function | state |
|---|---|---|
| `artifact_storage_map` | `write_setfit_apr` | `pub fn`, artifact.rs:576 — real |
| `canonical_tensor_names` | `canonical_name_for_hf` | `pub fn`, artifact.rs:966 — real |
| `architecture_derived_tensor_set` | `expected_tensor_names` | `pub fn`, artifact.rs:950 — real |
| `nullable_path_allowlist` | `first_unallowed_null_path` | **PRIVATE** fn, artifact.rs:1083 |

The fourth is private, so flipping it to `implemented` asserts something a reverse-coverage gate
may not resolve. Its equation also spans two crates (the walk in 04-02, the completeness gate in
04-13), so a single binding row under-describes it.

**Action for 04-10:** add `contracts/aprender/binding.yaml` to `files_modified`, take ownership of
status flips, and decide per-equation whether the bound function needs `pub` (or `pub(crate)` plus
a `--crate-dir` reverse-coverage run) before claiming `implemented`.

---

## F-02 — `pv lint <file>` is a VACUOUS gate

Independently confirmed with the pinned binary (`./target/release/pv`):

- `pv lint contracts/setfit-apr-v1.yaml` → `Summary: 0 errors, 0 warnings, 0 suppressed` /
  `Result: PASS`, with gate 7 skipped and gate 8 reporting `0 edges`. It validates nothing.
- `pv lint contracts/` → `Summary: 0 errors, 1010 warnings, 0 suppressed, 73 new`.

`pv validate <file>` IS a real gate (`0 error(s), 0 warning(s) / Contract is valid.`).

**Never cite `pv lint <file>` as evidence.** Use `pv validate <file>` or `pv lint <dir>`.
`pv` is not on PATH — use `./target/release/pv`, pinned.

---

## F-03 — the plans' clippy command is a gate that can never pass

`cargo clippy -p aprender-train --features setfit -- -D warnings` → **rc=101**, with 23 warning
lines from `crates/aprender-compute/src/blis/*` and zero findings in the code under test.
It fails on a dependency's pre-existing debt, so it cannot distinguish "my code is clean" from
"never linted."

`cargo clippy -p aprender-train --features setfit --no-deps -- -D warnings` → **rc=0**.

04-13 proved `--no-deps` is still a working gate by planting a deliberate `useless_format` probe
and observing RED. Both 04-13 and 04-14 hit this independently.

**REFINEMENT (verified after wave 4, from 04-04's D-04-04-A):** `--no-deps` rescues
`aprender-train` but **NOT `aprender-core`**, which has a pre-existing error of its own:

```
cargo clippy -p aprender-core --features setfit --no-deps -- -D warnings   -> rc=101
error: unreachable expression
  --> crates/aprender-core/src/demo/reliable/performance.rs:126:5
```

It is **arm64-only and pre-existing**: on aarch64 the `#[cfg(target_arch = "aarch64")] { return
"NEON" }` makes the trailing `"Scalar".to_string()` unreachable. Last touched by `bb519c6eb`
(APR-MONO), **zero** phase-4 commits touch that file. So on this host `aprender-core` exits 101 on an
empty diff — the gate cannot distinguish clean from dirty there either.

04-04's workaround is the right shape: a **path-scoped** assertion over the setfit surface only,
proven non-vacuous by planting a `useless_format` (0 → 1). Do not scope by crate.

**Action for 04-10:** every clippy leg must carry `--no-deps`, AND `aprender-core` legs must
additionally be path-scoped to the setfit surface or they fail on unrelated pre-existing arm64 debt.
Do not "fix" either by removing `-D warnings`.

---

## F-04 — zero-match test filters exit 0 (CR-02 vacuity), hit three times

1. 04-13: `setfit::bundle::bundle_nullable` matched **zero** tests and exited 0. `bundle_tests.rs`
   is included via `#[path]` inside `bundle.rs`, so the real module path is
   `setfit::bundle::bundle_tests::…`. A mutation that should have turned the suite red reported success.
2. Orchestrator (me): `cargo test -p aprender-core --lib setfit::artifact::` returned
   `0 passed … 14185 filtered out`, rc=0 — because I omitted `--features setfit`. With the feature
   the same filter returns 41. **A feature-gated module yields a vacuous green when the feature is off.**
3. This is exactly the class 04-10's `assert_tests_ran` guards exist to catch.

**Action for 04-10:** every guarded target must assert a stated minimum count AND carry the feature
flag its module needs. A filter is only proven by observing a non-zero count.

---

## F-05 — a `skip_serializing_if` gate must match the attribute, not the token

The bare token count in the setfit directories moved 0 → 4 (04-13) and is 3 in `artifact.rs`
(04-02). **Every one is prose** — doc comments and an assertion message explaining that the
attribute is forbidden. The attribute form (`serde(skip_serializing_if`) is still 0.

A future gate on the bare token turns red on its own documentation. Match `serde(skip_serializing_if`.

---

## F-06 — contract amendment: six HF templates have no canonical name (from 04-01)

Six of 21 HF name templates have no `tensor-names-v1` canonical form — four have no role at all,
and `position_embedding` has a `bert:` alias with an **empty `_fallback`**. On the pinned model that
is **22 of 101 encoder tensors with no name to write**. The contract now RESERVES those six, with
non-collision enumerated against the complete `_fallback` set, recorded as a D-01 amendment plus a
deferred upstreaming item. Plans written before wave 1 do not know this — read the contract, not the
plan text, for the reserved set.

---

## F-07 — `bashrs` is not installed on this host

04-01 recorded `bashrs make lint Makefile` as an **unrun check, not a passed one**. CLAUDE.md
mandates bashrs over shellcheck for shell/Makefile linting. Any plan asserting a bashrs gate must
either install it or record it unrun — never report it green.

---

## F-08 — intermediate commit `ab7ace94a` does not compile in isolation

Verified against the committed blob, not assumed: `close` passes 6 args to the now-7-param
`from_run_parts` (E0061). A signature change and its call sites cannot be split across commits and
still compile; `59a4eca93` restores it. The two always land together. Relevant only to `git bisect`
across that pair.

---

## F-09 — there is NO active pre-commit hook; "hooks run by default" is a false assumption

04-03 reported "the active pre-commit hook ran a failing test suite and did not block, and one of
its lines is `command not found: --features`." I investigated from the main checkout. The reality
is stronger than the diagnosis:

```
git config core.hooksPath        -> unset
git rev-parse --git-path hooks   -> .git/hooks
.git/hooks/pre-commit            -> DOES NOT EXIST (only *.sample)
```

**No pre-commit hook is active in this repository at all.** The repo ships `.githooks/pre-commit`,
but it requires `git config core.hooksPath .githooks` to take effect, and that is unset. No Claude
Code hook intercepts commits either (the configured hooks are GSD SessionStart/PostToolUse infra;
none contain `--features`).

Consequence: the standing executor instruction — *"Run `git commit` normally — hooks run by default.
Do NOT pass `--no-verify`"* — has been providing **no gate whatsoever** for every commit in this
phase. Nothing was bypassed and nothing is wrong with the commits; the per-commit verification in
this phase came entirely from executors running their suites explicitly, which they did. But the
belief that a hook was also checking was unfounded. This is the "a guard that does not run is
theater" class from CLAUDE.md, in the workflow's own scaffolding.

**Do not simply activate it.** Per F-03, `.githooks/pre-commit` runs `cargo clippy -- -D warnings`
workspace-wide, which exits 101 on pre-existing `aprender-compute` debt — activating it today would
block every commit immediately. Activation requires fixing that debt or scoping the hook first.
This is a repo-level decision for the human, not a phase-4 change.

---

## F-10 — the phase-3 slice fixture CANNOT carry a setfit-apr-v1 artifact (BLOCKING for 04-12)

Measured by 04-05, not predicted:

```
ProbeComputation { probe: "probe_unicode",
    reason: "SetFitError::VocabOutOfSlice(canonical id 5915 is outside the slice closure)" }
```

The slice fixture's `vocab_remap` is a **97-row closure** and it declares **64 position rows** against
a **256-token** truncation probe. Neither is a production defect — the real pin has the full
vocabulary and 512 positions — but the probes in the contract exercise ranges the slice does not have.

Worse for anyone trying to route around it: **`tune_encoder` gates on
`encoder.architecture_fingerprint()`, so no synthetic encoder can reach `HeadFitted` through the
shipped transitions either.** Both obvious paths are closed.

04-05's Task 2 solved it by substituting **the encoder and head only**, keeping the dataset,
selection, config, evidence and the entire trusted verify policy real, and kept the finding
executable as a test rather than a comment.

**Action for 04-12 (OPS-01, wave 5):** a plain `fx::head_fitted_run(..)` **will not close to APR**.
Reuse 04-05's substitution shape. Read `04-05-SUMMARY.md` before writing the lifecycle test.

---

## F-11 — do NOT use `head`/`tail` to produce file content in this environment

04-05 hit this live: `head -N file > file2` produced a **682-line** file when **1021** lines were
requested. `head` output is filtered by the RTK proxy, so a redirect captures the *rendered view*,
not the bytes. Caught and reverted within one command; nothing committed was affected.

This generalizes the user's global CLAUDE.md guidance ("avoid using this tool to run cat/head/tail")
into a correctness hazard, not just a token-efficiency preference: **any `head`/`tail`/`cat` redirect
can silently truncate.** Use `Read`, or `sed -n '1,Np'`, or Python for byte-exact extraction.

---

## D-04-04-B — RESOLVED: the 24 `aprender-train` failures are pre-existing and unrelated

04-04 observed 24 failures and correctly declined to call them pre-existing without measuring.
Measured on the merged wave-4 tree (`e21f108dd` + recovery merge):

```
test result: FAILED. 7882 passed; 24 failed; 15 ignored
failure set = 21 x gpu::*  +  3 x prune::*   ->  ZERO setfit failures
```

04-05 independently `diff`ed the failure set against the phase-3 known-red baseline: `DIFF_RC=0`,
zero new. Confirmed pre-existing, in subsystems this phase does not touch.

---

## F-12 — ORCHESTRATOR SELF-CORRECTION: `--ff-only` after the base moves, plus unconditional cleanup

I captured `EXPECTED_BASE` for wave 4, then committed the F-03 refinement on top of it before the
agents returned. `git merge --ff-only <04-04 branch>` therefore exited **128** ("Not possible to
fast-forward") — correctly, since the branch had diverged — and my cleanup loop then deleted the
branch **without checking the merge status**, so 04-04's four commits were briefly unreferenced.

Recovered in full by merging the dangling commit `b943116e0` directly (7 files, 0 conflicts,
`classify.rs` and `04-04-SUMMARY.md` restored, verified reachable).

**Rules for the remaining waves:** capture `EXPECTED_BASE` and then do not commit to the branch
until the wave's agents have returned and merged; use `git merge --no-edit` (not `--ff-only`) for
wave merges; and never delete a worktree branch without first asserting `git merge-base --is-ancestor
<branch> HEAD`.
