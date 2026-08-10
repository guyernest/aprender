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

pub mod config;
pub mod epoch;
pub mod reduce;

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
}

impl fmt::Display for SetFitTrainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
