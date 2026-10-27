---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
reviewed: 2026-10-26T00:00:00Z
depth: standard
files_reviewed: 44
files_reviewed_list:
  - CLAUDE.md
  - Cargo.lock
  - Cargo.toml
  - README.md
  - contracts/census.json
  - contracts/contracts.nt
  - contracts/decide-apr-v1.yaml
  - contracts/decide-tool-boundary-v1.yaml
  - contracts/laya-finetune-gate-v1.yaml
  - crates/apr-cli/src/commands/eval/setfit.rs
  - crates/apr-cli/src/commands/eval/setfit_tests.rs
  - crates/apr-cli/src/dispatch_analysis.rs
  - crates/apr-cli/src/extended_commands.rs
  - crates/apr-cli/src/lib_dispatch_coverage.rs
  - crates/apr-cli/src/lib_parse_rosetta_02.rs
  - crates/apr-cli/src/lib_verbose_inheritance_parse.rs
  - crates/apr-cli/src/setfit_tag_tests.rs
  - crates/aprender-contrastive-data/examples/laya_dir_to_canonical.rs
  - crates/aprender-core/tests/monorepo_invariants.rs
  - crates/aprender-decide/examples/pack_laya.rs
  - crates/aprender-decide/src/artifact.rs
  - crates/aprender-decide/src/artifact/ladder.rs
  - crates/aprender-decide/src/pack.rs
  - crates/aprender-decide/src/verify.rs
  - crates/aprender-decide/src/verify/tests.rs
  - crates/aprender-mcp-decide-lambda/README.md
  - crates/aprender-mcp-decide-motivation-lambda/Cargo.toml
  - crates/aprender-mcp-decide-motivation-lambda/README.md
  - crates/aprender-mcp-decide-motivation-lambda/src/main.rs
  - crates/aprender-mcp-decide/src/tests.rs
  - justfile
  - scripts/duplicate_bin_names_allowlist.txt
  - scripts/laya_claims.tsv
  - scripts/laya_gates.tsv
  - scripts/laya_train/README.md
  - scripts/laya_train/bakeoff.py
  - scripts/laya_train/contract.py
  - scripts/laya_train/gate.py
  - scripts/laya_train/lifecycle.py
  - scripts/laya_train/metrics.py
  - scripts/laya_train/prepare_goldstandard.py
  - scripts/laya_train/prepare_motivation.py
  - scripts/laya_train/probe_epoch.py
  - scripts/laya_train/train.py
findings:
  critical: 0
  warning: 6
  info: 5
  total: 11
status: issues_found
---

# Phase 12: Code Review Report

**Reviewed:** 2026-10-26
**Depth:** standard
**Files Reviewed:** 44
**Status:** issues_found

## Summary

I reviewed the phase's own hunks (`git diff f2ea0576e HEAD`). That covers the pilot-waiver mechanism in Rust (`verify.rs`, `pack.rs`, `artifact.rs`) and Python (`gate.py`, `contract.py`, `train.py`, `lifecycle.py`). It also covers the bake-off harness and its metrics, the motivation data preparation, the `--dump-probabilities` path in `apr eval`, the thin Lambda crate, and the justfile hunks. The foreign justfile edits were excluded. `Cargo.lock`, `census.json` and `contracts.nt` got a sanity check only.

The waiver logic itself is sound against the questions asked:
- **Margin and mismatches.** `decide_pilot_waiver` never waives `margin`, alone or with `ece_post`. A declaration naming `margin` fails to deserialize. A claim on a passing run, an undeclared id, a claimed `waives` that is not exactly `["ece_post"]`, and a one-digit `applies_to` mismatch are all refused.
- **Hash binding.** The four `applies_to` values are the report's `inputs_sha256` and `recipe_id`, and I confirmed they are re-hashed against the data dir (`check_inputs`) and `recipe.json` (`pack.rs:694`) before the decision.
- **Provenance.** The waiver can come only from the contract file. `gate.waiver` is bound at rung 4 in the manifest and tested on every load door.
- **Python/Rust parity.** `failed_clauses` is one function that `verify_report`, the trainer and the lifecycle check all read.

The Python metrics I checked have no row-index versus label key collision. `bakeoff.py` aligns every arm by index plus `text_sha256` (and SetFit by `row_ids`) and never by truth label. Class and band indices are consistent: `precision_recall_per_class` and `false_assignment_rate` use column positions, and the `other` index comes from `task.json`.

No blocker found. The warnings are gaps around the waiver's scope, the one-shot test read, evidence binding, and an overstated door. The infos are staleness and dead code.

## Warnings

### WR-01: A pilot waiver has no ceiling on `ece_post`, so an arbitrarily miscalibrated model is `deploy_eligible`

**File:** `crates/aprender-decide/src/verify.rs:2703` (`decide_pilot_waiver`), mirrored in `scripts/laya_train/gate.py:181-196`
**Issue:** An ECE-only failure matching a declaration returns `Waived` whatever the recomputed `ece_post` is. A model at 0.11 and one at 0.90 get the same verdict, so long as margin passes. The contract states it as an invariant ("a waived run is deploy-eligible"). The waiver is hash-bound to the exact run, but nothing bounds how bad the waived clause may be. The owner's decision (D-19) was a pilot that ships "labelled gate-failed", not "any calibration". The mechanism shipped was unused on the one real run (gate passed at 0.0936), so nothing exercised this edge. A future declaration inherits it silently.
**Fix:** Make the waiver carry the bound the owner accepted, e.g. a required `max_waived_ece` in each declaration. Compare the recomputed value in `decide_pilot_waiver` and add a case-table row for "just inside" and "just outside". Mirror it in `matching_pilot_waiver` and `validate_pilot_waivers`.
```rust
// WaiverDecision::Waived only if recomputed.ece_post <= decl.max_waived_ece (a finite declared bound)
```

### WR-02: A waiver is not bound to a single attempt, so the "ONE declared run" stop rule is process-only

**File:** `contracts/laya-finetune-gate-v1.yaml` (`motivation_v1.stop_rule`, `pilot_waivers.declarations[0]`); `crates/aprender-decide/src/verify.rs:3528`
**Issue:** `applies_to` is `(task, train, eval, recipe_id)`. `recipe.json` carries only the declared seed (13) and the shots/epochs, so every re-run of the same data and recipe produces the same `recipe_id`. MPS training is not bit-reproducible. Any number of retrains whose ECE-only failure differs is therefore waived by the same declaration. The `stop_rule` ("ONE declared run... cap two") lives only in prose and in an evidence JSON. The verifier cannot see an attempt count. Combined with WR-01, a retrain-until-it-ships loop is accepted by code that the contract says forbids it. It is not exploitable today because the declared run passed unwaived.
**Fix:** Bind the declaration to the run's output as well as its inputs, for example `applies_to.eval_probs_sha256` (the report already records it), so a waiver covers one outcome. Alternatively delete the declaration from the contract once the declared run has been recorded, so that a later run cannot silently reuse it.

### WR-03: `--dump-probabilities` write can fail after the one-shot test pass is consumed, with no number printed

**File:** `crates/apr-cli/src/commands/eval/setfit.rs:391` (pre-flight) and `:640-655` (write before print)
**Issue:** The pre-flight `refuse_existing_output` checks only that the destination does not exist. The classify pass over the canonical test rows runs, and then `atomic_write` runs. If the write fails (parent is a file, read-only filesystem, disk full), `?` returns before `report(...)`. The operator has consumed the canonical test measurement ("reportable exactly once per committed selection", per the note this same function emits) and got neither the accuracy nor the dump. The comment calls this "a failed write is a non-zero exit and never a printed success", which is correct, but it trades a lost one-shot measurement for it.
**Fix:** Make the pre-flight also prove the destination is writable before the test split is read (create the parent directory, then create and remove a probe file). Or write the dump to a temp sibling and print the row first, and treat a rename failure as a loud warning that keeps the printed row.

### WR-04: Bake-off Laya arm and SetFit provenance are not bound to the gate report's own hashes or to the artifact

**File:** `scripts/laya_train/bakeoff.py:220-261` (`read_laya_run`, `check_laya_recompute`), `:428`
**Issue:** The bake-off "reproduces the gate report bit for bit", but it never compares `gate["eval_probs_sha256"]`, `zero_shot_probs_sha256`, or `recipe_id` with the files it read. The recomputation check is circular for an edited run dir: if `eval-probs.json` and `gate-report.json` are changed consistently, it passes. Nothing ties the evidence to the packed artifact's sha either. `per_seed is None` returns early, so a report stripped of its `seeds` block skips the per-seed checks. The SetFit dump's `artifact_sha256` and `dataset_fingerprint` are copied into the evidence without being compared with anything. The harness is correctly labelled "not a gate", but the evidence is cited by the hand-off and the claims ledger.
**Fix:** In `check_laya_recompute`, also assert the following, and require `seeds.per_seed` when `gate["seeds"]["policy"]` is present:
```python
common.sha256_file(run/"eval-probs.json") == gate["eval_probs_sha256"]
common.sha256_file(run/"zero-shot-probs.json") == gate["zero_shot_probs_sha256"]
```
Record the packed artifact sha beside them.

### WR-05: A `blocked:<class>:<path>` arm with a non-existent evidence file is accepted (exit 0, `evidence_sha256: null`)

**File:** `scripts/laya_train/bakeoff.py:274-278` (`blocked_record`) and `:577`
**Issue:** `blocked_record` records `evidence_sha256: None` when the path is not a file, and `run` silently skips adding it to `inputs`. A typo'd or deleted evidence path therefore yields a clean `BAKEOFF OK` report whose "blocked" claim is unverifiable. The selftest covers only the present-file case. The real Kev arm has a hashed evidence file (`b117b126...`), so this is a latent hole, not a current defect.
**Fix:** Refuse when the evidence file is absent, in the same style as the other refusals:
```python
if not p.is_file():
    refuse("bakeoff-alignment", "%s blocked evidence %s is not a file" % (arm, evidence))
```

### WR-06: The served door of 8 is overstated for the 97-token task, and the check that backs it is vacuous

**File:** `contracts/decide-tool-boundary-v1.yaml` (`served_tasks[1].served_max_texts: 8`); `crates/aprender-mcp-decide/src/tests.rs` (`every_served_task_fits_its_door`); `scripts/laya_train/prepare_motivation.py:242`
**Issue:** Admission sums BUILT row lengths (prefix plus post) against 800. With P = 97, eight texts leave 24 tokens for all eight posts. The measured post p50 is 45, and the evidence itself records `texts_per_call` p50 5 / p95 4, `batch_of_4.max: false`, and an eight-text request refused at the token budget. `served_tasks[].served_max_texts: 8`, the test's `door * min_row <= budget` (true only for empty posts), and the lambda README ("the door (8 texts and 800 built tokens) is the same for both") all present 8 as the usable door. Separately, `measure_report` hard-codes `door_b = fit if p > 100 else DOOR_CEILING`. `100` is `800 / 8` frozen as a literal. If `classify_max_total_tokens` ever changes, `door_b` overclaims while `fit` stays correct. The hand-off discloses the effective 4 to 5, so this is a contract and docs overstatement, not a hidden surprise.
**Fix:** Compute `door_b = fit` always, since `fit` is already `min(DOOR_CEILING, total // p)`. Add a `served_tasks` field for the measured typical admitted count (or state in the contract that 8 is the count ceiling and the token budget decides), and change the README sentence accordingly.

## Info

### IN-01: Python `validate_pilot_waivers` takes the waivable set from the YAML; Rust hard-codes it

**File:** `scripts/laya_train/contract.py:338-354`
**Issue:** The docstring says the rules are "the SAME rules the Rust verifier applies". Rust's `WaivableClause` enum has only `ece_post`, and Rust ignores `waivable_clauses`. Python accepts whatever `waivable_clauses` lists. Adding `margin` there makes Python accept a declaration that Rust refuses to load, and that refusal covers every artifact. `matching_pilot_waiver` still never claims a margin waiver, so it fails closed, but the stated parity does not hold.
**Fix:** Hard-code `("ece_post",)` in Python, or assert `waivable_clauses == ["ece_post"]`.

### IN-02: `FALSIFY-LAYA-GATE-020` prediction is stale

**File:** `contracts/laya-finetune-gate-v1.yaml:1112`
**Issue:** The prediction still says "the committed contract declares nothing yet". Since 5.1.1 it declares `motivation-v1-pilot-ece`, and `waiver_real_contract_declarations_parse` now asserts exactly that. The contract text contradicts its own test.
**Fix:** Reword to "the committed contract declares exactly `motivation-v1-pilot-ece`".

### IN-03: `GUARD_DEFAULT` in `probe_epoch.py` is dead

**File:** `scripts/laya_train/probe_epoch.py:61`
**Issue:** The phase replaced `default=GUARD_DEFAULT` with the per-rule table (`EPOCHS_RULES[...][2]`), so the constant is no longer read.
**Fix:** Delete the constant.

### IN-04: Two small justfile inaccuracies

**File:** `justfile:1334`, `justfile:2168`
**Issue:** The `laya-prepare-motivation` comment advertises `--measure (--draft a|b | --task ...)`, but `prepare_motivation.py` has no `--draft` option. In `_laya-edge-health-get`, the config-refusal message builds `LAYA_EDGE_HEALTH_$name` from the loop variable `MAX`, which names a variable that does not exist (the real one is `LAYA_EDGE_HEALTH_MAX_SECONDS`).
**Fix:** Drop `--draft a|b` from the comment. Use `case "$name" in MAX) var=LAYA_EDGE_HEALTH_MAX_SECONDS ;; *) var=LAYA_EDGE_HEALTH_$name ;; esac` for the message.

### IN-05: Length-band cut points depend on float noise in `100/3`

**File:** `scripts/laya_train/metrics.py:232-243`
**Issue:** The cuts come from `np.percentile(a, 100/3)`. The evidence shows `126.00000000000003`. Lengths are integers compared with `n <= c1`, so a cut that lands a hair low rather than a hair high would move a row between bands. The band tables are reported figures, though not a gate.
**Fix:** Round each cut to a fixed number of decimals before comparing (e.g. `round(c, 9)`), or derive the boundaries from sorted integer ranks, so the band of a row on a boundary does not depend on float noise.

---

_Reviewed: 2026-10-26_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
