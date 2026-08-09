//! Balanced few-shot selection and the ordered selected-ID manifest model.
//!
//! Partial Fisher-Yates over sorted per-class buckets, with the swap index for step *i*
//! drawn at ordinal *i* in that class's domain. The output order IS the draw order, which
//! is what makes the ordered manifest statable as a contract equation rather than as an
//! implementation detail.
//!
//! # One dataset value in, one selection out
//!
//! [`FewShotSelector::select`] takes a single `&PreparedDataset<Canonical>`. That one
//! argument supplies the training pool, the validation witness, the exclusion record and
//! the fingerprint, so there is no signature into which a caller could feed a training
//! split from one dataset and a validation split from another. Witness mixing is not
//! rejected here; it is unrepresentable. A compatibility dataset is a different type and
//! cannot be passed at all (D-19).
//!
//! # Every selected row carries its LABEL
//!
//! [`SelectedExample`] holds `(id, label, exact_hash, normalized_hash)` and [`SelectedId`]
//! is an opaque ordinal into one [`Selection`]. Downstream pair construction therefore
//! derives its targets from real per-row labels rather than from class-size totals, which
//! would silently mislabel any layout the totals happen to be symmetric in.
//!
//! # Determinism, and what it does and does not survive
//!
//! The ordered selection is a pure function of `(post-exclusion sorted pools, root_seed,
//! shots_per_class)`. It survives thread count, iteration order and permuted ingest order,
//! because the buckets sort before any draw and draw *i* is a pure function of *i*.
//!
//! The `semantic_hash` deliberately does NOT survive permuted ingest order, and that is
//! correct rather than a gap: the payload embeds `dataset_fingerprint`, which is partly a
//! digest of the split's canonical JSONL bytes *in ingest order*. A permuted file is
//! different bytes and therefore a different dataset for provenance purposes. The
//! permutation test below asserts both halves — same selection, different fingerprint —
//! so neither can regress unnoticed.

use core::num::NonZeroU64;
use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::buckets::ClassBuckets;
use crate::error::ContrastiveDataError;
use crate::hash::{hex, CONTENT_NORMALIZATION_VERSION};
use crate::ledger::AccessLedger;
use crate::manifest::{SelectedExampleRecord, SelectionPayload, SELECTION_SCHEMA_VERSION};
use crate::prepared::{Canonical, DatasetProfile, PreparedDataset};
use crate::rng::{bounded, derive_key, domains};
use crate::split::{SplitRole, Train};

/// The selection-algorithm version. A change here changes selected IDENTITIES, which is
/// why it is versioned separately from the manifest schema.
pub const SELECTION_ALGORITHM_VERSION: u32 = 1;

/// The contracted shot counts.
const ALLOWED_SHOTS: [u32; 4] = [8, 16, 32, 64];

/// Rendered form of [`ALLOWED_SHOTS`] for the typed error.
const ALLOWED_SHOTS_TEXT: &str = "{8, 16, 32, 64}";

/// The access-ledger purpose recorded by a selection.
const SELECT_PURPOSE: &str = "select";

/// What a caller asks a selection for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionConfig {
    /// The root seed. Every draw key is derived from this plus a domain string.
    pub root_seed: u64,
    /// Shots per class — one of 8, 16, 32, 64.
    pub shots_per_class: u32,
}

/// One selected row, with its LABEL and both content hashes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedExample {
    /// The row identifier.
    pub id: String,
    /// The row's class label.
    pub label: usize,
    /// Exact SHA-256 of the raw input bytes.
    pub exact_hash: [u8; 32],
    /// `nfc-trim-ws-v1` normalized content hash.
    pub normalized_hash: [u8; 32],
}

/// An opaque ordinal into ONE [`Selection`].
///
/// The field is private and the constructor is private to this module, so a value of this
/// type can only have come from a `Selection`'s own accessors. That is what makes pair
/// endpoints structurally non-leaky downstream: there is no way to name a row that was
/// never selected.
///
/// An ordinal taken from one selection and applied to another is a programming error, not
/// a data error; the accessors panic with a named message rather than returning a
/// plausible row from the wrong selection.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SelectedId(u32);

/// A completed few-shot selection.
#[derive(Debug, Clone)]
pub struct Selection {
    ordered: Vec<SelectedExample>,
    by_id: BTreeMap<String, SelectedId>,
    by_class: BTreeMap<usize, Vec<SelectedId>>,
    class_sizes: Vec<(usize, u64)>,
    payload: SelectionPayload,
    semantic_hash: [u8; 32],
    ledger_hash: [u8; 32],
}

/// Balanced few-shot selection over one canonical prepared dataset.
#[derive(Debug, Clone, Copy)]
pub struct FewShotSelector;

impl FewShotSelector {
    /// Select `shots_per_class` examples from every declared class.
    ///
    /// Fail-closed first: an invalid shot count and an exhausted class pool are both
    /// rejected BEFORE any draw, so an invalid request never consumes an ordinal and never
    /// leaves a partially built selection behind.
    ///
    /// Then, per class in ASCENDING label order, a partial Fisher-Yates over that class's
    /// sorted post-exclusion pool: for step `i`, swap index
    /// `j = i + bounded(key, 0, i, pool_len - i)` where `key = derive_key(root_seed,
    /// "select/{label}")`. The first `shots_per_class` slots IN DRAW ORDER are that class's
    /// ordered selection, and the classes concatenated in ascending label order ARE the
    /// manifest order DATA-03 contracts.
    ///
    /// Each row's two hashes are copied from the split that already computed them — never
    /// re-derived here, so there is exactly one source of truth for a row's identity.
    ///
    /// # Errors
    ///
    /// [`ContrastiveDataError::InvalidShots`] for a shot count outside `{8, 16, 32, 64}`;
    /// [`ContrastiveDataError::CrossSplitDuplicateUnderflow`] when a class's post-exclusion
    /// pool cannot supply the requested shots (D-27's pool-exhaustion half);
    /// [`ContrastiveDataError::Serialization`] if the canonical payload cannot be built.
    #[provable_contracts_macros::contract(
        "contrastive-pair-protocol-v1",
        equation = "few_shot_selection"
    )]
    pub fn select(
        dataset: &PreparedDataset<Canonical>,
        cfg: &SelectionConfig,
        ledger: &mut AccessLedger,
    ) -> Result<Selection, ContrastiveDataError> {
        todo!("RED: implemented in the GREEN commit of task 2")
    }
}

/// The pure selection: no ledger, no payload, no hashing.
///
/// Split out because `Selection::replay` must recompute exactly this and compare, and it
/// cannot do so through `select` — `select` appends to the ledger, and a replay that
/// appended twice would produce a payload that disagrees with the manifest it is
/// validating.
pub(crate) fn compute_ordered(
    dataset: &PreparedDataset<Canonical>,
    root_seed: u64,
    shots_per_class: u32,
) -> Result<Vec<SelectedExample>, ContrastiveDataError> {
    todo!("RED: implemented in the GREEN commit of task 2")
}

impl Selection {
    /// Build the value from an ordered list and the payload it is attested by.
    pub(crate) fn assemble(
        ordered: Vec<SelectedExample>,
        payload: SelectionPayload,
        ledger_hash: [u8; 32],
    ) -> Result<Self, ContrastiveDataError> {
        todo!("RED: implemented in the GREEN commit of task 2")
    }

    /// The ordered selected examples: classes ascending, draw order within a class.
    pub fn examples(&self) -> &[SelectedExample] {
        &self.ordered
    }

    /// The ordered selected ids — the list DATA-03 contracts and Phase 5 replays.
    pub fn ordered_ids(&self) -> Vec<&str> {
        self.ordered.iter().map(|row| row.id.as_str()).collect()
    }

    /// How many rows were selected in total.
    pub fn len(&self) -> usize {
        self.ordered.len()
    }

    /// Whether the selection is empty. Never true for a selection this crate produced —
    /// `shots_per_class` is at least 8 — but required beside `len`.
    pub fn is_empty(&self) -> bool {
        self.ordered.is_empty()
    }

    /// `SHA-256` of the canonical payload bytes.
    pub fn semantic_hash(&self) -> [u8; 32] {
        self.semantic_hash
    }

    /// The access-ledger hash as of the moment this selection was built — including the
    /// selection's own record.
    pub fn ledger_hash(&self) -> [u8; 32] {
        self.ledger_hash
    }

    /// The payload this selection is attested by. Retained, never rebuilt.
    pub fn payload(&self) -> &SelectionPayload {
        &self.payload
    }

    /// Hex fingerprint of the whole dataset the selection came from.
    pub fn dataset_fingerprint_hex(&self) -> &str {
        &self.payload.dataset_fingerprint
    }

    /// Hex fingerprint of the validation split alone.
    pub fn validation_fingerprint_hex(&self) -> &str {
        &self.payload.validation_fingerprint
    }

    /// The root seed this selection was drawn from.
    pub fn root_seed(&self) -> u64 {
        self.payload.root_seed
    }

    /// Shots per class.
    pub fn shots_per_class(&self) -> u32 {
        self.payload.shots_per_class
    }

    /// SELECTED rows per class, ascending by label. Every entry equals
    /// `shots_per_class`; the vector exists so a consumer can read the class set without
    /// walking the ordered list.
    pub fn class_sizes(&self) -> &[(usize, u64)] {
        &self.class_sizes
    }

    /// The ordinal of a selected id, or `None` when the id was not selected.
    ///
    /// This is the ONLY way to obtain a [`SelectedId`], which is what makes the type a
    /// proof of membership.
    pub fn selected_id(&self, id: &str) -> Option<SelectedId> {
        self.by_id.get(id).copied()
    }

    /// The id behind an ordinal.
    ///
    /// # Panics
    ///
    /// If `selected` came from a DIFFERENT selection and is out of range here.
    pub fn id_of(&self, selected: SelectedId) -> &str {
        self.example_of(selected).id.as_str()
    }

    /// The class label behind an ordinal.
    ///
    /// # Panics
    ///
    /// If `selected` came from a DIFFERENT selection and is out of range here.
    pub fn label_of(&self, selected: SelectedId) -> usize {
        self.example_of(selected).label
    }

    /// The full row behind an ordinal.
    ///
    /// # Panics
    ///
    /// If `selected` came from a DIFFERENT selection and is out of range here. A
    /// `SelectedId` is proof of membership in the selection that produced it, not in every
    /// selection, and returning a plausible row from the wrong one would be worse than a
    /// named panic.
    pub fn example_of(&self, selected: SelectedId) -> &SelectedExample {
        self.ordered
            .get(selected.0 as usize)
            .expect("SelectedId ordinals are produced only by the Selection they index")
    }

    /// Every selected ordinal of one class, ascending. An unknown label yields an empty
    /// slice.
    pub fn ids_in_class(&self, label: usize) -> &[SelectedId] {
        self.by_class.get(&label).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
pub(crate) mod test_corpus {
    //! A synthetic three-class corpus shared by every test in this crate that needs a
    //! prepared dataset.
    //!
    //! Training rows are emitted INTERLEAVED by class and with ids that descend within a
    //! class, so ingest order is neither grouped nor sorted. A bucketing implementation
    //! that merely preserved ingest order would produce different selections, and the
    //! sorting assertions would otherwise be vacuous.

    use crate::ledger::AccessLedger;
    use crate::prepared::{Canonical, CanonicalDeclarations, PreparedDataset};
    use crate::schema::LabeledExample;
    use crate::select::{FewShotSelector, Selection, SelectionConfig};
    use crate::split::SplitDeclaration;

    pub(crate) const LABEL_TEXTS: [&str; 3] = ["none", "against", "favor"];

    /// The ten contracted benchmark seeds.
    ///
    /// Cited verbatim from `crates/apr-cli/src/commands/data_tweeteval.rs:45`
    /// (`const BENCHMARK_SEEDS: [u64; 10]`). Note that **42 is not among them**.
    pub(crate) const CONTRACTED_SEEDS: [u64; 10] = [13, 17, 23, 29, 31, 37, 41, 43, 47, 53];

    pub(crate) fn label_names() -> Vec<String> {
        LABEL_TEXTS.iter().map(|name| (*name).to_string()).collect()
    }

    fn row(id: String, input: String, label: usize, split: &str) -> LabeledExample {
        LabeledExample {
            id,
            input,
            label,
            label_text: LABEL_TEXTS[label].to_string(),
            source_split: split.to_string(),
        }
    }

    fn train_row(label: usize, index: usize) -> LabeledExample {
        row(
            format!("train:{label}-{index:03}"),
            format!("training post for class {label} item {index}"),
            label,
            "train",
        )
    }

    /// `(train, validation, test)` rows for a corpus with `per_class` training rows in
    /// each of the three classes.
    pub(crate) fn rows(
        per_class: usize,
    ) -> (
        Vec<LabeledExample>,
        Vec<LabeledExample>,
        Vec<LabeledExample>,
    ) {
        let mut train = Vec::with_capacity(per_class * 3);
        for step in 0..per_class {
            let index = per_class - 1 - step;
            for label in 0..3 {
                train.push(train_row(label, index));
            }
        }
        let validation = (0..3)
            .map(|label| {
                row(
                    format!("validation:{label}"),
                    format!("validation post for class {label}"),
                    label,
                    "validation",
                )
            })
            .collect();
        let test = (0..3)
            .map(|label| {
                row(
                    format!("test:{label}"),
                    format!("held-out post for class {label}"),
                    label,
                    "test",
                )
            })
            .collect();
        (train, validation, test)
    }

    pub(crate) fn declarations(train_counts: Vec<usize>) -> CanonicalDeclarations {
        let decl = |counts: Vec<usize>| SplitDeclaration {
            expected_class_counts: counts,
            label_names: label_names(),
        };
        CanonicalDeclarations {
            train: decl(train_counts),
            validation: decl(vec![1, 1, 1]),
            test: decl(vec![1, 1, 1]),
            label_names: label_names(),
        }
    }

    pub(crate) fn build(
        train: Vec<LabeledExample>,
        validation: Vec<LabeledExample>,
        test: Vec<LabeledExample>,
        decls: &CanonicalDeclarations,
        ledger: &mut AccessLedger,
    ) -> PreparedDataset<Canonical> {
        PreparedDataset::<Canonical>::from_labeled_rows(train, validation, test, decls, ledger)
            .expect("the synthetic corpus must be valid")
    }

    /// The standard corpus: `per_class` rows per class, no duplicates, no exclusions.
    pub(crate) fn dataset(
        per_class: usize,
        ledger: &mut AccessLedger,
    ) -> PreparedDataset<Canonical> {
        let (train, validation, test) = rows(per_class);
        build(
            train,
            validation,
            test,
            &declarations(vec![per_class; 3]),
            ledger,
        )
    }

    /// The same corpus with the validation class-0 row duplicating a training class-0
    /// row's content, so exactly one training id is excluded.
    pub(crate) fn dataset_with_cross_split_duplicate(
        per_class: usize,
        ledger: &mut AccessLedger,
    ) -> PreparedDataset<Canonical> {
        let (train, mut validation, test) = rows(per_class);
        validation[0].input = train_row(0, 0).input;
        build(
            train,
            validation,
            test,
            &declarations(vec![per_class; 3]),
            ledger,
        )
    }

    /// The same corpus with class 2 declared but absent from the training split.
    pub(crate) fn dataset_with_empty_class(
        per_class: usize,
        ledger: &mut AccessLedger,
    ) -> PreparedDataset<Canonical> {
        let (train, validation, test) = rows(per_class);
        let train = train.into_iter().filter(|row| row.label != 2).collect();
        build(
            train,
            validation,
            test,
            &declarations(vec![per_class, per_class, 0]),
            ledger,
        )
    }

    pub(crate) fn select(
        dataset: &PreparedDataset<Canonical>,
        root_seed: u64,
        shots_per_class: u32,
        ledger: &mut AccessLedger,
    ) -> Selection {
        FewShotSelector::select(
            dataset,
            &SelectionConfig {
                root_seed,
                shots_per_class,
            },
            ledger,
        )
        .expect("the synthetic corpus must support this selection")
    }

    /// A fresh dataset AND a fresh ledger, so two selections are comparable: a ledger that
    /// already carried a previous selection's record would produce a different payload.
    pub(crate) fn fresh_selection(
        per_class: usize,
        root_seed: u64,
        shots_per_class: u32,
    ) -> (Selection, AccessLedger) {
        let mut ledger = AccessLedger::new();
        let prepared = dataset(per_class, &mut ledger);
        let selection = select(&prepared, root_seed, shots_per_class, &mut ledger);
        (selection, ledger)
    }
}

#[cfg(test)]
mod select_tests {
    use super::test_corpus::{self, CONTRACTED_SEEDS};
    use super::{FewShotSelector, SelectionConfig};
    use crate::error::ContrastiveDataError;
    use crate::ledger::AccessLedger;
    use crate::prepared::PreparedDataset;
    use proptest::prelude::{prop_assert_eq, proptest};

    fn ordered(selection: &super::Selection) -> Vec<(String, usize)> {
        selection
            .examples()
            .iter()
            .map(|row| (row.id.clone(), row.label))
            .collect()
    }

    #[test]
    fn select_returns_exactly_shots_per_class_with_labels_and_hashes() {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset(12, &mut ledger);
        let selection = test_corpus::select(&dataset, 13, 8, &mut ledger);

        assert_eq!(selection.len(), 24);
        assert!(!selection.is_empty());
        assert_eq!(selection.class_sizes(), [(0, 8), (1, 8), (2, 8)]);

        let ids = selection.ordered_ids();
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 24, "every selected id must be distinct");

        for row in selection.examples() {
            assert!(
                row.id.starts_with(&format!("train:{}-", row.label)),
                "row {row:?} must come from its own class's training pool"
            );
            assert_eq!(
                Some(&row.exact_hash),
                dataset.train().exact_hash_of(&row.id),
                "hashes are COPIED from the split, never re-derived"
            );
            assert_eq!(
                Some(&row.normalized_hash),
                dataset.train().normalized_hash_of(&row.id)
            );
        }
    }

    #[test]
    fn select_replays_identically_across_two_calls() {
        let (first, _) = test_corpus::fresh_selection(12, 13, 8);
        let (second, _) = test_corpus::fresh_selection(12, 13, 8);
        assert_eq!(ordered(&first), ordered(&second));
        assert_eq!(first.semantic_hash(), second.semantic_hash());
    }

    /// DATA-03's headline evidence, stated directly rather than only through the goldens:
    /// each of the TEN contracted seeds replays exactly.
    #[test]
    fn select_replays_identically_for_every_contracted_seed() {
        assert_eq!(CONTRACTED_SEEDS.len(), 10);
        assert!(
            !CONTRACTED_SEEDS.contains(&42),
            "42 is not a contracted seed — see data_tweeteval.rs:45"
        );
        let mut seen = Vec::new();
        for seed in CONTRACTED_SEEDS {
            let (first, _) = test_corpus::fresh_selection(12, seed, 8);
            let (second, _) = test_corpus::fresh_selection(12, seed, 8);
            assert_eq!(ordered(&first), ordered(&second), "seed {seed}");
            assert_eq!(first.semantic_hash(), second.semantic_hash(), "seed {seed}");
            seen.push(first.ordered_ids().join(","));
        }
        let mut distinct = seen.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            seen.len(),
            "different seeds must produce different orderings"
        );
    }

    #[test]
    fn select_supports_all_four_contracted_shot_counts() {
        for shots in [8_u32, 16, 32, 64] {
            let (selection, _) = test_corpus::fresh_selection(64, 13, shots);
            assert_eq!(selection.len() as u32, shots * 3, "shots {shots}");
            assert!(selection
                .class_sizes()
                .iter()
                .all(|(_, size)| *size == u64::from(shots)));
        }
    }

    /// The ordered SELECTION survives a permuted ingest order; the dataset fingerprint,
    /// and therefore the semantic hash, deliberately does NOT. Both halves are asserted so
    /// neither can regress unnoticed.
    #[test]
    fn select_is_invariant_under_permuted_ingest_order() {
        let (train, validation, test) = test_corpus::rows(12);
        let decls = test_corpus::declarations(vec![12; 3]);

        let mut ledger_a = AccessLedger::new();
        let straight = test_corpus::build(
            train.clone(),
            validation.clone(),
            test.clone(),
            &decls,
            &mut ledger_a,
        );
        let selection_a = test_corpus::select(&straight, 29, 8, &mut ledger_a);

        let mut reversed = train;
        reversed.reverse();
        let mut ledger_b = AccessLedger::new();
        let permuted = test_corpus::build(reversed, validation, test, &decls, &mut ledger_b);
        let selection_b = test_corpus::select(&permuted, 29, 8, &mut ledger_b);

        assert_eq!(
            ordered(&selection_a),
            ordered(&selection_b),
            "buckets sort before any draw, so ingest order cannot reach the selection"
        );
        assert_ne!(
            straight.fingerprint().hex(),
            permuted.fingerprint().hex(),
            "a permuted file IS different bytes; provenance must say so"
        );
        assert_ne!(selection_a.semantic_hash(), selection_b.semantic_hash());
    }

    #[test]
    fn select_rejects_an_invalid_shot_count_before_any_draw() {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset(12, &mut ledger);
        let before = ledger.records().len();

        for shots in [0_u32, 1, 7, 9, 63, 65, 128] {
            let err = FewShotSelector::select(
                &dataset,
                &SelectionConfig {
                    root_seed: 13,
                    shots_per_class: shots,
                },
                &mut ledger,
            )
            .expect_err("an uncontracted shot count must be refused");
            match err {
                ContrastiveDataError::InvalidShots { got, allowed } => {
                    assert_eq!(got, shots as usize);
                    assert_eq!(allowed, "{8, 16, 32, 64}");
                }
                other => panic!("expected InvalidShots, got {other:?}"),
            }
        }
        assert_eq!(
            ledger.records().len(),
            before,
            "a refused request must not touch the ledger"
        );
    }

    /// D-27's pool-exhaustion half.
    #[test]
    fn select_rejects_a_class_pool_smaller_than_shots() {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset_with_empty_class(8, &mut ledger);
        let err = FewShotSelector::select(
            &dataset,
            &SelectionConfig {
                root_seed: 13,
                shots_per_class: 8,
            },
            &mut ledger,
        )
        .expect_err("an exhausted class pool must be refused");
        match err {
            ContrastiveDataError::CrossSplitDuplicateUnderflow {
                class_label,
                pool,
                shots,
            } => {
                assert_eq!((class_label, pool, shots), (2, 0, 8));
                let message = ContrastiveDataError::CrossSplitDuplicateUnderflow {
                    class_label,
                    pool,
                    shots,
                }
                .to_string();
                assert!(message.contains("class 2"), "{message}");
                assert!(message.contains("0 rows remain"), "{message}");
                assert!(message.contains("8 shots"), "{message}");
            }
            other => panic!("expected CrossSplitDuplicateUnderflow, got {other:?}"),
        }
    }

    #[test]
    fn select_excludes_cross_split_duplicates_from_the_pool() {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset_with_cross_split_duplicate(12, &mut ledger);
        let excluded = dataset.exclusions().excluded_train_ids().to_vec();
        assert_eq!(
            excluded.len(),
            1,
            "the fixture must exclude exactly one row"
        );

        let selection = test_corpus::select(&dataset, 13, 8, &mut ledger);
        for id in &excluded {
            assert!(
                selection.selected_id(id).is_none(),
                "excluded id {id:?} must be unselectable"
            );
        }
    }

    #[test]
    fn select_selected_ids_round_trip_and_labels_agree() {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset(12, &mut ledger);
        let selection = test_corpus::select(&dataset, 37, 8, &mut ledger);

        for row in selection.examples() {
            let selected = selection
                .selected_id(&row.id)
                .expect("every selected row resolves to an ordinal");
            assert_eq!(selection.id_of(selected), row.id);
            assert_eq!(selection.label_of(selected), row.label);
            assert_eq!(selection.example_of(selected), row);
        }
        assert!(selection.selected_id("train:0-999").is_none());
        assert!(selection.selected_id("validation:0").is_none());
    }

    #[test]
    fn select_ids_in_class_concatenate_to_the_full_ordered_list() {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset(12, &mut ledger);
        let selection = test_corpus::select(&dataset, 41, 8, &mut ledger);

        let mut rebuilt = Vec::new();
        for (label, _) in selection.class_sizes() {
            let ordinals = selection.ids_in_class(*label);
            assert!(
                ordinals.windows(2).all(|pair| pair[0] < pair[1]),
                "class {label} ordinals must ascend"
            );
            for selected in ordinals {
                assert_eq!(selection.label_of(*selected), *label);
                rebuilt.push(selection.id_of(*selected).to_string());
            }
        }
        let expected: Vec<String> = selection
            .ordered_ids()
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_eq!(rebuilt, expected);
        assert!(selection.ids_in_class(99).is_empty());
    }

    /// D-19's evidence trail: a selection can only be produced from a dataset that HAS a
    /// validation witness, and the ledger records the profile it ran under.
    #[test]
    fn select_appends_one_access_record_naming_the_selection() {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset(12, &mut ledger);
        let ingest_records = ledger.records().len();
        let selection = test_corpus::select(&dataset, 43, 8, &mut ledger);

        assert_eq!(ledger.records().len(), ingest_records + 1);
        let record = ledger.records().last().expect("a record was just appended");
        assert!(record.purpose.contains("select"));
        assert_eq!(record.role, "train", "selection reads ONLY the train pool");
        assert_eq!(record.profile, "canonical");
        assert_eq!(record.fingerprint_hex, dataset.fingerprint().hex());
        assert_eq!(
            record.fingerprint_hex,
            dataset.validation_witness().dataset_fingerprint_hex(),
            "reachable only from a dataset that has a validation witness (D-19)"
        );
        assert_eq!(selection.dataset_fingerprint_hex(), record.fingerprint_hex);
    }

    #[test]
    fn select_ledger_hash_matches_the_live_ledger_immediately_after() {
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset(12, &mut ledger);
        let selection = test_corpus::select(&dataset, 47, 8, &mut ledger);
        assert_eq!(selection.ledger_hash(), ledger.ledger_hash());

        ledger.record("train", "canonical", "unrelated", "aa");
        assert_ne!(
            selection.ledger_hash(),
            ledger.ledger_hash(),
            "the retained hash describes the ledger AS OF selection, not the live one"
        );
    }

    /// Typed by construction rather than asserted: `select` takes exactly one dataset
    /// value, so there is no argument list into which a foreign validation split could be
    /// substituted.
    #[test]
    fn select_takes_exactly_one_dataset_value() {
        fn signature_check(
            dataset: &PreparedDataset<crate::prepared::Canonical>,
            cfg: &SelectionConfig,
            ledger: &mut AccessLedger,
        ) -> Result<super::Selection, ContrastiveDataError> {
            FewShotSelector::select(dataset, cfg, ledger)
        }
        let mut ledger = AccessLedger::new();
        let dataset = test_corpus::dataset(12, &mut ledger);
        let selection = signature_check(
            &dataset,
            &SelectionConfig {
                root_seed: 53,
                shots_per_class: 8,
            },
            &mut ledger,
        )
        .expect("selection succeeds");
        assert_eq!(selection.len(), 24);
    }

    proptest! {
        /// Determinism over the seed space, not only over the ten contracted seeds.
        #[test]
        fn select_is_deterministic_for_any_seed(seed in 0_u64..u64::MAX) {
            let (first, _) = test_corpus::fresh_selection(12, seed, 8);
            let (second, _) = test_corpus::fresh_selection(12, seed, 8);
            prop_assert_eq!(first.ordered_ids(), second.ordered_ids());
            prop_assert_eq!(first.semantic_hash(), second.semantic_hash());
        }
    }
}
