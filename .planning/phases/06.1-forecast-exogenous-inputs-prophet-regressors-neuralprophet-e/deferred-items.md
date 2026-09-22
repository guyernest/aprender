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

### 5. BLOCKER — the SC2 no-argument invariance gate is RED under the `--workspace --lib` build CI runs

Found by plan 06.1-08's SC6 sweep. `invariance::every_baseline_case_reproduces_its_signature`
is **deterministically green** under `cargo nextest run -p aprender-forecast --lib` and
**deterministically red** under `cargo nextest run --workspace --lib` — the invocation SC6
names and `.github/workflows/ci.yml:289` runs. Same commit, same host, stable hash on each
side:

```
recorded (baseline, -p scope) aa669c2352dd376a
got      (--workspace scope)  e01f605419765a6a   case peyton/prophet/default, arch=aarch64
```

Not flakiness and not load. Ruled out by control:

| Control | Result |
|---|---|
| `-p aprender-forecast --lib` (3 runs) | PASS, stable |
| `--workspace --lib` filtered to this one test (3 retries) | FAIL, identical hash each retry |
| `-p aprender-forecast --lib` under a saturating 14-core busy load | **PASS** — so it is not CPU load, and not `FIT_BUDGET_SECS` |
| `-p aprender-forecast -p aprender-core --lib` | PASS — so `parallel`/`rayon` on core is **not** the variable, even though that scope does enable it |
| `-p aprender-forecast -p apr-cli --lib` | **FAIL**, same hash — one-variable flip |

**Mechanism: cargo feature unification.** `aprender-forecast` depends on `aprender-core` with
`default-features = false`. Under a scoped build, `cargo tree -e features -p aprender-forecast`
resolves **zero** `aprender-core` features. Under the workspace build it resolves **23**,
including `format-compression`, `format-encryption`, `format-quantize`, `half`,
`safetensors-compare`, `safetensors`, `setfit`, `conformance-fixtures`, `hf-hub-integration`,
`parallel` and `rayon`. `apr-cli`'s default feature set is what pulls most of them
(`safetensors-compare = ["aprender/safetensors-compare"]`, `setfit = [… "aprender/setfit" …]`).
Narrowing past "somewhere in apr-cli's default features" was not attempted: `apr-cli` does not
build with `--no-default-features` on this tree (unresolved `axum` / `realizar` imports), so
that bisect step is unavailable.

**Why this matters more than a red test.** The gate exists to prove D-19/SC2 — that a caller
who passes NO new argument gets a byte-identical answer, which is what makes the delivery "one
pinned line plus a lock entry". The baseline was captured under a build configuration CI never
performs, so the guarantee is currently proven only in a scope the consumer and CI do not use.

**Do NOT close this by re-recording the baseline under `--workspace`** — that would move the
goalposts and simply invert which scope is red. The real questions are (a) which enabled
feature changes the numerics, and (b) whether the answer a consumer gets is supposed to depend
on the feature set of unrelated workspace members at all. Both need their own measurement and
their own decision; this is outside plan 06.1-08's documentation-and-sweep scope.

#### Follow-up narrowing (orchestrator, 2026-09-21, at `6c40821c3`)

The blocker above was independently reproduced before being accepted: `-p aprender-forecast
--lib` rc=0, `-p aprender-forecast -p apr-cli --lib` rc=101 with byte-identical hashes
(`recorded aa669c2352dd376a, got e01f605419765a6a`). It is real and deterministic.

Three corrections to the mechanism paragraph above, each measured:

1. **The "23 core features" figure is a `--no-dedupe` artifact.** `cargo tree -e features`
   without `--no-dedupe` shows `apr-cli` pulling exactly **6** `aprender-core` feature edges:
   `default`, `format-compression`, `lz4_flex`, `parallel`, `rayon`, `zstd`. Since
   `aprender-core`'s `default = ["parallel"]` and `parallel = ["rayon"]`, that reduces to two
   independent axes: the parallel axis and the compression axis.
2. **`safetensors-compare`, `setfit`, `format-encryption`, `format-quantize`, `half` and
   `hf-hub-integration` are NOT in the delta** that reaches `aprender-forecast`. They do not
   appear in the deduped edge set; they should not be chased.
3. **The `parallel`/`rayon` exoneration above was reached on the wrong evidence.** The control
   `-p aprender-forecast -p aprender-core` tests **`aprender-core`'s** `parallel` feature. The
   crate whose feature set actually diverges is **`aprender-compute`** (`[lib] name = "trueno"`,
   the SIMD/GPU layer): `default` alone in the scoped build vs
   `default, parallel, rayon, gpu, wgpu, bytemuck, futures-intrusive, pollster` under `apr-cli`.
   Two same-named features in two different crates.

Additional controls run here, both PASS (so both are ruled out):

| Control | Result |
|---|---|
| `-p aprender-forecast -p aprender-compute --features aprender-compute/parallel --lib` | **PASS** — `aprender-compute`'s `parallel`+`rayon` is not the variable either |
| `-p aprender-forecast -p aprender-quant --lib` | **PASS** — refutes the `half` -> `num-traits/libm` hypothesis |

The `num-traits/libm` lead was worth testing and is now dead: the chain `apr-cli` ->
`aprender-quant`(default) -> `half`(default) -> `num-traits feature "libm"` is real and would
swap `exp`/`ln`/`powf` for non-bit-identical pure-Rust implementations, which is exactly the
shape of this symptom — but enabling it alone does not reproduce the flip.

**Remaining candidate surface, measured:** ~190 feature edges across the 79 packages
`aprender-forecast` links whose feature set grows when `apr-cli` joins the build. The full list
is reproducible with the two `cargo tree -e features` runs above; `gpu`/`wgpu` on
`aprender-compute` is the largest untested axis, since it can change kernel dispatch.

This does not change the recommendation: still **do not** re-record the baseline under
`--workspace`, and question (b) above — whether a consumer's answer may depend on the feature
set of unrelated workspace members at all — is the decision that should be taken first, because
a "yes" makes the bisect moot and a "no" makes it mandatory.
