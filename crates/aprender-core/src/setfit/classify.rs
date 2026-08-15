//! The D-08 classification request/response pair (OPS-04, OPS-06).
//!
//! ONE versioned envelope, owned by core, serialized byte-identically by every
//! surface: the Rust API, `apr classify --json` (04-07) and
//! `POST /v1/classify` (04-08). A surface-local response type would make "the
//! three surfaces agree" a claim about three independently-maintained structs;
//! here it is true by construction, because there is only one struct.
//!
//! # Why every field is private (review M1)
//!
//! The claim this module exists to make true is *"a non-finite value is
//! unrepresentable in a `ClassifyResponse`"*. A derived `Deserialize` over
//! public fields falsifies that claim twice over: any caller can build a value
//! with a struct literal, and any JSON body can produce one without passing the
//! validating constructor. So the fields are private with read accessors, the
//! only constructors validate, and BOTH serde directions route through a
//! PRIVATE wire struct via `#[serde(into = ..., try_from = ...)]` — the
//! `SetFitTrainConfig` precedent (aprender-train `config.rs:175-177`).
//!
//! The constructors are nonetheless `pub`, on purpose. The parity harness
//! (04-09) builds its in-band negative by constructing a response whose
//! probability is perturbed by 10x the contract tolerance — a FINITE value, and
//! therefore a legitimate construction. A test-only backdoor would have been a
//! shipped backdoor.
//!
//! # Why the request document is shared (review M2)
//!
//! [`ClassifyRequestDocument`] is the ONE document the CLI `--input` file and
//! the HTTP body both carry. The alternative — a line-delimited CLI format —
//! cannot represent a text containing a newline, so a whitespace probe would
//! arrive as two CLI texts and one HTTP text and the parity surfaces would
//! receive DIFFERENT ordered input sets while appearing to agree on everything
//! they did compare.

use serde::{Deserialize, Serialize};

/// The envelope's schema version.
///
/// Bumped only by a deliberate, `pv diff`-visible change to the field set. A
/// payload declaring any other version is a typed rejection rather than a
/// best-effort parse: silently accepting a v2 body as v1 is how a renamed field
/// becomes a missing field nobody notices.
pub const CLASSIFY_SCHEMA_VERSION: u32 = 1;

/// Contract bound `max_batch_texts` (`setfit-apr-v1` item 11).
///
/// Enforced in core, BEFORE tokenization: a batch bound checked after
/// tokenization is not a bound on the work an attacker can request (T-04-11).
pub const MAX_BATCH_TEXTS: usize = 256;

/// Absolute tolerance on one result row's probability mass.
///
/// Absolute and not relative: probabilities live in `[0, 1]`, and a relative
/// bound near zero would be unboundedly strict.
pub const PROBABILITY_MASS_ABS_TOLERANCE: f64 = 1e-6;

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

/// Everything the classify path and the envelope's invariants can refuse.
///
/// Every variant is small (no boxed payload is needed) and the enum is
/// `#[non_exhaustive]`, so 04-07/04-08/04-09 match with a wildcard arm and a
/// later variant is not a breaking change.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ClassifyError {
    /// The request carried zero texts.
    EmptyInput,
    /// The request carried more texts than [`MAX_BATCH_TEXTS`] allows.
    BatchTooLarge {
        /// The contract bound.
        max: usize,
        /// What the request asked for.
        got: usize,
    },
    /// A non-finite value reached a response field.
    NonFiniteResponse {
        /// Which field: `probabilities`, `logits`, `margin` or `latency_ms`.
        field: &'static str,
    },
    /// `latency_ms` was negative. Zero is legal — see [`ClassifyResponse::latency_ms`].
    NegativeLatency {
        /// The observed value.
        got: f64,
    },
    /// A probability row did not sum to 1 within [`PROBABILITY_MASS_ABS_TOLERANCE`].
    ProbabilityMassOutOfRange {
        /// The observed mass.
        mass: f64,
    },
    /// A vector's arity disagreed with the label count.
    LabelCountMismatch {
        /// The arity every row must have.
        expected: usize,
        /// The arity observed.
        got: usize,
    },
    /// A payload declared a schema version this build does not implement.
    UnsupportedSchemaVersion {
        /// [`CLASSIFY_SCHEMA_VERSION`].
        expected: u32,
        /// What the payload declared.
        got: u32,
    },
    /// Tokenization or the encoder forward pass failed.
    EncodeFailed {
        /// The typed error's rendering, naming what failed.
        reason: String,
    },
    /// The classifier head refused the embeddings it was handed.
    HeadFailed {
        /// The typed error's rendering, naming what failed.
        reason: String,
    },
}

impl std::fmt::Display for ClassifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "the request carried zero texts"),
            Self::BatchTooLarge { max, got } => {
                write!(f, "the request carried {got} texts; the bound is {max}")
            }
            Self::NonFiniteResponse { field } => {
                write!(f, "a non-finite value reached the `{field}` field")
            }
            Self::NegativeLatency { got } => write!(f, "latency_ms is {got}, which is negative"),
            Self::ProbabilityMassOutOfRange { mass } => write!(
                f,
                "a result row has probability mass {mass}, which is not 1 within \
                 {PROBABILITY_MASS_ABS_TOLERANCE}"
            ),
            Self::LabelCountMismatch { expected, got } => {
                write!(f, "expected {expected} entries per row, observed {got}")
            }
            Self::UnsupportedSchemaVersion { expected, got } => write!(
                f,
                "the payload declares classify schema version {got}; this build implements \
                 {expected}"
            ),
            Self::EncodeFailed { reason } => write!(f, "the encode step failed: {reason}"),
            Self::HeadFailed { reason } => write!(f, "the classifier head failed: {reason}"),
        }
    }
}

impl std::error::Error for ClassifyError {}

// ---------------------------------------------------------------------------
// The request document (review M2)
// ---------------------------------------------------------------------------

/// The ONE request document every surface parses.
///
/// `deny_unknown_fields`, so a key this schema does not model is a rejection
/// rather than a silently ignored knob. Fields are public because this is an
/// INPUT document: it carries no invariant a caller could break, and the bounds
/// that matter ([`MAX_BATCH_TEXTS`], non-emptiness) are enforced by
/// [`crate::setfit::artifact::VerifiedSetFitModel::classify`] where the work
/// would otherwise happen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassifyRequestDocument {
    /// The ordered texts to classify. Order is response order.
    pub texts: Vec<String>,
    /// Whether the response should carry per-class logits.
    ///
    /// Defaults to `false`, so a body that omits it is legal and means "no
    /// logits" rather than "missing field".
    #[serde(default)]
    pub include_logits: bool,
}

impl ClassifyRequestDocument {
    /// A request for `texts`, without logits.
    #[must_use]
    pub fn new<I, S>(texts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            texts: texts.into_iter().map(Into::into).collect(),
            include_logits: false,
        }
    }

    /// The same request, with per-class logits requested.
    #[must_use]
    pub fn with_logits(mut self) -> Self {
        self.include_logits = true;
        self
    }
}

// ---------------------------------------------------------------------------
// One text's result
// ---------------------------------------------------------------------------

/// One text's classification, in input order.
///
/// Every field is private; [`Self::new`] is the only constructor and it
/// validates. `Deserialize` routes through the same validation via the private
/// [`ClassifyResultWire`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(into = "ClassifyResultWire", try_from = "ClassifyResultWire")]
pub struct ClassifyResult {
    label: String,
    probabilities: Vec<f64>,
    logits: Option<Vec<f64>>,
    margin: f64,
    token_count: u32,
    truncated: bool,
}

impl ClassifyResult {
    /// The validating constructor — the only way a `ClassifyResult` exists.
    ///
    /// # Errors
    ///
    /// [`ClassifyError::NonFiniteResponse`] naming `probabilities`, `logits` or
    /// `margin`; [`ClassifyError::LabelCountMismatch`] when a logit vector's
    /// arity differs from the probability vector's;
    /// [`ClassifyError::ProbabilityMassOutOfRange`] when the row does not sum to
    /// 1 within [`PROBABILITY_MASS_ABS_TOLERANCE`].
    pub fn new(
        label: String,
        probabilities: Vec<f64>,
        logits: Option<Vec<f64>>,
        margin: f64,
        token_count: u32,
        truncated: bool,
    ) -> Result<Self, ClassifyError> {
        if probabilities.iter().any(|p| !p.is_finite()) {
            return Err(ClassifyError::NonFiniteResponse {
                field: "probabilities",
            });
        }
        if let Some(values) = logits.as_ref() {
            if values.iter().any(|l| !l.is_finite()) {
                return Err(ClassifyError::NonFiniteResponse { field: "logits" });
            }
            if values.len() != probabilities.len() {
                return Err(ClassifyError::LabelCountMismatch {
                    expected: probabilities.len(),
                    got: values.len(),
                });
            }
        }
        if !margin.is_finite() {
            return Err(ClassifyError::NonFiniteResponse { field: "margin" });
        }
        // Mass LAST, and NaN-visible: the finiteness checks above already
        // excluded NaN, so this is defense in depth rather than the primary
        // guard — but it is written so that it would still refuse a NaN if a
        // future refactor moved it ahead of them.
        let mass: f64 = probabilities.iter().sum();
        if !within((mass - 1.0).abs(), PROBABILITY_MASS_ABS_TOLERANCE) {
            return Err(ClassifyError::ProbabilityMassOutOfRange { mass });
        }
        Ok(Self {
            label,
            probabilities,
            logits,
            margin,
            token_count,
            truncated,
        })
    }

    /// The winning label, compared EXACTLY by every downstream gate.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The full ordered probability vector, one entry per ordered label.
    #[must_use]
    pub fn probabilities(&self) -> &[f64] {
        &self.probabilities
    }

    /// The per-class logits, when the request asked for them.
    #[must_use]
    pub fn logits(&self) -> Option<&[f64]> {
        self.logits.as_deref()
    }

    /// Top-1 minus top-2 probability.
    #[must_use]
    pub fn margin(&self) -> f64 {
        self.margin
    }

    /// The number of positions the model actually CONSUMED for this text.
    ///
    /// The attention-mask length after truncation (contract item 11), read off
    /// the tokenized batch — NOT a tokenizer-internal count, which varies with
    /// whether special tokens are counted. Under this definition a truncated
    /// text has `token_count == MAX_SEQUENCE_LENGTH` by construction.
    #[must_use]
    pub fn token_count(&self) -> u32 {
        self.token_count
    }

    /// Whether the input exceeded the pinned truncation bound.
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

/// [`ClassifyResult`]'s private wire form.
///
/// `deny_unknown_fields` rejects unknown KEYS; it says nothing about invalid
/// VALUES — that is what [`ClassifyResult::new`] is for, and routing through it
/// is this type's whole purpose. Field order here IS the serialized order
/// (`serde_json` emits declaration order), so the golden bytes are stable.
///
/// `logits` carries NO `skip_serializing_if`: an unrequested logit vector
/// serializes as an explicit `null`. A missing key is invisible both to a
/// reviewer's diff and to a null-walking guard; an explicit null is loud to
/// both. The adjacent artifact sub-documents are proscribed from the attribute
/// for the same reason.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassifyResultWire {
    label: String,
    probabilities: Vec<f64>,
    logits: Option<Vec<f64>>,
    margin: f64,
    token_count: u32,
    truncated: bool,
}

impl From<ClassifyResult> for ClassifyResultWire {
    fn from(value: ClassifyResult) -> Self {
        Self {
            label: value.label,
            probabilities: value.probabilities,
            logits: value.logits,
            margin: value.margin,
            token_count: value.token_count,
            truncated: value.truncated,
        }
    }
}

impl TryFrom<ClassifyResultWire> for ClassifyResult {
    type Error = ClassifyError;

    fn try_from(wire: ClassifyResultWire) -> Result<Self, Self::Error> {
        Self::new(
            wire.label,
            wire.probabilities,
            wire.logits,
            wire.margin,
            wire.token_count,
            wire.truncated,
        )
    }
}

// ---------------------------------------------------------------------------
// The envelope
// ---------------------------------------------------------------------------

/// The classification envelope OPS-04 specifies.
///
/// Every field is private; [`Self::new`] is the only constructor and it
/// validates. `Deserialize` routes through the same validation via the private
/// [`ClassifyResponseWire`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(into = "ClassifyResponseWire", try_from = "ClassifyResponseWire")]
pub struct ClassifyResponse {
    schema_version: u32,
    artifact_sha256: String,
    backend: String,
    latency_ms: f64,
    results: Vec<ClassifyResult>,
}

impl ClassifyResponse {
    /// The validating constructor — the only way a `ClassifyResponse` exists.
    ///
    /// `schema_version` is not a parameter: it is [`CLASSIFY_SCHEMA_VERSION`],
    /// so no caller can mint a response claiming a version this build does not
    /// implement.
    ///
    /// # Errors
    ///
    /// [`ClassifyError::NonFiniteResponse`] for a non-finite `latency_ms`,
    /// [`ClassifyError::NegativeLatency`] for a negative one, and
    /// [`ClassifyError::LabelCountMismatch`] when two result rows disagree on
    /// their probability arity — two rows of different width describe two
    /// different label sets, which no single response can be about.
    pub fn new(
        artifact_sha256: String,
        backend: String,
        latency_ms: f64,
        results: Vec<ClassifyResult>,
    ) -> Result<Self, ClassifyError> {
        if !latency_ms.is_finite() {
            return Err(ClassifyError::NonFiniteResponse {
                field: "latency_ms",
            });
        }
        if latency_ms < 0.0 {
            return Err(ClassifyError::NegativeLatency { got: latency_ms });
        }
        if let Some(first) = results.first() {
            let expected = first.probabilities.len();
            for result in &results {
                if result.probabilities.len() != expected {
                    return Err(ClassifyError::LabelCountMismatch {
                        expected,
                        got: result.probabilities.len(),
                    });
                }
            }
        }
        Ok(Self {
            schema_version: CLASSIFY_SCHEMA_VERSION,
            artifact_sha256,
            backend,
            latency_ms,
            results,
        })
    }

    /// The envelope's schema version.
    #[must_use]
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// The artifact this response was produced from.
    ///
    /// Present on every response from every surface, so Phase 5 can attribute a
    /// measurement to an artifact without trusting the caller's bookkeeping.
    #[must_use]
    pub fn artifact_sha256(&self) -> &str {
        &self.artifact_sha256
    }

    /// The execution-derived backend identity (D-12).
    ///
    /// Produced by the encode invocation that RAN — see
    /// [`crate::setfit::encoder::ExecutionBackend`]. There is no parameter, no
    /// setter and no configuration path that reaches this field.
    #[must_use]
    pub fn backend(&self) -> &str {
        &self.backend
    }

    /// Wall-clock milliseconds around the compute.
    ///
    /// A MEASUREMENT, not an identity: it is excluded from every cross-surface
    /// equality comparison, and NO gate may assert it is strictly positive. A
    /// fast operation under a coarse timer legitimately reports `0.0`, so a
    /// `> 0` assertion is a flake with a schedule.
    #[must_use]
    pub fn latency_ms(&self) -> f64 {
        self.latency_ms
    }

    /// The per-text results, in input order.
    #[must_use]
    pub fn results(&self) -> &[ClassifyResult] {
        &self.results
    }
}

/// [`ClassifyResponse`]'s private wire form. See [`ClassifyResultWire`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassifyResponseWire {
    schema_version: u32,
    artifact_sha256: String,
    backend: String,
    latency_ms: f64,
    results: Vec<ClassifyResultWire>,
}

impl From<ClassifyResponse> for ClassifyResponseWire {
    fn from(value: ClassifyResponse) -> Self {
        Self {
            schema_version: value.schema_version,
            artifact_sha256: value.artifact_sha256,
            backend: value.backend,
            latency_ms: value.latency_ms,
            results: value.results.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<ClassifyResponseWire> for ClassifyResponse {
    type Error = ClassifyError;

    fn try_from(wire: ClassifyResponseWire) -> Result<Self, Self::Error> {
        if wire.schema_version != CLASSIFY_SCHEMA_VERSION {
            return Err(ClassifyError::UnsupportedSchemaVersion {
                expected: CLASSIFY_SCHEMA_VERSION,
                got: wire.schema_version,
            });
        }
        let results = wire
            .results
            .into_iter()
            .map(ClassifyResult::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(wire.artifact_sha256, wire.backend, wire.latency_ms, results)
    }
}

// ---------------------------------------------------------------------------
// The one comparison helper
// ---------------------------------------------------------------------------

/// Whether `delta` is inside `bound`, with an INCOMPARABLE delta counting as
/// outside.
///
/// Written through `partial_cmp` rather than as `delta <= bound` so the NaN case
/// is VISIBLE rather than implied. `delta <= bound` happens to reject NaN, but
/// it refactors into `!(delta > bound)` — which ACCEPTS NaN silently, because
/// every comparison with NaN is false. The `partial_cmp` form cannot be
/// refactored into acceptance by accident.
fn within(delta: f64, bound: f64) -> bool {
    matches!(
        delta.partial_cmp(&bound),
        Some(core::cmp::Ordering::Less | core::cmp::Ordering::Equal)
    )
}

// ===========================================================================
// Test modules
//
// They live DIRECTLY in this file, not behind a `#[path]` include, because the
// three task filters name `setfit::classify::envelope`,
// `setfit::classify::backend` and `setfit::classify::classify_path`. A
// `#[path = "classify_tests.rs"] mod classify_tests;` wrapper would insert a
// segment and every one of those filters would select ZERO tests and still exit
// 0 — the CR-02 vacuous pass that bit plan 04-13's `bundle_nullable` filter.
// ===========================================================================

/// This file's PRODUCTION source, with the test modules cut off.
///
/// A source assertion must not scan its own needle. A guard greping for
/// `pub struct ...Wire` whose own text contains that literal can NEVER turn
/// green, and one greping for an attribute it also quotes passes vacuously on
/// its own text. Cutting at the first test module removes both failure modes at
/// once, and it is the same class of defect as orchestrator note F-05 (a
/// `skip_serializing_if` gate turning red on its own documentation) — which this
/// module's first run reproduced exactly.
#[cfg(test)]
fn production_source() -> &'static str {
    const SRC: &str = include_str!("classify.rs");
    // The cut is the banner ABOVE this function, so this function and all three
    // test modules fall outside it. `find` returns the FIRST occurrence, which
    // is the banner — not this literal, which lives below it.
    // `production_source_excludes_the_test_modules` pins that down rather than
    // assuming it.
    let cut = SRC.find("// Test modules").unwrap_or(SRC.len());
    &SRC[..cut]
}

/// The ONE model every classify suite runs against.
///
/// Built from `artifact::fixture`'s view through the REAL writer and the REAL
/// fail-closed loader, so these suites exercise a `VerifiedSetFitModel` that
/// passed all seven rungs — not a hand-assembled stand-in that skipped them.
#[cfg(test)]
fn fixture_verified_model() -> crate::setfit::artifact::VerifiedSetFitModel {
    let view = crate::setfit::artifact::fixture::fixture_view_full_pin_shape();
    let bytes =
        crate::setfit::artifact::write_setfit_apr(&view).expect("the fixture view is writable");
    crate::setfit::artifact::load_setfit_apr(&bytes).expect("the fixture artifact verifies")
}

/// The same fixture's encoder half, for the suites that need `SetFitMiniLm`
/// directly rather than through the verified typestate.
#[cfg(test)]
fn fixture_encoder_model() -> crate::setfit::SetFitMiniLm {
    let view = crate::setfit::artifact::fixture::fixture_view_full_pin_shape();
    crate::setfit::SetFitMiniLm::from_bundle_parts(
        &view.tokenizer_bytes,
        &view.architecture,
        view.tensors.clone(),
        view.root_seed,
    )
    .expect("the fixture parts rebuild a model")
}

/// Task 1: the envelope's invariants, on every path in and out.
#[cfg(test)]
mod envelope {
    use super::*;

    /// The exact bytes a fixture response serializes to.
    ///
    /// A committed LITERAL, not a re-serialization of the same struct: comparing
    /// a struct against itself proves only that serde is deterministic. This
    /// compares against bytes a human reviewed, so any field rename, reorder or
    /// removal is a loud diff in the D-08 schema.
    const GOLDEN_RESPONSE_JSON: &str = concat!(
        r#"{"schema_version":1,"#,
        r#""artifact_sha256":"9f2c7a1d4e8b60315a7c9e0d2f4b6813a5c7e9f1b3d50729468a0c2e4f6a8b1d","#,
        r#""backend":"cpu:setfit-core:fixture-kernel","#,
        r#""latency_ms":0.0,"#,
        r#""results":["#,
        r#"{"label":"positive","probabilities":[0.25,0.75],"logits":null,"#,
        r#""margin":0.5,"token_count":7,"truncated":false},"#,
        r#"{"label":"negative","probabilities":[0.875,0.125],"logits":null,"#,
        r#""margin":0.75,"token_count":5,"truncated":false}"#,
        r#"]}"#,
    );

    const GOLDEN_SHA: &str = "9f2c7a1d4e8b60315a7c9e0d2f4b6813a5c7e9f1b3d50729468a0c2e4f6a8b1d";

    /// A FIXTURE backend value, deliberately NOT the real v1 identity.
    ///
    /// This golden pins the SCHEMA — field names, declaration order, exact bytes
    /// — not the identity. Writing the real `<device>:setfit-core:<kernel>`
    /// value here would put the kernel literal in this file, and the D-12 gate
    /// requires that literal to live in `encoder.rs` and nowhere else: the
    /// identity must arrive as a VALUE returned by the encode call, never as a
    /// string this module knows how to spell.
    const GOLDEN_BACKEND: &str = "cpu:setfit-core:fixture-kernel";

    fn golden_response() -> ClassifyResponse {
        let a = ClassifyResult::new("positive".into(), vec![0.25, 0.75], None, 0.5, 7, false)
            .expect("fixture result a is valid");
        let b = ClassifyResult::new("negative".into(), vec![0.875, 0.125], None, 0.75, 5, false)
            .expect("fixture result b is valid");
        ClassifyResponse::new(GOLDEN_SHA.into(), GOLDEN_BACKEND.into(), 0.0, vec![a, b])
            .expect("fixture response is valid")
    }

    fn one_result(probabilities: Vec<f64>) -> Result<ClassifyResult, ClassifyError> {
        ClassifyResult::new("positive".into(), probabilities, None, 0.5, 7, false)
    }

    #[test]
    fn production_source_excludes_the_test_modules() {
        // The cut itself is load-bearing for five assertions below, so it is
        // asserted rather than assumed.
        let src = production_source();
        assert!(
            src.contains("pub struct ClassifyResponse {"),
            "the production half must still contain the declarations being scanned"
        );
        assert!(
            !src.contains("fn production_source"),
            "the cut must remove this test module, or every source assertion scans its own text"
        );
    }

    #[test]
    fn serialized_response_carries_exactly_the_contract_field_names() {
        let json = serde_json::to_string(&golden_response()).expect("serializes");
        let value: serde_json::Value = serde_json::from_str(&json).expect("reparses");
        let obj = value.as_object().expect("an object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "artifact_sha256",
                "backend",
                "latency_ms",
                "results",
                "schema_version"
            ],
            "the response's field set is the D-08 contract's field set"
        );

        let results = obj["results"].as_array().expect("results is an array");
        assert!(
            !results.is_empty(),
            "the fixture carries results to inspect"
        );
        for result in results {
            let robj = result.as_object().expect("a result object");
            let mut rkeys: Vec<&str> = robj.keys().map(String::as_str).collect();
            rkeys.sort_unstable();
            assert_eq!(
                rkeys,
                vec![
                    "label",
                    "logits",
                    "margin",
                    "probabilities",
                    "token_count",
                    "truncated"
                ],
                "each result's field set is the D-08 contract's field set"
            );
        }
    }

    #[test]
    fn serialized_response_matches_the_committed_golden_bytes() {
        let json = serde_json::to_string(&golden_response()).expect("serializes");
        assert_eq!(
            json, GOLDEN_RESPONSE_JSON,
            "the serialized envelope drifted from the committed golden"
        );
    }

    #[test]
    fn constructor_rejects_a_non_finite_probability() {
        let err = ClassifyResult::new("positive".into(), vec![f64::NAN, 0.5], None, 0.5, 7, false)
            .expect_err("NaN in probabilities must be refused");
        assert!(
            matches!(
                err,
                ClassifyError::NonFiniteResponse {
                    field: "probabilities"
                }
            ),
            "expected NonFiniteResponse{{probabilities}}, got {err:?}"
        );
    }

    #[test]
    fn constructor_rejects_a_non_finite_logit() {
        let err = ClassifyResult::new(
            "positive".into(),
            vec![0.25, 0.75],
            Some(vec![f64::INFINITY, 0.0]),
            0.5,
            7,
            false,
        )
        .expect_err("infinity in logits must be refused");
        assert!(
            matches!(err, ClassifyError::NonFiniteResponse { field: "logits" }),
            "expected NonFiniteResponse{{logits}}, got {err:?}"
        );
    }

    #[test]
    fn constructor_rejects_a_non_finite_margin() {
        let err = ClassifyResult::new(
            "positive".into(),
            vec![0.25, 0.75],
            None,
            f64::NEG_INFINITY,
            7,
            false,
        )
        .expect_err("a non-finite margin must be refused");
        assert!(
            matches!(err, ClassifyError::NonFiniteResponse { field: "margin" }),
            "expected NonFiniteResponse{{margin}}, got {err:?}"
        );
    }

    #[test]
    fn constructor_rejects_a_non_finite_latency() {
        let ok = one_result(vec![0.25, 0.75]).expect("a valid result");
        let err =
            ClassifyResponse::new(GOLDEN_SHA.into(), GOLDEN_BACKEND.into(), f64::NAN, vec![ok])
                .expect_err("NaN latency must be refused");
        assert!(
            matches!(
                err,
                ClassifyError::NonFiniteResponse {
                    field: "latency_ms"
                }
            ),
            "expected NonFiniteResponse{{latency_ms}}, got {err:?}"
        );
    }

    #[test]
    fn constructor_rejects_a_negative_latency() {
        let ok = one_result(vec![0.25, 0.75]).expect("a valid result");
        let err = ClassifyResponse::new(GOLDEN_SHA.into(), GOLDEN_BACKEND.into(), -1.0, vec![ok])
            .expect_err("a negative latency must be refused");
        assert!(
            matches!(err, ClassifyError::NegativeLatency { .. }),
            "expected NegativeLatency, got {err:?}"
        );
    }

    #[test]
    fn a_zero_latency_is_legal() {
        // The contract forbids any gate from asserting latency > 0: a fast
        // operation under a coarse timer legitimately reports exactly 0.
        let ok = one_result(vec![0.25, 0.75]).expect("a valid result");
        let response =
            ClassifyResponse::new(GOLDEN_SHA.into(), GOLDEN_BACKEND.into(), 0.0, vec![ok])
                .expect("zero latency is legal");
        assert!(response.latency_ms().is_finite() && response.latency_ms() >= 0.0);
    }

    #[test]
    fn probability_mass_must_be_one_within_tolerance() {
        let err = one_result(vec![0.25, 0.25]).expect_err("mass 0.5 must be refused");
        assert!(
            matches!(err, ClassifyError::ProbabilityMassOutOfRange { .. }),
            "expected ProbabilityMassOutOfRange, got {err:?}"
        );
        one_result(vec![0.25, 0.75]).expect("mass 1.0 is accepted");
        // Inside tolerance, so accepted; the bound is absolute, not relative.
        one_result(vec![0.25, 0.75 + 5e-7]).expect("mass within 1e-6 is accepted");
    }

    #[test]
    fn logits_arity_must_match_probabilities() {
        let err = ClassifyResult::new(
            "positive".into(),
            vec![0.25, 0.75],
            Some(vec![1.0]),
            0.5,
            7,
            false,
        )
        .expect_err("a 1-entry logit vector against 2 labels must be refused");
        assert!(
            matches!(
                err,
                ClassifyError::LabelCountMismatch {
                    expected: 2,
                    got: 1
                }
            ),
            "expected LabelCountMismatch{{2,1}}, got {err:?}"
        );
    }

    #[test]
    fn every_result_row_must_have_the_same_label_arity() {
        let two = one_result(vec![0.25, 0.75]).expect("valid");
        let three = ClassifyResult::new(
            "negative".into(),
            vec![0.25, 0.25, 0.5],
            None,
            0.25,
            5,
            false,
        )
        .expect("valid on its own");
        let err = ClassifyResponse::new(
            GOLDEN_SHA.into(),
            GOLDEN_BACKEND.into(),
            0.0,
            vec![two, three],
        )
        .expect_err("rows of differing arity describe two different label sets");
        assert!(
            matches!(
                err,
                ClassifyError::LabelCountMismatch {
                    expected: 2,
                    got: 3
                }
            ),
            "expected LabelCountMismatch{{2,3}}, got {err:?}"
        );
    }

    #[test]
    fn deserializing_a_null_probability_is_refused() {
        let body = GOLDEN_RESPONSE_JSON.replace("[0.25,0.75]", "[null,0.75]");
        assert!(
            serde_json::from_str::<ClassifyResponse>(&body).is_err(),
            "a null probability must not deserialize into a ClassifyResponse"
        );
    }

    #[test]
    fn deserializing_a_negative_latency_is_refused_typed() {
        // The TYPED assertion, on the conversion the Deserialize impl routes
        // through. Asserting only on serde_json's message would be a substring
        // test that a reworded error silently turns green.
        let wire = ClassifyResponseWire {
            schema_version: CLASSIFY_SCHEMA_VERSION,
            artifact_sha256: GOLDEN_SHA.into(),
            backend: GOLDEN_BACKEND.into(),
            latency_ms: -0.5,
            results: vec![ClassifyResultWire {
                label: "positive".into(),
                probabilities: vec![0.25, 0.75],
                logits: None,
                margin: 0.5,
                token_count: 7,
                truncated: false,
            }],
        };
        let err = ClassifyResponse::try_from(wire).expect_err("a negative latency is refused");
        assert!(
            matches!(err, ClassifyError::NegativeLatency { .. }),
            "expected NegativeLatency, got {err:?}"
        );

        // And the serde door genuinely uses that conversion.
        let body = GOLDEN_RESPONSE_JSON.replace(r#""latency_ms":0.0"#, r#""latency_ms":-0.5"#);
        assert!(
            serde_json::from_str::<ClassifyResponse>(&body).is_err(),
            "Deserialize must route through the validating constructor"
        );
    }

    #[test]
    fn deserializing_a_non_finite_probability_is_refused_typed() {
        let wire = ClassifyResultWire {
            label: "positive".into(),
            probabilities: vec![f64::NAN, 0.75],
            logits: None,
            margin: 0.5,
            token_count: 7,
            truncated: false,
        };
        let err = ClassifyResult::try_from(wire).expect_err("NaN is refused on the wire path too");
        assert!(
            matches!(
                err,
                ClassifyError::NonFiniteResponse {
                    field: "probabilities"
                }
            ),
            "expected NonFiniteResponse{{probabilities}}, got {err:?}"
        );
    }

    #[test]
    fn deserializing_an_unknown_schema_version_is_refused_typed() {
        let wire = ClassifyResponseWire {
            schema_version: 2,
            artifact_sha256: GOLDEN_SHA.into(),
            backend: GOLDEN_BACKEND.into(),
            latency_ms: 0.0,
            results: Vec::new(),
        };
        let err = ClassifyResponse::try_from(wire).expect_err("a v2 payload is not a v1 response");
        assert!(
            matches!(
                err,
                ClassifyError::UnsupportedSchemaVersion {
                    expected: 1,
                    got: 2
                }
            ),
            "expected UnsupportedSchemaVersion{{1,2}}, got {err:?}"
        );
    }

    #[test]
    fn deserializing_an_unknown_response_field_is_refused() {
        let body =
            GOLDEN_RESPONSE_JSON.replace(r#""latency_ms":0.0"#, r#""latency_ms":0.0,"tps":9"#);
        assert!(
            serde_json::from_str::<ClassifyResponse>(&body).is_err(),
            "deny_unknown_fields must refuse a key this schema does not model"
        );
    }

    #[test]
    fn a_valid_response_round_trips_through_serde() {
        let original = golden_response();
        let json = serde_json::to_string(&original).expect("serializes");
        let back: ClassifyResponse = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back, original, "the envelope round-trips unchanged");
    }

    #[test]
    fn logits_key_is_present_and_null_when_not_requested() {
        // "Absent" is an explicit `null`, NOT a missing key. A missing key is
        // invisible to a diff and to a null-walking guard; an explicit null is
        // loud to both.
        let json = serde_json::to_string(&golden_response()).expect("serializes");
        assert!(
            json.contains(r#""logits":null"#),
            "an unrequested logits vector must serialize as an explicit null: {json}"
        );
        // The ATTRIBUTE form, not the bare token. The bare token appears in this
        // module's own doc comments explaining why the attribute is forbidden,
        // so a gate on it would turn red on its own documentation — orchestrator
        // note F-05, observed here on the first run of this very assertion.
        assert!(
            !production_source().contains("serde(skip_serializing_if"),
            "skip_serializing_if would make the absent-logits case invisible to a diff"
        );
    }

    #[test]
    fn requested_logits_serialize_as_an_array() {
        let result = ClassifyResult::new(
            "positive".into(),
            vec![0.25, 0.75],
            Some(vec![-0.5, 0.5]),
            0.5,
            7,
            false,
        )
        .expect("valid");
        let response =
            ClassifyResponse::new(GOLDEN_SHA.into(), GOLDEN_BACKEND.into(), 0.0, vec![result])
                .expect("valid");
        let json = serde_json::to_string(&response).expect("serializes");
        assert!(
            json.contains(r#""logits":[-0.5,0.5]"#),
            "requested logits serialize in place: {json}"
        );
    }

    #[test]
    fn request_document_round_trips_awkward_texts_byte_identically() {
        let doc = ClassifyRequestDocument {
            texts: vec![
                "a line\nand another".to_string(),
                "a\ttab".to_string(),
                String::new(),
                "café — 日本語 🌍".to_string(),
            ],
            include_logits: true,
        };
        let json = serde_json::to_string(&doc).expect("serializes");
        let back: ClassifyRequestDocument = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(
            back, doc,
            "the shared request document must survive newlines, tabs, empties and non-ASCII"
        );
        // The defect this design exists to prevent: a line-delimited CLI format
        // would split the first text into two, so the CLI and HTTP surfaces
        // would receive DIFFERENT ordered input sets while appearing to agree.
        assert_eq!(back.texts.len(), 4, "four texts in, four texts out");
        assert_eq!(
            back.texts[0], "a line\nand another",
            "the embedded newline survives verbatim"
        );
    }

    #[test]
    fn request_document_defaults_include_logits_to_false() {
        let doc: ClassifyRequestDocument =
            serde_json::from_str(r#"{"texts":["hello"]}"#).expect("deserializes");
        assert!(!doc.include_logits, "include_logits defaults to false");
        assert!(!ClassifyRequestDocument::new(["hello"]).include_logits);
        assert!(
            ClassifyRequestDocument::new(["hello"])
                .with_logits()
                .include_logits
        );
    }

    #[test]
    fn request_document_rejects_an_unknown_field() {
        let err = serde_json::from_str::<ClassifyRequestDocument>(
            r#"{"texts":["hello"],"temperature":0.7}"#,
        )
        .expect_err("deny_unknown_fields refuses a field this schema does not model");
        assert!(
            err.to_string().contains("temperature"),
            "the rejection should name the unknown field: {err}"
        );
    }

    #[test]
    fn response_and_result_fields_are_private() {
        let src = production_source();
        for (marker, name) in [
            ("pub struct ClassifyResponse {", "ClassifyResponse"),
            ("pub struct ClassifyResult {", "ClassifyResult"),
        ] {
            let start = src
                .find(marker)
                .unwrap_or_else(|| panic!("{name} declaration not found"));
            let body = &src[start + marker.len()..];
            let end = body
                .find("\n}")
                .unwrap_or_else(|| panic!("{name} body not closed"));
            let block = &body[..end];
            assert!(
                !block.contains("pub "),
                "{name} must have NO public field — a public field falsifies \
                 \"a non-finite value is unrepresentable\" (review M1). Body was:\n{block}"
            );
        }
    }

    #[test]
    fn both_serde_directions_route_through_the_validating_constructor() {
        let src = production_source();
        assert!(
            src.contains(r#"try_from = "ClassifyResponseWire""#),
            "ClassifyResponse must deserialize via the validating wire conversion"
        );
        assert!(
            src.contains(r#"try_from = "ClassifyResultWire""#),
            "ClassifyResult must deserialize via the validating wire conversion"
        );
        assert!(
            src.contains(r#"into = "ClassifyResponseWire""#),
            "ClassifyResponse must serialize via the same wire form it parses"
        );
        assert!(
            !src.contains("pub struct ClassifyResponseWire"),
            "the wire struct must be private, or a caller could build one and skip validation"
        );
        assert!(
            !src.contains("pub struct ClassifyResultWire"),
            "the wire struct must be private, or a caller could build one and skip validation"
        );
    }

    #[test]
    fn within_is_nan_visible_in_both_argument_positions() {
        assert!(
            within(0.5, 1.0),
            "a finite delta inside the bound is inside"
        );
        assert!(within(1.0, 1.0), "the bound itself is inside");
        assert!(
            !within(1.5, 1.0),
            "a finite delta outside the bound is outside"
        );
        assert!(!within(f64::NAN, 1.0), "a NaN delta counts as OUTSIDE");
        assert!(!within(0.5, f64::NAN), "a NaN bound counts as OUTSIDE");
        assert!(
            !within(f64::from(f32::NAN), 1.0),
            "an f32 NaN widened to f64 counts as OUTSIDE"
        );
        // The idiom itself, not only its current behaviour: `delta <= bound`
        // happens to reject NaN, but refactors into `!(delta > bound)`, which
        // ACCEPTS it. `partial_cmp` cannot be refactored into acceptance.
        let src = production_source();
        let start = src.find("fn within(").expect("within is defined here");
        let body = &src[start..];
        let end = body.find("\n}").expect("within's body is closed");
        assert!(
            body[..end].contains("partial_cmp"),
            "within must be written through partial_cmp so the NaN case is visible"
        );
    }
}

/// Task 2: the execution-derived backend identity channel (D-12, review B6).
///
/// These suites live here rather than in `encoder.rs` because they exercise only
/// the re-exported surface — `SetFitMiniLm::encode_texts_traced` and
/// `ExecutionBackend::identity` — and because the plan fixes
/// `setfit::classify::backend` as this task's filter. No case here needs
/// encoder-private state.
#[cfg(test)]
mod backend {
    use super::*;

    /// Item 12 of the contract, read from the contract rather than from a copy
    /// of it. `include_str!` and not a runtime read: a missing contract is a
    /// COMPILE error here, where a runtime read would be a silent skip.
    const CONTRACT: &str = include_str!("../../../../contracts/setfit-apr-v1.yaml");

    /// Every `.rs` file in this directory, read at test time.
    ///
    /// `read_dir` and not a hardcoded list: a hardcoded list goes stale the
    /// moment a file is added, and a file added later is exactly where a
    /// capability-detection symbol would arrive unnoticed.
    fn setfit_sources() -> Vec<(String, String)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/setfit");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("the setfit source directory is readable") {
            let path = entry.expect("a readable directory entry").path();
            if path.extension().and_then(std::ffi::OsStr::to_str) == Some("rs") {
                let name = path
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .expect("a UTF-8 file name")
                    .to_string();
                let body = std::fs::read_to_string(&path).expect("a readable source file");
                out.push((name, body));
            }
        }
        // A non-zero MINIMUM, because a scan of zero files exits green and
        // proves nothing (orchestrator note F-04).
        assert!(
            out.len() >= 10,
            "expected at least 10 setfit source files, found {}: a path that resolved to \
             nothing would make every scan below vacuous",
            out.len()
        );
        out
    }

    #[test]
    fn encode_texts_traced_returns_the_identity_the_contract_pins() {
        let model = fixture_encoder_model();
        let (_, backend) = model
            .encode_texts_traced(&["hello world"])
            .expect("the fixture encodes");
        let identity = backend.identity();
        // Pinned against the CONTRACT's own text, not against a literal copied
        // into this file: a copy can drift from the contract silently, and the
        // D-12 gate additionally requires the kernel literal to appear nowhere
        // in this module.
        let pin = format!("v1 = \"{identity}\"");
        assert!(
            CONTRACT.contains(&pin),
            "the observed identity {identity:?} is not the value item 12 pins ({pin:?})"
        );
    }

    #[test]
    fn the_identity_carries_no_simd_capability_token() {
        let model = fixture_encoder_model();
        let (_, backend) = model
            .encode_texts_traced(&["hello world"])
            .expect("the fixture encodes");
        let identity = backend.identity().to_lowercase();
        for token in ["avx", "neon", "sse", "512"] {
            assert!(
                !identity.contains(token),
                "the identity {identity:?} names the capability token {token:?}. Detecting \
                 that a SIMD ISA is AVAILABLE is consistent with a SCALAR execution — \
                 trueno's Matrix::matmul dispatches on SIZE — so such a value describes the \
                 HOST, not the run (CLAUDE.md Verification Discipline rule 2)"
            );
        }
    }

    #[test]
    fn the_identity_has_exactly_three_colon_separated_segments() {
        let model = fixture_encoder_model();
        let (_, backend) = model
            .encode_texts_traced(&["hello world"])
            .expect("the fixture encodes");
        let identity = backend.identity();
        let segments: Vec<&str> = identity.split(':').collect();
        assert_eq!(
            segments.len(),
            3,
            "the grammar is <device>:<implementation>:<kernel> and stays three segments, so a \
             future GPU identity is COMPARABLE to this one: {identity:?}"
        );
        assert!(
            segments.iter().all(|s| !s.is_empty()),
            "no segment may be empty: {identity:?}"
        );
        assert_eq!(segments[0], backend.device());
        assert_eq!(segments[2], backend.kernel());
    }

    #[test]
    fn execution_backend_has_no_public_constructor_or_setter() {
        let src = include_str!("encoder.rs");
        let marker = "pub struct ExecutionBackend {";
        let start = src.find(marker).expect("ExecutionBackend is declared here");
        let body = &src[start + marker.len()..];
        let end = body.find("\n}").expect("its body is closed");
        assert!(
            !body[..end].contains("pub "),
            "ExecutionBackend must have no public field, or a caller could forge an identity"
        );

        let impl_marker = "impl ExecutionBackend {";
        let istart = src
            .find(impl_marker)
            .expect("ExecutionBackend has an inherent impl");
        let ibody = &src[istart + impl_marker.len()..];
        let iend = ibody.find("\n}").expect("the impl block is closed");
        let block = &ibody[..iend];
        for forbidden in ["pub fn new", "pub const fn new", "&mut self", "-> Self"] {
            assert!(
                !block.contains(forbidden),
                "ExecutionBackend's public impl must contain no {forbidden:?}: a constructor \
                 or a setter would let a value be MINTED rather than RETURNED by the encode \
                 invocation that ran (D-12)"
            );
        }
        assert!(
            !src.contains("pub const ENCODE_BACKEND")
                && !src.contains("pub(crate) const ENCODE_BACKEND"),
            "the constant instance stays module-private; only encode_with_backend hands it out"
        );
    }

    #[test]
    fn the_setfit_surface_names_no_capability_detection_symbol() {
        // The needles are assembled from fragments so this test's OWN text does
        // not contain them — otherwise scanning `classify.rs` would make the
        // guard permanently red, the F-05 failure mode. `concat!` is expanded at
        // compile time, so the comparison is against the whole symbol.
        let needles = [
            concat!("select_", "backend"),
            concat!("Backend::", "AVX"),
            concat!("detect_x86_", "backend"),
            concat!("detect_arm_", "backend"),
        ];
        let sources = setfit_sources();
        for (name, body) in &sources {
            for needle in needles {
                assert!(
                    !body.contains(needle),
                    "{name} names {needle:?}. A capability probe reports what the HOST CAN DO; \
                     the backend field must report what RAN (review B6, D-12)"
                );
            }
        }
        // The scan is proven able to fire, on a string it WOULD reject.
        let planted = format!("let b = {};", needles[1]);
        assert!(
            needles.iter().any(|n| planted.contains(n)),
            "the needle set must match a planted violation, or this gate is theater"
        );
    }

    #[test]
    fn the_kernel_literal_lives_only_in_encoder_rs() {
        // Assembled from fragments for the same reason as above: written whole,
        // this test would itself be a second home for the literal.
        let kernel = concat!("autograd-", "trueno-matmul");
        let sources = setfit_sources();
        let mut carriers: Vec<&str> = Vec::new();
        for (name, body) in &sources {
            if body.contains(kernel) {
                carriers.push(name.as_str());
            }
        }
        assert_eq!(
            carriers,
            vec!["encoder.rs"],
            "the kernel identity must be spelled in encoder.rs and nowhere else in this \
             directory — classify.rs included. It arrives everywhere else as a VALUE returned \
             by the encode call (review B6)"
        );
    }

    #[test]
    fn the_traced_and_untraced_encode_paths_agree_elementwise() {
        // `encode`/`encode_texts` were extended BESIDE, not modified: the
        // training path and every Phase 1 conformance fixture call them, and
        // their output must not have moved because a reporting channel was
        // added. Comparing the two paths' data is the behavioural half of that
        // claim; `git diff` on encoder.rs is the textual half.
        let model = fixture_encoder_model();
        let texts = ["hello world", "a much longer sentence for the batch"];
        let plain = model.encode_texts(&texts).expect("the untraced path runs");
        let (traced, _) = model
            .encode_texts_traced(&texts)
            .expect("the traced path runs");
        assert_eq!(plain.shape(), traced.shape(), "same shape");
        assert_eq!(
            plain.data(),
            traced.data(),
            "the traced path must return the SAME embeddings, bit for bit"
        );
    }
}
