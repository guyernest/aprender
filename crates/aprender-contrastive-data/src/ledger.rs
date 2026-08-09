//! Append-only access ledger: which splits were touched, under which profile.
//!
//! The ledger has a deterministic canonical byte form and a `ledger_hash`, and both the
//! records and the hash are embedded in the selection manifest payload. A ledger that
//! exists only in memory is not evidence — the downstream selection-lock gate needs an
//! artifact it can read.
//!
//! Implemented by plan 02-04.
