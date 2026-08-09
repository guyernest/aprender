//! Exact and normalized content hashes plus the dataset fingerprint.
//!
//! Two hashes per row, for two different jobs (D-17): the exact SHA-256 over the raw
//! `input` bytes is identity and provenance; the normalized hash (`nfc-trim-ws-v1` — NFC,
//! trimmed, internal whitespace collapsed, deliberately NO casefolding) is leakage
//! detection.
//!
//! Implemented by plan 02-03.
