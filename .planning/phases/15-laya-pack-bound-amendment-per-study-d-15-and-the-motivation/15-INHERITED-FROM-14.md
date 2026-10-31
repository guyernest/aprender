# Phase 15 seed: what Phase 15 inherits from Phase 14

This file is the hand-off from Phase 14 (Laya training MCP server) to Phase 15 (pack-bound amendment per study, D-15, and the motivation-v1 re-reproduction, D-16). It is built from `14-VERIFICATION.md` (verified 2026-10-30T23:59:00Z, gaps_found, 13/16), `14-GAP-ROUND-2-OWNER-DECISIONS.md`, `14-DRIFT-EVIDENCE.json`, `14-GATE-RUN-EVIDENCE.json`, `14-REPRODUCTION-DECLARATION.json` and `14-REVIEW-DISPOSITION.md`, all in `.planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/`. It states facts and consequences and recommends no bound parameter: the Phase 15 planner argues the bound and the owner signs off at a `checkpoint:decision` before any contract edit (owner answer 3). Phase 14 truth 9 (D-22, D-05) stays true because no Phase 14 plan edits `contracts/`, a decide crate or a decide server. It is not named CONTEXT, RESEARCH or PLAN on purpose, so Phase 15's discuss and plan steps do not mistake it for their own artifacts.

## Inherited truths

### Truth 12 (gaps[0], failed): covered by Phase 15 success criterion 4

D-16 trust bar: the server's first real gate run reproduces motivation-v1 (passes the gate and lands in the owner-approved band margin [0.30, 0.45], ECE [0.07, 0.12], with an accepted deploy-eligible .apr) before the server is trusted

Missing, verbatim from 14-VERIFICATION.md:

A reproduction that lands in band with an accepted, deploy-eligible .apr (needs the D-15 amendment first or a new declaration), OR an explicit owner waiver of D-16 recorded as an overrides: entry in this file

Its waiver branch is closed by owner answer 2: no waiver and no override entry. Covered by Phase 15 success criterion 4.

### Truth 13 (gaps[1], partial): covered by Phase 15 success criteria 1, 2 and 3

A gate-passing CUDA-trained checkpoint yields a deploy-eligible .apr from the arm64 pack step

Missing, verbatim from 14-VERIFICATION.md:

The planned amend-per-study change set: a per-set (fine_tuned, zero_shot) parity-bound amendment argued from 14-DRIFT-EVIDENCE.json, plus a stated role for the local laya-verify (owner-only, D-15)

A roadmap entry (new phase or inserted decimal phase) that owns it, because no later phase exists to defer to

The roadmap-entry item is met by the Phase 15 entry in ROADMAP.md. The amendment item is Phase 15 success criteria 1 and 2; the deploy-eligible `.apr` is criterion 3.

### Truth 8 (behavior_unverified_items[0]): a candidate for the owner, not a Phase 15 criterion

Cancelling a task while its arm64 pack/verify task is really packing stops the pack task, releases the single-flight lock and ends the run cancelled

This stays open in 14-VERIFICATION.md human_verification. It is not a Phase 15 success criterion: see the Candidates for the owner section for why a cancel on the D-16 run's own pack would end that run with no `.apr`.

## Binding owner decisions

Quoted from `14-GAP-ROUND-2-OWNER-DECISIONS.md` (2026-10-30, recorded by the orchestrator from the owner's AskUserQuestion answers). The Owner answer column, verbatim:

1. **New Phase 15 (Recommended).** A new roadmap phase owns the contract amendment and the D-16 reproduction. Phase 14's gap plans only record the hand-off, and truth 9 stays true.
2. **Re-reproduce after D-15.** A new declaration and a fresh SageMaker gate run (about 1 hr of ml.g5.xlarge), after the amended bound lands.
3. **Planner argues, I approve.** The plan derives a per-set bound from `14-DRIFT-EVIDENCE.json` and stops at a `checkpoint:decision` for owner sign-off before any contract edit.

The four consequence bullets, verbatim:

- D-16 is not waived. No `overrides:` entry is authorized by these answers.
- The amendment and the reproduction are deferred to Phase 15, not dropped. Phase 14's gap closure is the hand-off: the roadmap entry, and a verification that lists both truths as deferred to Phase 15.
- The paid compute for the reproduction is above the CLAUDE.md check-in threshold of 1 hr on a non-lambda-vector host. Answer 2 approves the run in principle. Each AWS command is still owner-run, as in every live plan of Phase 14.
- The bound parameters are not chosen yet. Phase 15 must not edit the contract before the owner signs off at its checkpoint.

The D-15 reply on record (`14-DRIFT-EVIDENCE.json` owner_replies[3], 2026-10-30, recorded by the orchestrator): reply `amend-per-study`; parameters field verbatim: "not specified by the owner in the reply; to be argued per set in the separate planned change set".

The D-16 attempt 1 reply on record (`14-GATE-RUN-EVIDENCE.json` owner_replies[0], 2026-10-29): reply `not-reproduced`; owner_note verbatim: "With only a few examples we get a mediocre model; the way forward is a mechanism to grow the dataset and improve annotation quality. Training and inference MCP servers can spin the data-quality flywheel." Its meaning field says the owner accepts the verdict under the committed declaration and that no further attempt is authorized under it; Phase 15 therefore needs a new declaration.

## Measured facts

Every row names its source file (in the Phase 14 directory) and the dotted JSON key; the value is the JSON literal as `json.dumps` prints it. Plan 14-23's verify re-resolves each row against its source and requires exact equality. The rows below are descriptions of what the source says, not conclusions.

| fact | file | key | value |
|---|---|---|---|
| D-16 declared margin band | `14-REPRODUCTION-DECLARATION.json` | `band.margin` | `[0.3, 0.45]` |
| D-16 declared post-calibration ECE band | `14-REPRODUCTION-DECLARATION.json` | `band.ece_post` | `[0.07, 0.12]` |
| declared attempt cap | `14-REPRODUCTION-DECLARATION.json` | `retry.attempt_cap` | `2` |
| declared retry classes | `14-REPRODUCTION-DECLARATION.json` | `retry.retry_classes` | `["in-band-gate-fail", "waived"]` |
| declared waiver rule | `14-REPRODUCTION-DECLARATION.json` | `waiver_rule` | `"inconclusive"` |
| declared: .apr must be deploy-eligible | `14-REPRODUCTION-DECLARATION.json` | `must.deploy_eligible` | `true` |
| declared: seeds that must pass | `14-REPRODUCTION-DECLARATION.json` | `must.seeds` | `[13, 17, 23]` |
| declared note on the must clauses | `14-REPRODUCTION-DECLARATION.json` | `must_note` | `"deploy_eligible is the PACK TASK's verify (pack-outcome.json), which the evaluator reads; a local laya-verify refusal on the arm64 Mac is a recorded cross-CPU finding, not a MUST clause (owner decision at the 14-14 Task 3 checkpoint)"` |
| attempt 1 verdict | `14-GATE-RUN-EVIDENCE.json` | `verdict` | `"not-reproduced"` |
| attempt 1: server trusted next | `14-GATE-RUN-EVIDENCE.json` | `next_trusted` | `false` |
| attempt 1 shipped seed | `14-GATE-RUN-EVIDENCE.json` | `seeds.shipped` | `17` |
| attempt 1 gate margin (shipped seed) | `14-GATE-RUN-EVIDENCE.json` | `gate.margin` | `0.2860488947445468` |
| attempt 1 gate post-calibration ECE | `14-GATE-RUN-EVIDENCE.json` | `gate.ece_post` | `0.09636022716760634` |
| attempt 1 per-seed margin, seed index 0 | `14-GATE-RUN-EVIDENCE.json` | `seeds.per_seed[0].margin` | `0.3400236527815074` |
| attempt 1 per-seed margin, seed index 2 | `14-GATE-RUN-EVIDENCE.json` | `seeds.per_seed[2].margin` | `0.3540824086953462` |
| attempt 1 pack line | `14-GATE-RUN-EVIDENCE.json` | `pack.line` | `"REFUSED RescoreDrift which=zero_shot row=17 max_abs=0.00001728534698486328 bound=0.00001 (nothing written)"` |
| attempt 1 zero_shot A10G noise (max_abs) | `14-GATE-RUN-EVIDENCE.json` | `noise.zero_shot.max_abs` | `1.7265105816122173e-06` |
| attempt 1 zero_shot bound | `14-GATE-RUN-EVIDENCE.json` | `noise.zero_shot.bound` | `1e-05` |
| attempt 1 billable seconds (ml.g5.xlarge) | `14-GATE-RUN-EVIDENCE.json` | `billable_seconds` | `1346` |
| attempt 1 Fargate pack task seconds | `14-GATE-RUN-EVIDENCE.json` | `pack_task_seconds` | `124.16` |
| Phase 12 comparison margin | `14-GATE-RUN-EVIDENCE.json` | `comparison.phase12.margin` | `0.3780294827021587` |
| Phase 12 comparison post-calibration ECE | `14-GATE-RUN-EVIDENCE.json` | `comparison.phase12.ece_post` | `0.09355141013860704` |
| drift study status | `14-DRIFT-EVIDENCE.json` | `status` | `"measured"` |
| owner reply to D-15 | `14-DRIFT-EVIDENCE.json` | `owner_replies[3].reply` | `"amend-per-study"` |
| Graviton spread [0] minimum | `14-DRIFT-EVIDENCE.json` | `graviton_spread[0].min` | `1.728534698486328e-05` |
| Graviton spread [0] within bound | `14-DRIFT-EVIDENCE.json` | `graviton_spread[0].within_bound` | `0` |
| Graviton spread [1] minimum | `14-DRIFT-EVIDENCE.json` | `graviton_spread[1].min` | `2.7477741241455078e-05` |
| Graviton spread [1] within bound | `14-DRIFT-EVIDENCE.json` | `graviton_spread[1].within_bound` | `3` |
| smoke-stance-64 fine_tuned A10G bound | `14-DRIFT-EVIDENCE.json` | `a10g_noise.smoke-stance-64.sets.fine_tuned.bound` | `3.259039634251337e-05` |
| row 0 max_abs over bound | `14-DRIFT-EVIDENCE.json` | `rows[0].max_abs_over_bound` | `1.05528` |
| row 1 max_abs over bound | `14-DRIFT-EVIDENCE.json` | `rows[1].max_abs_over_bound` | `1.49012` |
| row 6 max_abs over bound | `14-DRIFT-EVIDENCE.json` | `rows[6].max_abs_over_bound` | `0.525671` |
| row 8 max_abs over noise | `14-DRIFT-EVIDENCE.json` | `rows[8].max_abs_over_noise` | `10.0117` |
| motivation S3 objects expire about | `14-DRIFT-EVIDENCE.json` | `sources.motivation.expires_about` | `"2026-11-28"` |
| unmeasured item 3 | `14-DRIFT-EVIDENCE.json` | `unmeasured[3]` | `"the second set of a refused leg: pack_laya reports only the FIRST drifted set (exposing it is a decide-crate change, D-15)"` |
| row 0 artifact | `14-DRIFT-EVIDENCE.json` | `rows[0].artifact` | `"smoke-stance-64"` |
| row 0 step | `14-DRIFT-EVIDENCE.json` | `rows[0].step` | `"local verify"` |
| row 0 set | `14-DRIFT-EVIDENCE.json` | `rows[0].set` | `"fine_tuned"` |
| row 1 artifact | `14-DRIFT-EVIDENCE.json` | `rows[1].artifact` | `"motivation-v1-sm"` |
| row 1 step | `14-DRIFT-EVIDENCE.json` | `rows[1].step` | `"local pack"` |
| row 1 set | `14-DRIFT-EVIDENCE.json` | `rows[1].set` | `"zero_shot"` |
| row 6 artifact | `14-DRIFT-EVIDENCE.json` | `rows[6].artifact` | `"laya-antisemitism-v1"` |
| row 6 step | `14-DRIFT-EVIDENCE.json` | `rows[6].step` | `"local verify"` |
| row 6 set | `14-DRIFT-EVIDENCE.json` | `rows[6].set` | `"fine_tuned"` |
| row 8 artifact | `14-DRIFT-EVIDENCE.json` | `rows[8].artifact` | `"motivation-v1-sm"` |
| row 8 step | `14-DRIFT-EVIDENCE.json` | `rows[8].step` | `"pack-task pack"` |
| row 8 set | `14-DRIFT-EVIDENCE.json` | `rows[8].set` | `"zero_shot"` |

Reading the table, without recommending anything. The motivation-v1-sm zero_shot bound is the 1e-5 floor because 4 x its A10G noise (1.7265105816122173e-06, so 6.9e-06) is below the floor (`14-GATE-RUN-EVIDENCE.json` findings[0]). Every Graviton leg gave exactly one distinct value over its 3 repeats (`graviton_spread`, distinct_values 1 for all five legs). The Mac and Graviton disagree in both directions: motivation-v1-sm zero_shot is lower on the Mac (1.49 x the bound) than on Graviton (1.72853 x), and smoke-stance-64 fine_tuned is higher on the Mac (1.05528 x, refused) than on Graviton (0.843124 x, accepted). Among the MPS-trained artifacts (rows 2 to 7) the largest ratio is rows[6], 0.525671 of its bound. The shipped seed's margin (0.2860488947445468) is below the band floor of 0.30, so a re-pack under any bound alone would not close D-16 (owner answer 2).

## Facts read from the tree

Checked against the tree on 2026-10-30; Phase 15 should re-read them before relying on them.

- `contracts/laya-parity-v1.yaml` carries these literal lines: `version: 2.0.0`, `pack_rescore_noise_k: 4`, `pack_rescore_bound_max_abs: 1.0e-3`, `float_tolerance: 1.0e-5` (the floor on equation `pack_rescore_probs_abs`).
- justfile recipe `laya-verify` runs `pack_laya verify` through `cargo run --release -p aprender-decide --example pack_laya`.
- Recipes `laya-deploy` (its step (5) ELIGIBILITY) and `laya-upload` both run `just laya-verify` on this Mac and refuse unless it accepts with `deploy_eligible` true. An `.apr` that the Graviton pack task accepted can therefore still be refused at deploy time on this Mac. smoke-stance-64 is the observed case: rows[0] (1.05528 x the bound on the Mac) against the Graviton 0.843124 x accept.
- `pack_laya` reports only the first drifted set, so the motivation-v1-sm fine_tuned figure on Graviton was never produced (unmeasured[3]).

## Open owner checkpoints

Facts only; every item is the owner's.

1. The D-15 bound parameters per set (fine_tuned, zero_shot), at a `checkpoint:decision` before any contract edit (owner answer 3). The edit is one-way per Phase 14 CONTEXT D-15.
2. The role of the local `laya-verify`, decided as part of that amendment.
3. The new D-16 declaration (band, attempt cap, retry classes, waiver rule), committed before the paid run, as plan 14-14 committed `14-REPRODUCTION-DECLARATION.json`.
4. The compute go for each paid run.
5. The two candidates below.

## Compute

- Owner answer 2 estimates about 1 hr of ml.g5.xlarge for the reproduction.
- Attempt 1 billed 1346 s of ml.g5.xlarge (`billable_seconds`) and a 124.16 s Fargate pack task (`pack_task_seconds`); see the table rows.
- The Phase 14 declaration's attempt cap was 2 (`retry.attempt_cap`); the owner's reply on attempt 1 authorized no further attempt under that declaration.
- CLAUDE.md requires a check-in for compute spend above 1 hr on a non-lambda-vector host; the owner approved the run in principle on 2026-10-30 (answer 2).
- Every AWS command is run by the owner with the bang prefix. Owner replies are recorded on disk by the orchestrator, never relayed by an executor.

## Candidates for the owner

Each is explicitly not a decision.

(a) Truth 8, the packing-stage cancel: about 62 min of paid compute (`14-VERIFICATION.md` behavior_unverified_items[0]). The D-16 re-reproduction is the next planned run that passes its gate and reaches a real pack, but by truth 8's expected outcome a cancel ends the task cancelled with no `.apr`, so cancelling that run's pack would forfeit the reproduction. Observing the cancel needs either a separate gate-passing run (the report names tweet-stance-64 epochs 4, about 51 min of ml.g5.xlarge plus about 11 min of Fargate) or the owner's explicit choice to spend one attempt on it.

(b) Re-packing the already trained motivation-v1-sm under the amended bound, to show success criterion 3 on a CUDA checkpoint before its S3 objects expire (`sources.motivation.expires_about` in the table; the run is tagged expire). The local copies under `models/decide/motivation-v1-sm` on this Mac are gitignored. Its margin is out of band, so it cannot close D-16 either way.

## Not measured

The four `14-DRIFT-EVIDENCE.json` unmeasured strings, verbatim:

- x86_64 CPU: there is no x86_64 pack binary or image (new infrastructure, not planned)
- Graviton generation split: the production pack image cannot print its CPU generation, so host heterogeneity on Fargate is observable only as run-to-run spread across repeats
- a fresh A10G run: the A10G noise is already recorded in each run's rescore-noise.json; no GPU job is started
- the second set of a refused leg: pack_laya reports only the FIRST drifted set (exposing it is a decide-crate change, D-15)

The MPS-versus-CUDA accuracy gap is unexplained, in the words of `14-GATE-RUN-EVIDENCE.json` owner_replies[0].meaning: "The owner accepts the verdict not-reproduced under the committed declaration; no further attempt is authorized; the accuracy difference between the MPS (Phase 12) and CUDA (this run) was not explained by measurement."

The per-seed margin spread at 100 eval rows is not characterised beyond the per-seed rows of the table (seed index 0: 0.3400236527815074, seed 17: 0.2860488947445468, seed index 2: 0.3540824086953462).

## Not carried

From `14-REVIEW-DISPOSITION.md`. All are `open` and awaiting owner triage; none is Phase 15 scope unless the owner moves it.

- WR-01: job-written text is clipped to 1024 characters before redaction, so an id straddling the cut survives as a fragment.
- WR-02: `laya-trainer-synth` copies the finalizer from a hard-coded `target/` path; not engaged today.
- WR-03: the request function is deployed from the working tree with no clean-closure check and no build record.
- IN-01: override note always says "train job"; a Train exit 1 after a recorded pass is reported as the pack stage leaving no record.
- IN-02: results stored before the 14-18 redeploy are still served unredacted by `tasks/result` until task TTL.
- IN-03: redaction blind spots (hyphenated account form, 12 digits next to a letter, AROA/AIDA ids, swallowed colon).
- IN-04: an immutable commit tag blocks rebuilding the same HEAD after a partial build.
- IN-05: pre-filter and trainer gate-contract hashes are recorded but never compared.
- IN-06: `_laya-crates-root-swap` is labelled "decide config" during the laya deploy.
- IN-07: no pmcp.run login precheck before the grant revoke.
- IN-08: drift script has no total task cap and no stop-task on timeout (accepted AR-7).
- IN-09: real account ids in the committed justfile target table (predates the round).
