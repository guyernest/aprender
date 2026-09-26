# aprender-decide

**Method-neutral decision models** over `aprender-core`: answer one fixed task —
"which of these K criteria does this text belong to?" — with a calibrated
probability per criterion, in the task's criteria order.

Part of the [aprender](https://github.com/paiml/aprender) monorepo.

## The seam

`DecisionMethod` is designed for exactly one implementation today, with Kev (a
few-shot decoder classifier) as the known second:

| Method | Returns |
|--------|---------|
| `task()` | the parsed `task.json` (ordered labels) |
| `prepare(texts)` | one built row per text (tokenized once; a server prices its token budget from these) |
| `classify_prepared(rows)` | one `Decision { label_index, probabilities, tokens, truncated }` per row |
| `classify(texts)` | `prepare` then `classify_prepared` |

Probabilities are always an **array in criteria order**, never a map.

## Laya (the only method)

`aprender_decide::laya::Laya` is Laya's decision model on top of `aprender-core`'s
reusable ModernBERT encoder (`aprender::models::modernbert`): a type embedding, a
pre-norm `TransformerEncoderLayer` head with `nhead = max(1, d / 64)`, a per-marker
scorer MLP, Laya's request builder (`build_sequence`, including its option shrink
and right truncation) and its calibrated temperature buckets clamped to
`[0.5, 5.0]`. Head and scorer reuse core's `Linear`, `layer_norm`, `gelu_exact`
and `attention` — no numeric kernel is copied.

A task is refused once, at load, only when its built row would lose an option
marker (`LayaError::MarkersLost`); options that are merely long are shrunk exactly
as Laya does and served.

## task.json

`{"type": "choice", "instructions": "...", "criteria": {"name": "description" | null, ...}}`.
The **document order of `criteria` is the label index**. It is read from the raw
bytes by an order-preserving map visitor, so it is the same whether or not the
build unifies `serde_json/preserve_order` in (tested in both shapes). This crate
does not enable that feature.

## This crate is not

- a server — the thin decide MCP servers are the transport and call this crate;
- an encoder — that is `aprender-core` (D-13);
- a binary — library only.

## Contracts

- `contracts/laya-parity-v1.yaml` — torch -> `.apr` -> Rust parity ladder
  (ids and markers exact, logits <= 1e-4, probabilities <= 1e-5, argmax exact)
- `contracts/decide-apr-v1.yaml` — the task schema and the marker rule

## Tests

```bash
cargo test -p aprender-decide --lib                              # all lib tests
cargo test -p aprender-decide --lib laya::tests::tiny_parity     # parity vs Laya's oracle
cargo test -p aprender-decide -p aprender-mcp-setfit --lib task:: -- --nocapture  # preserve_order=ON shape
```
