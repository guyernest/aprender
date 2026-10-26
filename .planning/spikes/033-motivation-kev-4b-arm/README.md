---
spike: 033
idea: llm-decision-classifier
name: motivation-kev-4b-arm
type: standard
validates: "Given the Phase 12 motivation-v1 split and the partner's guide (Sections 4.1-4.4, 5, 6; 4.5 omitted), when Kev-4B runs through the Rust path (PEFT merge in fp32 -> llama.cpp f32 GGUF -> upstream Qwen35Model with batched prefill and hidden readout + the pointer head) it matches a torch fp32 oracle, and one real train row is timed so the owner can price the reference arm before any multi-hour compute"
verdict: PENDING
related: [015, 017, 020]
tags: [kev, qwen3.5, gguf, prefill, motivation, reference-arm]
---

# Spike 033: Kev-4B reference arm for motivation-v1 (Phase 12, plan 12-14)

This is the Phase 12 Kev reference arm (D-05, D-06, D-07, D-24). Laya is the served model regardless; Kev is measured on the
SAME stratified split so the method choice rests on a number. The evidence is
`.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-KEV-ARM-EVIDENCE.json`.

## The branch is used in place, not landed
`Cargo.toml` path-depends on `../aprender-016-upstream-sync` (branch `spike/016-upstream-sync`, the unpushed spike-016 merge
with the MTP depth fix and spike 020's `Qwen35Model::prefill_hidden`), exactly as spike 020 does. The branch is neither
modified nor landed; the evidence records its name, the worktree HEAD SHA and its dirty files (`Cargo.lock` only, pre-existing).
`Cargo.lock` is spike 020's, with the workspace member's entry regenerated OFFLINE for the package rename
(`cargo update --workspace --offline`); every other `[[package]]` entry is unchanged. Build:
`cargo build --release --offline --locked --manifest-path .planning/spikes/033-motivation-kev-4b-arm/Cargo.toml`.

## Where the outputs go (all gitignored)
- `/models/kev/kev-4b/` : `kev-4b-merged-f32.gguf` (16.8 GB), `head.safetensors`, `parity_fixture.json`, `export_status.json`
  (the merged fp32 HF directory is removed after the GGUF sha256 is recorded when free disk is below 20 GiB)
- `/models/kev/motivation-v1/` : `rows-train.json`, `rows-eval.json`, `probe-row.json` (token ids only, no post text)
- `/data/kev/motivation-v1/guide-s4-6-no-4-5.txt` : the partner's guide text (never committed, never printed)

## Pieces
- `tools/export_kev4b.py` : spike 017's exporter with one structural change. It writes `m.lm.state_dict()` straight to
  safetensors shards (`model.` prefix, index json, CausalLM config, kev tokenizer) instead of instantiating a second fp32
  `AutoModelForCausalLM` (peak RSS 21.2 GB at 4B for the whole export). The base checkpoint is multimodal, so "key set equals the
  base's" is checked on the text image `model.language_model.X -> model.X` (426 keys; the 297 visual and 15 mtp keys are not Kev's
  backbone). The parity fixture holds six public stance rows, a 3-question ticket and one seeded synthetic row of 6,808 tokens.
- `src/main.rs` : `parity <gguf> <head> <fixture> [--no-control]` (spike 020's behaviour; short and long rows reported separately and
  as a `PARITY_JSON` line) and `dump <gguf> <head> <rows.json> <out.jsonl> --identity FILE [--repeat N]`. The first line of
  `out.jsonl` is the identity header (keys sorted, compact); a resume whose header differs prints `REFUSED dump-identity` and
  exits 2; a torn final line (no trailing newline, or invalid JSON) is truncated with `DROPPED torn final line (<n> bytes)`; rows
  already present are skipped. It prints no text.
- `tools/extract_guide.py` : Sections 4.1-4.4, 5, 6 from the pinned docx without 4.5, with `--drop-paragraphs-matching` for D-11.
  The Section 4 lead-in (the text between heading 4 and 4.1) is KEPT: that reading reproduces the research's measured 4,490 words /
  6,811 Qwen tokens; without it the text is 6,469 tokens.
- `tools/encode_rows.py` : encodes train.jsonl and eval.jsonl as written (OPS-03) with `kev.model.encode(strict=True)` and records
  the leakage counts (guide text match and train/eval sha256 intersection).

## Regime disclosure
The instruction branch is about 6.8k tokens, about 6.6x Kev's 1,024-token training branch: outside Kev's training distribution
(the author marks it untested). Kev is a reference arm; a good or bad number here is a finding, not a verdict on the method.

## Verdict
PENDING: the owner chooses the Kev budget at the plan-14 checkpoint; the arm's own run is plan 12-15.
