# scripts/laya_train — the Laya back office (phase 8, D-01 / D-02)

A pinned, hash-locked `uv` project that runs **Laya's own code** for everything the Rust side
(`aprender-core` ModernBERT, `aprender-decide`) is measured against. Rust owns inference; this
project owns training and the reference numbers. There is no `apr` subcommand for any of it
(D-02: `contracts/apr-cli-commands-v1.yaml` is untouched) — it is driven through `just`.

## Recipes

| Recipe | What it does |
|--------|--------------|
| `just laya-fixtures` | `metrics.py --selftest`, then `fixtures.py`: regenerates the two tiny synthetic CI fixtures. Prints each file's sha256 and size and ends with `FIXTURES OK`. A re-run is byte-identical. |
| `just laya-prepare-stance` | The D-19 demo data: `data/decide/tweet-stance-16/{task.json,train.jsonl,eval.jsonl}` from the `s16-seed13` selection (48 shots, every one verified against the manifest's `exact_hash`) and all 280 TweetEval stance_abortion test rows. Needs `apr data tweet-eval-stance --output data/tweet-eval-stance` first. Output is under the root-anchored, gitignored `/data/`. |
| `just laya-train <data> <out> [args]` | Fine-tune, calibrate and gate (below). Exit **0** = GATE PASS, **3** = GATE FAIL, **2** = input refused. Args: `--epochs E` (only above 16 shots/class, in [4, 12]), `--stopping early_stopping\|fixed_epochs` (default: the contract's `early_stopping`), `--seeds N`, `--device mps\|cuda\|cpu`. |
| `just laya-train-lifecycle` | A real train -> F16 save -> complete dir -> reload -> calibrate -> gate on the committed tiny checkpoint, on CPU in seconds: both stopping rules, the `--seeds` refusals and a `--seeds 3` run. Prints `LIFECYCLE OK`. |
| `just laya-train-selftest` | `metrics.py`, `data.py` and `gate.py --selftest` (numpy + pyyaml only), then the lifecycle. Prints `METRICS SELFTEST OK`, `DATA SELFTEST OK`, `GATE SELFTEST OK`, `LIFECYCLE OK`, `LAYA TRAIN SELFTEST OK`. The `test_harness` of FALSIFY-LAYA-GATE-003..007 and 009. |

All recipes use `uv run --project scripts/laya_train --frozen`, so the committed `uv.lock` is what
runs — never a fresh resolution. Every threshold, recipe value, seed, stopping rule and the base pin
is read at run time from `contracts/laya-finetune-gate-v1.yaml` through `contract.py`; no script holds
one as a literal (D-04, D-07).

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
| `common.py` | Torch-free at import: the shared hashing (`sha256_bytes`, streaming `sha256_file`, `tree_sha256`), JSON / f32 serialization (`write_json`, `f32_list`, `f32_hex_list`) and the ONE F16 checkpoint policy `save_f16` (`temperature` kept F32) used by `fixtures.py`, `train.py` and `lifecycle.py`. |
| `fixtures.py` | Writes `crates/aprender-core/tests/fixtures/modernbert_tiny/` and `crates/aprender-decide/tests/fixtures/laya_tiny/` (see below). |
| `contract.py` | The ONE reader of `laya-finetune-gate-v1` (and the probe task of `decide-apr-v1`): thresholds, recipe, base, seed policy, epoch rule (`resolve_epochs`), stopping rule (`resolve_stopping`), seeds (`resolve_seeds`), the `recipe.json` object. No torch. |
| `data.py` | `task.json` / `*.jsonl` validation, normalized-text (`nfc-trim-ws-v1`) overlap refusal, conflicting-duplicate refusal, and the seeded, stratified, text-GROUP-disjoint calibration split returning sorted `slice_ids` + their sha256. `--selftest`. No torch. |
| `prepare_stance.py` | Writes the D-19 demo data dir (never commits tweet text; logs counts only). |
| `train.py` | The CLI behind `just laya-train` (steps below). |
| `gate.py` | The bounded NLL temperature fit, the early-stopping rule (`EarlyStopper`), `evaluate_gate` (thresholds from the contract, non-finite never passes) and `verify_report` (re-decides a report under the contract's thresholds; refuses mismatched thresholds and a `pass` its metrics contradict). `--selftest`. No torch. |
| `lifecycle.py` | `just laya-train-lifecycle`: runs `train.py` as a subprocess on the tiny fixture and asserts every ordering, hash, schema, seed and re-score rule listed in its docstring. |

## Training (`just laya-train`, plan 08-08)

In order, every value from the contract:

1. **Validate** the data dir. `eval.jsonl` is required.
2. **Write `recipe.json` first** (`sort_keys`, compact) and print `RECIPE WRITTEN <recipe_id>`, where
   `recipe_id` = sha256 of those bytes — before any model scores anything (D-04).
3. **Device**: request mps -> cuda -> cpu, load Laya's own `Agent` on the pinned base with
   `expected_sha256={"model.safetensors": <sha>}`, and record the device READ BACK from the parameters
   (Laya falls back to CPU silently; CPU is flagged). D-03.
4. **Train** with the spike-024 loop on `fit` rows built by `Agent._encode_state`. Under
   `early_stopping` (the default) the calibration slice's NLL at the bounded fitted T is the monitor
   after every epoch, the best epoch is restored and `STOP` is logged. Eval rows never reach training.
5. **Write the COMPLETE checkpoint dir** (F16 weights with an F32 `temperature`, `rl_agent_config.json`,
   `encoder/`, `tokenizer/` with `tokenizer_config.json` already in Laya's fixed form) and print
   `CHECKPOINT COMPLETE`, before anything reloads it.
6. **Reload** in fp32 on CPU (checkpoint sha256s asserted unchanged), print `SCORING START`, fit T by
   NLL in [0.5, 5.0] on the calibration slice (`clamp_hit` recorded), write T, reload again.
7. **Score** eval (`eval-probs.json`), the declared base on eval (`zero-shot-probs.json`) and the
   decide-apr-v1 probes (`probes.json`) — all on F16 reloads, text hashes only, never text.
8. **Gate**: pass iff `ft.macro_f1 - zs.macro_f1 >= 0.05` AND `ece_post <= 0.10` (contract
   constants) -> `gate-report.json`, `GATE PASS` (0) / `GATE FAIL` (3).

**Run dir** (`run_dir_layout`): `checkpoint/{model.safetensors, rl_agent_config.json, encoder/,
tokenizer/}`, `task.json`, `recipe.json`, `gate-report.json`, `eval-probs.json`,
`zero-shot-probs.json`, `probes.json`, and `variance-report.json` when `--seeds N > 1`. A run dir is
written once: a non-empty `--out` is refused.

**Seeds (D-08).** One declared seed (13) by default; the report says `single seed`. `--seeds N`
(1 <= N <= 3) trains the first N of `variance_seeds` (13, 17, 23), the declared seed first and exactly
as a single-seed run; each other seed runs in `<out>/.variance-seed-<s>/`, deleted once its metrics are
recorded. The seed varies the training RNG, not the data or the calibration slice.
`variance-report.json` (`laya-variance-report-v1`) holds per-seed rows and mean / sample sd (ddof 1) of
`macro_f1`, `f_avg`, `ece_post` and `margin` — information only. The gate is judged on seed 13, whose
checkpoint is the only one kept; the label becomes `mean ± sd over N seeds`; `recipe.json` (and the
recipe_id) does not change with N.

**Refusals** (exit 2, message `REFUSED <rule>: ...`):

| Rule | Refused input |
|------|---------------|
| `task-type` | `type` other than `"choice"` |
| `task-too-few-criteria` / `task-duplicate-criterion` | fewer than 2 criteria / a repeated criterion name |
| `task-unknown-key` / `task-schema` / `task-missing` | a key outside `{type, instructions, criteria}`, a malformed or absent task.json |
| `train-row-label` / `eval-row-label` | a label that is not a criterion NAME (an index is refused too) |
| `train-row-unknown-key` / `*-row-schema` | a row key outside `{text, label}`, a blank or non-object line |
| `eval-missing` | no `eval.jsonl` (D-06) |
| `eval-train-overlap` | an eval text equal to a train text after NFC / trim / whitespace collapse |
| `train-conflicting-labels` | two train rows with the same normalized text and different labels |
| `train-class-too-small` | a class too small to give a calibration slice of `calibration_slice_min_per_class` and keep a fit row |
| `epochs` | `--epochs` at <= 16 shots/class (fixed to 12), or missing / outside [4, 12] above 16 |
| `stopping` / `seeds` | an unknown stopping rule / `--seeds` outside [1, 3] |
| `base` | `--base` without `--variant synthetic-fixture` (production always uses the contract base) |
| `out-dir` | a non-empty `--out` |

**The D-19 demo's recorded outcome is GATE FAIL** (contract 1.2.0 `demo.outcome`). Both declared
recipes pass the margin and fail `ece_post`: `fixed_epochs` (recipe_id `d0f4e40d…`, ECE 0.377, T
clamped at 5.0) and `early_stopping` (`3d4b91da…`, ECE 0.222, T 3.14). Their gitignored run dirs
(`models/decide/tweet-stance-16-fixed-epochs/`, `models/decide/tweet-stance-16/`) are FAIL-CLOSED
TEST VECTORS: `gate.py --selftest` decides FAIL on both and, when the dirs are present, recomputes it
from their probability files; pack/verify (plan 08-09) must refuse both. No stance model is deployed
until a declared run passes (see `.planning/todos/pending/spike-laya-calibration-slice-and-temperature-cap.md`).

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
refusal. Whether the torch-free self-tests (`metrics.py`, `data.py` and `gate.py --selftest`, which
need only numpy + pyyaml) also run in CI is decided at plan 08-12's CI checkpoint — adding a Python
toolchain to the CI image is a CI-workflow change, which needs a human check-in. The torch lifecycle
runs locally only, through `just laya-train-selftest`. `gate.py --selftest`'s run-dir recompute needs
the gitignored demo run dirs and prints an explicit `SKIP` where they are absent (e.g. in CI); the
literal-number fail-closed cases run everywhere.
