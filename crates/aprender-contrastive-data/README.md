# aprender-contrastive-data

Deterministic, leakage-safe contrastive data construction: class buckets,
balanced few-shot selection, bounded pair sampling.

SetFit is its first consumer, not its owner — the crate owns contrastive and
Siamese *data construction* as a general capability: class buckets, balanced
few-shot selection, bounded positive/negative pair sampling, typed split roles,
dataset fingerprints, and the cross-split leakage checks that make the rest
trustworthy. Contract: `contracts/contrastive-pair-protocol-v1.yaml`.

## The bytes boundary

The public API is bytes-in / bytes-out and typed values. This crate performs no
filesystem access, opens no sockets, and exposes no path-shaped parameters —
not even in its tests. `apr-cli` owns every filesystem adapter.

That is enforced, not asserted. `make contrastive-data-boundary` compares the
resolved `cargo tree -e normal` closure against `allowed-deps.txt`, a POSITIVE
allowlist so that a new transitive dependency fails by default rather than
passing unnoticed, and bans `std::fs` / `std::net` / `std::path` / `Path` /
`PathBuf` throughout `src/`.

The reason is the destination: object storage behind a serverless consumer,
where a manifest is an object rather than a file. A crate whose API speaks in
`&Path` forces such a consumer to be a rewrite instead of a wrapper.

## Determinism

Every random decision is a pure function of its draw ordinal, obtained from the
counter-based Philox generator in `aprender-rand` (library name `trueno_rand`)
rather than from a stateful stream. Worker-count independence is therefore
structural: draw *i* cannot depend on how many draws preceded it, because
nothing precedes it.

## Entry points

The CLI adapters are `apr data select` (balanced few-shot selection) and
`apr data pairs` (bounded pair sampling); `aprender-train`'s `setfit` feature
consumes the same modules directly.

Part of the [aprender](https://github.com/paiml/aprender) monorepo.
