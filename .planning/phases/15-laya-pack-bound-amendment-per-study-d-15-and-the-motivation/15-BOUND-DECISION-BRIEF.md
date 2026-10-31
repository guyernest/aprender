# Phase 15 Plan 01: the owner's pack/verify re-score bound decision brief

Generated from `15-BOUND-CANDIDATES.json` (every number below re-resolves to `14-DRIFT-EVIDENCE.json` by its recorded key). No measurement was run for this brief (D-07).

## Question

Which per-set (fine_tuned, zero_shot) pack/verify re-score parameters, which structure, and which role for the local `laya-verify` does the `laya-parity-v1` amendment implement? The signed values become published constants that the pack image, `laya-verify` and every later deploy re-read (one-way, D-01, D-05). Nothing under `contracts/`, `crates/aprender-decide` or the `justfile` is edited before the signature.

Today the A1 bound is `max(1.0e-5, 4 x noise)` per set (stamp: noise_k 4, floor_abs 1e-05, bound_max_abs 0.001). It refuses the Graviton motivation-v1-sm zero_shot cell: 4 x its A10G noise (1.7265105816122173e-06) is 6.9e-06, below the 1e-05 floor, while the Graviton re-score drifted 1.728534698486328e-05.

## Evidence

Authority (D-04): Graviton cells (arm64 Fargate pack task, the CPU that judges production) carry authority true; Mac cells are information only. `trained_on` says which device produced the checkpoint and its noise record.

| id | cpu | authority | artifact | set | trained_on | observed max_abs | A10G/record noise | observed / noise |
|---|---|---|---|---|---|---|---|---|
| G-motiv-zs | graviton | TRUE | motivation-v1-sm | zero_shot | cuda | 1.728534698486328e-05 | 1.7265105816122173e-06 | 10.01 |
| G-smoke-ft | graviton | TRUE | smoke-stance-64 | fine_tuned | cuda | 2.7477741241455078e-05 | 8.147599085628343e-06 | 3.37 |
| G-smoke-zs | graviton | TRUE | smoke-stance-64 | zero_shot | cuda | 1.5079975128173828e-05 | 4.70304053501458e-06 | 3.21 |
| G-motiv-ft | graviton | TRUE | motivation-v1-sm | fine_tuned | cuda | <= 1.3021142293956345e-05 | 3.255285573489086e-06 | 4.00 |
| M-smoke-ft | mac | false | smoke-stance-64 | fine_tuned | cuda | 3.439188003540039e-05 | 8.147599085628343e-06 | 4.22 |
| M-motiv-zs | mac | false | motivation-v1-sm | zero_shot | cuda | 1.4901161193847656e-05 | 1.7265105816122173e-06 | 8.63 |
| M-mps-2 | mac | false | laya-motivation-v1 | fine_tuned | mps | 9.149312973022461e-06 | 1.820794164297812e-05 | 0.50 |
| M-mps-3 | mac | false | laya-motivation-v1 | zero_shot | mps | 3.337860107421875e-06 | 1.2524598510976759e-05 | 0.27 |
| M-mps-4 | mac | false | laya-stance-64 | fine_tuned | mps | 1.1324882507324219e-05 | 8.33931784072206e-06 | 1.36 |
| M-mps-5 | mac | false | laya-stance-64 | zero_shot | mps | 7.3462724685668945e-06 | 1.495515496430233e-05 | 0.49 |
| M-mps-6 | mac | false | laya-antisemitism-v1 | fine_tuned | mps | 7.554888725280762e-06 | 3.592973853439041e-06 | 2.10 |
| M-mps-7 | mac | false | laya-antisemitism-v1 | zero_shot | mps | 1.195073127746582e-05 | 1.5070898052527149e-05 | 0.79 |

G-motiv-ft (Graviton motivation-v1-sm fine_tuned) was never produced: `pack_laya` re-scores fine_tuned first and the refusal named zero_shot, so the value is only bounded above by the A10G record's own `bound` for that set (1.3021142293956345e-05, key `a10g_noise.motivation-v1-sm.sets.fine_tuned.bound`). It is shown as an upper bound, never as a measurement.

## Why A1 does not hold for CUDA-trained, Graviton-packed runs

A1's k = 4 was fitted when torch and Rust shared one aarch64 CPU (spike 028: k_needed peaked at 1.18). A CUDA-trained, Graviton-packed run adds a CUDA-versus-CPU term that A1 never saw. The Graviton cells are 3.2x to 10.0x their A10G noise (3.21, 3.37, 10.01) but only 1.5e-5 to 2.75e-5 in absolute terms, so the 1e-05 floor, not the noise term, decides, and 1e-05 is too small for them. The drift is stable across three repeats on Graviton (identical values), so it is a systematic CPU-and-device term, not run-to-run jitter.

## Structure

**S-B (recommended): additive per-set constants (six numbers) alongside the A1 stamp scalars, record v1 unchanged.** Reasons: no change to RescoreNoise (deny_unknown_fields), train.py, lifecycle.py, laya-finetune-gate-v1 rescore_noise_schema or the train image; the retained motivation-v1-sm v1 record stays verifiable before its S3 objects expire (about 2026-11-28), which keeps SC3 safe; old records and old artifacts stay valid. Accepted debt: k and floor have two meanings in one contract: the stamp the trainer writes versus the enforced per-set bound. What would force a later promote: a trainer that must stamp per-set values, a third scored set, or retiring the A1 stamp.

**S-A (record v2): record v2 carrying per-set k and floor_abs.** Cost: changes RescoreNoise (deny_unknown_fields) in Rust; changes write_noise_record and check_noise_record in Python (train.py, lifecycle.py); changes rescore_noise_schema in laya-finetune-gate-v1.yaml; forces a train-image rebuild; makes the retained motivation-v1-sm v1 record unverifiable unless a v1 reader is kept, which puts SC3 at risk before 2026-11-28.

Assumption-delta decision. Primary noun: the ENFORCED per-set re-score parameters (k_s, floor_s, ceiling_s for s in fine_tuned, zero_shot). Recommended: add-alongside (S-B): the six per-set constants are added next to the A1 scalars, which keep a narrowed meaning (the record STAMP the trainer writes and the absent-record floor). Accepted debt and promote trigger as above. The owner makes the final call; choosing S-A stops the phase for a re-plan.

## Options

### a1-today (not selectable: what refuses today)

Params: fine_tuned noise_k 4 / floor_abs 1e-05 / bound_max_abs 0.001; zero_shot noise_k 4 / floor_abs 1e-05 / bound_max_abs 0.001. The A1 stamp applied to both sets: what the verifier refuses today (Graviton motivation-v1-sm zero_shot 1.728534698486328e-05 against 1e-05).

Production acceptance modelled per cell (policy validity, then the A1 stamp ceiling max(1e-5, 4 x noise) <= 1e-3, then the per-set enforced ceiling, then drift <= bound):

| point | authority | observed | stamp bound | bound | headroom | noise engaged | verdict |
|---|---|---|---|---|---|---|---|
| G-motiv-zs | TRUE | 1.729e-05 | 1e-05 | 1e-05 | 0.58x | no | fail |
| G-smoke-ft | TRUE | 2.748e-05 | 3.259e-05 | 3.259e-05 | 1.19x | yes | pass |
| G-smoke-zs | TRUE | 1.508e-05 | 1.881e-05 | 1.881e-05 | 1.25x | yes | pass |
| G-motiv-ft | TRUE | <= 1.302e-05 | 1.302e-05 | 1.302e-05 | 1.00x | yes | pass (>=) |
| M-smoke-ft | false | 3.439e-05 | 3.259e-05 | 3.259e-05 | 0.95x | yes | fail |
| M-motiv-zs | false | 1.49e-05 | 1e-05 | 1e-05 | 0.67x | no | fail |
| M-mps-2 | false | 9.149e-06 | 7.283e-05 | 7.283e-05 | 7.96x | yes | pass |
| M-mps-3 | false | 3.338e-06 | 5.01e-05 | 5.01e-05 | 15.01x | yes | pass |
| M-mps-4 | false | 1.132e-05 | 3.336e-05 | 3.336e-05 | 2.95x | yes | pass |
| M-mps-5 | false | 7.346e-06 | 5.982e-05 | 5.982e-05 | 8.14x | yes | pass |
| M-mps-6 | false | 7.555e-06 | 1.437e-05 | 1.437e-05 | 1.90x | yes | pass |
| M-mps-7 | false | 1.195e-05 | 6.028e-05 | 6.028e-05 | 5.04x | yes | pass |

### sb-recommended (S-B)

Params: fine_tuned noise_k 4 / floor_abs 5e-05 / bound_max_abs 0.001; zero_shot noise_k 4 / floor_abs 3e-05 / bound_max_abs 0.001. fine_tuned is a fresh draw on every run (Graviton 2.7477741241455078e-05 on 459 rows, at most 1.3021142293956345e-05 on 100 rows), zero_shot is the fixed base on the eval rows (1.728534698486328e-05 and 1.5079975128173828e-05), so the fine_tuned floor carries more absolute slack; a single-attempt reproduction (D-12) cannot afford a post-gate pack refusal.

Production acceptance modelled per cell (policy validity, then the A1 stamp ceiling max(1e-5, 4 x noise) <= 1e-3, then the per-set enforced ceiling, then drift <= bound):

| point | authority | observed | stamp bound | bound | headroom | noise engaged | verdict |
|---|---|---|---|---|---|---|---|
| G-motiv-zs | TRUE | 1.729e-05 | 1e-05 | 3e-05 | 1.74x | no | pass |
| G-smoke-ft | TRUE | 2.748e-05 | 3.259e-05 | 5e-05 | 1.82x | no | pass |
| G-smoke-zs | TRUE | 1.508e-05 | 1.881e-05 | 3e-05 | 1.99x | no | pass |
| G-motiv-ft | TRUE | <= 1.302e-05 | 1.302e-05 | 5e-05 | 3.84x | no | pass (>=) |
| M-smoke-ft | false | 3.439e-05 | 3.259e-05 | 5e-05 | 1.45x | no | pass |
| M-motiv-zs | false | 1.49e-05 | 1e-05 | 3e-05 | 2.01x | no | pass |
| M-mps-2 | false | 9.149e-06 | 7.283e-05 | 7.283e-05 | 7.96x | yes | pass |
| M-mps-3 | false | 3.338e-06 | 5.01e-05 | 5.01e-05 | 15.01x | yes | pass |
| M-mps-4 | false | 1.132e-05 | 3.336e-05 | 5e-05 | 4.42x | no | pass |
| M-mps-5 | false | 7.346e-06 | 5.982e-05 | 5.982e-05 | 8.14x | yes | pass |
| M-mps-6 | false | 7.555e-06 | 1.437e-05 | 5e-05 | 6.62x | no | pass |
| M-mps-7 | false | 1.195e-05 | 6.028e-05 | 6.028e-05 | 5.04x | yes | pass |

- fine_tuned: the noise term engages above 1.25e-05 of A10G noise; reached by the cells M-mps-2 (the cells it does reach are MPS-trained Mac artifacts, not CUDA).
- zero_shot: the noise term engages above 7.5e-06 of A10G noise; reached by the cells M-mps-3, M-mps-5, M-mps-7 (the cells it does reach are MPS-trained Mac artifacts, not CUDA).

Stated plainly, because D-05 frames the amendment as keeping the noise-referenced formula: for sb-recommended no measured CUDA set reaches the noise term (largest A10G noise, fine_tuned 8.147599085628343e-06 against the engage level 1.25e-05; zero_shot 4.70304053501458e-06 against 7.5e-06). So for CUDA runs its bound is in effect the absolute floor (5e-05 fine_tuned, 3e-05 zero_shot); the formula and its ceiling remain, and the noise term governs only noisier (here, MPS) checkpoints.

### sb-minimal (S-B)

Params: fine_tuned noise_k 4 / floor_abs 1e-05 / bound_max_abs 0.001; zero_shot noise_k 4 / floor_abs 2.5e-05 / bound_max_abs 0.001. Smallest blast radius: fine_tuned unchanged, only the zero_shot cell that refused moves; the unmeasured motivation-v1-sm fine_tuned Graviton cell is bounded only at 1.00x.

Production acceptance modelled per cell (policy validity, then the A1 stamp ceiling max(1e-5, 4 x noise) <= 1e-3, then the per-set enforced ceiling, then drift <= bound):

| point | authority | observed | stamp bound | bound | headroom | noise engaged | verdict |
|---|---|---|---|---|---|---|---|
| G-motiv-zs | TRUE | 1.729e-05 | 1e-05 | 2.5e-05 | 1.45x | no | pass |
| G-smoke-ft | TRUE | 2.748e-05 | 3.259e-05 | 3.259e-05 | 1.19x | yes | pass |
| G-smoke-zs | TRUE | 1.508e-05 | 1.881e-05 | 2.5e-05 | 1.66x | no | pass |
| G-motiv-ft | TRUE | <= 1.302e-05 | 1.302e-05 | 1.302e-05 | 1.00x | yes | pass (>=) |
| M-smoke-ft | false | 3.439e-05 | 3.259e-05 | 3.259e-05 | 0.95x | yes | fail |
| M-motiv-zs | false | 1.49e-05 | 1e-05 | 2.5e-05 | 1.68x | no | pass |
| M-mps-2 | false | 9.149e-06 | 7.283e-05 | 7.283e-05 | 7.96x | yes | pass |
| M-mps-3 | false | 3.338e-06 | 5.01e-05 | 5.01e-05 | 15.01x | yes | pass |
| M-mps-4 | false | 1.132e-05 | 3.336e-05 | 3.336e-05 | 2.95x | yes | pass |
| M-mps-5 | false | 7.346e-06 | 5.982e-05 | 5.982e-05 | 8.14x | yes | pass |
| M-mps-6 | false | 7.555e-06 | 1.437e-05 | 1.437e-05 | 1.90x | yes | pass |
| M-mps-7 | false | 1.195e-05 | 6.028e-05 | 6.028e-05 | 5.04x | yes | pass |

- fine_tuned: the noise term engages above 2.5e-06 of A10G noise; reached by the cells G-smoke-ft, G-motiv-ft, M-smoke-ft, M-mps-2, M-mps-4, M-mps-6 (of these only G-smoke-ft, G-motiv-ft, M-smoke-ft are CUDA-trained).
- zero_shot: the noise term engages above 6.25e-06 of A10G noise; reached by the cells M-mps-3, M-mps-5, M-mps-7 (the cells it does reach are MPS-trained Mac artifacts, not CUDA).

HEADLINE RISK: the motivation-v1-sm fine_tuned Graviton cell has never been measured. Under sb-minimal its bound is 1.302e-05, which equals the upper bound 1.302e-05 only at 1.00x (verdict pass (>=)): there is no headroom on a cell nobody has seen. The plan 15-06 re-pack is its first measurement and may refuse (one Fargate task and an owner decision, no GPU). Graviton smoke fine_tuned headroom is only 1.19x.

### sb-uniform (S-B)

Params: fine_tuned noise_k 4 / floor_abs 5e-05 / bound_max_abs 0.001; zero_shot noise_k 4 / floor_abs 5e-05 / bound_max_abs 0.001. One number for both sets; loosens zero_shot, the deterministic base re-score, more than any cell needs.

Production acceptance modelled per cell (policy validity, then the A1 stamp ceiling max(1e-5, 4 x noise) <= 1e-3, then the per-set enforced ceiling, then drift <= bound):

| point | authority | observed | stamp bound | bound | headroom | noise engaged | verdict |
|---|---|---|---|---|---|---|---|
| G-motiv-zs | TRUE | 1.729e-05 | 1e-05 | 5e-05 | 2.89x | no | pass |
| G-smoke-ft | TRUE | 2.748e-05 | 3.259e-05 | 5e-05 | 1.82x | no | pass |
| G-smoke-zs | TRUE | 1.508e-05 | 1.881e-05 | 5e-05 | 3.32x | no | pass |
| G-motiv-ft | TRUE | <= 1.302e-05 | 1.302e-05 | 5e-05 | 3.84x | no | pass (>=) |
| M-smoke-ft | false | 3.439e-05 | 3.259e-05 | 5e-05 | 1.45x | no | pass |
| M-motiv-zs | false | 1.49e-05 | 1e-05 | 5e-05 | 3.36x | no | pass |
| M-mps-2 | false | 9.149e-06 | 7.283e-05 | 7.283e-05 | 7.96x | yes | pass |
| M-mps-3 | false | 3.338e-06 | 5.01e-05 | 5.01e-05 | 15.01x | yes | pass |
| M-mps-4 | false | 1.132e-05 | 3.336e-05 | 5e-05 | 4.42x | no | pass |
| M-mps-5 | false | 7.346e-06 | 5.982e-05 | 5.982e-05 | 8.14x | yes | pass |
| M-mps-6 | false | 7.555e-06 | 1.437e-05 | 5e-05 | 6.62x | no | pass |
| M-mps-7 | false | 1.195e-05 | 6.028e-05 | 6.028e-05 | 5.04x | yes | pass |

- fine_tuned: the noise term engages above 1.25e-05 of A10G noise; reached by the cells M-mps-2 (the cells it does reach are MPS-trained Mac artifacts, not CUDA).
- zero_shot: the noise term engages above 1.25e-05 of A10G noise; reached by the cells M-mps-3, M-mps-5, M-mps-7 (the cells it does reach are MPS-trained Mac artifacts, not CUDA).

### sa-record-v2 (S-A) and custom

sa-record-v2 carries no params here: choosing it stops the phase for a re-plan (Structure above). custom: the owner states the six numbers; constraint: every value, noise_k included, a finite number (not a bool), noise_k > 0, 1e-05 <= floor_abs <= bound_max_abs <= 0.001, and every measured Graviton cell passing the full acceptance (policy, stamp ceiling, enforced ceiling, drift), checked by Task 3.

## Red line

The red line argued against: the smallest cited porting regression is a local-window off-by-one: |dp| 1.4e-2 at window 63, 2.8e-2 and an argmax flip at 65 (`contracts/laya-parity-v1.yaml` window_mutation). Argmax stays exact under every option. Three different quantities are kept apart per option:

| option | floor headroom (1.4e-2 / max floor) | observed-cell headroom (Graviton, from the grid) | maximum permitted bound (max ceiling) and its ceiling headroom |
|---|---|---|---|
| sb-recommended | floor headroom 280x (max floor 5e-05) | observed-cell headroom: G-motiv-zs 1.74x, G-smoke-ft 1.82x, G-smoke-zs 1.99x, G-motiv-ft 3.84x (upper bound) | maximum permitted bound 0.001, ceiling headroom 14x |
| sb-minimal | floor headroom 560x (max floor 2.5e-05) | observed-cell headroom: G-motiv-zs 1.45x, G-smoke-ft 1.19x, G-smoke-zs 1.66x, G-motiv-ft 1.00x (upper bound) | maximum permitted bound 0.001, ceiling headroom 14x |
| sb-uniform | floor headroom 280x (max floor 5e-05) | observed-cell headroom: G-motiv-zs 2.89x, G-smoke-ft 1.82x, G-smoke-zs 3.32x, G-motiv-ft 3.84x (upper bound) | maximum permitted bound 0.001, ceiling headroom 14x |

The 280x figure is ONLY the floor headroom of sb-recommended (1.4e-2 / 5e-05). Its 1.0e-3 ceiling permits a noise-derived bound that is only 14x below 1.4e-2, reached when a checkpoint's own noise is large (noise of 2.5e-4 or more). The region between the bound and 1.4e-2 is unmeasured: no porting regression between them has been exercised against these bars.

## The reproduction's single attempt

The P14-D-16 motivation-v1 reproduction runs once (D-12, attempt cap 1). A pack refusal after a gate pass is class defect and is never retriable; attempt 1 of Phase 14 was exactly that (the zero_shot Graviton cell above). The reproduction's fine_tuned drift is a fresh draw on a new checkpoint. A bound that sits at 1.00x on an unmeasured cell (sb-minimal) can therefore consume the only attempt after a gate pass; the larger fine_tuned floor of sb-recommended and sb-uniform exists to avoid that.

## Limits of the claim

- x86_64 CPU: there is no x86_64 pack binary or image (new infrastructure, not planned)
- Graviton generation split: the production pack image cannot print its CPU generation, so host heterogeneity on Fargate is observable only as run-to-run spread across repeats
- a fresh A10G run: the A10G noise is already recorded in each run's rescore-noise.json; no GPU job is started
- the second set of a refused leg: pack_laya reports only the FIRST drifted set (exposing it is a decide-crate change, D-15)
- eval size differs (100 rows motivation, 459 rows smoke) and a larger eval set makes the max harder (spike 028)
- the P14-D-16 rerun's fine_tuned drift is a fresh draw
- [ASSUMED] the stable absolute drift is the CUDA-versus-CPU term, which no measurement isolates
- [ASSUMED] no porting regression of interest lies between the bound and 1.4e-2; that region is unmeasured

## Role of the local laya-verify

D-08: the Graviton pack task's record (`pack-outcome.json`) becomes the eligibility; the Mac `laya-verify` becomes advisory for device-float noise. Two concrete roles, each with a fixed consequence table that plan 15-03 implements:

### role-drift-advisory (recommended)

With a valid pack record (accepted true, deploy_eligible true, apr_sha256 equal to the local file's sha256), ONLY a local `REFUSED RescoreDrift` is logged as a cross-CPU finding; every other local refusal still blocks; no record keeps today's local gate.

| situation | consequence |
|---|---|
| rescore_drift | logged |
| other_local_refusal | blocks |
| gate_failed_exit_3 | blocks |
| record_invalid_or_sha_mismatch | blocks |
| explicit_record_absent | blocks |
| convention_record_absent | local gate |
| no_record | local gate |

### role-all-advisory

With a valid pack record every local refusal is logged and none blocks; no record keeps today's local gate.

| situation | consequence |
|---|---|
| rescore_drift | logged |
| other_local_refusal | logged |
| gate_failed_exit_3 | logged |
| record_invalid_or_sha_mismatch | blocks |
| explicit_record_absent | blocks |
| convention_record_absent | local gate |
| no_record | local gate |

Under both roles an explicitly named record that does not exist blocks, and a run with no record (including a convention-derived record path that does not exist) keeps the local verify as its gate. Honest limit: the record's authenticity rests on the fetch manifest sha256 and the apr_sha256 equality; a hand-forged `pack-outcome.json` in a local directory is not detectable by the recipe alone.

## Open questions

- Q1 (which structure): decided here: S-B recommended, S-A listed with its cost
- Q2 (what the local laya-verify may demote, and no-record behaviour): decided here (verify_roles): role-drift-advisory recommended; a run with no record keeps the local gate
- Q3 (recipe_id before the run): computed offline with scripts/laya_train/contract.py recipe_id_for at plan 15-07
- Q4 (which training levers may move): only max epochs within [4, 12] and the dataset are movable without a laya-finetune-gate-v1 edit; decided at plan 15-07's checkpoint
- Q5 (fetching the re-packed .apr): not needed: D-09 is shown on real bytes by smoke-stance-64 (plan 15-03) and by the reproduction's fetched artifact (plan 15-08)

## What the sign-off authorizes

Exactly: the contract and recipe edits of plans 15-02 and 15-03. No paid run; each paid run keeps its own approval.

## Reply format

One bound option id and one role id, for example `sb-recommended role-drift-advisory`. For custom, the six numbers. The orchestrator records the reply verbatim in `15-BOUND-DECISION.json`.
