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

**Action for 04-10:** every clippy leg in the Make targets must carry `--no-deps`, or the gate is
theater. Do not "fix" it by removing `-D warnings`.

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
