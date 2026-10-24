# `aprender-embed-reference-vectors.json` — schema as the graph-rag probe reads it

Source of truth: `parse_reference` and `reference_vectors` in
`built-in/graph-rag/crates/snapshot-build/tests/aprender_conformance.rs` (copy in this folder).

## Shape

```json
{
  "model": "all-MiniLM-L6-v2",
  "hf_revision": "<commit of sentence-transformers/all-MiniLM-L6-v2>",
  "tokenizer_sha256": "<64 hex>",
  "max_seq_length": 256,
  "texts": [
    { "id": "T-UPPER-A", "text": "the the ... <300-word tail A>", "vector": [ 384 floats ] },
    { "id": "T-UPPER-B", "text": "...", "vector": [ 384 floats ] },
    { "id": "T-LOWER-A", "text": "...", "vector": [ 384 floats ] },
    { "id": "T-LOWER-B", "text": "...", "vector": [ 384 floats ] },
    { "id": "T-253",     "text": "...", "vector": [ 384 floats ] },
    { "id": "T-254",     "text": "...", "vector": [ 384 floats ] },
    { "id": "T-255",     "text": "...", "vector": [ 384 floats ] }
  ]
}
```

## What the loader requires

| Field | Type | Required | Checked how |
|---|---|---|---|
| `texts` | array | yes | missing → the whole check fails naming the file |
| `texts[].id` | string | yes | one entry per boundary id above; a missing id fails naming it |
| `texts[].vector` | array of numbers | yes | must hold exactly 384 numbers; any non-numeric value fails |
| `texts[].text` | string | no | when present, compared whitespace-normalised to the brief's definition of that id; a mismatch fails ("computed over a different text than the brief defines") |
| `model`, `hf_revision`, `tokenizer_sha256`, `max_seq_length` | — | no | not read by the probe; keep them for the release-identity record |

Unknown fields are ignored. Entry order does not matter; lookup is by `id`.

## The seven ids and their texts (normative, from the brief)

- Word list A (50 words): river mountain forest valley stone cloud winter summer garden bridge ocean island desert meadow harbor lantern orchard canyon glacier prairie thunder autumn spring breeze shadow harvest village castle tower window mirror candle anchor compass feather violin ribbon basket blanket pillow kettle hammer ladder pencil wagon saddle fabric marble copper silver
- Word list B (50 words): budget profit revenue server network thread buffer socket packet router backup cluster signal sensor battery circuit voltage current engine pump valve wheel rocket orbit radio satellite salary audit tax loan share auction contract deposit premium currency ledger invoice database kernel program delay capacity software upgrade code protocol pipeline reactor market
- Texts are space-joined words. `the` × n means the word repeated n times.
- `T-UPPER-A` = `the` × 260 + list A × 6. `T-UPPER-B` = `the` × 260 + list B × 6.
- `T-LOWER-A` = `the` × 200 + list A × 1. `T-LOWER-B` = `the` × 200 + list B × 1.
- `T-253` = `the` × 253. `T-254` = `the` × 254. `T-255` = `the` × 255.

## Pass condition

For each id: cosine(server vector, reference vector) ≥ 0.9999. Reference vectors are computed with
sentence-transformers at `max_seq_length = 256`.

## Sample

`aprender-embed-reference-vectors.sample.json` in this folder shows the shape with 4-float vectors.
It is a shape sample only; the probe would reject it ("has 4 floats, expected 384").
