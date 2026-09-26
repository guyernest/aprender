//! Compile-fail proofs for the aprender-decide public API (trybuild).
//!
//! FALSIFY-DECIDE-APR-006 / decide-apr-v1 rung 8: a [`aprender_decide::Decider`] can be
//! minted only by the load ladder (`Decider::load_bytes` / `Decider::load_path`). Each
//! case under `tests/ui/` tries a second minting path and must fail to compile with the
//! error recorded in its `.stderr`.
//!
//! This integration target is dark in CI until `.github/workflows/ci.yml`'s explicit
//! `--test` line adds `-p aprender-decide --test ui` (plan 08-12).

#[test]
fn decider_has_no_second_minting_path() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
