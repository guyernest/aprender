//! Dataset identity attestation and its re-derivation from supplied buffers.
//!
//! Carries profile, schema version, label map, per-split JSONL digests, per-class counts,
//! normalization version, exclusion record, and dataset fingerprint. Construction from
//! attested bytes re-derives every field from the buffers the caller supplied and fails
//! typed on any disagreement — an attestation that is merely *quoted* back proves nothing.
//!
//! Implemented by plan 02-06.
