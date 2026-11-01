---
phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation
verified: 2026-11-01T06:30:00Z
status: gaps_found
score: 4/5 must-haves verified
covered_files:
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-01-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-01-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-02-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-02-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-03-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-03-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-04-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-04-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-05-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-05-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-06-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-06-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-07-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-07-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-08-PLAN.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-08-SUMMARY.md
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-BOUND-DECISION.json
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GATE-RUN-EVIDENCE.json
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-OFFLINE-GATE-EVIDENCE.json
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-REPACK-EVIDENCE.json
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-REPRODUCTION-DECLARATION.json
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-ROLL-EVIDENCE.json
  - .planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-RUN-GO.json
  - contracts/decide-apr-v1.yaml
  - contracts/laya-parity-v1.yaml
  - crates/aprender-decide/src/verify.rs
  - justfile
  - scripts/laya_deploy/laya_eligibility.py
covered_digest: "v4:sha256:eb1f5175b89bf56e301e24bdb73cb82a6cffd2244fd8837ab7d3bcd0b9ad033e"
behavior_unverified: 0
overrides_applied: 0
re_verification: false
gaps:
  - truth: "SC4 / Phase 14 truth 12 (D-16): a fresh owner-run SageMaker reproduction of motivation-v1 under a new committed declaration passes the gate and lands in the owner-approved band (margin [0.30, 0.45], ece_post [0.07, 0.12]) with an accepted, deploy-eligible .apr from the Graviton pack task; the gate-run evidence records next_trusted true"
    status: failed
    reason: "The single permitted attempt was consumed and judged NOT REPRODUCED. Shipped-seed (13) margin 0.27953446837732215 is below the 0.30 floor (by 0.0205). The gate passed, ece_post 0.0973 is in band, the pack was accepted and deploy-eligible, but the margin clause of the band is unmet, so next_trusted is false. Attempt cap 1, no waiver, no override; nothing is pre-authorized after a miss (D-12, D-13)."
    artifacts:
      - path: ".planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-GATE-RUN-EVIDENCE.json"
        issue: "verdict not-reproduced; next_trusted false; evaluation[0].defect_reasons = [out-of-band: margin 0.27953446837732215 is outside [0.3, 0.45]]"
    missing:
      - "A margin in [0.30, 0.45] on the shipped seed from an owner-approved, committed declaration (new attempt needs the owner's fresh approval and a new declaration; the epochs 12 -> 4 lever moved the shipped margin 0.2860 -> 0.2795, i.e. did not help)"
      - "next_trusted true in a Phase 15 gate-run evidence (or an owner-signed change to the band at a new declaration checkpoint, committed before any paid run; not an override)"
deferred: []
human_verification: []
---

# Phase 15: Laya pack-bound amendment per study (D-15) and the motivation-v1 re-reproduction (D-16) Verification Report

**Phase Goal:** Carry out the owner's D-15 decision (amend-per-study) and then close D-16: amend the pack/verify re-score bound in `contracts/laya-parity-v1.yaml` per set (fine_tuned, zero_shot), argued from `14-DRIFT-EVIDENCE.json`, with the owner's sign-off before any contract edit and a stated role for the local `laya-verify`; then run a fresh owner-run SageMaker reproduction of motivation-v1 that lands in the D-16 band with an accepted, deploy-eligible `.apr` from the Graviton pack task before the Phase 14 training server is trusted.
**Verified:** 2026-11-01
**Status:** gaps_found
**Re-verification:** No, initial verification

## Verdict in one paragraph

The amendment half of the goal is achieved and independently re-checked: the owner signed off before any contract edit, the contract carries per-set A2 parameters with a stated role for the local verify, the justfile/helper make that role real and sha256-bound, and a gate-passing CUDA checkpoint produced an accepted, deploy-eligible `.apr` from the Graviton pack task under the amended bound (twice: a re-pack of the Phase 14 checkpoint, and the Phase 15 run's own pack). The D-16 half is NOT achieved. The one permitted owner-run attempt was consumed and missed the declared margin band (0.2795 < 0.30); `next_trusted` is false and the Phase 14 training server remains untrusted. This is an honest negative result that the plans were designed to record, and it is not rounded into a pass.

## Goal Achievement

### Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| SC1 | Per-set (fine_tuned, zero_shot) amendment of the re-score bound in `laya-parity-v1`, argued from `14-DRIFT-EVIDENCE.json`, signed off by the owner at a `checkpoint:decision` before the contract is edited | VERIFIED | `15-BOUND-DECISION.json` (recorded_by orchestrator, owner reply `sb-recommended role-drift-advisory`, params fine_tuned k4/floor 5e-5/ceiling 1e-3, zero_shot k4/floor 3e-5/ceiling 1e-3). Sign-off commit `91969e11e` is dated 2026-10-31 12:48 PDT; the first contract edit (`6a13fac43`) is 13:55 PDT and the contract text (`05421e2c6`) 14:06 PDT, so no edit precedes the sign-off. `contracts/laya-parity-v1.yaml` lines 138-157 hold the six per-set constants matching the signed values exactly; the amendment text (lines 67-104) cites the Graviton evidence (1.7285e-05 vs noise 1.7265e-06 etc.) and the four `unmeasured` limits verbatim. `crates/aprender-decide/src/verify.rs` reads the floors from `parity.constants.*` (not hardcoded). `pv validate contracts/laya-parity-v1.yaml` and `contracts/decide-apr-v1.yaml`: 0 errors, 0 warnings (run by me). Suites cited from 15-02/15-04 evidence (`15-OFFLINE-GATE-EVIDENCE.json`: verify suite rc 0, claims OK 147 rows, readme_contract 16 passed), not re-run. |
| SC2 | The amendment states the role of the local `just laya-verify` (Mac vs Graviton disagree on the same bytes) | VERIFIED | Contract text lines 97-104: signed `role-drift-advisory`; a local RescoreDrift refusal is advisory and logged as a cross-CPU finding when a valid Graviton pack record exists for the exact sha256; every other refusal still blocks; no record keeps the local verify as the gate. Not doc-only (D-10): `justfile` laya-deploy step (5) (lines ~2517-2560) calls `laya_eligibility.py check-record --apr-sha256 "$H"` then `judge`; `laya_eligibility.py --selftest` re-run by me: `ELIGIBILITY SELFTEST OK 65 cases`. On the real Phase 15 record, `check-record` ACCEPTS with the true sha256 (`42efa308...`) and REFUSES a mismatched sha256 (`differs from the local file's sha256`), confirming D-09 byte-binding. The shipped `.apr` on disk hashes to `42efa308c305d3618fa0a7d69a8cd4b1dd03a354875bbe29568f3faa0b94feab`, equal to the pack record's `apr_sha256`. `CLAUDE.md` lines ~250-254 state A2 and the role. |
| SC3 | A gate-passing CUDA-trained checkpoint yields an accepted, deploy-eligible `.apr` from the Graviton (arm64 Fargate) pack task under the amended bound (Phase 14 truth 13) | VERIFIED | Two independent instances. (a) `15-REPACK-EVIDENCE.json`: Phase 14's gate-passing motivation-v1-sm checkpoint re-packed on the rolled pack image `sha256:dd38865c...`: accepted, deploy_eligible true, zero_shot 1.7285e-05 vs bound 3e-05 (the value A1 refused), fine_tuned 6.97e-06 vs 5e-05, argmax 100/100. Caveat recorded by the plan: this went through the drift measurement prefix, not the state machine (RESEARCH assumption A6). (b) The Phase 15 run's own state-machine pack task: `models/decide/motivation-v1-p15/logs/pack-outcome.json` (read by me) `ran true, accepted true, deploy_eligible true`, image `sha256:dd38865c...`, parity contract `466891fa...` which equals `shasum` of the on-disk `contracts/laya-parity-v1.yaml`; `rescore_max_abs 5.87e-06` vs 5.0e-05, `zs_rescore_max_abs 1.7285e-05` vs 3.0e-05; the local Mac `laya-verify` also accepted (no cross-CPU finding). The evaluator marks `must.deploy_eligible` true. |
| SC4 | After SC1 lands, a fresh owner-run reproduction under a new committed declaration passes the gate AND lands in the owner-approved band (margin [0.30, 0.45], ece_post [0.07, 0.12]) with an accepted, deploy-eligible `.apr`; Phase 15 gate-run evidence records `next_trusted` true; no waiver/override | FAILED (BLOCKER) | See "D-16 outcome" below. margin 0.27953446837732215 is outside [0.30, 0.45]. `15-GATE-RUN-EVIDENCE.json`: `verdict not-reproduced`, `next_trusted false`, `waived null`, `pilot_waiver null`. |
| SC5 | Every AWS command owner-run with the bang prefix; each paid run approved by the owner | VERIFIED | `owner_commands` in `15-GATE-RUN-EVIDENCE.json` and `15-REPACK-EVIDENCE.json` all begin with `!`; replies recorded on disk by the orchestrator (`recorded_by: orchestrator`) from the owner's pasted output (task id, `POLL DONE completed`, `FETCH OK ... apr 42efa308...`, hygiene read-back). Paid-run approval: see the note on `15-RUN-GO.json` below. No AWS/CDK/train call was made by this verification. |

**Score:** 4/5 roadmap success criteria verified; 0 present-but-behavior-unverified.

### D-16 outcome (the key finding, stated plainly)

Cross-checked against the fetched artifacts on disk, not only the evidence JSON:

| Quantity | Declared band / expectation | Observed | Source checked by me | In band |
| --- | --- | --- | --- | --- |
| shipped seed margin | [0.30, 0.45] | 0.27953446837732215 (seed 13) | `models/decide/motivation-v1-p15/run/gate-report.json` `margin`; `logs/train.log` `RESULT ... margin=0.2795` | NO (short by 0.0205) |
| shipped seed ece_post | [0.07, 0.12] | 0.09729856818914415 | gate-report `seeds.per_seed[13]` | yes |
| gate | pass | `GATE PASS` | `logs/train.log` last line; gate-report `pass true` | yes |
| device | `cuda` prefix | `cuda:0` | `logs/outcome.json` | yes |
| recipe_id, inputs sha256, seeds | as declared | all match (`recipe_id a9f1c4a7...`, seeds [13,17,23]) | `evaluation[0].must` all true | yes |
| deploy_eligible (pack task) | true | true, accepted | `logs/pack-outcome.json` | yes |
| next_trusted | true required | **false** | `15-GATE-RUN-EVIDENCE.json` | NO |

The attempt was properly consumed (exit_class `pass`, gate report and `GATE PASS` line present), so cap 1 applies and no retry is available under the declaration (D-12). Per-seed note: seeds 17 (margin 0.3122, ece 0.0923) and 23 (margin 0.3955, ece 0.1096, failed the 0.10 ECE bar) would not have been the shipped seed under the declared `median_ece` policy; seed 13 was the median-ECE seed, and the declared band is judged on the shipped seed. Reporting seed 17's margin as a pass would be rounding a miss into a pass and is not done here. The epochs 12 -> 4 lever (D-14) did not move the margin in the useful direction (attempt 1 shipped margin 0.2860 -> 0.2795), and 15-08 correctly records the run as not-reproduced.

### Truth 12 / truth 13 inheritance

| Inherited truth | Status | Note |
| --- | --- | --- |
| Phase 14 truth 13 (gate-passing CUDA checkpoint yields deploy-eligible `.apr` from the Graviton pack task) | VERIFIED | SC1-SC3 above. |
| Phase 14 truth 12 (D-16 reproduction lands in band, server trusted) | FAILED | SC4. Phase 14 training server stays untrusted. |

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `contracts/laya-parity-v1.yaml` | A2 per-set constants + role text | VERIFIED | Six constants equal the signed values; `pv validate` clean; file sha256 `466891fa...` is the value the pack task recorded. |
| `contracts/decide-apr-v1.yaml` | Pack record as eligibility authority (4.0.0) | VERIFIED | `pv validate` clean; helper and recipe match. |
| `crates/aprender-decide/src/verify.rs` | Per-set bound enforced from contract constants | VERIFIED (wired) | `RescoreSetPolicy`/per-set floors read from `parity.constants`; real-run `PACKED` line prints `rescore_bound=0.00005 zs_rescore_bound=0.00003`, i.e. the enforced per-set values reached the production image. |
| `justfile` laya-deploy/laya-upload step (5) | sha256-bound eligibility from the pack record | VERIFIED | Calls `laya_eligibility.py check-record/judge`; selftest 65 cases pass; dry-run laya-upload on the real artifact printed `ELIGIBLE pack-record sha256 42efa308...`, 0 AWS calls (`local_verify` in the gate-run evidence). |
| `scripts/laya_deploy/laya_eligibility.py` | Record check, byte binding, role judge | VERIFIED | Selftest and live accept/mismatch checks run by me. |
| `15-BOUND-DECISION.json` | Owner sign-off | VERIFIED | Present, orchestrator-recorded, commit `91969e11e` precedes contract edits. |
| `15-REPRODUCTION-DECLARATION.json` | Committed before the paid run | VERIFIED | sha256 `1471c106...` equals the file at commit `c078910ba` (2026-10-31 21:40 PDT = 04:40Z); the train task was created 04:58:20Z. |
| `15-GATE-RUN-EVIDENCE.json` | Run evidence incl. `next_trusted` | VERIFIED as a record; records a miss | Consistent with every on-disk artifact I read (gate-report, pack-outcome, outcome.json, .apr sha256). |
| `15-REPACK-EVIDENCE.json`, `15-ROLL-EVIDENCE.json`, `15-OFFLINE-GATE-EVIDENCE.json` | SC3 / image roll / offline gate | VERIFIED | Cited, spot-checked (see above). |

### Key Link Verification

| From | To | Via | Status |
| --- | --- | --- | --- |
| `laya-parity-v1.yaml` per-set constants | `aprender-decide` verify | `parity.constants.pack_rescore_*_{fine_tuned,zero_shot}` | WIRED |
| `laya-parity-v1.yaml` sha256 | pack task record | `parity_contract_sha256 466891fa...` equals on-disk hash | WIRED |
| Pack task `pack-outcome.json` (`apr_sha256`) | `laya-deploy`/`laya-upload` eligibility | `laya_eligibility.py check-record --apr-sha256` | WIRED (accept and mismatch both exercised) |
| Pack image `dd38865c...` | deployed pack task definition | `aprender-laya-pack-dev:2` (REPACK evidence `deploy`) and `pack_image_digest` in the run's pack record | WIRED |
| Declaration (committed) | evaluator | `recipe_id`/inputs/seeds/band compared in `evaluation[0]` | WIRED |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| --- | --- | --- | --- |
| Eligibility helper selftest | `python3 -I scripts/laya_deploy/laya_eligibility.py --selftest` | `ELIGIBILITY SELFTEST OK 65 cases` | PASS |
| Pack record accepts matching bytes | `check-record --record .../pack-outcome.json --apr-sha256 42efa308...` | `PACK RECORD OK` | PASS |
| Pack record refuses other bytes | same with all-zero sha | `REFUSED eligibility: ... differs from the local file's sha256` | PASS |
| `.apr` hash equals record | `shasum -a 256 motivation-v1-p15.apr` | `42efa308...` | PASS |
| Contracts validate | `./target/release/pv validate` on both | 0 errors, 0 warnings | PASS |
| Contract hash equals pack record's | `shasum -a 256 contracts/laya-parity-v1.yaml` | `466891fa...` | PASS |
| Full cargo/verify suites | not re-run (constraint); cited from `15-OFFLINE-GATE-EVIDENCE.json` and 15-02/15-04 summaries | rc 0 per evidence | CITED, not independently re-run |

### Requirements Coverage

REQUIREMENTS.md contains no Phase 15 IDs (ROADMAP: `Requirements: TBD`). Plan frontmatter `requirements:` lists phase-local decisions D-01..D-17 from 15-CONTEXT.md; these are mapped to the criteria above rather than to REQUIREMENTS.md rows. No orphaned REQUIREMENTS.md IDs. D-02 (D-16 not waived): honoured, no `overrides:` entry, `waived null`. D-11 (band unchanged): honoured, band identical to Phase 14's. D-12 (attempt cap 1): honoured, the miss was recorded not-reproduced.

### Anti-Patterns Found

No debt markers examined as blockers. `deferred-items.md` records one pre-existing contract-hygiene defect (two `llm-endpoint-openai-conformance-v1` obligations and one `motivation-enrichment-v1` equation) that plan 15-04 repaired inline with the user's authorization (commits `49a8df031`, `9b5e17101`); this is out-of-scope contract editing, disclosed and resolved, not a finding against the phase goal.

### Observations (non-blocking)

1. **15-RUN-GO.json.** The owner's literal reply is `go`, in direct answer to the prompt `Say "go" when you want plan 15-08 to run`, recorded by the orchestrator (not an executor) with the declaration's commit and sha256. In context that is an unambiguous run authorization, and it is corroborated by the owner themselves executing the bang-prefixed train command. The plan's own verify (substring `run-go` in the reply) did not match, and the record says so openly instead of padding it. I accept it on its merits; no gap.
2. **SC3 evidence (a)** used the drift prefix rather than the state machine; evidence (b), the real run's own pack, is the stronger, direct instance, so SC3 does not rest on assumption A6.
3. **Bound claim limits.** The amendment is argued from Graviton evidence only; x86_64, Graviton generation, a fresh A10G run, and `pack_laya` reporting only the first drifted set remain unmeasured by design (D-07), and the region between the enforced bound and the 1.4e-2 smallest cited regression is unmeasured (stated in the contract). The zero_shot Graviton drift is the same 1.7285e-05 on both the attempt-1 re-pack and the Phase 15 run, consistent with D-04 determinism.
4. **Billable seconds** for the Phase 15 run were not obtained (`billable_seconds null`, noted in evidence); report-only, not a must-have.
5. **Other phases.** `04-VERIFICATION.md` carries an out-of-set status; left untouched, noted only as instructed.

### Human Verification Required

None. The remaining decision is the owner's, not a verification item: whether to make another attempt (new declaration, new approval; nothing is pre-authorized, D-13) or to change the band at a new declaration checkpoint. Neither is a human check of existing evidence.

### Gaps Summary

One gap, a BLOCKER for the phase goal: **SC4 / Phase 14 truth 12 (D-16)** is unmet. The owner-run reproduction under the epochs-4 declaration was consumed and judged NOT REPRODUCED: shipped-seed margin 0.27953 outside [0.30, 0.45] (ece_post 0.0973 in band; pack accepted and deploy-eligible), `next_trusted` false, so the Phase 14 training server must not be treated as trusted. Everything the amendment half promised (SC1, SC2, SC3, SC5) holds and was re-checked against the contract, the helper, the justfile and the fetched run artifacts. Closing the gap needs an owner decision and a new committed declaration (the epochs lever did not raise the margin, so a different lever or a data change under D-14, or an owner-signed band change, is required), with a fresh paid-run approval; closure plans via `/gsd-plan-phase --gaps` should treat the margin shortfall, not the pack bound, as the root cause.

---

_Verified: 2026-11-01_
_Verifier: Claude (gsd-verifier)_
