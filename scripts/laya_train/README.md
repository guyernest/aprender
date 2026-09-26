# scripts/laya_train — the Laya back office (phase 8, D-01 / D-02)

A pinned, hash-locked `uv` project that runs **Laya's own code** for everything the Rust side
(`aprender-core` ModernBERT, `aprender-decide`) is measured against. Rust owns inference; this
project owns training and the reference numbers. There is no `apr` subcommand for any of it
(D-02: `contracts/apr-cli-commands-v1.yaml` is untouched) — it is driven through `just`.

## Recipes

| Recipe | What it does |
|--------|--------------|
| `just laya-fixtures` | `metrics.py --selftest`, then `fixtures.py`: regenerates the two tiny synthetic CI fixtures. Prints each file's sha256 and size and ends with `FIXTURES OK`. A re-run is byte-identical. |

Plan 08-08 adds the training recipes (`laya-train`, `laya-train-selftest`) to the same project.

Both recipes use `uv run --project scripts/laya_train --frozen`, so the committed `uv.lock` is what
runs — never a fresh resolution.

## Pins

Every package was verified by a human (plan 08-02 Task 1, package-legitimacy checkpoint, approved
2026-09-25) **before** the first `uv lock`. Exact `==` pins, one why-comment each in
`pyproject.toml`; transitive dependencies are hash-pinned in `uv.lock`.

| Package | Pin |
|---------|-----|
| Python | 3.13.7 (`.python-version`; `requires-python = ">=3.13,<3.14"`) |
| torch | 2.14.0 |
| transformers | 5.17.0 |
| tokenizers | 0.23.1 (matches the Rust workspace pin) |
| safetensors | 0.8.0 |
| huggingface-hub | 1.32.0 |
| numpy | 2.5.3 |
| pyyaml | 6.0.3 |
| laya | `git+https://github.com/NandhaKishorM/laya@4066d5d5fbf08b66c6757ddeedbd797bd7655bc0` (not on PyPI) |
| base weights (plan 08-08) | `convaiinnovations/laya@55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851` |

The lock is resolved for **macOS arm64 only** (`[tool.uv] environments`): this is the laptop back
office, and a Linux/x86 split would be a resolution nobody verified.

To change a pin: edit `pyproject.toml`, get the new package verified the same way, `uv lock`, and
re-run `just laya-fixtures` — every committed fixture is a numerical artifact of these versions.

## Files

| File | Purpose |
|------|---------|
| `metrics.py` | numpy-only (no torch import): `macro_f1`, `f_avg`, `ece_top_label` (the HOUSE top-label ECE — floor binning, `aprender::calibration::expected_calibration_error_top_label`), `nll`, and `--selftest` (hand cases plus a replay of every frozen case in `scripts/setfit_fixtures/claims_stats/ece_top_label_cases.json` to 1e-6). Shared by `fixtures.py` and the 08-08 gate. |
| `fixtures.py` | Writes `crates/aprender-core/tests/fixtures/modernbert_tiny/` and `crates/aprender-decide/tests/fixtures/laya_tiny/` (see below). |

## The two fixtures

Both are random-init, synthetic, deterministic (seed 20260925, deterministic torch algorithms, one
thread) and at most 1 MiB per directory. The `model.safetensors` files are committed through two
exact-path negations of the global `*.safetensors` rule in the root `.gitignore`; real checkpoints
stay under the root-anchored `/models/` and are never committed.

- **`modernbert_tiny/`** — a plain transformers ModernBERT (HF names, no prefix): vocab 512,
  hidden 32, 3 layers (full, sliding, sliding), 2 heads, `local_attention` 8, RoPE theta 160000 /
  10000 in the transformers 5.17 `rope_parameters` layout, every LayerNorm weight drawn in
  [0.5, 1.5]. Saved F16, reloaded to fp32, and the ladder recorded for two id rows (24 and 21
  tokens). `oracle.json` stores each block as a flat list of the exact f32 values with its shape,
  plus a `window_mutation` record: moving the half-window by one changes the first local layer by
  6-10 % rms while the first global layer is bit-identical — the rows exercise the window.
  `initializer_range` is 0.2 (10x ModernBERT's default) so attention is not near-uniform; the
  generator refuses if the window check stops discriminating.
- **`laya_tiny/`** — a tiny Laya checkpoint in the `run_dir_layout` of
  `contracts/laya-finetune-gate-v1.yaml`, plus its `data/` dir (D-05 `task.json`, synthetic
  `train.jsonl` / `eval.jsonl`). Built with Laya's `DecisionModel` constructor (hidden 32, so the
  head gets `nhead = max(1, 32 // 64) = 1`), saved F16 (`temperature` F32), then **reloaded through
  `laya.Agent(dir, device="cpu")`** and scored through `Agent.predict`. The oracle records, per row,
  the builder inputs, ids, markers, qtype, bucket, applied temperature, the ladder, marker states,
  logits and probabilities; plus the `many` question whose markers Laya drops past `max_len` (and
  which Laya refuses). `recipe.json` is `variant: "synthetic-fixture"`, so every pack-for-serving,
  verify and deploy path refuses this run (`synthetic_not_deployable`).

Encoding (for the Rust readers):

- `laya_tiny/oracle.json` ladder blocks and `m_opts` are **`f32le_base64`**: standard padded base64
  of the little-endian f32 bytes, row-major `[n_tokens, d]` / `[k, d]`. Exact values; hex would not
  fit the 1 MiB bound. Logits and probabilities are decimal lists plus `*_f32_hex` lists.
- `*_f32_hex` / `probabilities_f32_hex`: one value per string, 8 hex digits, the **big-endian f32 bit
  pattern** (the `decide-apr-v1` probe convention).
- Probabilities are exactly the unrounded `p` of `Agent._decode_answers` (captured at its
  `answer_confidence(p, k)` call); `predict`'s 4-decimal public answer is asserted equal to
  `round(p, 4)`.
- `gate-report.json` `calibration.slice_ids_sha256` = sha256 of the compact JSON array bytes
  `json.dumps(slice_ids, separators=(",", ":"))`, e.g. `[0,2,6,7,8,11]`.
- `recipe_id` = sha256 of the exact `recipe.json` bytes; `inputs_sha256.*` are file-byte hashes.
- `gate-report.json` is an honest record that no gate ran: the declared base IS the tiny model, so
  `zero-shot-probs.json` equals `eval-probs.json`, `margin` is 0.0 and `pass` is `false`.

The generator refuses (non-zero) if a probe row exceeds `decide-apr-v1` `probe_max_row_tokens`, if
Laya's loader rewrote any checkpoint file (sha256 before and after the reload), if the oracle does
not cover truncation / option shrink / all three qtypes, or if a fixture directory exceeds 1 MiB.

## What CI does and does not run

CI does **not** install this project's torch stack. The checks CI must enforce are re-derived in
Rust instead (plan 08-09, `aprender_decide::verify`): the gate metrics recomputed from the
probability files, split disjointness from `slice_ids` and the data dir, and the synthetic-variant
refusal. Whether the torch-free self-tests (`metrics.py --selftest`, and later `data.py` / `gate.py`)
also run in CI is decided at plan 08-12's CI checkpoint — adding a Python toolchain to the CI image is
a CI-workflow change, which needs a human check-in.
