//! Bounded pair sampling: canonical pairs, capacity math, budget resolution, and the
//! singleton and degenerate-layout policies.
//!
//! # Contract: contrastive-pair-protocol-v1.yaml
//!
//! Equations `canonical_pair`, `positive_capacity`, `negative_capacity`,
//! `default_epoch_budget`, `budget_resolution`, `pair_stream_degenerate_policy`,
//! `pair_stream`, `singleton_policy`, `untrusted_pair_ingest`, `split_span_fail_closed`.
//!
//! # Counting is cheap; enumerating is not
//!
//! SetFit's oversampling *count* is a closed form over the per-class sizes — `O(K)` — while
//! only ENUMERATING the pairs is quadratic. That asymmetry is the whole reason fidelity and
//! boundedness are compatible here (D-14): Aprender reproduces the reference's per-epoch
//! COUNT exactly while never materializing the pair set the reference's own
//! `np.triu_indices(n)` builds.
//!
//! # Everything fallible is typed
//!
//! Every capacity function returns `Result<u64, ContrastiveDataError>` and uses checked
//! arithmetic at every step. A wrapped capacity is not an obviously wrong huge number — it
//! is a small, plausible-looking one, and it would silently under-sample forever while
//! every balance and membership test stayed green.

use core::cmp::Ordering;

use crate::error::ContrastiveDataError;
use crate::select::SelectedId;

/// The default pair hard cap: 2²⁰.
///
/// This value is contract-resident (`default_epoch_budget`) and must byte-match the
/// contract. "Configurable hard cap" is locked D-14 text; the *value* is a discretion
/// choice, sized so the clamp NEVER engages for a contracted layout — the largest
/// contracted closed form is 24,576, forty-two times below this cap. The cap is a
/// denial-of-service ceiling for adversarial inputs, not a tuning knob that quietly
/// reshapes normal runs.
pub const DEFAULT_HARD_CAP: u64 = 1_048_576;

/// The version tag of the degenerate-layout policy recorded in every pair replay record.
///
/// Version-tagged because an undefined degenerate case is not an edge case; it is a place
/// where two implementations silently disagree.
pub const DEGENERATE_POLICY_VERSION: u32 = 1;

// ===========================================================================================
// 1. Pair identity (D-10 / D-12)
// ===========================================================================================

/// An unordered, self-pair-free pair of [`SelectedId`] ordinals.
///
/// The fields are PRIVATE and [`CanonicalPair::new`] is the SOLE constructor, so both
/// orientations of one unordered pair are the same value and `(x, x)` is unrepresentable
/// (D-12). No configuration value can resurrect a self-pair, and a conflicting label for
/// the same unordered pair is structurally impossible rather than merely tested against.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CanonicalPair {
    lo: SelectedId,
    hi: SelectedId,
}

impl CanonicalPair {
    /// The only way to build a pair.
    ///
    /// # Errors
    ///
    /// [`ContrastiveDataError::SelfPair`] when both endpoints are the same ordinal.
    #[provable_contracts_macros::contract(
        "contrastive-pair-protocol-v1",
        equation = "canonical_pair"
    )]
    pub fn new(a: SelectedId, b: SelectedId) -> Result<Self, ContrastiveDataError> {
        match a.cmp(&b) {
            Ordering::Less => Ok(Self { lo: a, hi: b }),
            Ordering::Greater => Ok(Self { lo: b, hi: a }),
            Ordering::Equal => Err(ContrastiveDataError::SelfPair {
                id: u64::from(a.ordinal()),
            }),
        }
    }

    /// The lower endpoint.
    pub fn lo(&self) -> SelectedId {
        self.lo
    }

    /// The upper endpoint.
    pub fn hi(&self) -> SelectedId {
        self.hi
    }
}

/// A canonical pair plus the target DERIVED from its endpoints' classes.
///
/// `1.0` when both endpoints share a class, `0.0` otherwise. The target is never accepted
/// from caller input, because a caller-supplied target is precisely how a poisoned pair
/// claims to be a positive.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LabeledPair {
    /// The unordered endpoint pair.
    pub pair: CanonicalPair,
    /// `1.0` same class, `0.0` different class. Derived, never supplied.
    pub target: f32,
}

// ===========================================================================================
// 2. Closed-form capacity (D-14) — all fallible
// ===========================================================================================

fn overflow(operation: &str) -> ContrastiveDataError {
    ContrastiveDataError::ArithmeticOverflow {
        operation: operation.to_string(),
    }
}

/// `Σ_k C(n_k, 2)` — the number of distinct same-class unordered pairs.
///
/// `O(K)` in the number of classes. Self-pairs are EXCLUDED, hence `n(n−1)/2` rather than
/// `n(n+1)/2`: that exclusion is an Aprender policy (deviation clause 3), NOT SetFit's
/// behaviour — the pinned `setfit==1.1.3` enumerates `np.triu_indices(n, 0)` and therefore
/// includes the diagonal, contradicting its own published documentation.
///
/// A singleton class contributes exactly 0, which is what makes
/// [`SingletonPolicy::NegativesOnly`] fall out of the arithmetic instead of needing a
/// special case.
///
/// # Errors
///
/// [`ContrastiveDataError::ArithmeticOverflow`] naming the operation that overflowed.
#[provable_contracts_macros::contract(
    "contrastive-pair-protocol-v1",
    equation = "positive_capacity"
)]
pub fn positive_capacity(class_sizes: &[u64]) -> Result<u64, ContrastiveDataError> {
    let mut total: u64 = 0;
    for &n in class_sizes {
        // `n * (n - 1)` is always even, so the halving is exact and loses nothing.
        let product = n
            .checked_mul(n.saturating_sub(1))
            .ok_or_else(|| overflow("positive_capacity/class_product"))?;
        total = total
            .checked_add(product / 2)
            .ok_or_else(|| overflow("positive_capacity/total"))?;
    }
    Ok(total)
}

/// `Σ_{j<k} n_j · n_k` — the number of distinct cross-class unordered pairs.
///
/// # Why the running-prefix evaluation order
///
/// The contract's formula line gives the algebraically equivalent `(S² − Σ n_k²) / 2`.
/// Both are `O(K)` and neither enumerates class PAIRS — the property the contract's
/// invariant actually protects — but `S²` overflows `u64` long before the true capacity
/// does, so it would report `ArithmeticOverflow` for layouts whose answer fits perfectly
/// well. This function therefore accumulates `Σ_k n_k · (Σ_{j<k} n_j)`, which overflows
/// exactly when the RESULT does. `negative_capacity_agrees_with_the_sum_of_squares_derivation`
/// pins the two against each other wherever the second is computable at all, so the
/// evaluation order cannot drift into a different quantity.
///
/// # Errors
///
/// [`ContrastiveDataError::ArithmeticOverflow`] naming the operation that overflowed.
#[provable_contracts_macros::contract(
    "contrastive-pair-protocol-v1",
    equation = "negative_capacity"
)]
pub fn negative_capacity(class_sizes: &[u64]) -> Result<u64, ContrastiveDataError> {
    let mut seen: u64 = 0;
    let mut total: u64 = 0;
    for &n in class_sizes {
        let cross = n
            .checked_mul(seen)
            .ok_or_else(|| overflow("negative_capacity/cross_product"))?;
        total = total
            .checked_add(cross)
            .ok_or_else(|| overflow("negative_capacity/total"))?;
        seen = seen
            .checked_add(n)
            .ok_or_else(|| overflow("negative_capacity/running_total"))?;
    }
    Ok(total)
}

/// The RAW closed form `2 · max(positive_capacity, negative_capacity)`, before any clamp.
///
/// This is D-14's oversampling count. Note it is NOT the contracted default budget —
/// [`effective_default_budget`] is, because the contracted equation includes the cap.
///
/// # Errors
///
/// [`ContrastiveDataError::ArithmeticOverflow`] naming the operation that overflowed.
pub fn default_epoch_budget(class_sizes: &[u64]) -> Result<u64, ContrastiveDataError> {
    let pos = positive_capacity(class_sizes)?;
    let neg = negative_capacity(class_sizes)?;
    pos.max(neg)
        .checked_mul(2)
        .ok_or_else(|| overflow("default_epoch_budget/balanced_total"))
}

/// The CONTRACTED default: `min(closed_form, hard_cap)`.
///
/// # Errors
///
/// [`ContrastiveDataError::ZeroHardCap`] for a zero cap;
/// [`ContrastiveDataError::ArithmeticOverflow`] from the closed form.
#[provable_contracts_macros::contract(
    "contrastive-pair-protocol-v1",
    equation = "default_epoch_budget"
)]
pub fn effective_default_budget(
    class_sizes: &[u64],
    hard_cap: u64,
) -> Result<u64, ContrastiveDataError> {
    if hard_cap == 0 {
        return Err(ContrastiveDataError::ZeroHardCap);
    }
    Ok(default_epoch_budget(class_sizes)?.min(hard_cap))
}

/// Pre-provisioned capacity check for the `unique` strategy, which does NOT ship in v1.
///
/// # Scope note, so this is not mistaken for dead API
///
/// D-11 locks the `unique` strategy's semantics *if it is present*, and the research
/// recommendation adopted by this plan ships `oversampling` only. The capacity check ships
/// anyway because D-11's hard part — a closed-form capacity that fails closed instead of
/// rejection-looping — is needed for the cap regardless, and because a typed error variant
/// no code path can raise is a claim nothing checks. It is public, documented and tested.
///
/// # Errors
///
/// [`ContrastiveDataError::BudgetExceedsCapacity`] when the budget exceeds `pos + neg`.
pub fn unique_capacity_check(class_sizes: &[u64], budget: u64) -> Result<(), ContrastiveDataError> {
    let pos = positive_capacity(class_sizes)?;
    let neg = negative_capacity(class_sizes)?;
    let capacity = pos
        .checked_add(neg)
        .ok_or_else(|| overflow("unique_capacity_check/total"))?;
    if budget > capacity {
        return Err(ContrastiveDataError::BudgetExceedsCapacity { budget, capacity });
    }
    Ok(())
}

// ===========================================================================================
// 3. Versioned policy enums and the pair configuration
// ===========================================================================================

/// The sampling strategy. v1 ships exactly one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub enum PairStrategy {
    /// Draw with replacement to the resolved budget (D-14).
    #[default]
    Oversampling,
}

impl PairStrategy {
    /// The wire name.
    pub fn as_str(self) -> &'static str {
        "oversampling"
    }

    /// The version tag recorded beside the name.
    pub fn strategy_version(self) -> u32 {
        1
    }
}

/// What a class with exactly one selected example does. v1 ships exactly one variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub enum SingletonPolicy {
    /// No positives (`C(1,2) = 0` falls out of the arithmetic), still present in negatives.
    #[default]
    NegativesOnly,
}

impl SingletonPolicy {
    /// The wire name.
    pub fn as_str(self) -> &'static str {
        "negatives_only"
    }

    /// The version tag recorded beside the name.
    pub fn policy_version(self) -> u32 {
        1
    }
}

/// What the stream actually emitted, recorded in the replay record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum EmittedKinds {
    /// Both kinds, alternating.
    Both,
    /// Positives only — a single class with two or more members.
    PositivesOnly,
    /// Negatives only — every class a singleton.
    NegativesOnly,
}

impl EmittedKinds {
    /// The wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::PositivesOnly => "positives_only",
            Self::NegativesOnly => "negatives_only",
        }
    }
}

/// The caller-supplied pair-stream configuration.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PairConfig {
    /// The root seed every draw key is derived from.
    pub root_seed: u64,
    /// The strategy. v1: `Oversampling`.
    pub strategy: PairStrategy,
    /// The singleton policy. v1: `NegativesOnly`.
    pub singleton_policy: SingletonPolicy,
    /// `None` resolves to `min(closed_form, hard_cap)`.
    pub budget: Option<u64>,
    /// `None` resolves to [`DEFAULT_HARD_CAP`].
    pub hard_cap: Option<u64>,
}

impl PairConfig {
    /// A configuration with default policy versions and a default budget and cap.
    pub fn new(root_seed: u64) -> Self {
        Self {
            root_seed,
            strategy: PairStrategy::Oversampling,
            singleton_policy: SingletonPolicy::NegativesOnly,
            budget: None,
            hard_cap: None,
        }
    }

    /// The resolved hard cap.
    pub fn resolved_hard_cap(&self) -> u64 {
        self.hard_cap.unwrap_or(DEFAULT_HARD_CAP)
    }
}

/// Resolve the effective budget, and report whether the DEFAULT clamp engaged.
///
/// # Errors
///
/// [`ContrastiveDataError::ZeroHardCap`], [`ContrastiveDataError::ZeroBudget`],
/// [`ContrastiveDataError::BudgetExceedsHardCap`], [`ContrastiveDataError::NoPairCapacity`],
/// [`ContrastiveDataError::ArithmeticOverflow`].
/// # The cap BINDS an explicit budget — it does not clamp it
///
/// Silent clamping would keep the cap's denial-of-service role while discarding a number
/// the user typed: the run would succeed and produce a DIFFERENT dataset than the one
/// requested, with nothing red and a manifest that looks fine. That is the worst
/// reproducibility outcome available. Dropping the cap for explicit budgets would remove
/// its DoS role entirely. Failing loudly and letting the user raise `--hard-cap` keeps both
/// properties, and makes the decision visible in the command that was run.
///
/// # Ordering, and one place the contract's formula and its invariants disagree
///
/// The formula line orders `effective budget == 0 -> ZeroBudget` before
/// `pos == 0 and neg == 0 -> NoPairCapacity`, but the same equation's invariant prose says
/// "pos == 0 AND neg == 0 ... is `NoPairCapacity{...}`. There is nothing to emit." Taken
/// literally, the formula's order would report `ZeroBudget` for `[1]` under a DEFAULT
/// budget — because the closed form is then 0 — which names the request when the layout is
/// what is wrong. This function follows the invariant: a zero cap (configuration defect)
/// first, then an explicit zero or over-cap budget (request defects, wrong whatever the
/// layout is), then absent capacity, then resolution. Each rung names the thing the reader
/// has to change.
#[provable_contracts_macros::contract(
    "contrastive-pair-protocol-v1",
    equation = "budget_resolution"
)]
pub fn resolve_budget(
    cfg: &PairConfig,
    class_sizes: &[u64],
) -> Result<(u64, bool), ContrastiveDataError> {
    let hard_cap = cfg.resolved_hard_cap();
    if hard_cap == 0 {
        return Err(ContrastiveDataError::ZeroHardCap);
    }
    if let Some(budget) = cfg.budget {
        if budget == 0 {
            return Err(ContrastiveDataError::ZeroBudget);
        }
        if budget > hard_cap {
            return Err(ContrastiveDataError::BudgetExceedsHardCap { budget, hard_cap });
        }
    }

    let pos = positive_capacity(class_sizes)?;
    let neg = negative_capacity(class_sizes)?;
    classify_degenerate(pos, neg)?;

    match cfg.budget {
        Some(budget) => Ok((budget, false)),
        None => {
            let closed_form = default_epoch_budget(class_sizes)?;
            let resolved = closed_form.min(hard_cap);
            if resolved == 0 {
                // Unreachable while `classify_degenerate` above accepts only layouts with
                // capacity, but typed rather than asserted so a future edit to that rung
                // cannot silently produce an empty stream.
                return Err(ContrastiveDataError::ZeroBudget);
            }
            Ok((resolved, closed_form > hard_cap))
        }
    }
}

/// Which kinds a layout can emit — total over every degenerate case.
///
/// * `pos == 0 && neg == 0` — a single class of size ≤ 1, or no examples at all — is
///   [`ContrastiveDataError::NoPairCapacity`]. There is nothing to emit, and reporting a
///   budget problem here would point at the wrong file.
/// * `pos == 0 && neg > 0` — every class a singleton, the K ≈ N adversarial layout — emits
///   NEGATIVES ONLY. It is not an error: the layout is legal and its pair space is
///   non-empty. This is [`SingletonPolicy::NegativesOnly`] arriving as arithmetic.
/// * `neg == 0 && pos > 0` — one class with two or more members — emits POSITIVES ONLY.
/// * otherwise both kinds alternate.
///
/// `emitted_kinds` is recorded even in the ordinary both-kinds case, so its absence cannot
/// be confused with the ordinary case.
///
/// # Errors
///
/// [`ContrastiveDataError::NoPairCapacity`] when neither kind has any capacity.
#[provable_contracts_macros::contract(
    "contrastive-pair-protocol-v1",
    equation = "pair_stream_degenerate_policy"
)]
pub fn classify_degenerate(pos: u64, neg: u64) -> Result<EmittedKinds, ContrastiveDataError> {
    match (pos, neg) {
        (0, 0) => Err(ContrastiveDataError::NoPairCapacity {
            positive_capacity: 0,
            negative_capacity: 0,
        }),
        (0, _) => Ok(EmittedKinds::NegativesOnly),
        (_, 0) => Ok(EmittedKinds::PositivesOnly),
        _ => Ok(EmittedKinds::Both),
    }
}

#[cfg(test)]
mod pair_tests {
    use super::{
        classify_degenerate, default_epoch_budget, effective_default_budget, negative_capacity,
        positive_capacity, resolve_budget, unique_capacity_check, CanonicalPair, EmittedKinds,
        PairConfig, PairStrategy, SingletonPolicy, DEFAULT_HARD_CAP, DEGENERATE_POLICY_VERSION,
    };
    use crate::error::ContrastiveDataError;
    use crate::select::{test_corpus, SelectedId};

    /// Twenty-four selected ordinals (8 shots × 3 classes) in selection order.
    fn ordinals() -> Vec<SelectedId> {
        let (selection, _) = test_corpus::fresh_selection(12, 13, 8);
        selection
            .examples()
            .iter()
            .map(|row| {
                selection
                    .selected_id(&row.id)
                    .expect("every selected example resolves to its own ordinal")
            })
            .collect()
    }

    #[test]
    fn canonical_pair_is_orientation_free_and_rejects_self_pairs() {
        let ids = ordinals();
        let (a, b) = (ids[2], ids[9]);

        let forward = CanonicalPair::new(a, b).expect("distinct endpoints pair");
        let backward = CanonicalPair::new(b, a).expect("distinct endpoints pair");
        assert_eq!(forward, backward, "orientation carries no information");
        assert!(forward.lo() < forward.hi(), "lo < hi always");
        assert_eq!(forward.lo(), a);
        assert_eq!(forward.hi(), b);

        match CanonicalPair::new(a, a).expect_err("a self-pair must be refused") {
            ContrastiveDataError::SelfPair { id } => assert_eq!(id, u64::from(a.ordinal())),
            other => panic!("expected SelfPair, got {other:?}"),
        }
    }

    #[test]
    fn positive_capacity_matches_the_contracted_closed_form() {
        assert_eq!(positive_capacity(&[8, 4, 8]).expect("no overflow"), 62);
        assert_eq!(positive_capacity(&[8, 8, 8]).expect("no overflow"), 84);
        assert_eq!(positive_capacity(&[64, 64, 64]).expect("no overflow"), 6048);
        assert_eq!(positive_capacity(&[6]).expect("no overflow"), 15);
        assert_eq!(positive_capacity(&[]).expect("no overflow"), 0);
    }

    /// A singleton class contributes ZERO positive capacity; its four-member neighbour
    /// still yields six. That is what makes `NegativesOnly` arithmetic rather than a
    /// special case.
    #[test]
    fn positive_capacity_of_a_singleton_class_is_zero() {
        assert_eq!(positive_capacity(&[4, 1]).expect("no overflow"), 6);
        assert_eq!(positive_capacity(&[1]).expect("no overflow"), 0);
        assert_eq!(positive_capacity(&[1; 32]).expect("no overflow"), 0);
    }

    #[test]
    fn negative_capacity_matches_the_contracted_closed_form() {
        assert_eq!(negative_capacity(&[8, 4, 8]).expect("no overflow"), 128);
        assert_eq!(negative_capacity(&[8, 8, 8]).expect("no overflow"), 192);
        assert_eq!(
            negative_capacity(&[64, 64, 64]).expect("no overflow"),
            12288
        );
        assert_eq!(negative_capacity(&[4, 1]).expect("no overflow"), 4);
        assert_eq!(negative_capacity(&[6]).expect("no overflow"), 0);
        assert_eq!(negative_capacity(&[1; 32]).expect("no overflow"), 496);
    }

    /// Two derivations of the same quantity must agree. The shipped evaluation order is
    /// the running-prefix form, which overflows strictly later than `(S² − Σn²)/2`; this
    /// pins them together wherever the second one is computable at all.
    #[test]
    fn negative_capacity_agrees_with_the_sum_of_squares_derivation() {
        for layout in [
            vec![8_u64, 4, 8],
            vec![8, 8, 8],
            vec![64, 64, 64],
            vec![4, 1],
            vec![1; 32],
            vec![3, 5, 7],
            vec![0, 9, 0, 2],
        ] {
            let s: u64 = layout.iter().sum();
            let sum_squares: u64 = layout.iter().map(|n| n * n).sum();
            let via_squares = (s * s - sum_squares) / 2;
            assert_eq!(
                negative_capacity(&layout).expect("no overflow"),
                via_squares,
                "the two derivations disagree on {layout:?}"
            );
        }
    }

    #[test]
    fn default_epoch_budget_matches_the_worked_values() {
        assert_eq!(default_epoch_budget(&[8, 4, 8]).expect("no overflow"), 256);
        assert_eq!(default_epoch_budget(&[8, 8, 8]).expect("no overflow"), 384);
        assert_eq!(
            default_epoch_budget(&[64, 64, 64]).expect("no overflow"),
            24_576
        );
        assert_eq!(default_epoch_budget(&[4, 1]).expect("no overflow"), 12);
        assert_eq!(default_epoch_budget(&[1; 32]).expect("no overflow"), 992);
        assert_eq!(default_epoch_budget(&[6]).expect("no overflow"), 30);
    }

    #[test]
    fn effective_default_budget_leaves_contracted_layouts_unclamped() {
        assert_eq!(
            effective_default_budget(&[64, 64, 64], DEFAULT_HARD_CAP).expect("no overflow"),
            24_576
        );
        assert_eq!(
            effective_default_budget(&[8, 8, 8], DEFAULT_HARD_CAP).expect("no overflow"),
            384
        );
        assert_eq!(DEFAULT_HARD_CAP, 1_048_576);
    }

    #[test]
    fn effective_default_budget_clamp_engages_under_a_small_cap() {
        assert_eq!(
            effective_default_budget(&[64, 64, 64], 10_000).expect("no overflow"),
            10_000
        );
        assert!(matches!(
            effective_default_budget(&[8, 8, 8], 0),
            Err(ContrastiveDataError::ZeroHardCap)
        ));
    }

    #[test]
    fn capacity_functions_return_arithmetic_overflow_rather_than_wrapping() {
        let named = |result: Result<u64, ContrastiveDataError>, needle: &str| match result {
            Err(ContrastiveDataError::ArithmeticOverflow { operation }) => {
                assert!(
                    operation.contains(needle),
                    "operation {operation:?} does not name {needle:?}"
                );
            }
            other => panic!("expected ArithmeticOverflow naming {needle}, got {other:?}"),
        };

        named(positive_capacity(&[u64::MAX]), "positive_capacity");
        named(
            positive_capacity(&[u64::MAX - 1, u64::MAX - 1]),
            "positive_capacity",
        );
        named(negative_capacity(&[u64::MAX, 2]), "negative_capacity");
        named(
            negative_capacity(&[u64::MAX / 2, u64::MAX / 2]),
            "negative_capacity",
        );
        // pos and neg BOTH fit; only the doubling overflows, so the error must name the
        // balancing step rather than one of the capacities.
        named(
            default_epoch_budget(&[4_294_967_296, 65_537]),
            "default_epoch_budget",
        );
        named(
            effective_default_budget(&[4_294_967_296, 65_537], DEFAULT_HARD_CAP),
            "default_epoch_budget",
        );
    }

    // -- budget resolution: one named test per branch (review finding F3) ------------------

    fn cfg_with(budget: Option<u64>, hard_cap: Option<u64>) -> PairConfig {
        PairConfig {
            budget,
            hard_cap,
            ..PairConfig::new(13)
        }
    }

    #[test]
    fn budget_resolution_rejects_a_zero_hard_cap() {
        assert!(matches!(
            resolve_budget(&cfg_with(None, Some(0)), &[8, 8, 8]),
            Err(ContrastiveDataError::ZeroHardCap)
        ));
        // A configuration defect outranks a request defect: it is fixed in a different
        // place, so naming the cap first is what points at the file to edit.
        assert!(matches!(
            resolve_budget(&cfg_with(Some(0), Some(0)), &[8, 8, 8]),
            Err(ContrastiveDataError::ZeroHardCap)
        ));
    }

    #[test]
    fn budget_resolution_rejects_a_zero_explicit_budget() {
        assert!(matches!(
            resolve_budget(&cfg_with(Some(0), None), &[8, 8, 8]),
            Err(ContrastiveDataError::ZeroBudget)
        ));
    }

    #[test]
    fn budget_resolution_rejects_an_explicit_budget_above_the_hard_cap() {
        match resolve_budget(&cfg_with(Some(20_000), Some(10_000)), &[64, 64, 64]) {
            Err(ContrastiveDataError::BudgetExceedsHardCap { budget, hard_cap }) => {
                assert_eq!(budget, 20_000);
                assert_eq!(hard_cap, 10_000);
            }
            other => panic!("expected BudgetExceedsHardCap naming both numbers, got {other:?}"),
        }
    }

    #[test]
    fn budget_resolution_accepts_an_explicit_budget_at_or_below_the_hard_cap() {
        assert_eq!(
            resolve_budget(&cfg_with(Some(10_000), Some(10_000)), &[64, 64, 64])
                .expect("at the cap is accepted"),
            (10_000, false)
        );
        assert_eq!(
            resolve_budget(&cfg_with(Some(7), None), &[8, 8, 8]).expect("below the cap"),
            (7, false)
        );
    }

    #[test]
    fn budget_resolution_default_clamps_and_reports_whether_it_engaged() {
        assert_eq!(
            resolve_budget(&cfg_with(None, None), &[64, 64, 64]).expect("closed form"),
            (24_576, false)
        );
        assert_eq!(
            resolve_budget(&cfg_with(None, Some(10_000)), &[64, 64, 64]).expect("clamped"),
            (10_000, true)
        );
    }

    #[test]
    fn budget_resolution_refuses_a_layout_with_no_pair_capacity() {
        match resolve_budget(&cfg_with(None, None), &[1]) {
            Err(ContrastiveDataError::NoPairCapacity {
                positive_capacity: pos,
                negative_capacity: neg,
            }) => {
                assert_eq!((pos, neg), (0, 0));
            }
            other => panic!("expected NoPairCapacity, got {other:?}"),
        }
        assert!(matches!(
            resolve_budget(&cfg_with(Some(5), None), &[]),
            Err(ContrastiveDataError::NoPairCapacity { .. })
        ));
    }

    // -- degenerate policy: one named test per branch (review finding F2) ------------------

    #[test]
    fn degenerate_policy_no_capacity_of_either_kind_is_a_typed_error() {
        match classify_degenerate(0, 0) {
            Err(ContrastiveDataError::NoPairCapacity {
                positive_capacity: pos,
                negative_capacity: neg,
            }) => assert_eq!((pos, neg), (0, 0)),
            other => panic!("expected NoPairCapacity, got {other:?}"),
        }
    }

    #[test]
    fn degenerate_policy_all_singletons_emits_negatives_only() {
        let pos = positive_capacity(&[1; 32]).expect("no overflow");
        let neg = negative_capacity(&[1; 32]).expect("no overflow");
        assert_eq!((pos, neg), (0, 496));
        assert_eq!(
            classify_degenerate(pos, neg).expect("a legal layout"),
            EmittedKinds::NegativesOnly
        );
    }

    #[test]
    fn degenerate_policy_one_class_emits_positives_only() {
        let pos = positive_capacity(&[6]).expect("no overflow");
        let neg = negative_capacity(&[6]).expect("no overflow");
        assert_eq!((pos, neg), (15, 0));
        assert_eq!(
            classify_degenerate(pos, neg).expect("a legal layout"),
            EmittedKinds::PositivesOnly
        );
    }

    #[test]
    fn degenerate_policy_both_kinds_alternate() {
        assert_eq!(
            classify_degenerate(62, 128).expect("a legal layout"),
            EmittedKinds::Both
        );
        assert_eq!(EmittedKinds::Both.as_str(), "both");
        assert_eq!(EmittedKinds::PositivesOnly.as_str(), "positives_only");
        assert_eq!(EmittedKinds::NegativesOnly.as_str(), "negatives_only");
    }

    #[test]
    fn strategy_and_policy_version_tags_are_one_and_render_snake_case() {
        assert_eq!(PairStrategy::Oversampling.as_str(), "oversampling");
        assert_eq!(PairStrategy::Oversampling.strategy_version(), 1);
        assert_eq!(SingletonPolicy::NegativesOnly.as_str(), "negatives_only");
        assert_eq!(SingletonPolicy::NegativesOnly.policy_version(), 1);
        assert_eq!(DEGENERATE_POLICY_VERSION, 1);
    }

    /// D-11's capacity check ships even though the `unique` strategy does not, so the
    /// error variant is reachable and tested rather than dead API.
    #[test]
    fn unique_capacity_check_is_reachable_and_names_both_numbers() {
        unique_capacity_check(&[8, 4, 8], 190).expect("190 <= 62 + 128");
        match unique_capacity_check(&[8, 4, 8], 191) {
            Err(ContrastiveDataError::BudgetExceedsCapacity { budget, capacity }) => {
                assert_eq!(budget, 191);
                assert_eq!(capacity, 190);
            }
            other => panic!("expected BudgetExceedsCapacity, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod pair_proptests {
    //! The RUNNABLE evidence behind the two DECLARED-not-executed Kani harnesses
    //! (`KANI-CPP-001`, `KANI-CPP-002`). `cargo-kani` is not installed in this repository
    //! and there is no `#[kani::proof]` harness anywhere under `crates/`, so these
    //! proptests — bounded identically at 4 — are the evidence, not a placeholder for it.

    use super::{negative_capacity, positive_capacity, CanonicalPair};
    use crate::error::ContrastiveDataError;
    use crate::select::{test_corpus, SelectedId};
    use proptest::collection::vec as prop_vec;
    use proptest::prelude::{prop_assert, prop_assert_eq, proptest};

    fn ordinals() -> Vec<SelectedId> {
        let (selection, _) = test_corpus::fresh_selection(12, 17, 8);
        selection
            .examples()
            .iter()
            .map(|row| {
                selection
                    .selected_id(&row.id)
                    .expect("every selected example resolves to its own ordinal")
            })
            .collect()
    }

    /// Naive reference enumeration — deliberately quadratic, deliberately only used inside
    /// the bound-4 proptest, and therefore the independent second derivation the closed
    /// forms are checked against.
    fn naive_capacities(sizes: &[u64]) -> (u64, u64) {
        let mut pos = 0;
        let mut neg = 0;
        for (j, &nj) in sizes.iter().enumerate() {
            pos += nj * nj.saturating_sub(1) / 2;
            for &nk in &sizes[j + 1..] {
                neg += nj * nk;
            }
        }
        (pos, neg)
    }

    proptest! {
        /// KANI-CPP-001's backing proptest, bound 4.
        #[test]
        fn canonical_pair_ordering(a in 0_usize..4, b in 0_usize..4) {
            let ids = ordinals();
            let (x, y) = (ids[a], ids[b]);
            match CanonicalPair::new(x, y) {
                Ok(pair) => {
                    prop_assert!(a != b);
                    prop_assert!(pair.lo() < pair.hi());
                    prop_assert_eq!(pair, CanonicalPair::new(y, x).expect("the mirror pairs"));
                }
                Err(ContrastiveDataError::SelfPair { id }) => {
                    prop_assert_eq!(a, b);
                    prop_assert_eq!(id, u64::from(x.ordinal()));
                }
                Err(other) => prop_assert!(false, "unexpected error {:?}", other),
            }
        }

        /// KANI-CPP-002's backing proptest, bound 4.
        #[test]
        fn capacity_no_overflow(sizes in prop_vec(0_u64..4, 0..=4_usize)) {
            let (want_pos, want_neg) = naive_capacities(&sizes);
            prop_assert_eq!(positive_capacity(&sizes).expect("bounded sizes never overflow"), want_pos);
            prop_assert_eq!(negative_capacity(&sizes).expect("bounded sizes never overflow"), want_neg);
        }
    }

    /// The adversarial half of KANI-CPP-002: near-`u64::MAX` vectors are a typed error,
    /// never a wrap. Outside `proptest!` because the inputs are enumerated, not sampled.
    #[test]
    fn capacity_no_overflow_adversarial_vectors_are_typed_errors() {
        for sizes in [
            vec![u64::MAX],
            vec![u64::MAX, u64::MAX],
            vec![u64::MAX - 1, 3],
            vec![u64::MAX / 2, u64::MAX / 2],
        ] {
            let pos = positive_capacity(&sizes);
            let neg = negative_capacity(&sizes);
            assert!(
                matches!(pos, Err(ContrastiveDataError::ArithmeticOverflow { .. }))
                    || matches!(neg, Err(ContrastiveDataError::ArithmeticOverflow { .. })),
                "{sizes:?} wrapped instead of erroring: pos={pos:?} neg={neg:?}"
            );
        }
    }
}
