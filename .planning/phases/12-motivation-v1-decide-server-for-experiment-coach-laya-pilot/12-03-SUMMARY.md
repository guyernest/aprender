---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 03
subsystem: laya-artifact-identity-and-trainer
tags: [laya, decide, artifact, manifest, pilot-waiver, trainer, byte-stability, contract]
status: complete

requires:
  - phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
    provides: "plan 12-02: pilot_waivers block, GateReport.pilot_waiver, decide_pilot_waiver, expected_keys(..., has_waiver)"
provides:
  - "aprender_decide::artifact GateSummary.waiver (default + skip_serializing_if) and the rung-4 gate.waiver comparison (24 entries)"
  - "decide-apr-v1 3.1.0: manifest gate waiver?, bindings row '/gate/waiver' (report:/pilot_waiver/id), amendment paragraph, FALSIFY-DECIDE-APR-014"
  - "pack_laya inspect: top-level waiver (id or the string none)"
  - "scripts/laya_train: contract.recipe_id_for, contract.pilot_waiver_declarations / validate_pilot_waivers, gate.failed_clauses / report_failed_clauses / matching_pilot_waiver, train.attach_pilot_waiver, lifecycle's independently recomputed has_waiver and WAIVED REPORT ASSEMBLY OK case"
  - "12-WAIVER-EVIDENCE.json: the real laya-verify line of the deployed antisemitism-v1 artifact after the change"
affects: [12-11]

actuals:
  tokens: 6600
  tasks: 3
  commits: 4
plan_head_before: 18ae8fe9fe29452131fd14a7a2cab022815020e5
plan_head_after: 8692788d532f83301be68a77cc97a3059f499d6d

tech-stack:
  added: []
  patterns:
    - "an optional manifest leaf is serialized only when present, so shipped artifacts keep their bytes; the golden sha256 test is the proof"
    - "generic binding sweeps take the union of an unwaived and a waived fixture, resolving each row's source pointers against the fixture that carries the leaf"
    - "one clause list (gate.failed_clauses) shared by the checker, the trainer and the lifecycle; has_waiver is recomputed from the contract, never read from the report"

key-files:
  created:
    - .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-WAIVER-EVIDENCE.json
  modified:
    - crates/aprender-decide/src/artifact.rs
    - crates/aprender-decide/src/artifact/ladder.rs
    - crates/aprender-decide/examples/pack_laya.rs
    - contracts/decide-apr-v1.yaml
    - scripts/laya_train/contract.py
    - scripts/laya_train/gate.py
    - scripts/laya_train/train.py
    - scripts/laya_train/lifecycle.py

key-decisions:
  - "decide-apr-v1 version 3.1.0: pv diff against the start commit suggested a MINOR bump (added FALSIFY-DECIDE-APR-014); pv diff does not see manifest.fields or bindings edits, so the amendment paragraph states them"
  - "gate.waiver is one more entry in check_manifest_bindings (24), compared as Option<String> against report.pilot_waiver.id, so strip, add and change are all one refusal naming gate.waiver"
  - "contract.pilot_waiver_declarations mirrors the Rust view fully (exact declaration keys, waives within waivable_clauses, exactly four applies_to keys, 64 lowercase hex, unique ids) and is also called from check_numbers so a malformed block refuses a training run before any model loads"
  - "the matching rule requires failed == [ece_post] exactly and waives == [ece_post] exactly, so a duplicated ece_post entry is not an ECE-only failure"

requirements-completed: [D-19, D-20]

coverage:
  - id: D1
    description: "A waived run packs with manifest gate.waiver filled from the report and loads through load_bytes, load_hashed and load_path; strip, add and change are refused at rung 4 naming gate.waiver"
    requirement: "D-19"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-decide --lib artifact::ladder::waived_run_packs_with_gate_waiver_and_loads_at_every_door / manifest_gate_waiver_disagrees_with_report"
        status: pass
    human_judgment: false
  - id: D2
    description: "An unwaived pack serializes no waiver key; the committed golden sha256 and the determinism tests still pass"
    requirement: "D-19"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-decide --lib artifact:: (unwaived_manifest_serializes_no_waiver_key, determinism::golden_sha, same_process_twice)"
        status: pass
      - kind: integration
        ref: "one real just laya-verify of models/decide/laya-antisemitism-v1.apr: deploy_eligible true, waived null, sha 2293bebb...6c776e (12-WAIVER-EVIDENCE.json)"
        status: pass
    human_judgment: false
  - id: D3
    description: "The optional /gate/waiver leaf is covered by both generic binding tests (union of leaves, per-fixture pointer resolution, forged on the waived fixture)"
    requirement: "D-19"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-decide --lib artifact::ladder::every_manifest_leaf_is_bound / manifest_bindings_table_matches_manifest_leaves"
        status: pass
    human_judgment: false
  - id: D4
    description: "The trainer writes pilot_waiver only through attach_pilot_waiver (failed_clauses then matching_pilot_waiver); margin failures, passing runs and any hash mismatch get no claim; pass and the exit status are untouched"
    requirement: "D-20"
    verification:
      - kind: integration
        ref: "just laya-train-selftest (gate.py selftest waiver cases; lifecycle WAIVED REPORT ASSEMBLY OK; LAYA TRAIN SELFTEST OK)"
        status: pass
    human_judgment: false
  - id: D5
    description: "recipe_id_for reproduces the deployed antisemitism-v1 run's recipe.json bytes and recipe_id"
    requirement: "D-19"
    verification:
      - kind: integration
        ref: "recipe_id_for over models/decide/laya-antisemitism-v1/recipe.json values == gate-report recipe_id d70c3f3da0da... and the file's bytes"
        status: pass
    human_judgment: false
---

# Phase 12 Plan 03: Waiver bound into artifact identity and the trainer's claim Summary

**A pilot-waived artifact now names its waiver inside its pinned sha (manifest `gate.waiver`, bound to the embedded report at rung 4 on every door), the trainer writes the claim only for a declared ECE-only failure through one shared clause list, and the deployed antisemitism-v1 artifact still verifies `deploy_eligible true, waived null` on real weights.**

## What was built

- **Artifact (Task 1, tracer).** `GateSummary.waiver: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`, filled in `build_manifest` from `report.pilot_waiver`. `check_manifest_bindings` gained `("gate.waiver", m.gate.waiver == report.pilot_waiver id)` (23 to 24 entries). decide-apr-v1 got the `waiver?` field text, the `'/gate/waiver'` bindings row sourced from `report:/pilot_waiver/id`, and an additive amendment paragraph. `pack_laya inspect` prints `"waiver":"none"` or the id.
- **Ladder tests.** `packed_waived()` packs the same fixture inputs with a report carrying `pilot_waiver {id: "test-pilot", waives: [ece_post]}` through the real write path. New: `waived_run_packs_with_gate_waiver_and_loads_at_every_door`, `manifest_gate_waiver_disagrees_with_report` (strip, change, add), `unwaived_manifest_serializes_no_waiver_key`. `manifest_bindings_table_matches_manifest_leaves` takes the union of both fixtures' leaves and resolves each row's `recipe:`/`report:` pointers against the fixture that carries the leaf. `every_manifest_leaf_is_bound` was refactored onto `sweep_bindings` and adds a second sweep on `packed_waived()` for exactly the leaves `packed()` lacks (asserted to be only `/gate/waiver`).
- **Trainer (Task 2).** `contract.recipe_id_for` (the one recipe builder; `train.main` calls it), `contract.pilot_waiver_declarations`/`validate_pilot_waivers` (Rust load rules mirrored, also in `check_numbers`), `gate.failed_clauses` (extracted from `verify_report`; `evaluate_gate` and the trainer use it too), `gate.report_failed_clauses`, `gate.matching_pilot_waiver`, `train.attach_pilot_waiver` plus the `PILOT WAIVER <id>` log line. `lifecycle.py` now passes an independently recomputed `has_waiver` to its exact gate-report key-set check and runs the `WAIVED REPORT ASSEMBLY OK` integration case (a real tiny-run report turned ECE-only failing, a synthetic matching declaration through `train.attach_pilot_waiver`, keys equal `expected_keys(..., has_waiver=True)`, `verify_report` still pass False with failed `["ece_post"]`, and no claim for a margin failure or for margin plus ece_post).
- **Contract and evidence (Task 3).** decide-apr-v1 3.1.0 with FALSIFY-DECIDE-APR-014; `12-WAIVER-EVIDENCE.json`.

## The real verify line of record

`just laya-verify models/decide/laya-antisemitism-v1.apr models/decide/laya-antisemitism-v1 data/decide/goldstandard-2024 $BASE` (under `lockf`, rc 0, 1300 s, built from this plan's code):

```
{"argmax":"2129/2129","artifact_sha256":"2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e","deploy_eligible":true,"noise":3.592973853439041e-6,"recomputed":{"ece_post":0.04236402879471284,"ft_macro_f1":0.7956129081807202,"margin":0.23603720989749355,"zs_macro_f1":0.5595756982832266},"rescore_bound":0.000014371895413756164,"rescore_max_abs":7.554888725280762e-6,"shipped_seed":23,"waived":null,"zs_noise":0.000015070898052527149,"zs_rescore_bound":0.000060283592210108594,"zs_rescore_max_abs":0.00001195073127746582}
```

`just laya-inspect` reports `"waiver":"none"`; `shasum -a 256` of the file equals the deployed pin.

## TDD record

The plan is `type: execute` (tracer, then a `tdd="true"` task), not `type: tdd`, so no commit-level RED/GREEN gate is enforced.

- **Task 1 RED by mutation (observed, source restored from a saved copy after each).** M1: the `gate.waiver` comparison replaced by `true` turned `manifest_gate_waiver_disagrees_with_report` and `every_manifest_leaf_is_bound` red (2 failed, 60 passed). M2: `skip_serializing_if` removed turned `determinism::golden_sha`, `manifest_bindings_table_matches_manifest_leaves`, `unwaived_manifest_serializes_no_waiver_key` and `every_manifest_leaf_is_bound` red (4 failed). So the golden sha256 does catch a serialized `null`, which is the T-12-11 mechanism.
- **Task 2 RED (observed).** The gate selftest cases were written first; `gate.py --selftest` failed with `NameError: name 'failed_clauses' is not defined` before the implementation, then passed with 92 `ok` lines and `GATE SELFTEST OK`. The lifecycle case was written together with the implementation it drives, so its RED is the absent helper, not a separate run.

## Deviations from Plan

**1. [No-op] FALSIFY-LAYA-GATE-021 `test_harness` was already present.** Plan 12-02 had committed `test_harness: 'just laya-train-selftest'` on FALSIFY-LAYA-GATE-021 (its summary said "none", which was wrong); `contracts/laya-finetune-gate-v1.yaml` is therefore unchanged by this plan and the check passes at HEAD. Nothing to fix here beyond this note.

**2. [Rule 2 - Missing critical functionality] Python reader mirrors more of the Rust view than the plan listed.** The plan named repeated ids and malformed hashes; the Rust view is `deny_unknown_fields` and `WaivableClause` has only `ece_post`, so `validate_pilot_waivers` also refuses unknown/missing declaration keys, a `waives` entry outside `waivable_clauses` (margin) and missing `applies_to` keys. Without this the two readers could disagree on a malformed declaration.

**3. [Rule 3] No `contracts.nt`/`shapes.ttl`/`census.json` change.** The plan expected possible regeneration. FALSIFY-DECIDE-APR-014 adds no triple and no census entry: `pv extract contracts --check` passes in a clean worktree of commit eacea60a3 (rc 0, `written: []`), and a fresh `pv census` equals the tracked file. As noted in 12-02, the same `--check` still fails in the main tree because of the untracked `.claude/worktrees`.

**4. [Rule 1 - test-code fix] `serde_json::json!` in a test tripped the repo's `unwrap` ban under `clippy --tests`** (the macro expands to `unwrap`); the fixture builds its `pilot_waiver` object with `Map::from_iter` instead. Library and examples were clean either way.

## Known tracking gap

STATE.md, ROADMAP.md and REQUIREMENTS.md were not updated: `gsd_run` state/roadmap queries error on `04-VERIFICATION.md`'s `status: gaps_acknowledged` (not touched), and the working tree carries the owner's uncommitted STATE.md, config.json, state.json and justfile changes. Tracking is left to the orchestrator.

## Task commits

| Task | Commit | Content |
|------|--------|---------|
| 1 (tracer) | 4d628af26 | GateSummary.waiver, rung-4 gate.waiver, decide-apr-v1 fields/bindings/amendment, ladder tests, inspect waiver |
| 2 | 766a40f4d | recipe_id_for, declaration validation, failed_clauses, matching_pilot_waiver, attach_pilot_waiver, lifecycle key-set check and integration case, selftest cases |
| 3 (contract) | eacea60a3 | decide-apr-v1 3.1.0, FALSIFY-DECIDE-APR-014 |
| 3 (evidence) | 8692788d5 | 12-WAIVER-EVIDENCE.json |

`commits: 4` is measured from the persisted ledger (`18ae8fe9f..8692788d5`); the docs commit carrying this SUMMARY is separate. `actuals.tokens` is chars/4 over the realized diff (26,326 characters of diff excluding this SUMMARY).

## Verification

- `cargo test -p aprender-decide --lib`: 181 passed, 0 failed (62 in `artifact::`); the six named binding tests and the determinism tests listed ok.
- `cargo clippy -p aprender-decide --lib --examples --tests --no-deps -- -D warnings` clean; `cargo fmt -p aprender-decide` applied.
- `gate.py --selftest`: `GATE SELFTEST OK`; `just laya-train-selftest`: `WAIVED REPORT ASSEMBLY OK`, `LIFECYCLE OK`, `LAYA TRAIN SELFTEST OK`.
- recipe_id_for over the deployed run's values: recipe_id `d70c3f3da0da...` and the exact recipe.json bytes reproduced.
- AST of `SeedRun`, `train_seed`, `calibration_logits` (train.py) and `fit_temperature`, `calibration_monitor`, `EarlyStopper` (gate.py) identical to the start commit.
- `pv validate` on decide-apr-v1 and laya-finetune-gate-v1: 0 errors, 0 warnings; census current; `pv extract contracts --check` rc 0 in a clean worktree.

## Threat Flags

None: no new endpoint, auth path or trust-boundary surface beyond the waiver mechanism covered by T-12-10..13. No dependency added (Cargo.lock and `scripts/laya_train/uv.lock` untouched).

## Known Stubs

None.

## Self-Check: PASSED

- FOUND: crates/aprender-decide/src/artifact.rs, crates/aprender-decide/src/artifact/ladder.rs, crates/aprender-decide/examples/pack_laya.rs, contracts/decide-apr-v1.yaml, scripts/laya_train/contract.py, scripts/laya_train/gate.py, scripts/laya_train/train.py, scripts/laya_train/lifecycle.py, .planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-WAIVER-EVIDENCE.json
- FOUND commits (each an ancestor of HEAD): 4d628af26, 766a40f4d, eacea60a3, 8692788d5
