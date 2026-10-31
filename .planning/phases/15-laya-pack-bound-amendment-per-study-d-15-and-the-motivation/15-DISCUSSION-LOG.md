# Phase 15: Laya pack-bound amendment per study (D-15) and the motivation-v1 re-reproduction (D-16) - Discussion Log

> **Audit trail only.** Not consumed by downstream agents. Decisions are in `15-CONTEXT.md`.

**Date:** 2026-10-31
**Phase:** 15
**Areas discussed:** What the bound must protect; Role of local laya-verify; New D-16 declaration terms; Compute sequencing and candidates

---

## What the bound must protect

| Question | Options presented | Selected |
|---|---|---|
| Authority host | Graviton pack task only (Recommended) / Both hosts must accept / You decide | Graviton pack task only |
| Bound shape | Keep formula, per-set parameters (Recommended) / Fixed absolute per set / Add cross-host term / You decide | Keep formula, per-set parameters |
| Red line | A real porting regression (Recommended) / Anything above measured Graviton spread / You decide | A real porting regression |
| Evidence level | Existing evidence only (Recommended) / Add cheap Graviton measurement / Fix pack_laya to report every drifted set | Existing evidence only |

## Role of local laya-verify

| Question | Options presented | Selected |
|---|---|---|
| Role | Advisory, Graviton verdict gates (Recommended) / Keep as hard gate on this Mac / Remove from deploy path | Advisory, Graviton verdict gates |
| Replacement deploy check | pack-outcome.json + sha256 match (Recommended) / Also re-run local verify, warn / You decide | pack-outcome.json + sha256 match |
| Scope | State it and change the recipes (Recommended) / State it only | State it and change the recipes |

## New D-16 declaration terms

| Question | Options presented | Selected |
|---|---|---|
| Band | Keep unchanged (Recommended) / Change with reasons / Decide at checkpoint | Keep unchanged |
| Attempts | Cap 1, no retry (Recommended) / Cap 2 retry only on infra failure / Cap 2 as Phase 14 | Cap 1, no retry |
| On a miss | Record not-reproduced, untrusted (Recommended) / Pre-authorize data-growth phase / Decide at that time | Record not-reproduced, untrusted |
| Run config | Identical inputs (Recommended) / Allow documented training changes / Investigate MPS-vs-CUDA gap first | Allow documented training changes |
| Which changes | Hyperparameters / Seed set / Eval-set size / Training data (multi-select) | All four |

*Note:* the owner chose to allow training-data changes, which edges toward the dataset-growth mechanism. Recorded in CONTEXT D-14 as bounded to what the declaration lists, with the general mechanism deferred.

## Compute sequencing and candidates

| Question | Options presented | Selected |
|---|---|---|
| Re-pack motivation-v1-sm | Yes, after the amendment lands (Recommended) / No | Yes, after the amendment lands |
| Truth 8 cancel check | Leave out of Phase 15 (Recommended) / Optional owner-run task / Make it a criterion | Leave out of Phase 15 |
| Ordering | Bound sign-off, edit, re-pack, declaration, run (Recommended) / Combined checkpoint | Bound sign-off, edit, re-pack, declaration, run |

## Claude's Discretion

- Concrete k, floor and ceiling per set; `pack-outcome.json` fields and sha256 comparison; proposed training changes.

## Deferred Ideas

- Dataset-growth and annotation mechanism; truth 8 cancel check; `pack_laya` all-sets reporting, x86_64 image; review findings triage; MPS-vs-CUDA gap.
