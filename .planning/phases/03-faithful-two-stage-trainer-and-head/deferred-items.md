# Phase 3 — Deferred Items

Pre-existing defects discovered during Phase 3 execution that are OUT OF SCOPE for the plan
that found them. Each is measured, not assumed. Phase 3 does not inherit these as failures
and does not hide them either.

Numbering continues from Phase 2's `deferred-items.md` (D-ITEM-01..04), so a reader who
follows a reference from either phase lands on the right entry.

---

## D-ITEM-05 — `cargo check -p aprender-train --no-default-features` cannot compile

**Found by:** plan 03-03, Task 1 (2026-08-09)
**Owner crate:** `aprender-train` (`src/monitor/`) — NOT a Phase 3 module
**Severity:** blocks one acceptance criterion of plan 03-03; blocks nothing else

### Measurement

```
cargo check -p aprender-train --no-default-features                     -> rc=101, 8 errors
cargo check -p aprender-train --no-default-features --features setfit   -> rc=101, 8 errors
```

The two diagnostic streams (`grep -A1 '^error'`, 26 lines each) are **byte-identical**.
Every one of the 8 errors is in `src/monitor/tui/{app,dashboard}.rs`:

- 3 x `E0433: failed to resolve: use of unresolved module or unlinked crate presentar_terminal`
- 5 x `E0282: type annotations needed` (the shadow of the first three)

### Cause

`crates/aprender-train/src/monitor/mod.rs:45` declares `pub mod tui;` **unconditionally**,
while its only dependency is gated: `tui = ["dep:presentar-terminal"]`, and `tui` is in
`default`. So the module is compiled in a build that does not link what it uses. Nothing to
do with Phase 3 — the `setfit` feature contributes exactly zero errors, which is what the
byte-identical streams prove.

### Why plan 03-03 did not fix it

`src/monitor/` is outside the plan's declared `files_modified`, and wave-1 plans are
explicitly instructed to stay inside their own file sets. It is also not a one-line fix to
verify: gating the module means auditing every `pub use tui::{...}` re-export in
`train/mod.rs` and `monitor/mod.rs` for the same treatment.

### What 03-03 did instead

`make setfit-feature-matrix` leg (a) is wired as a **two-sided diff** rather than a plain
green check: it runs the minimal build with and without `setfit` and fails if the diagnostic
streams differ. It therefore asserts the property Phase 3 owns ("setfit does not leak into
the minimal build") without importing a red Phase 3 did not cause. The leg was falsified
before being trusted — a module compiled only under `cfg(not(feature = "setfit"))` and
importing `aprender_contrastive_data` made it fire, and was reverted.

### Fix direction

Gate `monitor::tui` on `feature = "tui"` together with its re-exports, then re-run the leg;
if the streams are then both empty and both rc 0, replace the diff leg with the plain
`cargo check --no-default-features --features setfit` the plan originally asked for. Its own
PMAT ticket.

---

## D-ITEM-02 (Phase 2) — re-measured in Phase 3

`cargo clippy -p aprender-train --lib --features setfit -- -D warnings` exits **101** at
Phase 3 wave 1, and every diagnostic is pre-existing:

```
error: could not compile `aprender-compute` (lib) due to 19 previous errors
error: could not compile `aprender-present-terminal` (lib) due to 1 previous error
```

Diagnostics citing `crates/aprender-train/` : **0**. Diagnostics citing `train/setfit/` or
`scheduler/warmup_linear_decay.rs` : **0**. The error stream is identical with and without
`--features setfit`. `aprender-compute` is owned by plan 03-02 in this wave, so 03-03 did
not touch it. Unchanged from Phase 2's entry apart from the crate counts.
