# Phase 12: motivation-v1 decide server (pilot) - Pattern Map

**Mapped:** 2026-10-26
**Files analyzed:** 22 (new + modified)
**Analogs found:** 21 / 22 (Kev arm driver: partial, spike-crate analog only)

All analog paths below are git-tracked (verified with `git ls-files`). The uncommitted justfile `region`
diff is NOT part of any pattern here (D-23): plans must not depend on or commit it.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match |
|---|---|---|---|---|
| `scripts/laya_train/prepare_motivation.py` (new) | utility (data prep CLI) | batch / file-I/O | `scripts/laya_train/prepare_goldstandard.py` | exact |
| `justfile` `laya-prepare-motivation` (new recipe) | config / wrapper | request-response | `justfile:1318-1328` `laya-prepare-goldstandard` | exact |
| `justfile` `laya-deploy` edge `/health` 404 retry (mod) | config / deploy | request-response w/ retry | `justfile:2077-2094` (503 probe retry) applied at `:2068` | exact |
| `justfile` `laya-deploy-selftest` EDGE HEALTH rows (mod) | test | offline shadowed curl | `justfile:2151` `laya-deploy-selftest` | exact |
| `justfile` selftest rows for prepare_motivation `--measure` (mod) | test | — | `justfile:3434-3437` `must_fail`/`must_pass` rows | exact |
| `scripts/laya_train/metrics.py` (mod: P/R, false-assignment, recall@t, ECE bootstrap CI) | utility | transform | `metrics.py:54-105` (`_f1`, `macro_f1`, `ece_top_label`) | exact |
| `scripts/laya_train/bakeoff.py` (new) | utility (report) | batch / transform | `metrics.py` + `eval_probs_schema` (`laya-finetune-gate-v1.yaml:464-469`) | role-match |
| `scripts/laya_train/train.py`, `contract.py` (mod: OPTIONAL `pilot_waiver` in gate report) | service | file-I/O | `contract.py:187-192` `_OPTIONAL_KEY_RULES`; existing optional `shift_probe` / `rescore_noise_sha256` | exact |
| `scripts/laya_train/probe_epoch.py` (mod/param) | utility | batch | itself `probe_epoch.py:61-65` (GUARD_DEFAULT / EPOCHS_RULE) | exact |
| `crates/aprender-decide/src/verify.rs` (mod: waiver decision) | service (gate) | request-response | `verify.rs:3266-3287` (`check_gate` → `GateFailed`), `verify.rs:378` `VerifyPolicy::from_contract_views`, `verify.rs:205-211` `GateContractView` | exact |
| `crates/aprender-decide/src/pack.rs` (mod: `GateReport.pilot_waiver`) | model | transform | `pack.rs:351-357` `#[serde(default)] Option<..>` fields | exact |
| `crates/aprender-decide/src/artifact.rs` + `artifact/ladder.rs` (mod: `GateSummary.waiver`) | model | transform | `artifact.rs:206-215, 1213-1218` `GateSummary`; ladder leaf table near `ladder.rs:1123` | exact |
| `crates/aprender-decide/src/verify/tests.rs`, `tests/fail_closed_vectors.rs` (mod) | test | — | existing verify tests + FALSIFY-LAYA-GATE-010 vectors | exact |
| `contracts/laya-finetune-gate-v1.yaml` (mod: `pilot_waivers`, schema row, `run_field_bindings`, falsification_tests) | config / contract | — | its own `gate_report_schema` OPTIONAL rows (1.4.0 `shift_probe`) | exact |
| `contracts/decide-apr-v1.yaml` (mod: `gate.waiver` field + bindings row) | config / contract | — | existing `manifest.bindings` rows (`decide-apr-v1.yaml:146-148` class-A rule) | exact |
| `contracts/decide-tool-boundary-v1.yaml` (CONDITIONAL, only if variant (b), D-26) | config / contract | — | `:302/:311/:326` constants + `aprender-mcp-decide/src/lib.rs:751-792` | role-match |
| `crates/aprender-contrastive-data/examples/laya_dir_to_canonical.rs` (mod: K ≥ 2, identity args) | utility (adapter) | file-I/O transform | itself (`:160-166`, `:58-60`, `:459-461`) | exact |
| `crates/apr-cli/src/commands/eval/setfit.rs` + 7 sibling files (`--dump-probabilities`) | controller (CLI) | file-I/O | `.planning/phases/10-.../10-06-PLAN.md` (deferred plan; files_modified list lines 7-16) | exact (pre-planned) |
| `crates/aprender-mcp-decide-motivation-lambda/{Cargo.toml,src/main.rs,README.md}` (new) | config + entrypoint shim | event-driven (Lambda) | `crates/aprender-mcp-decide-antisemite-lambda/` | exact |
| Root `Cargo.toml`, `monorepo_invariants.rs`, `scripts/duplicate_bin_names_allowlist.txt`, `README.md`, `CLAUDE.md`, `crates/aprender-mcp-decide-lambda/README.md` (mod) | config / test register | — | Phase 10 D-19 additions of the antisemite crate | exact |
| `.planning/spikes/032-motivation-slice-and-tcap/run_cell.py` (new) | utility (spike) | batch | `.claude/skills/spike-findings-aprender/sources/027-laya-calibration-slice-and-tcap/run_cell.py` | exact |
| Kev arm driver (spike-style crate + Python encode/fit adapter) | utility | batch | `.planning/spikes/020-qwen35-batched-prefill/` (`Cargo.toml` path-dep on worktree, `src/main.rs` Row/Head) + `015/tools/fewshot.py` | partial |
| Evidence JSON / `12-HANDOFF.md` | docs | — | `10-TASK-EVIDENCE.json`, `10-GATE-RUN-EVIDENCE.json`, `10-SETFIT-CELL-EVIDENCE.json`, `10-HANDOFF.md` | exact |

## Pattern Assignments

### `scripts/laya_train/prepare_motivation.py` (utility, batch)

**Analog:** `scripts/laya_train/prepare_goldstandard.py`

**Docstring = declared rules before the run** (lines 1-40): usage block (`uv run --project scripts/laya_train --frozen python -I ...`, `--measure`, `--selftest | --selftest-tokenizer`), source "READ IN PLACE", `DECLARED RULES` (label map, grouping by `data.normalize` nfc-trim-ws-v1, RULE conflicting-labels, RULE split, no shift), "NO TEXT", "Write-once". Copy this structure; change label map, `EVAL_DIVISOR` to 4 ("0.25 = 1 / 4"), header, guide citation (docx sha256 + "25 October 2026 draft").

**Imports** (lines ~85-90) — reuse, never copy (OPS-03):
```python
import contract  # noqa: E402
from common import jsonl_bytes, sha256_bytes, sha256_file  # noqa: E402
import data  # noqa: E402
from data import DataError, group_train, normalized_sha256, refuse_overlap  # noqa: E402
from prepare_stance import write_once  # noqa: E402  (one write-once implementation, OPS-03)
```
ADD: `from prepare_goldstandard import measure_task, door_fields, contract_constant, load_tokenizer` (RESEARCH Q4: do NOT copy `report()`/`read_rows`, they are bound to the antisemitism header at `:95, :285-289`).

**Constants** (analog lines 92-111) → values from RESEARCH "Code Examples":
```python
CSV_HEADER = ["Post (English, original)", "Class"]
LABELS = ["social_affiliative", "conspiratorial", "other"]
LABEL_OF = {"Social-affiliative": 0, "Conspiratorial explanation": 1, "None": 2}
SPLIT_SEED = 13
EVAL_DIVISOR = 4
SENTINEL = "..."   # planted in selftest rows only (analog :100)
PINNED = {"rows_in": 395, "distinct_texts": 395, "duplicates_collapsed": 0,
          "mixed_label_groups_dropped": 0, "mixed_label_rows_dropped": 0,
          "csv_sha256": "bc61c76bd29bcdd4c6466266b4e1161841f2107a4f02f5ff4fd96f1b7a3a4ae7"}
DEFAULT_OUT = REPO / "data" / "decide" / "motivation-v1"
```
Expected counts: train {66, 41, 188}, eval {23, 14, 63}.

**Reader with no-text refusals** (analog `read_rows`, lines 131-163): refusals name row number + field only (`csv-missing`, `csv-encoding`, `csv-header` by column count, `csv-row`, `csv-label` (value shown only if ≤ 8 chars), `csv-text`). Row shape for this CSV: `rec[0]` text, `rec[1]` class.

**Core split** (analog `build`, lines 171-212) — copy verbatim apart from label/field names:
```python
rng = np.random.RandomState(SPLIT_SEED)
for c in range(len(LABELS)):
    idx = [i for i, (_, y) in enumerate(kept) if y == c]
    order = rng.permutation(len(idx))
    in_eval.update(idx[j] for j in order[:eval_count(len(idx))])
...
group_train(train_rows); refuse_overlap(train_rows, eval_rows)
data.calibration_split(train_rows, contract.constant("calibration_slice_fraction", "float"),
                       contract.constant("calibration_slice_min_per_class", "int"), SPLIT_SEED, len(LABELS))
task_doc = {"type": "choice", "instructions": ..., "criteria": dict(...)}
files = {"task.json": (json.dumps(task_doc, ensure_ascii=False, indent=2) + "\n").encode("utf-8"), ...}
```
`check_pin` (lines 215-220) copy as-is.

**`--measure`**: write a local `report()` equivalent that calls the imported `measure_task(tok, instructions, criteria)` + `door_fields(p, n, total, explicit)` and supplies post token lengths from this script's own reader (shape of analog `tweet_report`, lines 292-300). Refusals `prefix-cut`, `prefix-row`, `prefix-budget` as analog lines 303-326. Two task variants: script holds one approved TASK (analog lines 113-118 "ONE task per script"); drafts measured via `--task <draft.json>`.

**Selftests** (analog `_synthetic_csv` line 329+): sentinel-in-every-row synthetic CSV, assert sentinel never appears in stdout/stderr. **Pitfall 11:** `--selftest-tokenizer` must assert the stance control P = 57 against a literal, NOT the live `served_task_min_row_tokens` (now 86; analog's assertion is currently red).

---

### `justfile` `laya-prepare-motivation` (wrapper)

**Analog:** `justfile:1318-1328`
```just
laya-prepare-goldstandard *args:
    #!/usr/bin/env bash
    set -euo pipefail
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_goldstandard.py {{args}}
```
Preceding comment block describes rule + gitignored output; selftest rows modelled on `justfile:3434-3437` (`must_fail over-ceiling 'REFUSED prefix-budget' ...`, `must_pass bare '"markers": ?2,...'` → `?3` for this task).

---

### `justfile` `laya-deploy` health retry (F-10-12-1)

**Analog:** `justfile:2068` (current single GET) and the bounded retry at `justfile:2077-2094`:
```bash
TRIES="${LAYA_EDGE_SETTLE_TRIES:-8}"
t=1
while :; do
    set +e
    cargo run --release -p aprender-mcp-decide-lambda --example probe -- ... > "$PLOG" 2>&1
    prc=$?
    set -e
    [ "$prc" -eq 0 ] && break
    if grep -q 'Server is in error state' "$PLOG" && [ "$t" -lt "$TRIES" ]; then
        echo "  edge not settled (...), attempt $t/$TRIES; retrying in 15 s" >&2
        t=$((t + 1)); sleep 15; continue
    fi
    contain "the identity probe failed after $t attempt(s) (log: $PLOG)"
done
```
Apply to the `/health` GET: `curl -sS --max-time 30 -o "$HBODY" -w '%{http_code}'` (no `-f`), retry ONLY on 404 (`LAYA_EDGE_HEALTH_TRIES`, default ~40), any other code contains immediately; keep the `printf > models/decide/deploy-health-$SERVER.json` + `_laya-edge-health-check` lines (2069-2071). Add 404→200 / 404-forever / 500 rows to the offline EDGE HEALTH table in `laya-deploy-selftest` (`justfile:2151`).

---

### `scripts/laya_train/metrics.py` (additions) and `bakeoff.py` (new)

**Analog:** `metrics.py` — `_as_arrays` (:40), `_f1` (:54), `macro_f1` (:62), `ece_top_label(P, y, bins=15)` (:79), `nll` (:105). New functions take the same `(P, y)` signature, go through `_as_arrays`, and `ece_bootstrap_ci` must call `ece_top_label` (never a second ECE). Add `--selftest` cases next to existing ones. `bakeoff.py` reads `eval_probs_schema` files (`{rows: [{row, text_sha256, probabilities}], labels}`) and must reproduce the Rust-recomputed Laya gate numbers bit-for-bit (assert). Output counts only.

---

### `train.py` / `contract.py` (OPTIONAL `pilot_waiver`)

**Analog:** the 1.4.0 optional-field precedent. `contract.py:187-192` `_OPTIONAL_KEY_RULES` (an OPTIONAL key with no rule is refused by `expected_keys`); `train.py:773-776` recipe_id via `contract.recipe_json(...)` (`contract.py:275-301`). Waiver written only when `applies_to` equals the run's `inputs_sha256` + `recipe_id`; never changes `pass` or exit 3. Add a `run_field_bindings` row (sweep fails on unlisted leaf, `laya-finetune-gate-v1.yaml:604`).

---

### `crates/aprender-decide/src/{verify.rs, pack.rs, artifact.rs, artifact/ladder.rs}`

**Refusal site to extend** (`verify.rs:3266-3287`):
```rust
let clauses = check_gate(&inputs.gate_report, &recomputed, n, policy)?;
if !clauses.is_empty() {
    return Err(VerifyError::GateFailed(Box::new(GateFailure { ... })));
}
if inputs.recipe.seed_selection.is_none() {
    return Err(VerifyError::SeedPolicyMissing);
}
```
Waiver only when `clauses == [GateClause::EcePost]` and a policy entry matches id + applies_to + `waives == [ece_post]`; everything else unchanged. Policy mapping only in `VerifyPolicy::from_contract_views` (`verify.rs:378`); `GateContractView` is not `deny_unknown_fields` (`:205-211`).

**Optional read field** (`pack.rs:351-357`):
```rust
#[serde(default)]
pub rescore_noise_sha256: Option<String>,
#[serde(default)]
pub shift_probe: Option<ShiftProbe>,
```
**Manifest field** (`artifact.rs:206-215`, filled at `:1213-1218`): add `waiver: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` — MANDATORY so `laya-antisemitism-v1.apr` (sha `2293bebb…`) re-packs byte-identically. Contract annotation `#[contract("laya-finetune-gate-v1", equation = "gate_pass")]` style on the new fn; falsification table in RESEARCH Q2 (9 cases).

---

### `crates/aprender-contrastive-data/examples/laya_dir_to_canonical.rs` (K ≥ 2)

**Current 2-label lock** (`:160-166`):
```rust
let names = doc.criteria.0;
if names.len() != 2 {
    return Err(AdaptError::Data(format!(
        "task.json has {} criteria; this adapter writes the two-label cell only", names.len())));
}
```
Hardcoded identity to parameterise with antisemitism defaults (byte-identical Phase 10 output): `:58-60` (`CARVE_DOMAIN "laya-goldstandard-validation-carve"`, `DEFAULT_LAYA_DIR`, `DEFAULT_OUTPUT_DIR`) and the Manifest at `:459-461` (`dataset: "goldstandard-2024"`, `target: "antisemitism"`, `task: "single-label two-class antisemitism classification"`). Keep the empty-name/duplicate-name checks (`:167-178`). SetFit cell recipe: `justfile:1351` `laya-setfit-cell` with shots 32 (D-25).

---

### `apr eval --dump-probabilities` (deferred 10-06)

**Analog/spec:** `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-06-PLAN.md` — `files_modified` (lines 7-16: `extended_commands.rs`, `dispatch_analysis.rs`, `commands/eval/setfit.rs`, `setfit_tests.rs`, `setfit_tag_tests.rs`, `lib_dispatch_coverage.rs`, `lib_verbose_inheritance_parse.rs`, `lib_parse_rosetta_02.rs`), Task 1 at line 139 (clap `#[arg(long = "dump-probabilities", value_name = "FILE")]`, `check_split_flags` refusal off the test split, `--force` coverage). Execute that plan's tasks; output converted to `eval_probs_schema` for bakeoff.

---

### `crates/aprender-mcp-decide-motivation-lambda/` (new thin crate)

**Analog:** `crates/aprender-mcp-decide-antisemite-lambda/Cargo.toml` — copy with name/description changed:
```toml
[package]
name = "aprender-mcp-decide-motivation-lambda"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true
rust-version.workspace = true
description = "Lambda bootstrap of the aprender-mcp-decide-motivation server (a thin shim over the shared decide runtime)"
readme = "README.md"
publish = false

[[bin]]
name = "bootstrap"
path = "src/main.rs"

[dependencies]
aprender-mcp-decide-lambda = { path = "../aprender-mcp-decide-lambda" }

[lints]
workspace = true
```
`src/main.rs` (verbatim, doc comment names the new server id + Phase 12 D-02):
```rust
fn main() {
    if let Err(e) = aprender_mcp_decide_lambda::bootstrap::run(env!("CARGO_PKG_NAME")) {
        eprintln!("{}: {e}", env!("CARGO_PKG_NAME"));
        std::process::exit(1);
    }
}
```
README: copy antisemite README (shim statement, "one server id, one thin crate", zigbuild command), naming the motivation dataset; must link the monorepo (`test_every_readme_links_monorepo`).

### Register files (D-02)

- `crates/aprender-core/tests/monorepo_invariants.rs:312-322` add entry in the same comment style:
  `"aprender-mcp-decide-antisemite-lambda", // server id ...: the decide runtime's Lambda bootstrap (Phase 10 D-19)` → motivation line citing Phase 12 D-02; `:445` `DEPLOYMENT_UNIT_BASELINE: usize = 9` → 10 and update the `:440-444` comment.
- `scripts/duplicate_bin_names_allowlist.txt:16` append `aprender-mcp-decide-motivation-lambda` to the `bootstrap ...` line (sorted).
- Root `Cargo.toml` members: next to the antisemite entry (~line 75). `README.md` crate count +1 (equality).
- `crates/aprender-mcp-decide-lambda/src/tests.rs:622-635` `thin_server_crates_are_shims` discovers by dir: no edit.

---

### Spike harness `.planning/spikes/032-motivation-slice-and-tcap/`

**Analog:** `.claude/skills/spike-findings-aprender/sources/027-laya-calibration-slice-and-tcap/run_cell.py` (tracked). Drives `train.SeedRun.train_and_score` unchanged, refits T unclamped / `[0.5,5]` / `[0.5,10]`, oracle T on `T_GRID`. Add `--fraction` hook into `data.calibration_split`; add a guard refusing any `eval.jsonl` path; data = train-only re-split. Declare clamp-binding + slice rules before first run.

### Kev arm (partial analog)

**Analogs:** `.planning/spikes/020-qwen35-batched-prefill/` (`Cargo.toml` path-dep `realizar = { path = "/Users/guy/Development/machine-learning/aprender-016-upstream-sync/crates/aprender-serve", ... }`; `src/main.rs` `Row { ids, decide, opts, … }`, `Head::load`), `.planning/spikes/017-kev-rust-forward-parity/tools/export_and_oracle.py` (Python `kev.model.encode` → ids JSON), `.planning/spikes/015-kev-vs-setfit-few-shot/tools/fewshot.py` (`bias` adapter). Output in `eval_probs_schema`.

## Shared Patterns

### No-text output / refusal by row number
**Source:** `prepare_goldstandard.py:131-163` + sentinel selftest (`:100`, `:329+`). **Apply to:** prepare_motivation.py, bakeoff.py, spike harness, Kev adapter, all evidence.

### Write-once outputs
**Source:** `prepare_stance.write_once` (imported, OPS-03). **Apply to:** data dir, adapter output.

### Declare-before-read
Contract/docstring rules committed before the step that reads labels (split, slice, epochs, waiver `applies_to`, clamp rule, Kev budget). **Apply to:** every plan's spine.

### Optional field compatibility
`#[serde(default)]` read + `skip_serializing_if = "Option::is_none"` write; Python `_OPTIONAL_KEY_RULES`. **Apply to:** waiver across gate report, manifest, contracts.

### Bounded retry then contain
**Source:** `justfile:2077-2094`. **Apply to:** health 404 retry.

### One metrics implementation
**Source:** `metrics.py` (bit-identical with Rust, `numeric_agreement`). **Apply to:** bakeoff, spike, SetFit/Kev scoring.

### Evidence JSON shape
**Source:** `10-TASK-EVIDENCE.json`, `10-GATE-RUN-EVIDENCE.json`, `10-SETFIT-CELL-EVIDENCE.json`, `10-ACCEPTANCE-REPLAY.json`: `schema`, `phase`, `plan`, `measured_on`, `commit`, closed-set `outcome` with explicit `blocked` branch, verbatim `owner_reply`, hashes not text.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| Kev-4B merge/GGUF + Rust driver at 4B | utility | batch | Only 0.8B spike drivers exist; 4B never run in Rust (RESEARCH Q5). Use 020/017 as structural template only. |

## Metadata

**Analog search scope:** `scripts/laya_train/`, `crates/aprender-decide/`, `crates/aprender-mcp-decide*/`, `crates/aprender-contrastive-data/`, `crates/aprender-core/tests/`, `justfile`, `.planning/phases/10-*`, `.planning/spikes/`, skill sources.
**Files scanned:** ~15
**Pattern extraction date:** 2026-10-26
