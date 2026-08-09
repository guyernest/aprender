//! Canonical serialization and semantic hashing for every manifest in the protocol.
//!
//! The hashed canonical payload never contains its own digest and never contains volatile
//! metadata such as a creation timestamp; the digest and the volatile fields live in an
//! outer envelope. A payload that includes its own hash cannot be recomputed, and a
//! timestamp inside the hashed region makes two identical runs compare unequal.
//!
//! # The two objects, and why they are two
//!
//! [`SelectionPayload`] is **the hashed object**. It holds everything that decides what a
//! selection IS — schema and algorithm versions, profile, both fingerprints, the label
//! map, the normalization version, the seed, the shot count, the ordered examples, the
//! exclusion record, and the access ledger with its hash. It contains no digest of
//! itself.
//!
//! The outer envelope (added with `SelectionManifest`) holds the digest and the unhashed
//! volatile block. Splitting them is not tidiness: `semantic_hash == SHA256(payload)` is
//! simply false for any payload that carries `semantic_hash`, so the one-object version of
//! this schema described a digest nobody could recompute.
//!
//! # Canonical bytes
//!
//! Compact `serde_json` over structs with a fixed field order. Every keyed structure that
//! reaches these bytes is a `BTreeMap`, so nothing serialized here depends on hash
//! iteration order. Two identical runs produce byte-identical payloads.

use serde::{Deserialize, Serialize};

use crate::dedup::ExclusionRecord;
use crate::error::ContrastiveDataError;
use crate::hash::hex;
use crate::ledger::AccessRecord;
use crate::select::SelectedExample;

/// The selection-manifest schema version this build writes.
pub const SELECTION_SCHEMA_VERSION: u32 = 1;

/// Every selection-manifest schema version this build can read.
pub const SUPPORTED_SELECTION_SCHEMA_VERSIONS: &[u32] = &[1];

/// One selected row in its serialized form.
///
/// The two digests are hex rather than byte arrays because this object is read by humans
/// during an audit at least as often as by a program, and a 32-element JSON array of
/// integers is not readable. The hex rendering is lowercase and fixed-width, so it is
/// still byte-stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedExampleRecord {
    /// The row identifier.
    pub id: String,
    /// The row's class label.
    pub label: usize,
    /// Lowercase hex of the exact SHA-256 content hash.
    pub exact_hash: String,
    /// Lowercase hex of the `nfc-trim-ws-v1` normalized content hash.
    pub normalized_hash: String,
}

impl SelectedExampleRecord {
    /// Serialized form of a selected example.
    pub(crate) fn from_example(example: &SelectedExample) -> Self {
        Self {
            id: example.id.clone(),
            label: example.label,
            exact_hash: hex(&example.exact_hash),
            normalized_hash: hex(&example.normalized_hash),
        }
    }
}

/// Volatile, NEVER-hashed metadata.
///
/// # Why `created_at` is a caller-supplied string
///
/// This crate reads no clock, just as it opens no file (D-04). A timestamp minted inside
/// the library would be ambient input the caller cannot control, which is exactly what
/// makes a "deterministic" artifact irreproducible. `from_selection` therefore leaves
/// `created_at` empty and the CLI fills it in; the field is outside the hashed region, so
/// filling it changes no digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VolatileMetadata {
    /// When the manifest was written, in whatever format the caller records. Never hashed.
    pub created_at: String,
    /// The tool version that wrote it. Never hashed.
    pub tool_version: String,
}

/// The HASHED object. It contains NO digest of itself.
///
/// # What the `exclusions` field proves, and what it does not
///
/// Stated in the `revision_verified` spirit: `exclusions` proves which training ids the
/// cross-split duplicate detector removed from the selection pool **for the dataset this
/// payload's `dataset_fingerprint` names**, under the normalization version recorded
/// beside it. It does NOT prove the upstream corpus is duplicate-free, does not prove the
/// detector saw every split a consumer might later add, and says nothing about
/// near-duplicates that neither hash relation catches. A replay compares it for equality
/// against a freshly computed record; that comparison is the only claim it supports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionPayload {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Selection-algorithm version. A change here changes selected identities.
    pub algorithm_version: u32,
    /// The dataset profile the selection ran under. Always `canonical`.
    pub profile: String,
    /// Hex fingerprint of the WHOLE dataset.
    pub dataset_fingerprint: String,
    /// Hex fingerprint of the VALIDATION SPLIT ALONE — a different value from the above.
    pub validation_fingerprint: String,
    /// The ordered label map.
    pub label_names: Vec<String>,
    /// The content-normalization version the hashes were taken under.
    pub normalization_version: String,
    /// The root seed the selection was drawn from.
    pub root_seed: u64,
    /// Shots per class.
    pub shots_per_class: u32,
    /// The ORDERED selected examples: classes ascending, draw order within a class.
    pub ordered_examples: Vec<SelectedExampleRecord>,
    /// What cross-split duplication removed from the pool.
    pub exclusions: ExclusionRecord,
    /// The PERSISTED access ledger as of the moment selection finished.
    pub access_ledger: Vec<AccessRecord>,
    /// Hex `ledger_hash` of that ledger.
    pub ledger_hash: String,
}

impl SelectionPayload {
    /// Deterministic canonical serialization — the bytes whose SHA-256 IS the semantic
    /// hash.
    ///
    /// Compact JSON over a struct with a fixed field order. There is no timestamp, no
    /// path, no hostname, and no hash-ordered map anywhere inside, so two identical runs
    /// produce byte-identical output.
    ///
    /// # Errors
    ///
    /// [`ContrastiveDataError::Serialization`] if the payload cannot be serialized.
    #[provable_contracts_macros::contract(
        "contrastive-pair-protocol-v1",
        equation = "selection_canonical_payload"
    )]
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, ContrastiveDataError> {
        todo!("RED: implemented in the GREEN commit of task 2")
    }
}

#[cfg(test)]
mod payload_tests {
    use super::{SelectionPayload, SELECTION_SCHEMA_VERSION, SUPPORTED_SELECTION_SCHEMA_VERSIONS};
    use crate::hash::hex;
    use crate::ledger::AccessLedger;
    use crate::select::{test_corpus, SELECTION_ALGORITHM_VERSION};
    use sha2::{Digest, Sha256};

    fn built() -> (SelectionPayload, AccessLedger) {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset(12, &mut ledger);
        let selection = test_corpus::select(&dataset, 13, 8, &mut ledger);
        (selection.payload().clone(), ledger)
    }

    #[test]
    fn payload_bytes_contain_no_semantic_hash_key() {
        let (payload, _) = built();
        let bytes = payload.to_canonical_bytes().expect("payload serializes");
        let text = String::from_utf8(bytes).expect("canonical JSON is UTF-8");
        assert!(
            !text.contains("semantic_hash"),
            "the hashed payload must not carry its own digest — it would be unrecomputable"
        );
        // Vacuity guard: the payload really did serialize something substantial.
        assert!(
            text.contains("ordered_examples"),
            "payload text: {text:.120}"
        );
        assert!(text.contains("ledger_hash"));
    }

    #[test]
    fn payload_canonical_bytes_are_byte_stable_across_two_builds() {
        let (first, _) = built();
        let (second, _) = built();
        assert_eq!(first, second);
        assert_eq!(
            first.to_canonical_bytes().expect("first serializes"),
            second.to_canonical_bytes().expect("second serializes")
        );
    }

    /// Checker warning 1: these two fields hold DIFFERENT values. If they did not, two of
    /// the strict-replay rejection tests would be the same test wearing two names.
    #[test]
    fn payload_dataset_and_validation_fingerprints_differ() {
        let (payload, _) = built();
        assert_ne!(payload.dataset_fingerprint, payload.validation_fingerprint);
        assert_eq!(payload.dataset_fingerprint.len(), 64);
        assert_eq!(payload.validation_fingerprint.len(), 64);
    }

    /// D-16 / review finding F7: the ledger is PERSISTED inside the hashed payload, so a
    /// later phase's selection lock has an artifact to read.
    #[test]
    fn payload_carries_the_persisted_ledger_and_its_hash() {
        let (payload, ledger) = built();
        assert_eq!(payload.access_ledger, ledger.records());
        assert_eq!(payload.ledger_hash, hex(&ledger.ledger_hash()));
        // three ingest records plus the selection's own
        assert_eq!(payload.access_ledger.len(), 4);
        assert_eq!(payload.access_ledger[3].purpose, "select");
    }

    #[test]
    fn payload_records_the_versions_this_build_writes() {
        let (payload, _) = built();
        assert_eq!(payload.schema_version, SELECTION_SCHEMA_VERSION);
        assert_eq!(payload.algorithm_version, SELECTION_ALGORITHM_VERSION);
        assert!(SUPPORTED_SELECTION_SCHEMA_VERSIONS.contains(&payload.schema_version));
        assert_eq!(payload.profile, "canonical");
        assert_eq!(payload.normalization_version, "nfc-trim-ws-v1");
        assert_eq!(payload.label_names, test_corpus::label_names());
    }

    /// There is exactly ONE hashing rule for a selection, and this is it.
    #[test]
    fn payload_digest_is_the_selections_semantic_hash() {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset(12, &mut ledger);
        let selection = test_corpus::select(&dataset, 17, 8, &mut ledger);
        let bytes = selection
            .payload()
            .to_canonical_bytes()
            .expect("payload serializes");
        let expected: [u8; 32] = Sha256::digest(&bytes).into();
        assert_eq!(selection.semantic_hash(), expected);
    }

    #[test]
    fn payload_round_trips_through_serde_with_unknown_fields_denied() {
        let (payload, _) = built();
        let bytes = payload.to_canonical_bytes().expect("payload serializes");
        let restored: SelectionPayload =
            serde_json::from_slice(&bytes).expect("canonical bytes round-trip");
        assert_eq!(restored, payload);

        let mut text = String::from_utf8(bytes).expect("canonical JSON is UTF-8");
        text.insert_str(1, r#""extra":true,"#);
        let err = serde_json::from_str::<SelectionPayload>(&text);
        assert!(err.is_err(), "deny_unknown_fields must reject an added key");
    }
}
