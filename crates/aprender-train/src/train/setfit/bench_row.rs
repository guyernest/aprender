//! The benchmark row and run manifest (plan 05-05, EVAL-03 / EVAL-04).
//!
//! Contract: `setfit-benchmark-claims-v1` (authored in plan 05-05 task 1).
//!
//! IMPLEMENTATION LANDS IN TASK 2. This file currently declares only its test module, so the
//! suite compiles against a surface that does not exist yet — a genuine RED.

#[cfg(test)]
#[path = "bench_row_tests.rs"]
mod bench_row_tests;
