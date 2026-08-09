//! Sorted per-class buckets over a selection pool.
//!
//! Sorted `Vec`s throughout, never `HashMap` iteration: any collection whose order
//! reaches a hash or a manifest must have a defined order, or determinism becomes a
//! property of the allocator.
//!
//! Implemented by plan 02-05.
