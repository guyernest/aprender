# Deferred items — Phase 06

Out-of-scope discoveries surfaced while executing this phase. Per the executor scope
boundary these were **observed and logged, not fixed**: they are pre-existing, unrelated to
the task that surfaced them, and fixing them inside an unrelated commit would hide who
broke what.

## From plan 06-01 (2026-09-05)

### D-ITEM-06-01-a — `readme_contract` was already red on three counts before Phase 6

`cargo test -p aprender-core --test readme_contract` fails 4/15 on `440d7009a` (the
pre-plan commit). Plan 06-01 fixed the ONE it moved (`FALSIFY-README-005`, the workspace
crate count, which went 82→85 because this plan adds two members) and left the other
three, all of which predate Phase 6 and are untouched by it:

| Gate | Finding | Why not fixed here |
|---|---|---|
| `FALSIFY-README-007` | `README.md` claims **1778** provable contracts; `find contracts -name '*.yaml' \| wc -l` reports **1786** | This plan adds no contract; the drift is someone else's and dates from before it |
| `FALSIFY-README-CRATE-001` | `aprender-mcp-setfit-lambda` and `aprender-contrastive-data` have no `README.md` | Neither crate is touched by Phase 6 |
| `FALSIFY-README-CRATE-002` | `crates/aprender-mcp-setfit/README.md` lacks the `paiml/aprender` monorepo link | Phase 6's own two READMEs both carry it; 06-RESEARCH F7 already records this one as known |

Both Phase 6 crates satisfy all four gates. Net effect of 06-01 on this test: 4 failures →
3 failures.

### D-ITEM-06-01-b — `cargo clippy -p <crate> -- -D warnings` cannot pass for ANY crate in this tree

The plan's `<verify>` block runs
`cargo clippy -p aprender-forecast -p aprender-mcp-forecast --all-targets -- -D warnings`.
On this toolchain (1.93.0) the `-D warnings` flag reaches **path dependencies**, so the
command fails with 18 errors inside `crates/aprender-compute` — unused imports, unreachable
expressions, `dead_code`, unused variables.

Measured control, so this is not a guess about my own code: the same command against the
untouched, pre-existing `aprender-mcp-setfit` fails identically (`rc=101`, same 18
`aprender-compute` findings), as does `-p aprender-serve` (failing inside
`aprender-present-terminal`). The condition is workspace-wide and predates Phase 6.

06-01 therefore gated its crates with `--no-deps`, which scopes the lint to the primary
packages — the thing the criterion was trying to measure. Engagement was PROVEN, not
assumed: a `clippy::needless_bool` mutation inserted into `aprender-forecast/src/lib.rs`
turned the `--no-deps` command red (`rc=101`, `needless_bool` cited), and removing it
turned it green again.

Not fixed here because the 18 findings are in a crate this phase does not touch, and
because clearing them is a real piece of work with its own blast radius
(`aprender-compute` is depended on by most of the workspace). Worth a dedicated ticket:
either clean the crate or record why those lints are allowed there.

### D-ITEM-06-02-a — `aprender-core`'s own lib has a pre-existing `unreachable_code` error under `-D warnings`

Found while checking that plan 06-02's one-file test edit would not fail a lint gate.
`cargo clippy -p aprender-core --test monorepo_invariants --no-deps -- -D warnings` exits
101 on:

```
error: unreachable expression
   --> crates/aprender-core/src/demo/reliable/performance.rs:126:5
124 |         return "NEON".to_string();
126 |     "Scalar".to_string()
```

`--no-deps` does NOT rescue this one, because the failing code is in the primary package's
own lib, which the test target links against.

Measured control rather than assumed: the identical command against the **untouched**
sibling target `--test readme_contract` fails with the same single error (`rc=101`), and
06-02 modified no file under `crates/aprender-core/src/`. The finding predates this plan.

Not fixed here: it is a `cfg`-shaped early return in a demo module that no Phase 6 task
touches, and the fix (an `#[allow]`, a `cfg` restructure, or deleting the dead branch) is a
judgement call belonging to whoever owns `src/demo/`. Note that the 06-01 wording of
D-ITEM-06-01-b — "cannot pass for ANY crate in this tree" — has a second cause: not just
`-D warnings` reaching path dependencies, but `aprender-core`'s own lib.

---

## D-ITEM-06-03-a — every `cargo test` invocation in this workspace pays a ~12 s forced rebuild

**Found during:** plan 06-03 Task 2, measuring the warm parity-ladder wall (RESEARCH Pitfall 9).

**Measured, not inferred.** Three consecutive `cargo test -p aprender-forecast --lib
prophet::parity` invocations with `CARGO_INCREMENTAL=0`, no source edit between them:

| Invocation | cargo wall | `Finished ... in` | test execution |
|---|---|---|---|
| warm run 1 | 14 s | 12.21 s | 1.73 s |
| warm run 2 | 14 s | 12.20 s | 1.78 s |
| the same 32 tests, running `target/debug/deps/aprender_forecast-*` DIRECTLY | — | — | 1.77–1.79 s |

Every run recompiles `aprender-compute`, `aprender-core` and `aprender-forecast` even though
nothing changed. Both `crates/aprender-compute/build.rs` and `crates/aprender-core/build.rs`
exist; one or both is missing a `cargo:rerun-if-changed` (or emits an always-changing key), so
cargo can never call the unit fresh.

**Why it matters beyond this plan.** 06-01 Task 2's profile decision was taken on a
per-invocation wall (`13 s`) and projected as `7 x 13 = 91 s` for the ladder. Re-measured this
session, that 13 s proxy is **11.73 s of forced rebuild + 1.64 s of test** — so the projection
multiplied a CONSTANT per-invocation build cost seven times. The real ladder is ONE invocation:
1.73 s of tests, 14 s wall. The `[profile.dev.package.aprender-forecast] opt-level = 3` decision
is still correct (the fits genuinely need it), but the "~91 s against a 60 s target" concern
06-01 left for this plan is **closed as a measurement artefact**, not as an optimisation win.

**Not fixed here:** the build scripts belong to `aprender-compute` and `aprender-core`, which
this plan does not touch, and diagnosing which key is unstable is its own task. Owner: 06-09.

## From plan 06-06 (2026-09-06)

### D-ITEM-06-01-a is now CLOSED (not deferred any further)

`cargo test -p aprender-core --test readme_contract` is **15 passed / 0 failed** as of
`14eceff67`. The three failures 06-01 logged were closed one at a time by the plan that
moved each count:

| Gate | Closed by |
|---|---|
| `FALSIFY-README-CRATE-001` / `-002` | closed between 06-01 and 06-06 (both crate READMEs and the monorepo link now present) |
| `FALSIFY-README-007` (contract count) | 06-06 — adding `contracts/forecast-tool-boundary-v1.yaml` moved `find contracts -name '*.yaml'` to 1790, so this plan's edit turned it red and this plan corrected the row |

Nothing here is deferred; the row is left as the audit trail for the phase.

### D-ITEM-06-06-a — `.pv/contracts.idx`, `.pv/contracts.idx.mtime`, `.pv/lint-previous.json` are dirty in the working tree

These three TRACKED files are `pv`'s local index cache and are rewritten by any `pv`
invocation, including a read-only `pv validate`. They were **already modified before this
plan's first command** (visible in the pre-plan `git status`), so the drift predates
06-06; running `pv` here only advanced it further.

NOT committed by this plan: a cache blob is not a reviewable artifact, and committing one
inside a contract commit would make every future `pv` run produce a spurious diff for the
next author. The real question — should `.pv/` be tracked at all, or `.gitignore`d like
other tool caches — belongs to whoever owns `pv`, not to a forecasting plan.

### D-ITEM-06-07-a — the router pool is deferred as an advanced feature; its gate is disarmed, not passing

**Owner decision (2026-09-06):** the pmcp router pool (`pooled_app`, `--pool K`) is an
advanced feature held for a later stage. It stays in the tree — it is measured, correct and
opt-out (`pool <= 1` returns the plain `http_app`) — but nothing gates its *engagement*.

What is deferred, precisely:

- `just forecast-pool-ratio` **does not exist**. `contracts/forecast-tool-boundary-v1.yaml`
  cited it as the test for `FALSIFY-BOUNDARY-011`, so that test could never run. It is now
  marked `status: deferred` and removed from the contract's `guarantee` block, because a
  gate that cannot evaluate must not be counted as green.
- `pool_equality` asserts **determinism, not engagement**. Both concurrency tests pass
  bit-identically (8/8, 16/16, `max|Δ yhat| = 0.0`) even with the pool collapsed to a single
  router — a stateless router is trivially deterministic. So no test in CI would notice the
  pool silently reverting to one instance.

What is NOT deferred and still holds:

- `FALSIFY-BOUNDARY-012` — the `POOL SPEEDUP:` line and its `arch`/`profile`/`workers`/`cpus`
  provenance fields are printed and format-checked by the unit test today. Arming the gate
  later needs the recipe only, not a re-derivation of the format.
- The measured evidence stands and is reproducible: `POOL=1 -> 1.002x`, `POOL=8 -> 2.070x`,
  same host, same eight requests. The `1.002x` control is what makes the other number
  evidence rather than an assertion, and it reproduces spike-010's finding that pmcp's
  router mutex — not CPU — serialises concurrent fits.

To arm it later: add the `forecast-pool-ratio` recipe (release build, aarch64, three runs,
parse the single `POOL SPEEDUP:` line, assert best ratio >= 2.0), flip
`FALSIFY-BOUNDARY-011` off `status: deferred`, and restore it to the `guarantee` block.
The upstream fix — pmcp not holding `Arc<Mutex<Server>>` across the whole tool future —
would delete the pool entirely; that is the better long-term resolution.

---

## From plan 06-08 (host-gated evidence recipes) — 2026-09-06

### 1. `FALSIFY-BOUNDARY-011` can now be re-armed (was blocked on this plan)

The note above says the gate was deferred because "`just forecast-pool-ratio` **does not
exist**". It exists as of commit `adc8a560a` and PASSES on aarch64 release: three attempts at
5.150 / 5.162 / 5.146, best 5.162x against a 2.0 bar (06-EVIDENCE.md §3). The remaining work
is contract-side only — flip `FALSIFY-BOUNDARY-011` off `status: deferred` in
`contracts/forecast-tool-boundary-v1.yaml` and restore it to the `guarantee` block. Not done
here because that file is outside this plan's `files_modified` and already carries
uncommitted edits from an earlier review pass; 06-09 is the natural home.

Caveat to carry with it: the bar is **host-gated**. `forecast-pool-ratio` measures a release
build on aarch64; the CI runner is X64 and builds debug. The contract should say which host
the gate is claimed on, or CI will be asked to assert a number nobody measured there.

### 2. PRE-EXISTING: `cargo clippy -- -D warnings` is red workspace-wide on aarch64 macOS

Not caused by this plan; recorded because plan 06-08 Task 1's verify tripped over it and the
finding would otherwise be lost.

`cargo clippy -p aprender-forecast --all-targets -- -D warnings` exits 101 with **18 errors,
all in `crates/aprender-compute/`** and none in the crate being linted. Confirmed by control:
the same command with the new example removed from the tree produces the identical 18-finding
set, and `cargo clippy -p aprender-forecast --example mase_rolling_origin -- -D warnings`
reports the same 18, again none in `aprender-forecast`. (Mechanism: trailing `-- -D warnings`
becomes `CLIPPY_ARGS`, which clippy applies to every locally-compiled crate, not just the
selected package.)

The findings:

- `unreachable expression` x3 — `blis/backend_selection.rs:127`, `brick/simd_config/mod.rs:88`,
  `hardware/mod.rs:375`. Each follows an unconditional `return ComputeBackend::Neon` /
  `SimdWidth::Neon128` that is **cfg-gated to aarch64**, so the trailing scalar fallback is
  live code on x86_64 and dead here. **This class is arch-conditional and therefore invisible
  to CI**, which is X64 — the same shape as CLAUDE.md #2370's "findings accumulate where no
  gate looks".
- `dead_code` x9 — `pack_a_block_generic`, `pack_b_block_generic`, `pack_b_block_nr16`,
  `matmul_q4k_f32_parallel`, `compute_chunk_q4k_scalar`, `compute_chunk_scalar`,
  `extract_q6k_values`, `PREFETCH_DISTANCE`, `NT_STORE_THRESHOLD_BYTES`, `GEMV_TILE_THRESHOLD`.
- `unused_imports` x3 — `q4k/gemv/mod.rs:14`, `blis/packing.rs:419`, `vector/ops/rounding.rs:9`.
- `unused_variables` x2 — `brick/quant_ops/mod.rs:219,318` (`backend`).

Consequence worth stating plainly: **`make tier1` / `make tier2` cannot pass on an aarch64
macOS dev box today.** Whoever picks this up should fix `aprender-compute`, not add
`#[allow]`s, and should re-run on BOTH arches — the point of the finding is that one arch's
green says nothing about the other's.

### 3. PRE-EXISTING: substantial uncommitted phase-06 work in the tree

At plan 06-08's start the working tree carried 14 modified source files this plan did not
create — `crates/aprender-forecast/src/{forecast,np,prophet,safetensors,test_support,types,
chronos}.rs`, `build.rs`, `crates/aprender-mcp-forecast/src/{lib,main}.rs`,
`contracts/forecast-tool-boundary-v1.yaml`,
`crates/aprender-core/tests/monorepo_invariants.rs`. They look like review-hardening from a
pass over 06-06/06-07 (a `MAX_SPAN_DAYS` bound, cross-model option refusals, a `MAX_POOL`
ceiling) that was never committed: `git show HEAD:crates/aprender-forecast/src/forecast.rs`
has no `MAX_SPAN_DAYS`.

06-08 did not commit them (out of scope) but every measurement in 06-EVIDENCE.md ran against
them, which is why that file records the delta's sha256 beside the commit hash. **Someone
should decide whether that work lands or reverts before the phase closes** — right now the
branch's committed state and its tested state are different things.

### 4. OPEN ITEM for 06-09 — D-18 clause 2 in CI, gated on one x86_64 measurement

Decision at plan 06-08 Task 3's blocking-human checkpoint: **`measure-x86-first`**. Neither
hunk of `06-ci-chronos-step.patch` is applied in this phase by default. The patch is
preserved, verified to apply cleanly against ci.yml as of `adc8a560a`, and `.github/` is
untouched in the working tree and in every commit of plan 06-08.

**Why not `apply-now`.** Hunk (a) has two prerequisites nobody has provisioned, and a step
that dies at the weight-hash check before running a single test is a red run that proves
nothing:

1. a runner-local weights mount (`/srv/models` in the diff is a PLACEHOLDER), and
2. a way for `uv run --with huggingface_hub --with safetensors --with numpy` to resolve
   inside the network-less `sovereign-ci:stable` clean-room — a warm uv-cache mount
   (`/srv/uv-cache` is proposed) or those three packages baked into the image.

**The gating work, which is this phase's own open item and not new scope.** Run
`just chronos-gate` ONCE on an x86_64 Linux host with the weights (lambda-vector qualifies
and is pre-authorized compute per CLAUDE.md). Read two things off it: the printed
`f32_quantile_bar` line, and the measured max|delta| from
`peyton_ladder_matches_oracle_f32`. Then tighten `quantiles_abs_f32_nonaarch64` in
`contracts/chronos-bolt-parity-v1.yaml` from the PROVISIONAL-UNMEASURED `5.0e-6` to
measurement + margin, as a `pv diff`-visible contract edit. That single run discharges
REVIEW-06-02 and windows-ledger entry #4 together, and only then is there evidence to wire
a CI leg onto. The aarch64 evidence in `06-EVIDENCE.md` is unaffected either way.

**If no x86_64 host is reachable within this phase, this collapses to `defer`** — by plan
06-08's own option table. Record it that way explicitly: both hunks stay in the patch file,
and **D-18 clause 2 is a named CI gap**, evidenced locally by `just chronos-gate` and
`06-EVIDENCE.md` §4 only. It must not become a silent drop.

Hunk (b) (`cargo test -p aprender-mcp-forecast --test e2e_stdio` on the single
Integration-tests line) needs no runner provisioning and can be taken independently at any
time; it was not split out here because the decision was to gate on the measurement first.

### 5. OPEN ITEM for 06-09 — account for the uncommitted delta BEFORE committing it

Follow-on to item 3 above. Decision: **investigate, then commit** — not commit blind. The 18
uncommitted paths (14 source/doc + 3 `.pv/` artifacts, 510 insertions / 164 deletions) were
present in the session-start `git status` for plan 06-08, so they are neither 06-08's nor
06-07's. They are concentrated in exactly the crates 06-08 measured.

06-09 must establish their provenance and intent first, then commit them with a message that
says where they came from. Until that lands, `06-EVIDENCE.md`'s numbers are reproducible only
from `adc8a560a` **plus** a recorded sha256 delta
(`bd42a46dda0f0c660cc450b1c97d50f2c017d9111a6164a73272793b7dbcb2b6`, source-only) rather than
from a real commit. The delta digest stays as the honest record until then; it is not a
substitute for landing the work.
