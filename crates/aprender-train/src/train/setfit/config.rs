//! The SetFit trainer's twelve configuration knobs, in two types.
//!
//! Contract: `setfit-train-lifecycle-v1` (authored in plan 03-06). Requirement: TRN-02.
//!
//! # Two types, because "what was asked for" and "what was resolved" are different facts
//!
//! [`SetFitTrainConfig`] is the REQUESTED form. It is serializable, it holds the device as
//! a string-shaped [`DeviceRequest`] rather than a probed `Device`, and it is what plan
//! 03-08's artifact bundle embeds and hashes. [`ResolvedSetFitConfig`] adds the facts only
//! the host can supply — the probed `Device` — and is produced by
//! `SetFitRun::<Prepared>::prepare`. It is `Serialize` and deliberately NOT `Deserialize`:
//! a resolved runtime device arriving from a file would be an assertion about a machine
//! nobody checked (T-3-50).
//!
//! # The parser / probe split
//!
//! Copied in shape from `train/device.rs`: every knob has ONE pure validator that touches
//! no environment, and the probe happens later, at `prepare()`. That is what lets the
//! FALSIFY tables run identically on a CUDA host and a CPU host.

use core::fmt;

use crate::train::device::{resolve_device, Device, DeviceError};
use aprender::setfit::{FreezeGroup, MAX_SEQUENCE_LENGTH};
use aprender_contrastive_data::pairs::PairConfig;

/// The learning-rate schedule. v1 ships exactly one, and the enum exists so a future
/// second schedule is a versioned change rather than a silent one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum LrSchedule {
    /// Linear warmup to `encoder_lr`, then linear decay to zero — the reference recipe.
    #[default]
    WarmupLinearDecay,
}

/// How the multiclass head's L2 penalty is expressed.
///
/// The native form is `lambda`; `SklearnEquivalentC` is the reference-comparison form and
/// resolves at fit time through the contracted relation `lambda = 1 / (2 * C * n)` with
/// `n` the number of UNIQUE SELECTED ROWS. The relation itself, its half-constant, its
/// sum-vs-mean convention and its intercept exclusion are the plan 03-04 / 03-06 contract
/// equation — this type carries the request, never the arithmetic.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum HeadRegularization {
    /// Native: the penalty coefficient on `||W||^2`, intercept unpenalized.
    Lambda(f64),
    /// sklearn's inverse regularization strength.
    SklearnEquivalentC {
        /// sklearn's `C`. Strictly positive and finite.
        c: f64,
    },
}

/// A device string as REQUESTED, before any probe.
///
/// A newtype rather than a bare `String` so a probed [`Device`] and a requested spec
/// cannot be swapped at a call site: they are different facts with different trust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRequest(String);

impl DeviceRequest {
    /// The requested spec, verbatim.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DeviceRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The twelve requested knobs.
///
/// Every field is private; the only door is the validating constructor. Accessors are
/// read-only, so a value of this type is a validated value for its whole life.
#[derive(Debug, Clone, PartialEq)]
pub struct SetFitTrainConfig {
    encoder_lr: f64,
    epochs: u32,
    batch_size: u32,
    warmup_ratio: f64,
    grad_clip_max_norm: f32,
    max_length: u32,
    pair_config: PairConfig,
    freeze_policy: Vec<FreezeGroup>,
    head_regularization: HeadRegularization,
    root_seed: u64,
    device: DeviceRequest,
    lr_schedule: LrSchedule,
}

/// The requested configuration plus the facts only the host can supply.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSetFitConfig {
    requested: SetFitTrainConfig,
    device: Device,
}

impl ResolvedSetFitConfig {
    /// The requested form this was resolved from.
    #[must_use]
    pub fn requested(&self) -> &SetFitTrainConfig {
        &self.requested
    }

    /// The probed device.
    #[must_use]
    pub fn device(&self) -> Device {
        self.device
    }
}

/// Failure modes of the twelve-knob table.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SetFitConfigError {
    /// A knob whose value must be finite and strictly positive was not.
    NotFinitePositive {
        /// The knob's name, exactly as it is spelled in the wire form.
        knob: &'static str,
        /// The value that was observed.
        observed: f64,
    },
    /// `max_length` may only be the tokenizer's pinned bound.
    MaxLengthNotSupported {
        /// What the caller asked for.
        requested: u32,
        /// [`MAX_SEQUENCE_LENGTH`], the only supported value.
        pinned: u32,
    },
    /// A device spec that did not parse, or a probe that failed closed.
    Device(DeviceError),
}

impl fmt::Display for SetFitConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinitePositive { knob, observed } => write!(
                f,
                "`{knob}` must be finite and strictly positive, observed {observed} \
                 (contract setfit-train-lifecycle-v1, requirement TRN-02)",
            ),
            Self::MaxLengthNotSupported { requested, pinned } => write!(
                f,
                "`max_length` {requested} is not supported: the pinned tokenizer truncates \
                 at {pinned} and takes no max-length parameter \
                 (contract setfit-train-lifecycle-v1, requirement TRN-02)",
            ),
            Self::Device(inner) => write!(f, "`device` rejected: {inner}"),
        }
    }
}

impl std::error::Error for SetFitConfigError {}

impl From<DeviceError> for SetFitConfigError {
    fn from(inner: DeviceError) -> Self {
        Self::Device(inner)
    }
}

impl SetFitTrainConfig {
    /// TASK-1 SKELETON — replaced by the validating constructor in Task 2.
    ///
    /// This body performs NO validation on purpose. Task 2 is a TDD task whose FALSIFY
    /// tables are written first and must be able to RUN and FAIL against a compiled
    /// interface; a constructor that did not exist yet would give a compile error instead
    /// of an executable RED. It is `pub(crate)` so no out-of-crate caller can reach the
    /// permissive form even within this single plan's intermediate commit.
    pub(crate) fn from_parts_unvalidated(
        encoder_lr: f64,
        epochs: u32,
        batch_size: u32,
        warmup_ratio: f64,
        grad_clip_max_norm: f32,
        max_length: u32,
        pair_config: PairConfig,
        freeze_policy: Vec<FreezeGroup>,
        head_regularization: HeadRegularization,
        root_seed: u64,
        device: &str,
        lr_schedule: LrSchedule,
    ) -> Self {
        Self {
            encoder_lr,
            epochs,
            batch_size,
            warmup_ratio,
            grad_clip_max_norm,
            max_length,
            pair_config,
            freeze_policy,
            head_regularization,
            root_seed,
            device: DeviceRequest(device.to_string()),
            lr_schedule,
        }
    }

    /// The SetFit reference recipe's defaults.
    ///
    /// `root_seed` has no default by design: a seed nobody chose is a reproducibility
    /// claim nobody made.
    #[must_use]
    pub fn reference_defaults(root_seed: u64) -> Self {
        Self::from_parts_unvalidated(
            REFERENCE_ENCODER_LR,
            REFERENCE_EPOCHS,
            REFERENCE_BATCH_SIZE,
            REFERENCE_WARMUP_RATIO,
            REFERENCE_GRAD_CLIP_MAX_NORM,
            #[allow(clippy::cast_possible_truncation)]
            {
                MAX_SEQUENCE_LENGTH as u32
            },
            PairConfig::new(root_seed),
            Vec::new(),
            HeadRegularization::Lambda(0.0),
            root_seed,
            "cpu",
            LrSchedule::WarmupLinearDecay,
        )
    }

    /// Encoder learning rate.
    #[must_use]
    pub fn encoder_lr(&self) -> f64 {
        self.encoder_lr
    }

    /// Number of contrastive epochs.
    #[must_use]
    pub fn epochs(&self) -> u32 {
        self.epochs
    }

    /// Pair batch size.
    #[must_use]
    pub fn batch_size(&self) -> u32 {
        self.batch_size
    }

    /// Warmup fraction of total steps.
    #[must_use]
    pub fn warmup_ratio(&self) -> f64 {
        self.warmup_ratio
    }

    /// Gradient-clipping max norm.
    #[must_use]
    pub fn grad_clip_max_norm(&self) -> f32 {
        self.grad_clip_max_norm
    }

    /// Requested maximum sequence length.
    #[must_use]
    pub fn max_length(&self) -> u32 {
        self.max_length
    }

    /// The pair-stream configuration, delegated wholesale to Phase 2.
    #[must_use]
    pub fn pair_config(&self) -> &PairConfig {
        &self.pair_config
    }

    /// The freeze policy. Empty is D-20's all-trainable default.
    #[must_use]
    pub fn freeze_policy(&self) -> &[FreezeGroup] {
        &self.freeze_policy
    }

    /// The head's regularization request.
    #[must_use]
    pub fn head_regularization(&self) -> HeadRegularization {
        self.head_regularization
    }

    /// The root seed every RNG domain derives from.
    #[must_use]
    pub fn root_seed(&self) -> u64 {
        self.root_seed
    }

    /// The REQUESTED device spec, unprobed.
    #[must_use]
    pub fn device(&self) -> &DeviceRequest {
        &self.device
    }

    /// The learning-rate schedule.
    #[must_use]
    pub fn lr_schedule(&self) -> LrSchedule {
        self.lr_schedule
    }

    /// Probe the host and pair the requested form with the resolved device.
    ///
    /// # Errors
    ///
    /// [`SetFitConfigError::Device`] wrapping whatever `resolve_device` fails closed with
    /// — an unparseable spec, or an explicit CUDA request on a host without CUDA.
    pub fn resolve(self) -> Result<ResolvedSetFitConfig, SetFitConfigError> {
        let device = resolve_device(self.device.as_str())?;
        Ok(ResolvedSetFitConfig { requested: self, device })
    }
}

/// Reference encoder learning rate (SetFit recipe).
pub const REFERENCE_ENCODER_LR: f64 = 2e-5;
/// Reference epoch count.
pub const REFERENCE_EPOCHS: u32 = 1;
/// Reference pair batch size.
pub const REFERENCE_BATCH_SIZE: u32 = 16;
/// Reference warmup fraction.
pub const REFERENCE_WARMUP_RATIO: f64 = 0.1;
/// Reference gradient-clipping max norm.
pub const REFERENCE_GRAD_CLIP_MAX_NORM: f32 = 1.0;
