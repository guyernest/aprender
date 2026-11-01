# Phase 15 gap brief: what kind of miss was the margin miss (plan 15-09)

Prepared 2026-11-01 from `15-GAP-CLASS-DIAGNOSIS.json` (fixture `15-GAP-MARGIN-INPUTS.json`, class rule `rule_sha256` committed before the analysis ran).
Every figure below is a `brief_facts` string of the diagnosis copied verbatim and checked by `just laya-margin-diagnosis check-brief`.

## Question

The one permitted attempt under the epochs-4 declaration was consumed and judged not-reproduced: the shipped seed 13 landed below the margin floor
while the gate passed, the Graviton pack accepted a deploy-eligible artifact, and `next_trusted` is false. Attempt 1 (Phase 14) missed the same clause.
Two attempts have now missed the same clause, so the question for the owner is what to do about the CLASS of the miss, not about this one probe.

This brief spends nothing, runs nothing on AWS, edits no declaration and sets no trust. It names the class from numbers already on disk and lays out
five options with concrete values. Nothing takes effect until your reply is recorded by the orchestrator in plan 15-10.

## The class

class: single-draw-acceptance

shortfall: the shipped seed 13 margin 0.2795 is below the 0.3000 floor by 0.0205

In plain words. The shipped-seed margin is ONE draw from a distribution whose between-seed spread and whose row-resampling error are both larger than
the 0.0205 shortfall, and whose mean sits only a little above the floor:

- CUDA seed-runs: n=6, mean 0.3279, sd 0.0441
- the floor sits 0.6319 sd below the CUDA mean
- row-resampling se of one seed's margin on 100 rows: median 0.0642, range 0.0568 to 0.0765

The class rule (RULE_SPEC, fixed before the result): single-draw-acceptance holds iff the shortfall is no larger than the CUDA sd, no larger than the median
row-resampling se, the declared criterion passes less often than not, and the "recipe is deficient" test is not supported on the full set nor on any
leave-one-out set. All five predicates hold. The other two candidate classes: recipe-deficiency is refuted on the full set (see Evidence), and
device-anchoring is unproven (n=3, see Evidence), which is informational only and never selects the class.

The band was anchored on one MPS draw: 14-RESEARCH.md calls the intervals "the researcher's proposal, [ASSUMED]" placed "around 0.3780 / 0.0936"
(Phase 12's single MPS draw) and says "the median-of-three on a different device is a fresh draw". 15-RESEARCH.md rates the D-16 outcome "LOW (single
draw)" and 15-REVIEWS.md says "The paid-run outcome is a draw by construction".

Consequence: a further attempt under the same judging rule re-draws the same distribution. That closes the probe, not the class.

## Evidence

The nine recorded seed-run margins and post-calibration ECEs, each recomputed from the fixture bit for bit (absolute difference at most 1e-12) against
the tracked gate-run evidence. The shipped seed is the median by ECE, not by margin.

| run | device | epochs | seed | shipped | margin | ece_post |
|-----|--------|--------|------|---------|--------|----------|
| p12 | mps:0 | 12 | 13 | yes | 0.3780 | 0.0936 |
| p12 | mps:0 | 12 | 17 | no | 0.3502 | 0.1197 |
| p12 | mps:0 | 12 | 23 | no | 0.3709 | 0.0914 |
| p14 | cuda:0 | 12 | 13 | no | 0.3400 | 0.1124 |
| p14 | cuda:0 | 12 | 17 | yes | 0.2860 | 0.0964 |
| p14 | cuda:0 | 12 | 23 | no | 0.3541 | 0.0767 |
| p15 | cuda:0 | 4 | 13 | yes | 0.2795 | 0.0973 |
| p15 | cuda:0 | 4 | 17 | no | 0.3122 | 0.0923 |
| p15 | cuda:0 | 4 | 23 | no | 0.3955 | 0.1096 |

p12 is Phase 12 (MPS), p14 is Phase 14 attempt 1 (CUDA), p15 is Phase 15 (CUDA).

- the shipped seed is the lowest-margin seed of its run: p12 no, p14 yes, p15 yes
- declared criterion: p_pass 0.259 (n=6 seed-runs, resampling estimate), leave-one-out range 0.104 to 0.352
- gate (shipped ECE at most 0.1000) passes with probability 0.741 under every rule; declared criterion: defect probability 0.481, in-band gate fail 0.259
- independent re-draw: two misses in a row under the declared rule have probability 0.549
- deficiency: refuted on the full set; leave-one-out undetermined, refuted, undetermined, refuted, refuted, undetermined

The pass probabilities are resampling estimates over six CUDA seed-runs (all 216 ordered triples with replacement, the median-ECE ship rule), with a
leave-one-out range. They are not confidence intervals. Deficiency is refuted on the full set only narrowly: dropping any one of three seed-runs
turns it to undetermined, so the evidence does not robustly rule out a weak recipe, it only fails to support one.

The device gap (informational, never selects the class): device gap (MPS minus CUDA by seed): +0.0380 / +0.0641 / +0.0168, mean +0.0397, t 2.8990, p 0.1012, unproven (n=3).
The three differences share a sign, but three pairs cannot distinguish a device effect from noise (df 2). Explaining the MPS-versus-CUDA gap by a new
measurement stays deferred (15-CONTEXT Deferred Ideas) and is not planned here.

## What the epochs lever did

The one lever tried, max epochs 12 -> 4, did nothing detectable:

- epochs lever (12 -> 4 on CUDA): difference +0.0023, se 0.0403, z +0.0579, minimum detectable 0.1127
- best epoch per seed: p12 [2, 2, 2]; p14 [2, 2, 1]; p15 [1, 1, 2]

The selected checkpoint is early in every run, so the schedule length barely matters. The minimum detectable difference from one more 3-seed run against
another is about 0.1127, five times the 0.0205 shortfall. A new lever therefore cannot be shown to work by one more attempt; the lever is not detectable.

## Options

All paid options share: each paid option: run name motivation-v1-p15b, dataset prefix datasets/motivation-v1/, epochs 12, recipe f88f5b71ef925fdabb30d26a15c2eb5195a151dadfe0882c858b73ca7bf54d7e, seeds [13, 17, 23], device cuda, pack image sha256:dd38865c43aba2f3819e50addcf6db5e9da94ef84e762d93cf6b2c132f1d17e2; report-only: about 1346 s of ml.g5.xlarge billed on attempt 1 plus about 150 s of Fargate pack

The recipe is the Phase 12 and attempt 1 recipe (12 epochs), the closest reproduction, because the epochs-4 lever showed no effect. The gate contract, the
seed set, the learning rates and the eval set are not options in this round.

1. `stop-accept`
   - option stop-accept: no paid run, supersedes none, SC4 stays unmet
   - SC4 stays unmet by your decision. This is not an override.
2. `redraw`
   - option redraw: judging shipped_seed, margin band [0.3000, 0.4500], ece band [0.0700, 0.1200], attempt cap 1, retry classes none, supersedes none, p_pass 0.259, within cap 0.259, leave-one-out 0.104 to 0.352
   - option redraw claim scope: gate outcome and band only; not an identical reproduction of attempt 1 or of Phase 12: no rule change; the Phase 12 and attempt 1 recipe (12 epochs) is drawn again, the shipped seed's margin judged in [0.3000, 0.4500] as before
3. `mean-rule`
   - option mean-rule: judging seed_mean, margin band [0.3000, 0.4500], ece band [0.0700, 0.1200], attempt cap 1, retry classes none, supersedes D-11, p_pass 0.634, within cap 0.634, leave-one-out 0.464 to 0.736
   - option mean-rule claim scope: gate outcome and band only; not an identical reproduction of attempt 1 or of Phase 12: the margin clause is judged on the mean of the three seed margins in [0.3000, 0.4500] instead of the shipped seed's margin (D-11 superseded); the ECE clause and the gate stay on the shipped seed
4. `mean-rule-retry`
   - option mean-rule-retry: judging seed_mean, margin band [0.3000, 0.4500], ece band [0.0700, 0.1200], attempt cap 2, retry classes in-band-gate-fail, supersedes D-11, D-12, p_pass 0.634, within cap 0.799, leave-one-out 0.627 to 0.833
   - option mean-rule-retry claim scope: gate outcome and band only; not an identical reproduction of attempt 1 or of Phase 12: the mean-of-three margin rule (D-11 superseded) and one second paid run allowed after an in-band-gate-fail (D-12 superseded); a defect is never retried
   - This pre-authorizes a second paid run after an in-band gate fail, which D-13 otherwise forbids, so it needs your explicit reply.
5. `floor-rule`
   - option floor-rule: judging shipped_seed, margin band [0.2300, 0.4500], ece band [0.0700, 0.1200], attempt cap 1, retry classes none, supersedes D-11, p_pass 0.741, within cap 0.741, leave-one-out 0.648 to 0.896
   - option floor-rule claim scope: gate outcome and band only; not an identical reproduction of attempt 1 or of Phase 12: the shipped seed's margin floor moves from 0.3000 to 0.2300 (D-11 superseded); ceiling 0.4500 and the ECE clause unchanged
   - mean-rule p_pass 0.634; mean-rule-retry within cap 0.799; floor-rule with floor 0.2300 p_pass 0.741

What the numbers say about the options, stated plainly. `redraw` changes nothing, so it fails the way the last two attempts failed about three times in four
(defect probability 0.481 is the margin leaving its band). `mean-rule` removes most of that margin variance by judging the mean of the three seed margins,
but the ECE clause still fails the gate about one run in four, so it does not get above the gate's own 0.741. `floor-rule` moves the floor to 0.2300, the
CUDA mean minus two sd rounded down, which is derived from the same six draws it is then scored on: its 0.741 is an in-sample figure and equals the gate's
pass probability because no resampled margin falls below 0.2300. The within-cap figure for `mean-rule-retry` counts a second run only after an in-band gate fail.

## Recommendation

recommended: mean-rule

The diagnosis's rule picks `mean-rule` iff the class is single-draw-acceptance AND the mean-rule p_pass exceeds the redraw p_pass, else `stop-accept`. Both
conditions hold (0.634 against 0.259). The reason is the class: the miss is a single noisy draw judged against a floor barely below the recipe's own mean,
so a judging statistic that averages the three seeds addresses the cause, while `redraw` re-draws it and the epochs lever cannot be shown to help.

What would make `stop-accept` the better choice: a refusal to change the judging rule after two misses. D-11 and D-12 were your decisions, made before the first
run; every rule option (`mean-rule`, `mean-rule-retry`, `floor-rule`) supersedes at least D-11 after seeing the misses. If you hold that a bar moved after a
miss is not a bar, `stop-accept` records SC4 as unmet and leaves the training server untrusted, which is a legitimate and complete outcome.

## Safeguards

- The two consumed attempts are never re-judged to set trust. The post hoc view of them under a mean statistic is exploratory and sets nothing:
  post hoc view of the consumed runs (exploratory, sets nothing): p14 mean margin 0.3267, p15 mean margin 0.3290.
- Any rule change takes effect only for a NEW run under a declaration committed before it. Trust (`next_trusted`) is possible only from a genuine in-band
  result of that new run, or from an owner-signed new band committed before it (plans 15-10 to 15-12). Every artifact this plan wrote records
  `next_trusted_effect` none.
- The risk of moving a goalpost after a miss is named here: a rule chosen after seeing two misses and then met by the next run would be a weaker claim than the
  original bar, and its claim scope says so (gate outcome and band only; not an identical reproduction).
- D-11 (the band stays unchanged) and D-12 (attempt cap 1, no retry) are the owner's own decisions; only the owner's recorded reply can supersede them.
  No waiver and no overrides entry stands in for the trust bar (D-02).

## What each reply authorizes

- `stop-accept` ends this gap round with SC4 recorded unmet. No declaration, no run.
- A paid option (`redraw`, `mean-rule`, `mean-rule-retry`, `floor-rule`) commits a new declaration in plan 15-11 and, with `run-go`, authorizes ONE owner-run
  attempt under it.
- `run-hold` commits the declaration with `run_authorized` false; a later orchestrator-recorded go is a separate reply.
- Nothing here is an AWS action by an executor (D-03): every AWS command stays owner-run with the bang prefix.

## Limits of the claim

- Six CUDA seed-runs and a 100-row eval set (class counts 23, 14, 63). The probabilities are resampling estimates, not confidence intervals, and a
  leave-one-out range is a sensitivity check, not an interval.
- The MPS-versus-CUDA comparison has three pairs. It is stated as unproven.
- All parity and drift numbers elsewhere in the phase are aarch64; x86_64 is unmeasured. This analysis uses only the recorded margins and ECEs.
- The reply record scopes the claim: it records the option, the run-go flag and the superseded decisions, and is written only by the orchestrator.
- Out of scope in this round: any change to the gate contract, the seed set, the learning rates or the eval set; a dataset-growth mechanism; explaining the
  MPS-versus-CUDA gap by a new measurement.

## Reply format

One option id, plus `run-go` or `run-hold` for a paid option. For example: `mean-rule run-hold`, or `stop-accept`. Any other text is not recorded and
returns the phase to planning.
