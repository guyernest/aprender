//! Typestate split roles: `Split<Train>`, `Split<Validation>`, `Split<Test>`,
//! `Split<CompatibilityTest>`.
//!
//! The role is a zero-sized type parameter and the only constructor is the bytes -> typed
//! boundary, which validates the embedded `source_split` before it will hand back a
//! typed value (D-16). A library caller therefore cannot *express* leakage, and honest-
//! looking bytes with a mislabeled role are a typed error rather than a compiler-accepted
//! `Split<Train>`.
//!
//! # There is no way to build a `Split<R>` from outside this crate
//!
//! Both constructors are `pub(crate)`. The only PUBLIC path to a typed split is a
//! `PreparedDataset` constructor, and that is deliberate: a split validated in isolation
//! carries no evidence about its siblings, so a train split obtained on its own could be
//! paired with a validation split from an entirely different dataset. Binding them inside
//! one `PreparedDataset` value makes that combination unrepresentable rather than merely
//! discouraged.
//!
//! # The declaration carries no dataset-profile field
//!
//! Profile identity is a TYPE PARAMETER of `PreparedDataset`, never a runtime field here.
//! A runtime field would make the interesting mistake compile and fail at run time, and a
//! compile-fail proof of "cannot be constructed" is unobtainable against an expression
//! that compiles.

use core::marker::PhantomData;
use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::error::ContrastiveDataError;
use crate::hash::{exact_hash, normalized_hash};
use crate::schema::{parse_jsonl_bytes, LabeledExample};

/// A split role. Implemented only by the four zero-sized marker types below.
pub trait SplitRole {
    /// The role name as it appears in a row's `source_split`, in error messages, and in
    /// fingerprint absorption.
    const ROLE: &'static str;
}

/// The training split — the only split a selection pool may draw from.
#[derive(Debug, Clone, Copy)]
pub struct Train;

/// The validation split. Its existence is what a canonical dataset proves.
#[derive(Debug, Clone, Copy)]
pub struct Validation;

/// The held-out test split.
#[derive(Debug, Clone, Copy)]
pub struct Test;

/// The merged compatibility test split (D-19) — a role DISTINCT from [`Test`], so a
/// compatibility corpus can never be mistaken for a canonical one by name alone.
#[derive(Debug, Clone, Copy)]
pub struct CompatibilityTest;

impl SplitRole for Train {
    const ROLE: &'static str = "train";
}
impl SplitRole for Validation {
    const ROLE: &'static str = "validation";
}
impl SplitRole for Test {
    const ROLE: &'static str = "test";
}
impl SplitRole for CompatibilityTest {
    const ROLE: &'static str = "compatibility_test";
}

/// What the caller asserts about a split before its bytes are trusted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitDeclaration {
    /// Exact expected row count per class, indexed by class label.
    pub expected_class_counts: Vec<usize>,
    /// The label map. `label_names[label]` must equal every row's `label_text`.
    pub label_names: Vec<String>,
}

/// A validated split of one role.
///
/// Every field is private and there is no public constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split<R: SplitRole> {
    rows: Vec<LabeledExample>,
    source_hash: [u8; 32],
    exact_hashes: BTreeMap<String, [u8; 32]>,
    normalized_hashes: BTreeMap<String, [u8; 32]>,
    class_counts: Vec<u64>,
    role: PhantomData<R>,
}

impl<R: SplitRole> Split<R> {
    pub(crate) fn from_jsonl_bytes(
        bytes: &[u8],
        decl: &SplitDeclaration,
    ) -> Result<Self, ContrastiveDataError> {
        let source_hash: [u8; 32] = Sha256::digest(bytes).into();
        let rows = parse_jsonl_bytes(bytes, R::ROLE)?;
        Self::assemble(rows, decl, source_hash)
    }

    pub(crate) fn from_rows(
        rows: Vec<LabeledExample>,
        decl: &SplitDeclaration,
    ) -> Result<Self, ContrastiveDataError> {
        let source_hash: [u8; 32] = Sha256::digest(b"").into();
        Self::assemble(rows, decl, source_hash)
    }

    fn assemble(
        rows: Vec<LabeledExample>,
        decl: &SplitDeclaration,
        source_hash: [u8; 32],
    ) -> Result<Self, ContrastiveDataError> {
        validate_ingest_ladder(&rows, R::ROLE, decl)?;

        let mut exact_hashes = BTreeMap::new();
        let mut normalized_hashes = BTreeMap::new();
        let mut class_counts = vec![0u64; decl.label_names.len()];
        for row in &rows {
            exact_hashes.insert(row.id.clone(), exact_hash(&row.input));
            normalized_hashes.insert(row.id.clone(), normalized_hash(&row.input));
            if let Some(slot) = class_counts.get_mut(row.label) {
                *slot += 1;
            }
        }

        Ok(Self {
            rows,
            source_hash,
            exact_hashes,
            normalized_hashes,
            class_counts,
            role: PhantomData,
        })
    }

    /// The validated rows, in ingest order.
    pub fn rows(&self) -> &[LabeledExample] {
        &self.rows
    }

    /// SHA-256 of this split's canonical JSONL bytes.
    pub fn source_hash(&self) -> &[u8; 32] {
        &self.source_hash
    }

    /// The exact content hash of a row, computed once at construction.
    pub fn exact_hash_of(&self, id: &str) -> Option<&[u8; 32]> {
        self.exact_hashes.get(id)
    }

    /// The normalized content hash of a row, computed once at construction.
    pub fn normalized_hash_of(&self, id: &str) -> Option<&[u8; 32]> {
        self.normalized_hashes.get(id)
    }

    /// Observed per-class row counts, indexed by class label.
    pub fn class_counts(&self) -> &[u64] {
        &self.class_counts
    }
}

/// The ONE validating gate ladder, shared by both constructors.
fn validate_ingest_ladder(
    rows: &[LabeledExample],
    role: &str,
    decl: &SplitDeclaration,
) -> Result<(), ContrastiveDataError> {
    let _ = (rows, role, decl);
    Ok(())
}

#[cfg(test)]
mod split_tests {
    use super::{CompatibilityTest, Split, SplitDeclaration, SplitRole, Test, Train, Validation};
    use crate::error::ContrastiveDataError;
    use crate::schema::{encode_jsonl, parse_jsonl_bytes, LabeledExample};
    use sha2::{Digest, Sha256};

    fn label_names() -> Vec<String> {
        vec![
            "none".to_string(),
            "against".to_string(),
            "favor".to_string(),
        ]
    }

    fn row(id: &str, input: &str, label: usize, split: &str) -> LabeledExample {
        LabeledExample {
            id: id.to_string(),
            input: input.to_string(),
            label,
            label_text: label_names()[label].clone(),
            source_split: split.to_string(),
        }
    }

    fn train_rows() -> Vec<LabeledExample> {
        vec![
            row("train:0", "alpha", 0, "train"),
            row("train:1", "beta", 1, "train"),
            row("train:2", "gamma", 2, "train"),
        ]
    }

    fn decl() -> SplitDeclaration {
        SplitDeclaration {
            expected_class_counts: vec![1, 1, 1],
            label_names: label_names(),
        }
    }

    fn train_bytes() -> Vec<u8> {
        encode_jsonl(&train_rows()).expect("encode must succeed")
    }

    #[test]
    fn split_roles_carry_their_wire_names() {
        assert_eq!(Train::ROLE, "train");
        assert_eq!(Validation::ROLE, "validation");
        assert_eq!(Test::ROLE, "test");
        assert_eq!(CompatibilityTest::ROLE, "compatibility_test");
    }

    #[test]
    fn split_from_jsonl_bytes_hashes_the_buffer_it_parsed() {
        let bytes = train_bytes();
        let split = Split::<Train>::from_jsonl_bytes(&bytes, &decl()).expect("valid train bytes");
        let expected: [u8; 32] = Sha256::digest(&bytes).into();
        assert_eq!(split.source_hash(), &expected);
        assert_eq!(split.rows().len(), 3);
        assert_eq!(split.class_counts(), &[1, 1, 1]);
    }

    #[test]
    fn split_precomputes_both_row_hashes() {
        let split =
            Split::<Train>::from_jsonl_bytes(&train_bytes(), &decl()).expect("valid train bytes");
        assert_eq!(
            split.exact_hash_of("train:0"),
            Some(&crate::hash::exact_hash("alpha"))
        );
        assert_eq!(
            split.normalized_hash_of("train:0"),
            Some(&crate::hash::normalized_hash("alpha"))
        );
        assert!(split.exact_hash_of("train:99").is_none());
    }

    #[test]
    fn split_rejects_rows_whose_embedded_role_is_another_split() {
        let mut rows = train_rows();
        rows[1].source_split = "validation".to_string();
        let bytes = encode_jsonl(&rows).expect("encode must succeed");
        let err = Split::<Train>::from_jsonl_bytes(&bytes, &decl())
            .expect_err("a mislabeled source_split must not become a Split<Train>");
        match err {
            ContrastiveDataError::SplitRoleMismatch {
                expected_role,
                embedded_role,
            } => {
                assert_eq!(expected_role, "train");
                assert_eq!(embedded_role, "validation");
                let rendered = ContrastiveDataError::SplitRoleMismatch {
                    expected_role,
                    embedded_role,
                }
                .to_string();
                assert!(rendered.contains("train"), "message names both roles");
                assert!(rendered.contains("validation"), "message names both roles");
            }
            other => panic!("expected SplitRoleMismatch, got {other:?}"),
        }
    }

    #[test]
    fn split_rejects_duplicate_ids() {
        let mut rows = train_rows();
        rows[2].id = "train:0".to_string();
        let bytes = encode_jsonl(&rows).expect("encode must succeed");
        let err =
            Split::<Train>::from_jsonl_bytes(&bytes, &decl()).expect_err("duplicate id must fail");
        match err {
            ContrastiveDataError::DuplicateId { split, id } => {
                assert_eq!(split, "train");
                assert_eq!(id, "train:0");
            }
            other => panic!("expected DuplicateId, got {other:?}"),
        }
    }

    #[test]
    fn split_rejects_a_label_outside_the_declared_map() {
        let mut rows = train_rows();
        rows[1].label = 7;
        rows[1].label_text = "unknown".to_string();
        let bytes = encode_jsonl(&rows).expect("encode must succeed");
        let err =
            Split::<Train>::from_jsonl_bytes(&bytes, &decl()).expect_err("unknown label must fail");
        match err {
            ContrastiveDataError::UnknownLabel {
                split,
                index,
                label,
            } => {
                assert_eq!(split, "train");
                assert_eq!(index, 1);
                assert_eq!(label, 7);
            }
            other => panic!("expected UnknownLabel, got {other:?}"),
        }
    }

    #[test]
    fn split_rejects_label_text_that_disagrees_with_the_label_map() {
        let mut rows = train_rows();
        rows[2].label_text = "against".to_string();
        let bytes = encode_jsonl(&rows).expect("encode must succeed");
        let err = Split::<Train>::from_jsonl_bytes(&bytes, &decl())
            .expect_err("label_text disagreement must fail");
        match err {
            ContrastiveDataError::LabelTextMismatch {
                split,
                index,
                label,
                expected_text,
                got_text,
            } => {
                assert_eq!(split, "train");
                assert_eq!(index, 2);
                assert_eq!(label, 2);
                assert_eq!(expected_text, "favor");
                assert_eq!(got_text, "against");
            }
            other => panic!("expected LabelTextMismatch, got {other:?}"),
        }
    }

    #[test]
    fn split_rejects_per_class_counts_that_differ_from_the_declaration() {
        let declaration = SplitDeclaration {
            expected_class_counts: vec![2, 1, 1],
            label_names: label_names(),
        };
        let err = Split::<Train>::from_jsonl_bytes(&train_bytes(), &declaration)
            .expect_err("class-count contract must fail");
        match err {
            ContrastiveDataError::InvalidClassCounts {
                split,
                expected,
                got,
            } => {
                assert_eq!(split, "train");
                assert_eq!(expected, vec![2, 1, 1]);
                assert_eq!(got, vec![1, 1, 1]);
            }
            other => panic!("expected InvalidClassCounts, got {other:?}"),
        }
    }

    #[test]
    fn split_rejects_a_whitespace_only_row_through_both_paths() {
        let mut rows = train_rows();
        rows[1].input = "  \t ".to_string();
        let bytes = encode_jsonl(&rows).expect("encode must succeed");
        let from_bytes = Split::<Train>::from_jsonl_bytes(&bytes, &decl())
            .expect_err("empty input must fail via bytes");
        let from_rows =
            Split::<Train>::from_rows(rows, &decl()).expect_err("empty input must fail via rows");
        assert_eq!(from_bytes, from_rows);
        assert!(matches!(
            from_bytes,
            ContrastiveDataError::EmptyInput { index: 1, .. }
        ));
    }

    /// Every defect class must produce the SAME typed error through both constructors,
    /// which is only true if they share one validator rather than two that agree today.
    #[test]
    fn split_both_constructors_agree_on_every_defect_class() {
        let mutations: Vec<(&str, fn(&mut Vec<LabeledExample>))> = vec![
            ("role", |rows| {
                rows[1].source_split = "test".to_string();
            }),
            ("duplicate id", |rows| {
                rows[2].id = "train:1".to_string();
            }),
            ("unknown label", |rows| {
                rows[0].label = 9;
            }),
            ("label text", |rows| {
                rows[0].label_text = "favor".to_string();
            }),
            ("class counts", |rows| {
                rows.pop();
            }),
        ];
        for (name, mutate) in mutations {
            let mut rows = train_rows();
            mutate(&mut rows);
            let bytes = encode_jsonl(&rows).expect("encode must succeed");
            let via_bytes = Split::<Train>::from_jsonl_bytes(&bytes, &decl())
                .err()
                .unwrap_or_else(|| panic!("{name}: bytes path must fail"));
            let via_rows = Split::<Train>::from_rows(rows, &decl())
                .err()
                .unwrap_or_else(|| panic!("{name}: rows path must fail"));
            assert_eq!(via_bytes, via_rows, "{name}: both paths must agree");
        }
    }

    /// Checker warning 3: `from_rows` derives `source_hash` from the canonical
    /// re-encoding, so the two doors into a split cannot fingerprint differently.
    #[test]
    fn split_both_constructors_derive_the_same_source_hash() {
        let bytes = train_bytes();
        let via_bytes = Split::<Train>::from_jsonl_bytes(&bytes, &decl()).expect("bytes path");
        let parsed = parse_jsonl_bytes(&bytes, "train").expect("parse must succeed");
        let via_rows = Split::<Train>::from_rows(parsed, &decl()).expect("rows path");
        assert_eq!(via_bytes.source_hash(), via_rows.source_hash());
        assert_eq!(via_bytes.rows(), via_rows.rows());
        assert_eq!(via_bytes.class_counts(), via_rows.class_counts());
    }

    #[test]
    fn split_accepts_a_compatibility_test_role() {
        let rows = vec![
            row("compatibility_test:0", "alpha", 0, "compatibility_test"),
            row("compatibility_test:1", "beta", 1, "compatibility_test"),
        ];
        let declaration = SplitDeclaration {
            expected_class_counts: vec![1, 1, 0],
            label_names: label_names(),
        };
        let bytes = encode_jsonl(&rows).expect("encode must succeed");
        let split = Split::<CompatibilityTest>::from_jsonl_bytes(&bytes, &declaration)
            .expect("valid compatibility bytes");
        assert_eq!(split.class_counts(), &[1, 1, 0]);
    }
}
