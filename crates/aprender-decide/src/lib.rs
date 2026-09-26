//! Method-neutral decision models over aprender-core (D-14).
//!
//! A *decision method* answers one fixed [`Task`] — "which of these K criteria does
//! this text belong to?" — with a calibrated probability per criterion, in the
//! task's criteria order. The order of `task.json`'s `criteria` object IS the label
//! index (D-05), so every probability vector here is an array in that order, never a
//! map.
//!
//! # The seam
//!
//! [`DecisionMethod`] is designed for exactly ONE implementation today —
//! [`laya::Laya`], a ModernBERT encoder with a typed decision head — with Kev (a
//! few-shot decoder classifier) as the known second. It carries only what every
//! method needs: the task, tokenization/row building ([`DecisionMethod::prepare`]),
//! and scoring of already-built rows ([`DecisionMethod::classify_prepared`]). It has
//! no Kev-speculative methods; Kev adds an implementation, not a new trait shape.
//!
//! `prepare` is separate from scoring so a server can price a request (its token
//! budget) from the rows it is about to score, tokenizing each text exactly once.
//!
//! # What this crate is not
//!
//! Serving is transport-only and lives elsewhere (the thin decide MCP servers call
//! this crate; they re-implement nothing — OPS-03). The encoder is not here either:
//! it is aprender-core's reusable `aprender::models::modernbert` (D-13), and Laya's
//! head and scorer reuse its `Linear`, `layer_norm`, `gelu_exact` and `attention`.
//!
//! Contracts: `contracts/laya-parity-v1.yaml` (the torch -> .apr -> Rust parity
//! ladder) and `contracts/decide-apr-v1.yaml` (the task schema and marker rule).

pub mod laya;
pub mod task;

#[cfg(test)]
pub(crate) mod test_support;

use std::fmt;

pub use laya::LayaError;
pub use task::{Criterion, Task, TaskError};

use aprender::models::modernbert::ModernBertError;

/// One scored text.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    /// Index of the most probable criterion, in task criteria order (first on ties).
    pub label_index: usize,
    /// One calibrated probability per criterion, in task criteria order.
    pub probabilities: Vec<f32>,
    /// Tokens in the row the method actually scored (after any truncation).
    pub tokens: usize,
    /// True when the caller's text was cut to fit the method's window (D-12).
    pub truncated: bool,
}

/// A text already tokenized and built into the row a method will score.
///
/// Only a method's own [`DecisionMethod::prepare`] constructs one, so a row can never
/// carry read positions the builder did not produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRow {
    pub(crate) ids: Vec<u32>,
    pub(crate) markers: Vec<usize>,
    pub(crate) truncated: bool,
}

impl PreparedRow {
    /// The token ids of the built row.
    #[must_use]
    pub fn ids(&self) -> &[u32] {
        &self.ids
    }

    /// Method-specific read positions (Laya: the `[MASK]` option markers).
    #[must_use]
    pub fn markers(&self) -> &[usize] {
        &self.markers
    }

    /// Row length in tokens — what a server's token budget is priced on.
    #[must_use]
    pub fn tokens(&self) -> usize {
        self.ids.len()
    }

    /// True when the caller's text was cut to fit the window (D-12).
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

/// Why a decision method refused.
#[derive(Debug, Clone, PartialEq)]
pub enum DecideError {
    /// The task document was refused (D-05).
    Task(TaskError),
    /// The Laya method refused its inputs or its parts.
    Laya(LayaError),
    /// The ModernBERT encoder or one of its primitives refused at runtime.
    Encoder(ModernBertError),
}

impl fmt::Display for DecideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Task(e) => write!(f, "decide: {e}"),
            Self::Laya(e) => write!(f, "decide: {e}"),
            Self::Encoder(e) => write!(f, "decide: {e}"),
        }
    }
}

impl std::error::Error for DecideError {}

impl From<TaskError> for DecideError {
    fn from(e: TaskError) -> Self {
        Self::Task(e)
    }
}

impl From<LayaError> for DecideError {
    fn from(e: LayaError) -> Self {
        Self::Laya(e)
    }
}

impl From<ModernBertError> for DecideError {
    fn from(e: ModernBertError) -> Self {
        Self::Encoder(e)
    }
}

/// A decision method bound to one task.
///
/// Designed for one implementation ([`laya::Laya`]) with Kev as the known second;
/// see the crate docs for why it has no other methods.
pub trait DecisionMethod: Send + Sync {
    /// The task every decision answers (ordered labels).
    fn task(&self) -> &Task;

    /// Tokenize and build the row for every text, in order.
    ///
    /// # Errors
    ///
    /// A typed refusal when a text cannot be tokenized or its row cannot be built.
    fn prepare(&self, texts: &[String]) -> Result<Vec<PreparedRow>, DecideError>;

    /// Score rows produced by [`DecisionMethod::prepare`], in order.
    ///
    /// # Errors
    ///
    /// A typed refusal from the method's forward pass.
    fn classify_prepared(&self, rows: &[PreparedRow]) -> Result<Vec<Decision>, DecideError>;

    /// [`DecisionMethod::prepare`] then [`DecisionMethod::classify_prepared`].
    ///
    /// # Errors
    ///
    /// Either step's refusal.
    fn classify(&self, texts: &[String]) -> Result<Vec<Decision>, DecideError> {
        let rows = self.prepare(texts)?;
        self.classify_prepared(&rows)
    }
}
