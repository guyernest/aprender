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
/// Stage two's encode-once input (D-08).
///
/// `pub(crate)`: `HeadDataset` is an intermediate, and a public one would be a second way to
/// reach the head's fitting input — one that does not travel through the typestate.
pub(crate) mod head_input;
pub mod reduce;
pub mod thresholds;
pub mod tune;

/// The deterministic, network-free, synthetic-text fixture every Phase 3 trainer test uses.
///
/// `#[cfg(test)]` and nothing weaker: 03-10's acceptance criteria reject a `#[doc(hidden)]`
/// test-support door on the shipped surface.
#[cfg(test)]
pub(crate) mod test_fixtures;

/// The in-band pair-weighted fitter TRN-05's structural claim is measured against.
///
/// `#[cfg(test)]`: a pair-weighted head fitter must not be reachable from a shipped build by
/// any door, including a `#[doc(hidden)]` one.
#[cfg(test)]
mod negative;

use core::fmt;
use core::marker::PhantomData;

use aprender::classification::{HeadFitError, HeadFitReport, MultinomialLogisticRegression};
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

/// The evidence a state that has produced none carries.
///
/// A NAMED unit rather than a bare `()`. It reads better at the use site, and it is what
/// makes B-3's guard non-vacuous: with `type Evidence = ();` written out, a scan for an
/// anonymous tuple evidence type matches the deliberate empty case and can therefore never
/// distinguish it from an accidental `(A, B)`. Naming the empty case leaves the scan free to
/// mean exactly one thing.
pub type NoEvidence = ();

impl LifecycleState for Prepared {
    type Evidence = NoEvidence;
    const STATE: &'static str = "prepared";
}

/// Everything stage two produced, and the complete chain it stands on.
///
/// # A NAMED struct, not a `(PassedEvidence, HeadFitReport)` tuple (B-3)
///
/// `SetFitRun`'s field list is fixed, so `Evidence` is the ONLY home stage-two state has. A
/// two-tuple has no slot for the fitted head, the ordered labels, the effective lambda, the
/// encode ledger or the encode-call count — every one of which this plan and 03-08 require an
/// accessor for. Anything dropped here becomes something a downstream plan RECOMPUTES, which
/// is exactly the false green the in-band evidence discipline exists to remove.
///
/// Every field is private and every accessor hands out a shared borrow. There is no `Option`
/// anywhere: the absent-field pattern says a state that has not produced a thing has no field
/// for it, rather than a `None` every reader has to `expect` on.
#[derive(Debug)]
pub struct HeadFittedEvidence {
    passed: tune::PassedEvidence,
    head: MultinomialLogisticRegression,
    report: HeadFitReport,
    effective_lambda: f64,
    ordered_labels: Vec<String>,
    encode_ledger: Vec<String>,
    encode_call_count: usize,
}

impl HeadFittedEvidence {
    /// The complete stage-one chain this head was fitted on top of.
    #[must_use]
    pub fn passed(&self) -> &tune::PassedEvidence {
        &self.passed
    }

    /// The fitted head. A SHARED borrow: predict through it, never re-fit it.
    #[must_use]
    pub fn head(&self) -> &MultinomialLogisticRegression {
        &self.head
    }

    /// The optimizer's deterministic record of the fit.
    #[must_use]
    pub fn report(&self) -> &HeadFitReport {
        &self.report
    }

    /// The L2 coefficient the fit actually minimized under.
    ///
    /// Recorded rather than left to be recomputed. Without it nothing downstream can state
    /// which objective produced these weights, and 03-08's bundle would have to re-derive it
    /// — a second derivation of the one number TRN-05 is about.
    #[must_use]
    pub fn effective_lambda(&self) -> f64 {
        self.effective_lambda
    }

    /// The declared label map the head's weight rows are indexed by.
    #[must_use]
    pub fn ordered_labels(&self) -> &[String] {
        &self.ordered_labels
    }

    /// The ordered identifiers actually handed to the encoder (D-08's exactly-once proof).
    #[must_use]
    pub fn encode_ledger(&self) -> &[String] {
        &self.encode_ledger
    }

    /// How many times the encoder was invoked while building the head's input.
    #[must_use]
    pub fn encode_call_count(&self) -> usize {
        self.encode_call_count
    }
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

impl LifecycleState for HeadFitted {
    type Evidence = HeadFittedEvidence;
    const STATE: &'static str = "head_fitted";
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

impl SetFitRun<EncoderTuned> {
    /// Fit the multiclass head on each unique selected row, exactly once (D-08, TRN-05).
    ///
    /// # Pair multiplicity is INEXPRESSIBLE, not rejected
    ///
    /// This function takes NO parameters beyond `self`. Everything it fits on comes from the
    /// run it consumes: the dataset, the selection and the resolved configuration. There is
    /// no argument a caller could pass that says "weight this row twice", and no field on
    /// `SetFitRun` that could carry one — the pair stream is stage one's input and it does
    /// not survive into this state at all. A runtime check against multiplicity would need a
    /// multiplicity to check; the point is that there is nowhere for one to live.
    ///
    /// The adversarial half of that claim is `negative.rs`, which builds the pair-weighted
    /// fitter from the only surface that can still express it — raw embedding rows plus the
    /// public head — and shows it moves the coefficients at an IDENTICAL lambda.
    ///
    /// # The effective lambda tracks UNIQUE ROWS
    ///
    /// `SklearnEquivalentC { c }` resolves through `head_input::resolve_lambda` against the
    /// encode-once row count, so the reference default `C = 1.0` over a 24-row selection is
    /// `lambda = 1/48` whatever the pair budget is. The resolved value is handed to the head
    /// as `Regularization::Lambda`, so the head does not re-resolve it against its own row
    /// count and there is exactly one place the choice of `n` is made.
    ///
    /// # Errors
    ///
    /// Anything the encode-once input rejects (see `head_input::head_dataset`), plus
    /// [`SetFitTrainError::HeadFit`] carrying the head's own typed failure — a head that
    /// cannot converge is an error, never a warning and never a silently accepted fit.
    pub fn fit_head(self) -> Result<SetFitRun<HeadFitted>, SetFitTrainError> {
        self.fit_head_with_iteration_budget(head_input::HEAD_MAX_ITER)
    }

    /// The same body at a caller-chosen L-BFGS budget.
    ///
    /// `#[cfg(test)]`: the non-convergence path has to be reachable to be proven typed, and
    /// the honest way to reach it is a tiny iteration budget. Shipping this as a public knob
    /// would put a door on the surface whose only use is to make the head fail.
    #[cfg(test)]
    pub(crate) fn fit_head_with_max_iter(
        self,
        max_iter: usize,
    ) -> Result<SetFitRun<HeadFitted>, SetFitTrainError> {
        self.fit_head_with_iteration_budget(max_iter)
    }

    fn fit_head_with_iteration_budget(
        self,
        max_iter: usize,
    ) -> Result<SetFitRun<HeadFitted>, SetFitTrainError> {
        let Self { mut encoder, dataset, selection, config, evidence: passed, _state } = self;
        let head_input::FittedHead { head, report, lambda, input } =
            head_input::fit_on_selection(&mut encoder, &dataset, &selection, &config, max_iter)?;
        let evidence = HeadFittedEvidence {
            passed,
            head,
            report,
            effective_lambda: lambda,
            ordered_labels: input.ordered_labels().to_vec(),
            encode_ledger: input.encode_ledger().to_vec(),
            encode_call_count: input.encode_call_count(),
        };
        Ok(SetFitRun { encoder, dataset, selection, config, evidence, _state: PhantomData })
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
    /// The multiclass head refused the fit, with its own typed reason.
    ///
    /// The inner [`HeadFitError`] is preserved rather than rendered to a string, because
    /// TRN-04's whole point is that a caller can distinguish "your data was bad" from "the
    /// optimizer ran out of budget" from "the arithmetic went non-finite" without matching on
    /// message text.
    HeadFit(HeadFitError),
    /// The head's encode did not run isolated from training mode and the autograd graph.
    ///
    /// Unreachable while `head_dataset` sets eval mode, wraps the encode in `no_grad` and
    /// detaches every result — which is exactly why it is a CHECK rather than a comment. The
    /// three mechanisms are observed while they run and refused if any is absent, so a future
    /// encoder change that starts recording under `no_grad` fails closed instead of silently
    /// making the head's input irreproducible (T-3-24).
    HeadEncodeNotIsolated {
        /// The encoder reported training mode inside an encode window.
        training_observed: bool,
        /// A stored embedding tensor still required gradients after `detach`.
        requires_grad_observed: bool,
        /// Operations the encode appended to the autograd tape.
        tape_growth: usize,
    },
    /// The head's encode window size was zero.
    ///
    /// `ResolvedSetFitConfig` cannot carry a zero batch size, so this is unreachable from the
    /// shipped transition. It exists because `chunks(0)` PANICS: a typed refusal is what keeps
    /// the one internal caller that could ever get it wrong from taking the process down.
    HeadEncodeBatchSizeZero,
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
            Self::HeadFit(inner) => write!(
                f,
                "the multiclass head refused the fit: {inner}; a head that does not converge \
                 is an error rather than a warning, so no coefficients were produced \
                 (contract setfit-train-lifecycle-v1, requirement TRN-04)",
            ),
            Self::HeadEncodeNotIsolated {
                training_observed,
                requires_grad_observed,
                tape_growth,
            } => write!(
                f,
                "the head's encode did not run isolated (training mode observed: \
                 {training_observed}; embeddings still requiring grad: \
                 {requires_grad_observed}; autograd tape grew by {tape_growth} operations), so \
                 its embeddings are not reproducible and the head fitted on them could not be \
                 replayed \
                 (contract setfit-train-lifecycle-v1, requirement TRN-05)",
            ),
            Self::HeadEncodeBatchSizeZero => write!(
                f,
                "the head's encode window size is zero, so the selection would be chunked \
                 into nothing; a validated configuration cannot produce this \
                 (contract setfit-train-lifecycle-v1, requirement TRN-05)",
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

    // =======================================================================================
    // Stage two — plan 03-07, TRN-05
    // =======================================================================================

    use aprender::optim::ConvergenceStatus;
    use aprender_contrastive_data::ledger::AccessLedger;

    use config::HeadRegularization;

    /// The reference default the fixture configures the head with.
    const REFERENCE_C: f64 = 1.0;
    /// 3 classes x 8 shots.
    const SELECTED_ROWS: usize = 24;

    /// A complete calibrated pipeline: prepare -> tune_encoder -> fit_head.
    fn head_fitted_run(variant: fx::CalibrationVariant) -> SetFitRun<HeadFitted> {
        fx::prepared_run(variant, None)
            .tune_encoder()
            .expect("a run at a measured seed and cell must pass the evidence gate")
            .fit_head()
            .expect("the head must fit on the fixture's 24 encode-once rows")
    }

    /// Embedding rows for arbitrary probe texts, through the run's OWN encoder.
    fn probe_rows(run: &SetFitRun<HeadFitted>, texts: &[&str]) -> Vec<Vec<f32>> {
        let embedded = run.encoder().encode_texts(texts).expect("probe texts encode");
        let hidden = embedded.shape()[1];
        embedded.data().chunks(hidden).map(<[f32]>::to_vec).collect()
    }

    /// The pipeline reaches `HeadFitted`, and the head answers with a real distribution.
    #[test]
    fn fit_head_completes_the_pipeline_and_probabilities_sum_to_one() {
        let run = head_fitted_run(fx::calibrated_variant());
        assert_eq!(run.state_name(), "head_fitted");

        let texts: Vec<&str> =
            run.dataset().test().rows().iter().map(|r| r.input.as_str()).collect();
        assert!(!texts.is_empty(), "the fixture's test split must have rows to probe with");
        let rows = probe_rows(&run, &texts);
        let probs =
            run.evidence().head().predict_proba(&rows).expect("the fitted head must predict");

        assert_eq!(probs.len(), rows.len());
        for (row, p) in probs.iter().enumerate() {
            assert_eq!(p.len(), 3, "row {row}: one probability per declared class");
            assert!(p.iter().all(|v| v.is_finite()), "row {row}: {p:?} is not finite");
            let total: f64 = p.iter().sum();
            assert!((total - 1.0).abs() < 1e-6, "row {row}: probabilities sum to {total}");
        }
        assert_eq!(run.evidence().report().status, ConvergenceStatus::Converged);
    }

    /// Two identical pipelines agree BITWISE on the stored `f32` head.
    #[test]
    fn fit_head_two_identical_pipelines_agree_bitwise() {
        let first = head_fitted_run(fx::calibrated_variant());
        let second = head_fitted_run(fx::calibrated_variant());

        assert!(!first.evidence().head().weights().is_empty(), "a fitted head has weights");
        assert_eq!(
            first.evidence().head().weights(),
            second.evidence().head().weights(),
            "the whole path — pair stream, tuning, encode-once, L-BFGS from a zero start — \
             contains no randomness, so two identical runs must store identical f32 weights",
        );
        assert_eq!(first.evidence().head().intercepts(), second.evidence().head().intercepts());
        assert_eq!(first.evidence().encode_ledger(), second.evidence().encode_ledger());
        assert_eq!(first.evidence().effective_lambda(), second.evidence().effective_lambda());
    }

    /// The effective lambda resolves against UNIQUE ROWS: `C = 1` over 24 rows is exactly 1/48.
    #[test]
    fn fit_head_lambda_resolves_against_unique_rows_to_one_over_forty_eight() {
        let reg = HeadRegularization::SklearnEquivalentC { c: REFERENCE_C };
        assert_eq!(
            head_input::resolve_lambda(&reg, SELECTED_ROWS),
            1.0 / 48.0,
            "lambda = 1/(2*C*n) with n = 24 unique rows",
        );

        let run = head_fitted_run(fx::calibrated_variant());
        // `n` is PINNED to the selection's own length, not to a literal.
        assert_eq!(run.selection().len(), SELECTED_ROWS);
        assert_eq!(run.evidence().encode_ledger().len(), run.selection().len());
        assert_eq!(
            run.evidence().effective_lambda(),
            head_input::resolve_lambda(&reg, run.selection().len()),
            "the fit's lambda must be the one the unique-row count resolves to",
        );
        assert_eq!(run.evidence().effective_lambda(), 1.0 / 48.0);

        // The pair budget resolves to a DIFFERENT number, which is what makes the assertion
        // above discriminating rather than a coincidence of the fixture.
        let budget = run
            .config()
            .requested()
            .pair_config()
            .budget
            .expect("the fixture pins an explicit budget") as usize;
        assert_ne!(budget, SELECTED_ROWS);
        assert_ne!(head_input::resolve_lambda(&reg, budget), 1.0 / 48.0);
    }

    /// The pair budget does not reach the head's objective.
    ///
    /// # Why this is asserted against `fit_on_selection` and not against `fit_head`
    ///
    /// The plan asked for "varying the pair budget leaves the fitted weights bitwise
    /// unchanged" end to end. That statement is FALSE end to end, and asserting it would have
    /// been asserting something untrue: the budget is stage ONE's input, so two budgets take
    /// different numbers of optimizer steps and hand stage two two different encoders. The
    /// head's weights are then legitimately different, for a reason that has nothing to do
    /// with TRN-05.
    ///
    /// What IS true, and what TRN-05 actually claims, is that stage two never reads the
    /// budget: given the SAME encoder, two configurations differing only in pair budget
    /// produce the same lambda and the same coefficients, bitwise. That is asserted here
    /// against the very function `fit_head` runs. The end-to-end half — the recorded lambda
    /// is budget-independent even when the encoder is not — is asserted below it.
    #[test]
    fn fit_head_pair_budget_does_not_reach_the_head_objective() {
        let mut ledger = AccessLedger::new();
        let dataset = fx::synthetic_dataset(&mut ledger);
        let selection = fx::fixture_selection(fx::FIXTURE_SEED, 8);
        let mut encoder = fx::slice_encoder(fx::FIXTURE_SEED);

        let base = fx::calibrated_variant();
        let resolved = |budget: u64| {
            fx::config_for(fx::CalibrationVariant { budget, ..base }, None)
                .resolve()
                .expect("the fixture configuration resolves on a cpu host")
        };
        let (small, large) = (resolved(12), resolved(20));
        assert_ne!(
            small.requested().pair_config().budget,
            large.requested().pair_config().budget,
            "the two configurations must actually differ in the budget",
        );

        let fit = |config: &config::ResolvedSetFitConfig, encoder: &mut _| {
            head_input::fit_on_selection(
                encoder,
                &dataset,
                &selection,
                config,
                head_input::HEAD_MAX_ITER,
            )
            .expect("stage two must fit at either budget")
        };
        let a = fit(&small, &mut encoder);
        let b = fit(&large, &mut encoder);

        assert_eq!(a.lambda, 1.0 / 48.0);
        assert_eq!(a.lambda, b.lambda, "the budget must not move the head's L2 coefficient");
        assert_eq!(
            a.head.weights(),
            b.head.weights(),
            "the budget must not move a single coefficient of the head",
        );
        assert_eq!(a.head.intercepts(), b.head.intercepts());
        assert_eq!(a.input.encode_ledger(), b.input.encode_ledger());

        // End to end: two complete pipelines at different budgets record the SAME lambda,
        // even though their encoders — and therefore their coefficients — differ.
        let e2e = |budget: u64| {
            fx::prepared_run(fx::CalibrationVariant { budget, ..base }, None)
                .tune_encoder()
                .expect("both budgets clear the evidence gate at a calibrated seed and cell")
                .fit_head()
                .expect("both budgets fit a head")
        };
        let (run_small, run_large) = (e2e(12), e2e(20));
        assert_eq!(
            run_small.evidence().effective_lambda(),
            run_large.evidence().effective_lambda(),
            "the recorded objective must be budget-independent",
        );
        assert_eq!(run_small.evidence().effective_lambda(), 1.0 / 48.0);
        assert_eq!(
            run_small.evidence().encode_ledger(),
            run_large.evidence().encode_ledger(),
            "the head's input is the same 24 unique rows at either budget",
        );
    }

    /// A head that cannot converge is a TYPED error, never a silently accepted fit.
    #[test]
    fn fit_head_a_head_that_cannot_converge_surfaces_the_typed_error() {
        let tuned = fx::prepared_run(fx::calibrated_variant(), None)
            .tune_encoder()
            .expect("the calibrated cell passes the evidence gate");
        match tuned.fit_head_with_max_iter(1) {
            Err(SetFitTrainError::HeadFit(HeadFitError::NotConverged {
                iterations,
                gradient_norm,
                tol,
            })) => {
                assert_eq!(iterations, 1, "the budget was one iteration");
                assert!(
                    gradient_norm > tol,
                    "a non-convergence must report a gradient norm ({gradient_norm:e}) above \
                     the tolerance ({tol:e})",
                );
            }
            other => panic!(
                "a one-iteration budget must surface HeadFitError::NotConverged inside \
                 SetFitTrainError; a warning-and-return would hand back coefficients from a \
                 fit that never finished. Got {other:?}",
            ),
        }
    }

    /// The evidence carries the exactly-once proof and the whole stage-one chain.
    #[test]
    fn fit_head_evidence_carries_the_ledger_the_labels_and_the_passed_chain() {
        let run = head_fitted_run(fx::calibrated_variant());
        let evidence = run.evidence();

        let mut ledger: Vec<&str> = evidence.encode_ledger().iter().map(String::as_str).collect();
        assert_eq!(ledger.len(), SELECTED_ROWS);
        ledger.sort_unstable();
        let mut selected = run.selection().ordered_ids();
        selected.sort_unstable();
        assert_eq!(ledger, selected, "the ledger must survive the transition intact");

        let batch = run.config().requested().batch_size() as usize;
        assert_eq!(evidence.encode_call_count(), SELECTED_ROWS.div_ceil(batch));
        assert_eq!(evidence.ordered_labels(), run.dataset().label_names());
        assert_eq!(evidence.ordered_labels(), evidence.head().labels());
        assert!(
            Thresholds::frozen().is_calibrated(&evidence.passed().summary().calibration_regime_id),
            "the complete stage-one chain must survive into HeadFitted, not just a verdict",
        );
    }

    /// The setfit module directory.
    fn setfit_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/train/setfit")
    }

    /// This module's own source, with its test module removed.
    ///
    /// A source guard that reads the file it lives in finds every needle IN ITSELF. Scanning
    /// the whole of `mod.rs` for `pub fn fit_head(self)` would be satisfied by the assertion
    /// that spells it and would stay green if the transition were deleted; the first draft of
    /// this test did exactly that, and its `pub struct HeadFittedEvidence` count came back as
    /// 2. The cut is what turns the scan back into evidence about the shipped code.
    fn shipped_mod_source() -> String {
        let text = std::fs::read_to_string(setfit_dir().join("mod.rs")).expect("mod.rs readable");
        let header = format!("\nmod {} {{", "tests");
        let cut = text.find(&header).expect("mod.rs ends with its test module");
        text[..cut].to_string()
    }

    /// The same source with every comment removed.
    ///
    /// A structural guard has to scan the DECLARATIONS, not the prose about them: this
    /// module's doc comments legitimately quote the shapes the guard forbids, and a scan that
    /// counts them is red for writing the explanation. `negative_leaky.rs` established the
    /// same `split("//")` discipline in Phase 2.
    fn shipped_mod_code() -> String {
        shipped_mod_source()
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The structural claims, read off the SHIPPED source rather than trusted.
    #[test]
    fn fit_head_signature_and_evidence_shape_are_pinned() {
        let shipped = shipped_mod_code();

        assert_eq!(
            shipped.matches("pub fn fit_head(self) -> Result<SetFitRun<HeadFitted>").count(),
            1,
            "fit_head must exist and take ZERO non-self parameters; a pair-shaped argument \
             would make multiplicity expressible again",
        );
        assert_eq!(shipped.matches("pub struct HeadFittedEvidence").count(), 1);

        // B-3. The needle is the ASSOCIATED TYPE declaration, `type Evidence = (`, not the
        // looser `Evidence = (` the plan wrote: the loose form also matches the deliberate
        // `pub type NoEvidence = ();` — a substring straddling a word boundary — and so can
        // never distinguish the empty case from an accidental `(A, B)`.
        let tuple_evidence = format!("{}{}", "type Evidence = ", "(");
        assert_eq!(
            shipped.matches(&tuple_evidence).count(),
            0,
            "no lifecycle state may declare an anonymous tuple as its evidence (B-3)",
        );
        // The guard is two-sided: it must FIRE on the shape it forbids.
        assert_eq!(
            format!("    {tuple_evidence});").matches(&tuple_evidence).count(),
            1,
            "the needle must match the forbidden declaration, or the count above is 0 for \
             the wrong reason",
        );
        let optional_evidence = format!("{}{}", "Option<HeadFittedEvidence", ">");
        assert!(!shipped.contains(&optional_evidence), "the evidence is never optional");

        // Exactly one lambda resolution across the whole module directory, shared with the
        // adversary. Assembled at runtime so this file's own copy is not one of them.
        let resolver = format!("{}{}", "fn resolve_", "lambda");
        let mut sites = 0;
        for entry in std::fs::read_dir(setfit_dir()).expect("the setfit module is readable") {
            let path = entry.expect("a readable directory entry").path();
            if path.extension().is_some_and(|e| e == "rs") {
                sites += std::fs::read_to_string(&path)
                    .expect("every module file is readable")
                    .matches(&resolver)
                    .count();
            }
        }
        assert_eq!(
            sites, 1,
            "a second lambda resolution is how the adversarial control silently stops \
             isolating multiplicity",
        );

        // The six fields the plan names, plus the effective lambda 03-08's bundle needs.
        let source = shipped_mod_source();
        let declaration = source
            .split_once("pub struct HeadFittedEvidence {")
            .expect("the evidence struct is declared")
            .1
            .split_once("\n}")
            .expect("the declaration is closed")
            .0;
        for field in [
            "passed:",
            "head:",
            "report:",
            "effective_lambda:",
            "ordered_labels:",
            "encode_ledger:",
            "encode_call_count:",
        ] {
            assert!(declaration.contains(field), "HeadFittedEvidence must carry `{field}`");
        }
        assert!(!declaration.contains("Option<"), "no field of the evidence may be optional");
        assert!(!declaration.contains("pub "), "every field of the evidence is private");
    }
}
