# Phase 15: Laya pack-bound amendment per study (D-15) and the motivation-v1 re-reproduction (D-16) - Context

**Gathered:** 2026-10-31
**Status:** Ready for planning

<domain>
## Phase Boundary

Carry out the owner's D-15 decision (amend-per-study) and then close D-16. First amend the pack/verify re-score bound in `contracts/laya-parity-v1.yaml` per set (`fine_tuned`, `zero_shot`), argued from `14-DRIFT-EVIDENCE.json`, with the owner signing off at a `checkpoint:decision` before any contract edit; state and implement the new role of the local `laya-verify`. Then run one fresh, owner-run SageMaker reproduction of motivation-v1 under a new committed declaration. Inherits Phase 14 truths 12 and 13 (see `15-INHERITED-FROM-14.md`). Truth 8 (the packing-stage cancel) is NOT in scope.

</domain>

<decisions>
## Implementation Decisions

### Already decided in Phase 14 (binding, not re-opened)
- **D-01:** Amend-per-study. The planner argues the per-set bound from `14-DRIFT-EVIDENCE.json`; the owner signs off at a `checkpoint:decision` before any contract edit. No contract edit precedes that sign-off. — **Reversibility:** one-way — the amendment changes a published provable contract (`laya-parity-v1`) that the pack task, `laya-verify` and downstream deploys re-read.
- **D-02:** D-16 is not waived and no `overrides:` entry stands in for it. The reproduction runs after the amendment lands, under a new declaration.
- **D-03:** Every AWS command is owner-run with the bang prefix; each paid run needs the owner's approval; owner replies are recorded on disk by the orchestrator, never relayed by an executor.

### The bound: what it protects
- **D-04:** The Graviton (arm64 Fargate) pack task is the authority the amended bound is argued against. It is deterministic across repeats (one distinct value on all five legs). The Mac result is informational, not an input to the bound.
- **D-05:** Keep the amendment-A1 formula, `max(floor, k x training-device noise)` with a ceiling, but allow its parameters (k, floor, ceiling) to differ per set (`fine_tuned`, `zero_shot`). This addresses the case where 4 x the motivation-v1-sm zero_shot A10G noise (1.7265105816122173e-06) sits below the 1e-5 floor. — **Reversibility:** one-way — same contract edit as D-01.
- **D-06:** The bound must still refuse a real porting regression (served probabilities drifting enough to change decisions, e.g. a layout or kernel bug, anything approaching the 1e-3 ceiling). Platform float noise below that is acceptable. The plan states the red line it argued against.
- **D-07:** The checkpoint argues from existing evidence only. No new paid compute and no new Graviton or A10G measurement before the sign-off. The four `unmeasured` strings in `14-DRIFT-EVIDENCE.json` (x86_64, Graviton generation, fresh A10G run, second drifted set) are stated as limits of the claim. Fixing `pack_laya` to report every drifted set is NOT in this phase.

### The role of the local `laya-verify`
- **D-08:** `just laya-verify` becomes advisory. The deploy and upload gate is the pack task's recorded verify (`pack-outcome.json`, accepted, `deploy_eligible` true). A Mac refusal is logged as a cross-CPU finding, consistent with the `must_note` already committed in `14-REPRODUCTION-DECLARATION.json`.
- **D-09:** The deploy and upload steps must also require that the local `.apr` sha256 equals the sha256 the pack task recorded, so the verdict is bound to the exact bytes.
- **D-10:** Phase 15 states the role in the contract text AND edits the `laya-deploy` and `laya-upload` justfile recipes so the tooling matches; it is not a doc-only change.

### The new D-16 declaration
- **D-11:** The owner-approved band stays unchanged: margin [0.30, 0.45], post-calibration ECE [0.07, 0.12]. It was set before the first run and is not moved after seeing attempt 1 (margin 0.2860488947445468, ECE 0.09636022716760634).
- **D-12:** Attempt cap 1, no retry. A miss is recorded as not-reproduced (`next_trusted` false), the server stays untrusted, and the owner decides what follows. A failure before any gate verdict (crash, quota, timeout) does not consume the attempt.
- **D-13:** On a miss, nothing is pre-authorized (no follow-up data-growth phase is committed in advance). The amendment criteria (Phase 15 success criteria 1 to 3) can be met independently of D-16.
- **D-14:** The new run may differ from attempt 1. The declaration may permit any of: hyperparameters (epochs, learning rate, batch size, warmup), the seed set, eval-set size, and training data. Constraints: every change is listed with concrete values in the declaration and committed before the paid run; the planner proposes the values and the owner approves them; the evidence file states that the run is no longer an identical reproduction of attempt 1 or of Phase 12, so its claim scope is worded accordingly. Training-data changes stay bounded to what the declaration lists; building a general dataset-growth or annotation mechanism is a separate phase (see Deferred).

### Compute sequencing
- **D-15:** Re-pack the already trained motivation-v1-sm under the amended bound (one Fargate pack, no GPU) after the amendment lands, to show an accepted deploy-eligible `.apr` from a CUDA checkpoint for criterion 3. It cannot close D-16 (its margin is out of band). Its S3 objects expire about 2026-11-28 (`sources.motivation.expires_about`), so this must run before then.
- **D-16:** Truth 8, the packing-stage cancel (about 62 min of paid compute), stays out of Phase 15. It remains open in `14-VERIFICATION.md` `human_verification`.
- **D-17:** Order: (1) bound `checkpoint:decision`, (2) contract and recipe edits, (3) re-pack motivation-v1-sm, (4) new D-16 declaration committed with the owner-approved training changes, (5) the single owner-run SageMaker attempt (about 1 hr of ml.g5.xlarge), (6) gate-run evidence with `next_trusted`.

### Claude's Discretion
- Concrete values for k, floor and ceiling per set, argued at the checkpoint (the owner signs, the planner proposes).
- The exact `pack-outcome.json` fields the deploy and upload recipes read, and how the sha256 comparison is implemented.
- Concrete proposed training changes for the declaration (D-14).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 15 inputs
- `.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-INHERITED-FROM-14.md` - the three Phase 14 truths verbatim, 47 measured facts with source file and JSON key, tree facts, open checkpoints, compute note
- `.planning/ROADMAP.md` - Phase 15 goal and the 5 success criteria (truth 12 and truth 13)
- `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-GAP-ROUND-2-OWNER-DECISIONS.md` - binding owner answers 1 to 3
- `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-VERIFICATION.md` - truths 12, 13 (deferred) and 8 (open)
- `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-CONTEXT.md` - D-15 (amendment is one-way), D-16, D-22

### Evidence
- `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-DRIFT-EVIDENCE.json` - the per-leg rows, A10G noise, Graviton spread, unmeasured items (the bound is argued from this)
- `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-DRIFT-DECISION-BRIEF.md` - the per-artifact table and the two options put to the owner
- `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-GATE-RUN-EVIDENCE.json` - attempt 1 verdict, per-seed margins, pack refusal line
- `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-REPRODUCTION-DECLARATION.json` - the Phase 14 declaration (band, retry, must, must_note) the new one replaces
- `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-REVIEW-DISPOSITION.md` - WR-01..03 and IN-01..09, open, not Phase 15 scope

### Contracts and code
- `contracts/laya-parity-v1.yaml` - the contract the amendment edits (amendment A1, `pack_rescore_noise_k`, `pack_rescore_bound_max_abs`, `float_tolerance`)
- `contracts/laya-finetune-gate-v1.yaml` - the gate the D-16 run is judged by
- `contracts/decide-apr-v1.yaml` - the served `.apr` artifact contract
- `crates/aprender-decide/examples/pack_laya.rs` - `pack_laya` (pack and verify; reports only the first drifted set)
- `justfile` - recipes `laya-verify`, `laya-deploy` (its step 5 ELIGIBILITY) and `laya-upload`, to be edited per D-08 to D-10

### Project rules
- `CLAUDE.md` - provable-contract workflow (`pv validate` and `pv lint`, never a bash substitute), compute check-in above 1 hr on non-lambda-vector hosts, and no modifying `.github/workflows`

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `pack_laya` (`pack`, `verify`): already computes the per-set re-score against the bound; the amendment changes the bound it reads, not its structure.
- Phase 14 gate-run and drift scripts and evidence files: the pattern for recording owner replies and a declaration before a paid run.

### Established Patterns
- A paid or one-way action sits behind a committed declaration and an owner-recorded reply; the orchestrator records owner replies on disk.
- Contract edits go through `pv validate` and `pv lint`.

### Integration Points
- The Graviton pack task's `pack-outcome.json` is what the evaluator and (after D-08) the deploy and upload recipes read.
- The training server stays untrusted (`14-GATE-RUN-EVIDENCE.json` `next_trusted` false) until Phase 15 produces a D-16 pass.

</code_context>

<specifics>
## Specific Ideas

- Two observed disagreements motivate D-04 and D-08: smoke-stance-64 fine_tuned is 1.05528 x the bound on the M4 Pro (refused) and 0.843124 x on Graviton (accepted); motivation-v1-sm zero_shot was refused in the pack task at 1.7285e-05 against 1e-05.
- Attempt 1 also missed the band on margin itself (0.286 < 0.30), so amending the bound alone may not make D-16 pass; D-14 allows documented training changes for that reason.

</specifics>

<deferred>
## Deferred Ideas

- A general dataset-growth and annotation-quality mechanism (the owner's attempt-1 note: "a mechanism to grow the dataset and improve annotation quality"): its own phase; D-14 permits only the bounded training-data change the declaration lists.
- Truth 8 packing-stage cancel live check (~62 min paid): remains an owner candidate in `14-VERIFICATION.md`.
- `pack_laya` reporting every drifted set (unmeasured[3]), x86_64 pack image, Graviton generation visibility: not planned.
- Review findings WR-01, WR-02, WR-03 and IN-01 to IN-09: open, awaiting owner triage.
- Explaining the MPS-versus-CUDA accuracy gap: not scheduled.

### Reviewed Todos (not folded)
- `todo.match-phase` returned keyword-only matches (e.g. GPU ledger tests, Qwen3.5 forward) unrelated to this phase; none folded.

</deferred>

---

*Phase: 15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation*
*Context gathered: 2026-10-31*
