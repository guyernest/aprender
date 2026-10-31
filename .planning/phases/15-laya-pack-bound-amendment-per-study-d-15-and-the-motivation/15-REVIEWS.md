---
phase: 15
reviewers: [codex]
reviewer_instances: [fable]
reviewed_at: 2026-10-31T16:59:59Z
plans_reviewed: [15-01-PLAN.md, 15-02-PLAN.md, 15-03-PLAN.md, 15-04-PLAN.md, 15-05-PLAN.md, 15-06-PLAN.md, 15-07-PLAN.md, 15-08-PLAN.md]
models:
  codex: "gpt-6.1-sol (reasoning=high)"
  fable: "claude-fable-5-1 (reasoning=high)"
model_sources:
  codex: "banner"
  fable: "pinned"
---

# Cross-AI Plan Review — Phase 15

<!-- gsd:plan-revision-conflicts:begin -->
## Plan-Revision Conflicts
<!-- gsd:plan-revision-conflicts:end -->

## Codex Review

Reviewed **8/8 plans** against repository sources. No repo files were edited, and no AWS commands, skills, workflows, or other reviewers were invoked.

## 15-01

**Summary:** The decision brief provides good evidence traceability and approval sequencing. Its custom-parameter validation and safety-margin wording need tightening before owner sign-off.

**Strengths**

- Preserving A1 stamp values addresses a real compatibility constraint: Rust compares the record’s `k` and `floor_abs` bit for bit against policy. [verify.rs:2240](/Users/guy/Development/machine-learning/aprender/crates/aprender-decide/src/verify.rs:2240)
- The plan distinguishes measured values from the inferred upper bound and makes the signed brief identifiable by hash. [15-01-PLAN.md:292](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-01-PLAN.md:292)

**Concerns**

- **MEDIUM:** The grid judges acceptance solely by `drift <= bound`; it ignores ceiling refusals. Custom parameters can therefore receive a “pass” although the verifier would refuse the derived bound. The custom verifier also omits the promised finite-number check for `noise_k`. [15-01-PLAN.md:203](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-01-PLAN.md:203), [15-01-PLAN.md:314](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-01-PLAN.md:314)
- **MEDIUM:** “280x below” describes the recommended maximum **floor**, rather than every permitted enforced bound. The noise term can dominate; the retained ceiling permits bounds up to `1e-3`, only 14x below `0.014`. [15-01-PLAN.md:243](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-01-PLAN.md:243), [laya-parity-v1.yaml:99](/Users/guy/Development/machine-learning/aprender/contracts/laya-parity-v1.yaml:99)

**Suggestions**

- Model policy validity, stamp ceiling, and enforced ceiling in every grid verdict, including custom choices.
- Distinguish floor headroom, observed-cell headroom, and maximum permitted tolerance in the brief.

**Risk assessment:** **MEDIUM** — the approval structure is sound, but the owner should receive an acceptance model matching production.

## 15-02

**Summary:** The proposed additive policy is well matched to the existing verifier. The main remaining weakness is precision in the tests that distinguish the new enforcement from retained A1 checks.

**Strengths**

- The plan changes the existing bound calculation and preserves the sole contract-to-policy mapping, avoiding a competing implementation. [verify.rs:499](/Users/guy/Development/machine-learning/aprender/crates/aprender-decide/src/verify.rs:499), [verify.rs:2394](/Users/guy/Development/machine-learning/aprender/crates/aprender-decide/src/verify.rs:2394)
- Updating the Python integration test is necessary: it currently requires Python’s recorded bound to equal Rust’s enforced bound. Introducing `stamp_bound` resolves that changed meaning explicitly. [python_records.rs:79](/Users/guy/Development/machine-learning/aprender/crates/aprender-decide/tests/python_records.rs:79)

**Concerns**

- **MEDIUM:** The per-set ceiling test needs a deliberately discriminating fixture. The plan retains the A1 stamp ceiling; if both ceilings fail for the test input, removing A2’s ceiling check can leave the same refusal and conceal the mutation. [verify.rs:2435](/Users/guy/Development/machine-learning/aprender/crates/aprender-decide/src/verify.rs:2435), [15-02-PLAN.md:190](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-02-PLAN.md:190)
- **LOW:** Testing `0.9 × bound` and `1.1 × bound` does not establish the contract’s inclusive boundary. An accidental `<` implementation could pass both tests. [15-02-PLAN.md:188](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-02-PLAN.md:188), [laya-parity-v1.yaml:180](/Users/guy/Development/machine-learning/aprender/contracts/laya-parity-v1.yaml:180)

**Suggestions**

- Specify a ceiling fixture whose stamp passes while only the enforced per-set ceiling fails.
- Add exact-boundary tests at the pure comparison layer, retaining the broader end-to-end drift tests.

**Risk assessment:** **MEDIUM** — implementation scope is controlled; the distinguishing tests should be made explicit.

## 15-03

**Summary:** Centralizing eligibility and binding the pack verdict to artifact bytes is appropriate. Missing-record handling and the selectable advisory roles currently contradict required behavior.

**Strengths**

- The chosen record fields exist in the production writer, which records acceptance after pack, verify, hash checks, and artifact upload. [run_step.py:1426](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/container/run_step.py:1426)
- The exact refusal matcher corresponds to the CLI’s actual output format. [pack_laya.rs:125](/Users/guy/Development/machine-learning/aprender/crates/aprender-decide/examples/pack_laya.rs:125)

**Concerns**

- **HIGH:** Automatic discovery returns a record path whenever the directory basename is `run`, regardless of whether the record exists. The recipe then calls `check-record`, which refuses absence. Consequently, a legitimate local artifact in `<dir>/run` without a pack record never reaches the promised local-verifier fallback. [15-03-PLAN.md:120](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-03-PLAN.md:120)
- **HIGH:** `role-all-advisory` permits non-drift refusals, but recipe case (a) unconditionally requires a valid-record synthetic artifact to remain refused. The helper tests branch by role; the recipe tests and contract falsifier do not consistently do so. [15-03-PLAN.md:177](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-03-PLAN.md:177)

**Suggestions**

- Distinguish an absent automatically discovered record from an absent explicitly supplied record. Preserve the local fallback for the former.
- Make every test and contract statement consistent with the selected role, or require re-planning when `role-all-advisory` is chosen.

**Risk assessment:** **HIGH** — an authorized branch cannot satisfy its tests, and default discovery introduces a compatibility regression.

## 15-04

**Summary:** Armed cross-language and real-weight verification is valuable. Two defects currently prevent this plan from reliably establishing a clean release gate.

**Strengths**

- `laya-verify-suite` really arms the four integration targets and produces a fresh Python lifecycle record. Its leg checker rejects skipped execution. [justfile:1715](/Users/guy/Development/machine-learning/aprender/justfile:1715), [justfile:1741](/Users/guy/Development/machine-learning/aprender/justfile:1741)

**Concerns**

- **HIGH:** The proposed claims row cannot invoke the helper using either suggested kind. `kind=just` accepts recipe names; `kind=python` searches only `laya-train-selftest` output. Neither accepts an arbitrary Python command. [15-04-PLAN.md:143](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-04-PLAN.md:143), [justfile:6482](/Users/guy/Development/machine-learning/aprender/justfile:6482)
- **HIGH:** Any nonempty `pre_existing` list satisfies the final regression assertion. The underlying regression recipe exits on its first failure, so accepting one existing failure does not prove that subsequent stages ran or that new failures are absent. [15-04-PLAN.md:166](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-04-PLAN.md:166), [justfile:6619](/Users/guy/Development/machine-learning/aprender/justfile:6619)

**Suggestions**

- Bind the claim to the existing `laya-deploy-selftest` recipe, which the plan already makes responsible for running the helper.
- Require results for every regression stage. Exempt only individually identified failures reproduced at baseline, and execute everything that the first failure prevented.

**Risk assessment:** **HIGH** — the current plan can either fail on ledger syntax or certify incomplete regression coverage.

## 15-05

**Summary:** The build and provenance sequence is largely correct. Commit identity and deployment-diff evidence need more precise definitions.

**Strengths**

- The image command actually archives committed HEAD, checks context cleanliness, and records the resulting digest and commit. [laya_trainer_ops.py:505](/Users/guy/Development/machine-learning/aprender/scripts/laya_deploy/laya_trainer_ops.py:505), [laya_trainer_ops.py:607](/Users/guy/Development/machine-learning/aprender/scripts/laya_deploy/laya_trainer_ops.py:607)
- Finalizer provenance checks cover asset hash, ancestry, changed closure files, and dirty closure files. [laya_trainer_ops.py:738](/Users/guy/Development/machine-learning/aprender/scripts/laya_deploy/laya_trainer_ops.py:738)

**Concerns**

- **MEDIUM:** Task 1 records the current HEAD, but committing its runbook and evidence advances HEAD before the owner builds. Task 2 requires exact commit equality; Task 3 permits context-equivalent commits. Normal execution can therefore contradict Task 2’s verification. [15-05-PLAN.md:104](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-05-PLAN.md:104), [15-05-PLAN.md:159](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-05-PLAN.md:159)
- **MEDIUM:** Comparing two current-source synths identifies the image-induced delta, not necessarily the complete delta against the deployed stack. Recording only changed property names also permits unexpected changes within an allowed property such as `ContainerDefinitions`. [15-05-PLAN.md:181](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-05-PLAN.md:181)

**Suggestions**

- Use one consistent rule: recorded source commit plus verified context equivalence.
- Record candidate template hashes and expected old/new values. Describe the offline comparison as a prediction until checked against the live stack diff.

**Risk assessment:** **MEDIUM** — existing guards are useful, but evidence definitions can cause unnecessary stops or overly broad approvals.

## 15-06

**Summary:** The live re-pack has strong digest and contract checks. Its rehearsal and result-selection instructions create a serious evidence-contamination risk.

**Strengths**

- The generated script really copies into a fresh drift prefix, replaces retention tags, and reads tags back before starting Fargate. [laya_drift.py:970](/Users/guy/Development/machine-learning/aprender/scripts/laya_deploy/laya_drift.py:970)
- The production task definition is explicitly ARM64 and digest-pinned, supporting the intended Graviton authority claim. [laya-trainer-stack.ts:293](/Users/guy/Development/machine-learning/aprender/deploy-extensions/lib/laya-trainer-stack.ts:293)

**Concerns**

- **HIGH:** Task 1 generates the production results directory and runs that script against fake AWS without specifying isolation or cleanup. The script selects the first unused numbered directory. A rehearsal creates `motivation-1`; the live run then creates `motivation-2`, while Task 3 still reads `motivation-1`. [15-06-PLAN.md:100](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-06-PLAN.md:100), [laya_drift.py:1027](/Users/guy/Development/machine-learning/aprender/scripts/laya_deploy/laya_drift.py:1027)
- **MEDIUM:** The evidence check does not bind the selected record’s task identity to the newly launched task. Digest and contract equality alone cannot establish which invocation produced it. [15-06-PLAN.md:208](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-06-PLAN.md:208)

**Suggestions**

- Run fake rehearsals in a separate temporary results tree.
- Select the live result from its emitted directory and task identity, recording that identity and checking it against `pack-outcome.json`.

**Risk assessment:** **HIGH** — the plan can inspect rehearsal or stale output instead of the paid measurement.

## 15-07

**Summary:** Offline recipe derivation is well grounded. A deterministic text mismatch blocks the draft verification as written.

**Strengths**

- The proposed recipe builder is the exact function the trainer uses to write recipe bytes and their hash. The retained attempt’s canonical recipe hash also matched during this review. [contract.py:311](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/contract.py:311), [train.py:777](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/train.py:777)

**Concerns**

- **HIGH:** The action requires `claim_scope` to contain uppercase `NOT an identical reproduction`, while the verification requires lowercase `not an identical reproduction`. Python’s substring check is case-sensitive; the prescribed draft fails. [15-07-PLAN.md:123](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-07-PLAN.md:123), [15-07-PLAN.md:149](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-07-PLAN.md:149)
- **LOW:** The effective successful ECE interval is `[0.07, 0.10]`, because the gate’s `0.10` ceiling intersects the declaration’s `[0.07, 0.12]` band. That distinction should appear prominently in the owner brief. [laya-finetune-gate-v1.yaml:168](/Users/guy/Development/machine-learning/aprender/contracts/laya-finetune-gate-v1.yaml:168)

**Suggestions**

- Use one canonical claim-scope literal.
- State the effective ECE interval and retain the explicit uncertainty about the epochs change’s effect.

**Risk assessment:** **MEDIUM** — preparation is credible; the deterministic blocker is straightforward to repair, while reproduction remains uncertain.

## 15-08

**Summary:** Reusing the evaluator is appropriate, and its negative control works on the retained run. Declaration immutability and failure classification are insufficiently enforced.

**Strengths**

- The evaluator derives metrics from the fetched report and detects a pack refusal after a gate pass. Read-only evaluation confirmed the prior run’s margin and pack-refusal defects. [laya_repro_eval.py:223](/Users/guy/Development/machine-learning/aprender/scripts/laya_deploy/laya_repro_eval.py:223), [laya_repro_eval.py:293](/Users/guy/Development/machine-learning/aprender/scripts/laya_deploy/laya_repro_eval.py:293)
- Fetch verifies listed files against manifest sizes and hashes before installing the destination. [laya_fetch.py:292](/Users/guy/Development/machine-learning/aprender/scripts/laya_deploy/laya_fetch.py:292)

**Concerns**

- **HIGH:** The evidence hashes the declaration’s first-added committed blob, but evaluation reads the working file. Final verification repeats evaluation against that same working file without re-establishing equality to the recorded blob. A changed declaration could therefore produce a verdict while the evidence still names the original declaration hash. [15-08-PLAN.md:103](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-08-PLAN.md:103), [15-08-PLAN.md:214](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-08-PLAN.md:214)
- **HIGH:** Missing fetched `gate-report.json` is treated as proof that no gate verdict occurred. These differ: training can finish with `pass` or `gate_fail`, then fail to upload a report. The wrapper records the training outcome separately and explicitly handles upload failures. [15-08-PLAN.md:184](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-08-PLAN.md:184), [run_step.py:749](/Users/guy/Development/machine-learning/aprender/scripts/laya_train/container/run_step.py:749)
- **MEDIUM:** The evaluator supports missing pack evidence as a defect, but final verification unconditionally opens `pack-outcome.json`. A post-gate infrastructure failure can prevent recording the required final negative outcome. [laya_repro_eval.py:213](/Users/guy/Development/machine-learning/aprender/scripts/laya_deploy/laya_repro_eval.py:213), [15-08-PLAN.md:220](/Users/guy/Development/machine-learning/aprender/.planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation/15-08-PLAN.md:220)

**Suggestions**

- Evaluate a materialized copy of the recorded committed declaration; verify its hash again during final evidence validation.
- Classify attempt consumption from terminal outcome and gate-execution evidence. Treat missing evidence as unresolved rather than automatically unconsumed.
- Test failure paths with missing report, missing pack outcome, and failed fetch.

**Risk assessment:** **HIGH** — these gaps affect the trust decision and enforcement of the single-attempt rule.

---

**Overall assessment: revisions required before execution.**

The dependency order is coherent, especially recipe proof before amendment and real CUDA re-pack before reproduction. The principal blockers are the eligibility branches, incomplete regression acceptance, rehearsal isolation, declaration text mismatch, and mutable-declaration evaluation.

Also define how `run-hold` later becomes an authorized run without silently changing the declaration being hashed. If the single attempt misses, record SC1–SC3 independently and leave SC4 unmet; a correctly recorded miss does not close the trust bar.

---

## Claude Review (fable)

# Phase 15 Plan Review

## Cross-plan verification summary

The plan set rests on a handful of load-bearing claims about the code. I checked each one.

| Claim | Verdict | Evidence |
|---|---|---|
| The record header `k` and `floor_abs` are compared bit-for-bit with the contract, so an in-place per-set edit would invalidate every stored record | Confirmed | `crates/aprender-decide/src/verify.rs:2240-2256`; the retained run's record has `k: 4.0, floor_abs: 1e-05` (`models/decide/motivation-v1-sm/run/rescore-noise.json`) |
| Reported per-set `bound` is cross-checked bit-for-bit and the ceiling applies to the derived bound | Confirmed | `verify.rs:2425-2440` |
| Absent record falls to `float_tolerance` for both sets | Confirmed | `verify.rs:2400-2407` |
| `fine_tuned` is re-scored first, so the motivation `fine_tuned` Graviton figure was never produced | Confirmed | `verify.rs:3475-3497` |
| `ParityConstantsView` has no `deny_unknown_fields`, so added keys are rollback-safe | Confirmed | `verify.rs:473-474` (the `deny_unknown_fields` structs at 300/316 are the waiver views) |
| `check-declaration` tolerates extra top-level keys (`changes_from_attempt_1`, `claim_scope`) | Confirmed, resolves RESEARCH assumption A5 | `scripts/laya_deploy/laya_repro_eval.py:119-186` restricts only `must` and `band` keys |
| Attempt-1 recipe id reproduces from the canonical re-dump; epochs-4 id is `a9f1c4a7…` | Confirmed by computation | `models/decide/motivation-v1-sm/run/recipe.json`; `scripts/laya_train/contract.py:311-320` |
| `train` accepts `epochs 4` at 188 shots/class | Confirmed | `crates/aprender-mcp-laya-train/src/lib.rs:146-174`; contract `epochs_above_16_min: 4` at `contracts/laya-finetune-gate-v1.yaml:281` |
| The `motivation_v1` block pinning `recipe_id f88f…` is a record, not read by the verifier | Confirmed | `contracts/laya-finetune-gate-v1.yaml:235-256` |
| `pack-outcome.json` carries `accepted`, `deploy_eligible`, `apr_sha256`, `pack_image_digest`, `parity_contract_sha256`, `ran`, `verify_line` with `rescore_bound`/`zs_noise` | Confirmed | `scripts/laya_train/container/run_step.py:1297-1304, 1438-1446`; `models/decide/smoke-stance-64/logs/pack-outcome.json` |
| Fetched layout puts the record at `<dir>/logs/pack-outcome.json` next to `run/` | Confirmed | `scripts/laya_deploy/laya_fetch.py:146-158` |
| `outcome.json started_at` matches `%Y-%m-%dT%H:%M:%SZ` | Confirmed | `models/decide/motivation-v1-sm/logs/outcome.json` (`2026-10-30T04:39:07Z`) |
| The exact Mac refusal line with `row=199` is on record | Confirmed | `14-DRIFT-EVIDENCE.json` local legs |
| All cited dotted keys (`rows[8]`, `rows[11]`, `rows[12]`, `a10g_noise.*.sets.*`, `unmeasured`, `graviton_spread`) exist with the stated values | Confirmed | dumped this session |
| Mac-packed artifacts use a flat run layout (basename not `run`) so the record-path convention does not touch them; the deploy selftest's positive run uses `models/decide/laya-stance-64` | Confirmed | `ls models/decide/laya-*`; `justfile:3917-3918` |
| Pack image bakes `contracts/` and `crates/` | Confirmed | `deploy-extensions/container/pack-arm64/context-paths.txt` |
| Tools exist: `rtk`, `lockf`, `bashrs`, `pv` (`extract --check`, `diff`), `scripts/pv_bin.sh` exports `PV`, ratchet prints `PASS` | Confirmed | `scripts/pv_bin.sh:779-802`; `scripts/check_ont_ratchet.sh:455` |

Grid arithmetic in 15-01 (headroom 1.74x / 1.82x / 1.99x / ≥3.84x / 1.45x for `sb-recommended`) recomputes correctly from the evidence values. The 280x and 560x red-line ratios are right.

---

## 15-01

**Summary.** A well-constructed decision brief: every number is a dotted-key re-resolution of Phase 14 evidence, the grid is recomputed by the verifier, Graviton cells carry authority and Mac cells do not, and the sign-off is orchestrator-written, hash-bound to the brief, and pinned as the SC1 ancestry anchor. The `gate="blocking-human"` on Task 2 is correct for a one-way contract decision (`~/.claude/gsd-core/references/checkpoints.md:12,19`).

**Strengths**
- The verify re-resolves `observed_key` and `noise_key` against the source files and recomputes bound, headroom and verdict for every (option, point) cell, so a transcription error cannot survive. The `G-motiv-ft` upper-bound handling (`pass (>=)` / `undetermined`) is honest about the never-produced figure.
- Structure S-B versus S-A is put to the owner with the SC3 expiry (2026-11-28) as the concrete cost of S-A; this matches the bit-equality mechanism at `verify.rs:2240-2256`.
- The two `laya-verify` roles are decided in the same sign-off, closing RESEARCH open questions 1 and 2 before any code is written.
- Pre-sign-off edit guard (`git diff --quiet dd3c96937 -- …`) runs in Tasks 1 and 3.

**Concerns**
- **MEDIUM.** A `custom` option is validated only for finiteness and `1e-5 <= floor <= ceiling <= 1e-3` (Task 3). Nothing refuses or flags custom values under which a Graviton cell fails. Plan 15-02's `noise_per_set_graviton_cells` then goes red by design, and the phase stalls after the one-way sign-off. Task 3 already recomputes the four Graviton cells for `custom`; it should refuse (or at minimum print a `CHECKPOINT REACHED` warning) when any verdict is `fail`.
- **LOW.** With floors of 3e-5 / 5e-5 and k=4, the noise term only engages above 7.5e-6 / 1.25e-5 of A10G noise. Every CUDA checkpoint measured so far has noise ≤ 8.15e-6 (`a10g_noise`), so for CUDA runs the "noise-referenced" bound becomes effectively absolute. The brief should state this plainly as a consequence of the recommended shape, since D-05 frames the amendment as keeping the noise-referenced formula.
- **LOW.** `red_line` is in the JSON but the verify does not check that `smallest_cited_regression_abs` is 0.014 or that the ratio equals `0.014 / max floor`; add two asserts so the headline number is as traceable as the grid.

**Suggestions**
- Add the Graviton-cell pass requirement for `custom` in Task 3.
- Add an explicit sentence in "## Options" on when the noise term is active per option.
- Record in `sign_off` the `verify_role` consequence table (what blocks, what is logged) so 15-03's `ROLE` literal has a single quoted source.

**Risk: LOW.** Nothing is edited; the only failure mode is an under-constrained `custom` reply.

---

## 15-02

**Summary.** The S-B implementation is the right shape for the code as it stands: stamp fields unchanged, six additive flat constants, `RescoreSetPolicy` through the one mapping at `verify.rs:499`, enforcement in `rescore_bounds`, and a load-time invariant. The real-bytes tracer on the retained CUDA record proves the contract value reaches the binary. The nine table tests plus five recorded mutations meet the project's Discipline 4/7 bar.

**Strengths**
- Keeps `check_noise_header` and the reported-`bound` cross-check against the stamp exactly as today (`verify.rs:2240-2256, 2425-2431`), so the retained v1 record stays valid and the SC3 re-pack remains possible before expiry.
- Absent-record path pinned to `float_tolerance` with a dedicated test and mutation `m4`; protects `tests/fail_closed_vectors.rs` (armed in 15-04).
- `tests/python_records.rs:85-93` currently asserts `b.bound` bit-equal to Python's reported bound; the plan correctly moves that to `stamp_bound` and keeps the `NOISE which=<set> rust=` prefix that `_laya-leg-verdict` requires (`justfile:1738`).
- Task 3's anchored greps on `pack_rescore_noise_k: 4` and `pack_rescore_bound_max_abs: 1.0e-3` match the current lines exactly (no trailing comments at `contracts/laya-parity-v1.yaml:98,104`).
- Deliberately leaves `laya-finetune-gate-v1.yaml` untouched to avoid moving the gate-contract hash the train and pack images record.

**Concerns**
- **MEDIUM.** `rescore_bounds` carries `#[provable_contracts_macros::contract("laya-parity-v1", equation = "rescore_noise_reference")]` (`verify.rs:2393`). Adding a proof obligation, invariants and six constants is additive, but the plan does not say whether the macro or Class A of `laya-gap-regression` (`verify::tests::every_run_field_is_bound_or_report_only`, `run_field_bindings_table_matches_fixture_leaves`, `justfile:6690-6692`) requires `contracts/aprender/binding.yaml` rows for new constants. Task 3 edits binding notes but verifies only the parity contract. If a binding-table test demands a row per constant, the failure surfaces one plan later in 15-04's gap regression. Run `cargo test -p aprender-decide --lib verify::tests::every_run_field_is_bound_or_report_only` in Task 3's verify.
- **LOW.** Task 1 says "make the minimum edits that keep the crate compiling" to `tests.rs` and then Task 2 writes the nine tests. The tracer's first verify (`verify::tests::noise`) runs before Task 2; `noise_bound_accepts_what_floor_refuses` asserts the stamp formula today, so under signed floors above 1e-5 its drift (between floor and stamp bound) is still accepted, fine, but its `report.rescore_bound` assertion changes meaning. The plan names this; just be sure the assertion moves to `policy.rescore_set`, not to a literal.
- **LOW.** `RescorePolicyInvalid` is described as "exit 2 like the other refusals"; `pack_laya` maps every `VerifyError` to exit 2 except gate failure (`examples/pack_laya.rs:126`), so this is automatic. State it rather than implying new wiring.

**Suggestions**
- Add the Class A binding tests to Task 3's verify.
- Record in the SUMMARY whether the Mac PACKED or REFUSED the motivation-v1-sm re-pack under the signed params (the plan already asks; this is the first Mac fine_tuned figure for that run and belongs in `graviton_values`-style evidence).

**Risk: MEDIUM.** One-way contract edit, but the compatibility argument is verified against the actual bit-equality code and the retained record.

---

## 15-03

**Summary.** The helper-plus-recipe design is sound and the real-bytes tracer is a genuine end-to-end proof (smoke-stance-64 is accepted on Graviton at 0.843x and refused on this Mac at 1.055x, with its record and sha256 present locally). The must-not-match table and anchored `DRIFT_RE` close the obvious demotion hole. Two integration points with existing selftests need tightening.

**Strengths**
- Record checked before the multi-minute `laya-verify`; sha256 equality uses the recipe's existing `H_LOCAL` (`justfile:4084`); refusal prefix `REFUSED eligibility:` is preserved so `scripts/laya_gates.tsv:20-22` expectations hold.
- Only `RescoreDrift` is demoted under `role-drift-advisory`; the Display format at `verify.rs:1169-1174` (`which= row= max_abs= bound=`) matches `DRIFT_RE`.
- No record keeps today's gate; the synthetic fixture and every Mac-packed artifact have flat layouts (`models/decide/laya-*`), so `record-path` prints nothing for them.
- Residual risk (hand-forged local record) is stated in the contract text, not hidden.
- DRY_RUN with an aws recorder first on PATH, logs backed up and restored.

**Concerns**
- **MEDIUM.** The `record-advisory` armed case breaks an existing gate row. `row_deploy_selftest_skip` (`justfile:5606`) requires the last line to be exactly `DEPLOY SELFTEST OK (positive dry run SKIPPED: disarmed by LAYA_DEPLOY_SELFTEST_POSITIVE=0)`. The plan disarms `record-advisory` with a different variable and says a skipped case "names it among the SKIPPED reasons" on the last line. Anywhere the smoke files are absent (CI, worktrees, the clean export that `laya-gap-regression` Class C uses, `justfile:6660-6675, 6702`) that last line changes and the row goes red; 15-04's gap regression would then fail as a Phase 15 defect. Make `LAYA_DEPLOY_SELFTEST_POSITIVE=0` also disarm `record-advisory` and keep the disarmed line byte-identical, or move the real-bytes case to a FULL-tier gates row.
- **MEDIUM.** Cost: the armed `record-advisory` case runs `laya-verify` on an 807 MB artifact (about 5 minutes) inside `laya-deploy-selftest`, which `row_deploy_selftest_skip` alone invokes three times and `deploy-refusals`/`upload-eligibility` invoke again. That multiplies the main-checkout gates sweep by tens of minutes. The tracer in Task 1 already proves the mechanism on real bytes; keep the selftest to the synthetic cases (a) to (d).
- **LOW.** Convention-derived record path that does not exist. `record-path` prints `<dir>/logs/pack-outcome.json` for any `…/run` argument; the recipe then runs `check-record`, which refuses an absent file. The truths only specify refusal for an *explicitly named* absent record. Today every fetched run carries the record (`laya_fetch.py:156-157`), so the impact is nil, but the helper should distinguish "convention path absent, fall back to today's gate" from "explicit path absent, refuse", or the plan should state the stricter behaviour as intended.
- **LOW.** `ROLE` is a literal copied from `15-BOUND-DECISION.json` at authoring time. Add a `--selftest` case that reads the decision file and asserts equality, so a later re-sign cannot leave the helper stale.

**Suggestions**
- Fix the disarm variable and last-line format before Task 2's verify.
- Drop or FULL-tier the real-bytes selftest case.
- Specify the convention-absent behaviour explicitly.

**Risk: MEDIUM.** The eligibility logic itself is well guarded; the risk is selftest integration breaking downstream gates.

---

## 15-04

**Summary.** The right gate before paying for an image: the real trainer's record through the amended verifier on real weights, the full class sweep, docs and claims coherence, and a phase-wide SC1 ancestry check with a clean image context. Dependencies and commands verified to exist.

**Strengths**
- `laya-verify-suite` legs and the `FAIL-CLOSED VECTORS REFUSED 2/2` positive evidence match `justfile:1692-1722, 1734-1738`.
- The NOISE-line check recomputes both `stamp == max(1e-5, 4·rust)` and `bound == max(floor_s, k_s·rust)` from the signed params, which is the cross-language contract A2 depends on.
- Claims kinds `rust`, `just`, `python`, `pv`, `evidence` exist (`justfile:6419`); CLAUDE.md A1 sentence is at `CLAUDE.md:249` as stated; `pv extract --check` exists.
- "Pre-existing only if it reproduces at dd3c96937 with both logs" is the correct discipline against baseline edits.

**Concerns**
- **MEDIUM.** Inherits the 15-03 deploy-selftest last-line issue: `laya-gap-regression` Class C runs the gates selftest (`justfile:6702`) and judges the export by `^LAYA GATES SELFTEST OK`. If 15-03 lands as written, this plan's gap regression goes red on a Phase 15 defect and the plan must stop. Fix it in 15-03.
- **LOW.** The phase-wide SC1 check asserts `len(out) >= 3` commits; good. But `contracts` in the pathspec also matches `contracts/contracts.nt` and `contracts/shapes.ttl` regenerated in this plan, so the "image_head" recorded must be taken after that commit (the plan says so; keep the order).
- **LOW.** `readme_contract` is cited as gating CLAUDE.md paths; verify the test name exists before relying on it (`cargo test -p aprender-core --test readme_contract -- --list`).

**Suggestions**
- Add `LAYA_GATES_ONLY=deploy-selftest-skip just laya-gates-selftest` to Task 2 as an early, cheap check before the full gap regression.

**Risk: LOW to MEDIUM.** Dominated by the 15-03 integration defect; otherwise a verification-only plan.

---

## 15-05

**Summary.** Correctly identifies that the amendment reaches Graviton only through a rebuilt pack image and a re-pinned task definition, and that `aprender-decide` sits in the finalizer's cargo closure. The offline template diff to predict the owner's `cdk diff` is a good addition over the Phase 14 runbook. Recipe signatures and output lines verified.

**Strengths**
- `laya-trainer-synth env train_digest pack_digest` signature matches `justfile:1390-1400`; `images-dev.json` has `pack.digest/commit/build_id` as the plan reads them; the image dry run prints `image pack: commit <HEAD>` (`scripts/laya_deploy/laya_trainer_ops.py` cmd_image) so the HEAD grep works.
- Expected change set computed from two synths (old vs new digest) and the `PackVerify` EcsRunTask reference (`deploy-extensions/lib/laya-trainer-stack.ts:293, 467-487`) is anticipated.
- Identifier scan over committed files; `repo_uri` in `images-dev.json` carries the account id, so this guard matters.
- No train-image rebuild under S-B; the train image's stale contract copy is harmless because `contract.py::noise_policy` reads only the three stamp values.

**Concerns**
- **LOW.** The pre-roll template is produced by synthesizing with the *recorded* digest, not by reading the deployed template. That is fine as a prediction, but Task 3's comparison should also note any drift between the recorded digest and what `describe-task-definition` would show; the owner's C1 diff is the only live check, and the runbook STOP list covers it.
- **LOW.** Task 2 is `checkpoint:human-action gate="blocking"`. `checkpoints.md:11` says human-action still stops in auto mode, so this is safe, but `checkpoints.md:19-21` recommend `blocking-human` for trust-establishing steps, and `execute-plan.md:150` describes `blocking` as auto-approvable per protocol. Use `blocking-human` to remove the ambiguity; the same applies to 15-06 T2 and 15-08 T2.

**Suggestions**
- Switch the three owner-action checkpoints to `gate="blocking-human"`.

**Risk: LOW.** Owner-run build; everything the executor does is offline and verified by `AWS CALLS: 0`.

---

## 15-06

**Summary.** The deploy-then-re-pack sequence is correct and the plan catches the subtle dependency that the drift script embeds the task-definition revision, so it must be regenerated after `infra outputs` (D0). The SC3 evidence binds the acceptance to the new image digest and the committed contract hash, which is exactly Verification Discipline 2.

**Strengths**
- `laya_drift.py aws-plan --script/--results/--source` flags exist (`scripts/laya_deploy/laya_drift.py:1130-1171`); results land in `<results>/LEG-N/` (`:858`) so `motivation-1/pack-outcome.json` is the right path; last line `DRIFT GRAVITON DONE <args>` (`:1033`).
- The static script policy (copies only, `REPLACE` tagging directive, expire tag, tag read-back, no `s3 cp|mv|sync|rm`) mirrors the proven 14-21 Task 1 check.
- `verify_line` JSON carries `noise`, `zs_noise`, `rescore_bound`, `zs_rescore_bound`, `artifact_sha256`, `deploy_eligible` (observed in the smoke record), so the bound check in Task 3 parses real fields.
- Refusal path ends at `CHECKPOINT REACHED` with no executor retry and no contract edit.

**Concerns**
- **MEDIUM.** This is the first time the motivation `fine_tuned` set is scored on Graviton under any bound. If the signed fine_tuned floor is `sb-minimal`'s 1e-5 (unchanged), the A10G-derived bound is 1.3021e-5 and the Graviton figure is unmeasured; a refusal here costs a Fargate task and an owner decision but no GPU. The plan records it as an observation, which is right, but the 15-01 brief should make this specific cell the headline risk for `sb-minimal`.
- **LOW.** `run_fake` is a selftest-internal function in `laya_drift.py:1883`, not a subcommand; the plan assumes the 14-21 technique. Name the invocation (a `python3 -I -c` import) in the action so the executor does not improvise.
- **LOW.** Assumption A6 (drift path counts as "the Graviton pack task") is reasonable since it is the production image and task definition running `run_step.py`'s pack step, but the evidence should say "via the drift measurement prefix, not the state machine" verbatim.

**Risk: MEDIUM.** The outcome depends on a never-measured cell; the handling of either outcome is correct.

---

## 15-07

**Summary.** The declaration machinery is reused faithfully, the MUST values are computed rather than guessed, and the attempt-1 facts quoted are accurate (`train.log` lines 12-13: `best_epoch=2 epochs_run=5 steps=140/336`). The levers offered are exactly those the gate contract leaves movable, which was RESEARCH Pitfall 4.

**Strengths**
- Control recipe id reproduces `f88f5b71…` and the epochs-4 id `a9f1c4a7…` matches the canonical bytes (verified).
- Data hashes equal the Phase 14 declaration and the staged `datasets-dev.json` prefix, so no new staging and no `--expect` change.
- `attempt_cap 1`, `retry_classes []`, `waiver_rule inconclusive` are all legal per `laya_repro_eval.py:173-184`; extra keys tolerated.
- The pilot-waiver interaction is handled correctly for both drafts: the waiver binds `recipe_id f88f…` (`contracts/laya-finetune-gate-v1.yaml:233`), so epochs-4 cannot be waived and epochs-12 can only reach `inconclusive`.
- `run_authorized` derived only from the orchestrator-recorded `run_go`; the declaration blob must equal HEAD.

**Concerns**
- **MEDIUM.** The epochs-4 recommendation rests on an `[ASSUMED]` direction of effect, which the plan labels. But the margin miss on attempt 1 was a per-seed draw (0.340 / 0.286 / 0.354 at 100 eval rows) and the shipped seed is chosen by median ECE, not by margin. A shorter schedule may move ECE more than margin. The brief should say that the band criterion applies to the *shipped* seed selected by ECE, so a change that improves ECE ranking could ship a lower-margin seed. This is the honest framing of the single draw.
- **LOW.** The `report_only.train_wall_seconds_max 2256` is carried over; epochs 4 shortens training only. Fine, but state that the figure is report-only, not a MUST, as Phase 14 did.
- **LOW.** Task 2 option `revise-plan` correctly stops the phase; make the acceptance criterion say no `15-REPRODUCTION-DECLARATION.json` is written in that case (it does).

**Risk: MEDIUM.** The paid-run outcome is a draw by construction; the declaration process itself is low risk.

---

## 15-08

**Summary.** A faithful reuse of the 14-15 shape with the attempt-cap-1 rule, the pre-verdict carve-out, and the binding of the run to the rolled image and committed contract. The control evaluation of attempt 1 under the new declaration is a good positive control of the evaluator. Owner commands are built from gitignored state by substitution, so no identifier is committed.

**Strengths**
- `next_trusted` is recomputed from a fresh evaluator run and must equal the recorded line; `pilot_waiver` and `waived` must be null for a trusted verdict; `"overrides"` absent.
- Declaration ordering check compares `%ct` of the adding commit with `outcome.json started_at`, whose format matches (`2026-10-30T04:39:07Z`).
- `po.get('ran')` and the image/contract hash equality are real fields in the record.
- D-09 shown on the trust-bar bytes with the `aprender-mcp-decide-motivation` thin crate present (`crates/aprender-mcp-decide-motivation-lambda/`).
- Run name `motivation-v1-p15` satisfies the tool's grammar (`lib.rs:180-196`).

**Concerns**
- **MEDIUM.** A pre-verdict failure "does not consume the attempt" and "any re-run needs the owner's new recorded go", but the plan has no task for that re-run: Task 3 stops at `CHECKPOINT REACHED`. That is acceptable for D-12, but the plan should state that a re-run is a re-execution of Tasks 1 to 3 with a new owner reply appended, so the executor does not improvise a second owner command set.
- **LOW.** `gate="blocking"` on Task 2 (same note as 15-05).
- **LOW.** The identifier regex guesses the bucket prefix (`aprender-laya-train-dev-`); if the bucket name differs, a leak passes. Read `trainer.BucketName` from `outputs-dev.json` at verify time and assert its absence instead.

**Risk: MEDIUM.** The run is the phase's one paid, stochastic step; the judgement and recording path is sound.

---

## Overall assessment

**Do the plans achieve the phase goals?** Yes, with the caveats above. SC1 and SC2 are covered by 15-01 to 15-04 with mechanism-level proof; SC3 by 15-05 and 15-06 with digest and hash binding; SC4 by 15-07 and 15-08 with the pure evaluator; SC5 by the owner-run pattern throughout. The D-17 order is respected by the wave structure.

**Dependency ordering.** Correct. 15-03 runs on the A1 tree before 15-02, with a precondition on the contract hash, and its decide-apr-v1 text is written to be true before and after A2. 15-02 depends on 15-03's SUMMARY. No closure commits between 15-05's synth and 15-06's deploy.

**Scope.** Heavy, but proportionate to a one-way contract edit, an image roll, and a paid run. The repeated SC1 ancestry checks are cheap and justified. The one piece of over-reach is the real-bytes `record-advisory` selftest case in 15-03.

**Top three actions before execution**
1. In 15-03, make `LAYA_DEPLOY_SELFTEST_POSITIVE=0` disarm `record-advisory` and keep the disarmed last line byte-identical, or drop the real-bytes selftest case. Otherwise `row_deploy_selftest_skip` (`justfile:5606`) and 15-04's gap regression go red.
2. In 15-01 Task 3, refuse a `custom` option that fails any Graviton cell, so 15-02's table test cannot be red by construction after a one-way sign-off.
3. Switch the owner-action checkpoints in 15-05, 15-06 and 15-08 to `gate="blocking-human"`.

**Overall risk: MEDIUM.** The engineering is careful and verified against the code; the residual risk is the stochastic D-16 draw (acknowledged by the plans) and one selftest integration defect that is cheap to fix now.

---

## Consensus Summary

Both reviewers verified the plans against the source and cited `file:line` evidence, so both count at full weight. They agree that the D-17 ordering is correct (15-01 → 15-03 → 15-02 → 15-04, the image roll before the re-pack, the re-pack before the reproduction) and that the S-B "keep the A1 stamp, add per-set enforced constants" shape fits the bit-equality check at `crates/aprender-decide/src/verify.rs:2240-2256`. They differ sharply on severity. Codex rates 15-03, 15-04, 15-06, 15-07 and 15-08 as HIGH, with five plan-level blockers; Fable rates the whole set MEDIUM, with one integration defect.

The orchestrator checked the deterministic findings against the plan text and the code (marked below). **Confirmed** means the mechanism was checked directly. **Consistent** means the cited code supports the finding but the full execution path was not run.

### Agreed Strengths
- The S-B structure keeps the v1 stamp (`k`, `floor_abs`) and the stamp cross-check, so the retained record stays valid and the SC3 re-pack stays possible before the 2026-11-28 expiry (`verify.rs:2240-2256, 2425-2440`).
- The 15-02 policy goes through the single contract-to-policy mapping (`verify.rs:499`), and moving the Python cross-check to `stamp_bound` in `tests/python_records.rs` is necessary and correct.
- The 15-03 eligibility is bound to artifact bytes by sha256 through the production writer's record fields (`run_step.py` pack-outcome), and `DRIFT_RE` matches the real `RescoreDrift` Display format.
- The 15-05/15-06 provenance chain (committed-HEAD image, finalizer closure checks, digest-pinned ARM64 task definition, tag read-back before run-task) really exists in the code.
- 15-07 derives the recipe ids with the trainer's own builder; both reviewers reproduced `f88f…`, and Fable also reproduced the epochs-4 `a9f1c4a7…`.

### Agreed Concerns
1. **15-03: record discovery by convention refuses where it should fall back** (Codex HIGH, Fable LOW). `record-path` prints `<dir>/logs/pack-outcome.json` for any `…/run` argument (`15-03-PLAN.md:120-121`), and `check-record` then refuses an absent file. The truths only call for refusal when the record is *named explicitly* and absent. The fix: an absent convention-derived record falls back to today's local gate, while an absent explicit `LAYA_PACK_OUTCOME` refuses. *Confirmed in the plan text.*
2. **15-01: a `custom` option is under-validated** (both MEDIUM). The grid judges `drift <= bound` only. It does not refuse a custom choice that fails a Graviton cell (Fable), and it does not model ceiling refusals or check `noise_k` for finiteness (Codex). The result is a one-way sign-off that 15-02's Graviton-cell table test can turn red by construction. The fix: compute the full production acceptance (validity, stamp ceiling, enforced ceiling) for every option and refuse a custom choice that fails any Graviton cell.
3. **15-01: the brief understates what the bound permits** (Codex MEDIUM, Fable LOW). "280x below" describes the maximum floor, not the largest bound allowed (the 1e-3 ceiling is only 14x below 0.014). With the recommended floors the noise term rarely engages for CUDA runs, so the "noise-referenced" bound is effectively absolute. The brief should say both plainly.
4. **15-08: failure classification for a paid run** (Codex HIGH; Fable MEDIUM, on the same seam). A pre-verdict failure "does not consume the attempt", but a missing fetched `gate-report.json` is not proof that no verdict occurred (`run_step.py:749` records the training outcome separately). Codex also notes that final verification opens `pack-outcome.json` unconditionally, and Fable that the re-run path is unspecified. Fix: classify from the terminal outcome plus gate-execution evidence, treat missing evidence as *unresolved*, and spell out that a re-run means re-executing Tasks 1–3 with a newly recorded owner go.

### Divergent Views (each raised by one reviewer — investigate before re-planning)
- **15-03: selftest integration breaks an existing gate row** (Fable only, MEDIUM). *Confirmed.* `row_deploy_selftest_skip` (`justfile:5606`) requires the disarmed last line to be exactly `DEPLOY SELFTEST OK (positive dry run SKIPPED: disarmed by LAYA_DEPLOY_SELFTEST_POSITIVE=0)`, and the armed default requires the bare `DEPLOY SELFTEST OK` (`justfile:5609-5612`). The plan adds `record-advisory` under a *different* disarm variable, `LAYA_RECORD_SELFTEST_POSITIVE` (`15-03-PLAN.md:190`), and a skipped case is "never a bare OK" (`:192`). Wherever the smoke files are absent (worktrees, the Class C clean export), the last line changes and the row goes red, which then fails 15-04's gap regression. This is probably the highest-value fix in the set. Fable also flags the ~5-minute real-bytes `laya-verify` inside a selftest that is invoked several times per gates sweep.
- **15-03: `role-all-advisory` contradicts recipe case (a)** (Codex only, HIGH). Case (a) requires a valid record to keep a non-drift refusal (`15-03-PLAN.md:186`), which holds only under `role-drift-advisory`. Either make the tests branch on role, or require re-planning if the owner picks `role-all-advisory`.
- **15-07: the claim-scope literal fails its own verify** (Codex only, HIGH). *Confirmed:* the action writes `NOT an identical reproduction` (`15-07-PLAN.md:123`) and the verify asserts the lowercase `'not an identical reproduction' in x['claim_scope']` (`:149`). It is a deterministic blocker with a one-line fix. Codex also notes that the effective ECE pass window is `[0.07, 0.10]` (the gate ceiling cuts into the band); Fable notes that the shipped seed is chosen by ECE, not margin.
- **15-04: the gap-regression assertion is vacuous** (Codex only, HIGH). *Confirmed:* `assert last_line == 'LAYA GAP REGRESSION OK' or e['gap_regression']['pre_existing']` (`15-04-PLAN.md:166`) passes on any non-empty `pre_existing` list, and the recipe stops at its first failure, so later stages are never shown to run. Codex also says the proposed claims-ledger row cannot invoke the helper through `kind=just` or `kind=python` (`justfile:6482`); this was not verified.
- **15-06: the rehearsal contaminates the live results directory** (Codex HIGH; Fable did not raise it). *Consistent:* the fake-aws rehearsal runs the script with the same `--results models/laya-trainer/p15-repack-graviton` (`15-06-PLAN.md:101`), and the script picks the first unused `motivation-N` (`laya_drift.py:1027-1028`). The rehearsal therefore takes `motivation-1`, the paid run lands in `motivation-2`, and Task 3 reads `motivation-1`. Fix: rehearse into a temp results tree and bind the selected record to the launched task id. Fable instead flagged the never-measured motivation `fine_tuned` Graviton cell as the headline risk under `sb-minimal`.
- **15-08: the declaration is evaluated from the working file** (Codex only, HIGH). The evidence hashes the committed blob but the evaluator reads the working tree. Evaluate a materialized copy of the recorded blob and re-check its hash at final verification.
- **15-05: commit identity** (Codex only, MEDIUM). Task 1 records HEAD, then commits the runbook (which advances HEAD), while Task 2 requires exact equality. Use one rule: recorded commit plus verified context equivalence.
- **Checkpoint gates** (Fable only, LOW). Switch the owner-action checkpoints in 15-05 T2, 15-06 T2 and 15-08 T2 from `gate="blocking"` to `gate="blocking-human"`.
- **15-02 tests** (Codex MEDIUM/LOW): use a ceiling fixture where only the per-set ceiling fails, and exact-boundary tests for the inclusive `<=`. (Fable MEDIUM): add the Class A binding tests (`every_run_field_is_bound_or_report_only`) to Task 3's verify, so a missing binding row surfaces in 15-02, not in 15-04.

### Overall
Codex says "revisions required before execution". Fable says "yes, with caveats, MEDIUM". The confirmed deterministic defects (15-03 selftest last line, 15-07 case mismatch, 15-04 vacuous assertion), plus the consistent 15-06 rehearsal contamination, are cheap to fix now and expensive later: each one either reddens a downstream gate or corrupts the evidence for a paid run. A `--reviews` re-plan is warranted.
