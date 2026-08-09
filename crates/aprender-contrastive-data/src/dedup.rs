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
//! # Why coalescing is not an optimization
//!
//! `hash.rs` proves, as a property test, that an exact-hash collision implies a
//! normalized-hash collision. So the two edge kinds are not independent: every exact
//! duplicate appears in BOTH groupings. Emitting one group per key would remove the same
//! training row twice from the same class pool, understating the pool. A pool understated
//! near the boundary produces a `CrossSplitDuplicateUnderflow` at selection time — a
//! failure invented by the detector rather than present in the data.
//!
//! # Why nothing here returns `Err`
//!
//! [`coalesced_exclusions`] returns an [`ExclusionRecord`], not a `Result`, and that is a
//! decision rather than an omission (D-18, upheld verbatim by D-27). Hard-failing at
//! SELECTION time would make failures seed-dependent, so a subset of benchmark cells would
//! die and a completeness gate would reject the run for a reason unrelated to the method.
//! Hard-failing at PREPARE time would hand upstream data quality a veto over the whole
//! dataset. The only real failure is a reduced pool that can no longer supply the
//! requested shots, and that is raised where the shots are known: at selection.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::ContrastiveDataError;
use crate::hash::CONTENT_NORMALIZATION_VERSION;
use crate::schema::LabeledExample;

/// Which detection kinds fired inside one duplicate component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DetectionKinds {
    /// At least one pair of members shares an exact content hash.
    pub exact: bool,
    /// At least one pair of members shares a normalized content hash.
    pub normalized: bool,
}

/// One connected component of duplicate content spanning at least two split roles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateGroup {
    /// `(split_role, id)` members, sorted ascending.
    pub members: Vec<(String, String)>,
    /// Which detection kinds fired inside this component.
    pub detected_by: DetectionKinds,
    /// True when the members do not all carry the same label — impossible to reconcile
    /// automatically, and therefore worth surfacing rather than silently excluding.
    pub label_conflict: bool,
}

/// The deterministic record of everything cross-split duplication removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExclusionRecord {
    excluded_train_ids: Vec<String>,
    groups: Vec<DuplicateGroup>,
    reduced_pools: BTreeMap<usize, u64>,
    normalization_version: String,
}

impl ExclusionRecord {
    /// Training ids removed from the selection pool, sorted ascending.
    pub fn excluded_train_ids(&self) -> &[String] {
        &self.excluded_train_ids
    }

    /// Remaining training pool size per class label, after exclusion.
    pub fn reduced_pools(&self) -> &BTreeMap<usize, u64> {
        &self.reduced_pools
    }

    /// The duplicate components, sorted deterministically.
    pub fn groups(&self) -> &[DuplicateGroup] {
        &self.groups
    }

    /// Deterministic canonical serialization.
    ///
    /// # Errors
    ///
    /// [`ContrastiveDataError::Serialization`] if the record cannot be serialized.
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, ContrastiveDataError> {
        serde_json::to_vec(self).map_err(|error| ContrastiveDataError::Serialization {
            context: "exclusion_record".to_string(),
            detail: error.to_string(),
        })
    }

    /// SHA-256 of [`Self::to_canonical_bytes`].
    ///
    /// Total for the same reason the ledger's hash is: the canonical form is strings,
    /// integers and booleans, with every map a `BTreeMap<usize, u64>` whose keys
    /// serialize as strings. `serde_json` has no failure mode to report here.
    pub fn hash(&self) -> [u8; 32] {
        let bytes = self
            .to_canonical_bytes()
            .expect("ExclusionRecord canonical form is strings, integers and bools");
        Sha256::digest(bytes).into()
    }
}

/// Coalesce cross-split duplicate content into connected components.
///
/// Deterministic and total. `splits` is `(role, rows)` for every split of one dataset.
pub(crate) fn coalesced_exclusions(
    splits: &[(&'static str, &[LabeledExample])],
) -> ExclusionRecord {
    let _ = splits;
    ExclusionRecord {
        excluded_train_ids: Vec::new(),
        groups: Vec::new(),
        reduced_pools: BTreeMap::new(),
        normalization_version: CONTENT_NORMALIZATION_VERSION.to_string(),
    }
}

#[cfg(test)]
mod dedup_tests {
    use super::coalesced_exclusions;
    use crate::hash::CONTENT_NORMALIZATION_VERSION;
    use crate::schema::LabeledExample;

    fn row(id: &str, input: &str, label: usize, split: &str) -> LabeledExample {
        LabeledExample {
            id: id.to_string(),
            input: input.to_string(),
            label,
            label_text: ["none", "against", "favor"][label].to_string(),
            source_split: split.to_string(),
        }
    }

    fn train_base() -> Vec<LabeledExample> {
        vec![
            row("train:0", "alpha post", 0, "train"),
            row("train:1", "beta post", 1, "train"),
            row("train:2", "gamma post", 2, "train"),
        ]
    }

    /// Fixture A — a train row byte-identical to a validation row, same label.
    #[test]
    fn dedup_fixture_a_exact_duplicate_is_one_group_and_one_decrement() {
        let train = train_base();
        let validation = vec![row("validation:0", "alpha post", 0, "validation")];
        let record = coalesced_exclusions(&[("train", &train), ("validation", &validation)]);

        assert_eq!(record.excluded_train_ids(), ["train:0".to_string()]);
        assert_eq!(
            record.groups().len(),
            1,
            "an exact duplicate is also a normalized duplicate; it must not be two groups"
        );
        let group = &record.groups()[0];
        assert!(group.detected_by.exact, "exact edge must be recorded");
        assert!(
            group.detected_by.normalized,
            "an exact duplicate always co-fires the normalized edge"
        );
        assert!(!group.label_conflict);
        assert_eq!(
            group.members,
            vec![
                ("train".to_string(), "train:0".to_string()),
                ("validation".to_string(), "validation:0".to_string()),
            ]
        );
        assert_eq!(record.reduced_pools().get(&0), Some(&0));
        assert_eq!(record.reduced_pools().get(&1), Some(&1));
        assert_eq!(record.reduced_pools().get(&2), Some(&1));
    }

    /// Fixture B — differs from a test row only by trailing whitespace.
    #[test]
    fn dedup_fixture_b_whitespace_variant_is_normalized_only() {
        let train = train_base();
        let test = vec![row("test:0", "beta post  ", 1, "test")];
        let record = coalesced_exclusions(&[("train", &train), ("test", &test)]);

        assert_eq!(record.excluded_train_ids(), ["train:1".to_string()]);
        assert_eq!(record.groups().len(), 1);
        let group = &record.groups()[0];
        assert!(
            !group.detected_by.exact,
            "the bytes differ, so no exact edge exists"
        );
        assert!(group.detected_by.normalized);
        assert_eq!(record.reduced_pools().get(&1), Some(&0));
    }

    /// Fixture C — duplicate content across splits with DIFFERENT labels. Real data has
    /// none, so this path can only be reached synthetically.
    #[test]
    fn dedup_fixture_c_label_conflict_is_flagged() {
        let train = train_base();
        let validation = vec![row("validation:0", "gamma post", 0, "validation")];
        let record = coalesced_exclusions(&[("train", &train), ("validation", &validation)]);

        assert_eq!(record.excluded_train_ids(), ["train:2".to_string()]);
        assert_eq!(record.groups().len(), 1);
        assert!(record.groups()[0].label_conflict);
    }

    /// Fixture D — a three-way chain. `train:0` equals `validation:0` exactly, and
    /// `validation:0` equals `test:0` only after normalization. Union-find must merge all
    /// three into ONE component and decrement the train pool exactly once.
    #[test]
    fn dedup_fixture_d_three_way_chain_is_one_component() {
        let train = train_base();
        let validation = vec![row("validation:0", "alpha post", 0, "validation")];
        let test = vec![row("test:0", "  alpha   post ", 0, "test")];
        let record = coalesced_exclusions(&[
            ("test", &test),
            ("train", &train),
            ("validation", &validation),
        ]);

        assert_eq!(record.groups().len(), 1, "the chain is ONE component");
        let group = &record.groups()[0];
        assert_eq!(group.members.len(), 3);
        assert!(group.detected_by.exact);
        assert!(group.detected_by.normalized);
        assert_eq!(record.excluded_train_ids(), ["train:0".to_string()]);
        assert_eq!(record.reduced_pools().get(&0), Some(&0));
    }

    #[test]
    fn dedup_no_duplicates_leaves_the_pools_intact() {
        let train = train_base();
        let validation = vec![row("validation:0", "delta post", 0, "validation")];
        let record = coalesced_exclusions(&[("train", &train), ("validation", &validation)]);

        assert!(record.excluded_train_ids().is_empty());
        assert!(record.groups().is_empty());
        assert_eq!(record.reduced_pools().get(&0), Some(&1));
        assert_eq!(record.reduced_pools().get(&1), Some(&1));
        assert_eq!(record.reduced_pools().get(&2), Some(&1));
        assert!(!record.to_canonical_bytes().expect("serializes").is_empty());
    }

    #[test]
    fn dedup_records_the_normalization_version() {
        let train = train_base();
        let record = coalesced_exclusions(&[("train", &train)]);
        let json = String::from_utf8(record.to_canonical_bytes().expect("serializes"))
            .expect("canonical bytes are UTF-8");
        assert!(json.contains(CONTENT_NORMALIZATION_VERSION));
    }

    /// A record that reaches a manifest reaches a hash, so its content must not depend on
    /// the order the caller happened to collect rows in.
    #[test]
    fn dedup_is_order_independent_in_both_record_and_hash() {
        let mut train = train_base();
        train.push(row("train:3", "alpha post", 0, "train"));
        let validation = vec![
            row("validation:0", "alpha post", 0, "validation"),
            row("validation:1", "beta post ", 1, "validation"),
        ];
        let forward = coalesced_exclusions(&[("train", &train), ("validation", &validation)]);

        let mut permuted_train = train.clone();
        permuted_train.reverse();
        let mut permuted_validation = validation.clone();
        permuted_validation.reverse();
        let backward = coalesced_exclusions(&[
            ("validation", &permuted_validation),
            ("train", &permuted_train),
        ]);

        assert_eq!(forward, backward);
        assert_eq!(forward.hash(), backward.hash());
    }

    #[test]
    fn dedup_hash_changes_when_the_excluded_set_changes() {
        let train = train_base();
        let clean = coalesced_exclusions(&[("train", &train)]);
        let validation = vec![row("validation:0", "alpha post", 0, "validation")];
        let dirty = coalesced_exclusions(&[("train", &train), ("validation", &validation)]);
        assert_ne!(clean.hash(), dirty.hash());
    }

    #[test]
    fn dedup_within_split_duplicate_content_is_not_a_cross_split_group() {
        // Two train rows with identical content are NOT evaluation leakage; only content
        // spanning two split roles is.
        let train = vec![
            row("train:0", "alpha post", 0, "train"),
            row("train:1", "alpha post", 0, "train"),
        ];
        let record = coalesced_exclusions(&[("train", &train)]);
        assert!(record.groups().is_empty());
        assert!(record.excluded_train_ids().is_empty());
        assert_eq!(record.reduced_pools().get(&0), Some(&2));
    }
}
