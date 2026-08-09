//! Domain-separated Philox key derivation and the bounded-draw primitive.
//!
//! The byte encoding is frozen by the contract: the key is a little-endian 64-bit
//! truncation of a SHA-256 over a domain tag, the root seed, and a domain string drawn
//! from a closed table; the counter carries the draw ordinal. Bounded draws use 64-bit
//! multiply-shift. Modulo draws and `next_f32` are forbidden — the first is biased and
//! unauditable at the edges, the second has only 23 mantissa bits.
//!
//! Implemented by plan 02-04.
