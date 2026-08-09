//! Labeled-example schema and strict JSONL parse/encode over `&[u8]`.
//!
//! Deserialization is `deny_unknown_fields` at the bytes -> typed boundary: bytes that
//! arrive from object storage are untrusted, and a silently ignored extra field is how a
//! schema drift becomes a silent data change.
//!
//! Implemented by plan 02-03.
