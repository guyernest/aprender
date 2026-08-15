//! The fresh-process validation evaluator — the candidate a `.apr` file can be.
//!
//! Contract: `setfit-train-lifecycle-v1`, equation `validation_evaluation_provenance`.
//! Requirement: TRN-07.
//!
//! # The gap this closes, stated exactly
//!
//! 04-16 landed [`reload_verified_run_from_apr`](super::apr_reload::reload_verified_run_from_apr)
//! and recorded, in its own summary, the edge it deliberately left open:
//!
//! > A `SelectionCandidate` reads its artifact hash out of a `ValidationEvaluation`, and
//! > `evaluate_validation` takes a train-time `SetFitRun<ArtifactReloadedAndVerified>`. So a
//! > process that only has `.apr` files cannot yet build the candidate set it locks over — it
//! > can only consume a lock somebody else wrote. […] that is a second evaluation policy
//! > question and belongs to whichever plan owns `apr eval`'s candidate story.
//!
//! This module is that answer. Without it `apr eval --split validation --lock-out` cannot
//! exist at all: `ValidationEvaluation` has no public constructor by design, and its only
//! producer takes a type a fresh process cannot mint.
//!
//! # It is NOT a second evaluation policy, and here is the precise sense in which not
//!
//! An evaluation is two things: a PREDICTION over the canonical validation rows, and a
//! REDUCTION of those predictions to a number.
//!
//! * The reduction is literally the same code, and it is reached through the same door. Both
//!   evaluators hand their two index vectors to `evaluate::evaluation_from_predictions`, which
//!   owns the bounds check, the metric dispatch, macro-F1's empty-class convention, the
//!   index-order accumulation and the construction of the evidence record. That function was
//!   EXTRACTED from `evaluate_validation`'s tail by this plan rather than written beside it,
//!   so there is one implementation and both callers are it.
//!   `apr_evaluate_reduction_is_the_trainers_own` asserts this module computes no metric and
//!   constructs no `ValidationEvaluation`.
//! * The prediction goes through `VerifiedSetFitModel::classify` — `aprender-core`'s ONE
//!   classification path, the one `apr predict` and `POST /v1/classify` use. The trainer's
//!   `evaluate_validation` instead reaches `head_input::encode_eval_rows` plus the run's own
//!   head, because a train-time run HAS those objects and never serialized them.
//!
//! Choosing core's path here is not a convenience. A fresh process's whole claim is about the
//! artifact, and the artifact's conformance evidence — the six-probe replay every rung-8 load
//! performs — is evidence about core's encode path specifically. Reaching around it to a
//! second encode would measure something the artifact carries no probes for.
//!
//! # One caveat, recorded rather than hidden
//!
//! Two evaluations of the SAME model, one taken through the trainer's path and one through
//! this one, are not guaranteed bit-identical: they are two float pipelines. Nothing shipped
//! mixes them — a lock's candidate set is built by ONE caller and `apr eval` uses this door
//! for every candidate — but the possibility is real, so it is written here rather than left
//! for someone to discover. `ValidationEvaluation`'s wire form cannot record which path
//! produced it without changing the selection lock's canonical bytes, which would invalidate
//! every lock in existence; that trade was not worth making for a mixture no code performs.
//!
//! # What this module does NOT do
//!
//! It mints no lifecycle state, fabricates no evidence and constructs no `SetFitRun`. It takes
//! a credential that already passed the full ladder and the three-identifier provenance gate,
//! re-checks the two dataset fingerprints against the artifact's own record so the function is
//! total on its own arguments, and returns a measurement.

use aprender::setfit::{ClassifyRequestDocument, MAX_BATCH_TEXTS};
use aprender_contrastive_data::prepared::{Canonical, PreparedDataset};

use super::apr_reload::ReloadedSetFitCredential;
use super::evaluate::{ValidationEvaluation, ValidationMetricKind};
use super::SetFitTrainError;

/// A fresh-process evaluation failure.
///
/// A module-owned vocabulary, on the precedent [`super::apr_reload::AprReloadError`] set: the
/// module that owns an operation owns its failure names, and the trainer error wraps it whole
/// so a caller can tell a label-map disagreement from a corpus disagreement without matching
/// on text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AprEvaluateError {
    /// The dataset supplied is not the corpus the artifact records.
    DatasetFingerprintMismatch {
        /// The fingerprint the artifact's provenance records.
        recorded: String,
        /// The fingerprint the supplied dataset computes.
        supplied: String,
    },
    /// The validation SPLIT differs, even though the corpus matches.
    ValidationSplitFingerprintMismatch {
        /// The fingerprint the artifact's provenance records.
        recorded: String,
        /// The fingerprint the supplied dataset's validation witness computes.
        supplied: String,
    },
    /// The artifact's provenance is not readable as the two fingerprints.
    ProvenanceUnreadable {
        /// Which field could not be read.
        field: &'static str,
    },
    /// The artifact's ordered labels are not the dataset's declared label map.
    LabelMapMismatch {
        /// The labels the artifact's head indexes by, in row order.
        artifact: Vec<String>,
        /// The labels the dataset declares, in index order.
        dataset: Vec<String>,
    },
    /// The validation split has no rows.
    ValidationSplitEmpty,
    /// Core's one classification path refused a batch.
    ClassifyFailed {
        /// The typed error's rendering.
        reason: String,
    },
    /// A predicted label is not in the artifact's own ordered set.
    ///
    /// Unreachable while `classify` reports the head's own labels, and kept because the
    /// alternative to a typed refusal is an `expect` in the one place a silent mis-mapping
    /// would turn every metric below into a confidently wrong number.
    UnknownPredictedLabel {
        /// The label the classifier returned.
        label: String,
    },
}

impl core::fmt::Display for AprEvaluateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DatasetFingerprintMismatch { recorded, supplied } => write!(
                f,
                "this artifact was trained on the dataset fingerprinted `{recorded}`, and the \
                 dataset supplied fingerprints to `{supplied}`. A validation metric measured on \
                 a different corpus is not evidence about this artifact, and a selection lock \
                 taken over it would record a decision nobody made",
            ),
            Self::ValidationSplitFingerprintMismatch { recorded, supplied } => write!(
                f,
                "the artifact records the validation split `{recorded}` and the dataset supplied \
                 carries `{supplied}`. The corpus matches, so this is the SAME dataset whose \
                 validation rows changed — which the two fingerprints exist to distinguish",
            ),
            Self::ProvenanceUnreadable { field } => write!(
                f,
                "the artifact's provenance record does not carry a readable `{field}`, so there \
                 is nothing to check the supplied dataset against; the evaluation is refused \
                 rather than performed against an unverified corpus",
            ),
            Self::LabelMapMismatch { artifact, dataset } => write!(
                f,
                "the artifact's head indexes by {artifact:?} and the dataset declares \
                 {dataset:?}. Index i of one is not index i of the other, so every prediction \
                 would be compared against a different class's truth and the metric would be a \
                 confidently wrong number rather than an error",
            ),
            Self::ValidationSplitEmpty => {
                write!(f, "the canonical validation split has no rows to measure")
            }
            Self::ClassifyFailed { reason } => {
                write!(f, "the verified model refused a validation batch: {reason}")
            }
            Self::UnknownPredictedLabel { label } => write!(
                f,
                "the classifier returned the label `{label}`, which is not in the artifact's own \
                 ordered label set",
            ),
        }
    }
}

impl std::error::Error for AprEvaluateError {}

impl From<AprEvaluateError> for SetFitTrainError {
    fn from(inner: AprEvaluateError) -> Self {
        Self::AprEvaluate(inner)
    }
}

/// Measure `metric` on the canonical validation split, with a RELOADED artifact.
///
/// The counterpart of [`super::evaluate::evaluate_validation`] for a process that trained
/// nothing. Same metric implementations, same committed facts, same refusal to accept a
/// caller-supplied number; a different prediction path, for the reason the module header gives.
///
/// # Errors
///
/// [`AprEvaluateError`], wrapped as [`SetFitTrainError::AprEvaluate`]: both fingerprint
/// disagreements naming the two values, an unreadable provenance record naming the field, a
/// label-map disagreement naming both maps, an empty split, and anything core's classify path
/// reports.
pub fn evaluate_validation_from_artifact(
    credential: &ReloadedSetFitCredential,
    dataset: &PreparedDataset<Canonical>,
    metric: ValidationMetricKind,
) -> Result<ValidationEvaluation, SetFitTrainError> {
    let model = credential.model();

    // (1) THE CORPUS, from the ARTIFACT'S OWN RECORD. The reload door checked this against
    //     the dataset it was handed; checking it again here is not redundancy — this function
    //     takes its own `dataset` argument, and a version that trusted the caller to pass the
    //     same one would be a door whose guarantee depends on a convention.
    let provenance = model.doc_view().provenance.clone();
    let recorded_dataset = read_provenance_hex(&provenance, "dataset_fingerprint")?;
    let recorded_validation = read_provenance_hex(&provenance, "validation_split_fingerprint")?;

    let witness = dataset.validation_witness();
    let dataset_fingerprint = witness.dataset_fingerprint_hex();
    let validation_split_fingerprint = witness.fingerprint_hex();

    if recorded_dataset != dataset_fingerprint {
        return Err(AprEvaluateError::DatasetFingerprintMismatch {
            recorded: recorded_dataset,
            supplied: dataset_fingerprint,
        }
        .into());
    }
    if recorded_validation != validation_split_fingerprint {
        return Err(AprEvaluateError::ValidationSplitFingerprintMismatch {
            recorded: recorded_validation,
            supplied: validation_split_fingerprint,
        }
        .into());
    }

    // (2) THE LABEL MAP. Read off the REBUILT HEAD, which is the list a classification will
    //     actually index into — not the document's copy of it, which could have drifted.
    let artifact_labels: Vec<String> = model.ordered_labels().to_vec();
    let dataset_labels: Vec<String> = dataset.label_names().to_vec();
    if artifact_labels != dataset_labels {
        return Err(AprEvaluateError::LabelMapMismatch {
            artifact: artifact_labels,
            dataset: dataset_labels,
        }
        .into());
    }
    let classes = artifact_labels.len();

    // (3) THE ROWS. `dataset.validation()` is a `&Split<Validation>` BY TYPE; the
    //     compatibility profile has no such method (Ph2 D-19), so a compatibility-selected
    //     evaluation is non-constructible at this call site rather than merely rejected.
    let split = dataset.validation();
    let rows = split.rows();
    if rows.is_empty() {
        return Err(AprEvaluateError::ValidationSplitEmpty.into());
    }

    // (4) PREDICT, through core's ONE classify path, in batches core's own bound allows.
    //     Chunked rather than one call: `MAX_BATCH_TEXTS` is enforced INSIDE classify, so a
    //     validation split larger than it would be refused rather than measured, and silently
    //     dropping the tail would be worse than either.
    let mut predicted: Vec<usize> = Vec::with_capacity(rows.len());
    for chunk in rows.chunks(MAX_BATCH_TEXTS) {
        let request = ClassifyRequestDocument::new(chunk.iter().map(|row| row.input.clone()));
        let response = model.classify(&request).map_err(|error| {
            SetFitTrainError::from(AprEvaluateError::ClassifyFailed { reason: error.to_string() })
        })?;
        for result in response.results() {
            let index = artifact_labels
                .iter()
                .position(|label| label == result.label())
                .ok_or_else(|| {
                    SetFitTrainError::from(AprEvaluateError::UnknownPredictedLabel {
                        label: result.label().to_string(),
                    })
                })?;
            predicted.push(index);
        }
    }
    // The response is in request order and one row per text (core's envelope constructor
    // enforces a uniform arity), but a length disagreement here would silently mis-pair every
    // prediction with a truth, so it is checked rather than assumed.
    if predicted.len() != rows.len() {
        return Err(AprEvaluateError::ClassifyFailed {
            reason: format!(
                "the classifier returned {} results for {} validation rows",
                predicted.len(),
                rows.len()
            ),
        }
        .into());
    }

    // (5) THE SHARED TAIL. The bounds check, the metric dispatch and the construction of the
    //     evidence record are the trainer's own `evaluation_from_predictions` — the same
    //     function `evaluate_validation` calls. This module computes no metric and constructs
    //     no `ValidationEvaluation`; it hands over PREDICTIONS, never a number.
    let truth: Vec<usize> = rows.iter().map(|row| row.label).collect();
    super::evaluate::evaluation_from_predictions(
        metric,
        &truth,
        &predicted,
        classes,
        // READ OFF the credential, which read it off the loader's digest of the bytes. There
        // is no parameter here a caller could have supplied.
        credential.artifact_hash().to_string(),
        validation_split_fingerprint,
        dataset_fingerprint,
    )
}

/// Read one lowercase-hex provenance field, or refuse by name.
fn read_provenance_hex(
    provenance: &serde_json::Value,
    field: &'static str,
) -> Result<String, SetFitTrainError> {
    provenance
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AprEvaluateError::ProvenanceUnreadable { field }.into())
}

#[cfg(test)]
#[path = "apr_evaluate_tests.rs"]
mod apr_evaluate_tests;
