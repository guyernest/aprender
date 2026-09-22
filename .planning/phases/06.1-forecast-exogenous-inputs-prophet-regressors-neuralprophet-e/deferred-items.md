# Deferred items — phase 06.1

Out-of-scope discoveries logged rather than fixed, per the executor's SCOPE BOUNDARY rule:
only issues DIRECTLY caused by the current task's changes are auto-fixed.

## From plan 06.1-07

### 1. README claim-table drift — 2 pre-existing RED tests in the README drift gate

`cargo test -p aprender-core --test readme_contract` fails 2 of 15 at the wave base
`10e121ee5`, before any change in this plan:

| Test | README says | Derived says |
|---|---|---|
| `test_readme_contract_count_matches_workspace` | `**1790** provable contracts` | 1791 |
| `test_readme_crate_count_matches_workspace` | `**86** workspace crates` | 87 |

MEASURED as pre-existing, not inferred:
`git ls-tree -r --name-only 10e121ee5 contracts | grep -c '\.yaml$'` returns **1791** at the
wave base, and plan 06.1-07's whole diff against that base touches `contracts/` in exactly
one file — MODIFIED, not added (`git diff --stat 10e121ee5 -- contracts README.md`:
`1 file changed, 59 insertions(+), 9 deletions(-)`). No crate was added either. So both
numbers were already true before this plan and the README rows were already stale.

Not fixed here: the README claims table is a published document whose counts belong to
whichever change moved them, and editing it from this plan would attribute someone else's
drift to this phase. Two one-line edits when someone owns them.

**Consequence for this plan's verify:** the `<automated>` t3d check requires
`cargo test -p aprender-core --test readme_contract` to exit 0. It cannot at the wave base.
The check's OTHER clause — at least 10 tests ran — passes (15 ran).

### 2. `cargo fmt --all -- --check` is RED at the wave base

17 `Diff in` entries across `crates/aprender-image`, `crates/aprender-mcp-chronos` and
others — none in `crates/aprender-forecast`, which this plan changes. `cargo fmt
-p aprender-forecast` keeps the changed crate clean and the workspace-wide check stays red
for reasons that predate this plan.

### 3. `cargo clippy ... -- -D warnings` cannot pass while `aprender-compute` is dirty

`cargo clippy -p aprender-compute --lib -- -D warnings` reports **19 errors** on its own at
the wave base — cfg-gated dead code on aarch64 (`MR_512V2`, `NR_512V2`, `NeonBackend`,
`matmul_q4k_f32_parallel`, `PREFETCH_DISTANCE`, …). Because the flag reaches that dependency
in this workspace's configuration, `cargo clippy -p aprender-forecast -p aprender-mcp-forecast
--all-targets -- -D warnings` aborts there before it ever lints the two crates under change.

What this plan asserts instead, and what it measured: `cargo clippy -p aprender-forecast
-p aprender-mcp-forecast --all-targets` exits 0 with **zero** findings whose path is under
`crates/aprender-forecast/src` or `crates/aprender-mcp-forecast/src`.

This is the CLAUDE.md "Linting" section's own ceiling-gate class — findings accumulate
invisibly on a toolchain nobody's gate runs.

### 4. `pv diff` is blind to `constants:`, `door_surface` and `cost_axes` changes

`pv diff /tmp/forecast-tool-boundary-old.yaml contracts/forecast-tool-boundary-v1.yaml`
reports **`Contracts are identical.`** for a change that adds a new `constants:` key
(`fit_np_regressor_cost_per_column`), rewrites cost axis C-08's `formula:`, adds twelve
`calibration:` keys and rewrites six `door_surface.knobs` rows. `cmp` on the same two files
reports them differing at line 294, and `diff` reports 56 lines.

So `pv diff`'s semantic model covers equations / proof obligations / falsification tests and
not the door surface or the constants table, and its semver suggestion must not be read as
"no bump needed" for a door-surface change. Plan 06.1-07 set `metadata.version` to `1.10.0`
on its own reasoning (a new published constant plus two new refusals on an arm that
previously refused everything — additive, so MINOR) and recorded that the tool did not
supply it.

Worth its own ticket: either extend `pv diff` to the door-surface and constants sections, or
have it say plainly which sections it compares, so a green "identical" is not mistaken for
coverage.
