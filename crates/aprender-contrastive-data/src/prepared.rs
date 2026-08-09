//! The attested, profile-parameterized dataset a consumer must present before canonical
//! splits are exposed.
//!
//! The profile is a TYPE PARAMETER, not a runtime field: `PreparedDataset<Canonical>` and
//! `PreparedDataset<Compatibility>` are distinct types with distinct constructors, and
//! only the canonical one exposes a validation witness. Selection consumes
//! `&PreparedDataset<Canonical>`, so a compatibility dataset cannot be passed at all —
//! which is what makes DATA-06's "cannot be constructed" provable by `trybuild` rather
//! than merely rejected at runtime.
//!
//! Implemented by plan 02-06.
