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
use sha2::{Digest, Sha256};

use crate::dedup::ExclusionRecord;
use crate::error::ContrastiveDataError;
use crate::hash::hex;
use crate::ledger::{AccessLedger, AccessRecord};
use crate::select::{SelectedExample, Selection};

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
        serde_json::to_vec(self).map_err(|error| ContrastiveDataError::Serialization {
            context: "selection_payload".to_string(),
            detail: error.to_string(),
        })
    }
}

/// The OUTER envelope: digest, unhashed volatile block, payload.
///
/// This is the on-disk form of `selection-manifest.json`. A consumer writes
/// [`Self::to_file_bytes`] verbatim and composes no JSON of its own, so there is exactly
/// one serializer and no second place for the byte form to drift.
///
/// The three fields are public because forging one must be *possible* for the defence to
/// be meaningful: [`Selection::replay`](crate::select::Selection::replay) is what makes a
/// forged manifest useless, not the privacy of a struct field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionManifest {
    /// Lowercase hex of `SHA-256(payload.to_canonical_bytes())`.
    pub semantic_hash: String,
    /// Volatile metadata. NEVER part of the digest.
    pub volatile: VolatileMetadata,
    /// The hashed payload.
    pub payload: SelectionPayload,
}

impl SelectionManifest {
    /// Wrap the payload a [`Selection`](crate::select::Selection) already retains.
    ///
    /// This is a WRAP, not a rebuild. The selection carries the exact payload its
    /// `semantic_hash` was taken over, so re-deriving one here could only introduce a way
    /// for the two to disagree.
    ///
    /// # The ledger guard
    ///
    /// The payload embeds `access_ledger` and `ledger_hash` describing the ledger **as it
    /// stood when `select` finished**. If the caller has appended to that ledger since,
    /// the payload no longer attests the ledger being handed in, and wrapping it would
    /// publish a manifest whose persisted ledger is quietly stale. That is refused.
    ///
    /// # Errors
    ///
    /// [`ContrastiveDataError::SemanticHashMismatch`] when
    /// `ledger.ledger_hash() != sel.ledger_hash()`.
    pub fn from_selection(
        sel: &Selection,
        ledger: &AccessLedger,
    ) -> Result<Self, ContrastiveDataError> {
        let live = ledger.ledger_hash();
        if live != sel.ledger_hash() {
            return Err(ContrastiveDataError::SemanticHashMismatch {
                expected: hex(&sel.ledger_hash()),
                got: hex(&live),
            });
        }
        Ok(Self {
            semantic_hash: hex(&sel.semantic_hash()),
            volatile: VolatileMetadata {
                created_at: String::new(),
                tool_version: env!("CARGO_PKG_VERSION").to_string(),
            },
            payload: sel.payload().clone(),
        })
    }

    /// The on-disk byte form: pretty JSON plus a terminating newline.
    ///
    /// Pretty rather than compact because this file is reviewed in diffs; the DIGEST is
    /// taken over the payload's own compact canonical bytes, so the file's whitespace can
    /// be chosen for humans without weakening anything.
    ///
    /// # Errors
    ///
    /// [`ContrastiveDataError::Serialization`] if the envelope cannot be serialized.
    pub fn to_file_bytes(&self) -> Result<Vec<u8>, ContrastiveDataError> {
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|error| {
            ContrastiveDataError::Serialization {
                context: "selection_manifest".to_string(),
                detail: error.to_string(),
            }
        })?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Parse the full file form, verifying the digest BEFORE returning.
    ///
    /// A caller therefore cannot hold a `SelectionManifest` parsed from bytes whose digest
    /// does not match its payload.
    ///
    /// # Errors
    ///
    /// [`ContrastiveDataError::Serialization`] on malformed or extended JSON;
    /// [`ContrastiveDataError::SemanticHashMismatch`] when the digest disagrees.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ContrastiveDataError> {
        let manifest: Self =
            serde_json::from_slice(bytes).map_err(|error| ContrastiveDataError::Serialization {
                context: "selection_manifest".to_string(),
                detail: error.to_string(),
            })?;
        manifest.verify_digest()?;
        Ok(manifest)
    }

    /// Recompute `SHA-256(payload.to_canonical_bytes())` and compare it to the envelope.
    ///
    /// Over the manifest's OWN payload bytes — never over a payload rebuilt from live
    /// state. See `Selection::replay` for why a live rebuild would be unsatisfiable.
    pub(crate) fn verify_digest(&self) -> Result<(), ContrastiveDataError> {
        let digest: [u8; 32] = Sha256::digest(self.payload.to_canonical_bytes()?).into();
        let recomputed = hex(&digest);
        if recomputed == self.semantic_hash {
            return Ok(());
        }
        Err(ContrastiveDataError::SemanticHashMismatch {
            expected: self.semantic_hash.clone(),
            got: recomputed,
        })
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

/// The FROZEN golden corpus and the four committed selection goldens.
///
/// # No filesystem, by construction
///
/// Every byte here arrives through `include_bytes!`, whose paths resolve against THIS
/// SOURCE FILE at compile time. The verifier is therefore working-directory independent
/// and adds no `std::fs` to `src/` — which `make contrastive-data-boundary` bans outright,
/// with no `cfg(test)` exemption.
///
/// # What is algorithm-derived and what is capture-and-blessed
///
/// Stated plainly, because the two are not equally strong evidence:
///
/// * **Algorithm-derived.** [`GOLDEN_CASES`]'s `ordered_ids_sha256` values were computed
///   by an independent Python implementation written from the contract equations
///   (`rng_key_derivation`, `bounded_draw`, `few_shot_selection`) and from Salmon et al.
///   (2011), which never read this crate's source. They pin the SELECTION itself — which
///   rows, in which order — against a second implementation.
/// * **Capture-and-blessed.** The `*.payload.json` files are this crate's own canonical
///   serialization of those selections, written once by `tests/goldens_regenerate.rs`.
///   They pin the BYTE FORM against future drift; they do not independently corroborate
///   it. Their content is corroborated by the digests above.
///
/// Re-baselining is `cargo test -p aprender-contrastive-data --test goldens_regenerate --
/// --ignored`, and it must be a reviewed diff.
#[cfg(test)]
mod golden_tests {
    use super::{SelectionManifest, SelectionPayload};
    use crate::hash::hex;
    use crate::ledger::AccessLedger;
    use crate::prepared::{Canonical, CanonicalDeclarations, PreparedDataset};
    use crate::schema::parse_jsonl_bytes;
    use crate::select::{FewShotSelector, Selection, SelectionConfig};
    use crate::split::SplitDeclaration;
    use sha2::{Digest, Sha256};

    pub(super) const TRAIN_JSONL: &[u8] =
        include_bytes!("../tests/goldens/golden_corpus_train.jsonl");
    pub(super) const VALIDATION_JSONL: &[u8] =
        include_bytes!("../tests/goldens/golden_corpus_validation.jsonl");
    pub(super) const TEST_JSONL: &[u8] =
        include_bytes!("../tests/goldens/golden_corpus_test.jsonl");
    const SHA256_MANIFEST: &[u8] = include_bytes!("../tests/goldens/manifest.sha256");

    /// `(seed, shots, golden payload bytes, independently derived ordered-id digest)`.
    const GOLDEN_CASES: [(u64, u32, &[u8], &str); 4] = [
        (
            13,
            8,
            include_bytes!("../tests/goldens/selection_seed13_shots8.payload.json"),
            "1c99eec4d905430e4b5d05471a01af99f27b4ef65707767f2765cb10ef57701c",
        ),
        (
            13,
            16,
            include_bytes!("../tests/goldens/selection_seed13_shots16.payload.json"),
            "1ea9826fd29e0a298c911097d973e3ca4eb7d804bd542b481264804cd658ffaa",
        ),
        (
            17,
            8,
            include_bytes!("../tests/goldens/selection_seed17_shots8.payload.json"),
            "7bb11c386e9622d151c83d6a3471b56a21da5c32fc86c9f5b62d97e91d64c763",
        ),
        (
            17,
            16,
            include_bytes!("../tests/goldens/selection_seed17_shots16.payload.json"),
            "ca7c7c4c291beb63b9e878428e46f6a9217c23d0742043dd9e7489398f9dd301",
        ),
    ];

    /// Every file the committed `manifest.sha256` covers, paired with its embedded bytes.
    fn covered_files() -> Vec<(&'static str, &'static [u8])> {
        let mut files: Vec<(&str, &[u8])> = vec![
            ("golden_corpus_train.jsonl", TRAIN_JSONL),
            ("golden_corpus_validation.jsonl", VALIDATION_JSONL),
            ("golden_corpus_test.jsonl", TEST_JSONL),
        ];
        for (seed, shots, bytes, _) in GOLDEN_CASES {
            files.push((golden_name(seed, shots), bytes));
        }
        files
    }

    fn golden_name(seed: u64, shots: u32) -> &'static str {
        match (seed, shots) {
            (13, 8) => "selection_seed13_shots8.payload.json",
            (13, 16) => "selection_seed13_shots16.payload.json",
            (17, 8) => "selection_seed17_shots8.payload.json",
            (17, 16) => "selection_seed17_shots16.payload.json",
            other => panic!("no golden is committed for {other:?}"),
        }
    }

    /// The golden corpus's declarations. FROZEN alongside the corpus files.
    pub(super) fn declarations() -> CanonicalDeclarations {
        let label_names = ["none", "against", "favor"]
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<String>>();
        let decl = |counts: Vec<usize>| SplitDeclaration {
            expected_class_counts: counts,
            label_names: label_names.clone(),
        };
        CanonicalDeclarations {
            train: decl(vec![20, 20, 20]),
            validation: decl(vec![1, 1, 1]),
            test: decl(vec![1, 1, 1]),
            label_names,
        }
    }

    pub(super) fn golden_dataset(ledger: &mut AccessLedger) -> PreparedDataset<Canonical> {
        let parse = |bytes: &[u8], role: &str| {
            parse_jsonl_bytes(bytes, role).expect("the golden corpus must parse")
        };
        PreparedDataset::<Canonical>::from_labeled_rows(
            parse(TRAIN_JSONL, "train"),
            parse(VALIDATION_JSONL, "validation"),
            parse(TEST_JSONL, "test"),
            &declarations(),
            ledger,
        )
        .expect("the golden corpus must be a valid canonical dataset")
    }

    pub(super) fn golden_selection(seed: u64, shots: u32) -> (Selection, AccessLedger) {
        let mut ledger = AccessLedger::new();
        let dataset = golden_dataset(&mut ledger);
        let selection = FewShotSelector::select(
            &dataset,
            &SelectionConfig {
                root_seed: seed,
                shots_per_class: shots,
            },
            &mut ledger,
        )
        .expect("the golden corpus must support 8 and 16 shots");
        (selection, ledger)
    }

    /// The corpus carries exactly one cross-split duplicate, so the goldens exercise a
    /// NON-EMPTY exclusion record rather than only the easy path.
    #[test]
    fn golden_corpus_has_the_shape_the_goldens_were_derived_from() {
        let mut ledger = AccessLedger::new();
        let dataset = golden_dataset(&mut ledger);
        assert_eq!(dataset.train().rows().len(), 60);
        assert_eq!(dataset.validation().rows().len(), 3);
        assert_eq!(dataset.test().rows().len(), 3);
        assert_eq!(
            dataset.exclusions().excluded_train_ids(),
            ["train:0-07".to_string()],
            "the frozen corpus must exclude exactly this row"
        );
        assert_eq!(dataset.exclusions().reduced_pools().get(&0), Some(&19));

        // Two rows carry a whitespace variant, so the goldens pin the NORMALIZED hash as a
        // value distinct from the exact one. Without them every recorded pair would be
        // identical and the goldens would say nothing about `nfc-trim-ws-v1`.
        let train = dataset.train();
        let differing = train
            .rows()
            .iter()
            .filter(|row| train.exact_hash_of(&row.id) != train.normalized_hash_of(&row.id))
            .count();
        assert_eq!(
            differing, 2,
            "the frozen corpus must contain exactly two whitespace-variant rows"
        );
    }

    #[test]
    fn golden_files_match_the_committed_sha256_manifest() {
        let text = core::str::from_utf8(SHA256_MANIFEST).expect("manifest.sha256 is UTF-8");
        let recorded: Vec<(&str, &str)> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let (digest, name) = line
                    .split_once("  ")
                    .expect("each manifest line is `<hex>  <name>`");
                (name, digest)
            })
            .collect();

        let files = covered_files();
        // Vacuity guard: pin the population BEFORE asserting a relation over it, so an
        // empty manifest cannot satisfy an empty comparison (02-04's lesson).
        assert_eq!(files.len(), 7);
        assert_eq!(recorded.len(), 7, "manifest.sha256 must cover all 7 files");

        for (name, bytes) in files {
            let digest = hex(&Sha256::digest(bytes).into());
            let expected = recorded
                .iter()
                .find(|(entry, _)| *entry == name)
                .unwrap_or_else(|| panic!("manifest.sha256 has no entry for {name}"))
                .1;
            assert_eq!(digest, expected, "digest drift in {name}");
        }
    }

    /// The four committed payload goldens are byte-identical to what this build produces.
    #[test]
    fn golden_selection_payload_bytes_match_the_committed_goldens() {
        for (seed, shots, expected, _) in GOLDEN_CASES {
            let (selection, _) = golden_selection(seed, shots);
            let produced = selection
                .payload()
                .to_canonical_bytes()
                .expect("payload serializes");
            assert_eq!(
                produced,
                expected,
                "golden {} drifted",
                golden_name(seed, shots)
            );
        }
    }

    /// The independently derived half: which rows, in which order.
    ///
    /// These digests came from a Python implementation of the contract equations that has
    /// never read this crate. If this test and the byte-golden test disagree, the byte
    /// golden is the one that was re-blessed.
    #[test]
    fn golden_ordered_ids_match_the_independently_derived_digests() {
        for (seed, shots, _, expected) in GOLDEN_CASES {
            let (selection, _) = golden_selection(seed, shots);
            let joined = selection.ordered_ids().join("\n");
            let digest = hex(&Sha256::digest(joined.as_bytes()).into());
            assert_eq!(
                digest, expected,
                "seed {seed} shots {shots}: ordered ids disagree with the reference derivation"
            );
            assert_eq!(selection.len() as u32, shots * 3);
        }
    }

    /// A golden payload must parse back into the payload it was written from, so a golden
    /// cannot be a well-formed file describing something else.
    #[test]
    fn golden_payloads_round_trip_into_equal_manifests() {
        for (seed, shots, bytes, _) in GOLDEN_CASES {
            let (selection, ledger) = golden_selection(seed, shots);
            let parsed: SelectionPayload =
                serde_json::from_slice(bytes).expect("a golden payload must parse");
            assert_eq!(&parsed, selection.payload());

            let manifest =
                SelectionManifest::from_selection(&selection, &ledger).expect("wrap succeeds");
            let round_tripped =
                SelectionManifest::from_bytes(&manifest.to_file_bytes().expect("file bytes"))
                    .expect("the file form verifies its own digest");
            assert_eq!(round_tripped.payload, parsed);
        }
    }
}

#[cfg(test)]
mod manifest_tests {
    use super::{SelectionManifest, VolatileMetadata};
    use crate::error::ContrastiveDataError;
    use crate::hash::hex;
    use crate::ledger::AccessLedger;
    use crate::select::test_corpus;

    fn wrapped() -> (SelectionManifest, AccessLedger) {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset(12, &mut ledger);
        let selection = test_corpus::select(&dataset, 23, 8, &mut ledger);
        let manifest =
            SelectionManifest::from_selection(&selection, &ledger).expect("the wrap succeeds");
        (manifest, ledger)
    }

    /// `created_at` lives OUTSIDE the hashed region, so two manifests of the same
    /// selection that differ only in when they were written are the same artifact.
    #[test]
    fn manifest_semantic_hash_ignores_volatile_metadata() {
        let (mut first, _) = wrapped();
        let (mut second, _) = wrapped();
        first.volatile = VolatileMetadata {
            created_at: "2026-08-09T00:00:00Z".to_string(),
            tool_version: "0.0.1-alpha".to_string(),
        };
        second.volatile = VolatileMetadata {
            created_at: "2031-01-01T12:34:56Z".to_string(),
            tool_version: "9.9.9".to_string(),
        };

        assert_ne!(first.volatile, second.volatile);
        assert_eq!(first.semantic_hash, second.semantic_hash);
        assert_eq!(first.payload, second.payload);
        first.verify_digest().expect("digest still verifies");
        second.verify_digest().expect("digest still verifies");
    }

    /// Checker warning 2: the payload attests the ledger as it stood when `select`
    /// finished, so a ledger that has grown since is not the one it describes.
    #[test]
    fn manifest_from_selection_refuses_a_ledger_that_has_drifted() {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset(12, &mut ledger);
        let selection = test_corpus::select(&dataset, 29, 8, &mut ledger);
        SelectionManifest::from_selection(&selection, &ledger).expect("the matching ledger wraps");

        ledger.record("train", "canonical", "something-else", "aa");
        let err = SelectionManifest::from_selection(&selection, &ledger)
            .expect_err("a drifted ledger must be refused");
        match err {
            ContrastiveDataError::SemanticHashMismatch { expected, got } => {
                assert_eq!(expected, hex(&selection.ledger_hash()));
                assert_eq!(got, hex(&ledger.ledger_hash()));
                assert_ne!(expected, got);
            }
            other => panic!("expected SemanticHashMismatch, got {other:?}"),
        }
    }

    #[test]
    fn manifest_file_form_round_trips_every_payload_field() {
        let (manifest, _) = wrapped();
        let bytes = manifest.to_file_bytes().expect("file bytes");
        assert_eq!(
            bytes.last(),
            Some(&b'\n'),
            "the file form ends in a newline"
        );

        let restored = SelectionManifest::from_bytes(&bytes).expect("round-trip");
        assert_eq!(restored, manifest);
        assert_eq!(restored.payload, manifest.payload);

        // Two file forms differing ONLY in volatile metadata parse to equal payloads and
        // equal digests.
        let mut other = manifest.clone();
        other.volatile.created_at = "1999-12-31T23:59:59Z".to_string();
        let other_bytes = other.to_file_bytes().expect("file bytes");
        assert_ne!(other_bytes, bytes);
        let other_restored = SelectionManifest::from_bytes(&other_bytes).expect("round-trip");
        assert_eq!(other_restored.payload, restored.payload);
        assert_eq!(other_restored.semantic_hash, restored.semantic_hash);
    }

    #[test]
    fn manifest_from_bytes_rejects_a_digest_that_disagrees_with_its_payload() {
        let (mut manifest, _) = wrapped();
        let honest = manifest.semantic_hash.clone();
        manifest.semantic_hash = "0".repeat(64);
        let bytes = manifest.to_file_bytes().expect("file bytes");

        let err = SelectionManifest::from_bytes(&bytes)
            .expect_err("a disagreeing digest must be refused before the value is returned");
        match err {
            ContrastiveDataError::SemanticHashMismatch { expected, got } => {
                assert_eq!(expected, "0".repeat(64));
                assert_eq!(got, honest);
            }
            other => panic!("expected SemanticHashMismatch, got {other:?}"),
        }
    }

    #[test]
    fn manifest_from_bytes_rejects_an_unknown_envelope_field() {
        let (manifest, _) = wrapped();
        let mut text =
            String::from_utf8(manifest.to_file_bytes().expect("file bytes")).expect("UTF-8");
        text.insert_str(1, r#""rogue":1,"#);
        let err = SelectionManifest::from_bytes(text.as_bytes())
            .expect_err("deny_unknown_fields must reject an added envelope key");
        assert!(matches!(err, ContrastiveDataError::Serialization { .. }));
    }

    /// Review finding F7: the persisted ledger is a real ledger, not a decorative copy.
    #[test]
    fn manifest_persisted_ledger_reproduces_its_hash_through_access_ledger() {
        let (manifest, live) = wrapped();
        let records = serde_json::to_string(&manifest.payload.access_ledger)
            .expect("the persisted records serialize");
        let wire = format!(r#"{{"schema_version":1,"records":{records}}}"#);
        let rebuilt = AccessLedger::from_bytes(wire.as_bytes())
            .expect("the persisted records parse as a ledger");

        assert_eq!(rebuilt.records(), live.records());
        assert_eq!(hex(&rebuilt.ledger_hash()), manifest.payload.ledger_hash);
    }
}
