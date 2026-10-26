---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
plan: 02
subsystem: laya-gate-verifier
tags: [laya, decide, verify, pilot-waiver, gate, contract, fail-closed, falsification]
status: complete

requires:
  - phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server
    provides: the recompute-and-verify pipeline (check_gate, VerifyPolicy::from_contract_views), run_field_bindings sweep, the laya-finetune-gate-v1 contract
provides:
  - "laya-finetune-gate-v1 5.1.0: top-level pilot_waivers block (rule, waivable_clauses [ece_post], declarations []), equation pilot_waiver_honoured, gate_report_schema.pilot_waiver (OPTIONAL), two run_field_bindings rows, FALSIFY-LAYA-GATE-016..021"
  - "aprender_decide::verify: PilotWaiversView (validated at load), PilotWaiverDecl, WaiverAppliesTo, WaivableClause (no margin variant), RunIdentity, WaiverDecision, decide_pilot_waiver, VerifyPolicy.pilot_waivers, VerifyReport.waived, VerifyError::WaiverUndeclared / WaiverWithoutFailure"
  - "aprender_decide::pack: GateReport.pilot_waiver (serde default), PilotWaiverClaim"
  - "pack_laya: `waived` key on the verify JSON line and `waived=<id|none>` on the PACKED line"
  - "scripts/laya_train/contract.py: pilot_waiver optional-key rule, expected_keys(..., has_waiver=False)"
affects: [12-03, 12-11]

actuals:
  tokens: 13300
  tasks: 2
  commits: 3
plan_head_before: cd9529fd91d126e94121411fa00296f08abdb16f
plan_head_after: 1edb61ce82da43f8fef6daf21eed2a899c713292

tech-stack:
  added: []
  patterns:
    - "the decision is one pure function over already-verified values (RunIdentity read from the cross-checked gate report), placed between check_gate and the seed-policy check, with the GateFailed builder shared"
    - "a block the contract mapping reads is validated where it deserializes (serde try_from), so every caller fails alike"
    - "a separate accepted-waived fixture instead of mutating the sweep's passing positive control"

key-files:
  created: []
  modified:
    - contracts/laya-finetune-gate-v1.yaml
    - contracts/census.json
    - contracts/contracts.nt
    - crates/aprender-decide/src/verify.rs
    - crates/aprender-decide/src/verify/tests.rs
    - crates/aprender-decide/src/pack.rs
    - crates/aprender-decide/examples/pack_laya.rs
    - scripts/laya_train/contract.py

key-decisions:
  - "Contract version 5.1.0: pv diff between the plan's start commit and the working file suggested a MINOR bump (added equation pilot_waiver_honoured and FALSIFY-LAYA-GATE-016..021)"
  - "WaivableClause has only the ece_post variant, so a declaration naming margin fails to deserialize and the contract fails to load before any run is read (D-20)"
  - "declarations are validated at PilotWaiversView deserialization (unique ids, four applies_to values 64 lowercase hex), not in the decision, so the Rust and Python readers cannot disagree"
  - "RunIdentity::of(&gate_report) reuses the values check_inputs (data-dir re-hash) and PackInputs::from_run_dir (recipe_id re-hash) already bound; no file is re-read"
  - "a waived run is deploy-eligible and reported, never silently passing: VerifyReport.waived, printed by pack_laya"

requirements-completed: [D-19, D-20]

coverage:
  - id: D1
    description: "A declared, run-bound, ECE-only pilot waiver is accepted by the verifier and reported as waived; the same run without the declaration is refused"
    requirement: "D-19"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-decide --lib verify::tests::waiver_declared_ece_only_failure_is_accepted_and_reported"
        status: pass
    human_judgment: false
  - id: D2
    description: "Every undeclared, mismatched, claimed-but-undeclared and passing-run waiver is refused with the existing or a new named refusal (GateFailed exit 3; WaiverUndeclared, WaiverWithoutFailure exit 2)"
    requirement: "D-19"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-decide --lib verify::tests::waiver_undeclared_ece_failure_is_still_gate_failed / waiver_applies_to_mismatch_is_gate_failed / waiver_claimed_by_report_but_undeclared_is_refused / waiver_on_a_passing_run_is_refused / waiver_new_refusals_exit_2_and_name_their_variant / decide_pilot_waiver_case_table"
        status: pass
    human_judgment: false
  - id: D3
    description: "Margin is never waivable (GateFailed under a matching declaration; a margin declaration, a repeated id and a malformed applies_to hash fail the contract load)"
    requirement: "D-20"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-decide --lib verify::tests::waiver_never_covers_margin / waiver_contract_naming_margin_fails_to_load / waiver_contract_with_duplicate_ids_fails_to_load / waiver_contract_with_malformed_hash_fails_to_load"
        status: pass
    human_judgment: false
  - id: D4
    description: "The gate-report schema, run-field binding rows, Python optional-key rule and the sweep over both the unwaived and the waived base stay consistent; the armed FALSIFY-LAYA-GATE-010 vectors still refuse GateFailed"
    requirement: "D-19"
    verification:
      - kind: unit
        ref: "cargo test -p aprender-decide --lib verify::tests::every_run_field_is_bound_or_report_only / run_field_bindings_table_matches_fixture_leaves"
        status: pass
      - kind: integration
        ref: "just laya-train-selftest (LAYA TRAIN SELFTEST OK); LAYA_FAIL_CLOSED_VECTORS=1 cargo test -p aprender-decide --release --test fail_closed_vectors (1 passed, 410 s, no SKIP)"
        status: pass
    human_judgment: false
---

# Phase 12 Plan 02: Pilot waiver in the release-gating verifier Summary

**A declared, run-bound pilot waiver can excuse an ECE-only gate failure in `aprender_decide::verify` and nothing else: margin is unwaivable (the contract refuses to load a declaration naming it), every undeclared, mismatched or passing-run claim keeps a named refusal, and the waived verdict is printed beside `deploy_eligible`.**

## What was built

- **Contract (laya-finetune-gate-v1, 5.0.0 to 5.1.0).** Top-level `pilot_waivers` (`rule`, `waivable_clauses: [ece_post]`, `declarations: []`), equation `pilot_waiver_honoured`, the OPTIONAL `gate_report_schema.pilot_waiver`, run_field_bindings rows `/pilot_waiver/id` (names `WaiverUndeclared`) and `/pilot_waiver/waives/[*]` (names `GateFailed`), FALSIFY-LAYA-GATE-016..020 with `test:` lines, and 021 staged with a prose `implemented_by` for plan 12-03. An AMENDMENT paragraph states that no threshold, constant, seed, recipe value, data rule or existing schema field moved.
- **Verifier.** `GateContractView.pilot_waivers` (serde default) maps through the one `VerifyPolicy::from_contract_views`; no argument or env var can add a declaration. `decide_pilot_waiver(clauses, claim, run, policy)` is pure and sits between `check_gate` and the seed-policy check in `verify_loaded`; the `GateFailure` builder is one shared closure. `VerifyReport.waived: Option<String>`; two new refusals, both exit 2.
- **Load-time validation.** `PilotWaiversView` deserializes through a raw struct (`serde(try_from)`): a repeated declaration id, or an `applies_to` value that is not 64 lowercase hex, is a deserialization error naming the id and field. `WaivableClause` has only `EcePost`.
- **CLI.** `pack_laya` verify JSON line gains `waived` (null or the id) right after `deploy_eligible`; the PACKED line gains `waived=<id|none>`.
- **Python.** `_OPTIONAL_KEY_RULES["pilot_waiver"]`, every rule takes a fourth `waived` parameter, `expected_keys(..., has_waiver=False)`; lifecycle.py's two calls are valid unchanged.

## Operational note (read before editing the contract)

A malformed `pilot_waivers` block (margin in `waives`, a repeated id, a malformed hash) refuses EVERY artifact at load, the deployed antisemitism-v1 artifact included, so that symptom points at the contract, not at artifact drift. The block's `rule` text and FALSIFY-LAYA-GATE-018 say the same.

## TDD record

- **Commit shape.** The plan is `type: execute` with a tracer then a `tdd="true"` task, not `type: tdd`, so no RED/GREEN gate sequence is enforced. The tracer (Task 1) had already committed the decision code, so the Task 2 tests were written against finished branch code and passed on their first run. The honest RED evidence is therefore two things, both recorded here:
  1. **Bindings RED (observed).** With the sweep and union-of-bases edits in tests.rs and the contract rows not yet added, `run_field_bindings_table_matches_fixture_leaves` failed with `leaves with no row: [("gate-report.json", "/pilot_waiver/id"), ("gate-report.json", "/pilot_waiver/waives/[*]")]` and `every_run_field_is_bound_or_report_only` failed with `gate-report.json/pilot_waiver/id: no run_field_bindings row` (and the `waives` leaf). Adding the schema and binding rows turned both green. Semantic assessment: both failed on the planned assertion (an unbound leaf), not on a build or fixture fault.
  2. **Mutation RED (observed, 9 mutants of the branch code, each restored by `git checkout` of verify.rs).** M1 margin+ece made waivable: RED in `decide_pilot_waiver_case_table`, `waiver_never_covers_margin`. M2 applies_to unchecked: RED in the case table and `waiver_applies_to_mismatch_is_gate_failed`. M3 passing-run claim ignored: RED in the case table and `waiver_on_a_passing_run_is_refused`. M4 undeclared becomes GateFailed: RED in the case table, `waiver_claimed_by_report_but_undeclared_is_refused`, `waiver_declared_ece_only_failure_is_accepted_and_reported`. M5 duplicate ids allowed: RED in `waiver_contract_with_duplicate_ids_fails_to_load`. M6 hash shape unchecked: RED in `waiver_contract_with_malformed_hash_fails_to_load`. M7 waived not reported: RED in `waiver_declared_ece_only_failure_is_accepted_and_reported`. M8 claim waives unchecked: RED in the case table and `waiver_claimed_by_report_but_undeclared_is_refused`. M9 a `Margin` variant added to `WaivableClause`: RED in `waiver_contract_naming_margin_fails_to_load`. Every mutant turned at least one named test red; the record is `scratchpad/p12-02/mutations.log`.
- **Commit-scope.** The Task 2 tests, the contract `test:` lines and the binding rows had to land in one commit (the contract's staged-binding rule and the sweep), so Task 2 is a single `test(12-02)` commit rather than a `test` then `feat` pair. Flagged here as a deviation from the 2-3 commit TDD pattern; there is no missing-RED-commit finding because the plan is not `type: tdd`.

## Fixtures

`fullest_copy` and `permissive_policy` are byte-identical to the start commit (checked against `cd9529fd9`). The separate waived base is `fullest_waived_copy()` (the ECE-only failed copy plus noise record, shift probe, `mps:0` device and the claim) returned with `waiver_policy(&run)`, whose single declaration is read from the built copy. The table test takes the union of both bases' leaves; the sweep keeps its unwaived loop and adds a waived loop over exactly the two `/pilot_waiver` leaves (asserted: the waived base adds exactly two leaves), each mutated on a fresh waived copy: `/pilot_waiver/id` is refused `WaiverUndeclared`, `/pilot_waiver/waives/0` is refused `GateFailed`.

## `pv diff` and derived files

- `pv diff` (start-commit file vs working file) reported `v5.0.0 → v5.0.0`, suggested bump **minor**, additions: equation `pilot_waiver_honoured` and FALSIFY-LAYA-GATE-016..021. Chosen version: **5.1.0**.
- **The graph needed a clean-checkout regeneration.** `pv extract contracts --check` failed in the main tree before and after regeneration (the untracked `.claude/worktrees` pollute extraction, plan 08-33); it passes in a clean worktree of the commit (`clean_check=0`), which is what CI sees.
- **Pre-existing drift, not this plan's:** at the plan's start commit the tracked `contracts/census.json` was already stale by one kernel contract (1850 recorded, 1851 fresh) and `contracts/contracts.nt` lacked the five triples of `llm-endpoint-openai-conformance-v1`. Both were regenerated; the `.nt` change is exactly those five lines, and this plan's equation and tests add no triple. `shapes.ttl` is unchanged.

## Task commits

| Task | Commit | Content |
|------|--------|---------|
| 1 (tracer) | 0125d26f8 | pilot_waivers block, equation, typed views and policy mapping, decide_pilot_waiver in verify_loaded, VerifyReport.waived, new refusals, GateReport.pilot_waiver, pack_laya `waived`, the accept test and the undeclared-refusal test |
| 2 | 79351bbaa | the waiver_* table, case table, waived fixture and sweep extension, schema row, binding rows, FALSIFY-LAYA-GATE-016..021, version 5.1.0, Python optional-key rule, census |
| 2 (derived) | 1edb61ce8 | contracts.nt regenerated from a clean checkout |

`commits: 3` is measured from the persisted ledger (`cd9529fd9..1edb61ce8`); the docs commit carrying this SUMMARY is separate.

## Deviations from Plan

**1. [Rule 3 - Blocking] The tracked census and RDF graph were stale at the start commit.** `cmp` of a fresh census against `contracts/census.json` could never pass without regenerating more than this plan's change. Regenerated both (see above); described in the commit messages.

**2. [Rule 1 - Bug] First chore commit message misdescribed the `.nt` diff** (it named this plan's equation as the source of the five triples; the diff showed they belong to `llm-endpoint-openai-conformance-v1`). Corrected by amending the local, unpushed commit before anything else referenced its hash.

**3. Tracer RED evidence is mutation-based** (see TDD record): the branch code predated the Task 2 tests, so the tests could not be observed failing on absent code.

**4. `waiver_real_contract_declares_nothing_yet` will fail on purpose in plan 12-11,** which adds the first declaration; it says so in its doc comment and 12-11 updates it in the same commit.

No auth gates. No threat flags: no new network endpoint, auth path or trust-boundary schema beyond the waiver mechanism the plan's threat model (T-12-06..09) already covers. No dependency added: Cargo.lock and `scripts/laya_train/uv.lock` untouched.

## Known Stubs

None.

## Issues Encountered

- STATE.md, ROADMAP.md and REQUIREMENTS.md updates were left to the orchestrator, as instructed: `gsd_run` state/roadmap queries error on `04-VERIFICATION.md`'s `status: gaps_acknowledged` (not touched), and the working tree carries the owner's pre-existing uncommitted STATE.md, config.json, state.json and justfile changes (all untouched by this plan).
- zsh note for future scripts: `"$START:contracts/..."` is parsed as a history modifier; use `"${START}:contracts/..."`.

## Verification

- `cargo test -p aprender-decide --lib`: 178 passed, 0 failed; all fifteen named waiver and sweep tests listed ok.
- `cargo clippy -p aprender-decide --lib --examples --no-deps -- -D warnings` clean (and clean with `--tests`); `cargo fmt -p aprender-decide -- --check` clean; `cargo test -p aprender-decide --no-run` compiles every integration target; `python_records` and `demo_run` pass.
- Armed `LAYA_FAIL_CLOSED_VECTORS=1` real-weights run under `lockf`: `demo_vectors_are_refused_fail_closed ... ok`, 1 passed, 410 s, no SKIP (both vectors still refuse GateFailed).
- `just laya-train-selftest`: `LAYA TRAIN SELFTEST OK`.
- `"$PV" validate contracts/laya-finetune-gate-v1.yaml`: 0 errors, 0 warnings; `"$PV" extract contracts --check` passes in a clean checkout of the final commit; a fresh census equals the tracked file.
- Contract assertions (016..020 carry cargo `test:` lines, 021 none and cites 12-03, `pilot_waiver` OPTIONAL, empty declarations, `waivable_clauses [ece_post]`, both binding rows, version above 5.0.0) and the fixture-unchanged check against the start commit passed.

## Next Phase Readiness

Plan 12-03 adds the artifact manifest `gate.waiver` field and the trainer's `matching_pilot_waiver` write (FALSIFY-LAYA-GATE-021, using `expected_keys(..., has_waiver=True)`); plan 12-11 adds the motivation-v1 declaration before the one gate run and updates `waiver_real_contract_declares_nothing_yet`.

## Self-Check: PASSED

- FOUND: contracts/laya-finetune-gate-v1.yaml, contracts/census.json, contracts/contracts.nt, crates/aprender-decide/src/verify.rs, crates/aprender-decide/src/verify/tests.rs, crates/aprender-decide/src/pack.rs, crates/aprender-decide/examples/pack_laya.rs, scripts/laya_train/contract.py
- FOUND commits: 0125d26f8, 79351bbaa, 1edb61ce8 (each an ancestor of HEAD)
