---
phase: 05-benchmark-and-claims-gate
reviewed: 2026-09-08T00:00:00Z
depth: standard
files_reviewed: 52
files_reviewed_list:
  - crates/apr-cli/Cargo.toml
  - crates/apr-cli/src/commands/data_tweeteval.rs
  - crates/apr-cli/src/commands/eval/setfit_tests.rs
  - crates/apr-cli/src/commands/finetune_display_tests.rs
  - crates/apr-cli/src/commands/finetune_selection_tests.rs
  - crates/apr-cli/src/commands/finetune_tests.rs
  - crates/apr-cli/src/commands/finetune.rs
  - crates/apr-cli/src/commands/mod.rs
  - crates/apr-cli/src/commands/output_verification.rs
  - crates/apr-cli/src/commands/predict_tests.rs
  - crates/apr-cli/src/commands/setfit_bench_tests.rs
  - crates/apr-cli/src/commands/setfit_bench.rs
  - crates/apr-cli/src/commands/setfit_train.rs
  - crates/apr-cli/src/dispatch_analysis.rs
  - crates/apr-cli/src/dispatch.rs
  - crates/apr-cli/src/model_ops_commands.rs
  - crates/apr-cli/src/setfit_commands.rs
  - crates/apr-cli/tests/setfit_cli_lifecycle.rs
  - crates/apr-cli/tests/setfit_parity.rs
  - crates/aprender-core/src/calibration_tests.rs
  - crates/aprender-core/src/calibration.rs
  - crates/aprender-core/src/error.rs
  - crates/aprender-core/src/generated_contracts.rs
  - crates/aprender-core/src/stats/hypothesis.rs
  - crates/aprender-core/src/stats/mod.rs
  - crates/aprender-core/src/stats/tests_claims_stats.rs
  - crates/aprender-train/src/eval/classification/metrics.rs
  - crates/aprender-train/src/finetune/classify_pipeline/mod.rs
  - crates/aprender-train/src/finetune/classify_pipeline/training.rs
  - crates/aprender-train/src/finetune/classify_reload_tests.rs
  - crates/aprender-train/src/finetune/classify_trainer_tests.rs
  - crates/aprender-train/src/finetune/classify_trainer.rs
  - crates/aprender-train/src/finetune/mod.rs
  - crates/aprender-train/src/train/setfit/apr_evaluate_row_tests.rs
  - crates/aprender-train/src/train/setfit/apr_evaluate_tests.rs
  - crates/aprender-train/src/train/setfit/apr_evaluate.rs
  - crates/aprender-train/src/train/setfit/bench_gate_tests.rs
  - crates/aprender-train/src/train/setfit/bench_gate.rs
  - crates/aprender-train/src/train/setfit/bench_metrics_tests.rs
  - crates/aprender-train/src/train/setfit/bench_metrics.rs
  - crates/aprender-train/src/train/setfit/bench_row_tests.rs
  - crates/aprender-train/src/train/setfit/bench_row.rs
  - crates/aprender-train/src/train/setfit/evidence.rs
  - crates/aprender-train/src/train/setfit/mod.rs
  - crates/aprender-train/src/train/setfit/thresholds.rs
  - crates/aprender-train/src/train/setfit/tune.rs
  - crates/aprender-train/src/train/setfit/verify.rs
  - crates/aprender-train/src/transformer/model.rs
  - crates/aprender-train/tests/setfit_apr_lifecycle.rs
  - Makefile
  - scripts/run_bench_cells.sh
  - scripts/setfit_fixtures/gen_claims_fixtures.py
findings:
  critical: 2
  warning: 16
  info: 4
  total: 22
status: issues_found
---

# Phase 5: Code Review Report

**Reviewed:** 2026-09-08
**Depth:** standard
**Files Reviewed:** 52
**Status:** issues_found

## Summary

Phase 5 ships a falsifiable claims gate (`bench_row` / `bench_gate` / `bench_metrics`), an
f64 claims-statistics surface in `aprender-core::stats::hypothesis`, two new multiclass
calibration metrics, a 40-cell sweep driver, and a large Makefile hardening pass. The
typestate design (`VerifiedRunSet` with no public constructor) and the fixture-pinned f64
statistics are genuinely well built, and the numeric closed forms check out against the
frozen scipy fixtures.

Two defects reach BLOCKER. The first is the Makefile hardening itself: the new
`.SHELLFLAGS := -o pipefail -c` is dead — a pre-existing `.SHELLFLAGS := -e -c` 28 lines
later overwrites it, so none of the 14 laundering pipes the change was written to close are
actually closed. This was verified by running Make, not by reading. The second is a
path-traversal in the gate's provenance recomputation: the "committed lock / ledger bytes"
a row names are joined to the bench directory without any check that the path is relative,
so an absolute or `..`-bearing path in an untrusted transported row resolves outside the
benchmark directory entirely — defeating the exact invariant the module documents.

Both are of the class CLAUDE.md's Verification Discipline section names: a guard that
cannot fire, and a claim about a file that is not the file it claims.

## Narrative Findings (AI reviewer)

## Critical Issues

### CR-01: The new `.SHELLFLAGS` pipefail hardening is overwritten and never takes effect

**File:** `Makefile:29` (overwritten by `Makefile:57`)
**Issue:**
This phase added, with a 17-line justification block:

```make
# Measured on this Makefile before the change: 577 recipe lines, 14 with a pipe.
# The worst was the release gate itself, `contracts:` -> `pv lint contracts/ 2>&1
# | tail -5`, which could never fail the build no matter what pv reported.
.SHELLFLAGS := -o pipefail -c
```

Line 57 of the *same file* still carries the pre-existing `.SHELLFLAGS := -e -c`. `:=` is a
plain assignment and the last one in the makefile wins, so the value in force for every
recipe is `-e -c`. `pipefail` is not set anywhere. Measured, not inferred:

```
$ cat > /tmp/probe.mk <<'EOF'
include Makefile
shellflags-probe:
	@echo "flags=[$(.SHELLFLAGS)]"
	@false | true
	@echo "pipefail-did-not-fire"
EOF
$ make -f /tmp/probe.mk shellflags-probe; echo "rc=$?"
flags=[-e -c]
pipefail-did-not-fire
rc=0
```

Every consequence claimed for the change is false. `contracts:` was rewritten to
`@. scripts/pv_bin.sh && "$$PV" lint contracts/ 2>&1 | tail -5` (Makefile:1043) and still
reports `tail`'s status. This is precisely the #2336/#2360 defect class the comment block
cites, reintroduced by the fix for it.

Compounding it, the two comment blocks now contradict each other on the record: lines 21-28
argue `-e` was *deliberately excluded* ("248 recipe lines use `;` chains (-e would abort
them mid-recipe)"), while lines 49-54 argue `-e` is *deliberately the only flag set* and
`-o pipefail` was excluded for SIGPIPE reasons. A reader cannot tell which policy the file
is under.

**Fix:** Delete the duplicate and set one value, then re-run the probe above to confirm
`false | true` exits non-zero:

```make
# Makefile:57 — single site
.SHELLFLAGS := -e -o pipefail -c
```

If the blast-radius argument against combining them stands, then delete line 29 and its
comment block outright rather than leaving a dead assignment that documents a property the
build does not have. Either way, re-measure the `contracts:`/`coverage:`/`contract-test:`
pipes with a deliberately-failing producer before claiming the class is closed
(CLAUDE.md Verification Discipline rules 1 and 8).

---

### CR-02: Row-controlled provenance paths escape the benchmark directory (path traversal / arbitrary read)

**File:** `crates/aprender-train/src/train/setfit/bench_gate.rs:963-969` and `:993-999`
**Issue:**
`verify_provenance` resolves the file a row names by:

```rust
let relative = PathBuf::from(&evidence.lock.lock_record_path);
let path = bench_dir.join(&relative);
let bytes = read_evidence(cell, &path)?;
```

Nothing validates `lock_record_path` (or `candidate_ledger_path`). `bench_row.rs:396-409`
documents them as "RELATIVE to the benchmark directory" but only declares them as `String`
with `deny_unknown_fields`; there is no `is_absolute` check, no `Component::ParentDir`
rejection, and no canonicalise-and-contain check anywhere in `bench_row.rs` or
`bench_gate.rs`.

`Path::join` with an absolute argument **discards the base**. A row carrying
`"lock_record_path": "/tmp/attacker/lock.json"` makes the gate read `/tmp/attacker/lock.json`;
`"../../../elsewhere/lock.json"` escapes upward. Both then hash normally and both **pass**,
because the only assertion is `sha256(bytes) == row.lock_hash`.

This is not a theoretical input. The `--record` transport mode
(`crates/apr-cli/src/commands/setfit_bench.rs:913`) exists specifically to ingest a row
file *executed on a different host*, and validates only digest, schema, cell identity and
filename — never the embedded paths. The module header states the threat model explicitly
("A row's `lock_hash` and `candidate_ledger_sha256` are CLAIMS ABOUT FILES", "Provenance is
recomputed, never trusted"), so the row is by construction untrusted input. The property
that "selection safety is recomputed from **committed** bytes" silently becomes "recomputed
from *some* bytes somewhere on this filesystem".

Secondary impact: an arbitrary-file-read primitive (bounded at 16 MiB) driven by attacker
data, and `EvidenceReadFailed`/`ProvenanceMismatch` messages that echo the resolved path.

**Fix:** Reject non-relative and escaping paths before the join, in `bench_row`'s
`from_bytes` (so the refusal is a schema refusal, at the earliest door) and defensively in
`verify_provenance`:

```rust
fn contained_evidence_path(raw: &str) -> Result<PathBuf, BenchRowError> {
    let p = Path::new(raw);
    if p.is_absolute() {
        return Err(BenchRowError::EvidencePathEscapes { path: raw.to_string(),
            detail: "must be relative to the benchmark directory".into() });
    }
    for component in p.components() {
        match component {
            Component::Normal(_) => {}
            _ => return Err(BenchRowError::EvidencePathEscapes { path: raw.to_string(),
                     detail: "must contain no `..`, `.`, root or prefix component".into() }),
        }
    }
    Ok(p.to_path_buf())
}
```

Then add the two negatives to `bench_gate_tests.rs`'s doctored-negative table (an absolute
path and a `..` path), because the existing six negatives prove nothing about this route.

---

## Warnings

### WR-01: `is_evidence_failure` misclassifies real evidence failures as transient, and has no case table

**File:** `scripts/run_bench_cells.sh:289-295`
**Issue:**

```bash
is_evidence_failure() {
    log="$1"
    if grep -qE 'UncalibratedRegime|evidence table|threshold|selection lock|attestation|digest mismatch|already recorded|outside the contracted matrix|candidate' "$log"; then
```

The classifier decides whether the operator is told `HALT: evidence-class failure ...
Re-running will not fix it` or `RESUMING IS SAFE`. Several refusals this phase itself
introduced contain none of these tokens, so they land on the *dangerous* side:

- `BenchMetricsError::LabelOrderMismatch` — "the declared label order ... is not ...".
- `BenchMetricsError::WrongSplit` — "requires evidence from the `validation` split".
- `BenchMetricsError::EmptySplit`, `ClassIndexOutsideLabelMap`.
- `SetFitTrainError::SelectionLabelOutOfRange`, `AprEvaluateError::LabelMapMismatch`.
- `BenchGateError::RowSlotMismatch` — "the manifest slot and the row payload ...".

Each of those is exactly the "no re-run can fix it" class, and each is reported as
transient. Symmetrically, `threshold` and `candidate` are generic enough to fire on
unrelated infrastructure output.

CLAUDE.md Verification Discipline rule 7 requires guard regexes to ship a must-match /
must-not-match case table. There is none: `setfit_bench_tests.rs:1542` only asserts that
the string `is_evidence_failure` appears in the script, and
`driver_holds_a_single_writer_lock_with_distinct_failure_exit_codes` only greps for
`UncalibratedRegime`. Both are source-string matches; neither exercises the classifier.

**Fix:** Invert the default — classify as **evidence** unless a known-transient signature
matches (ENOSPC, "Killed", ssh/network, "Resource temporarily unavailable") — and ship a
case table:

```bash
# scripts/lib/bench_failure_cases.txt: one `expect<TAB>fixture-line` per row
#   evidence   the declared label order ["a"] is not `test_rows`'s own ["b"]
#   evidence   cell setfit/s8/seed13: ... carries a payload for setfit/s8/seed17
#   transient  No space left on device (os error 28)
```
and a `--self-test` mode in the driver that runs the table and exits non-zero on any
disagreement, wired into `setfit-bench-tests`.

### WR-02: `assemble_quality_block` can panic instead of returning its typed refusal

**File:** `crates/aprender-core/src/calibration.rs:195-230`, reached from
`crates/aprender-train/src/train/setfit/bench_metrics.rs:229-234`
**Issue:** `multiclass_rows` enforces its preconditions with hard `assert!`, including
`assert!((sum - 1.0).abs() < 1e-3, ...)`. `assemble_quality_block` returns
`Result<QualityBlock, BenchMetricsError>` and its own error docs say a row of NaNs "is worse
than a missing row" — but a NaN probability, or an f64→f32 narrowing that pushes a row's sum
outside 1e-3, aborts the process instead of producing that refusal. The narrowing at
`bench_metrics.rs:225-229` (`row.iter().map(|&p| p as f32)`) is precisely where such a row
can appear, and the call is inside a 40-cell sweep whose driver classifies a crash as
transient (see WR-01).

**Fix:** Add fallible mirrors used by the claims path and keep the `assert!` forms for the
contract-macro entry points:

```rust
pub fn try_expected_calibration_error_top_label(
    probabilities: &[f32], n_classes: usize, labels: &[usize], n_bins: usize,
) -> Result<f32, CalibrationError> { ... }
```
and map `CalibrationError` into a new `BenchMetricsError::DegenerateProbabilities` variant.

### WR-03: The claims-layer moments accept NaN, so the "never a non-finite f64" guarantee does not hold

**File:** `crates/aprender-core/src/stats/hypothesis.rs:330-352`
**Issue:** `moments_or_zero_variance` guards two degenerate shapes but not non-finite input.
With any NaN in `values`: `values.iter().all(|&v| v == first)` is false, `mean` is NaN,
`variance` is NaN, `std == 0.0` is false — so it returns `Ok((NaN, NaN))`. `ttest_1samp_f64`
then yields `statistic: NaN, pvalue: NaN`, and `paired_ci` yields NaN `low`/`high`/
`half_width`. `bench_gate::seed_dispersion_ci95` takes the `Ok` arm and emits
`Some(NaN)` for every bound, which `serde_json` serialises as `null` — the exact Ph3 CR-03
outcome the doc comment on this function and on `Ci95` claims is unreachable.

**Fix:** Reject non-finite input at the same door:

```rust
if let Some((i, bad)) = values.iter().copied().enumerate().find(|(_, v)| !v.is_finite()) {
    return Err(AprenderError::NonFiniteObservation { index: i, value: bad });
}
```
and add a fixture case (`kind: "non_finite_input"`, `expect: "NonFiniteObservation"`) to
`seed_dispersion_ci_cases.json` so the refusal is asserted rather than assumed.

### WR-04: `mechanism_classes` compares order-and-multiplicity, producing spurious asymmetry notes

**File:** `crates/aprender-train/src/train/setfit/bench_gate.rs:1533-1538`, consumed at
`crates/apr-cli/src/commands/setfit_bench.rs:2676-2680`
**Issue:**

```rust
fn mechanism_classes(mechanisms: &[String]) -> Vec<MechanismClass> {
    let mut out: Vec<MechanismClass> = mechanisms.iter().map(|m| mechanism_class(m.as_str())).collect();
    out.dedup();   // only removes CONSECUTIVE duplicates
    out
}
```

The input is `distinct_sorted(...)`, i.e. sorted by *mechanism string*, not by class. Classes
therefore interleave and `dedup()` does not produce a set. With
`["child_max_rss_time_l", "sysinfo_sampled_20", "vm_hwm"]` the result is
`[Exact, Sampled, Exact]`. The renderer then does

```rust
if group.train_peak_rss_mechanism_classes != group.inference_peak_rss_mechanism_classes {
    out.push_str(&format!("  ^ {WITHIN_ROW_ASYMMETRY_NOTE}\n"));
}
```

so `[Exact, Sampled, Exact]` vs `[Sampled, Exact]` — the same class *set* — reports an
asymmetry that does not exist. `class_tags` also renders the duplicate
(`exact_kernel_high_water_mark, sampled_lower_bound, exact_kernel_high_water_mark`) into the
published table.

**Fix:** Make it a set with a deterministic order:

```rust
fn mechanism_classes(mechanisms: &[String]) -> Vec<MechanismClass> {
    let mut out: Vec<MechanismClass> = mechanisms.iter().map(|m| mechanism_class(m)).collect();
    out.sort_unstable_by_key(|c| c.tag());
    out.dedup();
    out
}
```

### WR-05: `--val-split NaN` bypasses the range guard and silently disables validation

**File:** `crates/aprender-train/src/finetune/classify_trainer.rs:197-201`,
`crates/apr-cli/src/model_ops_commands.rs` (`val_split: Option<f32>`)
**Issue:** `if config.val_split < 0.0 || config.val_split > 0.5` is false for NaN, so NaN
passes. `split_dataset` then computes `((len as f32) * NaN).ceil() as usize` → `0`, clamped
by `.max(1)` to a one-row validation set; `let validation_requested = config.val_split > 0.0`
is false, so the run proceeds as if validation were disabled while one row has been removed
from training. No refusal, no warning.

The new clap flag has no `value_parser` range check. This phase fixed the identical NaN-guard
class for `--threshold` in `dispatch.rs:564-568` via
`commands::threshold_arg::guard_f32` (GH-2391, "against a NaN threshold every term is false"),
and did not apply it to the flag it introduced.

**Fix:**

```rust
if !config.val_split.is_finite() || !(0.0..=0.5).contains(&config.val_split) {
    return Err(crate::Error::ConfigError(format!(
        "SSC-026: val_split must be a finite value in [0.0, 0.5] (0.0 disables validation), got {}",
        config.val_split)));
}
```
plus `commands::threshold_arg::guard_f32("--val-split", val_split, ..)` at the dispatch door,
so the refusal comes before the 9B base model is loaded.

### WR-06: A missing lock or ledger file is reported as a missing *row* file

**File:** `crates/aprender-train/src/train/setfit/bench_gate.rs:524-531`, reached from
`verify_provenance` at `:965` and `:995`
**Issue:** `read_evidence` maps `ErrorKind::NotFound` to `BenchGateError::RowFileMissing`
unconditionally. It is called for three different file kinds. When the *lock record* is
absent, the operator gets:

> cell setfit/s8/seed13 is recorded complete in the manifest, but
> `<bench>/locks/setfit-s8-seed13.lock.json` does not exist. A recorded digest with no bytes
> behind it is an omission the manifest cannot see; re-run the cell or **restore the row file**

The path named is a lock; the remedy names a row; `variant_tag()` reports
`row_file_missing`, so any downstream triage keyed on the tag is wrong too. The module's own
stated bar is that "which cell" and "which file" are what a reader needs to re-run.

**Fix:** Pass the file kind into `read_evidence` and mint an `EvidenceFileMissing { cell,
kind, path }` variant, or route the provenance calls through a wrapper that remaps
`RowFileMissing` to `ProvenanceMismatch`/`EvidenceReadFailed` with the correct remedy.

### WR-07: `AprenderError` is not `#[non_exhaustive]`; the new variant is a semver break

**File:** `crates/aprender-core/src/error.rs:23-24`, new variant at `:111-127`
**Issue:** `pub enum AprenderError` carries only `#[derive(Debug)]`. Adding
`ZeroVarianceDifferences` is a breaking change for any downstream crate that matches it
exhaustively — and `aprender` is published to crates.io. The sibling `AprFormatError` in the
same file *is* `#[non_exhaustive]` (see the comment at `:246`), so the convention exists and
was not followed for the parent.

**Fix:** Add `#[non_exhaustive]` to `AprenderError` in the same change that adds the variant,
and note the minor-version bump. If exhaustive matching inside the workspace depends on it,
add the `_ =>` arms now rather than after publish.

### WR-08: The driver's resume check scans the whole manifest for the digest, not this cell's entry

**File:** `scripts/run_bench_cells.sh:274-283`
**Issue:** After extracting `row_hash` from the row file, the check is:

```bash
while IFS= read -r line; do
    case "$line" in
        *"$row_hash"*) return 0 ;;
```

Any line of `run-manifest.json` containing the digest as a substring satisfies it. The
comment above it claims "the manifest must record exactly that digest **for this cell**",
which is not what the code checks — a digest recorded against a *different* cell would
cause this cell to be skipped as complete. The stated guarantee ("it is the collision
`RunManifest::record` refuses, and re-running is how the operator sees it") is therefore not
delivered by this predicate.

**Fix:** Delegate to the tool that already knows the answer instead of re-implementing a
manifest reader in shell (CLAUDE.md: dogfood the in-tree CLI):

```bash
cell_is_complete() {
    "$APR" setfit bench verify-cell --bench-dir "$BENCH_DIR" \
        --method "$1" --shots "$2" --seed "$3" >/dev/null 2>&1
}
```
`verify-cell` was built in this same phase for exactly this scope (steps 1+4+6 over one
declared cell) and checks the *cell's own* manifest entry.

### WR-09: Documented cell counts contradict `EXPECTED_CELLS`

**File:** `crates/aprender-train/src/train/setfit/bench_gate.rs:244`, `:492`, `:1399`;
`crates/apr-cli/src/commands/setfit_bench.rs:2452`
**Issue:** `EXPECTED_CELLS = ACTIVE_METHODS.len() * BENCH_SHOTS.len() * BENCH_SEEDS.len()`
= 1 × 4 × 10 = **40** (`bench_row.rs:109`). Four live doc comments still say 80:

- `:244` "The manifest's cell sequence is not exactly the contract-derived **80** in contract order."
- `:492` "**Eighty rows** that passed every rule of `setfit-benchmark-claims-v1`"
- `:1399` "Unreachable through verify_run, which proved **all 80 cells** present."
- `setfit_bench.rs:2452` "would be a report over **eighty** pending cells"

The module header at `:21` correctly says "the contract-derived ACTIVE 40". In a phase whose
whole subject is that published counts must be derivable, four stale ones in the gate's own
rustdoc is a real defect, not a typo.

**Fix:** Replace the literals with the constant in the doc text
(`the contract-derived [`EXPECTED_CELLS`]`), and add the file to whatever the phase uses as
its drift gate so the next narrowing goes red instead of quiet.

### WR-10: `verify_cell` reports `declared: 0` for a manifest that declares 40

**File:** `crates/aprender-train/src/train/setfit/bench_gate.rs:857-862`
**Issue:**

```rust
let Some(entry) = manifest.payload.cells.iter().find(|e| e.cell() == cell) else {
    return Err(BenchGateError::ExpectationSetMismatch { declared: 0, expected: EXPECTED_CELLS });
};
```

`declared` is documented as "How many cells the manifest declares". The refusal renders as
"the run manifest declares **0** cells, but setfit-benchmark-claims-v1 derives 40" for a
manifest that declares all 40 and simply does not contain the *requested* cell. A false
number inside a refusal is the same failure mode this phase spent its budget eliminating
from published numbers.

**Fix:** Either report the true count (`manifest.payload.cells.len()`) or, better, mint a
`CellNotDeclared { cell }` variant whose message says what actually happened.

### WR-11: The sweep driver hardcodes `40` instead of deriving it from the matrix arrays

**File:** `scripts/run_bench_cells.sh:385-395`
**Issue:** The arrays `SHOTS` and `SEEDS` are the contracted matrix, but the vacuity floor
and the DONE line both hardcode `40`:

```bash
printf 'DONE %s: %s executed, %s skipped, %s of 40 cells covered\n' ...
if [ "$total" -ne 40 ]; then
```

The comment above it argues that "the count is asserted, not reported" — but the assertion
is against a literal, so an edit to `SHOTS`/`SEEDS` makes the *guard* the thing that lies.
`EXPECTED_CELLS` is derived in Rust for exactly this reason.

**Fix:**

```bash
EXPECTED_CELLS=$(( ${#SHOTS[@]} * ${#SEEDS[@]} ))
readonly EXPECTED_CELLS
...
if [ "$total" -ne "$EXPECTED_CELLS" ]; then
```

### WR-12: `aggregate` publishes `key_sequence` entries for cells it then skips

**File:** `crates/aprender-train/src/train/setfit/bench_gate.rs:1393-1402`
**Issue:** `key_sequence.push(cell.render())` runs before the `let Some(row) = by_cell.get(&cell)
else { continue; }`, so a skipped cell still appears in the published `key_sequence`. The
comment justifies the skip as "Skipped rather than panicked so a future in-crate caller gets
a short series and a loud `expect` below rather than an abort here" — but `summarise` calls
`mean_f64(...).expect(SERIES_INVARIANT)`, which *is* an abort, and for an empty series it
aborts before anything is reported. The escape hatch described does not exist; the only
outcome of the `continue` is a `key_sequence` that names a cell with no data, followed by a
panic.

**Fix:** Push the key only after the row is resolved, and replace the `continue` with an
explicit `unreachable!`-free typed path or an honest `expect` at the lookup:

```rust
let Some(row) = by_cell.get(&cell) else { continue };
key_sequence.push(cell.render());
```

### WR-13: `TrainRssSampler` leaks its polling thread when `finish()` is not reached

**File:** `crates/apr-cli/src/commands/setfit_bench.rs:524-614`
**Issue:** On non-Linux the constructor spawns a thread that polls `sysinfo` at
`SAMPLE_TARGET_HZ` until `stop` is set. `stop` is only set in `finish(mut self)`. Any `?`
between `start()` and `finish()` in the training path drops the sampler, and `Drop` is not
implemented — the thread runs for the remainder of the process, contending for CPU with the
very measurement the module exists to keep clean, and holding an `Arc` alive.

**Fix:**

```rust
#[cfg(not(target_os = "linux"))]
impl Drop for TrainRssSampler {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(handle) = self.handle.take() { let _ = handle.join(); }
    }
}
```
(and let `finish` take the handle before the drop runs).

### WR-14: `make audit` writes to fixed, world-writable `/tmp` paths

**File:** `Makefile:1197-1215`
**Issue:**

```make
@cargo tree --duplicates > /tmp/apr-dup.txt 2>&1; \
@cargo audit > /tmp/apr-audit.txt 2>&1; rc=$$?; \
```

Predictable paths in a shared directory. A pre-planted symlink at `/tmp/apr-dup.txt` is
followed by `>` and truncates/overwrites the target with the caller's privileges. The rest
of the Makefile already uses `target/` for scratch output (`target/contract-audit-phase5-*.log`).

**Fix:** `@mkdir -p target && cargo tree --duplicates > target/apr-dup.txt 2>&1; ...`
(or `mktemp` if `/tmp` is required).

### WR-15: `throughput_rows_per_sec` reports `0.0` for an empty pass instead of refusing

**File:** `crates/apr-cli/src/commands/setfit_bench.rs:767-794`
**Issue:** The function refuses a zero-wall-time pass with a well-argued message ("the number
this would produce is not a throughput"), but an `n_rows == 0` pass skips the loop, measures a
positive elapsed, and returns `0.0` — a *published* throughput of zero rows/second for a
measurement that never ran. That is the same class of number the zero-wall-time guard exists
to prevent, on the other axis.

**Fix:**

```rust
if n_rows == 0 {
    return Err(CliError::InferenceFailed(
        "the throughput pass measured zero rows; a rate over an empty pass is not a throughput"
            .to_string()));
}
```

### WR-16: `bench run --cold-probe` is an unauthenticated file-load surface on the public CLI

**File:** `crates/apr-cli/src/commands/setfit_bench.rs:132-136`,
`crates/apr-cli/src/setfit_commands.rs:203-227`
**Issue:** The cold-probe mode is documented as "machinery, not a user surface" and only
`hide_short_help`-ed, but `run()` dispatches it *first*, "because it is the mode with the
fewest obligations: it takes no bench directory, writes nothing, and must not pay for any
check the other two need." It therefore skips the `--bench-dir` requirement and every cell
validation, and loads an arbitrary `--cold-probe <ARTIFACT>` (plus optional
`--cold-probe-base`) through the full reload path with only `requires = "cold_probe"`
relating the three flags. A `--probe-text` file is read with no size bound visible at that
door. Being reachable by users, it needs the same input discipline as the modes around it.

**Fix:** Route `probe_text` through `read_bounded`, validate that the artifact is a regular
file before the reload, and consider `hide = true` plus an explicit
"internal — spawned by `bench run`" refusal when the process was not started by
`measure_cold` (e.g. an env marker set on the child `Command`).

## Info

### IN-01: `stats/mod.rs` re-exports the unused helper and not the used one

**File:** `crates/aprender-core/src/stats/mod.rs:35-39`
**Issue:** The re-export list carries `paired_ci95_df9` — which under the active single-method
scope is never reached (`shot_delta` only runs when `methods_present.len() >= 2`) — but omits
`ci95_one_sample_df9`, the function the claims path actually calls. `bench_gate.rs:90` reaches
it through the full `stats::hypothesis::` path as a result, so the two claims helpers are
imported inconsistently.
**Fix:** Add `ci95_one_sample_df9` to the `pub use` list and import both from `stats::`.

### IN-02: `VerifyCell::out` is enforced by a conflict that produces the wrong message

**File:** `crates/apr-cli/src/setfit_commands.rs:311-313`,
`crates/apr-cli/src/dispatch_analysis.rs:1161`
**Issue:** `out` is declared `conflicts_with = "bench_dir"` where `bench_dir` is a *required*
argument, so `--out` is always rejected — the intent. But the rendered clap error names
`--bench-dir`, not the documented reason ("This door emits no machine-readable report
payload"). The field is destructured away with `..` and never read.
**Fix:** Drop the field and let clap reject `--out` as an unknown argument, or keep it and
handle it in `verify_cell::run` with the explanatory `CliError::ValidationFailed` the doc
comment promises.

### IN-03: `is_evidence_failure` assigns a global where the file's own convention requires `local`

**File:** `scripts/run_bench_cells.sh:289-290` (and `holder` at `:163`)
**Issue:** `run_one_cell` carries a comment explaining that "`local` throughout ... an
unqualified assignment here would silently rewrite the loop's own `shots`/`seed`
mid-iteration". `is_evidence_failure` then does `log="$1"` with no `local`, and the lock
branch assigns `holder` globally. Harmless today (bash dynamic scoping happens to bind
`run_one_cell`'s own `log`), but it is the pattern the file elsewhere calls out as a defect,
and it is a bashrs finding.
**Fix:** `local log; log="$1"` and `local holder`.

### IN-04: `beta_continued_fraction_f64` silently returns a non-converged value

**File:** `crates/aprender-core/src/stats/hypothesis.rs:717-767`
**Issue:** The Lentz loop breaks on convergence but falls out of `for m in 1..=MAX_ITER`
without any signal when it does not converge, returning whatever `h` happened to hold. For a
p-value asserted to 1e-9 against scipy this is the one place a silent numeric failure could
enter a published number. The fixtures cover df = 9, so the risk is small today.
**Fix:** `debug_assert!(converged, ...)` at minimum, or return `Option<f64>` and let
`t_distribution_pvalue_f64` surface a refusal rather than a value.

---

_Reviewed: 2026-09-08_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
