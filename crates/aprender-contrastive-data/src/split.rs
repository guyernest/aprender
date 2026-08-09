//! Typestate split roles: `Split<Train>`, `Split<Validation>`, `Split<Test>`,
//! `Split<CompatibilityTest>`.
//!
//! The role is a zero-sized type parameter and the only constructor is the bytes -> typed
//! boundary, which validates the embedded `source_split` before it will hand back a
//! typed value (D-16). A library caller therefore cannot *express* leakage, and honest-
//! looking bytes with a mislabeled role are a typed error rather than a compiler-accepted
//! `Split<Train>`.
//!
//! Implemented by plan 02-03.
