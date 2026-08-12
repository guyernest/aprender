//! TRN-01 / TRN-07 / SAFE-03 — "cannot be expressed" is checked by the COMPILER.
//!
//! Obligations: the lifecycle typestate (`OBLIG-STL-*`, contract
//! `setfit-train-lifecycle-v1`), TRN-01's legality claim, TRN-07's
//! no-caller-asserted-metric claim and SAFE-03's probe-is-not-SetFit claim.
//!
//! Every other Phase 3 gate observes a value and rejects it. These seven observe that
//! there is no value to observe: each `tests/ui/*.rs` is a complete program using only
//! the crate's PUBLIC API which must fail to compile, with the diagnostic pinned by a
//! committed `.stderr` snapshot. A runtime rejection can be caught and ignored by a
//! caller; a non-compiling program cannot.
//!
//! # The seven cases and what each one would otherwise be
//!
//! | Case | Without the compile error it would be |
//! |------|---------------------------------------|
//! | `setfit_direct_state_construction` | a `SetFitRun<EncoderTuned>` that never tuned |
//! | `setfit_fit_head_before_tune` | stage two run on an untuned encoder — a linear probe wearing SetFit's name |
//! | `setfit_token_without_lock` | canonical test access with no lock behind it |
//! | `setfit_pairs_into_fit_head` | the head fitted on pair multiplicities instead of unique rows |
//! | `setfit_probe_claims_setfit` | SAFE-03's baseline reported as the method under test |
//! | `setfit_metric_value_asserted` | the caller supplying the number the selection lock commits |
//! | `setfit_external_codec_impl` | an out-of-crate codec inside the verification path |
//!
//! # What a snapshot must contain to be evidence
//!
//! Each `.stderr` must name the crate's real types, methods or visibility —
//! `SetFitRun`, `fit_head`, `CanonicalTestToken`, `FrozenProbeRun`,
//! `ValidationEvaluation`, `SetFitCodec`, `Sealed`. A snapshot showing a syntax error, an
//! unresolved import or a misspelled name would be a red case that proves nothing about
//! legality, and the CASE must be fixed rather than the snapshot blessed. The acceptance
//! grep for this plan is exactly that rule mechanised across all seven files.
//!
//! # Snapshots are rustc-version sensitive, and that is understood
//!
//! `.stderr` files pin rustc's exact wording. A toolchain bump can reword a diagnostic and
//! turn this suite red without any behaviour change; the re-baseline command is
//!
//! ```text
//! TRYBUILD=overwrite cargo test -p aprender-train --test ui --features setfit
//! ```
//!
//! and the resulting diff MUST be reviewed against the list of names above before it is
//! committed. The assertions here are about types, arity and visibility, so a legitimate
//! reword keeps every one of those names present.
//!
//! # Why the whole file is feature-gated
//!
//! `train::setfit` exists only under `--features setfit` (`train/mod.rs:51`). Without the
//! gate the harness would compile in a default build, find seven cases that fail because
//! the module is absent, and report SEVEN PASSING compile-fail tests — the vacuous-proof
//! failure mode this suite exists to rule out. The `#[cfg]` makes the default build run
//! zero cases instead of seven fake ones.

#![cfg(feature = "setfit")]

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
