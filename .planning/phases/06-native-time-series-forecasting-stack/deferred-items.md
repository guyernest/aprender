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
