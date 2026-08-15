//! The `setfit-apr-v1` WRITER: one pure deterministic function from a complete
//! artifact view to APR v2 bytes.
//!
//! Contract: `contracts/setfit-apr-v1.yaml` (APR-01). Everything this module
//! encodes — the storage map, the normative `SetFitArtifactDoc` field list, the
//! canonical tensor-name table, the four-path nullable allowlist over five
//! walked sub-documents, the six synthetic probe inputs — is READ FROM that
//! contract rather than restated here in a second, subtly different form. A
//! schema restated in three places is a schema that disagrees with itself.
//!
//! # What the writer guarantees
//!
//! `write_setfit_apr(view)` is a function of `view` and NOTHING ELSE: no clock,
//! no environment, no host name, no filesystem lookup, no random state. Two
//! calls on the same view — in the same process or in two different ones —
//! produce byte-identical output with an identical SHA-256. That is not a
//! nicety: the artifact hash is the identity every Phase 4 response carries, and
//! the codec's byte-canonical closure obligation (`serialize(deserialize(bytes))
//! == bytes`) fails outright the moment any writer input is not derivable from
//! the view.
//!
//! # Why the head is TENSORS and not metadata (review B1)
//!
//! `setfit.head.weight` and `setfit.head.bias` are first-class named F32 tensors.
//! A `K*d` float array inside the JSON document would be lossy at the text
//! boundary, unaligned, invisible to `apr tensors` / `apr diff` / `apr qa`, and
//! would push the metadata section toward the container's 16 MiB
//! `MAX_METADATA_SIZE`. Because they are tensors, the architecture-derived
//! tensor-set rule can REFUSE an artifact that is missing either one.
//!
//! # Why exactly ONE custom metadata key (Pitfall 1)
//!
//! `AprV2Metadata.custom` is a `HashMap<String, Value>` with `#[serde(flatten)]`
//! (header_impl.rs:297-299) and `HashMap` iteration order is unspecified. With N
//! top-level custom keys the serialized JSON key order varies per process, so the
//! container checksum over it is not reproducible and the closure check fails
//! intermittently — the worst kind of red. ONE key holding one
//! `serde_json::Map` is reproducible, because `serde_json::Map` is BTreeMap-backed
//! and no workspace crate enables `preserve_order`.
//!
//! # Why `created_at` stays `None` (Pitfall 2)
//!
//! A timestamp would make two artifacts of the same run differ, breaking both the
//! closure equation and the artifact-hash identity. The container's own
//! `license` / `data_source` / `data_license` fields serialize as explicit
//! `null` BY DESIGN (no `skip_serializing_if`, required by FALSIFY-SHIP-022);
//! that is deterministic, expected, and deliberately OUTSIDE the null walk's
//! scope — see [`WALKED_SUBDOCUMENTS`].

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::{Map as JsonMap, Value};

use super::tokenizer::sha256_hex;
use super::{EncoderArchitecture, SetFitMiniLm};
use crate::classification::MultinomialLogisticRegression;
use crate::format::v2::{AprV2Metadata, AprV2Writer, TensorDType};

// ===========================================================================
// Contract-resident constants
// ===========================================================================

/// The schema identifier written into the doc's first-level `schema` field.
pub const ARTIFACT_SCHEMA: &str = "setfit-apr-v1";

/// The schema version written into the doc's first-level `schema_version` field.
pub const ARTIFACT_SCHEMA_VERSION: u32 = 1;

/// The ONLY typed container metadata key this writer sets (D-02(a) amendment,
/// D-04 detection). A SetFit-shaped tensor set without this tag is a plain APR.
pub const MODEL_TYPE_TAG: &str = "setfit";

/// The ONE custom metadata key — see the module docs for why N keys are unsafe.
pub const CUSTOM_METADATA_KEY: &str = "setfit";

/// Canonical name of the classifier weight tensor, `[num_labels, n_features]`.
pub const HEAD_WEIGHT_TENSOR: &str = "setfit.head.weight";

/// Canonical name of the classifier intercept tensor, `[num_labels]`.
pub const HEAD_BIAS_TENSOR: &str = "setfit.head.bias";

/// Canonical name of the raw `tokenizer.json` payload, dtype U8.
pub const TOKENIZER_BLOB_TENSOR: &str = "tokenizer.blob";

/// The normative `SetFitArtifactDoc` field list, in declaration order.
///
/// Read from `contracts/setfit-apr-v1.yaml` equation `artifact_doc_schema`
/// (review B2). A field missing from this list is a finding, not a
/// simplification; the doc-key-set test compares against this literal so an
/// added or renamed field is a LOUD failure rather than a silent widening.
pub const SETFIT_ARTIFACT_DOC_FIELDS: [&str; 16] = [
    "schema",
    "schema_version",
    "bundle_schema_version",
    "format_id",
    "architecture",
    "tokenizer_sha256",
    "preprocessing",
    "root_seed",
    "head",
    "ordered_labels",
    "requested_config",
    "resolved_config",
    "evidence",
    "provenance",
    "hf_name_map",
    "probes",
];

/// The FIVE embedded sub-documents the null walk visits.
///
/// # The walk is FIVE and the allowlist is FOUR, deliberately
///
/// `resolved_config` (`ResolvedConfigRecord`, one `String`) and `provenance`
/// (`ProvenanceRecord`, four `String` + one `u64` + one `u32`) contribute ZERO
/// allowlisted paths today, and they are WALKED ANYWAY. That is not redundancy;
/// it is the whole point. An UNWALKED subtree cannot reject anything, so a
/// future `Option` field added to either type would fail SILENTLY on the first
/// production artifact instead of loudly at this guard — the exact inversion of
/// the fail-loud asymmetry the allowlist design rests on (checker warning W-A).
///
/// The second half of the same guarantee lives in `aprender-train` (plan 04-13),
/// the only crate that can NAME all five types: its completeness gate asserts
/// that serializing an all-`None` instance of each produces exactly
/// [`NULLABLE_PATH_ALLOWLIST`], with `resolved_config` and `provenance` each
/// asserted BY NAME to contribute an empty set.
pub const WALKED_SUBDOCUMENTS: [&str; 5] = [
    "architecture",
    "requested_config",
    "resolved_config",
    "evidence",
    "provenance",
];

/// The four paths at which a `null` is LEGITIMATE, read from the contract.
///
/// # The derivation rule
///
/// This is exactly the set of paths at which an `Option`-typed field of the FIVE
/// [`WALKED_SUBDOCUMENTS`] can serialize. None of those five types carries
/// `skip_serializing_if`, so every `None` emits an EXPLICIT `null` the walk can
/// see. The list is NOT hand-picked, and no path may join it without the same
/// written analysis the contract requires: owning type, `Option<...>` field
/// type, source location, whether `None` is the production shape, and whether
/// the `Option` wraps a float.
///
/// | path | owning type | `Option` field type |
/// |------|-------------|---------------------|
/// | `architecture.vocab_remap` | `EncoderArchitecture` | `Option<Vec<u32>>` |
/// | `requested_config.pair_config.budget` | `PairConfigWire` | `Option<u64>` |
/// | `requested_config.pair_config.hard_cap` | `PairConfigWire` | `Option<u64>` |
/// | `evidence.epsilon_used` | `EvidenceSummary` | `Option<f64>` |
///
/// # Why this is an allowlisted NULL SCAN and not an enumerated float-path check
///
/// `serde_json` maps EVERY non-finite `f64` to `null` SILENTLY — `+inf`, `-inf`
/// and every `NaN` payload render identically — so a stray `null` IS the CR-03
/// signature. `UpdateEvidence::to_canonical_bytes` (evidence.rs:456-483) already
/// records, in its own words, why a scan beats a field list: "An enumerated field
/// list is the guard that silently stops covering the newest field, which is how
/// these checks rot." That guard's blanket form is EXACT only because
/// `UpdateEvidence` has no `Option` field. THREE of the five sub-documents here
/// DO have `Option` fields, so a blanket scan would refuse honest artifacts. This
/// allowlist is the minimum widening that keeps the guard armed.
///
/// # The residual, recorded rather than claimed away
///
/// At the three `Option<non-float>` paths a `null` is UNAMBIGUOUS: no `f64` can
/// produce a `null` at a `Vec<u32>` or `u64` field. At `evidence.epsilon_used`,
/// which is `Option<f64>`, `None` and a non-finite epsilon render IDENTICALLY and
/// this writer cannot separate them. Today that residual is EMPTY —
/// `epsilon_used` has exactly one assignment in the tree and it is `None`
/// (evidence.rs:655, asserted at evidence.rs:1144).
///
/// # `skip_serializing_if` on any of the five types is FORBIDDEN
///
/// It would change `SetFitBundle::to_canonical_bytes` output and break Phase 3's
/// committed closure tests, AND it would silently empty this allowlist, turning a
/// guarded schema into an unguarded one with no test turning red. The fix for a
/// new nullable field is a new allowlist entry plus 04-13's completeness gate.
pub const NULLABLE_PATH_ALLOWLIST: [&str; 4] = [
    // `EncoderArchitecture::vocab_remap: Option<Vec<u32>>`
    // (aprender-core/src/setfit/mod.rs:126-127). `None` is the FULL PIN — the
    // PRODUCTION shape (import.rs:501); `Some` occurs only on the slice-fixture
    // path (import.rs:620). An allowlist omitting this path would refuse EVERY
    // pinned MiniLM artifact while every slice-fixture test stayed green.
    "architecture.vocab_remap",
    // `PairConfigWire::budget: Option<u64>`
    // (aprender-train/src/train/setfit/config.rs:716).
    "requested_config.pair_config.budget",
    // `PairConfigWire::hard_cap: Option<u64>`
    // (aprender-train/src/train/setfit/config.rs:717).
    "requested_config.pair_config.hard_cap",
    // `EvidenceSummary::epsilon_used: Option<f64>`
    // (aprender-train/src/train/setfit/evidence.rs:599). The ONE allowlisted
    // path whose `Option` wraps a float — see the residual note above.
    "evidence.epsilon_used",
];

/// Number of contract-resident synthetic probes.
pub const PROBE_COUNT: usize = 6;

/// The unit `probe_truncation_boundary` repeats (note the trailing space).
pub const PROBE_TRUNCATION_REPEAT_UNIT: &str = "few shot classification with contrastive pairs ";

/// How many times [`PROBE_TRUNCATION_REPEAT_UNIT`] repeats, with no separator.
pub const PROBE_TRUNCATION_REPEAT_COUNT: usize = 64;

/// The contract's probe identifiers, in probe order.
pub const PROBE_IDS: [&str; PROBE_COUNT] = [
    "probe_ascii_pangram",
    "probe_unicode",
    "probe_truncation_boundary",
    "probe_minimal",
    "probe_social",
    "probe_whitespace",
];

/// The six FIXED, SYNTHETIC, contract-resident probe inputs, in probe order.
///
/// They are NEVER sampled from the dataset or the selection (Pitfall 8): the
/// train-time `VerifyProbe` rows come from the selection, and reusing that source
/// would embed TweetEval text in every shipped artifact — colliding with DATA-01's
/// no-vendored-corpus posture AND making probes dataset-dependent, hence useless
/// for Phase 5 cross-cell comparison.
///
/// The six cover six DIFFERENT failure shapes, not six samples of one: ASCII
/// baseline, multi-byte UTF-8, the truncation boundary, a minimal input,
/// social-media punctuation, and embedded control whitespace.
#[must_use]
pub fn probe_inputs() -> Vec<String> {
    vec![
        "the quick brown fox jumps over the lazy dog".to_string(),
        "El rapido zorro marron salta sobre el perro perezoso — naive cafe, pi = 3.14159"
            .to_string(),
        PROBE_TRUNCATION_REPEAT_UNIT.repeat(PROBE_TRUNCATION_REPEAT_COUNT),
        "ok".to_string(),
        "Stance detection: I firmly support this position!!! #debate @user123 https://example.com"
            .to_string(),
        "line one\nline two\ttabbed   spaced".to_string(),
    ]
}

// ---------------------------------------------------------------------------
// The canonical tensor-name table (contract equation `canonical_tensor_names`)
// ---------------------------------------------------------------------------

/// The five global `(hf_name, canonical_name)` pairs.
///
/// D-01 AMENDMENT (recorded in-contract, not silent): four of these five have NO
/// canonical form in `tensor-names-v1` — `token_type_embeddings` and both
/// `embeddings.LayerNorm` leaves have no role at all, and `position_embedding`
/// has a `bert:` alias whose `_fallback` list is EMPTY. `setfit-apr-v1` RESERVES
/// their names, following the same global convention so generic tooling sees one
/// coherent scheme, and the non-collision against `tensor-names-v1`'s complete
/// `_fallback` set was enumerated and checked rather than asserted.
const GLOBAL_NAME_TABLE: [(&str, &str); 5] = [
    // tensor-names-v1 global_roles.embedding _fallback
    ("embeddings.word_embeddings.weight", "token_embd.weight"),
    // SETFIT-SCHEMA-OWNED: position_embedding's _fallback is empty
    (
        "embeddings.position_embeddings.weight",
        "position_embd.weight",
    ),
    // SETFIT-SCHEMA-OWNED: no tensor-names-v1 role
    (
        "embeddings.token_type_embeddings.weight",
        "token_types.weight",
    ),
    // SETFIT-SCHEMA-OWNED: no tensor-names-v1 role
    ("embeddings.LayerNorm.weight", "token_embd_norm.weight"),
    // SETFIT-SCHEMA-OWNED: no tensor-names-v1 role
    ("embeddings.LayerNorm.bias", "token_embd_norm.bias"),
];

/// The sixteen per-layer `(hf_template, canonical_template)` pairs.
///
/// `{n}` is expanded over `architecture.num_layers` — the expected set is a
/// FUNCTION of the architecture, never a hardcoded six-layer list. That is what
/// makes the identical rule apply to the pinned model and to a reduced fixture.
///
/// Three of the sixteen are SETFIT-SCHEMA-OWNED under the same D-01 amendment:
/// `tensor-names-v1` has no `o_proj_bias`, `ffn_up_bias` or `ffn_down_bias` role.
const LAYER_NAME_TABLE: [(&str, &str); 16] = [
    (
        "encoder.layer.{n}.attention.self.query.weight",
        "blk.{n}.attn_q.weight",
    ),
    (
        "encoder.layer.{n}.attention.self.query.bias",
        "blk.{n}.attn_q.bias",
    ),
    (
        "encoder.layer.{n}.attention.self.key.weight",
        "blk.{n}.attn_k.weight",
    ),
    (
        "encoder.layer.{n}.attention.self.key.bias",
        "blk.{n}.attn_k.bias",
    ),
    (
        "encoder.layer.{n}.attention.self.value.weight",
        "blk.{n}.attn_v.weight",
    ),
    (
        "encoder.layer.{n}.attention.self.value.bias",
        "blk.{n}.attn_v.bias",
    ),
    (
        "encoder.layer.{n}.attention.output.dense.weight",
        "blk.{n}.attn_output.weight",
    ),
    // SETFIT-SCHEMA-OWNED: there is no o_proj_bias role
    (
        "encoder.layer.{n}.attention.output.dense.bias",
        "blk.{n}.attn_output.bias",
    ),
    (
        "encoder.layer.{n}.attention.output.LayerNorm.weight",
        "blk.{n}.attn_norm.weight",
    ),
    (
        "encoder.layer.{n}.attention.output.LayerNorm.bias",
        "blk.{n}.attn_norm.bias",
    ),
    (
        "encoder.layer.{n}.intermediate.dense.weight",
        "blk.{n}.ffn_up.weight",
    ),
    // SETFIT-SCHEMA-OWNED: there is no ffn_up_bias role
    (
        "encoder.layer.{n}.intermediate.dense.bias",
        "blk.{n}.ffn_up.bias",
    ),
    (
        "encoder.layer.{n}.output.dense.weight",
        "blk.{n}.ffn_down.weight",
    ),
    // SETFIT-SCHEMA-OWNED: there is no ffn_down_bias role
    (
        "encoder.layer.{n}.output.dense.bias",
        "blk.{n}.ffn_down.bias",
    ),
    (
        "encoder.layer.{n}.output.LayerNorm.weight",
        "blk.{n}.ffn_norm.weight",
    ),
    (
        "encoder.layer.{n}.output.LayerNorm.bias",
        "blk.{n}.ffn_norm.bias",
    ),
];

// ===========================================================================
// The view: the writer's complete input
// ===========================================================================

/// Everything `write_setfit_apr` is allowed to know.
///
/// The field list mirrors `SetFitBundle`'s TWENTY fields 1:1 so plan 04-05's
/// codec can populate it without inventing anything (review B3). Everything in
/// the artifact MUST be derivable from this value — no clock, no environment, no
/// host name, no lookup — because anything else breaks the closure equation.
///
/// # `architecture` is the TYPED record, and the sub-document is derived from it
///
/// The contract's forward bijection row is `architecture = to_value(bundle.architecture)`
/// — a FUNCTION of the typed value. Carrying both the typed record and a
/// separately-supplied `serde_json::Value` copy would be transporting one fact
/// twice, and two copies of one fact are two values that can disagree: the codec
/// would have to keep them in sync, and a divergence would break the bijection
/// with nothing turning red. So the view carries the typed record and the writer
/// computes `serde_json::to_value` itself. A pleasant consequence: the ONLY
/// `null` the `architecture` subtree can emit is the allowlisted
/// `vocab_remap`, because every other field is non-`Option`. The subtree is
/// walked anyway, so a future `Option` there is caught — see
/// [`WALKED_SUBDOCUMENTS`].
#[derive(Debug, Clone)]
pub struct SetFitArtifactView {
    /// The `SetFitBundle` wire version this artifact is written from.
    pub bundle_schema_version: u32,
    /// The codec identifier that wrote the payload.
    pub format_id: String,
    /// The encoder architecture record — typed, so the rebuild can use it.
    pub architecture: EncoderArchitecture,
    /// The exact `tokenizer.json` bytes, byte-identical to upstream.
    pub tokenizer_bytes: Vec<u8>,
    /// The pooling policy identifier the encoder applied.
    pub pooling: String,
    /// The normalization policy identifier the encoder applied.
    pub normalization: String,
    /// The epsilon the L2 normalization clamped with.
    pub l2_epsilon: f32,
    /// The tokenizer's truncation bound.
    pub truncation_max_sequence_length: u32,
    /// The tokenizer's padding mode.
    pub padding_mode: String,
    /// The run's requested max sequence length.
    pub max_length: u32,
    /// The root seed every dropout stream derives from.
    pub root_seed: u64,
    /// Every named encoder tensor, keyed by HF dotted name, `(shape, data)`.
    pub tensors: BTreeMap<String, (Vec<usize>, Vec<f32>)>,
    /// The head's `K * d` weights, row-major; row `i` belongs to `ordered_labels[i]`.
    pub head_weights: Vec<f32>,
    /// The head's `K` intercepts.
    pub head_intercepts: Vec<f32>,
    /// The head's fitted feature dimension.
    pub head_n_features: usize,
    /// The ordered labels the head's weight rows are indexed by.
    pub ordered_labels: Vec<String>,
    /// `to_value(SetFitTrainConfig)` — opaque here; `aprender-core` cannot name it.
    pub requested_config: Value,
    /// `to_value(ResolvedConfigRecord)` — opaque here.
    pub resolved_config: Value,
    /// `to_value(EvidenceSummary)` — opaque here.
    pub evidence: Value,
    /// `to_value(ProvenanceRecord)` — opaque here (bundle field 20, plan 04-13).
    pub provenance: Value,
}

// ===========================================================================
// The typed failure
// ===========================================================================

/// A `setfit-apr-v1` write (and, from 04-03, load) failure.
///
/// `Debug + Clone + PartialEq` is REQUIRED, not incidental: plan 04-05 embeds
/// this type inside `CodecError`, which derives exactly those three. A variant
/// carrying a non-`Clone` payload would make the typed-source mapping impossible
/// and force the stringification review B3 flagged.
///
/// `#[non_exhaustive]`: 04-03 adds the loader's rungs to this same enum, so a
/// downstream `match` must already be written to tolerate new variants.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SetFitArtifactError {
    /// A supplied HF tensor name has no entry in the architecture-derived map.
    UnmappedTensorName {
        /// The HF dotted name with no canonical form.
        hf_name: String,
    },

    /// The view is missing tensors the architecture-derived set requires.
    ///
    /// Names the MISSING tensors, not merely the two counts: "104 != 103" is not
    /// a diagnosis.
    IncompleteTensorSet {
        /// The expected HF dotted names that the view does not carry, sorted.
        missing: Vec<String>,
    },

    /// The view's parts contradict one another (shape vs element count, head
    /// arity vs label count, head feature dimension vs encoder width, a
    /// canonical-name collision).
    InconsistentTensorSet {
        /// What disagreed with what.
        reason: String,
    },

    /// A `NaN`/`±Inf` float, or a `null` at a path outside
    /// [`NULLABLE_PATH_ALLOWLIST`], was found BEFORE serialization.
    ///
    /// `path` names the exact dotted path of the FIRST offender in document
    /// order, so the diagnosis does not require reading the contract.
    NonFiniteValue {
        /// Dotted path of the offending value.
        path: String,
    },

    /// The tokenizer bytes do not hash to the digest the architecture records.
    ///
    /// An artifact whose tokenizer does not match its encoder produces
    /// confidently wrong embeddings for every input while looking structurally
    /// valid the whole time.
    TokenizerHashMismatch {
        /// The digest the architecture record claims.
        expected: String,
        /// The digest of the bytes actually supplied.
        got: String,
    },

    /// The APR v2 container refused the write.
    ContainerWrite {
        /// The container's own diagnostic.
        reason: String,
    },

    /// A probe could not be computed: the rebuild, the head or the encode failed.
    ProbeComputation {
        /// Which probe (or `<rebuild>` / `<head>` for the shared setup).
        probe: String,
        /// The underlying diagnostic, forwarded rather than flattened.
        reason: String,
    },
}

impl std::fmt::Display for SetFitArtifactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnmappedTensorName { hf_name } => write!(
                f,
                "SetFitArtifactError::UnmappedTensorName({hf_name} has no canonical form in the \
                 architecture-derived name map)"
            ),
            Self::IncompleteTensorSet { missing } => write!(
                f,
                "SetFitArtifactError::IncompleteTensorSet(missing {} tensor(s): {})",
                missing.len(),
                missing.join(", ")
            ),
            Self::InconsistentTensorSet { reason } => {
                write!(f, "SetFitArtifactError::InconsistentTensorSet({reason})")
            }
            Self::NonFiniteValue { path } => write!(
                f,
                "SetFitArtifactError::NonFiniteValue(at {path}; a null outside the four-path \
                 allowlist is a silently destroyed non-finite value)"
            ),
            Self::TokenizerHashMismatch { expected, got } => write!(
                f,
                "SetFitArtifactError::TokenizerHashMismatch(expected {expected}, got {got})"
            ),
            Self::ContainerWrite { reason } => {
                write!(f, "SetFitArtifactError::ContainerWrite({reason})")
            }
            Self::ProbeComputation { probe, reason } => {
                write!(
                    f,
                    "SetFitArtifactError::ProbeComputation({probe}: {reason})"
                )
            }
        }
    }
}

impl std::error::Error for SetFitArtifactError {}

// ===========================================================================
// Public API
// ===========================================================================

/// Lowercase-hex SHA-256 of artifact bytes.
///
/// A TRUSTED free function in core: the codec must never hash its own output
/// (verify.rs:198-205 discipline), so the hash a run records and the hash a
/// consumer recomputes come from ONE implementation. It delegates to the same
/// `sha256_hex` the tokenizer digest uses, so there is exactly one hashing path
/// in this crate.
#[must_use]
pub fn artifact_sha256_hex(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}

/// Write a complete view as `setfit-apr-v1` bytes.
///
/// # Order is load-bearing
///
/// 1. derive the architecture-expected canonical set and validate the view's
///    tensor names map onto it 1:1;
/// 2. scan every `f32` the view carries (tensors, head weights, head intercepts);
/// 3. check the tokenizer bytes hash to the digest the architecture records —
///    BEFORE the rebuild, so a probe mismatch can never be a mis-paired
///    tokenizer wearing a math-divergence diagnosis;
/// 4. compute the six probes from the view's OWN parts;
/// 5. build the one-key document;
/// 6. walk all FIVE embedded sub-documents for `null`s outside the allowlist;
/// 7. hand the tensors and the document to the container.
///
/// Nothing is written until every rung has passed: there is no partial artifact.
///
/// # Errors
///
/// [`SetFitArtifactError`], each variant naming the specific tensor, path, probe
/// or digest that failed.
pub fn write_setfit_apr(view: &SetFitArtifactView) -> Result<Vec<u8>, SetFitArtifactError> {
    // (1) NAMES. The expected set is a FUNCTION of the view's own declared
    //     architecture, so the identical rule judges the pinned model and a
    //     reduced fixture.
    let hf_name_map = build_hf_name_map(view.architecture.num_layers);
    validate_view_structure(view, &hf_name_map)?;

    // (2) EVERY f32 THE VIEW CARRIES, before any of it can reach a payload.
    scan_view_floats(view)?;

    // (3) TOKENIZER IDENTITY, BEFORE THE REBUILD. An encoder rebuilt with a
    //     substituted tokenizer produces confidently wrong embeddings for every
    //     input and looks structurally valid the whole time — so a probe
    //     mismatch must never be able to arrive wearing a math-divergence
    //     diagnosis when it is really a mis-paired tokenizer.
    let observed = sha256_hex(&view.tokenizer_bytes);
    if observed != view.architecture.tokenizer_sha256 {
        return Err(SetFitArtifactError::TokenizerHashMismatch {
            expected: view.architecture.tokenizer_sha256.clone(),
            got: observed,
        });
    }

    // (4) PROBES, computed from the view's OWN parts — never carried, which is
    //     what makes them closure-safe (a carried probe set would be a 21st
    //     bundle field with no source).
    let probes = compute_probes(view)?;

    // (5) THE ONE DOCUMENT.
    let doc = build_artifact_doc(view, &hf_name_map, &probes)?;

    // (6) THE FIVE-SUBDOCUMENT NULL WALK. Runs at WRITE time and not only as a
    //     byte comparison: an allowlisted `null` round-trips to `None` and back
    //     to `null`, so closure HOLDS while the value is GONE.
    guard_subdocument_nulls(&doc)?;

    // (7) THE CONTAINER. Nothing has been written until here, so a refusal at
    //     any rung above leaves no partial artifact.
    write_container(view, &hf_name_map, doc)
}

/// Rung 1: the view's tensor names map 1:1 onto the architecture-derived set,
/// and its parts do not contradict one another.
fn validate_view_structure(
    view: &SetFitArtifactView,
    hf_name_map: &BTreeMap<String, String>,
) -> Result<(), SetFitArtifactError> {
    // An HF name with no canonical entry is a typed error, NEVER a silently
    // dropped tensor.
    for hf in view.tensors.keys() {
        if !hf_name_map.contains_key(hf) {
            return Err(SetFitArtifactError::UnmappedTensorName {
                hf_name: hf.clone(),
            });
        }
    }
    let missing: Vec<String> = hf_name_map
        .keys()
        .filter(|expected| !view.tensors.contains_key(*expected))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(SetFitArtifactError::IncompleteTensorSet { missing });
    }
    // Injectivity: two HF names resolving to one canonical name would OVERWRITE
    // a tensor in the container's index rather than fail.
    let canonical: BTreeSet<&String> = hf_name_map.values().collect();
    if canonical.len() != hf_name_map.len() {
        return Err(SetFitArtifactError::InconsistentTensorSet {
            reason: format!(
                "the canonical name map is not injective: {} HF names resolve to {} canonical names",
                hf_name_map.len(),
                canonical.len()
            ),
        });
    }
    // The structural per-entry rule, applied to the DECLARED shape before any
    // payload is written.
    for (hf, (shape, data)) in &view.tensors {
        if shape.is_empty() {
            return Err(SetFitArtifactError::InconsistentTensorSet {
                reason: format!("{hf}: a tensor with no declared shape is not writable"),
            });
        }
        let elements: usize = shape.iter().product();
        if elements != data.len() {
            return Err(SetFitArtifactError::InconsistentTensorSet {
                reason: format!(
                    "{hf}: shape {shape:?} implies {elements} elements but {} were supplied",
                    data.len()
                ),
            });
        }
    }

    let num_labels = view.ordered_labels.len();
    if num_labels < 2 {
        return Err(SetFitArtifactError::InconsistentTensorSet {
            reason: format!("a classifier head needs at least two labels, got {num_labels}"),
        });
    }
    // The head must be able to CONSUME this encoder's embedding. A head fitted
    // at a different width would refuse every probe at replay time, in a fresh
    // process, long after the artifact shipped.
    if view.head_n_features != view.architecture.hidden {
        return Err(SetFitArtifactError::InconsistentTensorSet {
            reason: format!(
                "head_n_features {} does not match the encoder's hidden width {}",
                view.head_n_features, view.architecture.hidden
            ),
        });
    }
    let expected_weights = num_labels.saturating_mul(view.head_n_features);
    if view.head_weights.len() != expected_weights {
        return Err(SetFitArtifactError::InconsistentTensorSet {
            reason: format!(
                "setfit.head.weight declares [{num_labels}, {}] = {expected_weights} values but {} were supplied",
                view.head_n_features,
                view.head_weights.len()
            ),
        });
    }
    if view.head_intercepts.len() != num_labels {
        return Err(SetFitArtifactError::InconsistentTensorSet {
            reason: format!(
                "setfit.head.bias declares [{num_labels}] values but {} were supplied",
                view.head_intercepts.len()
            ),
        });
    }
    Ok(())
}

/// Rung 2: every `f32` the view carries is finite, named by its exact path.
fn scan_view_floats(view: &SetFitArtifactView) -> Result<(), SetFitArtifactError> {
    for (hf, (_, data)) in &view.tensors {
        for (index, value) in data.iter().enumerate() {
            if !value.is_finite() {
                return Err(SetFitArtifactError::NonFiniteValue {
                    path: format!("tensors.{hf}[{index}]"),
                });
            }
        }
    }
    for (array, values) in [
        ("head_weights", &view.head_weights),
        ("head_intercepts", &view.head_intercepts),
    ] {
        for (index, value) in values.iter().enumerate() {
            if !value.is_finite() {
                return Err(SetFitArtifactError::NonFiniteValue {
                    path: format!("{array}[{index}]"),
                });
            }
        }
    }
    if !view.l2_epsilon.is_finite() {
        return Err(SetFitArtifactError::NonFiniteValue {
            path: "preprocessing.l2_epsilon".to_string(),
        });
    }
    Ok(())
}

/// Rung 4: the six contract-resident probes, replayed through a model rebuilt
/// from the view's own parts.
///
/// The tensor map is CLONED because `from_bundle_parts` takes ownership and
/// drains it (encoder.rs:408-419, where taking by value is what avoids a
/// per-tensor copy on the reload path), while this writer only borrows the view.
/// That is a real cost on a full pin and it is the right trade: computing the
/// probes from anything other than the view's OWN tensors would record
/// expectations for a model the artifact does not contain.
fn compute_probes(view: &SetFitArtifactView) -> Result<Vec<Value>, SetFitArtifactError> {
    let model = SetFitMiniLm::from_bundle_parts(
        &view.tokenizer_bytes,
        &view.architecture,
        view.tensors.clone(),
        view.root_seed,
    )
    .map_err(|e| SetFitArtifactError::ProbeComputation {
        probe: "<rebuild>".to_string(),
        reason: e.to_string(),
    })?;
    let head = MultinomialLogisticRegression::from_stored_coefficients(
        view.ordered_labels.clone(),
        view.head_n_features,
        view.head_weights.clone(),
        view.head_intercepts.clone(),
    )
    .map_err(|e| SetFitArtifactError::ProbeComputation {
        probe: "<head>".to_string(),
        reason: e.to_string(),
    })?;

    let d = view.head_n_features;
    let k = view.ordered_labels.len();
    let mut records = Vec::with_capacity(PROBE_COUNT);

    for (index, input) in probe_inputs().into_iter().enumerate() {
        let id = PROBE_IDS[index];
        let fail = |reason: String| SetFitArtifactError::ProbeComputation {
            probe: id.to_string(),
            reason,
        };

        let pooled = model
            .encode_texts(&[input.as_str()])
            .map_err(|e| fail(e.to_string()))?;
        if pooled.shape().to_vec() != vec![1, d] {
            return Err(fail(format!(
                "the encode produced shape {:?}, expected [1, {d}]",
                pooled.shape()
            )));
        }
        let embedding: Vec<f32> = pooled.data().to_vec();
        for (position, value) in embedding.iter().enumerate() {
            if !value.is_finite() {
                return Err(SetFitArtifactError::NonFiniteValue {
                    path: format!("probes.{index}.embedding_hex.{position}"),
                });
            }
        }

        // The logits are accumulated in `f64` in the SAME order
        // `predict_proba` uses (multinomial.rs:1174-1185) and then narrowed to
        // `f32` for storage, so the recorded logits and the recorded
        // probabilities cannot describe two different computations. The
        // narrowing is deterministic and the contract's replay tolerance
        // (1.0e-5 absolute) is orders above `f32` epsilon at these magnitudes.
        let mut logits = Vec::with_capacity(k);
        for c in 0..k {
            let mut z = f64::from(view.head_intercepts[c]);
            for j in 0..d {
                z += f64::from(view.head_weights[c * d + j]) * f64::from(embedding[j]);
            }
            logits.push(z as f32);
        }

        let probabilities: Vec<f32> = head
            .predict_proba(&[embedding.clone()])
            .map_err(|e| fail(e.to_string()))?
            .into_iter()
            .next()
            .ok_or_else(|| fail("predict_proba returned no rows".to_string()))?
            .into_iter()
            .map(|p| p as f32)
            .collect();

        for (field, values) in [
            ("logits_hex", &logits),
            ("probabilities_hex", &probabilities),
        ] {
            for (position, value) in values.iter().enumerate() {
                if !value.is_finite() {
                    return Err(SetFitArtifactError::NonFiniteValue {
                        path: format!("probes.{index}.{field}.{position}"),
                    });
                }
            }
        }

        // The label goes through the head's own `predict`, so the tie-break
        // (lowest index on an exact tie) is the house rule and not a second one
        // written here.
        let label = head
            .predict(&[embedding.clone()])
            .map_err(|e| fail(e.to_string()))?
            .into_iter()
            .next()
            .ok_or_else(|| fail("predict returned no rows".to_string()))?;

        let mut record = JsonMap::new();
        record.insert("input".to_string(), Value::String(input));
        record.insert(
            "embedding_hex".to_string(),
            Value::Array(f32_slice_hex(&embedding)),
        );
        record.insert(
            "logits_hex".to_string(),
            Value::Array(f32_slice_hex(&logits)),
        );
        record.insert(
            "probabilities_hex".to_string(),
            Value::Array(f32_slice_hex(&probabilities)),
        );
        record.insert("label".to_string(), Value::String(label));
        records.push(Value::Object(record));
    }
    Ok(records)
}

/// Rung 7: hand the tensors and the ONE document to the APR v2 container.
fn write_container(
    view: &SetFitArtifactView,
    hf_name_map: &BTreeMap<String, String>,
    doc: JsonMap<String, Value>,
) -> Result<Vec<u8>, SetFitArtifactError> {
    let mut custom: HashMap<String, Value> = HashMap::with_capacity(1);
    custom.insert(CUSTOM_METADATA_KEY.to_string(), Value::Object(doc));

    let metadata = AprV2Metadata {
        model_type: MODEL_TYPE_TAG.to_string(),
        // NO TIMESTAMP IS WRITTEN — see the module docs. Written explicitly
        // rather than left to `Default` so the choice is visible at the site
        // that makes it.
        created_at: None,
        custom,
        ..Default::default()
    };

    // `AprV2Writer::new` already sets LAYOUT_ROW_MAJOR (writer.rs:30-39); there
    // is no GGUF import path into this writer, so there is no transpose at this
    // boundary and no column-major kernel may ever be pointed at these tensors.
    let mut writer = AprV2Writer::new(metadata);
    for (hf, (shape, data)) in &view.tensors {
        let canonical =
            hf_name_map
                .get(hf)
                .ok_or_else(|| SetFitArtifactError::UnmappedTensorName {
                    hf_name: hf.clone(),
                })?;
        writer.add_f32_tensor(canonical.clone(), shape.clone(), data);
    }
    let num_labels = view.ordered_labels.len();
    writer.add_f32_tensor(
        HEAD_WEIGHT_TENSOR,
        vec![num_labels, view.head_n_features],
        &view.head_weights,
    );
    writer.add_f32_tensor(HEAD_BIAS_TENSOR, vec![num_labels], &view.head_intercepts);
    // dtype U8 makes the tokenizer's byte-exactness STRUCTURAL: there is no
    // float path it could be rounded through.
    writer.add_tensor(
        TOKENIZER_BLOB_TENSOR,
        TensorDType::U8,
        vec![view.tokenizer_bytes.len()],
        view.tokenizer_bytes.clone(),
    );

    writer
        .write()
        .map_err(|e| SetFitArtifactError::ContainerWrite {
            reason: e.to_string(),
        })
}

// ===========================================================================
// Name derivation
// ===========================================================================

/// The architecture-derived HF -> canonical tensor-name map.
///
/// A FUNCTION of `num_layers`: the sixteen per-layer templates are expanded over
/// `0..num_layers`, so the same rule judges the pinned six-layer model and a
/// reduced fixture. There is no test-only schema exception.
#[must_use]
pub fn build_hf_name_map(num_layers: usize) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (hf, canonical) in GLOBAL_NAME_TABLE {
        map.insert(hf.to_string(), canonical.to_string());
    }
    for n in 0..num_layers {
        let index = n.to_string();
        for (hf, canonical) in LAYER_NAME_TABLE {
            map.insert(hf.replace("{n}", &index), canonical.replace("{n}", &index));
        }
    }
    map
}

/// Every tensor name the container is expected to carry, including the three
/// schema-owned entries. `|expected| = 5 + 16 * num_layers + 3`.
#[must_use]
pub fn expected_container_tensor_names(num_layers: usize) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = build_hf_name_map(num_layers).into_values().collect();
    names.insert(HEAD_WEIGHT_TENSOR.to_string());
    names.insert(HEAD_BIAS_TENSOR.to_string());
    names.insert(TOKENIZER_BLOB_TENSOR.to_string());
    names
}

// ===========================================================================
// Bit-pattern hex (never decimal text)
// ===========================================================================

/// One `f32` as the lowercase hex of its LITTLE-ENDIAN bit pattern.
///
/// Byte order matches `bundle.rs`'s `f32_to_hex` EXACTLY (bundle.rs:699-715,
/// `hex::encode(value.to_bits().to_le_bytes())`), because the contract's forward
/// bijection row is `preprocessing.l2_epsilon_hex = f32_to_hex(bundle.l2_epsilon)`
/// — a different byte order here would make the doc and the bundle disagree about
/// the same number while both looked like "the bit pattern in hex".
///
/// Decimal text is not the identity on `f32`, and a `null` from a non-finite
/// value would be indistinguishable from a missing one; hex keeps every
/// doc-OWNED float exact and off the null path entirely.
fn f32_bits_hex(value: f32) -> String {
    let mut out = String::with_capacity(8);
    for byte in value.to_bits().to_le_bytes() {
        out.push(nibble_hex(byte >> 4));
        out.push(nibble_hex(byte & 0x0F));
    }
    out
}

fn nibble_hex(n: u8) -> char {
    match n {
        0..=9 => char::from(b'0' + n),
        // The caller masks to 4 bits, so 10..=15 is the only other reachable
        // range; the arm is written as a catch-all rather than 10..=15 plus an
        // unreachable arm, because an unreachable arm here would be a panic
        // path in a codec whose contract is that it does not panic.
        _ => char::from(b'a' + (n & 0x0F) - 10),
    }
}

/// A slice of `f32` as an array of bit-pattern hex strings.
fn f32_slice_hex(values: &[f32]) -> Vec<Value> {
    values
        .iter()
        .map(|v| Value::String(f32_bits_hex(*v)))
        .collect()
}

// ===========================================================================
// The null walk (contract equation `nullable_path_allowlist`)
// ===========================================================================

/// Join one path segment onto a prefix.
///
/// The `first_null_path` / `join_path` shape of evidence.rs:199-224, inverted to
/// build TOP-DOWN: that precedent returns only the FIRST offender and assembles
/// the path on the way back up, while this walk must collect EVERY offender so a
/// rejection can be reasoned about and the accept tests can assert the exact
/// observed set.
fn join_path(prefix: &str, segment: &str) -> String {
    if prefix.is_empty() {
        segment.to_string()
    } else {
        format!("{prefix}.{segment}")
    }
}

/// Collect the dotted path of EVERY `Value::Null` under `value`, in document order.
fn collect_null_paths(prefix: &str, value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Null => out.push(prefix.to_string()),
        Value::Object(map) => {
            for (key, child) in map {
                collect_null_paths(&join_path(prefix, key), child, out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect_null_paths(&join_path(prefix, &index.to_string()), child, out);
            }
        }
        _ => {}
    }
}

/// Every `null` path observed across the FIVE walked sub-documents, allowlisted
/// or not, in document order.
///
/// A missing sub-document contributes nothing rather than panicking: the doc
/// builder is the only producer and it always inserts all five, and the
/// `SETFIT_ARTIFACT_DOC_FIELDS` test is what proves that.
fn observed_null_paths(doc: &JsonMap<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    for name in WALKED_SUBDOCUMENTS {
        if let Some(sub) = doc.get(name) {
            collect_null_paths(name, sub, &mut out);
        }
    }
    out
}

/// The observed `null` paths that are NOT in [`NULLABLE_PATH_ALLOWLIST`].
fn disallowed_null_paths(doc: &JsonMap<String, Value>) -> Vec<String> {
    observed_null_paths(doc)
        .into_iter()
        .filter(|path| !NULLABLE_PATH_ALLOWLIST.contains(&path.as_str()))
        .collect()
}

/// Refuse a document carrying a `null` outside the allowlist.
///
/// The refusal names the FIRST offending path in document order, which is what
/// the contract's postcondition requires. The walk collects all of them so the
/// full set is available to a caller and to the tests; only the error's single
/// `path` field is contract-shaped.
fn guard_subdocument_nulls(doc: &JsonMap<String, Value>) -> Result<(), SetFitArtifactError> {
    if let Some(path) = disallowed_null_paths(doc).into_iter().next() {
        return Err(SetFitArtifactError::NonFiniteValue { path });
    }
    Ok(())
}

// ===========================================================================
// Document construction (stub — filled in the GREEN step)
// ===========================================================================

/// Build the normative `SetFitArtifactDoc` as ONE `serde_json::Map`.
///
/// The insertion order below is the contract's declaration order, for a reader's
/// benefit only: `serde_json::Map` is BTreeMap-backed (no workspace crate enables
/// `preserve_order`), so the SERIALIZED key order is sorted and does not depend
/// on the order of these calls. That is precisely what makes the bytes
/// reproducible.
fn build_artifact_doc(
    view: &SetFitArtifactView,
    hf_name_map: &BTreeMap<String, String>,
    probes: &[Value],
) -> Result<JsonMap<String, Value>, SetFitArtifactError> {
    // The architecture sub-document is DERIVED from the typed record — see
    // `SetFitArtifactView`. One fact, one copy.
    let architecture = serde_json::to_value(&view.architecture).map_err(|e| {
        SetFitArtifactError::InconsistentTensorSet {
            reason: format!("the architecture record does not serialize: {e}"),
        }
    })?;

    let mut preprocessing = JsonMap::new();
    preprocessing.insert("pooling".to_string(), Value::String(view.pooling.clone()));
    preprocessing.insert(
        "normalization".to_string(),
        Value::String(view.normalization.clone()),
    );
    // A bit-pattern hex string, never a JSON number: decimal text is not the
    // identity on f32, and a non-finite value would render as an indistinguishable
    // `null`. Hex keeps every doc-OWNED float exact and off the null path.
    preprocessing.insert(
        "l2_epsilon_hex".to_string(),
        Value::String(f32_bits_hex(view.l2_epsilon)),
    );
    preprocessing.insert(
        "truncation_max_sequence_length".to_string(),
        Value::from(view.truncation_max_sequence_length),
    );
    preprocessing.insert(
        "padding_mode".to_string(),
        Value::String(view.padding_mode.clone()),
    );
    preprocessing.insert("max_length".to_string(), Value::from(view.max_length));

    let mut head = JsonMap::new();
    head.insert("n_features".to_string(), Value::from(view.head_n_features));
    head.insert(
        "num_labels".to_string(),
        Value::from(view.ordered_labels.len()),
    );

    // The map is WRITTEN INTO the artifact so `deserialize` inverts a map the
    // artifact carries rather than re-deriving names from a table that may have
    // moved between the write and the read.
    let mut names = JsonMap::new();
    for (hf, canonical) in hf_name_map {
        names.insert(hf.clone(), Value::String(canonical.clone()));
    }

    let mut doc = JsonMap::new();
    doc.insert(
        "schema".to_string(),
        Value::String(ARTIFACT_SCHEMA.to_string()),
    );
    doc.insert(
        "schema_version".to_string(),
        Value::from(ARTIFACT_SCHEMA_VERSION),
    );
    doc.insert(
        "bundle_schema_version".to_string(),
        Value::from(view.bundle_schema_version),
    );
    doc.insert(
        "format_id".to_string(),
        Value::String(view.format_id.clone()),
    );
    doc.insert("architecture".to_string(), architecture);
    doc.insert(
        "tokenizer_sha256".to_string(),
        Value::String(view.architecture.tokenizer_sha256.clone()),
    );
    doc.insert("preprocessing".to_string(), Value::Object(preprocessing));
    doc.insert("root_seed".to_string(), Value::from(view.root_seed));
    doc.insert("head".to_string(), Value::Object(head));
    doc.insert(
        "ordered_labels".to_string(),
        Value::Array(
            view.ordered_labels
                .iter()
                .map(|label| Value::String(label.clone()))
                .collect(),
        ),
    );
    doc.insert(
        "requested_config".to_string(),
        view.requested_config.clone(),
    );
    doc.insert("resolved_config".to_string(), view.resolved_config.clone());
    doc.insert("evidence".to_string(), view.evidence.clone());
    doc.insert("provenance".to_string(), view.provenance.clone());
    doc.insert("hf_name_map".to_string(), Value::Object(names));
    doc.insert("probes".to_string(), Value::Array(probes.to_vec()));

    debug_assert_eq!(
        doc.len(),
        SETFIT_ARTIFACT_DOC_FIELDS.len(),
        "the doc must carry exactly the contract's field list"
    );
    Ok(doc)
}

// ===========================================================================
// Test fixtures
//
// `mod fixture`, `mod tests`, `mod nullable` and `mod determinism` are DIRECT
// children of `artifact`, not nested inside one `tests` module. That is forced
// by the acceptance criteria's filters: `cargo test` matches on the full test
// path, so `setfit::artifact::nullable` is a substring of
// `setfit::artifact::nullable::foo` but NOT of
// `setfit::artifact::tests::nullable::foo`. A nested layout would make both
// counted filters match ZERO tests — and a filter matching zero tests exits 0,
// which is precisely the CR-02 vacuous pass this phase exists to prevent.
// ===========================================================================

#[cfg(all(test, feature = "setfit"))]
mod fixture {
    //! The tiny fixture, in the PRODUCTION nullability shape.
    //!
    //! Two rules govern everything here, and both come from the contract rather
    //! than from convenience:
    //!
    //! 1. **The NAME TOPOLOGY is exact.** The fixture keeps the same per-layer
    //!    template set as the pinned model at reduced dimensions and layer
    //!    count, and it is judged by the SAME production rule. There is no
    //!    test-only schema exception.
    //! 2. **The NULLABILITY TOPOLOGY is exact too**, and this is the one an
    //!    earlier draft got wrong. [`fixture_view_full_pin_shape`] is the
    //!    DEFAULT because `architecture.vocab_remap: None` is what a pinned
    //!    MiniLM serializes (import.rs:501). The slice fixture sets `Some`
    //!    (import.rs:620), which emits NO null at that path — so a suite built
    //!    only on the slice shape earns every green count on the one input shape
    //!    at which the guard CANNOT fire, and an allowlist that omitted
    //!    `architecture.vocab_remap` would refuse every production artifact while
    //!    every test stayed green. That is the tests-pass/production-fails class
    //!    this phase exists to prevent.

    use super::*;

    use crate::setfit::{
        L2_EPS, MAX_SEQUENCE_LENGTH, NORMALIZATION_POLICY, PADDING_MODE, PINNED_ACTIVATION,
        PINNED_REVISION, POOLING_POLICY,
    };
    use serde_json::json;

    /// Reduced dimensions. `positions` is NOT reduced: the tokenizer truncates at
    /// [`MAX_SEQUENCE_LENGTH`], so `probe_truncation_boundary` produces a
    /// 256-position row and an encoder with fewer position rows would refuse it
    /// with `OversizeInput` before a single probe could be recorded.
    pub(super) const FIXTURE_HIDDEN: usize = 8;
    pub(super) const FIXTURE_HEADS: usize = 2;
    pub(super) const FIXTURE_HEAD_DIM: usize = FIXTURE_HIDDEN / FIXTURE_HEADS;
    pub(super) const FIXTURE_LAYERS: usize = 2;
    pub(super) const FIXTURE_INTERMEDIATE: usize = 16;
    pub(super) const FIXTURE_POSITIONS: usize = MAX_SEQUENCE_LENGTH;
    pub(super) const FIXTURE_TYPE_VOCAB: usize = 2;
    pub(super) const FIXTURE_LABELS: [&str; 3] = ["against", "favor", "neutral"];
    pub(super) const FIXTURE_ROOT_SEED: u64 = 0x0405_0000_0000_0002;
    pub(super) const FIXTURE_BUNDLE_SCHEMA_VERSION: u32 = 1;
    pub(super) const FIXTURE_FORMAT_ID: &str = "setfit-apr-v1-fixture";

    /// A tiny WordPiece vocabulary. Everything outside it becomes `[UNK]`, so
    /// every id the tokenizer can emit is `< TINY_VOCAB.len()` — which is what
    /// lets the fixture carry `vocab_remap: None` (the production shape) with a
    /// 48-row embedding table instead of the pin's 30522.
    pub(super) const TINY_VOCAB: [&str; 48] = [
        "[PAD]",
        "[UNK]",
        "[CLS]",
        "[SEP]",
        "[MASK]",
        "the",
        "quick",
        "brown",
        "fox",
        "jumps",
        "over",
        "lazy",
        "dog",
        "ok",
        "few",
        "shot",
        "classification",
        "with",
        "contrastive",
        "pairs",
        "line",
        "one",
        "two",
        "tabbed",
        "spaced",
        "stance",
        "detection",
        "i",
        "firmly",
        "support",
        "this",
        "position",
        "el",
        "zorro",
        "cafe",
        "naive",
        "pi",
        "##s",
        "##ed",
        ".",
        ",",
        "!",
        "#",
        "@",
        ":",
        "/",
        "-",
        "=",
    ];

    /// A valid, self-contained `tokenizer.json` in the pinned file's exact shape
    /// (BertNormalizer + BertPreTokenizer + TemplateProcessing + WordPiece) with
    /// [`TINY_VOCAB`] substituted for the 30522-entry pin.
    ///
    /// Self-contained ON PURPOSE. Reading the committed
    /// `tests/fixtures/setfit/tokenizer.json` would work, but that path honours
    /// the `APRENDER_SETFIT_FIXTURES` override (tokenizer_tests.rs:41-49), so an
    /// environment that set it would silently change the artifact bytes and
    /// therefore the pinned golden hash — a gate whose expected value depends on
    /// an environment variable is not a gate.
    pub(super) fn tiny_tokenizer_json() -> Vec<u8> {
        let mut s = String::new();
        s.push_str(r#"{"version":"1.0","truncation":null,"padding":null,"added_tokens":["#);
        for (id, content) in ["[PAD]", "[UNK]", "[CLS]", "[SEP]", "[MASK]"]
            .iter()
            .enumerate()
        {
            if id > 0 {
                s.push(',');
            }
            s.push_str(&format!(
                r#"{{"id":{id},"special":true,"content":"{content}","single_word":false,"lstrip":false,"rstrip":false,"normalized":false}}"#
            ));
        }
        s.push_str(
            r###"],"normalizer":{"type":"BertNormalizer","clean_text":true,"handle_chinese_chars":true,"strip_accents":null,"lowercase":true},"pre_tokenizer":{"type":"BertPreTokenizer"},"post_processor":{"type":"TemplateProcessing","single":[{"SpecialToken":{"id":"[CLS]","type_id":0}},{"Sequence":{"id":"A","type_id":0}},{"SpecialToken":{"id":"[SEP]","type_id":0}}],"pair":[{"SpecialToken":{"id":"[CLS]","type_id":0}},{"Sequence":{"id":"A","type_id":0}},{"SpecialToken":{"id":"[SEP]","type_id":0}},{"Sequence":{"id":"B","type_id":1}},{"SpecialToken":{"id":"[SEP]","type_id":1}}],"special_tokens":{"[CLS]":{"id":"[CLS]","ids":[2],"tokens":["[CLS]"]},"[SEP]":{"id":"[SEP]","ids":[3],"tokens":["[SEP]"]}}},"decoder":{"type":"WordPiece","prefix":"##","cleanup":true},"model":{"type":"WordPiece","unk_token":"[UNK]","continuing_subword_prefix":"##","max_input_chars_per_word":100,"vocab":{"###,
        );
        for (id, token) in TINY_VOCAB.iter().enumerate() {
            if id > 0 {
                s.push(',');
            }
            s.push_str(&format!(r#""{token}":{id}"#));
        }
        s.push_str("}}}");
        s.into_bytes()
    }

    /// A deterministic, platform-independent filler.
    ///
    /// Every produced value is `k / 65536 - 0.5` for an integer `k`, so it is
    /// EXACTLY representable in `f32` on every target — the fixture's own bytes
    /// therefore cannot be a source of cross-platform hash drift.
    pub(super) struct Filler(u64);

    impl Filler {
        pub(super) fn new(seed: u64) -> Self {
            Self(seed | 1)
        }

        fn next(&mut self) -> f32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let quantum = ((self.0 >> 40) & 0xFFFF) as f32 / 65536.0;
            quantum - 0.5
        }

        pub(super) fn vec(&mut self, n: usize) -> Vec<f32> {
            (0..n).map(|_| self.next()).collect()
        }
    }

    pub(super) fn fixture_architecture(vocab_remap: Option<Vec<u32>>) -> EncoderArchitecture {
        EncoderArchitecture {
            hidden: FIXTURE_HIDDEN,
            heads: FIXTURE_HEADS,
            head_dim: FIXTURE_HEAD_DIM,
            num_layers: FIXTURE_LAYERS,
            intermediate: FIXTURE_INTERMEDIATE,
            vocab: TINY_VOCAB.len(),
            positions: FIXTURE_POSITIONS,
            type_vocab_size: FIXTURE_TYPE_VOCAB,
            layer_norm_eps: 1e-12,
            pad_token_id: 0,
            hidden_act: PINNED_ACTIVATION.to_string(),
            source_revision: PINNED_REVISION.to_string(),
            tokenizer_sha256: sha256_hex(&tiny_tokenizer_json()),
            vocab_remap,
        }
    }

    pub(super) fn fixture_tensors(
        arch: &EncoderArchitecture,
    ) -> BTreeMap<String, (Vec<usize>, Vec<f32>)> {
        let h = arch.hidden;
        let im = arch.intermediate;
        let mut f = Filler::new(0x0402_0001);
        let mut t: BTreeMap<String, (Vec<usize>, Vec<f32>)> = BTreeMap::new();
        let mut put = |t: &mut BTreeMap<String, (Vec<usize>, Vec<f32>)>,
                       f: &mut Filler,
                       name: String,
                       shape: Vec<usize>| {
            let n = shape.iter().product();
            t.insert(name, (shape, f.vec(n)));
        };

        put(
            &mut t,
            &mut f,
            "embeddings.word_embeddings.weight".to_string(),
            vec![arch.vocab, h],
        );
        put(
            &mut t,
            &mut f,
            "embeddings.position_embeddings.weight".to_string(),
            vec![arch.positions, h],
        );
        put(
            &mut t,
            &mut f,
            "embeddings.token_type_embeddings.weight".to_string(),
            vec![arch.type_vocab_size, h],
        );
        put(
            &mut t,
            &mut f,
            "embeddings.LayerNorm.weight".to_string(),
            vec![h],
        );
        put(
            &mut t,
            &mut f,
            "embeddings.LayerNorm.bias".to_string(),
            vec![h],
        );

        for n in 0..arch.num_layers {
            let p = format!("encoder.layer.{n}");
            for leaf in ["query", "key", "value"] {
                put(
                    &mut t,
                    &mut f,
                    format!("{p}.attention.self.{leaf}.weight"),
                    vec![h, h],
                );
                put(
                    &mut t,
                    &mut f,
                    format!("{p}.attention.self.{leaf}.bias"),
                    vec![h],
                );
            }
            put(
                &mut t,
                &mut f,
                format!("{p}.attention.output.dense.weight"),
                vec![h, h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.attention.output.dense.bias"),
                vec![h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.attention.output.LayerNorm.weight"),
                vec![h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.attention.output.LayerNorm.bias"),
                vec![h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.intermediate.dense.weight"),
                vec![im, h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.intermediate.dense.bias"),
                vec![im],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.output.dense.weight"),
                vec![h, im],
            );
            put(&mut t, &mut f, format!("{p}.output.dense.bias"), vec![h]);
            put(
                &mut t,
                &mut f,
                format!("{p}.output.LayerNorm.weight"),
                vec![h],
            );
            put(
                &mut t,
                &mut f,
                format!("{p}.output.LayerNorm.bias"),
                vec![h],
            );
        }
        t
    }

    /// `pair_config.budget` and `pair_config.hard_cap` are `null`: both are
    /// absent-by-default knobs, so both are routinely null on an HONEST artifact.
    pub(super) fn fixture_requested_config() -> Value {
        json!({
            "max_length": 256,
            "pair_config": { "budget": null, "hard_cap": null, "strategy": "all_pairs" },
            "requested_device": "cpu",
            "seed": 7
        })
    }

    /// `ResolvedConfigRecord` — one `String`, ZERO allowlisted paths, WALKED anyway.
    pub(super) fn fixture_resolved_config() -> Value {
        json!({ "resolved_device": "cpu" })
    }

    /// `epsilon_used` is `null` — the production shape (evidence.rs:655). The
    /// fractional `f64`s are here on purpose: they are what makes the
    /// number-formatting half of the round-trip stability proof non-vacuous.
    pub(super) fn fixture_evidence() -> Value {
        json!({
            "epsilon_used": null,
            "table_hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "per_class": {
                "against": { "support": 8, "mean_margin": 0.1 },
                "favor": { "support": 8, "mean_margin": 0.333_333_333_333_333_3 },
                "neutral": { "support": 8, "mean_margin": 2.220_446_049_250_313e-16 }
            }
        })
    }

    /// `ProvenanceRecord` — four `String`, one `u64`, one `u32`; ZERO allowlisted
    /// paths, WALKED anyway, and fully populated here so the full-pin fixture
    /// carries no null in this subtree at all.
    pub(super) fn fixture_provenance() -> Value {
        json!({
            "dataset_fingerprint": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            "validation_split_fingerprint": "vvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvvv",
            "selection_semantic_hash": "ssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssss",
            "selection_ledger_hash": "llllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllll",
            "selection_root_seed": 11,
            "shots_per_class": 8
        })
    }

    fn fixture_view(vocab_remap: Option<Vec<u32>>) -> SetFitArtifactView {
        let architecture = fixture_architecture(vocab_remap);
        let tensors = fixture_tensors(&architecture);
        let mut head = Filler::new(0x0402_0002);
        let k = FIXTURE_LABELS.len();
        SetFitArtifactView {
            bundle_schema_version: FIXTURE_BUNDLE_SCHEMA_VERSION,
            format_id: FIXTURE_FORMAT_ID.to_string(),
            tokenizer_bytes: tiny_tokenizer_json(),
            head_weights: head.vec(k * architecture.hidden),
            head_intercepts: head.vec(k),
            head_n_features: architecture.hidden,
            architecture,
            tensors,
            pooling: POOLING_POLICY.to_string(),
            normalization: NORMALIZATION_POLICY.to_string(),
            l2_epsilon: L2_EPS,
            truncation_max_sequence_length: MAX_SEQUENCE_LENGTH as u32,
            padding_mode: PADDING_MODE.to_string(),
            max_length: MAX_SEQUENCE_LENGTH as u32,
            root_seed: FIXTURE_ROOT_SEED,
            ordered_labels: FIXTURE_LABELS.iter().map(|s| (*s).to_string()).collect(),
            requested_config: fixture_requested_config(),
            resolved_config: fixture_resolved_config(),
            evidence: fixture_evidence(),
            provenance: fixture_provenance(),
        }
    }

    /// THE DEFAULT BUILDER. `architecture.vocab_remap: None` — the FULL PIN.
    ///
    /// Every suite that touches the writer must exercise this shape by default,
    /// and every downstream plan that "builds the fixture artifact the same way"
    /// inherits it from here.
    pub(super) fn fixture_view_full_pin_shape() -> SetFitArtifactView {
        fixture_view(None)
    }

    /// The slice topology, used ONLY by the tests that need `vocab_remap: Some`.
    ///
    /// The identity remap is the smallest one that is valid for this vocabulary:
    /// `slice_to_orig[i] == i`, so `slice_vocab() == dims.vocab` and every id the
    /// tiny tokenizer emits resolves.
    pub(super) fn fixture_view_slice_shape() -> SetFitArtifactView {
        let remap: Vec<u32> = (0..TINY_VOCAB.len())
            .map(|i| u32::try_from(i).expect("48 fits in u32"))
            .collect();
        fixture_view(Some(remap))
    }
}

#[cfg(all(test, feature = "setfit"))]
mod tests {
    //! Writer behaviour: the tensor set, the head tensors, the one-key document,
    //! the typed refusals and the probe records.

    use super::fixture::*;
    use super::*;

    use crate::format::v2::{AprV2Flags, AprV2Reader};

    /// Read the ONE custom document back out of written bytes.
    fn read_doc(bytes: &[u8]) -> JsonMap<String, Value> {
        let reader =
            AprV2Reader::from_bytes(bytes).expect("the writer emits a parseable container");
        reader
            .metadata()
            .custom
            .get(CUSTOM_METADATA_KEY)
            .expect("the one custom key is present")
            .as_object()
            .expect("the custom key holds a JSON object")
            .clone()
    }

    fn written(view: &SetFitArtifactView) -> Vec<u8> {
        write_setfit_apr(view).expect("the fixture view is writable")
    }

    #[test]
    fn writer_writes_exactly_the_architecture_derived_tensor_set() {
        let view = fixture_view_full_pin_shape();
        let bytes = written(&view);
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");
        let observed: BTreeSet<String> = reader
            .tensor_names()
            .into_iter()
            .map(str::to_string)
            .collect();
        let expected = expected_container_tensor_names(view.architecture.num_layers);
        assert_eq!(observed, expected);
        // The count is DERIVED (5 global + 16 per layer + 3 schema-owned), never
        // quoted: the same arithmetic gives 104 for the pinned six-layer model.
        assert_eq!(expected.len(), 5 + 16 * FIXTURE_LAYERS + 3);
    }

    #[test]
    fn the_head_is_two_named_f32_tensors_with_the_declared_shapes() {
        let view = fixture_view_full_pin_shape();
        let bytes = written(&view);
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");

        let weight = reader
            .get_tensor(HEAD_WEIGHT_TENSOR)
            .expect("setfit.head.weight is a first-class tensor entry");
        assert_eq!(
            weight.shape,
            vec![FIXTURE_LABELS.len(), view.head_n_features]
        );
        let bias = reader
            .get_tensor(HEAD_BIAS_TENSOR)
            .expect("setfit.head.bias is a first-class tensor entry");
        assert_eq!(bias.shape, vec![FIXTURE_LABELS.len()]);

        // Bit-exact round trip, not "close enough": the payload is raw LE f32.
        assert_eq!(
            reader.get_f32_tensor(HEAD_WEIGHT_TENSOR).expect("f32"),
            view.head_weights
        );
        assert_eq!(
            reader.get_f32_tensor(HEAD_BIAS_TENSOR).expect("f32"),
            view.head_intercepts
        );
    }

    #[test]
    fn metadata_declares_the_setfit_model_type_and_exactly_one_custom_key() {
        let bytes = written(&fixture_view_full_pin_shape());
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");
        assert_eq!(reader.metadata().model_type, MODEL_TYPE_TAG);
        assert_eq!(
            reader.metadata().custom.len(),
            1,
            "N custom keys serialize in HashMap iteration order and are not reproducible"
        );
        assert!(reader.metadata().custom.contains_key(CUSTOM_METADATA_KEY));
        assert_eq!(
            reader.metadata().created_at,
            None,
            "a timestamp would make two artifacts of the same run differ"
        );
    }

    #[test]
    fn the_doc_key_set_equals_the_contract_field_list_exactly() {
        let doc = read_doc(&written(&fixture_view_full_pin_shape()));
        let observed: BTreeSet<&str> = doc.keys().map(String::as_str).collect();
        let expected: BTreeSet<&str> = SETFIT_ARTIFACT_DOC_FIELDS.iter().copied().collect();
        assert_eq!(
            observed, expected,
            "an added, renamed or missing doc field must be a LOUD failure"
        );
        assert_eq!(doc["schema"], Value::String(ARTIFACT_SCHEMA.to_string()));
        assert_eq!(doc["schema_version"], Value::from(ARTIFACT_SCHEMA_VERSION));
    }

    #[test]
    fn the_doc_carries_the_first_level_identity_fields_and_the_nested_groups() {
        let view = fixture_view_full_pin_shape();
        let doc = read_doc(&written(&view));
        assert_eq!(
            doc["tokenizer_sha256"],
            Value::String(view.architecture.tokenizer_sha256.clone())
        );
        assert_eq!(
            doc["ordered_labels"],
            serde_json::to_value(&view.ordered_labels).expect("labels serialize")
        );
        let preprocessing = doc["preprocessing"].as_object().expect("object");
        assert_eq!(
            preprocessing["l2_epsilon_hex"],
            Value::String(f32_bits_hex(view.l2_epsilon)),
            "every doc-OWNED float is a bit-pattern hex string, never a JSON number"
        );
        assert_eq!(
            preprocessing["pooling"],
            Value::String(view.pooling.clone())
        );
        let head = doc["head"].as_object().expect("object");
        assert_eq!(head["n_features"], Value::from(view.head_n_features));
        assert_eq!(head["num_labels"], Value::from(view.ordered_labels.len()));
    }

    #[test]
    fn a_missing_encoder_tensor_is_a_typed_incomplete_tensor_set() {
        let mut view = fixture_view_full_pin_shape();
        view.tensors
            .remove("encoder.layer.1.output.LayerNorm.bias")
            .expect("the fixture carries it");
        let err = write_setfit_apr(&view).expect_err("a partial artifact must not be produced");
        assert!(
            matches!(&err, SetFitArtifactError::IncompleteTensorSet { missing }
                if missing == &vec!["encoder.layer.1.output.LayerNorm.bias".to_string()]),
            "got {err:?}"
        );
    }

    #[test]
    fn an_unmapped_hf_tensor_name_is_a_typed_refusal() {
        let mut view = fixture_view_full_pin_shape();
        view.tensors.insert(
            "encoder.layer.0.attention.self.rotary.weight".to_string(),
            (vec![1], vec![0.0]),
        );
        let err = write_setfit_apr(&view).expect_err("an unnamed tensor must not be dropped");
        assert!(
            matches!(&err, SetFitArtifactError::UnmappedTensorName { hf_name }
                if hf_name == "encoder.layer.0.attention.self.rotary.weight"),
            "got {err:?}"
        );
    }

    #[test]
    fn a_non_finite_encoder_tensor_value_is_refused_before_serialization() {
        let mut view = fixture_view_full_pin_shape();
        let entry = view
            .tensors
            .get_mut("embeddings.LayerNorm.bias")
            .expect("present");
        entry.1[2] = f32::NAN;
        let err = write_setfit_apr(&view).expect_err("NaN must never reach the payload");
        assert!(
            matches!(&err, SetFitArtifactError::NonFiniteValue { path }
                if path == "tensors.embeddings.LayerNorm.bias[2]"),
            "got {err:?}"
        );
    }

    #[test]
    fn a_non_finite_head_coefficient_is_refused_before_serialization() {
        let mut view = fixture_view_full_pin_shape();
        view.head_intercepts[1] = f32::INFINITY;
        let err = write_setfit_apr(&view).expect_err("+Inf must never reach the payload");
        assert!(
            matches!(&err, SetFitArtifactError::NonFiniteValue { path }
                if path == "head_intercepts[1]"),
            "got {err:?}"
        );
    }

    #[test]
    fn a_head_arity_that_disagrees_with_the_label_set_is_refused() {
        let mut view = fixture_view_full_pin_shape();
        view.head_weights.push(0.5);
        let err = write_setfit_apr(&view).expect_err("K*d is not negotiable");
        assert!(
            matches!(&err, SetFitArtifactError::InconsistentTensorSet { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn a_head_feature_dimension_that_disagrees_with_the_encoder_width_is_refused() {
        let mut view = fixture_view_full_pin_shape();
        view.head_n_features = FIXTURE_HIDDEN + 1;
        view.head_weights = vec![0.25; FIXTURE_LABELS.len() * view.head_n_features];
        let err = write_setfit_apr(&view)
            .expect_err("a head that cannot consume this encoder's embedding is not shippable");
        assert!(
            matches!(&err, SetFitArtifactError::InconsistentTensorSet { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn the_tokenizer_blob_round_trips_byte_identically_and_matches_the_recorded_digest() {
        let view = fixture_view_full_pin_shape();
        let bytes = written(&view);
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");
        let entry = reader
            .get_tensor(TOKENIZER_BLOB_TENSOR)
            .expect("tokenizer.blob is a tensor entry");
        assert_eq!(entry.dtype, TensorDType::U8);
        assert_eq!(entry.shape, vec![view.tokenizer_bytes.len()]);
        let payload = reader
            .get_tensor_data(TOKENIZER_BLOB_TENSOR)
            .expect("payload");
        // BYTE-IDENTICAL: an artifact carrying only the digest could DETECT a
        // substituted tokenizer and could not REBUILD the right one.
        assert_eq!(payload, view.tokenizer_bytes.as_slice());
        let doc = read_doc(&bytes);
        assert_eq!(
            doc["tokenizer_sha256"],
            Value::String(sha256_hex(payload)),
            "the recorded digest must describe the bytes that travelled"
        );
    }

    #[test]
    fn a_tokenizer_digest_that_does_not_describe_the_bytes_is_refused() {
        let mut view = fixture_view_full_pin_shape();
        view.architecture.tokenizer_sha256 = "0".repeat(64);
        let err = write_setfit_apr(&view)
            .expect_err("a mis-paired tokenizer produces confidently wrong embeddings");
        assert!(
            matches!(&err, SetFitArtifactError::TokenizerHashMismatch { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn the_hf_name_map_is_written_total_and_injective_over_the_encoder_tensor_set() {
        let view = fixture_view_full_pin_shape();
        let doc = read_doc(&written(&view));
        let map = doc["hf_name_map"].as_object().expect("object");
        assert_eq!(map.len(), view.tensors.len(), "total over the tensor set");
        let canonical: BTreeSet<&str> = map.values().filter_map(Value::as_str).collect();
        assert_eq!(
            canonical.len(),
            map.len(),
            "injective: no two HF names collide"
        );
        for hf in view.tensors.keys() {
            assert!(map.contains_key(hf), "{hf} has no canonical entry");
        }
        // The map is WRITTEN INTO the artifact precisely so `deserialize` inverts
        // a map the artifact carries rather than re-deriving names from a table
        // that may have moved between the write and the read.
        assert_eq!(
            map["embeddings.position_embeddings.weight"],
            Value::String("position_embd.weight".to_string())
        );
    }

    #[test]
    fn the_six_probe_records_use_the_contract_resident_inputs_only() {
        let doc = read_doc(&written(&fixture_view_full_pin_shape()));
        let probes = doc["probes"].as_array().expect("array");
        assert_eq!(probes.len(), PROBE_COUNT);
        let expected = probe_inputs();
        for (index, probe) in probes.iter().enumerate() {
            let record = probe.as_object().expect("object");
            let keys: BTreeSet<&str> = record.keys().map(String::as_str).collect();
            assert_eq!(
                keys,
                [
                    "embedding_hex",
                    "input",
                    "label",
                    "logits_hex",
                    "probabilities_hex"
                ]
                .into_iter()
                .collect::<BTreeSet<&str>>()
            );
            assert_eq!(record["input"], Value::String(expected[index].clone()));
        }
        // T-04-04: no dataset text is embedded. The truncation probe is a
        // repetition of a contract-resident unit, not a corpus sample.
        assert!(expected[2].starts_with(PROBE_TRUNCATION_REPEAT_UNIT));
        assert_eq!(
            expected[2].len(),
            PROBE_TRUNCATION_REPEAT_UNIT.len() * PROBE_TRUNCATION_REPEAT_COUNT
        );
    }

    #[test]
    fn probe_expectations_are_bit_pattern_hex_and_the_label_comes_from_the_label_set() {
        let view = fixture_view_full_pin_shape();
        let doc = read_doc(&written(&view));
        let probes = doc["probes"].as_array().expect("array");
        for probe in probes {
            let record = probe.as_object().expect("object");
            let embedding = record["embedding_hex"].as_array().expect("array");
            assert_eq!(embedding.len(), view.architecture.hidden);
            for value in embedding {
                let hex = value.as_str().expect("hex string");
                assert_eq!(hex.len(), 8, "an f32 bit pattern is 8 hex characters");
                assert!(hex
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
            }
            for field in ["logits_hex", "probabilities_hex"] {
                let values = record[field].as_array().expect("array");
                assert_eq!(values.len(), view.ordered_labels.len());
            }
            let label = record["label"].as_str().expect("string");
            assert!(view.ordered_labels.iter().any(|l| l == label), "{label}");
        }
    }

    #[test]
    fn the_slice_shape_fixture_writes_under_the_same_production_rule() {
        let view = fixture_view_slice_shape();
        assert!(view.architecture.vocab_remap.is_some());
        let bytes = write_setfit_apr(&view).expect("the slice topology is writable");
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");
        let observed: BTreeSet<String> = reader
            .tensor_names()
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_eq!(
            observed,
            expected_container_tensor_names(view.architecture.num_layers),
            "the same rule judges the fixture and the pin; there is no test-only branch"
        );
    }

    #[test]
    fn every_tensor_is_written_row_major_and_the_container_says_so() {
        let bytes = written(&fixture_view_full_pin_shape());
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");
        assert!(
            reader.header().flags.contains(AprV2Flags::LAYOUT_ROW_MAJOR),
            "LAYOUT-001/002: there is no GGUF import path into this writer"
        );
        assert!(!reader
            .header()
            .flags
            .contains(AprV2Flags::LAYOUT_COLUMN_MAJOR));
    }

    #[test]
    fn the_per_entry_structural_size_rule_holds_for_every_written_tensor() {
        let bytes = written(&fixture_view_full_pin_shape());
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");
        for entry in reader.tensor_index() {
            let elements: usize = entry.shape.iter().product();
            let width = if entry.dtype == TensorDType::U8 { 1 } else { 4 };
            assert_eq!(
                entry.size as usize,
                elements * width,
                "{}: declared size disagrees with declared shape",
                entry.name
            );
        }
    }

    #[test]
    fn artifact_sha256_hex_is_the_sha256_of_the_bytes_it_was_given() {
        let bytes = written(&fixture_view_full_pin_shape());
        assert_eq!(artifact_sha256_hex(&bytes), sha256_hex(&bytes));
        assert_eq!(artifact_sha256_hex(&bytes).len(), 64);
        assert_ne!(
            artifact_sha256_hex(&bytes),
            artifact_sha256_hex(&bytes[1..])
        );
    }

    #[test]
    fn f32_bit_pattern_hex_matches_the_bundle_little_endian_precedent() {
        // bundle.rs:699-715 encodes `value.to_bits().to_le_bytes()`. 1.0f32 is
        // 0x3f800000, whose LE bytes are 00 00 80 3f. A big-endian rendering
        // would read "3f800000" and would silently disagree with the bundle
        // about the same number.
        assert_eq!(f32_bits_hex(1.0), "0000803f");
        assert_eq!(f32_bits_hex(0.0), "00000000");
        assert_eq!(f32_bits_hex(-2.0), "000000c0");
    }
}

#[cfg(all(test, feature = "setfit"))]
mod nullable {
    //! The four-path allowlist over five walked sub-documents.
    //!
    //! ACCEPT cases are not optional and are counted separately. A suite with
    //! zero accept cases would have passed with an allowlist containing only
    //! `evidence.epsilon_used` — an allowlist that refuses every production
    //! artifact — because every fixture used the slice shape, which sets
    //! `vocab_remap: Some(..)` and never reaches the failing path.

    use super::fixture::*;
    use super::*;

    use crate::format::v2::AprV2Reader;

    fn doc_of(view: &SetFitArtifactView) -> JsonMap<String, Value> {
        let bytes = write_setfit_apr(view).expect("writable");
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");
        reader
            .metadata()
            .custom
            .get(CUSTOM_METADATA_KEY)
            .expect("one custom key")
            .as_object()
            .expect("object")
            .clone()
    }

    // ---- ACCEPT ----------------------------------------------------------

    #[test]
    fn accept_the_production_full_pin_shape_whose_vocab_remap_is_none() {
        let view = fixture_view_full_pin_shape();
        assert!(
            view.architecture.vocab_remap.is_none(),
            "the DEFAULT fixture must carry the production nullability shape"
        );
        let bytes = write_setfit_apr(&view)
            .expect("a guard that rejects the full pin refuses every real artifact");
        assert!(!bytes.is_empty());
        let doc = doc_of(&view);
        assert!(
            observed_null_paths(&doc).contains(&"architecture.vocab_remap".to_string()),
            "the accept case must actually REACH the allowlisted path"
        );
    }

    #[test]
    fn accept_a_view_whose_pair_config_budget_and_hard_cap_are_both_none() {
        let view = fixture_view_full_pin_shape();
        let doc = doc_of(&view);
        let observed = observed_null_paths(&doc);
        assert!(observed.contains(&"requested_config.pair_config.budget".to_string()));
        assert!(observed.contains(&"requested_config.pair_config.hard_cap".to_string()));
        assert!(disallowed_null_paths(&doc).is_empty());
    }

    #[test]
    fn accept_a_view_whose_evidence_epsilon_used_is_none() {
        let view = fixture_view_full_pin_shape();
        let doc = doc_of(&view);
        assert!(observed_null_paths(&doc).contains(&"evidence.epsilon_used".to_string()));
        assert!(write_setfit_apr(&view).is_ok());
    }

    #[test]
    fn the_full_pin_fixture_emits_exactly_the_four_allowlisted_nulls_and_no_others() {
        let doc = doc_of(&fixture_view_full_pin_shape());
        let observed: BTreeSet<String> = observed_null_paths(&doc).into_iter().collect();
        let allowed: BTreeSet<String> = NULLABLE_PATH_ALLOWLIST
            .iter()
            .map(|p| (*p).to_string())
            .collect();
        assert_eq!(observed, allowed);
    }

    // ---- REJECT ----------------------------------------------------------

    #[test]
    fn reject_a_null_inside_provenance_and_name_the_path() {
        // THE case that proves the FIFTH subtree is actually walked: an unwalked
        // subtree accepts every null silently, so a suite without this test
        // cannot tell a walked provenance from an unwalked one.
        let mut view = fixture_view_full_pin_shape();
        view.provenance["dataset_fingerprint"] = Value::Null;
        let err = write_setfit_apr(&view).expect_err("provenance is one of the five");
        assert!(
            matches!(&err, SetFitArtifactError::NonFiniteValue { path }
                if path == "provenance.dataset_fingerprint"),
            "got {err:?}"
        );
    }

    #[test]
    fn reject_a_null_at_resolved_config_resolved_device_and_name_the_path() {
        let mut view = fixture_view_full_pin_shape();
        view.resolved_config["resolved_device"] = Value::Null;
        let err = write_setfit_apr(&view).expect_err("resolved_config is one of the five");
        assert!(
            matches!(&err, SetFitArtifactError::NonFiniteValue { path }
                if path == "resolved_config.resolved_device"),
            "got {err:?}"
        );
    }

    #[test]
    fn reject_a_non_finite_f64_smuggled_into_evidence_as_a_null() {
        // This is the CR-03 signature end to end: `serde_json` renders every
        // non-finite f64 as `null` SILENTLY, so the value is already gone by the
        // time the walk sees it — which is exactly why the walk exists.
        let mut view = fixture_view_full_pin_shape();
        view.evidence["per_class"]["favor"]["mean_margin"] =
            serde_json::to_value(f64::NAN).expect("serde_json maps non-finite f64 to null");
        assert_eq!(
            view.evidence["per_class"]["favor"]["mean_margin"],
            Value::Null
        );
        let err = write_setfit_apr(&view).expect_err("a destroyed measurement must not ship");
        assert!(
            matches!(&err, SetFitArtifactError::NonFiniteValue { path }
                if path == "evidence.per_class.favor.mean_margin"),
            "got {err:?}"
        );
    }

    #[test]
    fn reject_a_null_at_architecture_hidden_act_and_name_the_path() {
        // `architecture` is derived from the TYPED `EncoderArchitecture`, whose
        // only `Option` is the allowlisted `vocab_remap` — so no view can smuggle
        // a null in here, which is a stronger guarantee than a check. The guard
        // is still exercised at the exact function `write_setfit_apr` calls, so
        // that a future `Option` on that record is caught the moment it lands.
        let mut doc = doc_of(&fixture_view_full_pin_shape());
        doc["architecture"]["hidden_act"] = Value::Null;
        let err = guard_subdocument_nulls(&doc).expect_err("architecture is one of the five");
        assert!(
            matches!(&err, SetFitArtifactError::NonFiniteValue { path }
                if path == "architecture.hidden_act"),
            "got {err:?}"
        );
    }

    // ---- THE CONSTANTS THEMSELVES ---------------------------------------

    #[test]
    fn the_allowlist_constant_is_exactly_the_contracts_four_paths() {
        assert_eq!(NULLABLE_PATH_ALLOWLIST.len(), 4);
        assert_eq!(
            NULLABLE_PATH_ALLOWLIST,
            [
                "architecture.vocab_remap",
                "requested_config.pair_config.budget",
                "requested_config.pair_config.hard_cap",
                "evidence.epsilon_used",
            ],
            "a fifth entry or a missing entry is a silent widening of the guard"
        );
    }

    #[test]
    fn the_walk_covers_all_five_sub_documents() {
        assert_eq!(WALKED_SUBDOCUMENTS.len(), 5);
        assert_eq!(
            WALKED_SUBDOCUMENTS,
            [
                "architecture",
                "requested_config",
                "resolved_config",
                "evidence",
                "provenance",
            ]
        );
        // The two numbers differ on purpose: `resolved_config` and `provenance`
        // contribute ZERO allowlisted paths and are walked anyway.
        assert!(WALKED_SUBDOCUMENTS.len() > NULLABLE_PATH_ALLOWLIST.len());
        for name in WALKED_SUBDOCUMENTS {
            assert!(
                SETFIT_ARTIFACT_DOC_FIELDS.contains(&name),
                "{name} must be a doc field to be walkable"
            );
        }
    }

    #[test]
    fn the_walk_does_not_cover_the_containers_typed_metadata_nulls() {
        // `license`, `data_source` and `data_license` serialize as explicit
        // `null` by container design (FALSIFY-SHIP-022). Pointing the walk at
        // them would refuse every artifact this writer produces.
        let bytes = write_setfit_apr(&fixture_view_full_pin_shape()).expect("writable");
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");
        assert_eq!(reader.metadata().license, None);
        assert_eq!(reader.metadata().data_source, None);
        assert_eq!(reader.metadata().data_license, None);
        for name in WALKED_SUBDOCUMENTS {
            assert!(!["license", "data_source", "data_license"].contains(&name));
        }
    }
}

#[cfg(all(test, feature = "setfit"))]
mod determinism {
    //! Determinism proven where it can actually break.

    use super::fixture::*;
    use super::*;

    use crate::format::v2::{AprV2Metadata, AprV2Reader};

    /// Set in the child process only.
    const CHILD_ENV: &str = "SETFIT_APR_DETERMINISM_CHILD";

    /// The child's single line of output.
    const CHILD_MARKER: &str = "SETFIT_APR_CHILD_SHA256=";

    /// The SHA-256 of the tiny fixture artifact IN ITS DEFAULT FULL-PIN
    /// NULLABILITY SHAPE (`architecture.vocab_remap: None`).
    ///
    /// The name states the shape on purpose: a later reader must be able to tell
    /// WHICH fixture this pins without reading the builder. A drift here is
    /// either a deliberate writer change (re-bless it, Ph1 D-13) or a real
    /// finding — the artifact hash IS the identity every Phase 4 response
    /// carries, so a platform that produces different bytes for this view has a
    /// parity defect, not a flaky test.
    const GOLDEN_SHA256_FIXTURE_VIEW_FULL_PIN_SHAPE: &str =
        "13e5c2965e95fc970c19a93f298a33b123f5c524a03c1e33e4a0e36967000bf4";

    #[test]
    fn two_writes_of_one_view_are_byte_identical_in_one_process() {
        let view = fixture_view_full_pin_shape();
        let first = write_setfit_apr(&view).expect("writable");
        let second = write_setfit_apr(&view).expect("writable");
        assert_eq!(first, second);
        assert_eq!(artifact_sha256_hex(&first), artifact_sha256_hex(&second));
    }

    #[test]
    fn metadata_survives_a_parse_and_re_serialize_byte_identically() {
        let bytes = write_setfit_apr(&fixture_view_full_pin_shape()).expect("writable");
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");
        let once = reader.metadata().to_json().expect("metadata serializes");
        let reparsed = AprV2Metadata::from_json(&once).expect("metadata parses");
        let twice = reparsed.to_json().expect("metadata re-serializes");
        assert_eq!(once, twice, "write -> parse -> write must be the identity");
        assert_eq!(
            reparsed.custom.len(),
            1,
            "exactly one top-level custom key beyond the typed fields"
        );
    }

    #[test]
    fn all_five_sub_documents_round_trip_byte_identically_including_the_allowlisted_nulls() {
        let view = fixture_view_full_pin_shape();
        let bytes = write_setfit_apr(&view).expect("writable");
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");
        let doc = reader
            .metadata()
            .custom
            .get(CUSTOM_METADATA_KEY)
            .expect("one custom key")
            .as_object()
            .expect("object")
            .clone();

        for name in WALKED_SUBDOCUMENTS {
            let sub = doc.get(name).expect("all five sub-documents are present");
            let once = serde_json::to_vec(sub).expect("serializes");
            let back: Value = serde_json::from_slice(&once).expect("parses");
            let twice = serde_json::to_vec(&back).expect("re-serializes");
            assert_eq!(once, twice, "{name} is not round-trip stable");
        }

        // The number-formatting half is only exercised if a fractional f64 is
        // actually present; assert that rather than assume it.
        let evidence = serde_json::to_string(&doc["evidence"]).expect("string");
        assert!(evidence.contains("0.3333333333333333"), "got {evidence}");

        // A `null -> None -> null` round trip is byte-STABLE, which is exactly
        // why closure alone cannot substitute for the write-time null scan: an
        // allowlisted null round-trips perfectly while carrying no value.
        assert!(observed_null_paths(&doc).contains(&"architecture.vocab_remap".to_string()));
        assert!(observed_null_paths(&doc).contains(&"evidence.epsilon_used".to_string()));
        assert!(doc.contains_key("provenance"));
    }

    #[test]
    fn the_fixture_artifact_hash_matches_the_committed_golden() {
        let bytes = write_setfit_apr(&fixture_view_full_pin_shape()).expect("writable");
        assert_eq!(
            artifact_sha256_hex(&bytes),
            GOLDEN_SHA256_FIXTURE_VIEW_FULL_PIN_SHAPE
        );
    }

    #[test]
    fn the_public_writer_path_can_never_produce_two_custom_keys() {
        // In-band negative: build metadata with TWO custom keys by hand and show
        // the resulting instability is what the one-key rule prevents. `custom`
        // is a `HashMap` with `RandomState`, so with >1 key the serialized order
        // is unspecified — three runs of one binary produced three orders when
        // this was measured. The public writer is then shown to produce exactly
        // one key, so it cannot reach that state at all.
        let mut two = AprV2Metadata {
            model_type: MODEL_TYPE_TAG.to_string(),
            ..Default::default()
        };
        two.custom.insert("setfit".to_string(), Value::from("a"));
        two.custom
            .insert("setfit_extra".to_string(), Value::from("b"));
        let rendered = String::from_utf8(two.to_json().expect("serializes")).expect("utf8");
        // Both keys are present; which comes FIRST is not specified anywhere.
        assert!(rendered.contains("setfit_extra"));
        assert_eq!(two.custom.len(), 2);

        let bytes = write_setfit_apr(&fixture_view_full_pin_shape()).expect("writable");
        let reader = AprV2Reader::from_bytes(&bytes).expect("parseable");
        assert_eq!(reader.metadata().custom.len(), 1);
    }

    #[test]
    fn cross_process_writes_produce_the_same_artifact_sha256() {
        let parent = artifact_sha256_hex(
            &write_setfit_apr(&fixture_view_full_pin_shape()).expect("parent write"),
        );
        let exe = std::env::current_exe().expect("the test binary knows its own path");
        let output = std::process::Command::new(exe)
            .arg("setfit::artifact::determinism::child_writes_the_fixture_and_prints_its_sha256")
            .arg("--exact")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env(CHILD_ENV, "1")
            .output()
            .expect("spawn the child test binary");

        // CLAUDE.md Verification rule 1: the status is read DIRECTLY off
        // `Output.status`. Reading it through a pipe would report the LAST
        // command's status and the assertion would be unreachable.
        assert!(
            output.status.success(),
            "child exited {:?}\nstdout: {}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8_lossy(&output.stdout);
        // libtest with `--nocapture` prints the test's stdout on the SAME line as
        // its `test <name> ... ` prefix, so the marker is searched for ANYWHERE
        // in the line rather than at its start. A `strip_prefix` here found
        // nothing and the test failed loudly — which is the correct behaviour for
        // a missing marker and is why the `expect` is not an `unwrap_or_default`.
        let child = stdout
            .lines()
            .find_map(|line| {
                line.find(CHILD_MARKER)
                    .map(|at| &line[at + CHILD_MARKER.len()..])
            })
            .map(str::trim)
            .expect("the child must print its marker line; a missing marker FAILS, never passes");
        assert_eq!(child.len(), 64, "the child printed {child:?}, not a sha256");
        assert_eq!(
            child, parent,
            "same view, two processes, two different HashMap RandomStates"
        );
    }

    #[test]
    fn child_writes_the_fixture_and_prints_its_sha256() {
        // A no-op unless the parent set the env var, so a plain `cargo test` run
        // does not spawn anything and this test cannot recurse.
        if std::env::var(CHILD_ENV).is_err() {
            return;
        }
        let bytes = write_setfit_apr(&fixture_view_full_pin_shape()).expect("child write");
        println!("{CHILD_MARKER}{}", artifact_sha256_hex(&bytes));
    }
}
