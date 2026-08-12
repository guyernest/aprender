//! The canonical SetFit-identity evidence table and its binding hash (D-09, D-10, D-12).
//!
//! Contract: `setfit-train-lifecycle-v1` (authored in plan 03-06). Requirement: TRN-03.
//!
//! # The relative-delta formula, and why it has a floored, support-restricted denominator
//!
//! ```text
//! relative_delta(p) = ||dTheta_p||_2 / max(denom(p), s_class(p))
//!
//! denom(p) = ||theta_init_p restricted to the SUPPORT of dTheta_p||_2   for the SPARSE class
//!          = ||theta_init_p||_2                                          otherwise
//! ```
//!
//! Two properties follow, and both are load-bearing.
//!
//! **It is FINITE at zero initialization.** The naive `||dTheta|| / ||theta_init||` is `0/0`
//! or `x/0` for a bias initialized to exactly zero. `NaN > eps` is FALSE, so an un-floored
//! form would have silently REJECTED every run containing a zero-init parameter — a gate that
//! fails closed for the wrong reason is worse than one that does not exist, because the
//! diagnosis points at the model instead of at the metric. The positive contracted floor
//! `s_class` removes the case entirely: when the initial norm is below unit scale the ratio
//! degrades gracefully into an ABSOLUTE movement measure, which is the right question to ask
//! about a parameter that started at zero.
//!
//! **For the sparse class it is INVARIANT to vocabulary size.** A whole-table denominator
//! would make the embedding class's ratio a function of how many rows the table has: a
//! few-shot batch touches a handful of rows regardless, so `||dTheta||` is fixed while
//! `||theta_init||` grows as `sqrt(V)`, and a ratio calibrated on a 97-row fixture would be
//! roughly `sqrt(30522/97) ~ 17.7` times too large for the production encoder. Restricting
//! the denominator to the rows the delta actually touched cancels the vocabulary factor,
//! which is what makes a fixture-calibrated epsilon transferable at all. Every row records
//! its observed `delta_support_fraction` so the claim stays checkable rather than asserted:
//! see `evidence_sparse_denominator_is_restricted_to_the_support`.
//!
//! A parameter that did not move AT ALL is never SetFit, whatever its ratio says, so the
//! strict predicate `||dTheta||_2 > 0` is recorded ALONGSIDE the ratio rather than folded
//! into it.
//!
//! # The summary's hash BINDS it to the table
//!
//! [`EvidenceSummary::table_hash`] is the SHA-256 of the full canonical table bytes. Editing
//! a row and leaving the summary alone produces a summary whose hash does not match its
//! table, and `evidence_hash_binds_the_summary_to_the_table` proves the detection. That is
//! tamper-EVIDENCE and linkage. It is not a signature and it does not make the pair
//! unforgeable: anyone who can edit the table can recompute the hash. The distinction is
//! recorded because the earlier wording of D-12 overclaimed it.
//!
//! # No wall-clock field, anywhere
//!
//! Every field of every serialized struct here is a hash, a count or a measured norm. A
//! timing field would make two identical runs serialize differently and would take TRN-06's
//! bitwise claim with it.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::tune::{ParamRecord, TuneOutput};

/// The schema version of the evidence wire form.
pub(crate) const EVIDENCE_SCHEMA_VERSION: u32 = 1;

/// The contract this evidence discharges.
pub(crate) const EVIDENCE_CONTRACT_VERSION: &str = "setfit-train-lifecycle-v1";

// ===========================================================================================
// Parameter classification
// ===========================================================================================

/// The parameter classes epsilon is frozen per, in plan 03-06.
///
/// Five rather than three, because weight and bias initializations differ by orders of
/// magnitude within the same block and a single epsilon over both would be set by whichever
/// is noisier. `Ord` so the class map iterates deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterClass {
    /// `embeddings.*_embeddings.weight` — the SPARSE class.
    Embedding,
    /// `*.LayerNorm.weight`.
    LayerNormWeight,
    /// `*.LayerNorm.bias`.
    LayerNormBias,
    /// `*.attention.self.*.weight`, `*.dense.weight`.
    ProjectionWeight,
    /// `*.attention.self.query.bias`, `*.attention.self.value.bias`, `*.dense.bias`.
    ProjectionBias,
    /// `*.attention.self.key.bias` — the ANALYTICALLY GRADIENT-FREE class.
    ///
    /// Split out of [`Self::ProjectionBias`] on a mechanism, not on a failing margin. Softmax
    /// is invariant to a constant shift of its inputs, and adding the key bias `b_k` to every
    /// key contributes `q_i . b_k` to the pre-softmax logit of EVERY key `j` for a given query
    /// `i` — the same amount for all `j`. The shift cancels, so `dL/db_k = 0` in exact
    /// arithmetic. The query bias does not have this property (`(q + b_q) . k_j` varies with
    /// `j`), and neither does the value bias (it adds a constant to the attention output,
    /// which the downstream layers see), which is why this class is the key bias ALONE and not
    /// "attention biases".
    ///
    /// Measured on the fixture slice: `grad_norm_max` 2.290e-10 for
    /// `encoder.layer.1.attention.self.key.bias` against 8.007e-3 for
    /// `encoder.layer.0.attention.self.query.weight` in the same block — 3.5e7x smaller, which
    /// is f32 cancellation residue rather than a small gradient.
    AttentionKeyBias,
}

impl ParameterClass {
    /// Every class, in `Ord` order — the iteration order every report uses.
    pub(crate) const ALL: [Self; 6] = [
        Self::Embedding,
        Self::LayerNormWeight,
        Self::LayerNormBias,
        Self::ProjectionWeight,
        Self::ProjectionBias,
        Self::AttentionKeyBias,
    ];

    /// Whether the denominator is restricted to the delta's support.
    ///
    /// Only the embedding tables. Every other parameter here is dense: a batch that touches
    /// the block at all touches every element of it, so the support IS the whole tensor and
    /// restricting would be a no-op dressed up as a policy.
    pub(crate) fn is_sparse(self) -> bool {
        matches!(self, Self::Embedding)
    }

    /// The contracted positive scale floor `s_class`.
    ///
    /// Unit for every class in v1, and stated as a per-class table anyway because plan 03-06
    /// freezes a per-class EPSILON and the two want to be read side by side. The value is 1.0
    /// rather than something smaller because it only ENGAGES when the (possibly
    /// support-restricted) initial norm is below unit scale — that is, for a parameter that
    /// started at or near zero — and unit scale is where the ratio's meaning changes from
    /// "fraction of the initial magnitude" to "absolute movement". Making the transition
    /// happen at 1.0 puts it somewhere a reader can name.
    pub(crate) fn scale_floor(self) -> f64 {
        match self {
            Self::Embedding
            | Self::LayerNormWeight
            | Self::LayerNormBias
            | Self::ProjectionWeight
            | Self::ProjectionBias
            | Self::AttentionKeyBias => 1.0,
        }
    }

    /// The stable string this class serializes as.
    pub(crate) fn tag(self) -> &'static str {
        match self {
            Self::Embedding => "embedding",
            Self::LayerNormWeight => "layer_norm_weight",
            Self::LayerNormBias => "layer_norm_bias",
            Self::ProjectionWeight => "projection_weight",
            Self::ProjectionBias => "projection_bias",
            Self::AttentionKeyBias => "attention_key_bias",
        }
    }
}

impl fmt::Display for ParameterClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// Failure modes of the evidence layer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum EvidenceError {
    /// A parameter name matched no class.
    ///
    /// FAIL CLOSED. A default bucket would absorb a renamed parameter into whichever class
    /// happened to be the fallback, and the epsilon frozen for that class would then be
    /// applied to something it was never measured on — silently, since nothing about the
    /// output would change shape.
    UnclassifiedParameter {
        /// The offending dotted name.
        name: String,
    },
    /// The canonical bytes could not be produced.
    Serialization {
        /// The renderer's diagnostic.
        reason: String,
    },
    /// A measurement in the table is not finite, so the canonical bytes would carry a `null`.
    ///
    /// FAIL CLOSED, for two independent reasons (REVIEW CR-03): the digest over those bytes is
    /// not injective — `+inf`, `-inf` and every `NaN` all render as the same `null` — and a
    /// bundle sealed with it cannot be reloaded, because `null` does not deserialize to `f64`.
    /// Neither failure is visible in the digest's shape, which is why this is refused at
    /// production rather than diagnosed later.
    NonFiniteMeasurement {
        /// Dotted path to the offending field, e.g. `rows.encoder.layer.0.attn.q.init_norm`.
        field: String,
    },
}

/// The dotted path of the first `null` in a JSON tree, in document order.
///
/// `None` means every leaf is a real value. Used only as a finiteness check — see
/// [`UpdateEvidence::to_canonical_bytes`] for why the check is a null-scan and not a list of
/// float field names.
fn first_null_path(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => Some(String::new()),
        serde_json::Value::Object(map) => map.iter().find_map(|(key, child)| {
            first_null_path(child).map(|rest| join_path(key, &rest))
        }),
        serde_json::Value::Array(items) => items.iter().enumerate().find_map(|(index, child)| {
            first_null_path(child).map(|rest| join_path(&index.to_string(), &rest))
        }),
        _ => None,
    }
}

/// Join one path segment onto a (possibly empty) remainder.
fn join_path(head: &str, rest: &str) -> String {
    if rest.is_empty() { head.to_string() } else { format!("{head}.{rest}") }
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnclassifiedParameter { name } => write!(
                f,
                "parameter `{name}` matches no parameter class; the encoder's dotted naming \
                 has drifted from the class mapping and a per-class epsilon cannot be applied \
                 to it (contract setfit-train-lifecycle-v1, requirement TRN-03)",
            ),
            Self::Serialization { reason } => write!(
                f,
                "the evidence table could not be rendered canonically: {reason} \
                 (contract setfit-train-lifecycle-v1, requirement TRN-03)",
            ),
            Self::NonFiniteMeasurement { field } => write!(
                f,
                "the evidence field `{field}` is not finite, so the canonical bytes would carry \
                 a `null` there. Refused at production: serde_json renders +inf, -inf and every \
                 NaN identically, so the table hash could not tell three different divergences \
                 apart, and the bundle sealed with it would fail its own reload (contract \
                 setfit-train-lifecycle-v1, requirement TRN-03)",
            ),
        }
    }
}

impl std::error::Error for EvidenceError {}

/// Classify a parameter by its HF dotted name. Pure, total on its accepted domain, fail-closed
/// everywhere else.
///
/// # Errors
///
/// [`EvidenceError::UnclassifiedParameter`] for any name outside the mapping.
pub(crate) fn classify_parameter(name: &str) -> Result<ParameterClass, EvidenceError> {
    let unclassified = || EvidenceError::UnclassifiedParameter { name: name.to_string() };
    let leaf = name.rsplit('.').next().unwrap_or("");

    // The embedding TABLES only. `embeddings.LayerNorm.weight` lives under the same prefix
    // and is deliberately NOT sparse, which is why this test is on the suffix rather than on
    // the `embeddings.` prefix alone.
    if name.starts_with("embeddings.") && name.ends_with("_embeddings.weight") {
        return Ok(ParameterClass::Embedding);
    }
    if name.contains(".LayerNorm.") {
        return match leaf {
            "weight" => Ok(ParameterClass::LayerNormWeight),
            "bias" => Ok(ParameterClass::LayerNormBias),
            _ => Err(unclassified()),
        };
    }
    // BEFORE the general projection branch: the key BIAS is gradient-free, the key WEIGHT is
    // not (only a constant shift of the logits cancels, and `W_k x` is not constant in x).
    if name.contains(".attention.self.key.") {
        return match leaf {
            "weight" => Ok(ParameterClass::ProjectionWeight),
            "bias" => Ok(ParameterClass::AttentionKeyBias),
            _ => Err(unclassified()),
        };
    }
    if name.contains(".dense.") || name.contains(".attention.self.") {
        return match leaf {
            "weight" => Ok(ParameterClass::ProjectionWeight),
            "bias" => Ok(ParameterClass::ProjectionBias),
            _ => Err(unclassified()),
        };
    }
    Err(unclassified())
}

// ===========================================================================================
// The relative delta
// ===========================================================================================

/// The denominator actually used, and the floor that was applied to it.
///
/// Returned as a pair rather than folded into the ratio so the evidence row can record BOTH
/// and a reader can tell a small ratio caused by a large denominator from one caused by a
/// small delta.
pub(crate) fn denominator_of(record: &ParamRecord, class: ParameterClass) -> (f64, f64) {
    let raw = if class.is_sparse() { record.init_norm_on_support } else { record.init_norm };
    (raw, class.scale_floor())
}

/// `||dTheta|| / max(denom, s_class)` — THE formula, defined exactly once.
pub(crate) fn relative_delta(record: &ParamRecord, class: ParameterClass) -> f64 {
    let (raw, floor) = denominator_of(record, class);
    record.delta_norm / raw.max(floor)
}

/// The strict predicate: did this parameter move AT ALL?
///
/// Separate from the ratio on purpose. A parameter whose delta is exactly zero has a ratio of
/// exactly zero too, but the two facts answer different questions and 03-06 gates on both.
pub(crate) fn moved(record: &ParamRecord) -> bool {
    record.delta_norm > 0.0
}

// ===========================================================================================
// The canonical table
// ===========================================================================================

/// One parameter's row. Fixed field order; the wire form is the canonical form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRow {
    /// The HF dotted name.
    pub(crate) name: String,
    /// The class the per-class epsilon is frozen for.
    pub(crate) class: ParameterClass,
    /// Elements in the tensor.
    pub(crate) element_count: u64,
    /// `||theta_init||_2` over the whole tensor.
    pub(crate) init_norm: f64,
    /// `||theta_final - theta_init||_2`.
    pub(crate) delta_norm: f64,
    /// Elements whose delta is not exactly zero.
    pub(crate) delta_support_count: u64,
    /// `delta_support_count / element_count`.
    pub(crate) delta_support_fraction: f64,
    /// The denominator BEFORE the floor was applied.
    pub(crate) denom_used: f64,
    /// The contracted floor for this class.
    pub(crate) scale_floor_used: f64,
    /// `delta_norm / max(denom_used, scale_floor_used)`.
    pub(crate) relative_delta: f64,
    /// The strict `||dTheta|| > 0` predicate.
    pub(crate) moved: bool,
    /// Largest per-step PRE-clip gradient norm.
    pub(crate) grad_norm_max: f64,
    /// Index-order mean of the per-step PRE-clip gradient norms.
    pub(crate) grad_norm_mean: f64,
    /// Steps at which this parameter had a gradient.
    pub(crate) steps_observed: u64,
}

/// The full evidence table: per-parameter rows plus the run-level recorded facts.
///
/// `rows` is a `BTreeMap`, so name ordering is discharged STRUCTURALLY rather than by a sort
/// somebody has to remember to call. `batch_boundary_list` is a `Vec` and its order IS the
/// event order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateEvidence {
    /// Wire schema version.
    pub(crate) schema_version: u32,
    /// Per-parameter rows, in name order.
    pub(crate) rows: BTreeMap<String, EvidenceRow>,
    /// Hex SHA-256 of the loss trace's LE `f32` bits, in step order.
    pub(crate) loss_trace_hash: String,
    /// Hex SHA-256 of the pairs actually consumed, absorbed at the draw.
    pub(crate) consumed_pair_digest: String,
    /// Hex SHA-256 of the batch boundaries actually opened.
    pub(crate) batch_boundary_digest: String,
    /// `(epoch, start_ordinal, len)` per batch, in the order the batches opened.
    ///
    /// The RECORDED source for plan 03-08's `batch_boundaries()` accessor. Without it that
    /// accessor would have to recompute the boundaries from configuration, which is the
    /// false-green the in-band digests exist to remove.
    pub(crate) batch_boundary_list: Vec<(u32, u64, u32)>,
    /// Hex SHA-256 of the ordered trainable parameter names.
    pub(crate) parameter_registry_hash: String,
    /// Optimizer steps taken.
    pub(crate) step_count: u64,
    /// Mean of the first `k` losses.
    pub(crate) first_k_mean: f64,
    /// Mean of the last `k` losses.
    pub(crate) last_k_mean: f64,
    /// The endpoint window.
    pub(crate) k: usize,
    /// Smallest relative delta among the sparse class.
    pub(crate) embedding_delta_min: f64,
    /// Median relative delta among the sparse class.
    pub(crate) embedding_delta_median: f64,
    /// Largest relative delta among the sparse class.
    pub(crate) embedding_delta_max: f64,
    /// Largest PRE-clip global gradient norm across steps.
    pub(crate) pre_clip_norm_max: f64,
    /// The regime this evidence was measured under.
    pub(crate) calibration_regime_id: String,
}

impl UpdateEvidence {
    /// Build the table from a recorded [`TuneOutput`].
    ///
    /// Every digest is MOVED from the recorded output; nothing here recomputes one.
    ///
    /// # Errors
    ///
    /// [`EvidenceError::UnclassifiedParameter`] if any parameter name matches no class.
    pub(crate) fn from_tune_output(
        out: &TuneOutput,
        calibration_regime_id: &str,
    ) -> Result<Self, EvidenceError> {
        let mut rows = BTreeMap::new();
        for (name, record) in &out.per_name {
            rows.insert(name.clone(), row_for(name, record)?);
        }
        let embedding_deltas: Vec<f64> =
            rows.values().filter(|r| r.class.is_sparse()).map(|r| r.relative_delta).collect();
        Ok(Self {
            schema_version: EVIDENCE_SCHEMA_VERSION,
            rows,
            loss_trace_hash: hex::encode(out.loss_trace_hash),
            consumed_pair_digest: hex::encode(out.consumed_pair_digest),
            batch_boundary_digest: hex::encode(out.batch_boundary_digest),
            batch_boundary_list: out.batch_boundaries.clone(),
            parameter_registry_hash: hex::encode(out.parameter_registry_hash),
            step_count: out.step_count,
            first_k_mean: out.first_k_mean,
            last_k_mean: out.last_k_mean,
            k: out.endpoint_k,
            embedding_delta_min: min_of(&embedding_deltas),
            embedding_delta_median: median_of(&embedding_deltas),
            embedding_delta_max: max_of(&embedding_deltas),
            pre_clip_norm_max: max_of(
                &out.pre_clip_norms.iter().map(|v| f64::from(*v)).collect::<Vec<f64>>(),
            ),
            calibration_regime_id: calibration_regime_id.to_string(),
        })
    }

    /// The canonical bytes.
    ///
    /// `serde_json` over a fixed-field-order struct with `BTreeMap` rows. Two runs producing
    /// the same measurements produce the same bytes.
    ///
    /// # Errors
    ///
    /// [`EvidenceError::Serialization`].
    pub(crate) fn to_canonical_bytes(&self) -> Result<Vec<u8>, EvidenceError> {
        let bytes = serde_json::to_vec(self)
            .map_err(|e| EvidenceError::Serialization { reason: e.to_string() })?;

        // REVIEW CR-03. `serde_json` renders EVERY non-finite `f64` as `null`, silently: `+inf`,
        // `-inf` and every `NaN` payload produce byte-identical output. So `table_hash` was not
        // injective over exactly the values a diverged run produces, and the bundle sealed with
        // that digest could not be reloaded — `null` is not an `f64`. A digest is not a
        // finiteness check; this is the check.
        //
        // The scan runs on the bytes PARSED BACK, never on a `Value` used to produce them:
        // routing emission through `serde_json::Value` could reorder keys relative to struct
        // field order and would invalidate every digest already recorded. Parsing cannot.
        //
        // Scanning for `null` rather than enumerating the float fields is deliberate. Neither
        // `EvidenceRow` nor `UpdateEvidence` has an `Option` field, so a `null` can ONLY be a
        // non-finite float — which makes this exact today and still exact after someone adds an
        // `f64`. An enumerated field list is the guard that silently stops covering the newest
        // field, which is how these checks rot.
        if let Some(path) = first_null_path(&serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|e| EvidenceError::Serialization { reason: e.to_string() })?)
        {
            return Err(EvidenceError::NonFiniteMeasurement { field: path });
        }

        Ok(bytes)
    }

    /// SHA-256 of the canonical bytes.
    ///
    /// # Errors
    ///
    /// [`EvidenceError::Serialization`].
    pub(crate) fn table_hash(&self) -> Result<[u8; 32], EvidenceError> {
        let mut hasher = Sha256::new();
        hasher.update(self.to_canonical_bytes()?);
        Ok(hasher.finalize().into())
    }

    /// Rows of one class, in name order.
    pub(crate) fn rows_of_class(&self, class: ParameterClass) -> Vec<&EvidenceRow> {
        self.rows.values().filter(|r| r.class == class).collect()
    }
}

/// Build one row. The single place a `ParamRecord` becomes evidence.
fn row_for(name: &str, record: &ParamRecord) -> Result<EvidenceRow, EvidenceError> {
    let class = classify_parameter(name)?;
    let (denom_used, scale_floor_used) = denominator_of(record, class);
    #[allow(clippy::cast_precision_loss)]
    let fraction = if record.element_count == 0 {
        0.0
    } else {
        record.delta_support_count as f64 / record.element_count as f64
    };
    Ok(EvidenceRow {
        name: name.to_string(),
        class,
        element_count: record.element_count,
        init_norm: record.init_norm,
        delta_norm: record.delta_norm,
        delta_support_count: record.delta_support_count,
        delta_support_fraction: fraction,
        denom_used,
        scale_floor_used,
        relative_delta: relative_delta(record, class),
        moved: moved(record),
        grad_norm_max: record.grad_norm_max,
        grad_norm_mean: record.grad_norm_mean,
        steps_observed: record.steps_observed,
    })
}

// ===========================================================================================
// The summary (D-12)
// ===========================================================================================

/// The verdict. `Unjudged` is the only arm plan 03-05 can produce.
///
/// `Pass`/`Fail` arrive in plan 03-06, together with the frozen per-class epsilon they
/// compare against. Shipping them now would invite a comparison against a threshold that does
/// not exist yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Verdict {
    /// No threshold has been frozen, so no verdict is available.
    ///
    /// Still reachable: `EvidenceSummary::of` builds an UNJUDGED summary, and only the gate
    /// promotes it. A summary that never reached the gate must not claim a verdict.
    Unjudged,
    /// Every gated parameter cleared its contracted epsilon.
    Pass,
    /// At least one gated parameter missed it, or the run-level floor was not met.
    Fail,
}

/// Min / median / worst relative delta for one class.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassStats {
    /// Smallest relative delta in the class.
    pub(crate) min: f64,
    /// Median relative delta in the class.
    pub(crate) median: f64,
    /// Largest relative delta in the class. "Worst" in the D-12 sense: furthest from frozen.
    pub(crate) worst: f64,
    /// Rows in the class.
    pub(crate) count: usize,
    /// Whether EVERY row of the class satisfies the strict `||dTheta|| > 0` predicate.
    pub(crate) all_moved: bool,
}

/// The D-12 summary, BOUND to its table by [`Self::table_hash`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSummary {
    /// Wire schema version.
    pub(crate) schema_version: u32,
    /// The verdict. `Unjudged` in this plan.
    pub(crate) verdict: Verdict,
    /// Trainable parameters after `apply_freeze`.
    pub(crate) trainable_count: usize,
    /// Frozen parameters after `apply_freeze`.
    pub(crate) frozen_count: usize,
    /// Per-class statistics, in class order.
    pub(crate) per_class: BTreeMap<String, ClassStats>,
    /// The name of the row with the SMALLEST relative delta, over EVERY row in the table.
    ///
    /// # It is NOT "the parameter the gate would reject first"
    ///
    /// The table records ungated classes too — `attention_key_bias` above all, whose gradient
    /// is analytically zero, so its movement is f32 cancellation residue and is very nearly
    /// always the smallest number here. The gate never judges it. The parameter a rejection
    /// actually blames is `SetFitTrainError::EvidenceRejected::worst`, which is chosen among
    /// the GATED rows and by margin against that class's own epsilon.
    ///
    /// This field stays table-wide on purpose: `EvidenceSummary::of` is built without a
    /// `Thresholds`, and giving `ParameterClass` its own `gated` predicate would put the
    /// gated/ungated decision in two places — the failure the frozen table exists to prevent.
    pub(crate) worst_param_name: String,
    /// The frozen epsilon, when one exists. `None` while unjudged.
    pub(crate) epsilon_used: Option<f64>,
    /// The regime this evidence was measured under.
    pub(crate) calibration_regime_id: String,
    /// The contract this evidence discharges.
    pub(crate) contract_version: String,
    /// Hex SHA-256 of the full canonical table, BINDING this summary to it.
    pub(crate) table_hash: String,
}

impl EvidenceSummary {
    /// Summarize a table.
    ///
    /// # Errors
    ///
    /// [`EvidenceError::Serialization`] from the binding hash.
    pub(crate) fn of(
        evidence: &UpdateEvidence,
        trainable_count: usize,
        frozen_count: usize,
    ) -> Result<Self, EvidenceError> {
        let mut per_class = BTreeMap::new();
        for class in ParameterClass::ALL {
            let rows = evidence.rows_of_class(class);
            if rows.is_empty() {
                continue;
            }
            let values: Vec<f64> = rows.iter().map(|r| r.relative_delta).collect();
            per_class.insert(
                class.tag().to_string(),
                ClassStats {
                    min: min_of(&values),
                    median: median_of(&values),
                    worst: max_of(&values),
                    count: rows.len(),
                    all_moved: rows.iter().all(|r| r.moved),
                },
            );
        }
        let worst_param_name = evidence
            .rows
            .values()
            .min_by(|a, b| {
                a.relative_delta
                    .partial_cmp(&b.relative_delta)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.name.cmp(&b.name))
            })
            .map_or_else(String::new, |r| r.name.clone());

        Ok(Self {
            schema_version: EVIDENCE_SCHEMA_VERSION,
            verdict: Verdict::Unjudged,
            trainable_count,
            frozen_count,
            per_class,
            worst_param_name,
            epsilon_used: None,
            calibration_regime_id: evidence.calibration_regime_id.clone(),
            contract_version: EVIDENCE_CONTRACT_VERSION.to_string(),
            table_hash: hex::encode(evidence.table_hash()?),
        })
    }
}

// ===========================================================================================
// Fixed-order statistics
// ===========================================================================================

/// Smallest value, or `0.0` for an empty slice.
fn min_of(values: &[f64]) -> f64 {
    // An empty slice must not report +inf, which would serialize as `null`.
    if values.is_empty() {
        return 0.0;
    }
    values.iter().copied().fold(f64::INFINITY, f64::min)
}

/// Largest value, or `0.0` for an empty slice.
fn max_of(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

/// Median, in a FIXED order: sort ascending, then the middle, averaging the two middles for an
/// even count. `0.0` for an empty slice.
///
/// `sort_by` with a total comparator rather than `partial_cmp().unwrap()`: every value here is
/// finite by construction, and a comparator that panics on the one input that violates that
/// assumption turns a measurement into a crash.
///
/// # The even-count average stays in f64
///
/// An earlier form routed the two middle values through `reduce::sum_in_index_order`, which
/// takes `&[f32]` — so it NARROWED two `f64` relative deltas to `f32` before averaging them.
/// That is the exact defect `reduce`'s own module doc exists to prevent: a relative delta
/// below `f32::MIN_POSITIVE` (reachable for a near-frozen parameter, since the denominator is
/// floored at 1.0) flushes to zero, and the median that the run-level embedding floor is
/// compared against would report no movement for a parameter that moved. The two middles are
/// added in `f64` here, in index order, which is what the reduction discipline asks for at
/// this width.
fn median_of(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use aprender::setfit::FreezeGroup;

    use crate::train::setfit::test_fixtures as fx;
    use crate::train::setfit::thresholds::Thresholds;
    use crate::train::setfit::tune::{run_tuning, validate_evidence};
    use crate::train::setfit::{EncoderTuned, SetFitRun, SetFitTrainError};

    /// Every name the fixture encoder emits, with its expected class. A CASE TABLE, not a
    /// spot check: a mapping tested on three names is a mapping that has not been tested.
    const CASE_TABLE: [(&str, ParameterClass); 18] = [
        ("embeddings.word_embeddings.weight", ParameterClass::Embedding),
        ("embeddings.position_embeddings.weight", ParameterClass::Embedding),
        ("embeddings.token_type_embeddings.weight", ParameterClass::Embedding),
        ("embeddings.LayerNorm.weight", ParameterClass::LayerNormWeight),
        ("embeddings.LayerNorm.bias", ParameterClass::LayerNormBias),
        ("encoder.layer.0.attention.self.query.weight", ParameterClass::ProjectionWeight),
        ("encoder.layer.0.attention.self.key.bias", ParameterClass::AttentionKeyBias),
        ("encoder.layer.1.attention.self.key.bias", ParameterClass::AttentionKeyBias),
        // The BOUNDARY of the split, tested from both sides: the key WEIGHT is an ordinary
        // projection weight, and the query/value biases are ordinary projection biases. Only
        // the key BIAS is gradient-free.
        ("encoder.layer.0.attention.self.key.weight", ParameterClass::ProjectionWeight),
        ("encoder.layer.0.attention.self.query.bias", ParameterClass::ProjectionBias),
        ("encoder.layer.0.attention.self.value.bias", ParameterClass::ProjectionBias),
        ("encoder.layer.0.attention.self.value.weight", ParameterClass::ProjectionWeight),
        ("encoder.layer.0.attention.output.dense.weight", ParameterClass::ProjectionWeight),
        ("encoder.layer.0.attention.output.LayerNorm.bias", ParameterClass::LayerNormBias),
        ("encoder.layer.1.intermediate.dense.weight", ParameterClass::ProjectionWeight),
        ("encoder.layer.1.intermediate.dense.bias", ParameterClass::ProjectionBias),
        ("encoder.layer.1.output.dense.weight", ParameterClass::ProjectionWeight),
        ("encoder.layer.1.output.LayerNorm.weight", ParameterClass::LayerNormWeight),
    ];

    fn record(init_norm: f64, support_norm: f64, delta_norm: f64, support: u64) -> ParamRecord {
        ParamRecord {
            element_count: 100,
            init_norm,
            init_norm_on_support: support_norm,
            delta_norm,
            delta_support_count: support,
            grad_norm_max: 0.0,
            grad_norm_mean: 0.0,
            steps_observed: 0,
        }
    }

    #[test]
    fn evidence_classify_parameter_case_table() {
        for (name, expected) in CASE_TABLE {
            assert_eq!(classify_parameter(name), Ok(expected), "case table row `{name}`",);
        }
    }

    /// Every name the fixture ACTUALLY emits is covered by the case table AND classifiable.
    ///
    /// Without this the case table is a list of names somebody typed, which can drift from the
    /// encoder's real naming without anything turning red.
    #[test]
    fn evidence_case_table_covers_every_name_the_fixture_emits() {
        let mut encoder = fx::slice_encoder(fx::FIXTURE_SEED);
        let names: Vec<String> =
            encoder.trainable_parameters_mut().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names.len(), 37);
        for name in &names {
            classify_parameter(name)
                .unwrap_or_else(|e| panic!("the fixture emits an unclassifiable name: {e}"));
        }
        // And every SHAPE of name in the table really occurs: no table row is fiction.
        for (name, _) in CASE_TABLE {
            assert!(
                names.iter().any(|n| n == name),
                "case-table row `{name}` is not a name the fixture emits",
            );
        }
    }

    /// An unmatched name is a typed error, NOT a default bucket.
    #[test]
    fn evidence_unclassified_parameter_fails_closed() {
        for name in [
            "",
            "classifier.weight",
            "encoder.layer.0.attention.self.query.gamma",
            "embeddings.word_embeddings.bias",
            "pooler.dense",
        ] {
            match classify_parameter(name) {
                Err(EvidenceError::UnclassifiedParameter { name: reported }) => {
                    assert_eq!(reported, name);
                }
                other => panic!("`{name}` must be unclassified, got {other:?}"),
            }
        }
        // Non-vacuity: a neighbouring valid name IS classified, so the rejections above are
        // not "everything is rejected".
        assert!(classify_parameter("pooler.dense.weight").is_ok());
    }

    /// THE zero-init proof: a parameter whose initial tensor is exactly zero has a FINITE,
    /// non-NaN relative delta.
    #[test]
    // The 0.0/0.0 below is the SUBJECT of this test, not an accident: it demonstrates the
    // NaN the un-floored ratio produces and that `NaN > eps` is false. Computing it any
    // other way would stop demonstrating the artifact.
    // Likewise the negated comparison: `!(NaN > eps)` IS the incomparability being
    // demonstrated. `partial_cmp` would state it in a form that no longer shows the artifact
    // the un-floored ratio produces.
    #[allow(clippy::zero_divided_by_zero, clippy::neg_cmp_op_on_partial_ord)]
    fn evidence_relative_delta_is_finite_at_zero_initialization() {
        let zero_init = record(0.0, 0.0, 0.25, 40);
        for class in ParameterClass::ALL {
            let value = relative_delta(&zero_init, class);
            assert!(
                value.is_finite(),
                "{class}: relative delta must be finite at zero init, got {value}",
            );
            assert!(!value.is_nan(), "{class}: and not NaN");
            assert_eq!(
                value,
                0.25 / class.scale_floor(),
                "{class}: the floor must be what divides",
            );
        }

        // The un-floored form has TWO failure modes at zero init, and they fail in OPPOSITE
        // directions. Both are demonstrated here rather than asserted from memory, because
        // the plan's prose named only one of them and named it as the only one.
        //
        // (1) delta > 0, init == 0  ->  +inf, which is GREATER than every threshold. A
        //     parameter that crawled 1e-45 away from zero would pass the gate outright.
        let moved_from_zero = zero_init.delta_norm / zero_init.init_norm;
        assert!(moved_from_zero.is_infinite());
        assert!(
            moved_from_zero > 1e-3,
            "the un-floored form ACCEPTS any movement from a zero init, however small",
        );
        // (2) delta == 0, init == 0  ->  NaN, and `NaN > eps` is FALSE. A parameter that did
        //     not move is rejected — correctly, but for a reason the diagnosis cannot state,
        //     and by the same expression that wrongly accepted case (1).
        let never_moved = 0.0_f64 / 0.0_f64;
        assert!(never_moved.is_nan());
        assert!(
            !(never_moved > 1e-3),
            "NaN > eps is FALSE, so the un-floored form's rejection is a NaN artifact",
        );

        // The floored form gives a finite, ORDERED answer in both cases, and the strict
        // predicate — not the ratio — is what distinguishes them.
        let floored_moved = relative_delta(&zero_init, ParameterClass::LayerNormBias);
        let floored_still =
            relative_delta(&record(0.0, 0.0, 0.0, 0), ParameterClass::LayerNormBias);
        assert!(floored_moved.is_finite() && floored_still.is_finite());
        assert!(floored_moved > floored_still);
        assert!(moved(&zero_init));
        assert!(!moved(&record(0.0, 0.0, 0.0, 0)));
    }

    /// The companion: a zeroed AND unchanged parameter fails the strict predicate.
    #[test]
    fn evidence_strict_predicate_rejects_a_parameter_that_did_not_move() {
        let frozen = record(0.0, 0.0, 0.0, 0);
        assert!(!moved(&frozen), "a zero delta is not movement");
        assert!(
            relative_delta(&frozen, ParameterClass::LayerNormBias).is_finite(),
            "and its ratio is still finite, which is exactly why the two predicates differ",
        );
        assert_eq!(relative_delta(&frozen, ParameterClass::LayerNormBias), 0.0);

        let moved_a_little = record(0.0, 0.0, f64::MIN_POSITIVE, 1);
        assert!(moved(&moved_a_little));
    }

    /// The sparse denominator uses ONLY the rows the delta touched.
    #[test]
    fn evidence_sparse_denominator_is_restricted_to_the_support() {
        // A 2-of-N support: the whole-table norm is 100.0, the support-restricted norm 3.0.
        let sparse = record(100.0, 3.0, 6.0, 2);
        let (denom, floor) = denominator_of(&sparse, ParameterClass::Embedding);
        assert_eq!(denom, 3.0, "the sparse denominator must be support-restricted");
        assert_eq!(floor, 1.0);
        assert_eq!(relative_delta(&sparse, ParameterClass::Embedding), 2.0);

        // Every dense class uses the whole tensor.
        for class in ParameterClass::ALL.into_iter().filter(|c| !c.is_sparse()) {
            let (denom, _) = denominator_of(&sparse, class);
            assert_eq!(denom, 100.0, "{class} must use the whole initial tensor");
        }
    }

    /// Vocabulary-size invariance, which is the transfer argument in one assertion.
    ///
    /// Two embedding tables with the SAME touched rows and the SAME delta, differing only in
    /// how many untouched rows they carry, must produce the SAME relative delta.
    #[test]
    fn evidence_sparse_relative_delta_is_invariant_to_vocabulary_size() {
        let small_vocab = record(10.0, 3.0, 6.0, 2);
        let mut large_vocab = record(500.0, 3.0, 6.0, 2);
        large_vocab.element_count = 1_000_000;

        assert_eq!(
            relative_delta(&small_vocab, ParameterClass::Embedding),
            relative_delta(&large_vocab, ParameterClass::Embedding),
            "the sparse ratio must not depend on the untouched rows",
        );
        // Control: the whole-table form DOES depend on them, so the invariance above is a
        // property of the support restriction and not of the numbers chosen.
        assert_ne!(
            small_vocab.delta_norm / small_vocab.init_norm,
            large_vocab.delta_norm / large_vocab.init_norm,
        );
    }

    /// Canonical bytes are bitwise stable across two identical runs.
    #[test]
    fn evidence_canonical_bytes_are_stable_across_two_runs() {
        let a = evidence_for(fx::default_variant(), None);
        let b = evidence_for(fx::default_variant(), None);
        assert_eq!(
            a.to_canonical_bytes().expect("bytes"),
            b.to_canonical_bytes().expect("bytes"),
            "two identical runs must serialize identically",
        );
        assert_eq!(a.table_hash().expect("hash"), b.table_hash().expect("hash"));
        assert_eq!(a.rows.len(), 37, "non-vacuity: the table has rows");
    }

    /// A single-field mutation changes `table_hash`, and the summary detects the divergence.
    #[test]
    fn evidence_hash_binds_the_summary_to_the_table() {
        let evidence = evidence_for(fx::default_variant(), None);
        let summary = EvidenceSummary::of(&evidence, 37, 0).expect("summary");
        assert_eq!(summary.table_hash, hex::encode(evidence.table_hash().expect("h")));

        let mut tampered = evidence.clone();
        let key = tampered.rows.keys().next().cloned().expect("the table has rows");
        let row = tampered.rows.get_mut(&key).expect("row");
        row.relative_delta += 1.0;

        assert_ne!(
            evidence.table_hash().expect("h"),
            tampered.table_hash().expect("h"),
            "a single-field mutation must change the table hash",
        );
        assert_ne!(
            summary.table_hash,
            hex::encode(tampered.table_hash().expect("h")),
            "so the untouched summary no longer matches the edited table",
        );
    }

    /// `deny_unknown_fields` rejects an extended payload — verified with a SUBSTITUTION that
    /// is asserted to have applied, so this cannot become a test of the valid payload.
    #[test]
    fn evidence_deny_unknown_fields_rejects_an_extended_payload() {
        let evidence = evidence_for(fx::default_variant(), None);
        let json = String::from_utf8(evidence.to_canonical_bytes().expect("bytes")).expect("utf8");

        serde_json::from_str::<UpdateEvidence>(&json).expect("the valid payload must round-trip");

        let extended =
            json.replacen("{\"schema_version\":", "{\"injected\":1,\"schema_version\":", 1);
        assert_ne!(extended, json, "the substitution must have applied");
        assert!(
            serde_json::from_str::<UpdateEvidence>(&extended).is_err(),
            "an unknown field must be rejected",
        );
    }

    /// No wall-clock field survives into the wire form.
    #[test]
    fn evidence_wire_form_carries_no_wall_clock_field() {
        let evidence = evidence_for(fx::default_variant(), None);
        let json = String::from_utf8(evidence.to_canonical_bytes().expect("bytes")).expect("utf8");
        for banned in ["elapsed", "seconds", "nanos", "millis", "timestamp", "_at\""] {
            assert!(!json.contains(banned), "the evidence wire form must not contain `{banned}`",);
        }
        let summary = EvidenceSummary::of(&evidence, 37, 0).expect("summary");
        let summary_json = serde_json::to_string(&summary).expect("summary json");
        for banned in ["elapsed", "seconds", "nanos", "millis", "timestamp"] {
            assert!(!summary_json.contains(banned), "summary contains `{banned}`");
        }
    }

    /// A non-finite measurement is REFUSED at canonical-bytes time (REVIEW CR-03).
    ///
    /// The premise, measured directly rather than assumed: `serde_json` renders `+inf`, `-inf`
    /// and every `NaN` as `null` — three different divergences, one byte string — and
    /// `from_str::<f64>("null")` is `Err("invalid type: null, expected f64")`. So before this
    /// guard existed, `table_hash` was NOT injective over exactly the values a diverged run
    /// produces, and the bundle sealed with that digest could not be reloaded.
    ///
    /// A case table over all three values and over both structs, because a guard tested on one
    /// non-finite value in one field is a guard that has not been tested.
    #[test]
    fn evidence_refuses_a_non_finite_measurement_in_either_struct() {
        // The premise itself, so this test does not rest on a claim made in prose.
        for probe in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            assert_eq!(
                serde_json::to_string(&probe).expect("serde renders any f64"),
                "null",
                "the whole defect rests on this rendering; if it ever changes, revisit the guard",
            );
        }

        for poison in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            // (a) a row-level field, nested under the rows map.
            let mut evidence = evidence_for(fx::default_variant(), None);
            let row_name = evidence
                .rows
                .keys()
                .next()
                .expect("the fixture table has rows")
                .clone();
            evidence.rows.get_mut(&row_name).expect("row present").relative_delta = poison;
            match evidence.to_canonical_bytes() {
                Err(EvidenceError::NonFiniteMeasurement { field }) => {
                    assert!(
                        field.contains(&row_name) && field.ends_with("relative_delta"),
                        "the path must name the offending field, got `{field}`",
                    );
                }
                other => panic!("expected NonFiniteMeasurement for {poison:?}, got {other:?}"),
            }

            // (b) a top-level field of UpdateEvidence itself.
            let mut evidence = evidence_for(fx::default_variant(), None);
            evidence.pre_clip_norm_max = poison;
            match evidence.to_canonical_bytes() {
                Err(EvidenceError::NonFiniteMeasurement { field }) => {
                    assert_eq!(field, "pre_clip_norm_max");
                }
                other => panic!("expected NonFiniteMeasurement for {poison:?}, got {other:?}"),
            }

            // (c) and table_hash, the actual consumer, must fail too rather than hash a null.
            let mut evidence = evidence_for(fx::default_variant(), None);
            evidence.pre_clip_norm_max = poison;
            assert!(
                evidence.table_hash().is_err(),
                "table_hash must not produce a digest over a null",
            );
        }
    }

    /// The control: an untouched fixture table serializes and hashes.
    ///
    /// Without this, the test above could pass because `to_canonical_bytes` rejects everything.
    #[test]
    fn evidence_control_finite_table_still_serializes_and_hashes() {
        let evidence = evidence_for(fx::default_variant(), None);
        let bytes = evidence.to_canonical_bytes().expect("a finite table must serialize");
        assert!(!bytes.is_empty(), "the canonical bytes are non-empty");
        evidence.table_hash().expect("a finite table must hash");
    }

    /// The digests are MOVED from the recorded output, not recomputed.
    #[test]
    fn evidence_digests_come_from_the_recorded_output() {
        let out = tune_output(fx::default_variant(), None);
        let evidence = UpdateEvidence::from_tune_output(&out, "test").expect("evidence");
        assert_eq!(evidence.consumed_pair_digest, hex::encode(out.consumed_pair_digest));
        assert_eq!(evidence.batch_boundary_digest, hex::encode(out.batch_boundary_digest));
        assert_eq!(evidence.loss_trace_hash, hex::encode(out.loss_trace_hash));
        assert_eq!(evidence.parameter_registry_hash, hex::encode(out.parameter_registry_hash));
        assert_eq!(evidence.batch_boundary_list, out.batch_boundaries);
        assert_eq!(evidence.step_count, out.step_count);
        assert!(
            !evidence.batch_boundary_list.is_empty(),
            "03-08's batch_boundaries() accessor needs a recorded source",
        );
    }

    /// The split is a MECHANISM, and this is the measurement that pins it.
    ///
    /// `AttentionKeyBias` exists because softmax is invariant to a constant shift of its
    /// inputs, so the key bias contributes the same amount to every key's pre-softmax logit
    /// for a given query and `dL/db_k = 0` in exact arithmetic. If that reasoning is right,
    /// the key bias's gradient must be orders of magnitude below its OWN BLOCK's query bias,
    /// which has no such invariance since `(q + b_q) . k_j` varies with `j`.
    ///
    /// This runs in every `cargo test`, not behind `--ignored`, because it is the load-bearing
    /// justification for a class boundary that removes parameters from the gate. If a future
    /// encoder, kernel or reduction order makes the key bias gradient real, the split loses
    /// its basis and this test says so instead of the boundary quietly becoming folklore.
    #[test]
    fn evidence_attention_key_bias_is_gradient_free_relative_to_its_own_block() {
        let evidence = evidence_for(fx::default_variant(), None);

        let key_rows = evidence.rows_of_class(ParameterClass::AttentionKeyBias);
        let bias_rows = evidence.rows_of_class(ParameterClass::ProjectionBias);
        assert!(!key_rows.is_empty(), "the fixture must emit key biases");
        assert!(!bias_rows.is_empty(), "and ordinary projection biases to compare against");

        let key_worst = max_of(&key_rows.iter().map(|r| r.grad_norm_max).collect::<Vec<f64>>());
        let bias_best = min_of(&bias_rows.iter().map(|r| r.grad_norm_max).collect::<Vec<f64>>());

        // Non-vacuity FIRST: an all-zero comparison would satisfy any ratio.
        assert!(bias_best > 0.0, "the comparison class must have real gradients");
        assert!(key_worst > 0.0, "and the key bias must have a measurable residue, not a hole");

        assert!(
            key_worst * 1e4 < bias_best,
            "the key bias gradient ({key_worst:e}) is not >=1e4x below the smallest ordinary \
             projection-bias gradient ({bias_best:e}); the shift-invariance argument the \
             AttentionKeyBias split rests on no longer holds and the class must be re-derived",
        );

        // The OTHER half of the boundary: only the key BIAS is split out. The key WEIGHT and
        // the query/value biases stay ordinary, so the split cannot silently widen into
        // "attention parameters are exempt".
        assert_eq!(
            classify_parameter("encoder.layer.0.attention.self.key.weight"),
            Ok(ParameterClass::ProjectionWeight),
        );
        assert_eq!(
            classify_parameter("encoder.layer.0.attention.self.query.bias"),
            Ok(ParameterClass::ProjectionBias),
        );
        assert_eq!(
            classify_parameter("encoder.layer.0.attention.self.value.bias"),
            Ok(ParameterClass::ProjectionBias),
        );
    }

    /// The summary is `Unjudged` and carries no epsilon in this plan.
    #[test]
    fn evidence_summary_is_unjudged_with_no_epsilon() {
        let evidence = evidence_for(fx::default_variant(), None);
        let summary = EvidenceSummary::of(&evidence, 37, 0).expect("summary");
        assert_eq!(summary.verdict, Verdict::Unjudged);
        assert_eq!(summary.epsilon_used, None);
        assert_eq!(summary.contract_version, EVIDENCE_CONTRACT_VERSION);
        assert_eq!(
            summary.per_class.len(),
            ParameterClass::ALL.len(),
            "every class the mapping can emit must be populated by the fixture; a class with \
             no rows would have its epsilon frozen against nothing",
        );
        assert_eq!(ParameterClass::ALL.len(), 6, "five classes plus the gradient-free split");
        assert!(!summary.worst_param_name.is_empty());
        for (class, stats) in &summary.per_class {
            assert!(stats.count > 0, "{class}");
            assert!(stats.min <= stats.median, "{class}");
            assert!(stats.median <= stats.worst, "{class}");
        }
    }

    /// Fixed-order statistics behave on the edge cases the table can present.
    #[test]
    fn evidence_statistics_handle_empty_and_even_inputs() {
        assert_eq!(min_of(&[]), 0.0);
        assert_eq!(max_of(&[]), 0.0);
        assert_eq!(median_of(&[]), 0.0);
        assert_eq!(min_of(&[3.0, 1.0, 2.0]), 1.0);
        assert_eq!(max_of(&[3.0, 1.0, 2.0]), 3.0);
        assert_eq!(median_of(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median_of(&[4.0, 1.0, 3.0, 2.0]), 2.5);
        assert!(min_of(&[]).is_finite(), "an empty min must serialize as a number");
    }

    // -----------------------------------------------------------------------------------
    // The armed gate (plan 03-06) — negative / control / mirror
    //
    // Ph1 D-24 / Ph2 D-25 discipline: each negative is REJECTED and its message NAMES the
    // offender, a CONTROL with the same setup minus the poison PASSES, and a MIRROR shows the
    // untouched path is unchanged by the gate's presence. All of them run in every `cargo
    // test` — a gate that is only ever exercised on the honest path is not evidence.
    // -----------------------------------------------------------------------------------

    /// The reference-defaults control run, built once and reused.
    ///
    /// A full `tune_encoder` pass per test would repeat the fixture load and the tuning loop
    /// several times over for no additional evidence.
    ///
    /// `calibrated_variant` rather than `default_variant`: everything below reaches a
    /// THRESHOLD comparison, and the regime check runs first, so these tests need a run at a
    /// seed and cell the epsilons were actually measured at. `default_variant` sits on
    /// `FIXTURE_SEED`, which the calibration never swept — see the seed-negative in `mod.rs`.
    fn control_run() -> Result<SetFitRun<EncoderTuned>, SetFitTrainError> {
        fx::prepared_run(fx::calibrated_variant(), None).tune_encoder()
    }

    /// CONTROL: a reference-defaults fixture run passes the gate and mints `EncoderTuned`.
    ///
    /// Without this the three negatives below would be satisfied by a gate that rejects
    /// everything, which is the failure mode that makes a rejection-only test suite worthless.
    #[test]
    fn negative_control_reference_run_passes_the_gate() {
        let run = control_run().expect("the reference-defaults fixture run must pass the gate");
        let passed = run.evidence();
        let summary = passed.summary();

        assert_eq!(summary.verdict, Verdict::Pass, "the control's verdict must be Pass");
        assert_eq!(summary.contract_version, EVIDENCE_CONTRACT_VERSION);
        assert!(
            Thresholds::frozen().is_calibrated(&summary.calibration_regime_id),
            "the control must run inside the calibrated regime, got `{}`",
            summary.calibration_regime_id,
        );

        // The per-class epsilon actually applied is the CONTRACT's, not a local literal.
        let frozen = Thresholds::frozen();
        for class in ParameterClass::ALL {
            let entry = frozen.of(class);
            let Some(eps) = entry.eps else { continue };
            for row in passed.table().rows_of_class(class) {
                assert!(
                    row.relative_delta > eps,
                    "{}: {} passed the gate at relative delta {:e} which does not exceed the \
                     contracted epsilon {eps:e}",
                    class.tag(),
                    row.name,
                    row.relative_delta,
                );
            }
        }

        // The summary is BOUND to the table it summarizes.
        assert_eq!(
            summary.table_hash,
            hex::encode(passed.table().table_hash().expect("table hash")),
            "the summary must bind to its own table",
        );
        assert_eq!(summary.trainable_count, passed.table().rows.len());
    }

    /// NEGATIVE 1 — an all-frozen run cannot pass by being un-checkable (SAFE-03, D-09).
    #[test]
    fn negative_all_frozen_run_has_no_trainable_parameters() {
        // Calibrated coordinates on purpose: the regime check runs BEFORE the empty-trainable-set
        // check, so an uncalibrated fixture here would be refused for the wrong reason and this
        // test would stop being about SAFE-03 at all.
        let variant = fx::calibrated_variant();
        // Every group of every layer, plus the embeddings: the complete freeze.
        let mut policy = vec![FreezeGroup::Embeddings];
        for layer in 0..fx::slice_encoder(variant.root_seed).num_layers() {
            policy.push(FreezeGroup::LayerAttention(layer));
            policy.push(FreezeGroup::LayerFfn(layer));
            policy.push(FreezeGroup::LayerNorm(layer));
        }
        let run = fx::prepared_run_with_freeze(variant, policy);
        match run.tune_encoder() {
            Err(SetFitTrainError::NoTrainableParameters { trainable_count }) => {
                assert_eq!(trainable_count, 0, "the message must report the observed count");
                let rendered =
                    SetFitTrainError::NoTrainableParameters { trainable_count }.to_string();
                assert!(rendered.contains("trainable_count 0"), "rendered: {rendered}");
                assert!(rendered.contains("SAFE-03"), "the diagnosis must name what it enforces");
            }
            other => panic!("an all-frozen run must be rejected as unpassable, got {other:?}"),
        }
    }

    /// NEGATIVE 2 — a 1e-30-learning-rate run is rejected and the message NAMES the offender.
    #[test]
    fn negative_null_learning_rate_run_is_rejected_naming_the_offender() {
        // Calibrated coordinates: this test is about the EPSILON comparison, which only runs
        // once the regime check has passed.
        let run = fx::prepared_run(fx::calibrated_variant(), Some(1e-30));
        match run.tune_encoder() {
            Err(SetFitTrainError::EvidenceRejected { worst, summary, table }) => {
                // The offender is named by its DOTTED HF name, not an index.
                assert!(
                    worst.name.contains('.'),
                    "the offender must be named by its dotted HF name, got `{}`",
                    worst.name,
                );
                // Its class is one the contract actually gates.
                let frozen = Thresholds::frozen();
                let class = ParameterClass::ALL
                    .into_iter()
                    .find(|c| c.tag() == worst.class)
                    .unwrap_or_else(|| panic!("unknown class `{}`", worst.class));
                assert!(frozen.of(class).gated, "an ungated class must never be blamed");

                // The measured delta and the CONTRACTED epsilon are both present and consistent.
                assert_eq!(
                    Some(worst.eps),
                    frozen.of(class).eps,
                    "the blamed epsilon must be the contract's value for that class",
                );
                assert!(
                    worst.relative_delta <= worst.eps,
                    "the offender must actually have missed its threshold: {:e} vs {:e}",
                    worst.relative_delta,
                    worst.eps,
                );
                // At 1e-30 every update underflows the parameter ULP, so the delta is exactly 0.
                assert_eq!(worst.relative_delta, 0.0, "a 1e-30 run moves nothing at f32 scale");

                // AUDITABLE FAILURE: the COMPLETE record travels inside the error.
                assert_eq!(
                    table.rows.len(),
                    summary.trainable_count,
                    "the failed table must carry ONE ROW PER TRAINABLE PARAMETER so a rejection \
                     can be investigated rather than merely reported",
                );
                assert!(table.rows.len() > 1, "non-vacuity: the fixture has many parameters");
                assert!(table.rows.contains_key(&worst.name), "the offender must be IN the table");
                assert_eq!(summary.verdict, Verdict::Fail);
                // Run-level facts survive the rejection too.
                assert!(!table.loss_trace_hash.is_empty());
                assert!(!table.consumed_pair_digest.is_empty());
                assert!(!table.batch_boundary_list.is_empty());

                let rendered =
                    SetFitTrainError::EvidenceRejected { worst, summary, table }.to_string();
                assert!(rendered.contains("REJECTED"), "rendered: {rendered}");
                assert!(rendered.contains("TRN-03"), "the diagnosis must name its requirement");
            }
            other => panic!("a 1e-30 run must be rejected by the gate, got {other:?}"),
        }
    }

    /// NEGATIVE 2b — a run in which EVERY gated parameter MOVED is still rejected, by the
    /// epsilon and by nothing else.
    ///
    /// This test exists because mutating the gate exposed a hole in the set above. Deleting
    /// the `relative_delta > eps` comparison outright left every other negative GREEN: the
    /// 1e-30 run has bit-for-bit zero deltas so the strict movement predicate catches it
    /// before the threshold is ever consulted, and even a real 1e-8 run contains some
    /// parameters that did not move at all, so it too is rejected without the epsilon.
    ///
    /// The only witness that isolates the threshold is a table in which EVERYTHING moved and
    /// everything fell short. It is built from the CONTROL's real table, with every relative
    /// delta scaled below its class epsilon and `moved` left true, so the sole reason to
    /// reject it is the comparison this test exists to protect.
    #[test]
    fn negative_a_run_that_moved_everywhere_but_fell_short_is_rejected_by_the_epsilon() {
        let out = tune_output(fx::default_variant(), None);
        let frozen = Thresholds::frozen();

        // CONTROL: unmodified, this table passes.
        let good = UpdateEvidence::from_tune_output(&out, &regime_id()).expect("evidence");
        validate_evidence(&good, &frozen, out.trainable_count, out.frozen_count)
            .expect("CONTROL: the reference table passes before it is scaled down");

        // The poison: everything still MOVED, everything now falls short.
        let mut short = good.clone();
        for row in short.rows.values_mut() {
            row.relative_delta *= 1e-3;
            assert!(row.moved, "the scaling must not disturb the movement predicate");
            assert!(row.delta_norm > 0.0);
        }
        short.embedding_delta_median *= 1e-3;

        // Non-vacuity: EVERY gated row moved, so nothing here is rejectable by `moved`.
        let gated_rows: Vec<&EvidenceRow> =
            short.rows.values().filter(|r| frozen.of(r.class).gated).collect();
        assert!(!gated_rows.is_empty());
        assert!(
            gated_rows.iter().all(|r| r.moved && r.delta_norm > 0.0),
            "if any gated parameter failed to move, the strict predicate could reject this \
             table and the epsilon would again go untested",
        );
        assert!(
            gated_rows.iter().all(|r| r.grad_norm_max.is_finite()),
            "and every gradient is finite, so the finiteness predicate cannot reject it either",
        );

        match validate_evidence(&short, &frozen, out.trainable_count, out.frozen_count) {
            Err(SetFitTrainError::EvidenceRejected { worst, .. }) => {
                let class = ParameterClass::ALL
                    .into_iter()
                    .find(|c| c.tag() == worst.class)
                    .unwrap_or_else(|| panic!("unknown class `{}`", worst.class));
                assert_eq!(Some(worst.eps), frozen.of(class).eps);
                assert!(
                    worst.relative_delta > 0.0,
                    "the blamed parameter MOVED ({:e}); only the threshold rejected it",
                    worst.relative_delta,
                );
                assert!(worst.relative_delta <= worst.eps);
            }
            other => panic!(
                "a table in which everything moved but fell short of its epsilon MUST be \
                 rejected; got {other:?}. If this returned Ok, the epsilon comparison is not \
                 doing anything and every frozen threshold in the contract is decoration.",
            ),
        }
    }

    /// NEGATIVE 3 — an out-of-regime run is refused BEFORE any threshold is compared.
    ///
    /// Driven through `validate_evidence` directly with a doctored regime id, because the point
    /// is the ORDER of the checks: the table handed in is the CONTROL's, which passes every
    /// threshold. If the regime check ran second, this table would pass and the test would be
    /// green for the wrong reason.
    #[test]
    fn negative_uncalibrated_regime_is_refused_before_any_comparison() {
        let out = tune_output(fx::default_variant(), None);
        let frozen = Thresholds::frozen();

        let good = UpdateEvidence::from_tune_output(&out, &regime_id()).expect("evidence");
        validate_evidence(&good, &frozen, out.trainable_count, out.frozen_count)
            .expect("CONTROL: this very table passes when its regime is calibrated");

        let foreign =
            "minilm-full-h384-l6-a12-i1536-v30522@production|seeds=1,42,7|cells=s16e2b8,s8e1b4";
        let bad = UpdateEvidence::from_tune_output(&out, foreign).expect("evidence");
        match validate_evidence(&bad, &frozen, out.trainable_count, out.frozen_count) {
            Err(SetFitTrainError::UncalibratedRegime { observed, calibrated }) => {
                assert_eq!(observed, foreign);
                assert_eq!(calibrated.len(), 1, "exactly one calibrated fingerprint");
                assert!(!calibrated.contains(&foreign.to_string()));
                let rendered =
                    SetFitTrainError::UncalibratedRegime { observed, calibrated }.to_string();
                assert!(rendered.contains("D-10(c)"), "rendered: {rendered}");
            }
            other => panic!("an out-of-regime run must fail closed, got {other:?}"),
        }
    }

    /// NEGATIVE 3b — a trainable set of ONLY gradient-free parameters is unpassable.
    ///
    /// The hole the `attention_key_bias` exclusion would otherwise open: freeze everything the
    /// gate checks, leave only what it cannot check, and a naive `for p in gated { .. }` loop
    /// passes vacuously because it iterates over nothing.
    #[test]
    fn negative_only_gradient_free_parameters_cannot_testify() {
        let out = tune_output(fx::default_variant(), None);
        let full = UpdateEvidence::from_tune_output(&out, &regime_id()).expect("evidence");

        let mut ungated_only = full.clone();
        ungated_only.rows.retain(|_, r| r.class == ParameterClass::AttentionKeyBias);
        let kept = ungated_only.rows.len();
        assert!(kept > 0, "non-vacuity: the fixture must emit key biases");

        match validate_evidence(&ungated_only, &Thresholds::frozen(), kept, out.frozen_count) {
            Err(SetFitTrainError::NoTestifyingParameters { trainable_count, ungated_count }) => {
                assert_eq!(trainable_count, kept);
                assert_eq!(ungated_count, kept);
            }
            other => panic!("an all-ungated trainable set must be unpassable, got {other:?}"),
        }
    }

    /// MIRROR — arming the gate changed nothing about what the loop records.
    ///
    /// Two independent passing runs agree bit-for-bit on every recorded digest. The gate READS
    /// the evidence; if it had started to influence what the loop consumed, these would diverge.
    #[test]
    fn negative_mirror_two_passing_runs_agree_on_every_digest() {
        let first = control_run().expect("first control run");
        let second = control_run().expect("second control run");
        let (a, b) = (first.evidence().table(), second.evidence().table());

        assert_eq!(a.loss_trace_hash, b.loss_trace_hash, "loss trace");
        assert_eq!(a.consumed_pair_digest, b.consumed_pair_digest, "consumed pairs");
        assert_eq!(a.batch_boundary_digest, b.batch_boundary_digest, "batch boundaries");
        assert_eq!(a.batch_boundary_list, b.batch_boundary_list, "readable boundary list");
        assert_eq!(a.parameter_registry_hash, b.parameter_registry_hash, "registry");
        assert_eq!(a.step_count, b.step_count, "step count");

        // Non-vacuity: the digests are real, not two empty strings compared to each other.
        assert_eq!(a.loss_trace_hash.len(), 64, "a SHA-256 renders as 64 hex characters");
        assert!(a.step_count > 0, "the mirror must compare runs that actually ran");

        // And the whole table hash agrees, which covers every per-parameter measurement at once.
        assert_eq!(
            a.table_hash().expect("hash"),
            b.table_hash().expect("hash"),
            "two identical runs must produce identical evidence bytes",
        );
    }

    /// `PassedEvidence` retains the run-level fields every downstream accessor resolves to.
    ///
    /// W-06: anything dropped here becomes something a later plan RECOMPUTES, which is exactly
    /// the false-green the in-band digests exist to remove.
    #[test]
    fn evidence_gate_passed_evidence_carries_the_complete_record() {
        let run = control_run().expect("control");
        let table = run.evidence().table();

        assert!(!table.loss_trace_hash.is_empty());
        assert!(!table.consumed_pair_digest.is_empty());
        assert!(!table.batch_boundary_digest.is_empty());
        assert!(!table.batch_boundary_list.is_empty(), "03-08's recorded boundary source");
        assert!(!table.parameter_registry_hash.is_empty());
        assert!(table.step_count > 0);
        assert!(table.k > 0);
        assert!(table.first_k_mean.is_finite() && table.last_k_mean.is_finite());
        assert!(table.embedding_delta_min.is_finite());
        assert!(table.embedding_delta_median > 0.0);
        assert!(table.pre_clip_norm_max.is_finite());
        assert!(!table.calibration_regime_id.is_empty());
    }

    // -----------------------------------------------------------------------------------
    // The calibration matrix (measurement, not judgment)
    // -----------------------------------------------------------------------------------

    /// A run's recorded output for one cell.
    fn tune_output(
        variant: fx::CalibrationVariant,
        encoder_lr_override: Option<f64>,
    ) -> TuneOutput {
        let (encoder, dataset, selection, config) =
            fx::prepared_run(variant, encoder_lr_override).into_parts();
        run_tuning(encoder, &dataset, &selection, &config).expect("the fixture run must tune")
    }

    /// Its evidence table.
    fn evidence_for(
        variant: fx::CalibrationVariant,
        encoder_lr_override: Option<f64>,
    ) -> UpdateEvidence {
        let out = tune_output(variant, encoder_lr_override);
        UpdateEvidence::from_tune_output(&out, &regime_id()).expect("evidence")
    }

    /// The regime this matrix was measured under: architecture, seed set, boundary set.
    fn regime_id() -> String {
        let mut seeds: Vec<String> =
            fx::calibration_variants().iter().map(|v| v.root_seed.to_string()).collect();
        seeds.sort_unstable();
        seeds.dedup();
        let mut cells: Vec<String> =
            fx::calibration_variants().iter().map(|v| v.label.to_string()).collect();
        cells.sort_unstable();
        cells.dedup();
        format!(
            "minilm-slice-h64-l2-a2-i256-v97@1110a243|seeds={}|cells={}",
            seeds.join(","),
            cells.join(","),
        )
    }

    /// The largest `relative_delta` that pure `f32` REPRESENTATION ROUNDING can produce for
    /// this row, and therefore the floor a frozen epsilon has to clear to mean anything.
    ///
    /// Rigorous rather than estimated: rounding each element to the nearest `f32` perturbs it
    /// by at most half a ULP, i.e. `|dx_i| <= (EPSILON/2)|x_i|`, so
    /// `||dTheta||_2 <= (EPSILON/2)||theta||_2` and the ratio is bounded by
    /// `(EPSILON/2) * init_norm / max(denom_used, scale_floor_used)`. Every input is a field
    /// the evidence row already records, so this is MEASURED from the run rather than assumed
    /// from a nominal parameter magnitude.
    ///
    /// Plan 03-05 flagged `projection_bias` as unfreezable because `min/10` "sits close to f32
    /// resolution" while its predicate actually tested `median/min > 100` — a spread statistic
    /// that says nothing about resolution. This is the quantity that claim is about.
    fn rounding_noise_floor(row: &EvidenceRow) -> f64 {
        let denom = row.denom_used.max(row.scale_floor_used);
        (f64::from(f32::EPSILON) / 2.0) * row.init_norm / denom
    }

    /// The learning rate of the null control. Chosen by the plan; measured below to be far
    /// under the `f32` resolution of every parameter, which is what makes it a null.
    const CONTROL_LR: f64 = 1e-30;

    /// A SECOND control, at a small but fully REPRESENTABLE learning rate.
    ///
    /// `CONTROL_LR` is a numerical null: at 1e-30 every per-element update underflows the
    /// parameter's ULP, so the measured delta is bit-for-bit zero and the matrix bounds
    /// epsilon from ABOVE only (03-05's concern 3). 1e-8 is 2000x below the reference 2e-5 —
    /// a run that is not meaningfully training — but it is large enough that AdamW writes a
    /// different `f32` back. It therefore gives a REAL lower bound: whatever a
    /// nearly-not-training run can produce, a frozen epsilon must sit above.
    ///
    /// It is also the measurement that decides what the gradient-free class may assert. If a
    /// parameter's `||dTheta|| > 0` is true at 1e-8 just as it is at 2e-5, that predicate does
    /// not distinguish training from not-training FOR THAT PARAMETER, and arming it would be
    /// the same vacuity that disqualified the pair-loss endpoint statistic.
    const NEAR_NULL_LR: f64 = 1e-8;

    /// THE calibration matrix — `#[ignore]`d, and deliberately so.
    ///
    /// Twelve complete `run_tuning` passes over a real-weight MiniLM slice do not belong on
    /// the `evidence_` filter a developer types dozens of times a day. `#[ignore]` keeps it
    /// off every default run including `cargo test --workspace --lib`, while `pub(crate)`
    /// `run_tuning` and `#[cfg(test)]` `calibration_variants` both stay exactly as narrow as
    /// they are — the out-of-crate integration target an earlier draft specified could not
    /// have compiled against either.
    ///
    /// Invoke with:
    /// `cargo test -p aprender-train --lib --features setfit calibration_matrix -- --ignored --nocapture`
    #[test]
    #[ignore = "12 full tuning passes; run explicitly with --ignored (plan 03-05 epsilon basis)"]
    fn calibration_matrix_epsilon_basis() {
        let variants = fx::calibration_variants();
        assert!(variants.len() >= 6, "at least six cells");

        let mut report = String::new();
        report.push_str(&format!("\nCALIBRATION REGIME: {}\n", regime_id()));
        report.push_str(
            "\ncell             class                real_min      real_median   real_max      \
             ctrl_max      nnull_max     nnull_moved   noise_floor   support_frac  \
             all_moved\n",
        );

        // Cross-cell aggregates, per class, that plan 03-06 freezes epsilon from.
        let mut real_min_across: BTreeMap<&'static str, f64> = BTreeMap::new();
        let mut ctrl_max_across: BTreeMap<&'static str, f64> = BTreeMap::new();
        let mut median_max_across: BTreeMap<&'static str, f64> = BTreeMap::new();
        // The rounding-noise floor the frozen epsilon has to clear, per class, worst cell.
        let mut noise_floor_across: BTreeMap<&'static str, f64> = BTreeMap::new();
        // The near-null (1e-8) control's worst case per class — the REAL lower bound.
        let mut near_null_max_across: BTreeMap<&'static str, f64> = BTreeMap::new();
        // Whether EVERY near-null run still satisfies the strict `||dTheta|| > 0` predicate.
        let mut near_null_moved_all: BTreeMap<&'static str, bool> = BTreeMap::new();
        // WHICH parameter sets each class's lower bound. Without the name, 03-06 knows the
        // number but not what to widen if the margin turns out to be too narrow.
        let mut binding_param: BTreeMap<&'static str, String> = BTreeMap::new();
        let mut endpoint_rows: Vec<String> = Vec::new();
        let mut endpoint_deltas: Vec<f64> = Vec::new();
        let mut cells_run = 0_usize;

        for variant in &variants {
            let real = evidence_for(*variant, None);
            let control = evidence_for(*variant, Some(CONTROL_LR));
            let near_null = evidence_for(*variant, Some(NEAR_NULL_LR));
            cells_run += 3;

            let endpoint_delta = real.last_k_mean - real.first_k_mean;
            endpoint_deltas.push(endpoint_delta);
            endpoint_rows.push(format!(
                "  seed {:>3} cell {:<9} k={} first_k={:.9} last_k={:.9} delta={endpoint_delta:+.9}\n",
                variant.root_seed,
                variant.label,
                real.k,
                real.first_k_mean,
                real.last_k_mean,
            ));

            for class in ParameterClass::ALL {
                let real_rows = real.rows_of_class(class);
                let control_rows = control.rows_of_class(class);
                let near_null_rows = near_null.rows_of_class(class);
                assert!(!real_rows.is_empty(), "{class} has no rows");
                assert_eq!(real_rows.len(), control_rows.len());
                assert_eq!(real_rows.len(), near_null_rows.len());

                let real_values: Vec<f64> = real_rows.iter().map(|r| r.relative_delta).collect();
                let control_values: Vec<f64> =
                    control_rows.iter().map(|r| r.relative_delta).collect();
                let support: Vec<f64> =
                    real_rows.iter().map(|r| r.delta_support_fraction).collect();

                let near_null_values: Vec<f64> =
                    near_null_rows.iter().map(|r| r.relative_delta).collect();
                let real_min = min_of(&real_values);
                let control_max = max_of(&control_values);
                let near_null_max = max_of(&near_null_values);
                let near_null_moved = near_null_rows.iter().all(|r| r.moved);

                let cell_noise_floor = max_of(
                    &real_rows.iter().map(|r| rounding_noise_floor(r)).collect::<Vec<f64>>(),
                );

                report.push_str(&format!(
                    "seed{:<3} {:<9} {:<20} {:<13.3e} {:<13.3e} {:<13.3e} {:<13.3e} {:<13.3e} \
                     {:<13} {:<13.3e} {:<13.4} {}\n",
                    variant.root_seed,
                    variant.label,
                    class.tag(),
                    real_min,
                    median_of(&real_values),
                    max_of(&real_values),
                    control_max,
                    near_null_max,
                    near_null_moved,
                    cell_noise_floor,
                    median_of(&support),
                    real_rows.iter().all(|r| r.moved),
                ));

                // (c) SEPARATION, per class and per cell.
                assert!(
                    control_max < real_min,
                    // near_null is reported, not asserted: whether it separates is the
                    // question this matrix exists to answer, not a property to presume.
                    "seed {} cell {} class {}: the 1e-30 control's max relative delta \
                     ({control_max:e}) is not below the real run's min ({real_min:e})",
                    variant.root_seed,
                    variant.label,
                    class.tag(),
                );

                let slot = real_min_across.entry(class.tag()).or_insert(f64::INFINITY);
                if real_min < *slot {
                    *slot = real_min;
                    let binding = real_rows
                        .iter()
                        .min_by(|a, b| {
                            a.relative_delta
                                .partial_cmp(&b.relative_delta)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .map_or_else(String::new, |r| {
                            format!(
                                "{} (seed {} cell {}) delta_norm={:.3e} init_norm={:.3e} \
                                 grad_norm_max={:.3e} grad_norm_mean={:.3e} steps_observed={} \
                                 noise_floor={:.3e}",
                                r.name,
                                variant.root_seed,
                                variant.label,
                                r.delta_norm,
                                r.init_norm,
                                r.grad_norm_max,
                                r.grad_norm_mean,
                                r.steps_observed,
                                rounding_noise_floor(r),
                            )
                        });
                    binding_param.insert(class.tag(), binding);
                }
                let slot = noise_floor_across.entry(class.tag()).or_insert(f64::NEG_INFINITY);
                *slot = slot.max(cell_noise_floor);
                let slot = near_null_max_across.entry(class.tag()).or_insert(f64::NEG_INFINITY);
                *slot = slot.max(near_null_max);
                let slot = near_null_moved_all.entry(class.tag()).or_insert(true);
                *slot = *slot && near_null_moved;
                let slot = ctrl_max_across.entry(class.tag()).or_insert(f64::NEG_INFINITY);
                *slot = slot.max(control_max);
                let slot = median_max_across.entry(class.tag()).or_insert(f64::NEG_INFINITY);
                *slot = slot.max(median_of(&real_values));
            }
        }

        report.push_str("\nENDPOINT MEANS (computed, NOT judged)\n");
        for row in &endpoint_rows {
            report.push_str(row);
        }
        report.push_str(&format!(
            "  CROSS-SEED SPREAD of (last_k - first_k): min={:+.9} max={:+.9} range={:.9}\n",
            min_of(&endpoint_deltas),
            max_of(&endpoint_deltas),
            max_of(&endpoint_deltas) - min_of(&endpoint_deltas),
        ));

        report.push_str("\nCROSS-CELL EPSILON BASIS (03-06 freezes from these)\n");
        report.push_str(
            "class                worst_ctrl    worst_nnull   best_real     10x_lower     \
             10x_upper     noise_floor   eps/noise     nnull_moved   supports_margin  \
             median/min\n",
        );
        for class in ParameterClass::ALL {
            let worst_ctrl = ctrl_max_across.get(class.tag()).copied().unwrap_or(0.0);
            let best_real = real_min_across.get(class.tag()).copied().unwrap_or(0.0);
            let worst_median = median_max_across.get(class.tag()).copied().unwrap_or(0.0);
            let worst_nnull = near_null_max_across.get(class.tag()).copied().unwrap_or(0.0);
            // The lower edge is the WORSE of the two controls: a frozen epsilon has to sit
            // above anything a not-really-training run produced, and the 1e-8 control is the
            // one that produces anything at all.
            let lower = worst_ctrl.max(worst_nnull) * 10.0;
            let upper = best_real / 10.0;
            let spread = if best_real > 0.0 { worst_median / best_real } else { f64::INFINITY };
            let noise_floor = noise_floor_across.get(class.tag()).copied().unwrap_or(0.0);
            let eps_over_noise =
                if noise_floor > 0.0 { upper / noise_floor } else { f64::INFINITY };
            report.push_str(&format!(
                "{:<20} {:<13.3e} {:<13.3e} {:<13.3e} {:<13.3e} {:<13.3e} {:<13.3e} {:<13.2e} \
                 {:<13} {:<16} {:.1e}\n",
                class.tag(),
                worst_ctrl,
                worst_nnull,
                best_real,
                lower,
                upper,
                noise_floor,
                eps_over_noise,
                near_null_moved_all.get(class.tag()).copied().unwrap_or(false),
                lower < upper,
                spread,
            ));
        }

        report.push_str("\nWHAT BINDS EACH CLASS'S LOWER EDGE\n");
        for class in ParameterClass::ALL {
            report.push_str(&format!(
                "  {:<20} {}\n",
                class.tag(),
                binding_param.get(class.tag()).map_or("-", String::as_str),
            ));
        }

        // FLAG, not a silent narrowing: a class whose slowest member is orders of magnitude
        // below its own median cannot be gated by one epsilon without either failing that
        // member or admitting a nearly-frozen encoder. Reported here and carried into the
        // SUMMARY with its numbers; widening the fixture or the matrix is 03-06's call.
        report.push_str("\nFLAGS\n");
        let mut flagged = 0_usize;
        for class in ParameterClass::ALL {
            let best_real = real_min_across.get(class.tag()).copied().unwrap_or(0.0);
            let worst_median = median_max_across.get(class.tag()).copied().unwrap_or(0.0);
            if best_real > 0.0 && worst_median / best_real > 100.0 {
                flagged += 1;
                report.push_str(&format!(
                    "  WIDE-SPREAD {}: median {:.3e} is {:.1e}x its own class minimum {:.3e}; \
                     a single per-class epsilon at min/10 = {:.3e} sits close to f32 \
                     resolution\n",
                    class.tag(),
                    worst_median,
                    worst_median / best_real,
                    best_real,
                    best_real / 10.0,
                ));
            }
        }
        // The flag that actually tests the claim `WIDE-SPREAD` makes in prose. `median/min` is a
        // spread statistic; it can be large for a class whose slowest member still moves far
        // above rounding noise, and small for one that does not move at all. THIS is the
        // freeze blocker: an epsilon at or under the rounding-noise floor cannot reject any
        // parameter that moved by even one ULP, so the class's gate would be a restatement of
        // the strict `||dTheta|| > 0` predicate wearing a threshold's name.
        for class in ParameterClass::ALL {
            let best_real = real_min_across.get(class.tag()).copied().unwrap_or(0.0);
            let noise_floor = noise_floor_across.get(class.tag()).copied().unwrap_or(0.0);
            let eps = best_real / 10.0;
            if noise_floor > 0.0 && eps <= noise_floor {
                flagged += 1;
                report.push_str(&format!(
                    "  EPS-BELOW-NOISE {}: the 10x-margin epsilon {:.3e} is at or under the \
                     f32 rounding-noise floor {:.3e} (ratio {:.2e}); this class cannot be \
                     frozen at this width\n",
                    class.tag(),
                    eps,
                    noise_floor,
                    eps / noise_floor,
                ));
            }
        }
        // Does the strict `||dTheta|| > 0` predicate DISCRIMINATE for this class? If a
        // 1e-8 run — 2000x below the reference rate, not meaningfully training — still moves
        // every member of the class, then `moved()` is true for reasons that have nothing to
        // do with tuning and arming it for that class asserts something that cannot fail.
        for class in ParameterClass::ALL {
            if near_null_moved_all.get(class.tag()).copied().unwrap_or(false) {
                flagged += 1;
                report.push_str(&format!(
                    "  MOVED-ALONE-INSUFFICIENT {}: every member still satisfies \
                     ||dTheta|| > 0 under the 1e-8 near-null control (worst relative delta \
                     {:.3e}), so the strict predicate ALONE does not separate training from \
                     not-training for this class -- the class's epsilon is what does, and \
                     this is the measurement that shows the epsilon is not decoration\n",
                    class.tag(),
                    near_null_max_across.get(class.tag()).copied().unwrap_or(0.0),
                ));
            }
        }
        if flagged == 0 {
            report.push_str("  none\n");
        }

        assert!(cells_run >= 12, "the matrix must run >= 12 passes, ran {cells_run}");

        // The report goes to stdout AND to a file. The file is not belt-and-braces: a
        // summarizing wrapper around `cargo test` (this repo ships one) drops `--nocapture`
        // output entirely, and a calibration whose numbers cannot be read is a calibration
        // that did not happen. `target/` is NOT a safe destination — `.cargo/config.toml`
        // redirects the target directory and it may not exist relative to the test's cwd,
        // which is how the first run of this test failed.
        println!("{report}");
        let destination = std::env::var("SETFIT_CALIBRATION_REPORT").map_or_else(
            |_| std::env::temp_dir().join("setfit-calibration-matrix.txt"),
            std::path::PathBuf::from,
        );
        std::fs::write(&destination, &report).unwrap_or_else(|e| {
            panic!("the calibration report must be writable at {destination:?}: {e}")
        });
        println!("calibration report written to {}", destination.display());
    }
}
