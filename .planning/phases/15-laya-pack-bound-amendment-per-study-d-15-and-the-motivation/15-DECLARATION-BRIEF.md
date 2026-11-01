# Phase 15 declaration brief: the single trust-bar rerun (plan 15-07)

Prepared 2026-11-01 from attempt 1's records (`models/decide/motivation-v1-sm/`, task `a2cbacae-529c-4703-a796-b6eda6742446`) and the plan 15-06
re-pack. Every MUST value below was computed offline and each draft passes `laya_repro_eval.py check-declaration` (DECLARATION OK). Nothing here
has been run on AWS; no paid run is authorized by this document.

## What is fixed

These are decided (D-11, D-12, D-02, D-13) and are not on the ballot:

- Band (D-11), set before the first run and not moved after attempt 1: margin [0.30, 0.45] and post-calibration ECE [0.07, 0.12].
- Effective pass window, stated plainly: margin [0.30, 0.45], but post-calibration ECE effectively [0.07, 0.10]. The gate refuses ECE above
  `gate_max_ece` 0.10 (`contracts/laya-finetune-gate-v1.yaml`), and that ceiling cuts into the declared [0.07, 0.12] band. An ECE of 0.11 is inside
  the band and still a GATE FAIL. The drafts carry this as `effective_ece_window` [0.07, 0.10]; the evaluator judges band and gate separately.
- Attempt cap 1, `retry_classes` [] (D-12). A run that ends before any gate verdict (crash, quota, timeout: `outcome.json` exit_class refused or
  crash, no `run/gate-report.json`, no GATE PASS / GATE FAIL line in `train.log`) is not an attempt; it is recorded under `pre_verdict_failures` and
  any re-run needs the owner's new approval. A run whose gate executed is the attempt even if a file failed to upload. A run with missing or
  inconclusive evidence (no fetch, no outcome.json) is unresolved and the owner decides; it is never counted as "not an attempt". A miss is
  recorded not-reproduced with `next_trusted` false.
- No waiver and no overrides entry stands in for the trust bar (D-02). `waiver_rule` is `inconclusive`.
- Nothing after a miss is pre-authorized (D-13). The declaration names no follow-up phase or run.
- Fixed inputs: `data/decide/motivation-v1` (task_json f74a3ad3..., train_jsonl f1d84f19..., eval_jsonl ee95d71f..., identical to Phase 12 and
  attempt 1), seeds [13, 17, 23], device prefix `cuda`, `deploy_eligible` true, judged on the amended pack image
  `sha256:dd38865c43aba2f3819e50addcf6db5e9da94ef84e762d93cf6b2c132f1d17e2` under laya-parity-v1 A2 and the signed bound (S-B, role
  `role-drift-advisory`, bound_max_abs 0.001). Plan 15-06 already showed this image accepts attempt 1's Graviton re-pack (SC3), so the pack
  refusal that ended attempt 1 is removed; the margin is a fresh draw either way.

## Attempt 1 per seed and per epoch

Data: fit 220 rows, calibration slice 75, eval 100, 28 steps per epoch (336 steps at 12 epochs). Zero-shot macro-F1 0.3696.

| Seed | best epoch | epochs run (of max 12) | macro-F1 | margin | ECE post | rank key | result |
|------|-----------|------------------------|----------|--------|----------|----------|--------|
| 13 | 2 | 5 | 0.7096 | 0.3400 | 0.1124 | 1124 | gate fail (ECE above 0.10) |
| 17 | 2 | 5 | 0.6556 | 0.2860 | 0.0964 | 963 | shipped (median ECE); gate PASS, margin below 0.30 |
| 23 | 1 | 4 | 0.7236 | 0.3541 | 0.0767 | 767 | would pass both |

Per-epoch shape (from `train.log`): every seed stopped by patience 3. Training cross-entropy `ce_last` reaches 0.0000 by epoch 2 or 3, and the
fitted temperature t* sits at the 5.0 cap from epoch 3 (seed 23 from epoch 2). Calibration NLL at t* is lowest at epoch 1 or 2 (seed 13: 0.636,
0.486, then 0.80, 0.72, 0.92; seed 17: 0.539, 0.467, then 0.76, 0.56, 0.57; seed 23: 0.480, then 0.59, 0.75, 0.73). So the selected checkpoint is
taken early in a 12-epoch cosine schedule, when the learning rate is still near its peak.

Learning-rate factor (cosine from peak to eta_min, 0.5 x (1 + cos(pi x step / total))):

| End of | 12-epoch schedule (336 steps) | 4-epoch schedule (112 steps) |
|--------|------------------------------|------------------------------|
| epoch 1 (step 28) | about 0.98 | about 0.85 |
| epoch 2 (step 56) | about 0.93 | 0.50 |
| epoch 3 (step 84) | about 0.85 | about 0.15 |

## How the shipped seed is chosen

The band is judged on the SHIPPED seed. `seed_selection` policy `median_ece` ships the seed in the middle of the three by post-calibration ECE
(`rank_key` = floor(ECE x 10000), ties to the smaller seed), not the middle by margin. Attempt 1 shipped seed 17, its LOWEST-margin seed (0.2860
against 0.3400 and 0.3541), because seed 17's ECE (0.0964) was the median. Seeds 13 and 23 would have cleared the 0.30 margin floor on their own.

Consequence for any lever: a change that improves or reshuffles the ECE ranking can ship a different, possibly lower-margin, seed. The single
draw is honest only if read this way: it is one shipped seed picked by calibration, judged on margin and ECE.

## Levers

Movable without editing `laya-finetune-gate-v1`:

- Maximum epochs, within [`epochs_above_16_min` 4, `epochs_above_16_max` 12] at 188 shots per class. Early stopping makes `epochs` a maximum; it changes
  the cosine schedule's length and so the learning rate at each epoch.
- The dataset (changing it means a new prefix and a `prepare_motivation.py` change; that is `revise-plan`, below).

Pinned by `laya-finetune-gate-v1` (each a contract edit outside this plan): encoder and head learning rates, batch size, the seed set, eval size.
There is no warmup in the recipe, so none to move.

## Proposed changes

Recommended: `epochs-4` (max epochs 12 -> 4, same data, same seeds).

- New recipe id `a9f1c4a7176655664f391e3237a3205d1c9989155160edb2d78ef46808ba5087`, computed with the trainer's own builder
  (`contract.recipe_id_for`). Positive control: the attempt-1 arguments reproduce
  `f88f5b71ef925fdabb30d26a15c2eb5195a151dadfe0882c858b73ca7bf54d7e`, byte-equal to attempt 1's `recipe.json`.
- Because the id differs, Phase 12's pilot waiver `motivation-v1-pilot-ece` cannot apply.
- Rationale from the log: the selected epoch (1 or 2) happens under a near-peak learning rate; a 4-epoch schedule roughly halves the factor at epoch
  2 (0.93 -> 0.50) and decays further by epoch 3, which may reduce the overconfidence the log shows (ce 0.0, t* at the cap).
- [ASSUMED] The direction of this effect on margin, and on which seed's ECE is the median and therefore ships, is unmeasured. It is a hypothesis from the
  log, not a result, and the outcome is one draw. Epochs 4 could equally move the shipped seed to a lower-margin one.

## Alternatives

- `epochs-12-redraw`: identical recipe (id f88f5b71..., the same as Phase 12 and attempt 1) and data, new pack image only. Closest to attempt 1 and
  isolates the amendment's effect. Attempt 1 with this recipe missed the band on margin; Phase 12's pilot waiver binds this recipe id, so a waived
  acceptance is possible and counts only as inconclusive.
- `revise-plan`: any data, eval-size, seed-set, learning-rate or batch-size change. These need a `laya-finetune-gate-v1` edit (a second one-way
  contract change) or a new dataset prefix. This stops the phase for re-planning at CHECKPOINT REACHED and commits NO declaration.

## Claim scope

Both drafts' `claim_scope` starts with the same literal and names the actual changes after it:

`gate outcome and band only; not an identical reproduction of attempt 1 or of Phase 12: <the changes>; bytes differ by design; no determinism claim`

For epochs-4 the changes are the 4-epoch maximum and the amended pack image; for epochs-12-redraw, the pack image only.

## Cost

Attempt 1: 1346 s billable on ml.g5.xlarge (`train_seconds` 1176.4, of which the three trainings took only about 54.7 + 50.7 + 40.0 s; the rest is
scoring and rescore-noise), plus a 124.16 s Fargate pack. Epochs 4 shortens only the training loop (about 112 steps against the 140 attempt 1
already ran on seeds 13 and 17), so expect a similar bill for either option. `train_wall_seconds_max` 2256 is report-only: never a MUST clause, the
evaluator does not read it.

## What the reply authorizes

- Reply with one option id and `run-go` or `run-hold`, e.g. `epochs-4 run-go`.
- The orchestrator records it verbatim in `15-DECLARATION-OWNER-REPLY.json` (committed alone). Task 3 then commits `15-REPRODUCTION-DECLARATION.json`
  (the chosen draft, with `run_authorized` equal to the recorded `run_go`).
- With `run-go`: the declaration commit plus the single paid run of plan 15-08 (one GPU training job, one Fargate pack; cap 1).
- With `run-hold`: the declaration is committed with `run_authorized` false and nothing is run. The committed declaration is NEVER edited again (its
  first-add blob is what plan 15-08 hashes and evaluates). A later go is a separate orchestrator-recorded `15-RUN-GO.json` bound to the declaration's
  sha256.
- With `revise-plan`: no declaration is committed; the phase returns to planning.
