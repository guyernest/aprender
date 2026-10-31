---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
plan: 02
subsystem: decide
tags: [laya, parity-contract, pack-bound, amendment-a2, per-set-bound, verify]
requires:
  - phase: 15-01
    provides: 15-BOUND-DECISION.json (S-B params, role-drift-advisory) and the SC1 anchor 91969e11e
  - phase: 15-03
    provides: the repaired Phase 8 binding audit (green base) and the pack-record eligibility role
provides:
  - laya-parity-v1 3.0.0 amendment A2 (six per-set constants, A2 text, FALSIFY-LAYA-PARITY-007)
  - aprender_decide::verify RescoreSetPolicy, VerifyPolicy::rescore_set, RescoreBound.stamp_bound, VerifyError::RescorePolicyInvalid
  - eleven noise_per_set tests with the Graviton cells as data, and an eight-mutation proof
affects: [15-04, 15-05, 15-06]
tech-stack:
  added: []
  patterns: [stamp versus enforced split (S-B add-alongside), one contract-to-policy mapping, discriminating fixture so a retained coarser guard cannot hide a mutation]
key-files:
  created: []
  modified:
    - contracts/laya-parity-v1.yaml
    - contracts/aprender/binding.yaml
    - crates/aprender-decide/src/verify.rs
    - crates/aprender-decide/src/verify/tests.rs
    - crates/aprender-decide/tests/python_records.rs
    - crates/aprender-decide/examples/pack_laya.rs
key-decisions:
  - "Enforced bound is per set: max(floor_s, k_s x noise) refused above ceiling_s; A1 keys stay the record stamp and the absent-record floor (S-B, as signed)"
  - "pv diff suggested a major bump, so laya-parity-v1 went 2.0.0 to 3.0.0"
  - "RescorePolicyInvalid is checked before the absent-record return, so an unusable per-set policy is refused even for a run with no record"
  - "New per-set tests use a policy with metric_recompute_abs widened to 1.0 (a2_base), because a 4e-5 probability shift on one row moves the reported NLL by about 1.4e-5 and ReportedMetricMismatch would refuse after the re-score; those tests pin the re-score rule only"
requirements-completed: [D-01, D-04, D-05, D-06, D-07, D-10]
duration: n/a
completed: 2026-10-31
status: complete
actuals:
  tokens: 40000
  tasks: 3
  commits: 3
plan_head_before: 171684f251d0788d5e86c44c153a0da421df7107
plan_head_after: 05421e2c6ee95aa2608634a09b8a62e703f972b9
---

# Phase 15 Plan 02: Per-set pack bound (laya-parity-v1 amendment A2) Summary

**The verifier now enforces the owner-signed per-set re-score bound (fine_tuned floor 5.0e-5, zero_shot floor 3.0e-5, k 4, ceiling 1.0e-3) while every A1 record stays valid and a run with no record is still held to 1.0e-5; the real binary prints the signed bound on the retained CUDA run, and eight source mutations each turn a named test red.**

## Commits

| Task | Commit | What |
|------|--------|------|
| 1 (tracer) | 6a13fac43 | six constants, RescoreSetPolicy, rescore_set, stamp_bound, RescorePolicyInvalid, per-set rescore_bounds; real-bytes pack |
| 2 | 8e1176c6c | eleven `noise_per_set_*` tests, python_records on stamp_bound and the enforced formula |
| 3 | 05421e2c6 | A2 text, equation and obligation, FALSIFY-LAYA-PARITY-007, version 3.0.0, binding notes, pack_laya doc |

All three descend from the sign-off commit 91969e11e.

## Verification results

- Tracer (real bytes, `lockf -k /tmp/aprender-laya-real-weights.lock just laya-pack` of `models/decide/motivation-v1-sm`, 90 s): `A2 ENGAGED PACKED /tmp/p15-02-t1/m.apr sha256=dd7b1aafa3ed1284f7d9e46a4b78b840daea605a2e2b226762a9bd6260e5a292 rescore_max_abs=0.000007241964340209961 zs_rescore_max_abs=0.000014901161193847656 rescore_bound=0.00005 zs_rescore_bound=0.00003 noise=0.000003255285573489086 zs_noise=0.0000017265105816122173 shipped_seed=17 argmax=100/100 waived=none`. **This Mac PACKED** the retained CUDA run (informational only, D-04): the printed bounds equal max(floor_s, k_s x the recorded noise) for the signed params (5e-5 and 3e-5), and the printed noises equal the record's. The scratch .apr was deleted.
- `verify::tests::noise` (7 tests incl. `noise_absent_uses_floor`, `noise_bound_accepts_what_floor_refuses`, `noise_k_mismatch`): ok.
- `verify::tests::noise_per_set`: exactly 11 passed, 0 failed. Whole `cargo test -p aprender-decide`: 192 lib tests and every other target ok (python_records and fail_closed_vectors run unarmed here and SKIP; their armed runs are plan 15-04).
- `cargo clippy -p aprender-decide --lib --tests --examples`: no warning in aprender-decide (the only warnings are the two pre-existing `unused variable backend` in aprender-compute, which also make `-D warnings` fail there, unrelated).
- PARITY CONTRACT OK: `pv validate` 0 errors 0 warnings; `pv lint` (file form) PASS, 33 warnings before and after (no new finding). `pv diff` of the pre-edit contract against the edited file: `Contract diff: v2.0.0 -> v2.0.0 / Suggested bump: major` (pack_rescore_probs_abs formula and invariants changed, one proof obligation and FALSIFY-LAYA-PARITY-007 added), so the version is 3.0.0.
- A2 VALUES OK; SC1 ANCESTRY OK 4 commits (the 3 plan commits plus the 15-03 commit that touched binding.yaml/decide, all descend from 91969e11e); the four `14-DRIFT-EVIDENCE.json` unmeasured strings are verbatim in the text.
- `scripts/check_ont_ratchet.sh`: PASS.
- BINDING CLASS: the two Class A binding tests 2 passed, 0 failed; `rtk proxy make contract-audit-phase8` rc 0: `Phase 8 source resolution: resolved 45 Phase 8 binding rows to definition sites`, `Phase 8 binding audit: 4 contract(s) audited, no binding finding outside PHASE8_LIVE_EXEMPT=''`. It was also green on the base before any edit (precondition).
- `contracts/laya-finetune-gate-v1.yaml` and `scripts/laya_train/` byte-identical to dd3c96937 (`git diff --quiet`). `scripts/laya_train/contract.py noise_policy()` still reads `(4.0, 1e-05, 0.001)` from the amended contract.

## Mutation table (Discipline 4 and 7)

Each mutation was a temporary edit of verify.rs, applied by a script, the named test run `--exact`, and verify.rs restored from a saved copy (`git diff` on verify.rs is empty after the run; the full suite was re-run green after).

| # | Mutation | Test that went RED | Failing assertion |
|---|----------|--------------------|-------------------|
| m1 | enforced bound uses the stamp fields (`floor.max(policy.rescore_noise_k * noise)`) | `noise_per_set_graviton_cells` | tests.rs `G-motiv-zs must be ACCEPTED under the signed policy: ... RescoreDrift which=zero_shot row=3 max_abs=1.7315e-5 bound=0.00001` |
| m2 | `rescore_set` returns the zero-shot parameters for fine_tuned | `noise_per_set_fields_not_swapped` | `4e-5 on fine_tuned is inside its 5e-5 floor: RescoreDrift { which: FineTuned, ... bound: 3e-5 }` |
| m3 | per-set ceiling check removed (`if false && ...`) | `noise_per_set_ceiling` | `expected a refusal, got VerifyReport { ... }` |
| m4 | absent-record path uses floor_s | `noise_per_set_absent_record_floor` | `assertion left == right failed` (bound is not float_tolerance) |
| m5 | `check_rescore_policy(policy)?` removed | `noise_per_set_policy_invalid` | `floor below float_tolerance: refused before any bound is derived: [RescoreBound { ... }]` |
| m6 | per-set ceiling compared against the stamp, not the enforced bound | `noise_per_set_ceiling` | `expected a refusal, got VerifyReport { ... }` (stamp 4e-5 passes the 1e-4 ceiling, only the enforced 1.6e-4 fails) |
| m7 | per-set ceiling comparison strict (`enforced < ceiling_s`) | `noise_per_set_ceiling_inclusive` | `a ceiling equal to the enforced bound accepts: RescoreBoundCeiling { bound: 0.00016000000000016001, ceiling: 0.00016000000000016001 }` |
| m8 | drift comparison in `rescore` strict (`delta < tol`) | `noise_per_set_drift_inclusive` | `tol equal to the exact delta accepts: RescoreDrift { row: 0, max_abs: 2.9996037483215332e-5, bound: 2.9996037483215332e-5 }` |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug in my own test design] Per-set drift tests tripped the metric clause**
- **Found during:** Task 2 first run (`noise_per_set_fields_not_swapped`, `noise_per_set_boundary`)
- **Issue:** a 4e-5 to 5.5e-5 shift of one probability on the fixture also moves the reported NLL by about 1.4e-5, so `ReportedMetricMismatch` refused after a passing re-score. The re-score rule was right; the test measured the wrong clause.
- **Fix:** the new tests run under `a2_base()` (the permissive contract policy with `metric_recompute_abs` widened to 1.0), documented in its doc comment; the metric clause keeps its own tests. The Graviton-cell drifts (1.5e-5 to 2.7e-5) did not trip it, but the whole family uses the same base for uniformity.
- **Files modified:** crates/aprender-decide/src/verify/tests.rs
- **Commit:** 8e1176c6c

### Notes (no deviation)

- `G-motiv-ft` is an upper-bounded record bound (no observed drift row), so the Graviton table replays the three observed-drift cells; the fourth is stated in the A2 text with its recorded noise.
- `RescorePolicyInvalid` is checked first in `rescore_bounds`, before the absent-record early return (stricter than the minimum: a bad per-set policy is refused even when no record exists). `fail_closed_vectors` uses the real, valid contract, so its documented refusals are unchanged.
- `contracts/contracts.nt` was not regenerated (the plan did not call for it; plan 15-04 regenerates it). `just laya-train-selftest` was not re-run (the trainer, the train image and the gate contract are byte-identical; only `contract.py noise_policy()` was exercised directly).

## Authentication gates

None.

## Known Stubs

None.

## Threat Flags

None beyond the plan's threat model (T-15-10 absent-record floor shown by m4; T-15-11 ceiling and policy invariant shown by m3, m5, m6; T-15-12 SC1 ancestry; T-15-13 old records stay valid, `noise_per_set_stamp_unchanged` and the real motivation-v1-sm pack). No dependency added, no network surface.

## State updates

See the orchestrator note: `gsd_run query state.*` verbs and the ROADMAP progress verb were attempted after this file was written.

## Self-Check: PASSED

- Commits 6a13fac43, 8e1176c6c, 05421e2c6 are ancestors of HEAD; the six modified files exist; `A2 VALUES OK; SC1 ANCESTRY OK` re-run after the last commit.
