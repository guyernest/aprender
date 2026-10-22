# Phase 10: antisemitism-v1 Decide Server for experiment-coach - Pattern Map

**Mapped:** 2026-10-21
**Files analyzed:** 13 new or modified (plus 5 phase-directory evidence/hand-off records)
**Analogs found:** 12 / 13 (the SetFit data adapter has a role-match analog only; its exact shape has never existed)

Every analog path below was checked with `git ls-files` this session and is tracked source. Nothing under `.gsd/capabilities/` is cited. The spike driver `.planning/spikes/027-laya-calibration-slice-and-tcap/run_cell.py` is tracked under `.planning/` (the spike dir's own `.gitignore` covers `data/` and `results/` only). The partner repo `/Users/guy/projects/mcp/motivation-experiment` is read-only context and is not mapped as a target.

Line numbers are from the files as read on 2026-10-21; re-grep before quoting them in a plan if the file has moved.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `scripts/laya_train/prepare_goldstandard.py` (new) | utility / data-prep script | file-I/O, batch transform (CSV -> JSONL + task.json) | `scripts/laya_train/prepare_stance.py` + `scripts/laya_train/data.py` (`normalize`, `group_train`, `calibration_split`, `selftest`) | exact (same role, same dir, same output contract) |
| `data/decide/goldstandard-2024/task.json` (new, gitignored) | config (served task) | file-I/O | the `task` dict in `prepare_stance.py:45-51, 131-132`; schema in `scripts/laya_train/data.py:48, 93-129` (`load_task`) | exact |
| `scripts/laya_train/prepare_goldstandard.py --measure` (mode of the above) | utility / measurement | transform (tokenizer only) | no in-tree Python analog; the Rust side is `crates/aprender-mcp-decide-lambda/examples/probe.rs:165-188` (`--plan-only`) and RESEARCH.md Code Examples | role-match |
| `scripts/laya_train/probe_epoch.py` (new) | utility / driver | batch (one training epoch, no run dir) | `.planning/spikes/027-laya-calibration-slice-and-tcap/run_cell.py` (imports `train`, marks output non-gate) + `scripts/laya_train/train.py:233-250, 303-397` (`Base`, `train_seed`) | exact |
| SetFit cell data adapter (new; `crates/apr-cli/examples/<name>.rs` behind `setfit`, or a new example in `crates/aprender-contrastive-data`) | utility / data adapter | file-I/O, transform (Laya JSONL -> attested canonical dir) | `crates/apr-cli/src/commands/data_tweeteval.rs:486-512, 612-630, 775-800` (`prepare_canonical`, `BenchmarkManifest`, `write_outputs`) | role-match (same doors, different source) |
| SetFit cell driver (justfile recipe `laya-setfit-cell`, or steps in a plan) | config / script | batch (select -> train -> eval) | `scripts/run_bench_cells.sh:200-220, 322-335` (`apr data select`, `|| rc=$?` form) | role-match |
| `crates/aprender-mcp-decide-lambda/examples/probe.rs` (modify: `--replay`) | CLI example | request-response (JSON-RPC over HTTPS) | itself: `examples/probe.rs:56-119` (arg parsing), `165-257` (checks + JSON line); library side `src/probe.rs:352-420` (`run_identity_probe`) | exact |
| `crates/aprender-mcp-decide-lambda/src/probe.rs` (modify if the replay check lives in the library) | library + test | request-response | `src/probe.rs:85-116` (`ProbeReport` + `ok()`), `796-823` (`probe_fails_when_response_labels_differ` loopback test) | exact |
| `contracts/decide-tool-boundary-v1.yaml` (modify: `served_task_min_row_tokens`, FALSIFY-DECIDE-TOOL-009 harness) | config (contract) | n/a | itself: lines 284-306 (the bounds block), 648-656 (FALSIFY-009) | exact |
| `crates/aprender-mcp-decide-lambda/README.md` (modify: "Deployed" table) | docs | n/a | itself, lines 76-88 | exact |
| `scripts/laya_claims.tsv` (modify rows 81, 111; add rows for new sentences) | config (claims ledger) | n/a | itself, rows 1-3, 81, 111 | exact |
| `scripts/laya_gates.tsv` (add a row for the `8 x P <= 800` gate) | config (gate table) | n/a | itself, rows 1-6 | exact |
| `justfile` (add `laya-prepare-goldstandard`, `laya-probe-epoch`, `laya-setfit-cell`) | config (recipes) | batch | `justfile:1305-1317` (`laya-prepare-stance`), `2384-2447` (`laya-upload` refusal idiom), `2599-2625` (`laya-deploy-verify` probe build) | exact |
| `10-PROBE-EVIDENCE.json`, `10-GATE-RUN-EVIDENCE.json`, `10-SETFIT-CELL-EVIDENCE.json`, `10-LIVE-DEPLOY-EVIDENCE.json`, `10-ACCEPTANCE-REPLAY.json`, `10-HANDOFF.md` (new, phase dir) | evidence records | file-I/O | `08-GATE-RUN-EVIDENCE.json`, `08-LIVE-REDEPLOY-EVIDENCE.json` in the Phase 8 directory (key shapes below) | exact |

## Pattern Assignments

### `scripts/laya_train/prepare_goldstandard.py` (data-prep script, CSV -> Laya data dir)

**Analog:** `scripts/laya_train/prepare_stance.py` (207 lines, read whole) and `scripts/laya_train/data.py`

**Module docstring declares the rule, the command line, and the no-text promise** (prepare_stance.py:1-27). Copy this form; the conflict rule, the split seed and the grouping rule go here BEFORE the run (CONTEXT Claude's Discretion):
```python
"""TweetEval stance_abortion -> a Laya data dir (D-19 as amended by A2, laya-finetune-gate-v1 1.4.0).

    uv run --project scripts/laya_train --frozen python scripts/laya_train/prepare_stance.py [--cell s64|s16] [--out DIR]
    (or: just laya-prepare-stance [s64|s16])
...
Never overwrites: an existing non-empty --out is refused unless every file already holds exactly the bytes
this run would write (re-running is then a no-op). Output lands under the root-anchored, gitignored /data/:
tweet text is never committed and never printed (counts and sha256s only).
"""
```

**Imports pattern** (prepare_stance.py:28-42) — sibling imports after a `sys.path.insert`, `contract.REPO` as the root:
```python
import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import contract  # noqa: E402
from common import jsonl_bytes  # noqa: E402
from data import (DataError, decode_jsonl, exact_sha256, in_distribution_heldout, normalized_sha256,  # noqa: E402
                  refuse_unencodable, sha256_bytes)

REPO = contract.REPO
SRC = REPO / "data" / "tweet-eval-stance"
OUT = REPO / "data" / "decide" / "tweet-stance-16"
```
For GoldStandard: import `normalize, normalized_sha256, group_train, DataError, sha256_bytes` from `data`, `csv` from stdlib, `numpy` for the seeded draw; `OUT = REPO / "data" / "decide" / "goldstandard-2024"`; the default CSV path is the partner repo path (read in place, never copied).

**Task dict and criteria order** (prepare_stance.py:45-51, 120-122, 131-132) — document order IS the label index (D-02), so the dict lists `not_antisemitic` first and the script cross-checks it:
```python
INSTRUCTIONS = "What stance does the author of this tweet take on abortion?"
DESCRIPTIONS = {
    "none": "No stance on abortion, or the tweet is not about abortion",
    "against": "Opposes abortion (pro-life)",
    "favor": "Supports abortion rights (pro-choice)",
}
...
    order = list(decl["criteria_order"])
    if list(DESCRIPTIONS) != order:
        fail("criteria descriptions %s are not in the contract order %s" % (list(DESCRIPTIONS), order))
...
    task = {"type": "choice", "instructions": INSTRUCTIONS, "criteria": DESCRIPTIONS}
    files = {"task.json": (json.dumps(task, ensure_ascii=False, indent=2) + "\n").encode("utf-8"),
             "train.jsonl": jsonl_bytes(shots)}
```
Rows are `[(text, label_name)]` through `common.jsonl_bytes`; `data.load_task` (data.py:48 `TASK_KEYS = ("type", "instructions", "criteria")`) refuses any other key. Keep the two descriptions one line each (48-token option cap, RESEARCH Q2); the guide lives in `INSTRUCTIONS`.

**Two failure classes, two exit codes** (prepare_stance.py:54-57, 189-194): dataset inconsistency -> `fail()` exit 1; an input refusal -> `DataError` -> one `REFUSED <rule>` line, exit 2, no traceback:
```python
def fail(msg):
    """A consistency check on the pinned public dataset or the committed manifest failed (exit 1)."""
    print("PREPARE FAILED: " + msg, file=sys.stderr)
    sys.exit(1)
...
    try:
        files, summary = build(args.cell, args.src)
        status = write_once(out, files)
    except DataError as e:                     # every input refusal: one REFUSED <rule> line, exit 2, no traceback
        print(str(e), file=sys.stderr)
        return 2
```

**Missing-source hint** (prepare_stance.py:123-126) — the `apr data`-style hint the justfile recipe repeats:
```python
    for name in need:
        if not (src / name).is_file():
            fail("%s is missing; run: apr data tweet-eval-stance --output data/tweet-eval-stance" % (src / name))
```

**Normalization and grouping — do not re-implement** (data.py:44-47, 61-78, 263-274). `normalize` is the Rust `split_whitespace` set, not `str.split`; `group_train` is the grouping rule and the conflict detector the trainer will run again:
```python
def normalize(text):
    composed = unicodedata.normalize("NFC", text)
    words, cur = [], []
    for ch in composed:
        if ch in _WHITE_SPACE:
            ...
    return " ".join(words)

def normalized_sha256(text):
    return sha256_bytes(normalize(text).encode("utf-8"))

def group_train(rows):
    """{normalized hash: [row indices]} in first-seen order; conflicting labels refused."""
    groups = {}
    for i, (t, _) in enumerate(rows):
        groups.setdefault(normalized_sha256(t), []).append(i)
    for h, ids in groups.items():
        labs = sorted({rows[i][1] for i in ids})
        if len(labs) > 1:
            raise DataError("train-conflicting-labels", ...)
    return groups
```
For GoldStandard the prepare script groups BEFORE labelling (so it can drop the 5 mixed groups by rule and count them) and then, as its own assertion, calls `data.group_train` on the rows it is about to write so the `train-conflicting-labels` refusal can never fire later.

**Seeded, stratified, group-disjoint draw — the template for the 20 % eval split** (data.py:283-307). Same shape, with `fraction=0.2` over groups per class and `np.random.RandomState(SEED)`; the trainer draws its own 25 % calibration slice from the TRAIN file later, so the prepare script must write only `train.jsonl` / `eval.jsonl`:
```python
def calibration_split(rows, fraction, min_per_class, seed, k):
    """Group-disjoint, stratified, seeded calibration slice of the train rows."""
    groups = group_train(rows)
    counts = class_counts(rows, k)
    rng = np.random.RandomState(int(seed))
    calib = []
    for c in range(k):
        need = max(int(min_per_class), int(math.ceil(float(fraction) * counts[c])))
        cls_groups = [ids for ids in groups.values() if rows[ids[0]][1] == c]
        order = rng.permutation(len(cls_groups))
        took = 0
        for g in order:
            if took >= need:
                break
            calib.extend(cls_groups[g])
            took += len(cls_groups[g])
        if took < need or took >= counts[c]:
            raise DataError("train-class-too-small", ...)
    slice_ids = sorted(calib)
    in_slice = set(slice_ids)
    fit_ids = [i for i in range(len(rows)) if i not in in_slice]
    return fit_ids, slice_ids, slice_ids, slice_ids_sha256(slice_ids)
```
Pre-assert what the trainer asserts: `data.refuse_overlap(train_rows, eval_rows)` (data.py:222-228) and every criterion present in eval (`eval-class-coverage`, data.py:210-219).

**Write-once output** (prepare_stance.py:161-178) — copy verbatim; it is what makes a re-run a no-op and refuses a stale `shift.jsonl`:
```python
def write_once(out, files):
    present = sorted(q.name for q in out.iterdir() if not q.name.startswith(".")) if out.exists() else []
    if present:
        extra = sorted(set(present) - set(files))
        differ = sorted(n for n, b in files.items() if not (out / n).is_file() or (out / n).read_bytes() != b)
        if extra or differ:
            raise DataError("prepare-out-dir", "%s exists and differs from what this run would write (differing or "
                            "missing %s, foreign %s); refusing to overwrite" % (out, differ, extra))
        return "unchanged (byte-identical)"
    out.mkdir(parents=True, exist_ok=True)
    for name, body in files.items():
        (out / name).write_bytes(body)
    return "written"
```

**Counts-and-hashes-only report** (prepare_stance.py:195-203) — the only stdout the script may produce:
```python
    print("prepared %s (cell %s, %s): %s" % (where, args.cell, status, json.dumps(summary, sort_keys=True)))
    for name in sorted(files):
        print("  %-12s %4d lines  sha256 %s" % (name, files[name].count(b"\n"), sha256_bytes(files[name])))
    print("PREPARE OK")
    return 0
```
Add to the summary: `duplicates_collapsed`, `mixed_label_groups_dropped` (expected 5 / 13 rows), per-class train/eval counts, `split_seed`.

**`--selftest` skeleton** (data.py:593-640) — `case()` / `expect()` over a `tempfile.TemporaryDirectory`, numpy-only, exit code = number of failures; the new script's selftest builds a synthetic CSV (no tweet text) and asserts the dedup, the mixed-label drop, eval disjointness, stratification and the `write_once` refusal:
```python
def selftest():
    failures = []

    def case(name, ok, detail=""):
        print("  %-4s %-68s %s" % ("ok" if ok else "FAIL", name, detail))
        if not ok:
            failures.append(name)

    def expect(name, rule, fn):
        try:
            fn()
            case(name, False, "accepted")
        except DataError as e:
            case(name, e.rule == rule, str(e)[:110])
        except Exception as e:                        # a traceback-class escape is a FAIL, never a crash
            case(name, False, "raised %s, not a DataError %s: %s" % (type(e).__name__, rule, str(e)[:70]))

    with tempfile.TemporaryDirectory(prefix="laya-data-selftest-") as tmp:
        tmp = Path(tmp)
        def write(name, text): ...
```
Entry point as data.py:824-826: `if sys.argv[1:] == ["--selftest"]: sys.exit(selftest())`.

**`--measure` mode:** no Python analog exists. Use the RESEARCH.md "Code Examples" snippet (`laya.common.build_sequence(tok, "", q, 512, 192)` -> `P`; `encode_text` -> `t`), print `P`, marker count (must equal 2), per-option token counts (flag 48 = cut), head retained vs full instruction tokens, row-length percentiles, and assert `8 * P <= 800`. The Rust cross-check is `probe --plan-only --apr <apr> --maximal distributed` (probe.rs:174-188 below), whose `tokens_planned` must equal `8 x P`.

---

### `scripts/laya_train/probe_epoch.py` (driver, one epoch, no run dir)

**Analog:** `.planning/spikes/027-laya-calibration-slice-and-tcap/run_cell.py` (docstring + imports 1-60, run body 190-215) with `scripts/laya_train/train.py` as the library.

**Docstring declares "a MEASUREMENT, not a gate run" and the exact reuse** (run_cell.py:1-8) — copy the stance:
```python
"""Spike 027: one (size, recipe) cell over seeds -- a MEASUREMENT, not a gate run.

    uv run --frozen --project scripts/laya_train python \
        .planning/spikes/027-laya-calibration-slice-and-tcap/run_cell.py --size s64 --recipe es12 --seeds 13,17,23

Reuses scripts/laya_train unchanged: data.py (validation, overlap refusal, the contract's group-disjoint
calibration split with the DECLARED seed 13), contract.py (recipe values, base pin, early-stopping block),
train.SeedRun.train_and_score ...
```

**Imports pattern** (run_cell.py:29-48) — the driver lives beside `train.py`, so the `sys.path.insert` collapses to `HERE`:
```python
REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "scripts" / "laya_train"))
import numpy as np  # noqa: E402
import torch  # noqa: E402

import contract  # noqa: E402
import data  # noqa: E402
import gate  # noqa: E402
import metrics  # noqa: E402
import train  # noqa: E402
from common import f32_list, sha256_bytes, write_json  # noqa: E402
```

**The spike marks its output as non-gate** (run_cell.py:199-201). The probe goes one step further and writes NO run dir at all (CONTEXT discretion; RESEARCH Q3): no `recipe.json`, no checkpoint, nothing under `models/decide`. The only output is the evidence JSON in the phase directory. If a scratch dir is unavoidable, the marker is the precedent:
```python
        (out / "NOT-A-GATE-RUN.txt").write_text(
            "spike 027 measurement (recipe %s, seed %d, calibration split seed %d). Not a declared gate run (D-07).\n"
            % (a.recipe, seed, split_seed))
```

**Base construction in the production variant** (train.py:233-245) — `Base(args)` reads the contract pin when `variant == "production"` and refuses `--base`:
```python
class Base:
    def __init__(self, args):
        if args.variant == "production":
            if args.base or args.base_sha256:
                refuse("REFUSED base: --base is accepted only with --variant synthetic-fixture; production "
                       "always uses the contract base")
            b = contract.base()
            self.src, self.revision, self.digest = b["repo"], b["revision"], b["model_safetensors_sha256"]
            self.block = contract.production_base_block()
            self._dir = None
```
Build it with `types.SimpleNamespace(variant="production", base=None, base_sha256=None)`; `base.load(device)` (train.py:253-254) is what `train_seed` calls.

**Device request** (train.py:186-196) — call `train.request_device(None)` so the probe's device policy is the trainer's; read the actual device back from `info["device_used"]`, never from the request (Verification Discipline 2):
```python
def request_device(forced):
    if forced:
        return forced
    for d in contract.device_order():
        if d == "mps" and hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            return "mps"
        if d == "cuda" and torch.cuda.is_available():
            return "cuda"
        if d == "cpu":
            return "cpu"
    return "cpu"
```

**`train_seed` signature and the `info` it returns** (train.py:303-307, 314, 333, 390-397) — the probe calls it once with `epochs=1, stopping="fixed_epochs"` (no calibration monitor, no `calib_rows`):
```python
def train_seed(seed, base, requested, question, fit_rows, k, epochs, stopping="fixed_epochs", calib_rows=None):
    """The spike-024 ft_laya.py loop on Laya's own rows; returns (agent, info).
    ...
    agent, device_used = base.load(requested)
    log("DEVICE seed=%d requested=%s used=%s torch=%s" % (seed, requested, device_used, torch.__version__))
    ...
    items = [agent._encode_state(t, ["q"], internal)[0] for t, _ in fit_rows]
    ...
    steps = epochs * math.ceil(len(items) / bs)
    ...
    info = {"seed": seed, "device_requested": requested, "device_used": device_used, "epochs": epochs,
            "stopping": stopping, "steps": steps_run, "steps_planned": steps, "epochs_run": epochs_run,
            "fit_rows": len(items), "train_seconds": round(train_s, 1), "loss_temperature": t_loss,
            "ce_first": ..., "ce_last": ..., "stopping_record": stop_record}
    log("TRAIN seed=%d fit_rows=%d epochs=%d epochs_run=%d steps=%d/%d seconds=%.1f ce_first=%s ce_last=%s" % (...))
    return agent, info
```
`s_per_step = info["train_seconds"] / info["steps"]`. Row-length stats come from the same `agent._encode_state(t, ["q"], internal)[0]["ids"]` path (train.py:320), so the probe's P and percentiles are the training rows, not an estimate.

**Data validation before the model loads** — the probe repeats `train.main`'s sequence exactly (RESEARCH Code Examples): `data.load_task`, `data.load_rows(train)`, `data.load_rows(eval)`, `data.refuse_overlap`, `data.calibration_split(rows, 0.25, 2, contract.declared_seed(), k)` and uses only the fit ids. A `--dry-run` that stops here and prints `fit_rows` and `steps_planned = ceil(fit_rows / 8)` is the Wave 0 tracer (RESEARCH Wave 0 Gaps).

**Exit codes** (train.py:93): `EXIT_PASS, EXIT_REFUSED, EXIT_GATE_FAIL = 0, 2, 3`; the probe uses 0 / 2 only (it has no gate).

---

### SetFit cell data adapter (Laya `{text,label}` JSONL -> attested canonical dir)

**Analog (role-match):** `crates/apr-cli/src/commands/data_tweeteval.rs` — the only existing writer of an attested canonical dataset. The adapter maps rows; the crate computes every hash (RESEARCH "Don't Hand-Roll").

**Assembly through the public door** (data_tweeteval.rs:486-508):
```rust
    let decls = CanonicalDeclarations {
        train: split_decl(TRAIN_COUNTS),
        validation: split_decl(VALIDATION_COUNTS),
        test: split_decl(TEST_COUNTS),
        label_names: label_names(),
    };
    let mut ledger = AccessLedger::new();
    let dataset = PreparedDataset::<Canonical>::from_labeled_rows(
        train,
        validation,
        test,
        &decls,
        &mut ledger,
    )
    .map_err(|error| dataset_error(&error))?;
    let jsonl = dataset
        .encode_jsonl()
        .map_err(|error| dataset_error(&error))?;
    let attestation = DatasetAttestation::from_prepared(&dataset);
```
Declarations are per-split exact class counts plus the shared label map (`crates/aprender-contrastive-data/src/split.rs:74-79`):
```rust
pub struct SplitDeclaration {
    /// Exact expected row count per class, indexed by class label.
    pub expected_class_counts: Vec<usize>,
    /// The label map. `label_names[label]` must equal every row's `label_text`.
    pub label_names: Vec<String>,
}
```
For GoldStandard: `label_names = ["not_antisemitic", "antisemitic"]` (the Laya criteria order); `train` = Laya train minus a seeded, stratified, text-disjoint 10 % validation carve (never from eval); `test` = Laya `eval.jsonl` row for row; `expected_class_counts` are computed from the rows the adapter is about to pass (the declaration is the assertion). Rows are `LabeledExample { id, input, label, label_text, source_split }` (`crates/aprender-contrastive-data/src/schema.rs:36-52`, `deny_unknown_fields`); mint ids as `gs-<normalized sha256 prefix>` or the row ordinal, deterministic.

**Manifest shape the `apr` doors read** (data_tweeteval.rs:612-630 for the write; 660-700 for the read):
```rust
    let manifest = BenchmarkManifest {
        schema_version: DATASET_ATTESTATION_SCHEMA_VERSION,   // = 2, crates/aprender-contrastive-data/src/attestation.rs:55
        dataset: DATASET_ID,
        target: TARGET,
        task: "single-label three-class stance classification",
        profile: profile_name(profile),
        labels,
        source: SourceManifest { repository: UPSTREAM_REPOSITORY, data_path: UPSTREAM_DATA_PATH, ... },
        ...
    };
```
The reader (`attestation_bytes_from_manifest`, data_tweeteval.rs:667-700) requires only a numeric `schema_version` in `SUPPORTED_DATASET_ATTESTATION_SCHEMA_VERSIONS` and a `dataset_attestation` object; `read_attested_canonical` then reads `{role}.jsonl` for the declared roles. `BenchmarkManifest` is private to `data_tweeteval.rs`, so the adapter writes its own serde struct with at least `schema_version`, `dataset` (`"goldstandard-2024"`), `task` (`"single-label two-class antisemitism classification"`), `labels`, `source` (partner path + Laya data dir sha256s), and `dataset_attestation: DatasetAttestation` (`to_bytes` at attestation.rs:112-118, or serialize the struct directly).

**Refuse-to-overwrite writer** (data_tweeteval.rs:775-800) — same discipline as `write_once`:
```rust
fn write_outputs(output_dir: &Path, profile: TweetEvalStanceProfile, split_bytes: &BTreeMap<String, Vec<u8>>,
                 manifest: &BenchmarkManifest, force: bool) -> Result<()> {
    let known_files = ["train.jsonl", "validation.jsonl", "test.jsonl", MANIFEST_FILE];
    if !force {
        if let Some(existing) = known_files.iter().map(|name| output_dir.join(name)).find(|path| path.exists()) {
            return Err(CliError::ValidationFailed(format!(
                "Refusing to replace existing benchmark file {} (pass --force to replace known outputs)",
                existing.display())));
        }
    }
    fs::create_dir_all(output_dir)?;
```

**Placement:** `crates/apr-cli/examples/` already holds four examples (`gpu_chat_inference.rs`, `ptx_parity_validation.rs`, `serve_with_tracing.rs`, `tool_calling_demo.rs`); `crates/aprender-contrastive-data` has no `examples/` dir. Either is admissible; the contrastive-data crate is the smaller dependency surface and its unit test (round-trip `from_labeled_rows -> encode_jsonl -> read back`) is the Wave 0 item. Whichever crate is touched gets `cargo clippy -p <crate> --examples -- -D warnings` (no `unwrap()`; use `expect()`/`?`).

**Test pattern to copy:** `data_tweeteval.rs:1379-1392` (`manifest_is_schema_version_two_with_an_attestation_and_an_exclusion_record`) asserts `manifest["schema_version"] == 2` and `attestation["schema_version"] == 2` on the written bytes.

---

### SetFit cell driver (recipe or plan steps: select -> train -> eval)

**Analog:** `scripts/run_bench_cells.sh:200-220` (selection) and `322-335` (cell). Copy the `|| rc=$?` form and the pinned `"$APR"`; do NOT copy `setfit bench run` (TweetEval-bound, three-class; RESEARCH Q6):
```bash
            rc=0
            "$APR" data select --data "$DATA_DIR" --shots "$shots" --seed "$seed" \
                --output "$target_dir" || rc=$?
            if [[ "$rc" -ne 0 ]]; then
                printf 'FAIL selection s%s/seed%s (apr data select exited %s)\n' "$shots" "$seed" "$rc" >&2
                exit "$EXIT_TRANSIENT"
            fi
```
The cell itself is `"$APR" setfit train --config <toml: epochs=1,batch_size=16> --data <adapted> --selection <dir>/selection-manifest.json --model-dir <MiniLM snapshot 1110a243> --output <apr> --seed 13` (dry-run first), then `"$APR" eval setfit --artifact <apr> --data <adapted> --selection <sel> --split test`. `apr` is built with `--features setfit` and pinned via `. scripts/apr_bin.sh || exit 1`. A lifecycle refusal is recorded verbatim and ends the cell (Pattern 3, refusal is a result).

---

### `crates/aprender-mcp-decide-lambda/examples/probe.rs` (modify: `--replay` mode)

**Analog:** the file itself (257 lines, read whole) plus `src/probe.rs`.

**Arg parsing pattern** (probe.rs:56-119) — `args_os`, a `value()` closure, a `match` on flag names, a USAGE string, consistency checks before `Ok(Args{..})`. Add `replay: Option<PathBuf>` the same way and extend USAGE:
```rust
struct Args {
    url: Option<String>,
    apr: PathBuf,
    expect_sha256: Option<String>,
    maximal: Option<MaximalShape>,
    cold_first: bool,
    probe_id: Option<String>,
    bearer: Option<String>,
    plan_only: bool,
}
...
        match arg.as_str() {
            "--url" => url = Some(value("--url")?),
            "--apr" => apr = Some(PathBuf::from(value("--apr")?)),
            "--expect-sha256" => expect_sha256 = Some(value("--expect-sha256")?),
            "--maximal" => maximal = Some(value("--maximal")?.parse::<MaximalShape>()?),
            "--cold-first" => cold_first = true,
            "--probe-id" => probe_id = Some(value("--probe-id")?),
            "--bearer" => bearer = Some(value("--bearer")?),
            "--plan-only" => plan_only = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    if plan_only && maximal.is_none() {
        return Err("--plan-only needs --maximal".to_string());
    }
```

**Identity and labels come from `--apr`, and `--expect-sha256` must agree** (probe.rs:140-154) — the replay reuses `expected` and `labels` unchanged:
```rust
    let model = match aprender_mcp_decide::load_model_from_path(&args.apr) {
        Ok(m) => m,
        Err(e) => return fail(&format!("cannot load --apr: {e}")),
    };
    let local_sha = model.identity().artifact_sha256.clone();
    let expected = match &args.expect_sha256 {
        Some(given) if *given != local_sha => {
            return fail(&format!("--expect-sha256 {given} disagrees with --apr, which hashes to {local_sha}"))
        }
        Some(given) => given.clone(),
        None => local_sha,
    };
    let labels = model.task().owned_labels();
```

**One JSON line per probe, checks as booleans, `ok &=` into the exit code** (probe.rs:193-225, 252-256). The replay line follows this shape with the new checks `one_result_per_text`, `probabilities_len_match`, `sums_within_1e-5`, `label_is_argmax`, `labels_in_task_order`, `identity_matches`, `elapsed_ms`, `tokens_total`:
```rust
    let mut ok = true;
    if !args.cold_first {
        match run_identity_probe(url, bearer, &expected, &labels, &probe_id).await {
            Ok(report) => {
                ok &= report.ok();
                let elapsed_ms: Vec<u128> = report.calls.iter().map(|c| c.elapsed_ms).collect();
                println!("{}", serde_json::json!({
                    "probe": "identity",
                    "probe_id": report.probe_id,
                    "one_tool_named_classify": report.one_tool_named_classify,
                    "description_has_labels_in_order": report.description_has_labels_in_order,
                    "response_labels_match": report.response_labels_match,
                    "identity_matches": report.identity_matches,
                    "artifact_sha256": report.artifact_sha256,
                    "elapsed_ms": elapsed_ms,
                    "tokens_total": report.tokens_total,
                    "shape": "identity",
                    "load_header": load_header,
                }));
            }
            Err(e) => return fail(&format!("identity probe failed: {e}")),
        }
    ...
    if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
```
`#![allow(clippy::disallowed_methods)]` at the top of the example (probe.rs:29) is the sanctioned exception for `serde_json::json!`; keep it, add nothing else.

**Library side to extend** (`src/probe.rs:352-420`, `run_identity_probe`): the RPC helper `Rpc::new(url, bearer, probe_id)`, `rpc.call("tools/call", classify_params(&texts))`, `classify_payload(&result)`, and the `payload["labels"]` / `payload["model"]["artifact_sha256"]` / `tokens_and_truncated(&payload)` readers already exist. A `run_replay(url, bearer, expected_sha256, expected_labels, texts, probe_id) -> Result<ReplayReport, ProbeError>` copies this body and adds the per-result checks over `payload["results"]`:
```rust
    let (result, call) = rpc.call("tools/call", classify_params(&[IDENTITY_PROBE_TEXT.to_string()])).await?;
    let payload = classify_payload(&result).map_err(|detail| ProbeError::Protocol { method: "tools/call", detail })?;
    let artifact_sha256 = payload["model"]["artifact_sha256"].as_str().unwrap_or("").to_string();
    let labels: Vec<String> = payload["labels"].as_array()
        .map(|a| a.iter().filter_map(|l| l.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let (tokens_total, _) = tokens_and_truncated(&payload);
    Ok(ProbeReport {
        probe_id: probe_id.to_string(),
        one_tool_named_classify,
        description_has_labels_in_order,
        response_labels_match: labels.as_slice() == expected_labels,
        identity_matches: !artifact_sha256.is_empty() && artifact_sha256 == expected_sha256,
        artifact_sha256, labels, tokens_total,
        calls: vec![init, list, call],
    })
```
**Report struct + `ok()` rule** (`src/probe.rs:85-116`): every boolean that is deploy evidence is ANDed in `ok()` — "a report field that is deploy evidence and is not in this AND would be a check the verdict ignores." A `ReplayReport::ok()` must AND all six replay checks.

**Replay texts:** a fixed, committed, synthetic (non-tweet) sample of 2..8 strings, as `IDENTITY_PROBE_TEXT` is a constant today; never tweet text (ASVS V8). Send the partner's two headers exactly (`content-type: application/json`, `accept: application/json, text/event-stream`, lambda README:98-100) — check `Rpc::new` already sets them before adding anything.

**Unit test to copy** (`src/probe.rs:796-823`, loopback fixture `tiny_model()` + `serve(Arc::clone(&model))`):
```rust
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn probe_fails_when_response_labels_differ() {
        let model = tiny_model();
        let sha = crate::tests::tiny_golden_sha256();
        let labels = model.task().owned_labels();
        let url = serve(Arc::clone(&model)).await;

        let report = run_identity_probe(&url, None, sha, &labels, &new_probe_id())
            .await
            .expect("identity probe completes");
        assert!(report.response_labels_match, "{report:?}");
        assert!(report.ok(), "{report:?}");

        let mut reordered = labels.clone();
        reordered.swap(0, 1);
        let report = run_identity_probe(&url, None, sha, &reordered, &new_probe_id()).await.expect("...");
        assert!(!report.response_labels_match, "{report:?}");
        assert!(!report.ok(), "{report:?}");
    }
```
The replay test: N texts in -> `one_result_per_text` true and every sum within 1e-5 against the tiny fixture; then a reordered label list -> `labels_in_task_order` false and `ok()` false.

---

### `contracts/decide-tool-boundary-v1.yaml` (modify two sites)

**Analog:** itself. The bound and its derivation comment (lines 284-306):
```yaml
  # 10 240 MB tier (v7.0.0): D-10's list maximum 8, reachable inside the budget because
  # 8 x 57 = 456 <= 800 (57 = the shortest built row of the served stance task). The superseded
  # 3 008 MB tier was floor(120 / 57) = 2.
  classify_max_texts: 8
  ...
  classify_max_total_tokens: 800
  ...
  # The SHORTEST built row of the served stance task (laya-stance-64.apr): task prefix + markers
  # with an empty state. Measured, not derived: `probe --plan-only --maximal distributed` plans 8
  # empty texts to 456 tokens. classify_max_texts x this <= classify_max_total_tokens, and the
  # maximal-request builder refuses to emit a DISTRIBUTED request the budget would refuse.
  served_task_min_row_tokens: 57
```
Edit: the value to the measured P of the approved `task.json`, the comment to name `laya-antisemitism-v1.apr` and the new `8 x P` figure, and the `8 x 57 = 456` comment above `classify_max_texts`. The constant is asserted by `cargo test -p aprender-mcp-decide --lib tests::max_texts_fit_the_budget_at_the_shortest_row` (listed in FALSIFY-007's `test:` line at 640-641).

**FALSIFY-DECIDE-TOOL-009** (lines 648-656) — the harness line names the served artifact and the evidence path; both change:
```yaml
  - id: FALSIFY-DECIDE-TOOL-009
    rule: 'The maximal legal classify completes under the API Gateway cap on a cold instance, for both shapes and every cold sample'
    prediction: 'At lambda_memory_mb 10240, every recorded cold sample of the CONCENTRATED (...) and DISTRIBUTED (8 texts, <= 800 built tokens) shapes, each proven cold by its load log line, completes under 30000 ms as the instance''s first POST.'
    implemented_by: 'plan 08-30, LIVE at 10 240 MB (...): deployed-passed, 8 proven-cold samples in the evidence file below (...)'
    test_harness: 'just laya-deploy-verify models/decide/laya-stance-64.apr aprender-mcp-decide'
    evidence: '.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-LIVE-REDEPLOY-EVIDENCE.json'
    if_fails: '...'
```
The `implemented_by` prose accretes ("SUPERSEDED TIER: ..."): append the Phase 10 deployment the same way rather than deleting the 08-30 record. Gate: `cargo run --release -p aprender-contracts-cli --bin pv -- validate contracts/decide-tool-boundary-v1.yaml`, then `git show HEAD:contracts/decide-tool-boundary-v1.yaml > <scratch>/old.yaml` and `pv diff <scratch>/old.yaml contracts/decide-tool-boundary-v1.yaml` for the semver suggestion (two filesystem paths).

---

### `crates/aprender-mcp-decide-lambda/README.md` (modify the "Deployed" table)

**Analog:** itself, lines 76-88:
```markdown
## Deployed

Live on pmcp.run as **`aprender-mcp-decide`** since 2026-09-27, on the 10,240 MB tier since
2026-09-28 (plan 08-30). The outcome is `deployed-passed`. The record is
`.planning/phases/08-.../08-LIVE-REDEPLOY-EVIDENCE.json`.

| | |
|---|---|
| Endpoint | `https://aprender-mcp-decide.us-east.true-mcp.com/mcp` (POST only) |
| Auth | Off, by the owner's decision (the cost risk was accepted). No token or header is needed |
| Tier | 10,240 MB, arm64 |
| Task | Abortion stance (TweetEval abortion target, "legalization of abortion"), labels `none`, `against`, `favor` |
| Model | `laya-stance-64.apr`, sha256 `24a44d7e050166c9b64e2716f2bcb3ce91747f7a3b927d03d6eeae5f89b6275a` |
```
Rewrite Task and Model rows for `antisemitism-v1` (labels `not_antisemitic`, `antisemitic`; the new sha), point the record at `10-LIVE-DEPLOY-EVIDENCE.json`, and state that the stance demo is no longer served (D-13). The example request at lines 96-104 uses an abortion tweet; replace the text with a synthetic sentence.

---

### `scripts/laya_claims.tsv` and `scripts/laya_gates.tsv` (ledgers)

**Claims row format** (laya_claims.tsv:1, 81, 111) — `claim_id, file, anchor (a literal substring that must exist), kind, owner, test, finding`:
```
claim_id	file	anchor	kind	owner	test	finding
tool.FALSIFY-DECIDE-TOOL-009.harness	contracts/decide-tool-boundary-v1.yaml	test_harness: 'just laya-deploy-verify models/decide/laya-stance-64.apr aprender-mcp-decide'	just	-	laya-deploy-verify	D-18 live harness (08-30)
claude-md.decide-row.tier	CLAUDE.md	The model is live on pmcp.run as `aprender-mcp-decide` on the 10,240 MB tier	evidence	-	.planning/phases/08-.../08-LIVE-REDEPLOY-EVIDENCE.json#live.final	D-18 (08-30)
```
Row 81's anchor must be updated in the same commit as the contract harness line, or `just laya-claims-check` fails; row 111 survives unchanged only if CLAUDE.md keeps that sentence verbatim. New sentences (README task/model rows, any CLAUDE.md addition) get new rows with an `evidence` test pointing at `10-LIVE-DEPLOY-EVIDENCE.json#live.final`.

**Gates row format** (laya_gates.tsv:1-6) — `gate_id, target, must_fail, must_pass, finding`; each gate ships a must-fail AND a must-pass case:
```
gate_id	target	must_fail	must_pass	finding
sha256-helper	just laya-deploy-config	laya-deploy on a config whose pin was mutated, foreign rtk first on PATH -> REFUSED sha-pin	foreign rtk first on PATH, and rtk absent from PATH: laya-deploy-config pins exactly the digest shasum -a 256 prints	V11-c
```
The new row: `prefix-budget`, target `prepare_goldstandard.py --measure`, must_fail = a task whose P x 8 > 800 is refused, must_pass = the approved task prints P with markers == 2 and no cut flag.

---

### `justfile` (new recipes)

**Analog for `laya-prepare-goldstandard`:** `justfile:1305-1317` (`laya-prepare-stance`) — bash shebang, `set -euo pipefail`, a presence check with the fetch hint, then the `uv run --frozen` invocation:
```just
laya-prepare-stance cell="s64":
    #!/usr/bin/env bash
    set -euo pipefail
    need="data/tweet-eval-stance/train.jsonl data/tweet-eval-stance/test.jsonl"
    if [ "{{cell}}" = "s64" ]; then need="$need data/tweet-eval-stance/validation.jsonl"; fi
    for f in $need; do
        test -f "$f" || {
            echo "ERROR: $f is missing — fetch the local dataset first:" >&2
            echo "       apr data tweet-eval-stance --output data/tweet-eval-stance" >&2
            exit 1
        }
    done
    uv run --project scripts/laya_train --frozen python scripts/laya_train/prepare_stance.py --cell "{{cell}}"
```
Parameter: `csv="/Users/guy/projects/mcp/motivation-experiment/datasets/antisemite/GoldStandard2024.csv"`; the missing-file message says where the partner CSV lives.

**Analog for refusals and rc capture inside a recipe:** `justfile:2384-2412` (`laya-upload`) — `refuse()` helper with a rule name and an exit code, `set +e; just ...; vrc=$?; set -e`, `shasum -a 256` directly (never through rtk), a `DRY_RUN=1` branch before any AWS call:
```just
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
    ...
    set +e
    just laya-verify "$APR" "{{run}}" "{{data}}" "{{base}}" > "$VLOG" 2>&1
    vrc=$?
    set -e
    if [ "$vrc" -ne 0 ]; then
        refuse eligibility "$(grep -m 1 '^REFUSED' "$VLOG" || echo "laya-verify exited $vrc (log: $VLOG)")" "$vrc"
    fi
    ...
    if [ "${DRY_RUN:-0}" = "1" ]; then
        echo "DRY-RUN: would upload $APR ($SIZE bytes) to s3://aprender-decide-weights-<account>-$ENV/$KEY"
        exit 0
    fi
```
`laya-probe-epoch` captures the probe's rc to a file the same way and refuses to run if `models/decide/laya-antisemitism-v1` already exists (so the probe never follows a declared run).

**Analog for building and locating the probe example:** `justfile:2612-2619` (`laya-deploy-verify`):
```just
    cargo build --release -q -p aprender-mcp-decide-lambda --example probe
    TD="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
    PROBE="$TD/release/examples/probe"
    for shape in concentrated distributed; do
        "$PROBE" --plan-only --apr "$APR" --maximal "$shape" > "models/decide/plan-$shape.json" \
            || { echo "ERROR: cannot build the $shape maximal request for $APR" >&2; exit 1; }
    done
```
The acceptance replay step invokes `"$PROBE" --url <endpoint> --apr <apr> --expect-sha256 <H> --replay <texts.json>` the same way and copies the JSON line into `10-ACCEPTANCE-REPLAY.json`.

---

### Phase evidence records (`10-*-EVIDENCE.json`, `10-ACCEPTANCE-REPLAY.json`, `10-HANDOFF.md`)

**Analog for the gate run / pack record:** `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-GATE-RUN-EVIDENCE.json`. Top-level keys (shape dumped this session):
```
outcome, decided_by, human_option, declaration_commits{...}, code_commits{...}, run_commit, recipe_id,
run_dir, data_dir, data_sha256{task_json, train_jsonl, eval_jsonl, shift_jsonl},
seeds{policy, shipped, per_seed[{seed, macro_f1, ece_post, margin, t_applied, pass, rank_key,
                                 model_safetensors_sha256, eval_probs_sha256}]},
gate{pass, zero_shot_macro_f1, ft_macro_f1, margin, ece_post, t_applied, clamp_hit,
     thresholds{min_macro_f1_margin, max_ece, ece_bins}, rust_recomputed{ece_post, ft_macro_f1, margin, zs_macro_f1}},
noise{fine_tuned{max_abs, bound, argmax_agree, n}, zero_shot{...}},
pack{exit, rescore_max_abs, zs_rescore_max_abs, rescore_bound, zs_rescore_bound, argmax},
artifact{path, sha256, bytes}, deploy_eligible, e2e_stdio{passed, identity_sha256},
shift_probe{gate_clause, n, ...}, device_used, torch_version, wall_seconds
```
`10-GATE-RUN-EVIDENCE.json` copies this schema; `shift_jsonl` / `shift_probe` become `null` with a note that D-04's live set is the slot. Add `probe_evidence: "10-PROBE-EVIDENCE.json"`, `epochs_declared` and `epochs_reasoning`, and `task_json_sha256` beside `data_sha256.task_json` (D-05/D-06).

**Analog for the deploy record:** `08-LIVE-REDEPLOY-EVIDENCE.json` top-level keys:
```
schema ("laya-live-redeploy-v1"), server, artifact_sha256,
offline_dry_run{ok, commit, command, rc, wall_s, run_on, positive_leg, lines[], aws_calls},
decision{choice, date, decided_by, orchestrator_stop_guard},
live_attempts[{attempt, started, ended, commit, contract, config, command, rc, passed_before_refusal[],
               refusal, refused_by, crates_root, live_function_after, aws_writes_to_the_function, cold_samples, fallback}],
live{tier_mb, contract, what,
     cold_samples[{attempt, sample, shape, tokens_total, elapsed_ms, proven_cold, artifact_sha256,
                   download_ms, sha_ms, build_ms, load_ms, classify_ms, gateway_overhead_ms, probe_id}],
     cold_wall_ms{min, max, worst_sample, worst_slack_to_cap_ms}, warm_ms{...}, accepted_region_cold, final}
```
`10-LIVE-DEPLOY-EVIDENCE.json` keeps `schema`, `server`, `artifact_sha256`, `offline_dry_run`, `decision` (D-13: replace / auth off, dated, decided_by owner), `resolver_proof{cargo_pmcp_version, sdk_commit, proven_at}`, `live_attempts[]`, `live{}` with timings REPORTED (D-16: no gate clause other than the recipe's own 30 s containment). Account ids scrubbed, as 08-30 did.

**Probe / SetFit / replay records** have no Phase 8 file; use the key lists in RESEARCH.md Q3 (`10-PROBE-EVIDENCE.json`), Q6 step 4 (`10-SETFIT-CELL-EVIDENCE.json`, with the mandatory `label: "same eval split, subsampled train ..."` and `cell: "s64e1b16"`) and Q8 (`10-ACCEPTANCE-REPLAY.json` = the probe's replay JSON line plus endpoint, auth posture, date, commit). Every number names the command or file it came from (CLAUDE.md "Derive the numbers").

## Shared Patterns

### Refusal is a result; never move a threshold inline
**Source:** `scripts/laya_train/train.py:93` (`EXIT_PASS, EXIT_REFUSED, EXIT_GATE_FAIL = 0, 2, 3`), `prepare_stance.py:189-194` (`DataError` -> `REFUSED <rule>`, exit 2), `justfile:2384-2412` (`refuse()` helper).
**Apply to:** prepare script, probe driver, SetFit cell, every justfile recipe, pack/verify steps. Capture `rc` from a file or `|| rc=$?`, never through a pipe; a 3 from `laya-train` or a `REFUSED` from `pack` stops at an owner checkpoint (D-09).

### Write once, declare before running
**Source:** `prepare_stance.py:161-178` (`write_once`), `train.py:762-763` (`REFUSED out-dir`), `data_tweeteval.rs:775-800` (`write_outputs` refuses existing files without `--force`).
**Apply to:** the data dir, the adapted SetFit dir, the run dir, the evidence files (append new keys; never rewrite a recorded attempt).

### No tweet text in git, stdout or logs
**Source:** `prepare_stance.py:26, 195-203` (counts and sha256s only), lambda README:56-57 (server logs no request text), `examples/probe.rs:27, 76-78` (bearer never printed; bad args reported by position).
**Apply to:** prepare script output, `--measure` output (percentiles only), probe `--replay` texts (synthetic, committed), every `10-*` record, the README example request.

### Pinned binaries and the served path
**Source:** `scripts/apr_bin.sh` (CLAUDE.md Step 0) for `apr`; `justfile:2612-2614` for the probe example via `cargo metadata` target dir; `train.py:314` `DEVICE ... used=` as the only admissible device claim.
**Apply to:** the SetFit cell (`"$APR"` built with `--features setfit`), the replay, the probe driver's evidence (`device_used` from `info`, never from the request).

### Contract edits go through `pv`, with the claims ledger in the same commit
**Source:** CLAUDE.md "Contract Validation"; `scripts/laya_claims.tsv:81` pins the FALSIFY-009 harness string; `just laya-claims-check` (justfile:3434).
**Apply to:** `contracts/decide-tool-boundary-v1.yaml`, README "Deployed" table, any CLAUDE.md sentence about the live model. `pv` is not on PATH: `cargo run --release -p aprender-contracts-cli --bin pv -- validate ...`.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `prepare_goldstandard.py --measure` (tokenizer-only prefix measurement) | utility | transform | No in-tree Python measures `P`; the method is the RESEARCH.md Code Example (`laya.common.build_sequence`), controlled against the contract's 57, cross-checked by `probe --plan-only` (probe.rs:174-188) once the `.apr` exists |
| Two-class attested canonical dataset | data | file-I/O | `data_tweeteval.rs` is the only writer and is three-class/TweetEval-bound; the public doors are label-count generic but nothing two-label has passed them (RESEARCH Q6, Pitfall 6) — the Wave 0 round-trip test is the proof |

## Metadata

**Analog search scope:** `scripts/laya_train/`, `.planning/spikes/027-*/`, `crates/aprender-mcp-decide-lambda/{examples,src}`, `crates/apr-cli/{examples,src/commands}`, `crates/aprender-contrastive-data/src/`, `contracts/decide-tool-boundary-v1.yaml`, `justfile` (laya-* recipes), `scripts/{laya_claims,laya_gates}.tsv`, `scripts/run_bench_cells.sh`, Phase 8 evidence JSONs (key shapes only).
**Files scanned:** 16 tracked sources (all verified with `git ls-files`), 2 evidence JSONs (shape-dumped with `python3 -I`, no values copied).
**Not run (read-only session):** training, `just`, AWS, `pmat embed sync`, the partner repo beyond what RESEARCH.md already cites.
**Pattern extraction date:** 2026-10-21
