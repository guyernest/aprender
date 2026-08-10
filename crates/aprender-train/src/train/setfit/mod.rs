//! The two-stage SetFit trainer (D-05), and its lifecycle as a phantom typestate (D-06).
//!
//! Contract: `setfit-train-lifecycle-v1` (authored in plan 03-06).
//! Requirements: TRN-01 (the trainer's home and lifecycle), TRN-02 (configuration).
//!
//! # The lifecycle is a TYPE PARAMETER, never a runtime field
//!
//! `SetFitRun<Prepared>`, `<EncoderTuned>`, `<HeadFitted>` and
//! `<ArtifactReloadedAndVerified>` are four distinct types, and each transition consumes
//! `self` and returns the next one. An illegal ordering — fitting the head before the
//! encoder is tuned, exporting before the artifact round-trips — is therefore not
//! *rejected*, it is **inexpressible**. That is the same argument Phase 2 used for
//! `Split<Train>` and `PreparedDataset<Canonical>`, and it is what makes plan 03-09's
//! trybuild non-constructibility proof possible at all: proving a runtime field is always
//! in the right state needs whole-program reasoning; proving a type does not exist is a
//! compiler error.
//!
//! # Why the state trait is SEALED
//!
//! [`LifecycleState`] has a private supertrait, so no out-of-crate type can implement it.
//! Without the seal a caller could declare its own marker, implement the trait, and mint a
//! `SetFitRun` in a state this crate never defined — which is exactly the spoofing route
//! (T-3-09) the typestate exists to close.
//!
//! # Interface-first: four markers, one impl
//!
//! All four markers are declared HERE, in the plan that opens the module, because
//! everything wave 2 builds programs against them. Only [`Prepared`] implements
//! [`LifecycleState`] in this plan: a state's `Evidence` associated type is the evidence
//! its transition produces, and those evidence types land with their transitions
//! (03-05 for `EncoderTuned`, 03-07 for `HeadFitted`, 03-08 for
//! `ArtifactReloadedAndVerified`). A marker without an impl is a declared contract; a
//! marker with an impl and a placeholder evidence type would be a lie that compiles.

pub mod baseline;
pub mod config;
pub mod epoch;
pub mod evidence;
pub mod reduce;
pub mod thresholds;
pub mod tune;

/// The deterministic, network-free, synthetic-text fixture every Phase 3 trainer test uses.
///
/// `#[cfg(test)]` and nothing weaker: 03-10's acceptance criteria reject a `#[doc(hidden)]`
/// test-support door on the shipped surface.
#[cfg(test)]
pub(crate) mod test_fixtures;

use core::fmt;
use core::marker::PhantomData;

use aprender::setfit::SetFitMiniLm;
use aprender_contrastive_data::pairs::resolve_budget;
use aprender_contrastive_data::prepared::{Canonical, PreparedDataset};
use aprender_contrastive_data::select::Selection;
use aprender_contrastive_data::ContrastiveDataError;

use crate::train::device::{Device, DeviceError};
use config::{ResolvedSetFitConfig, SetFitConfigError, SetFitTrainConfig};

/// The seal. Private module, public-in-private trait — the standard Rust sealing idiom.
mod sealed {
    /// Implemented only by this crate's four lifecycle markers.
    pub trait Sealed {}
}

/// One state of the SetFit training lifecycle.
///
/// Sealed: the four markers below are the complete, closed set.
pub trait LifecycleState: sealed::Sealed {
    /// The evidence this state carries.
    ///
    /// An ASSOCIATED TYPE rather than a field on `SetFitRun`, so a state that has no
    /// evidence has no evidence *field* — not an `Option` that every accessor has to
    /// `expect` on. Phase 2's `DatasetProfile::Splits` established this exact shape and
    /// the reason travels with it: an absent field beats `Option` + `expect`, and it is
    /// what makes the non-constructibility proof provable.
    type Evidence: fmt::Debug;

    /// The state's name, for error messages and provenance records.
    const STATE: &'static str;
}

/// Inputs validated, device probed, nothing trained yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prepared;

/// The contrastive stage has run AND its SetFit-identity evidence passed (D-11).
///
/// Declared here, implemented in plan 03-05 together with the evidence type it carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncoderTuned;

/// The multiclass head has been fitted on each unique selected row exactly once (D-08).
///
/// Declared here, implemented in plan 03-07.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeadFitted;

/// The artifact has been closed, reloaded from bytes and re-verified (D-07).
///
/// Declared here, implemented in plan 03-08.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactReloadedAndVerified;

impl sealed::Sealed for Prepared {}
impl sealed::Sealed for EncoderTuned {}
impl sealed::Sealed for HeadFitted {}
impl sealed::Sealed for ArtifactReloadedAndVerified {}

impl LifecycleState for Prepared {
    type Evidence = ();
    const STATE: &'static str = "prepared";
}

impl LifecycleState for EncoderTuned {
    /// PROOF that the gate passed, not a report about it.
    ///
    /// `PassedEvidence` is constructible only by `tune::validate_evidence`, so a value of
    /// this type cannot exist unless every gated parameter cleared its contracted epsilon.
    /// The evidence FIELD exists only in this state — there is no `Option` for a caller to
    /// `expect` on, which is what makes "EncoderTuned implies passed evidence" provable
    /// rather than conventional.
    type Evidence = tune::PassedEvidence;
    const STATE: &'static str = "encoder_tuned";
}

/// A SetFit training run in lifecycle state `S`.
///
/// # Every field is private and there is no public constructor
///
/// The only door is [`SetFitRun::<Prepared>::prepare`]. Later plans add transitions, each
/// consuming `self`.
///
/// # The run holds its own DATASET, not only the selection
///
/// `SelectedExample` carries `{id, label, exact_hash, normalized_hash}` and **no text**
/// (contrastive-data `select.rs`); the text lives in `Split<Train>::rows()`. A run holding
/// only the `Selection` therefore could not obtain a single string to encode — neither the
/// tuning loop (03-05) nor the encode-once head input (03-07) would have an input. The
/// `dataset` field is what makes those plans writable, and `prepare` proves the two agree
/// before the run exists.
#[derive(Debug)]
pub struct SetFitRun<S: LifecycleState> {
    encoder: SetFitMiniLm,
    dataset: PreparedDataset<Canonical>,
    selection: Selection,
    config: ResolvedSetFitConfig,
    evidence: S::Evidence,
    _state: PhantomData<S>,
}

impl<S: LifecycleState> SetFitRun<S> {
    /// The resolved configuration.
    #[must_use]
    pub fn config(&self) -> &ResolvedSetFitConfig {
        &self.config
    }

    /// The typed selection this run trains on.
    #[must_use]
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// The prepared dataset — the run's only source of text.
    #[must_use]
    pub fn dataset(&self) -> &PreparedDataset<Canonical> {
        &self.dataset
    }

    /// The encoder, read-only.
    #[must_use]
    pub fn encoder(&self) -> &SetFitMiniLm {
        &self.encoder
    }

    /// This state's evidence.
    #[must_use]
    pub fn evidence(&self) -> &S::Evidence {
        &self.evidence
    }

    /// The lifecycle state's name.
    #[must_use]
    pub fn state_name(&self) -> &'static str {
        S::STATE
    }
}

impl SetFitRun<Prepared> {
    /// The ONLY door into the lifecycle.
    ///
    /// Three checks, in the order that names the thing the reader has to change:
    ///
    /// 1. **Device probe.** The requested spec is resolved; an explicit CUDA request on a
    ///    host without CUDA fails closed in `resolve_device` rather than falling back
    ///    silently. Phase 3 is CPU-only by decision, so a resolved non-CPU device is
    ///    [`SetFitTrainError::UnsupportedDeviceForPhase3`] — including `auto` on a CUDA
    ///    host, which is deliberate: a phase that only supports CPU should make the
    ///    operator say `cpu`, not quietly disagree with the machine.
    /// 2. **Pair budget against selection capacity.** Delegated wholesale to Phase 2's
    ///    `resolve_budget`, which owns the binds-not-clamps rule and every degenerate
    ///    layout. Reimplementing capacity arithmetic here would create a second answer.
    /// 3. **Every selected id resolves to a row of `dataset.train()`, with matching
    ///    content.** A selection whose ids are not in this dataset is a typed error at the
    ///    door, not a panic three plans later. The exact hash is compared as well as the
    ///    id: an id that resolves to a row whose bytes have since changed is a DIFFERENT
    ///    example wearing the same name, and it would silently invalidate every provenance
    ///    claim the run goes on to make.
    ///
    /// # Errors
    ///
    /// [`SetFitTrainError`] — see each variant.
    pub fn prepare(
        encoder: SetFitMiniLm,
        dataset: PreparedDataset<Canonical>,
        selection: Selection,
        config: SetFitTrainConfig,
    ) -> Result<Self, SetFitTrainError> {
        let config = config.resolve()?;
        if config.device() != Device::Cpu {
            return Err(SetFitTrainError::UnsupportedDeviceForPhase3 {
                resolved: config.device().tag(),
            });
        }

        let class_sizes = dense_class_sizes(&dataset, &selection)?;
        let (budget, _default_was_clamped) =
            resolve_budget(config.requested().pair_config(), &class_sizes)?;
        debug_assert!(budget > 0, "resolve_budget never returns a zero budget");

        let train = dataset.train();
        for example in selection.examples() {
            let Some(observed) = train.exact_hash_of(&example.id) else {
                return Err(SetFitTrainError::SelectionRowMissing { id: example.id.clone() });
            };
            if observed != &example.exact_hash {
                return Err(SetFitTrainError::SelectionRowContentMismatch {
                    id: example.id.clone(),
                });
            }
        }

        Ok(Self { encoder, dataset, selection, config, evidence: (), _state: PhantomData })
    }

    /// Consume the run and hand back the four inputs stage one operates on.
    ///
    /// `pub(crate)`: the lifecycle's public transitions consume `self` and return the next
    /// state, and this is how they get at the parts. It is deliberately NOT public — a public
    /// destructor would let a caller take the encoder out of a `Prepared` run, tune it by
    /// hand, and put nothing back, which is the ungated tuning path the typestate exists to
    /// forbid.
    pub(crate) fn into_parts(
        self,
    ) -> (SetFitMiniLm, PreparedDataset<Canonical>, Selection, ResolvedSetFitConfig) {
        (self.encoder, self.dataset, self.selection, self.config)
    }

    /// Run the contrastive stage and, ONLY if its evidence passes, mint `EncoderTuned`.
    ///
    /// The gate runs INSIDE the transition. There is no ordering in which a caller obtains a
    /// tuned run and then decides whether to check it, because the checked value is the only
    /// thing this function can return.
    ///
    /// # The regime is recorded by the RUN, not chosen at judgement time
    ///
    /// `calibration_regime_id` is stamped when the evidence table is built, from the encoder,
    /// selection and configuration the run actually used — its own architecture, its own seed,
    /// its own cell. A run cannot present a regime it did not execute in, and the gate refuses
    /// any (architecture, seed, cell) the thresholds were not measured at.
    ///
    /// # Errors
    ///
    /// [`SetFitTrainError::NoTrainableParameters`] for an all-frozen run,
    /// [`SetFitTrainError::NoTestifyingParameters`] when nothing trainable can testify,
    /// [`SetFitTrainError::UncalibratedRegime`] outside the calibrated set, and
    /// [`SetFitTrainError::EvidenceRejected`] — carrying the complete table — when the
    /// measured movement misses the contracted thresholds.
    pub fn tune_encoder(self) -> Result<SetFitRun<EncoderTuned>, SetFitTrainError> {
        let (encoder, dataset, selection, config) = self.into_parts();
        let regime = calibration_regime_id(&encoder, &selection, &config);
        let out = tune::run_tuning(encoder, &dataset, &selection, &config)?;

        let table = evidence::UpdateEvidence::from_tune_output(&out, &regime)
            .map_err(|e| SetFitTrainError::Evidence { reason: e.to_string() })?;
        let passed = tune::validate_evidence(
            &table,
            &thresholds::Thresholds::frozen(),
            out.trainable_count,
            out.frozen_count,
        )?;

        Ok(SetFitRun {
            encoder: out.encoder,
            dataset,
            selection,
            config,
            evidence: passed,
            _state: PhantomData,
        })
    }
}

/// The short source-revision tag of the pinned MiniLM slice the epsilons were measured on.
///
/// The first eight hex digits of `1110a243fdf4706b3f48f1d95db1a4f5529b4d41`, the upstream
/// all-MiniLM-L6-v2 revision the fixture slice was carved from.
///
/// # It is a REVISION TAG, not a weights hash, and it is ASSERTED rather than observed
///
/// `SetFitMiniLm` publishes its dimensions ([`SetFitMiniLm::architecture_fingerprint`]) and
/// nothing about where its weights came from, so this constant is appended by this crate
/// rather than read off the model. Two encoders with identical dimensions and different weight
/// values are therefore indistinguishable to the gate. Closing that needs the encoder to carry
/// a content hash of its weights, which is a larger change than this constant; it is recorded
/// here as a KNOWN LIMIT so a later reader does not mistake the tag for a proof of identity.
const SLICE_SOURCE_REVISION: &str = "1110a243";

/// The regime a run executes in, derived from the encoder, selection and configuration it
/// actually used.
///
/// Derived rather than supplied: a caller-provided regime string would let a run claim to be
/// something it is not, and the gate's fail-closed check would then be checking a label
/// instead of the run.
///
/// # The id states the run's OWN coordinates
///
/// Architecture from the encoder, seed from the configuration, cell from the selection AND the
/// configuration — every component measured from the thing that produced it. It is never the
/// enumerated calibrated string: that string names the seeds and cells the CALIBRATION swept,
/// and a run that stamped it would be claiming to have executed six cells it never ran. The
/// gate then decides membership component-wise ([`thresholds::Thresholds::is_calibrated`]),
/// which is what makes the recorded id and the calibrated set two halves of one check.
fn calibration_regime_id(
    encoder: &SetFitMiniLm,
    selection: &Selection,
    config: &ResolvedSetFitConfig,
) -> String {
    let architecture = format!("{}@{SLICE_SOURCE_REVISION}", encoder.architecture_fingerprint());
    thresholds::RegimeCoordinates::render_run(
        &architecture,
        config.requested().root_seed(),
        &cell_label(selection, config),
    )
}

/// The cell label `s{shots}e{epochs}b{batch}` this run executed in.
///
/// The shot count comes from the SELECTION, not from a configuration knob: the selection is
/// the object that drew the rows, and a label derived from an intent could disagree with the
/// run it labels.
fn cell_label(selection: &Selection, config: &ResolvedSetFitConfig) -> String {
    format!(
        "{}e{}b{}",
        shots_component(selection.class_sizes()),
        config.requested().epochs(),
        config.requested().batch_size(),
    )
}

/// The `s{n}` component of a cell label, from the SELECTED rows per class.
///
/// # A non-uniform selection is labelled as such, never as one of its classes
///
/// `FewShotSelector` draws the same number of rows for every class, so every calibrated cell
/// takes the uniform branch. A selection whose classes disagree is not an n-shot cell in any
/// honest sense, and naming one class's count would mint the label of a cell the run did not
/// execute — the very substitution this function exists to remove. Such a run is labelled
/// `smixed{min}-{max}`, and a selection that drew nothing is `sempty`; neither is a member of
/// any calibrated cell set, so both fail closed rather than borrowing another cell's epsilons.
///
/// Takes the sparse `(label, size)` list rather than the `Selection` so the two branches
/// `FewShotSelector` cannot currently produce are still directly testable.
fn shots_component(class_sizes: &[(usize, u64)]) -> String {
    let mut sizes = class_sizes.iter().map(|&(_, size)| size);
    let Some(first) = sizes.next() else {
        return "sempty".to_string();
    };
    let (min, max) = sizes.fold((first, first), |(lo, hi), size| (lo.min(size), hi.max(size)));
    if min == max {
        format!("s{min}")
    } else {
        format!("smixed{min}-{max}")
    }
}

/// Build the dense, label-indexed class-size vector Phase 2's capacity functions take.
///
/// `Selection::class_sizes()` is a sparse `(label, size)` list ascending by label; the
/// capacity functions index by position. The label map's length is the authority on `K`,
/// so a class that was declared but drew no rows contributes a zero rather than shifting
/// every later class's index by one.
fn dense_class_sizes(
    dataset: &PreparedDataset<Canonical>,
    selection: &Selection,
) -> Result<Vec<u64>, SetFitTrainError> {
    let classes = dataset.label_names().len();
    let mut sizes = vec![0_u64; classes];
    for &(label, size) in selection.class_sizes() {
        let slot = sizes
            .get_mut(label)
            .ok_or(SetFitTrainError::SelectionLabelOutOfRange { label, classes })?;
        *slot = size;
    }
    Ok(sizes)
}

/// Failure modes of the SetFit training lifecycle.
///
/// Later plans extend this enum with their own transitions' failures; it is
/// `#[non_exhaustive]` so doing so is not a breaking change.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SetFitTrainError {
    /// A configuration knob was rejected.
    Config(SetFitConfigError),
    /// The device spec was rejected or the probe failed closed.
    Device(DeviceError),
    /// The device resolved to something Phase 3 does not support.
    UnsupportedDeviceForPhase3 {
        /// The resolved device's tag.
        resolved: String,
    },
    /// Phase 2's pair capacity / budget ladder rejected the request.
    Capacity(ContrastiveDataError),
    /// A selected row's label is outside the dataset's declared label map.
    SelectionLabelOutOfRange {
        /// The offending label.
        label: usize,
        /// The number of declared classes.
        classes: usize,
    },
    /// A selected id does not name any row of `dataset.train()`.
    SelectionRowMissing {
        /// The offending row identifier.
        id: String,
    },
    /// A selected id names a row whose exact content hash disagrees with the selection's.
    SelectionRowContentMismatch {
        /// The offending row identifier.
        id: String,
    },
    /// The `max_length` knob does not equal the encoder's pinned sequence length.
    ///
    /// Distinct from [`config::SetFitConfigError::MaxLengthNotSupported`], which rejects the
    /// same disagreement at CONSTRUCTION. This variant is what the tuning loop raises when it
    /// CONSUMES the knob: the knob was validated in 03-03 and never read, which is how a
    /// validated-but-ignored setting silently becomes decoration.
    MaxLengthNotConsumable {
        /// The requested length.
        requested: u32,
        /// The encoder's pinned length.
        pinned: u32,
    },
    /// The encoder rejected a tokenize, encode, freeze or forward-ordinal call.
    ///
    /// Carries a RENDERED string rather than the typed `SetFitError`, following 03-02's
    /// precedent: this enum derives `PartialEq` and the encoder's error type is free to grow
    /// float payloads, which would make that derive a liability at a distance.
    Encoder {
        /// The encoder's rendered diagnostic.
        reason: String,
    },
    /// The trainable parameter registry changed order or membership mid-run.
    ///
    /// `AdamW` indexes its first and second moment buffers POSITIONALLY, so a reordered
    /// registry pairs each moment with a different parameter — an update that is silently
    /// wrong rather than loudly broken (T-3-54).
    ParameterRegistryMoved,
    /// The evidence layer could not build or render the table.
    Evidence {
        /// The evidence layer's rendered diagnostic.
        reason: String,
    },
    /// The run recorded a calibration regime these thresholds were not measured in.
    ///
    /// FAIL CLOSED, and checked FIRST. An epsilon measured on one architecture is not
    /// evidence about another, so the gate refuses to judge rather than judging leniently.
    UncalibratedRegime {
        /// The regime the run recorded.
        observed: String,
        /// The regimes the frozen thresholds were measured in.
        calibrated: Vec<String>,
    },
    /// The trainable parameter set is empty, so no evidence of movement can exist.
    ///
    /// SAFE-03 automatic under D-09: an all-frozen run cannot pass by being un-checkable.
    NoTrainableParameters {
        /// The observed count. Zero, and reported so the message is self-contained.
        trainable_count: usize,
    },
    /// Every trainable parameter belongs to a class that cannot serve as evidence.
    ///
    /// Distinct from [`Self::NoTrainableParameters`]: the set is NOT empty, but every member
    /// is analytically gradient-free, so nothing in it can testify that tuning occurred.
    /// Without this variant, freezing everything except the attention key biases would leave
    /// the gate with nothing to check and produce a vacuous pass.
    NoTestifyingParameters {
        /// Trainable parameters after `apply_freeze`.
        trainable_count: usize,
        /// How many of them are ungated.
        ungated_count: usize,
    },
    /// The evidence failed the gate. Carries the COMPLETE auditable record.
    ///
    /// `Box`ed because this variant is far larger than every other, and an enum is as big as
    /// its widest arm on every success path too.
    EvidenceRejected {
        /// The bound summary.
        summary: Box<evidence::EvidenceSummary>,
        /// The complete per-parameter table — one row per trainable parameter.
        table: Box<evidence::UpdateEvidence>,
        /// The parameter furthest from passing, with its class, delta and contracted epsilon.
        worst: tune::FailedParameter,
    },
}

impl fmt::Display for SetFitTrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Evidence { reason } => {
                write!(f, "setfit evidence could not be built: {reason}")
            }
            Self::UncalibratedRegime { observed, calibrated } => write!(
                f,
                "this run recorded calibration regime `{observed}`, which the frozen \
                 thresholds were NOT measured in (calibrated: {}). The gate fails closed \
                 rather than applying an epsilon measured on a different architecture; \
                 extending the calibrated set requires a calibration run on this encoder AND \
                 a deliberate edit to contracts/setfit-train-lifecycle-v1.yaml (D-10(c))",
                calibrated.join(", "),
            ),
            Self::NoTrainableParameters { trainable_count } => write!(
                f,
                "the trainable parameter set is empty (trainable_count {trainable_count}), so \
                 no evidence that the encoder moved can exist; a run in this state cannot \
                 identify as SetFit (contract setfit-train-lifecycle-v1, requirement SAFE-03)",
            ),
            Self::NoTestifyingParameters { trainable_count, ungated_count } => write!(
                f,
                "all {ungated_count} of the {trainable_count} trainable parameters belong to \
                 a class that cannot serve as encoder-update evidence (their gradients are \
                 analytically zero), so the gate has nothing to check and refuses to pass the \
                 run (contract setfit-train-lifecycle-v1, requirement SAFE-03)",
            ),
            Self::EvidenceRejected { worst, summary, .. } => write!(
                f,
                "setfit evidence REJECTED: parameter `{}` (class {}) moved by relative delta \
                 {:e}, which does not exceed the contracted epsilon {:e} for that class; \
                 {} trainable parameters were measured under regime `{}` (contract \
                 setfit-train-lifecycle-v1, requirement TRN-03)",
                worst.name,
                worst.class,
                worst.relative_delta,
                worst.eps,
                summary.trainable_count,
                summary.calibration_regime_id,
            ),
            Self::Config(inner) => write!(f, "setfit configuration rejected: {inner}"),
            Self::Device(inner) => write!(f, "setfit device rejected: {inner}"),
            Self::UnsupportedDeviceForPhase3 { resolved } => write!(
                f,
                "device resolved to `{resolved}`, but the Phase 3 SetFit trainer is \
                 CPU-only; pass `--device cpu` explicitly \
                 (contract setfit-train-lifecycle-v1, requirement TRN-02)",
            ),
            Self::Capacity(inner) => write!(f, "pair budget rejected: {inner}"),
            Self::SelectionLabelOutOfRange { label, classes } => write!(
                f,
                "selection names class {label} but the dataset declares {classes} classes \
                 (contract setfit-train-lifecycle-v1, requirement TRN-01)",
            ),
            Self::SelectionRowMissing { id } => write!(
                f,
                "selected id `{id}` is not a row of this dataset's train split \
                 (contract setfit-train-lifecycle-v1, requirement TRN-01)",
            ),
            Self::SelectionRowContentMismatch { id } => write!(
                f,
                "selected id `{id}` resolves to a row whose exact content hash disagrees \
                 with the selection's — the same name, different bytes \
                 (contract setfit-train-lifecycle-v1, requirement TRN-01)",
            ),
            Self::MaxLengthNotConsumable { requested, pinned } => write!(
                f,
                "the tuning loop consumes max_length {pinned}, but this run requested \
                 {requested}; the encoder's sequence length is an equality constraint, not a \
                 runtime setting \
                 (contract setfit-train-lifecycle-v1, requirement TRN-02)",
            ),
            Self::Encoder { reason } => write!(
                f,
                "the encoder rejected a tuning-loop call: {reason} \
                 (contract setfit-train-lifecycle-v1, requirement TRN-03)",
            ),
            Self::ParameterRegistryMoved => write!(
                f,
                "the trainable parameter registry changed between the pre-loop snapshot and \
                 an optimizer step; AdamW's moment state is positional, so the update would \
                 have paired moments with the wrong parameters \
                 (contract setfit-train-lifecycle-v1, requirement TRN-03)",
            ),
        }
    }
}

impl std::error::Error for SetFitTrainError {}

impl From<SetFitConfigError> for SetFitTrainError {
    fn from(inner: SetFitConfigError) -> Self {
        Self::Config(inner)
    }
}

impl From<DeviceError> for SetFitTrainError {
    fn from(inner: DeviceError) -> Self {
        Self::Device(inner)
    }
}

impl From<ContrastiveDataError> for SetFitTrainError {
    fn from(inner: ContrastiveDataError) -> Self {
        Self::Capacity(inner)
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixtures as fx;
    use super::thresholds::Thresholds;
    use super::*;

    /// The regime id a fixture cell's run would record, built through the shipped derivation.
    fn regime_of(variant: fx::CalibrationVariant) -> String {
        let run = fx::prepared_run(variant, None);
        calibration_regime_id(run.encoder(), run.selection(), run.config())
    }

    /// The recorded id states the RUN's coordinates — its seed, its cell, no placeholders.
    ///
    /// The exact strings, not a `contains`: the whole defect this test exists for was an id
    /// that looked plausible (it named the right architecture) while its seed and cell
    /// components described something other than the run carrying it.
    #[test]
    fn regime_id_states_the_runs_own_seed_and_cell() {
        assert_eq!(
            regime_of(fx::calibrated_variant()),
            "minilm-slice-h64-l2-a2-i256-v97@1110a243|seeds=1|cells=s8e1b4",
        );
        assert_eq!(
            regime_of(fx::default_variant()),
            format!(
                "minilm-slice-h64-l2-a2-i256-v97@1110a243|seeds={}|cells=s8e1b4",
                fx::FIXTURE_SEED,
            ),
        );
        assert_eq!(
            regime_of(fx::uncalibrated_cell_variant()),
            "minilm-slice-h64-l2-a2-i256-v97@1110a243|seeds=1|cells=s8e1b3",
        );

        // No run may record the enumerated CALIBRATED string, and none may record a
        // placeholder. Both were observed: the first was returned for every run on the
        // fixture architecture, the second for every run off it.
        for variant in
            [fx::calibrated_variant(), fx::default_variant(), fx::uncalibrated_cell_variant()]
        {
            let id = regime_of(variant);
            assert!(!id.contains('?'), "a recorded id must never be a placeholder: `{id}`");
            assert!(
                !id.contains("seeds=1,42,7"),
                "a single run must never claim the calibration's whole seed sweep: `{id}`",
            );
        }
    }

    /// And the gate agrees with those coordinates, one cell at a time.
    #[test]
    fn regime_id_is_calibrated_only_at_a_measured_seed_and_cell() {
        let frozen = Thresholds::frozen();
        assert!(
            frozen.is_calibrated(&regime_of(fx::calibrated_variant())),
            "seed 1 in cell s8e1b4 was measured",
        );
        assert!(
            !frozen.is_calibrated(&regime_of(fx::default_variant())),
            "FIXTURE_SEED was never swept",
        );
        assert!(
            !frozen.is_calibrated(&regime_of(fx::uncalibrated_cell_variant())),
            "cell s8e1b3 was never measured",
        );
    }

    /// A compact description of a `tune_encoder` outcome, for the negatives' failure messages.
    ///
    /// `{other:?}` on the `Ok` arm renders the entire `SetFitRun` — encoder dims, every
    /// synthetic row, the whole evidence table — which is ~100 KB of scrollback in place of
    /// the one fact that matters: which regime the gate accepted.
    fn outcome(result: &Result<SetFitRun<EncoderTuned>, SetFitTrainError>) -> String {
        match result {
            Ok(run) => format!(
                "Ok — the gate ACCEPTED the run and recorded regime `{}`",
                run.evidence().summary().calibration_regime_id,
            ),
            Err(err) => format!("Err({err})"),
        }
    }

    /// CONTROL — a run at a calibrated (architecture, seed, cell) still passes the WHOLE gate.
    ///
    /// Without this the two negatives below would be satisfied by a regime check that refuses
    /// everything, which is the failure mode a fail-closed gate is one keystroke away from.
    ///
    /// It asserts the PLUMBING, not the id's spelling: that the id `tune_encoder` stamps into
    /// the evidence summary is the one the derivation produced for this run, and that the gate
    /// accepts it. The spelling is pinned by `regime_id_states_the_runs_own_seed_and_cell`
    /// above, which is where a wrong id belongs — keeping it out of here is what lets this
    /// test be green both before and after the derivation was fixed, and therefore evidence
    /// that the new check is not simply refusing everything.
    #[test]
    fn regime_gate_a_calibrated_run_passes() {
        let result = fx::prepared_run(fx::calibrated_variant(), None).tune_encoder();
        let run = result.expect("a run at a measured seed and cell must pass the regime check");
        let recorded = &run.evidence().summary().calibration_regime_id;
        assert!(
            Thresholds::frozen().is_calibrated(recorded),
            "the passing run's OWN recorded id must be a calibrated one, got `{recorded}`",
        );
        assert_eq!(
            recorded,
            &regime_of(fx::calibrated_variant()),
            "the id the run RECORDS must be the id the derivation produces for it; a divergence \
             here would mean the gate judged something other than what it stamped",
        );
    }

    /// NEGATIVE (seed) — a run at an unswept seed is refused, on a calibrated cell.
    ///
    /// The epsilons were measured over seeds {1, 7, 42}. `FIXTURE_SEED` is not one of them, so
    /// applying those numbers to this run would be applying a measurement to a run it was
    /// never taken on.
    #[test]
    fn regime_gate_an_unswept_seed_is_refused() {
        let result = fx::prepared_run(fx::default_variant(), None).tune_encoder();
        match &result {
            Err(SetFitTrainError::UncalibratedRegime { observed, calibrated }) => {
                assert_eq!(
                    observed,
                    &format!(
                        "minilm-slice-h64-l2-a2-i256-v97@1110a243|seeds={}|cells=s8e1b4",
                        fx::FIXTURE_SEED,
                    ),
                    "the refusal must name the run's REAL coordinates",
                );
                assert_eq!(calibrated.len(), 1, "exactly one calibrated entry");
                assert!(
                    calibrated[0].contains("seeds=1,42,7"),
                    "the diagnosis must show the seeds that WERE measured: {calibrated:?}",
                );
            }
            _ => panic!(
                "a run at seed {} — which the calibration never swept — must fail closed, got \
                 {}. If the gate accepted it, every epsilon in the contract is being applied to \
                 a run it was not measured on.",
                fx::FIXTURE_SEED,
                outcome(&result),
            ),
        }
    }

    /// NEGATIVE (cell) — a run in an unmeasured cell is refused, at a calibrated seed.
    ///
    /// Same architecture, same seed as the control, one knob apart: batch 3 instead of 4. The
    /// cell is the only thing that changed, so it is the only thing the refusal can be about.
    #[test]
    fn regime_gate_an_unmeasured_cell_is_refused() {
        let result = fx::prepared_run(fx::uncalibrated_cell_variant(), None).tune_encoder();
        match &result {
            Err(SetFitTrainError::UncalibratedRegime { observed, calibrated }) => {
                assert_eq!(
                    observed, "minilm-slice-h64-l2-a2-i256-v97@1110a243|seeds=1|cells=s8e1b3",
                    "the refusal must name the run's REAL cell",
                );
                assert!(
                    calibrated[0].contains("cells=s16e2b8,s8e1b4"),
                    "the diagnosis must show the cells that WERE measured: {calibrated:?}",
                );
            }
            _ => panic!(
                "a run in cell s8e1b3 — which the calibration never measured — must fail \
                 closed, got {}",
                outcome(&result),
            ),
        }
    }

    /// A non-uniform selection is labelled as one, not as an n-shot cell it is not.
    ///
    /// `FewShotSelector` cannot currently emit a non-uniform draw, which is exactly why the
    /// branch needs a test: an unexercised default is how a future selector would silently
    /// inherit a calibrated cell's epsilons.
    #[test]
    fn regime_shots_component_never_names_one_class_of_a_non_uniform_selection() {
        // The uniform case, cross-checked against a real selection.
        let uniform = fx::fixture_selection(fx::FIXTURE_SEED, 8);
        assert_eq!(shots_component(uniform.class_sizes()), "s8");
        assert_eq!(shots_component(&[(0, 8), (1, 8), (2, 8)]), "s8");

        // Non-uniform: the label names the SPREAD, never `s8` (the majority) or `s5`.
        assert_eq!(shots_component(&[(0, 8), (1, 5), (2, 8)]), "smixed5-8");
        assert_eq!(shots_component(&[]), "sempty");

        // And neither label is a member of any calibrated cell set.
        let frozen = Thresholds::frozen();
        for cell in ["smixed5-8e1b4", "semptye1b4"] {
            assert!(
                !frozen.is_calibrated(&thresholds::RegimeCoordinates::render_run(
                    "minilm-slice-h64-l2-a2-i256-v97@1110a243",
                    1,
                    cell,
                )),
                "`{cell}` must not borrow a measured cell's epsilons",
            );
        }
    }
}
