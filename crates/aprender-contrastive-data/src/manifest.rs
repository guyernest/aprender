//! Canonical serialization and semantic hashing for every manifest in the protocol.
//!
//! The hashed canonical payload never contains its own digest and never contains volatile
//! metadata such as a creation timestamp; the digest and the volatile fields live in an
//! outer envelope. A payload that includes its own hash cannot be recomputed, and a
//! timestamp inside the hashed region makes two identical runs compare unequal.
//!
//! Implemented by plan 02-07.
