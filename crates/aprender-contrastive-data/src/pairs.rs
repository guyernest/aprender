//! Bounded pair sampling: canonical pairs, capacity math, budget resolution, and the
//! singleton and degenerate-layout policies.
//!
//! Pairs are SAMPLED, never enumerated. Capacity is a closed form in the number of
//! classes, so faithful counts cost nothing while retained state stays
//! `O(examples + classes)`; only enumeration is quadratic. Endpoints are canonicalized to
//! `(min, max)` by the sole constructor, so both orientations and self-pairs are
//! structurally impossible rather than merely tested against.
//!
//! Implemented by plan 02-07.
