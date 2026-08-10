//! Counter-based dropout masks for the SetFit encoder (TRN-06, D-15).
//!
//! The mask element at `(root_seed, site, forward_ordinal, i)` is a PURE FUNCTION
//! OF ITS INDEX: it is computed directly, without drawing elements `0..i`, and it
//! is bitwise identical on replay. That is what makes TRN-06's "two clean runs
//! agree bitwise" a fact about the type rather than a discipline the training
//! loop has to maintain.
//!
//! # What this displaces, and why
//!
//! [`crate::nn::Dropout`] holds a `Mutex<StdRng>` seeded through `seed_from_u64`.
//! Two independent defects follow from that on a reproducibility path:
//!
//! 1. **Draw `i` depends on every draw before it.** Worker count, evaluation
//!    passes, or an extra forward anywhere upstream shift the whole stream.
//! 2. **`StdRng` is explicitly not stable across `rand` versions.** A dependency
//!    bump silently moves every mask, and therefore every loss value, with no
//!    test failing and no diff to review.
//!
//! `nn::Dropout` is untouched — other consumers keep it. Only the SetFit
//! encoder's four dotted sites are rerouted here.
//!
//! # Every function here is STATELESS
//!
//! No mask function takes `&mut self`, and no generator state crosses any
//! boundary. [`SiteDropout`] does carry two atomics, but they are *coordinates*
//! (the mode flag and the current forward ordinal), never accumulated RNG state:
//! setting them to the same values always reproduces the same masks.
//!
//! # Philox is a STATISTICAL generator, never a CSPRNG
//!
//! Philox 4x32-10 is used here solely for training determinism. It is **not**
//! cryptographic randomness: the key is derived from a caller-visible seed, the
//! stream is seekable by construction, and nothing about it resists an adversary
//! who knows the seed. Never reuse anything in this module for tokens, nonces,
//! salts or key material.
//!
//! # The frozen byte encoding
//!
//! Pinned by [`dropout_rng_tests::dropout_rng_byte_encoding_golden_is_frozen`],
//! whose constants were derived from this table by an independent Python
//! implementation rather than captured from a first run of this code:
//!
//! | Decision | Value |
//! |---|---|
//! | domain tag | `b"apr-setfit-dropout-v1\0"` — 21 ASCII bytes plus one NUL terminator |
//! | root seed | `u64::to_le_bytes`, exactly 8 bytes |
//! | site | the DOTTED HF NAME, UTF-8, no terminator (it is last) |
//! | key truncation | digest bytes `0..8` as two LITTLE-ENDIAN `u32` lanes; `8..32` discarded |
//! | counter | `[element as u32, (element >> 32) as u32, forward_ordinal, 0]` |
//! | 64-bit assembly | `((lanes[1] as u64) << 32) \| (lanes[0] as u64)` — lane 0 is the LOW half |
//! | keep rule | `keep(i) iff assemble64(draw(i)) >= threshold` |
//! | threshold | `floor(p * 2^64)` as `u128`, clamped to `0 ..= 2^64` |
//!
//! ## The tag is this module's OWN
//!
//! `b"apr-setfit-dropout-v1\0"`, deliberately NOT Phase 2's
//! `b"apr-contrastive-v1\0"`. Reusing that tag would give the pair sampler and the
//! dropout sites the same key whenever they share a root seed and a domain string
//! — a silent seed-reuse bug that correlates two streams the design assumes are
//! independent, with nothing failing to announce it.
//!
//! ## Modulo and float scaling are FORBIDDEN
//!
//! The keep decision is a comparison against a `u128` threshold, never
//! `x % n < ...` and never `(x as f64 / 2^64) < p`. Modulo's bias at the top of
//! the range is real and, worse, unauditable — two implementations can both look
//! correct and disagree. Float scaling loses low bits and leaves the rounding
//! mode unstated, so the same `p` can produce different masks on different
//! targets. Both rules are asserted by a source grep in the plan's acceptance
//! criteria, because a comment alone does not survive an edit.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use sha2::{Digest, Sha256};
use trueno_rand::Philox4x32;

use crate::autograd::Tensor;
use crate::nn::transformer::AttentionDropoutMasks;

/// The frozen domain-separation tag.
///
/// The trailing NUL is load-bearing: without a terminator the tag and the seed
/// bytes are ambiguous under concatenation, so a different tag with a different
/// seed could derive the same key.
const DOMAIN_TAG: &[u8] = b"apr-setfit-dropout-v1\0";

/// `2^64`, as the exact `f64` it is (every power of two below 2^1024 is exact).
///
/// Spelled as a literal rather than `2f64.powi(64)` so the constant a reader
/// checks against the contract text is the one the code multiplies by.
const TWO_POW_64_F64: f64 = 18_446_744_073_709_551_616.0;

/// `2^64` as a `u128` — the saturating top of the threshold range.
const TWO_POW_64_U128: u128 = 1_u128 << 64;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// A dropout-mask derivation was asked for something it cannot represent.
///
/// Every variant names the OFFENDING VALUE. A rate rejection that says only
/// "invalid probability" cannot be told apart from a units mistake, and the
/// near-one case below is precisely the one a reader would otherwise assume was
/// fine.
///
/// `PartialEq` but not `Eq`: the payloads are floats.
#[derive(Debug, Clone, PartialEq)]
pub enum DropoutRngError {
    /// `p` was `NaN` or `±Inf`.
    RateNotFinite {
        /// The offending rate.
        observed: f32,
    },

    /// `p` was negative.
    RateNegative {
        /// The offending rate.
        observed: f32,
    },

    /// `p` was at or above 1.0: every element would be dropped.
    RateAtOrAboveOne {
        /// The offending rate.
        observed: f32,
    },

    /// `p` was below 1.0 and still produced a non-finite inverted-dropout scale.
    ///
    /// The case a bare `p >= 1.0` check misses. `p = 1.0 - 1e-40` rounds to
    /// `1.0` in `f32` arithmetic on the `1.0 - p` subtraction, so the scale
    /// `1/(1-p)` is `+inf` and every KEPT element becomes `inf` — a silent
    /// all-NaN forward pass one multiplication later. Validation therefore
    /// rejects on the computed SCALE, not only on the rate.
    RateScaleNotFinite {
        /// The offending rate.
        observed: f32,
        /// The non-finite `1/(1-p)` it produced.
        scale: f32,
    },

    /// A forward-call ordinal did not fit the `u32` counter lane.
    ///
    /// `u32::MAX` itself is rejected, not only values beyond it: the boundary is
    /// exclusive so the representable range and the accepted range are the same
    /// set, and no caller has to reason about an off-by-one at the wrap point.
    /// An accepted wrap would silently REUSE an earlier step's masks, which is
    /// the one failure of this scheme that looks perfectly reproducible.
    ForwardOrdinalOverflow {
        /// The ordinal that does not fit.
        observed: u64,
    },
}

impl std::fmt::Display for DropoutRngError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateNotFinite { observed } => write!(
                f,
                "DropoutRngError::RateNotFinite(dropout rate {observed} is not finite)"
            ),
            Self::RateNegative { observed } => write!(
                f,
                "DropoutRngError::RateNegative(dropout rate {observed} is negative)"
            ),
            Self::RateAtOrAboveOne { observed } => write!(
                f,
                "DropoutRngError::RateAtOrAboveOne(dropout rate {observed} would drop every element)"
            ),
            Self::RateScaleNotFinite { observed, scale } => write!(
                f,
                "DropoutRngError::RateScaleNotFinite(dropout rate {observed} is below 1.0 but its \
                 inverted-dropout scale 1/(1-p) is {scale})"
            ),
            Self::ForwardOrdinalOverflow { observed } => write!(
                f,
                "DropoutRngError::ForwardOrdinalOverflow(forward ordinal {observed} does not fit \
                 the u32 counter lane; the limit is {} exclusive)",
                u32::MAX
            ),
        }
    }
}

impl std::error::Error for DropoutRngError {}

// ---------------------------------------------------------------------------
// Key derivation and draws
// ---------------------------------------------------------------------------

/// A Philox key derived from a `(root_seed, site)` pair.
///
/// Opaque on purpose: the only way to obtain one is [`derive_key`], so no call
/// site can invent a key that skips domain separation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainKey([u32; 2]);

impl DomainKey {
    /// The two Philox key lanes, in derivation order.
    ///
    /// Exposed so a golden test can pin the byte encoding BY VALUE rather than
    /// by behaviour: an endianness or truncation change must be visible as a
    /// number in a diff, not only as a training run that quietly moved.
    #[must_use]
    pub fn lanes(self) -> [u32; 2] {
        self.0
    }
}

/// Derive a domain-separated Philox key for one dotted dropout site.
///
/// `key = trunc64_le(SHA-256(DOMAIN_TAG ‖ root_seed.to_le_bytes() ‖ site.as_bytes()))`,
/// where `trunc64_le` reads digest bytes `0..8` as
/// `[u32::from_le_bytes(d[0..4]), u32::from_le_bytes(d[4..8])]`. Digest bytes
/// `8..32` are discarded.
///
/// Keying on the DOTTED NAME rather than a position is inherited from Phase 1's
/// `site_seed` and is the half of that design worth keeping: inserting a layer
/// must not renumber the streams of the layers after it. The trade is that
/// RENAMING a site changes its stream, which is acceptable — the names are HF's
/// own and are pinned by the parameter-order gate — whereas positional drift is
/// not, because it silently re-addresses every site downstream of an edit.
///
/// The little-endian choices are stated rather than inherited from the host: a
/// big-endian machine reading these bytes natively would derive a different key
/// for the same seed, and the divergence would surface only as a different
/// training trajectory.
#[must_use]
pub fn derive_key(root_seed: u64, site: &str) -> DomainKey {
    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_TAG);
    hasher.update(root_seed.to_le_bytes());
    hasher.update(site.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();

    // A SHA-256 digest is exactly 32 bytes, so both 4-byte windows exist by
    // construction; the explicit element form keeps this total with no `unwrap`.
    let lane0 = u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]);
    let lane1 = u32::from_le_bytes([digest[4], digest[5], digest[6], digest[7]]);
    DomainKey([lane0, lane1])
}

/// One Philox 4x32-10 output block at `(key, forward_ordinal, element)`.
///
/// `counter = [element as u32, (element >> 32) as u32, forward_ordinal, 0]`.
///
/// The `forward_ordinal` lane is D-15's `block` coordinate and it is the reason
/// this module exists in its current shape. `pair_cosine_mse(za, zb, labels)`
/// takes TWO `[B,H]` embedding matrices, so a training step performs TWO
/// SEPARATE encoder forwards — one per siamese branch. Keying only on the step
/// would hand branch A and branch B the identical mask at every corresponding
/// element: an artificial correlation between the two halves of the pair
/// objective, a silent divergence from the reference recipe, and perfectly
/// deterministic-looking. See [`forward_ordinal`] for the `2*step + branch`
/// mapping.
///
/// The result depends on nothing else — not on how many draws preceded it, not
/// on which thread asks, not on the order the elements are requested in.
#[must_use]
pub fn draw(key: &DomainKey, forward_ordinal: u32, element: u64) -> [u32; 4] {
    let counter = [element as u32, (element >> 32) as u32, forward_ordinal, 0];
    Philox4x32::generate_at(key.0, counter)
}

/// Assemble a 64-bit value from an output block: lane 0 is the LOW half.
///
/// Frozen for the same reason the seed encoding is — the opposite convention is
/// equally natural and would silently produce a different, equally
/// plausible-looking stream.
#[must_use]
fn assemble64(lanes: [u32; 4]) -> u64 {
    (u64::from(lanes[1]) << 32) | u64::from(lanes[0])
}

/// The keep threshold for rate `p`, exactly.
///
/// `keep(i)` iff `assemble64(draw(i)) >= keep_threshold(p)`, so the drop
/// probability is `threshold / 2^64`.
///
/// # The rule is spelled out because the obvious phrasing was ambiguous
///
/// The first draft said `round(p * 2^64)`. That is underspecified twice over: it
/// does not state the rounding mode, and `p * 2^64` as an `f64` product cannot
/// represent most results exactly, so "round" and "truncate" disagree on a set
/// of rates nobody would think to test. The rule here is instead:
///
/// ```text
/// threshold = clamp(floor(p * 18446744073709551616.0), 0, 2^64)   // as u128
/// ```
///
/// with `p` widened to `f64` first. Rust's `f64 -> u128` `as` cast is DEFINED
/// (round toward zero, saturating at both ends), so this is bit-reproducible on
/// every target — no `unsafe`, no UB, no platform-dependent intrinsic.
///
/// # Why `u128` and not `u64`
///
/// `p == 1.0` must map to `2^64` — "drop everything", since no 64-bit draw is
/// ever `>= 2^64`. In a `u64` that value wraps to `0`, which means "drop
/// NOTHING": the exact inversion of the intent, produced silently. The threshold
/// is therefore a `u128` and the comparison widens the draw. (Rate validation
/// rejects `p == 1.0` before any [`SiteDropout`] is built; the rule is stated
/// totally anyway so the function is correct on its own terms and testable at
/// the boundary.)
#[must_use]
pub fn keep_threshold(p: f64) -> u128 {
    if !p.is_finite() || p <= 0.0 {
        // Also catches NaN, whose `as` cast is defined to be 0 but reads as an
        // accident at the call site.
        return 0;
    }
    let scaled = (p * TWO_POW_64_F64).floor();
    let raw = scaled as u128;
    raw.min(TWO_POW_64_U128)
}

/// Validate a dropout rate and return its inverted-dropout scale `1/(1-p)`.
///
/// Rejects, with the offending value named, when any of these holds:
/// `!p.is_finite()`, `p < 0.0`, `p >= 1.0`, or the computed `f32` scale is not
/// finite. The last clause is not redundant: `p = 1.0 - 1e-40` passes `p < 1.0`
/// (it is a perfectly ordinary `f32` strictly below one) and still yields an
/// infinite scale, because `1.0 - p` underflows to `0.0` in `f32`.
///
/// Kept elements are multiplied by the returned scale, matching
/// [`crate::nn::Dropout`]'s inverted-dropout semantics exactly, so switching a
/// site to this module changes WHICH elements are dropped and nothing else about
/// the arithmetic.
///
/// # Errors
///
/// [`DropoutRngError::RateNotFinite`], [`DropoutRngError::RateNegative`],
/// [`DropoutRngError::RateAtOrAboveOne`] or
/// [`DropoutRngError::RateScaleNotFinite`], each naming `p`.
pub fn validate_rate(p: f32) -> Result<f32, DropoutRngError> {
    if !p.is_finite() {
        return Err(DropoutRngError::RateNotFinite { observed: p });
    }
    if p < 0.0 {
        return Err(DropoutRngError::RateNegative { observed: p });
    }
    if p >= 1.0 {
        return Err(DropoutRngError::RateAtOrAboveOne { observed: p });
    }
    let scale = 1.0 / (1.0 - p);
    if !scale.is_finite() {
        return Err(DropoutRngError::RateScaleNotFinite { observed: p, scale });
    }
    Ok(scale)
}

/// D-15's `block`: the forward-call ordinal `2 * step + branch`.
///
/// `branch` is 0 for the pair's A sentence and 1 for its B sentence. The mapping
/// is strictly monotone and a pure function of `(step, branch)`, so it is
/// replay-exact and the two branches of one step necessarily draw from different
/// Philox streams.
///
/// # Errors
///
/// [`DropoutRngError::ForwardOrdinalOverflow`] naming the computed ordinal when
/// it does not fit a `u32` counter lane. Checked arithmetic throughout: a `u64`
/// overflow here would wrap to a SMALL ordinal and reuse an early step's masks.
pub fn forward_ordinal(step: u64, branch: u32) -> Result<u32, DropoutRngError> {
    let doubled = step
        .checked_mul(2)
        .and_then(|s| s.checked_add(u64::from(branch)));
    match doubled {
        Some(ordinal) => checked_forward_ordinal(ordinal),
        // `2*step + branch` overflowed u64 itself. There is no honest `observed`
        // to report other than the step that produced it, so report the step
        // doubled in the widest type available.
        None => Err(DropoutRngError::ForwardOrdinalOverflow { observed: u64::MAX }),
    }
}

/// Narrow a forward ordinal to the `u32` counter lane, or reject it.
///
/// # Errors
///
/// [`DropoutRngError::ForwardOrdinalOverflow`] naming `ordinal` when it is at or
/// above `u32::MAX`.
pub fn checked_forward_ordinal(ordinal: u64) -> Result<u32, DropoutRngError> {
    if ordinal >= u64::from(u32::MAX) {
        return Err(DropoutRngError::ForwardOrdinalOverflow { observed: ordinal });
    }
    // Below u32::MAX by the check above, so the conversion is total.
    u32::try_from(ordinal)
        .map_err(|_| DropoutRngError::ForwardOrdinalOverflow { observed: ordinal })
}

// ---------------------------------------------------------------------------
// The site
// ---------------------------------------------------------------------------

/// One dotted dropout site's mask source.
///
/// Replaces [`crate::nn::Dropout`] on the SetFit encoder route only. The mode
/// flag and the forward ordinal are interior-mutable because a forward pass runs
/// through `&self` all the way down and the ordinal has to reach four sites per
/// layer; they are COORDINATES, not accumulated state, so nothing about
/// reproducibility depends on how many times they were read.
#[derive(Debug)]
pub struct SiteDropout {
    /// The dotted HF name this site's stream is keyed on.
    site: String,
    key: DomainKey,
    p: f32,
    /// `1/(1-p)`, validated finite at construction.
    scale: f32,
    /// `floor(p * 2^64)`, clamped — see [`keep_threshold`].
    threshold: u128,
    training: AtomicBool,
    forward_ordinal: AtomicU32,
}

impl SiteDropout {
    /// Build a site keyed on `(root_seed, site)` at rate `p`.
    ///
    /// Starts in TRAINING mode at forward ordinal 0, matching
    /// [`crate::nn::Dropout::with_seed`], so the encoder's existing
    /// `set_training(false)` at the end of construction still lands the model in
    /// eval mode exactly as it did before.
    ///
    /// # Errors
    ///
    /// Whatever [`validate_rate`] rejects, naming `p`.
    pub fn new(root_seed: u64, site: &str, p: f32) -> Result<Self, DropoutRngError> {
        let scale = validate_rate(p)?;
        Ok(Self {
            site: site.to_string(),
            key: derive_key(root_seed, site),
            p,
            scale,
            threshold: keep_threshold(f64::from(p)),
            training: AtomicBool::new(true),
            forward_ordinal: AtomicU32::new(0),
        })
    }

    /// The dotted HF name this site is keyed on.
    #[must_use]
    pub fn site(&self) -> &str {
        &self.site
    }

    /// The derived Philox key.
    ///
    /// A READ accessor, so "every site has its own stream" can be asserted on the
    /// derivation itself rather than inferred from outputs that differ.
    #[must_use]
    pub fn key(&self) -> DomainKey {
        self.key
    }

    /// The dropout probability.
    #[must_use]
    pub fn probability(&self) -> f32 {
        self.p
    }

    /// The inverted-dropout scale `1/(1-p)` applied to kept elements.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The keep threshold, `floor(p * 2^64)`.
    #[must_use]
    pub fn threshold(&self) -> u128 {
        self.threshold
    }

    /// Whether this site is in training mode.
    #[must_use]
    pub fn training(&self) -> bool {
        self.training.load(Ordering::Relaxed)
    }

    /// Flip the mode. Takes `&self`: see the type docs.
    pub fn set_training(&self, training: bool) {
        self.training.store(training, Ordering::Relaxed);
    }

    /// The forward-call ordinal this site currently draws at.
    #[must_use]
    pub fn current_forward_ordinal(&self) -> u32 {
        self.forward_ordinal.load(Ordering::Relaxed)
    }

    /// Point this site at forward ordinal `ordinal`.
    ///
    /// # Errors
    ///
    /// [`DropoutRngError::ForwardOrdinalOverflow`] naming `ordinal`; the site is
    /// left at its previous ordinal in that case.
    pub fn set_forward_ordinal(&self, ordinal: u64) -> Result<(), DropoutRngError> {
        let narrowed = checked_forward_ordinal(ordinal)?;
        self.forward_ordinal.store(narrowed, Ordering::Relaxed);
        Ok(())
    }

    /// The inverted-dropout multiplier for element `i` at `forward_ordinal`.
    ///
    /// `0.0` when dropped, [`Self::scale`] when kept. THE definition — every
    /// other mask function in this module is `(0..len).map(mask_element)` — so
    /// "pure function of the index" is structural rather than a property some
    /// vectorized fast path might not share.
    #[must_use]
    pub fn mask_element(&self, forward_ordinal: u32, i: u64) -> f32 {
        let x = assemble64(draw(&self.key, forward_ordinal, i));
        if u128::from(x) >= self.threshold {
            self.scale
        } else {
            0.0
        }
    }

    /// `len` inverted-dropout multipliers at an EXPLICIT forward ordinal.
    #[must_use]
    pub fn mask_at(&self, forward_ordinal: u32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| self.mask_element(forward_ordinal, i as u64))
            .collect()
    }

    /// `len` inverted-dropout multipliers at the CURRENT forward ordinal.
    #[must_use]
    pub fn mask(&self, len: usize) -> Vec<f32> {
        self.mask_at(self.current_forward_ordinal(), len)
    }

    /// Apply this site: identity in eval mode or at `p == 0`, masked otherwise.
    ///
    /// The mask is a non-grad CONSTANT tensor applied with the autograd-aware
    /// [`Tensor::mul`], exactly as `nn::Dropout` does post-PMAT-922. Building the
    /// scaled values into a fresh `Tensor::new` leaf instead would SEVER the
    /// graph and freeze every parameter upstream of the site — the whole reason
    /// this shape is copied rather than reinvented.
    #[must_use]
    pub fn forward(&self, input: &Tensor) -> Tensor {
        if !self.training() || self.p == 0.0 {
            return input.clone();
        }
        let mask_data = self.mask(input.data().len());
        let mask = Tensor::new(&mask_data, input.shape());
        input.mul(&mask)
    }
}

/// The attention-probs site (site 2) reaches this same implementation.
///
/// One mask implementation for all four dotted sites: the site that lives INSIDE
/// `MultiHeadAttention` cannot be a second hand-rolled derivation, or D-15's
/// branch independence would hold at three sites and silently not at the fourth.
impl AttentionDropoutMasks for SiteDropout {
    fn attention_dropout_mask(&self, len: usize) -> Vec<f32> {
        self.mask(len)
    }
}
