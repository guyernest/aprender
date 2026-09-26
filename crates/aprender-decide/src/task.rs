//! The `task.json` document (D-05): `{type: "choice", instructions, criteria}`.
//!
//! **The document order of `criteria` is the label index.** That is the one rule this
//! module exists to keep, and it is where it is most easily lost: `serde_json::Map` is
//! a key-sorted `BTreeMap` by default and an insertion-ordered `IndexMap` only under
//! the `preserve_order` feature, which `pmcp` turns on for every build that links it.
//! A library tested alone and the same library linked into a server would then read
//! the same bytes into different label orders (RESEARCH Pitfall 2).
//!
//! So criteria are NEVER read through `serde_json::Value` / `Map`. They are read
//! straight from the raw bytes by a hand-written [`serde::de::Visitor::visit_map`]
//! that pushes `(name, description)` pairs in document order — the same order under
//! both map backings (`tests::serde_backing_canary` records which backing a run had).
//!
//! Refusals ([`TaskError`]): a `type` other than `"choice"`, fewer than 2 criteria, a
//! duplicate criterion name, an empty name, a non-string description, and any unknown
//! top-level key (`deny_unknown_fields`). Nothing is defaulted.
//!
//! Contract: `contracts/decide-apr-v1.yaml` `task_json_schema` and
//! `equations.task_order_is_label_index`.

use serde::de::{Deserializer, MapAccess, Visitor};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt;

/// One criterion: its name is the label; its description, when present, is rendered
/// into the option text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    /// The label (a criterion name, unique within the task).
    pub name: String,
    /// Optional description; `null` and `""` both mean "name only".
    pub description: Option<String>,
}

/// A parsed, validated `task.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    instructions: String,
    criteria: Vec<Criterion>,
    sha256: String,
}

/// Why a `task.json` was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    /// Not valid JSON for the schema: malformed, an unknown top-level key, a missing
    /// field, or a description that is neither a string nor `null`.
    Parse(String),
    /// `type` is not `"choice"` (the only task type a decision artifact serves).
    UnsupportedType(String),
    /// Fewer than 2 criteria.
    TooFewCriteria(usize),
    /// The same criterion name appears twice; the second occurrence would silently
    /// shadow or shift a label.
    DuplicateCriterion(String),
    /// A criterion name is empty.
    EmptyCriterionName {
        /// Its position in document order.
        index: usize,
    },
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "task.json: {e}"),
            Self::UnsupportedType(t) => {
                write!(
                    f,
                    "task.json: type {t:?} is not supported (only \"choice\")"
                )
            }
            Self::TooFewCriteria(n) => {
                write!(f, "task.json: {n} criteria, at least 2 are required")
            }
            Self::DuplicateCriterion(name) => {
                write!(f, "task.json: criterion {name:?} appears more than once")
            }
            Self::EmptyCriterionName { index } => {
                write!(f, "task.json: criterion {index} has an empty name")
            }
        }
    }
}

impl std::error::Error for TaskError {}

/// Criteria in DOCUMENT order, with the first repeated name (if any) recorded so the
/// caller can refuse it as a typed [`TaskError::DuplicateCriterion`].
struct OrderedCriteria {
    pairs: Vec<(String, Option<String>)>,
    first_duplicate: Option<String>,
}

impl<'de> Deserialize<'de> for OrderedCriteria {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedVisitor;

        impl<'de> Visitor<'de> for OrderedVisitor {
            type Value = OrderedCriteria;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a criteria object mapping names to descriptions (string or null)")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut pairs: Vec<(String, Option<String>)> = Vec::new();
                let mut first_duplicate = None;
                while let Some((name, description)) = map.next_entry::<String, Option<String>>()? {
                    if first_duplicate.is_none() && pairs.iter().any(|(n, _)| *n == name) {
                        first_duplicate = Some(name.clone());
                    }
                    pairs.push((name, description.filter(|d| !d.is_empty())));
                }
                Ok(OrderedCriteria {
                    pairs,
                    first_duplicate,
                })
            }
        }

        deserializer.deserialize_map(OrderedVisitor)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskDoc {
    #[serde(rename = "type")]
    kind: String,
    instructions: String,
    criteria: OrderedCriteria,
}

impl Task {
    /// Parse and validate `task.json` from its raw bytes.
    ///
    /// # Errors
    ///
    /// A [`TaskError`] for every refusal listed in the module docs.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, TaskError> {
        let doc: TaskDoc =
            serde_json::from_slice(bytes).map_err(|e| TaskError::Parse(e.to_string()))?;
        if doc.kind != "choice" {
            return Err(TaskError::UnsupportedType(doc.kind));
        }
        if let Some(name) = doc.criteria.first_duplicate {
            return Err(TaskError::DuplicateCriterion(name));
        }
        if let Some(index) = doc.criteria.pairs.iter().position(|(n, _)| n.is_empty()) {
            return Err(TaskError::EmptyCriterionName { index });
        }
        if doc.criteria.pairs.len() < 2 {
            return Err(TaskError::TooFewCriteria(doc.criteria.pairs.len()));
        }
        let criteria = doc
            .criteria
            .pairs
            .into_iter()
            .map(|(name, description)| Criterion { name, description })
            .collect();
        let sha256 = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Self {
            instructions: doc.instructions,
            criteria,
            sha256,
        })
    }

    /// The task instructions (Laya's question text).
    #[must_use]
    pub fn instructions(&self) -> &str {
        &self.instructions
    }

    /// The criteria in label-index order.
    #[must_use]
    pub fn criteria(&self) -> &[Criterion] {
        &self.criteria
    }

    /// The labels (criterion names) in label-index order.
    #[must_use]
    pub fn labels(&self) -> Vec<&str> {
        self.criteria.iter().map(|c| c.name.as_str()).collect()
    }

    /// The option texts in label-index order: `"name: description"`, or `"name"`
    /// when there is no description (Laya `render_options` for `choice`).
    #[must_use]
    pub fn render_options(&self) -> Vec<String> {
        self.criteria
            .iter()
            .map(|c| {
                c.description
                    .as_ref()
                    .map_or_else(|| c.name.clone(), |d| format!("{}: {d}", c.name))
            })
            .collect()
    }

    /// Lower-case hex sha256 of the exact bytes this task was parsed from.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}
