//! Balanced few-shot selection and the ordered selected-ID manifest model.
//!
//! Partial Fisher-Yates over sorted per-class buckets, with the swap index for step *i*
//! drawn at ordinal *i* in that class's domain. The output order IS the draw order, which
//! is what makes the ordered manifest statable as a contract equation rather than as an
//! implementation detail.
//!
//! Implemented by plan 02-05.
