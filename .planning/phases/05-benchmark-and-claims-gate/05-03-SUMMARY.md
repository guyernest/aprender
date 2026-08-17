---
phase: 05-benchmark-and-claims-gate
plan: 03
subsystem: training
tags: [setfit, calibration-regime, epsilon-basis, contract-gate, d-04, checkpoint, halted]

requires:
  - phase: 05-benchmark-and-claims-gate
    provides: "05-01's 12 banked calibration passes — the measurement bank every candidate is derived from"
  - phase: 05-benchmark-and-claims-gate
    provides: "05-02's per-regime RegimeThresholds / table_for restructure and the deliberate sole() tripwire"
  - phase: 05-benchmark-and-claims-gate
    provides: "05-14's fail-closed epsilon_basis derivation, which this plan turns from RED to GREEN"
provides:
  - "05-03-epsilon-basis-decision.md — the candidate lower-bound tables, derived by RUNNING the shipped combine, with the L1/L3 refutations and the D-04 options table (COMMITTED)"
  - "05-03-prepared-edit.patch — the complete three-place gate edit, prepared, verified, and NOT applied (awaiting D-04 approval)"
affects: [05-07, 05-09, 05-11, 05-12, 05-13]

tech-stack:
  added: []
  patterns:
    - "a candidate rule's verdict is obtained by CALLING the single fail-closed derivation with that rule, never by open-coding a second comparison"
    - "the bound is cited, the FACTOR is chosen — and the contract records which is which"
    - "a prepared-but-unapproved gate edit is preserved as a committed patch artifact, so the ceremony survives worktree teardown without the gate changing"

key-files:
  created:
    - .planning/phases/05-benchmark-and-claims-gate/05-03-epsilon-basis-decision.md
    - .planning/phases/05-benchmark-and-claims-gate/05-03-prepared-edit.patch
  modified: []

decisions:
  - "HALTED AT THE D-04 BLOCKING CHECKPOINT — a designed stop, not a failure. Task 1 is complete and committed; Task 2 requires an explicit human selection of the lower bound; Task 3 is deliberately not started. No contract, threshold-table, constant or test change is committed."
  - "The candidate tables were derived by RUNNING the shipped combine over the committed store (rc=101, the expected fail-closed refusal), never by hand arithmetic — threat T-05-03-05 names hand-derivation as the tampering vector this plan exists to prevent"
  - "epsilon_basis was WIDENED to take a named LowerBound rule rather than gaining a sibling function: a second function would have made `grep -c 'fn epsilon_basis'` return 2, and open-coding four candidate comparisons would have destroyed 05-14's one-comparison invariant by construction. Non-comment `lower < upper` is still exactly 1."
  - "The prepared edit is preserved as a COMMITTED PATCH rather than left in the working tree. The worktree is force-removed on return, so an uncommitted edit would be destroyed; a patch file changes no gate (nothing reads it, no include_str! parses it) and gives the checkpoint a hash-referenceable artifact — the same reasoning the plan gives for committing the memo."
  - "Recommended NON-BINDING: option A (L2 at the chosen 10x noise-floor factor, attention_key_bias ungated on a measured margin, frozen from the four-cell provisional basis). The strongest argument against it is stated in the memo rather than buried."
  - "mod.rs is NOT edited. It carries one `calibrated.len(), 1` assertion that the prepared edit turns red, but a parallel executor (05-05) owns that file this wave, so it is reported as a deviation with the exact patch instead."

metrics:
  duration: ~2h10m
  tasks_completed: 1
  files_changed: 2
  completed: 2026-08-17

actuals:
  tokens: 34000
  tasks: 1
  commits: 2

status: halted
---

# Phase 5 Plan 03: Epsilon Basis Decision — Task 1 complete, HALTED at the D-04 checkpoint

**The 10x/10x rule's replacement is now a decision with measured numbers attached rather than an
open question: three candidate lower bounds derived by running the shipped combine, two of them
refuted by arithmetic on the record, and a complete three-place gate edit prepared, verified
end-to-end, and deliberately NOT applied — awaiting the human's selection at D-04.**

## Status: HALTED at a BLOCKING HUMAN CHECKPOINT (designed stop)

This is not a blocker and not a failure. Plan 05-03 is `autonomous: false` and Task 2 is
`checkpoint:decision`. Task 1 is complete and committed; Task 3 has deliberately not begun.

**Nothing that changes the gate is committed.** The contract, the threshold table, the constants
and the tests are byte-identical at `HEAD` to what they were before this plan ran. The two
commits are both evidence artifacts.

## The measured result

Derived by RUNNING the shipped combine over the 12 committed passes — invocation and status in
the memo, status captured on its own line, never through a pipe:

```
APRENDER_CALIBRATION_COMBINE="s8:13,s8:31,s8:53,s64:13"  ->  COMBINE_RC=101
```

`rc=101` is the EXPECTED state: 05-14 made an empty basis a hard refusal, so the same four-cell
combine 05-01 recorded exiting `rc=0` now refuses. The report is written before the panic, which
is what makes the candidate tables readable.

| candidate | lower edge | windows | narrowest gated width |
|---|---|---|---|
| **L1** (D-03, being replaced) | `10 x max(worst_ctrl, worst_nnull)` | **5 of 6 EMPTY** | — |
| **L2-bare** (bare contracted clearance) | `noise_floor` | 6 of 6 EXIST | 101.94x |
| **L2-10x** (chosen factor) | `10 x noise_floor` | 6 of 6 EXIST | 10.19x |

L1's exceed factors reproduce 05-01 FINDING 1 to the digit (8.56x, 13.84x, 8.54x, 31.94x, 5.41x),
and the collapse holds within `s64` alone (39.7 against a rule needing > 100).

**L3 is refuted by arithmetic, not by taste:** largest admissible near-null factor **0.313**,
largest admissible factor PRODUCT **3.131** against the contracted 100, `projection_bias` binding.
Below 1 the margin is INVERTED — epsilon would sit under the near-null delta it must exceed.

**A measured result worth stating because its absence would have been invisible:** the four-cell
noise floors were re-derived from this run rather than carried forward from the s8 half, and they
COINCIDE — `s64:13` did not raise any class's floor (`5.960e-8` for all five dense classes in
every cell; embedding's worst is `1.779e-6` from `s8:53` against `s64:13`'s `1.741e-6`). Folding
the s64 cell in moved `best_real` but left `lower` alone.

## Tasks

| Task | Name | Status | Commit |
|---|---|---|---|
| 1 | Derive the candidates, commit the memo, prepare the edit | **COMPLETE** | `2f76df22f` (memo only) |
| 2 | D-04 checkpoint — the human SELECTS the epsilon basis | **AWAITING HUMAN** | — |
| 3 | Commit the approved edit and re-verify both ways | NOT STARTED (blocked by Task 2) | — |

## Task 1 acceptance criteria — all machine-checked

| Criterion | Result |
|---|---|
| memo exists and carries `CANDIDATE LOWER BOUNDS` | PASS (line 78) |
| the memo IS the last pre-checkpoint commit's whole content | PASS — `git log -1 --name-only` names it alone |
| that commit touches NEITHER the contract NOR `thresholds.rs` | PASS |
| both files nonetheless carry uncommitted edits (prepared, not skipped) | PASS — `git status --porcelain` shows ` M` on both |
| `grep -c 'fn sole'` in `thresholds.rs` | **0** |
| `grep -c 'frozen\.of('` in `evidence.rs` | **0** |
| `grep -c 'fn epsilon_basis'` | **1** |
| non-comment `lower < upper` in `evidence.rs` | **1** (05-14's invariant held across both plans) |
| `pv validate` via `cargo run` (never a hardcoded path) | **rc=0, 0 error(s), 0 warning(s)** |
| `thresholds` suite green AND non-vacuous | **rc=0, `30 passed`** (was 29 + the new envelope test) |

## Judgement-checked evidence

**`pv diff`, two filesystem paths, old revision materialized with `git show`:**

```
Contract diff: v2.0.0 → v2.0.0
Suggested bump: major
  ~ calibration_regime: invariants changed
  ~ evidence_gate: invariants changed
  ~ gradient_free_parameters: invariants changed
```

Taken as given — the bump is `pv`'s call. The prepared header records it verbatim along with why
MAJOR is right on the merits (the production regime's epsilons are not frozen under the 2.x
derivation rule, and two `attention_key_bias` prose claims are corrected rather than extended).

**Byte-identity control — and a measurement error caught while running it.** The first pass used a
bare `git diff -U0` and reported **19** deletion lines. The `rtk` hook rewrites `git diff` into a
lossy prose summary, so that count was taken on a rewritten stream. Re-run through `rtk proxy`,
the true count is **20** — the hidden line was `-  version: 2.0.0`. The conclusion is unchanged
(that line is in the metadata header, which is within the permitted set), but the first
measurement could not have supported it. Same artifact family as 05-01's `git log --oneline`
rewrite and 05-14's `test result:` rewrite.

All 20 deletions, on the raw stream: 1 metadata version line; 6 in the header's "ONE CLASS IS
DELIBERATELY NOT GATED" paragraph; 1 closing the clearance invariant; 2 in
`gradient_free_parameters`' "cancellation residue" sentences; 1 closing RE-DERIVATION; 2 in
"EXACTLY ONE FINGERPRINT"; 7 in "CONSEQUENCE FOR PHASE 5". Every one is deliberately amended
invariant prose or the metadata header.

Machine-checked on the raw stream: deleted fixture `calibrated_regimes` entry lines = **0**;
deleted `frozen_thresholds` field lines (`eps:`/`scale_floor:`/`sparse:`/`gated:`/
`embedding_delta_floor_value:`) = **0**. The fixture entry and its entire block are untouched.

**05-14's fail-closed derivation — RED before, GREEN after, both RUN:**

| state | invocation | rc |
|---|---|---|
| 05-01 recorded (pre-05-14) | four-cell combine | `0` (the D-18 defect) |
| after 05-14, before this plan | identical combine | `101` |
| **against the prepared table** | **identical combine** | **`0`** |

The green run's report states its own reason:

```
REGIME TABLE FOR THE DERIVED REGIME: resolved for
`minilm-slice-h384-l6-a12-i1536-v30522@1110a243|seeds=13,31,53|cells=s64e1b16,s8e1b16`
— gated classes: [embedding, layer_norm_weight, layer_norm_bias, projection_weight, projection_bias]
...
Every class above has a non-empty window (10x_lower < 10x_upper).
```

`attention_key_bias` appears in that table carrying `declared-ungated`, with its numbers intact —
the row is annotated, never removed.

**The per-regime association loop is LIVE and bites.** Proven by induced mutation rather than
asserted: perturbing the production `embedding` epsilon `1.8e-4` → `1.9e-4` turned
`thresholds_match_the_contract` RED, naming the production regime and the class
(`...|cells=s8e1b16,...,s64e1b16/embedding: epsilon`). Reverted; suite back to `30 passed`.

**`production_envelope_is_calibrated` is green and two-sided:** all 40 production ids (10 seeds x
4 cells, rendered through `RegimeCoordinates::render_run` — the run's own grammar) resolve the
PRODUCTION table; a fixture id still resolves the FIXTURE table; and three out-of-envelope
coordinates still resolve nothing, so the entry is a measured envelope rather than an
architecture-wide permit.

**Isolation:** `epsilon_basis` filter `9 passed` (05-14's guards intact); clippy
`--all-targets` rc=0 with zero findings in either edited file; `rustfmt` clean on both.

## What the prepared edit contains (NOT applied)

`05-03-prepared-edit.patch`, 1576 lines, verified faithful by
`git apply --check --reverse` → **rc=0**. Three files:

- **`contracts/setfit-train-lifecycle-v1.yaml`** — the production `calibrated_regimes` entry (all
  10 contracted seeds, all 4 cell labels); an additive `per_regime_thresholds` map carrying BOTH
  regimes' tables; four production-scoped invariants (the near-null bound's unsatisfiability with
  the table inline and the L3 arithmetic; which bound binds and that the factor is CHOSEN, with
  both factor columns printed; the weakening stated explicitly with the claim spelled out; and
  the MEASURED-vs-COVERED record with `PROSPECTIVE VALIDATION: NOT RUN — compute budget`); the
  ARCHITECTURE FINGERPRINT PROVENANCE note; the D-17 corrections; version 3.0.0 with the `pv diff`
  record.
- **`crates/aprender-train/src/train/setfit/thresholds.rs`** — `PRODUCTION_REGIME` +
  `production_regime()` (eps 1.8e-4 / 1.8e-5 / 7.1e-5 / 1.2e-4 / 3.4e-5, `attention_key_bias`
  null, floor 2.6e-4); `CALIBRATED_REGIMES` len 1 → 2 with full-list string equality preserved;
  `sole()` and its two wrappers DELETED; six test reads migrated to `table_for`; the new 40-cell
  envelope test.
- **`crates/aprender-train/src/train/setfit/evidence.rs`** — the `LowerBound` rule,
  `CONTRACTED_NEAR_NULL_LOWER_BOUND` (fixture, unchanged) and `PRODUCTION_LOWER_BOUND`; the
  report-only `CANDIDATE LOWER BOUNDS` renderer; the `EMBEDDING DELTA MEDIAN MIN` line; the five
  regime-less reads migrated; two 05-14 state-dependent tests migrated.

## Deviations from Plan

### 1. [Rule 2 — Missing critical] The run-level floor was the one frozen number the report did not derive

- **Found during:** Task 1 step (1), deriving the recommended table.
- **Issue:** the contract derives `embedding_delta_floor` from the smallest embedding-class
  MEDIAN, but the report printed only `EMBEDDING DELTA MIN`. The floor would have had to be
  hand-computed from the per-cell table — the exact hand-derivation T-05-03-05 exists to prevent.
- **Fix:** one accumulator and one report line, `EMBEDDING DELTA MEDIAN MIN across measured
  cells: 2.611e-3`, so `2.6e-4` is read off a run.
- **In:** the prepared patch (`evidence.rs`), not committed.

### 2. [Rule 3 — Blocking] `epsilon_basis` had to be widened rather than duplicated

- **Issue:** the plan requires four candidate emptiness determinations while 05-14 pins the
  comparison count at one, and a helper named `epsilon_basis_with` would have made
  `grep -c 'fn epsilon_basis'` return 2 — failing the plan's own criterion.
- **Fix:** widened the single function's signature with a named `LowerBound`; all call sites pass
  an explicit rule. Both counts remain 1.

### 3. [Rule 3 — Blocking] Two of 05-14's state-dependent tests asserted the production regime is uncalibrated

- **Issue:** `evidence_epsilon_basis_without_a_resolved_table_every_class_is_required` asserted
  `table_for(PRODUCTION_REGIME_TODAY).is_none()`, and
  `negative_uncalibrated_regime_is_refused_before_any_comparison` asserted
  `calibrated.len() == 1`. Both are true only until this plan lands.
- **Fix:** the first now derives under `UNCALIBRATABLE_REGIME` (red by construction, which is why
  05-14's own doc said the DURABLE test uses it) and asserts the state change POSITIVELY, so the
  transition is recorded by a test rather than by a test's disappearance; a control was added
  proving the production table's declaration is what moves the verdict. The second asserts 2 and
  gains a check that no calibrated entry carries the `minilm-full-` rendering — architecture is
  matched for equality, never by family.
- **In:** the prepared patch, not committed.

### 4. [SCOPE BOUNDARY — reported, NOT fixed] `mod.rs` carries one assertion the prepared edit turns red

- **Found during:** Task 1 step (3), running the `setfit::` suite against the prepared edit.
- **Issue:** `crates/aprender-train/src/train/setfit/mod.rs:1690`, in
  `regime_gate_an_unswept_seed_is_refused`, asserts `assert_eq!(calibrated.len(), 1, "exactly one
  calibrated entry")`. With the production regime calibrated this is `2`. Result:
  **`322 passed; 1 failed`** — the single failure, and it is entirely this literal.
- **NOT FIXED, deliberately.** The orchestrator's dispatch states that plan 05-05 is editing
  `mod.rs` concurrently in another worktree this wave, and instructs: "if a task appears to
  require editing mod.rs or the Makefile, treat it as a deviation and report it rather than
  editing them." The plan's own `files_modified` also excludes it.
- **The required change, exactly** (also worth making the two neighbours position-independent,
  since `calibrated[0]` now depends on list order):
  ```rust
  // mod.rs:1690
  -  assert_eq!(calibrated.len(), 1, "exactly one calibrated entry");
  +  assert_eq!(calibrated.len(), 2, "two calibrated entries since 05-03");
  // mod.rs:1692 and :1720 — prefer position-independent forms
  -  calibrated[0].contains("seeds=1,42,7"),
  +  calibrated.iter().any(|c| c.contains("seeds=1,42,7")),
  -  calibrated[0].contains("cells=s16e2b8,s8e1b4"),
  +  calibrated.iter().any(|c| c.contains("cells=s16e2b8,s8e1b4")),
  ```
- **Whoever applies the approved patch must apply this too**, or the `setfit::` suite stays red
  and Task 3's acceptance criteria cannot be met. It is a one-line semantic change plus two
  robustness improvements; it relaxes nothing.

### 5. [Rule 3 — Blocking] The prepared edit had to be preserved as a committed patch

- **Issue:** D-04 requires the gate edit to stay uncommitted until approved, but this executor
  runs in a worktree the orchestrator force-removes on return — an uncommitted edit is destroyed.
- **Fix:** `05-03-prepared-edit.patch`, committed. A patch file is not a gate change: nothing
  reads it, no `include_str!` parses it, no test consults it, and the gate's behaviour at `HEAD`
  is byte-identical to before. It is the same category as the memo the plan already authorizes
  committing, for the same stated reason — the checkpoint should review a hash-referenceable
  artifact rather than a working-tree file that can be silently rewritten.

### 6. [Measurement discipline] The byte-identity control was re-measured

Covered above: the first `git diff -U0` was taken on an `rtk`-rewritten stream and undercounted
deletions 19 vs 20. Re-run through `rtk proxy`. `git diff` written to the patch file was
rewritten the same way and produced a **non-applicable** summary — caught by
`git apply --check --reverse` returning 128, and fixed by regenerating through `rtk proxy`
(re-checked: rc=0). Both are the CLAUDE.md rule-1/rule-8 family, and both were caught by
verifying the measurement rather than the result.

## Out-of-scope discovery (NOT fixed)

`crates/aprender-train/src/train/setfit/apr_reload.rs:331` remains unformatted at HEAD —
pre-existing, untouched by this plan, and already logged by 05-02 and 05-14. `rustfmt` was run on
the two files this plan owns rather than `cargo fmt -p aprender-train`, so the pre-existing diff
was not silently absorbed into this plan's changes.

## Known Stubs

None. Every symbol introduced is reached by a test in the default suite, and the one report-only
renderer is exercised by the combine run quoted above.

## Threat Flags

None. No network endpoint, auth path, file-access pattern or trust-boundary schema was introduced.
The threat register is addressed rather than extended — T-05-03-05 in particular is discharged by
deriving every candidate through the shipped combine and by printing L3's admissible factors so
relaxation is refuted by arithmetic on the record.

## Downstream impact

**F-10 is NOT yet unblocked.** The prepared edit would unblock it, but nothing is applied.
**05-07**, **05-12** and (transitively) **05-09**, **05-11**, **05-13** remain blocked until the
D-04 selection is made and Task 3 runs.

This is distinct from option F: no basis has been rejected and nothing about the epsilon question
has been prejudged. The decision is available, fully evidenced, and awaiting one answer.

## Next Steps

1. Human selects at D-04: an option (A–F) **and** a factor (`10x` or bare).
2. If the selection matches the prepared edit: `git apply` the patch, apply the `mod.rs` change
   from deviation 4, re-verify both directions, and commit contract + `thresholds.rs` +
   `evidence.rs` as ONE commit (the memo commit is already in history and is not re-litigated).
3. If it differs: re-prepare, re-present the full bundle, and amend the memo by a follow-up commit
   recording the selection — never rewrite it in place.

## Self-Check: PASSED

| Claim | Check | Result |
|---|---|---|
| `05-03-epsilon-basis-decision.md` exists | `test -f` | FOUND |
| `05-03-prepared-edit.patch` exists and is faithful | `git apply --check --reverse` | FOUND, rc=0 |
| memo commit `2f76df22f` exists | `git log --format=%h -1` | FOUND |
| memo commit contains ONLY the memo | `git log -1 --name-only --format=` | FOUND, 1 file |
| no gate change committed | `git status --porcelain` shows contract/thresholds/evidence as ` M`, not staged | CONFIRMED |
| `pv validate` at the prepared state | `cargo run ... pv validate` | rc=0, 0 errors |
| four-cell combine GREEN against the prepared table | env-gated `--ignored` run | rc=0 |
| `thresholds` 30 passed, `epsilon_basis` 9 passed | `rtk proxy cargo test` | CONFIRMED |
| `setfit::` state fully characterised | `rtk proxy cargo test` | 322 passed, 1 failed (mod.rs only — deviation 4) |
