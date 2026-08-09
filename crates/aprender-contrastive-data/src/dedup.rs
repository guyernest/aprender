//! Cross-split duplicate coalescing and the deterministic exclusion record.
//!
//! Duplicate groups are connected components over the union of exact-hash and
//! normalized-hash edges. Grouping independently by both keys would double-count — an
//! exact duplicate is necessarily also a normalized duplicate — and could decrement a
//! class pool twice.
//!
//! Prepare-time duplicate content is excluded and recorded, never fatal (D-18, upheld by
//! D-27); the typed error fires only when the reduced pool can no longer supply
//! `shots_per_class`.
//!
//! Implemented by plan 02-03.
