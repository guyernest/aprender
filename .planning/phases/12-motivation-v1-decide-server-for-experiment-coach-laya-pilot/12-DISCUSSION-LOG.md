# Phase 12: motivation-v1 decide server for experiment-coach (pilot, augury-dev) - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-25
**Phase:** 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
**Areas discussed:** Bake-off rules, Guide → task.json, Split & metrics on 395 rows, Thin-data gate failure
**Todo cross-reference:** `spike-laya-calibration-slice-and-temperature-cap.md` — offered "Keep it deferred (Recommended)" / "Fold it in"; user chose **Fold it in**.

---

## Bake-off rules

| Question | Options | Selected |
|---|---|---|
| Winner metric | False-assignment floor then macro-F1 (Rec.) / Macro-F1 only / Owner picks from table | False-assignment floor then macro-F1 |
| Must winner be servable | Laya serves, others reference (Rec.) / Ship whichever wins / Laya or SetFit, not Kev | Laya serves, others reference |
| Kev arm execution | Python offline, train shots (Rec.) / Rust path from spike branch / Drop Kev | **Rust path from spike branch** |
| Kev size | 4B (Rec.) / 0.8B / Both | 4B |
| False-assignment ceiling | Report-only for pilot / ≤10% at 0.6 (Rec.) / ≤5% at 0.6 | **Report-only for pilot** (so FA is a tiebreak, not a filter) |
| Spike branch | Use in place (Rec.) / Land in Phase 12 | Use in place |

## Guide → task.json

| Question | Options | Selected |
|---|---|---|
| Prefix sizing | Measure two variants, owner picks (Rec.) / Fit door 8 hard / Fullest 192, door drops | Measure two variants |
| Approval | Owner blocking checkpoint (Rec.) / Also Dr. Yuster | Owner checkpoint |
| Kev prompt | Full guide, train shots (Rec.) / Same condensed text / Both | Full guide, train shots |
| Output | Probabilities only (Rec.) / Discuss spans | Probabilities only |

## Split & metrics on 395 rows

| Question | Options | Selected |
|---|---|---|
| Split | 25% hold-out, 3 seeds (Rec.) / 20% / hold-out + 5-fold CV | 25% hold-out |
| Length bands | Eval tertiles (Rec.) / Fixed bands / Length-matched probe | Eval tertiles |
| Calibration slice | Set by folded spike (Rec.) / Back-office default | Set by folded spike |
| Source blind spot | State as unmeasured (Rec.) / Request mapping now | State as unmeasured |

## Thin-data gate failure

| Question | Options | Selected |
|---|---|---|
| ECE fail with margin pass | Halt for owner (Rec.) / Ship as gate-failed pilot / Ask Dr. Yuster first | **Ship as gate-failed pilot** |
| Training reweighting | No, file prior (Rec.) / Class-weighted loss | No |
| T clamp binding | Owner architectural decision (Rec.) / Pre-authorise raise to 10 | Owner decision |
| Hand-off reweighting recipe | Formula + both priors (Rec.) / Priors only | Formula + both priors |
| Follow-up: pack refuses gate-failed runs — mechanism | Declared pilot waiver in contract (Rec.) / Pack flag / Halt instead | Declared pilot waiver |
| Follow-up: does ship-anyway cover margin failure | No, ECE only (Rec.) / Yes, any failure | No, ECE only |

**Notes:** the follow-ups were raised because CLAUDE.md states Rust re-derives the gate from packed bytes before
anything is deployable, so "ship anyway" needs a declared mechanism.

## Claude's Discretion

Split seed and grouping rule; the condensed draft wording; where the waiver clause lives and the metadata field
name; the Kev split adapter and the pointer-head fit; the ECE bootstrap method; evidence file names.

## Deferred Ideas

Server-side prior correction; per-source results; evidence-span output; a hard false-assignment gate; serving
SetFit or Kev; landing the spike branch; thread context; held-out unseen posts; raising the T cap.
