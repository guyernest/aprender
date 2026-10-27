# Phase 12 hand-off: motivation-v1 decide server (PILOT)

**For:** the owner, and experiment-coach (the partner that will pin, threshold and reweight on it). **Written:** 2026-10-26 (plan 12-16).
**Delivery:** this file lives in the aprender phase directory; nothing was written into the partner repo.

Every number below either sits in a generated table (the verbatim output of `bakeoff.py --render-tables` over 12-BAKEOFF-EVIDENCE.json) or in the `Numbers and their records` table at the end, one row per number with its record file and JSON pointer. No post text, account id or bucket name appears anywhere in this file. Clients pin by endpoint and sha256 and nothing else is promised.

## 0. Lead: the false-assignment rate, per arm and per length band

A false assignment is a post whose true class is `other` that receives a function (social_affiliative or conspiratorial) at the threshold. In experiment-coach's field use it is a matched message sent to a real account, so this is the number that matters. The first table gives it for every arm, overall and inside each character-length tertile, at experiment-coach's `motivation_min_prob` of 0.6 and at the guide's example threshold of 0.85; the RECALL table that follows gives the true-class recall at the same two thresholds. The eval split is 100 held-out rows with 63 `other` rows overall, about 28, 21 and 14 per band: the per-band rates rest on very few rows, and the conspiratorial class has only 14 eval rows (2, 5 and 7 per band). Read the per-band figures as indications, not as measurements of a rate.

**Threshold disclosure (D-05).** These rates count a probability strictly above the threshold; experiment-coach assigns at p >= motivation_min_prob (motivation-experiment crates/core/src/treatment.rs:59), so at exact equality its counts can be higher: the client-equivalent (>=) counts are the `client_ge` column. On this eval set the two agree at both thresholds (see the numbers table), but the declared rates are not an exact replay of the client.

<!-- generated:bakeoff-tables begin -->
Eval rows: 100; length cuts c1 = 126.0000, c2 = 244.0000 (code points); band counts [34, 33, 33].

### LEAD: false assignment (eval `other` rows whose largest non-other probability is > threshold)

| arm | scope | other rows | >0.6 num/den | rate | client_ge num | >0.85 num/den | rate | client_ge num |
|---|---|---|---|---|---|---|---|---|
| laya | all | 63 | 7/63 | 0.1111 | 7 | 4/63 | 0.0635 | 4 |
| laya | band 0 (len <= c1) | 28 | 2/28 | 0.0714 | 2 | 1/28 | 0.0357 | 1 |
| laya | band 1 (c1 < len <= c2) | 21 | 1/21 | 0.0476 | 1 | 1/21 | 0.0476 | 1 |
| laya | band 2 (len > c2) | 14 | 4/14 | 0.2857 | 4 | 2/14 | 0.1429 | 2 |
| laya_zero_shot | all | 63 | 6/63 | 0.0952 | 6 | 1/63 | 0.0159 | 1 |
| laya_zero_shot | band 0 (len <= c1) | 28 | 3/28 | 0.1071 | 3 | 1/28 | 0.0357 | 1 |
| laya_zero_shot | band 1 (c1 < len <= c2) | 21 | 2/21 | 0.0952 | 2 | 0/21 | 0.0000 | 0 |
| laya_zero_shot | band 2 (len > c2) | 14 | 1/14 | 0.0714 | 1 | 0/14 | 0.0000 | 0 |
| setfit | all | 63 | 13/63 | 0.2063 | 13 | 0/63 | 0.0000 | 0 |
| setfit | band 0 (len <= c1) | 28 | 2/28 | 0.0714 | 2 | 0/28 | 0.0000 | 0 |
| setfit | band 1 (c1 < len <= c2) | 21 | 8/21 | 0.3810 | 8 | 0/21 | 0.0000 | 0 |
| setfit | band 2 (len > c2) | 14 | 3/14 | 0.2143 | 3 | 0/14 | 0.0000 | 0 |
| kev | blocked (budget) | - | - | - | - | - | - | - |

### RECALL: true rows assigned to their class at the threshold (p > threshold)

| arm | scope | class | @0.6 assigned/support | recall | @0.85 assigned/support | recall |
|---|---|---|---|---|---|---|
| laya | all | social_affiliative | 17/23 | 0.7391 | 14/23 | 0.6087 |
| laya | all | conspiratorial | 8/14 | 0.5714 | 5/14 | 0.3571 |
| laya | band 0 (len <= c1) | social_affiliative | 3/4 | 0.7500 | 1/4 | 0.2500 |
| laya | band 0 (len <= c1) | conspiratorial | 2/2 | 1.0000 | 2/2 | 1.0000 |
| laya | band 1 (c1 < len <= c2) | social_affiliative | 3/7 | 0.4286 | 3/7 | 0.4286 |
| laya | band 1 (c1 < len <= c2) | conspiratorial | 3/5 | 0.6000 | 1/5 | 0.2000 |
| laya | band 2 (len > c2) | social_affiliative | 11/12 | 0.9167 | 10/12 | 0.8333 |
| laya | band 2 (len > c2) | conspiratorial | 3/7 | 0.4286 | 2/7 | 0.2857 |
| laya_zero_shot | all | social_affiliative | 1/23 | 0.0435 | 0/23 | 0.0000 |
| laya_zero_shot | all | conspiratorial | 6/14 | 0.4286 | 1/14 | 0.0714 |
| laya_zero_shot | band 0 (len <= c1) | social_affiliative | 1/4 | 0.2500 | 0/4 | 0.0000 |
| laya_zero_shot | band 0 (len <= c1) | conspiratorial | 1/2 | 0.5000 | 1/2 | 0.5000 |
| laya_zero_shot | band 1 (c1 < len <= c2) | social_affiliative | 0/7 | 0.0000 | 0/7 | 0.0000 |
| laya_zero_shot | band 1 (c1 < len <= c2) | conspiratorial | 2/5 | 0.4000 | 0/5 | 0.0000 |
| laya_zero_shot | band 2 (len > c2) | social_affiliative | 0/12 | 0.0000 | 0/12 | 0.0000 |
| laya_zero_shot | band 2 (len > c2) | conspiratorial | 3/7 | 0.4286 | 0/7 | 0.0000 |
| setfit | all | social_affiliative | 15/23 | 0.6522 | 10/23 | 0.4348 |
| setfit | all | conspiratorial | 6/14 | 0.4286 | 4/14 | 0.2857 |
| setfit | band 0 (len <= c1) | social_affiliative | 2/4 | 0.5000 | 1/4 | 0.2500 |
| setfit | band 0 (len <= c1) | conspiratorial | 2/2 | 1.0000 | 1/2 | 0.5000 |
| setfit | band 1 (c1 < len <= c2) | social_affiliative | 4/7 | 0.5714 | 4/7 | 0.5714 |
| setfit | band 1 (c1 < len <= c2) | conspiratorial | 2/5 | 0.4000 | 2/5 | 0.4000 |
| setfit | band 2 (len > c2) | social_affiliative | 9/12 | 0.7500 | 5/12 | 0.4167 |
| setfit | band 2 (len > c2) | conspiratorial | 2/7 | 0.2857 | 1/7 | 0.1429 |
| kev | blocked (budget) | - | - | - | - | - |

### RANKING: macro-F1 on the shared eval split (laya_zero_shot is a baseline, not ranked)

| rank | arm | macro-F1 | false assignment >0.6 | tiebreak applied |
|---|---|---|---|---|
| 1 | laya | 0.7476 | 7/63 (0.1111) | no |
| 2 | setfit | 0.6254 | 13/63 (0.2063) | no |
| - | kev | blocked (budget) | - | - |
| baseline | laya_zero_shot | 0.3696 | - | - |

Findings: none

### PER-CLASS: precision / recall (support = true rows, predicted = argmax rows)

| arm | scope | class | support | predicted | precision | recall |
|---|---|---|---|---|---|---|
| laya | all | social_affiliative | 23 | 24 | 0.7083 | 0.7391 |
| laya | all | conspiratorial | 14 | 10 | 0.8000 | 0.5714 |
| laya | all | other | 63 | 66 | 0.8333 | 0.8730 |
| laya | band 0 (len <= c1) | social_affiliative | 4 | 4 | 0.7500 | 0.7500 |
| laya | band 0 (len <= c1) | conspiratorial | 2 | 3 | 0.6667 | 1.0000 |
| laya | band 0 (len <= c1) | other | 28 | 27 | 0.9630 | 0.9286 |
| laya | band 1 (c1 < len <= c2) | social_affiliative | 7 | 5 | 0.6000 | 0.4286 |
| laya | band 1 (c1 < len <= c2) | conspiratorial | 5 | 4 | 0.7500 | 0.6000 |
| laya | band 1 (c1 < len <= c2) | other | 21 | 24 | 0.7917 | 0.9048 |
| laya | band 2 (len > c2) | social_affiliative | 12 | 15 | 0.7333 | 0.9167 |
| laya | band 2 (len > c2) | conspiratorial | 7 | 3 | 1.0000 | 0.4286 |
| laya | band 2 (len > c2) | other | 14 | 15 | 0.6667 | 0.7143 |
| laya_zero_shot | all | social_affiliative | 23 | 17 | 0.1176 | 0.0870 |
| laya_zero_shot | all | conspiratorial | 14 | 31 | 0.2903 | 0.6429 |
| laya_zero_shot | all | other | 63 | 52 | 0.6731 | 0.5556 |
| laya_zero_shot | band 0 (len <= c1) | social_affiliative | 4 | 9 | 0.1111 | 0.2500 |
| laya_zero_shot | band 0 (len <= c1) | conspiratorial | 2 | 9 | 0.1111 | 0.5000 |
| laya_zero_shot | band 0 (len <= c1) | other | 28 | 16 | 0.8750 | 0.5000 |
| laya_zero_shot | band 1 (c1 < len <= c2) | social_affiliative | 7 | 2 | 0.0000 | 0.0000 |
| laya_zero_shot | band 1 (c1 < len <= c2) | conspiratorial | 5 | 11 | 0.2727 | 0.6000 |
| laya_zero_shot | band 1 (c1 < len <= c2) | other | 21 | 20 | 0.7500 | 0.7143 |
| laya_zero_shot | band 2 (len > c2) | social_affiliative | 12 | 6 | 0.1667 | 0.0833 |
| laya_zero_shot | band 2 (len > c2) | conspiratorial | 7 | 11 | 0.4545 | 0.7143 |
| laya_zero_shot | band 2 (len > c2) | other | 14 | 16 | 0.3750 | 0.4286 |
| setfit | all | social_affiliative | 23 | 29 | 0.5862 | 0.7391 |
| setfit | all | conspiratorial | 14 | 26 | 0.3846 | 0.7143 |
| setfit | all | other | 63 | 45 | 0.8667 | 0.6190 |
| setfit | band 0 (len <= c1) | social_affiliative | 4 | 3 | 0.6667 | 0.5000 |
| setfit | band 0 (len <= c1) | conspiratorial | 2 | 7 | 0.2857 | 1.0000 |
| setfit | band 0 (len <= c1) | other | 28 | 24 | 0.9583 | 0.8214 |
| setfit | band 1 (c1 < len <= c2) | social_affiliative | 7 | 7 | 0.5714 | 0.5714 |
| setfit | band 1 (c1 < len <= c2) | conspiratorial | 5 | 12 | 0.2500 | 0.6000 |
| setfit | band 1 (c1 < len <= c2) | other | 21 | 14 | 0.7857 | 0.5238 |
| setfit | band 2 (len > c2) | social_affiliative | 12 | 19 | 0.5789 | 0.9167 |
| setfit | band 2 (len > c2) | conspiratorial | 7 | 7 | 0.7143 | 0.7143 |
| setfit | band 2 (len > c2) | other | 14 | 7 | 0.7143 | 0.3571 |
| kev | blocked (budget) | - | - | - | - | - |

### ECE: house top-label ECE (15 bins) with the class-stratified bootstrap 95 % interval

| arm | scope | rows | ECE | 95 % interval |
|---|---|---|---|---|
| laya | all | 100 | 0.0936 | [0.0607, 0.1732] |
| laya | band 0 (len <= c1) | 34 | 0.1245 | [0.0566, 0.2051] |
| laya | band 1 (c1 < len <= c2) | 33 | 0.1687 | [0.0820, 0.2964] |
| laya | band 2 (len > c2) | 33 | 0.1538 | [0.0876, 0.2933] |
| laya_zero_shot | all | 100 | 0.1393 | [0.0939, 0.2382] |
| laya_zero_shot | band 0 (len <= c1) | 34 | 0.2032 | [0.1448, 0.3810] |
| laya_zero_shot | band 1 (c1 < len <= c2) | 33 | 0.2495 | [0.1804, 0.3742] |
| laya_zero_shot | band 2 (len > c2) | 33 | 0.1905 | [0.1452, 0.3692] |
| setfit | all | 100 | 0.1111 | [0.0733, 0.2069] |
| setfit | band 0 (len <= c1) | 34 | 0.1195 | [0.0923, 0.2458] |
| setfit | band 1 (c1 < len <= c2) | 33 | 0.2521 | [0.1684, 0.4306] |
| setfit | band 2 (len > c2) | 33 | 0.1270 | [0.0948, 0.2901] |
| kev | blocked (budget) | - | - | - |
<!-- generated:bakeoff-tables end -->

The numbers above are the uncorrected `softmax(z / T)` probabilities the server returns (section 8). Laya has the lower false-assignment count at 0.6 than SetFit and the higher recall at 0.6 for both functions; at 0.85 SetFit's false-assignment count is zero but its recall of both functions is lower than Laya's. The zero-shot Laya base is a baseline, not a candidate: its recall at 0.85 is near zero. The false-assignment rate is a share of `other` posts, so it carries into the field only if the field's `other` posts look like this file's `other` posts; the field has far more of them (section 8), which lowers the precision of an assignment even at an unchanged rate.

## 1. Status: a PILOT, and what the gate said

- **motivation-v1 is a pilot (D-01)** on Dr. Iris Yuster's classification guide, **25 October 2026 draft** (docx sha256 in the numbers table), which is not frozen. A retrain is expected after the freeze. The labelled file is small (its size is in the numbers table); the guide's own open issues (thread context, label stability, the assignment threshold) are untouched here.
- **Gate result: passed on the first declared run** (12-GATE-RUN-EVIDENCE.json, outcome `gate_pass`, `final_attempt` 1). Fine-tuned macro-F1, zero-shot macro-F1, the margin and the post-calibration ECE are in the numbers table against the gate's thresholds of a margin of at least 0.05 and an ECE of at most 0.10.
- **It is not a gate-failed pilot and no waiver was used.** The waiver `motivation-v1-pilot-ece` was declared before the run as a contingency (D-19) and was not needed: the shipped seed's ECE is under 0.10 (12-GATE-RUN-EVIDENCE.json `waived` is null, `pilot_waiver` is null).
- **How close that is, stated plainly.** The shipped seed is the median-ECE seed of three (seed 13). One of the three seeds (seed 17) is above the ECE bar on its own, and the bootstrap interval of the shipped ECE on the 100 eval rows runs from `lo` to `hi` in the numbers table, so its upper end is well above 0.10. The gate certifies the point value on this split; it is a small split, and the per-length-band ECE points in the ECE table are all above 0.10. The claim scope, quoted from `contracts/laya-finetune-gate-v1.yaml` `eval_set.claim`, is in-distribution calibration on a held-out 25 % of the same file, not robustness to a shifted population.
- Dr. Yuster was not asked to review the condensed task text for the pilot (D-10).

## 2. Identity (SC-3)

| Item | Value | Record |
|------|-------|--------|
| artifact | `laya-motivation-v1.apr` | 12-DEPLOY-EVIDENCE.json `artifact_path` |
| `artifact_sha256` (the pin) | `d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd` | gate run, deploy and replay records, all equal |
| labels, in order | `["social_affiliative", "conspiratorial", "other"]` | 12-GATE-RUN-EVIDENCE.json `inspect.labels` |
| recipe_id | `f88f5b71ef925fdabb30d26a15c2eb5195a151dadfe0882c858b73ca7bf54d7e` | same, `inspect.recipe_id` |
| base / method / variant | `laya-en-root@55cf4c4e` (the same Laya root as antisemitism-v1) / `laya` / `production` | the run's `recipe.json` `base` (gitignored) and `inspect.variant` |
| waived status | none (`waived` null) | 12-GATE-RUN-EVIDENCE.json |
| task_json_sha256 | `f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3` | 12-TASK-EVIDENCE.json; equals the artifact's `inputs_sha256.task_json` |

The verify line of the pack step reads `deploy_eligible: true`, `argmax 100/100` and the `artifact_sha256` above (12-GATE-RUN-EVIDENCE.json `verify_line`). The returned `model.artifact_sha256` is what experiment-coach pins; nothing else is promised.

## 3. Endpoint and auth posture

- Server: `aprender-mcp-decide-motivation` on pmcp.run, profile `ze-kasher-dev`, region `us-east-1` (D-23), the 10,240 MB tier, arm64. The antisemitism server `aprender-mcp-decide-antisemite` is unchanged (12-DEPLOY-EVIDENCE.json `antisemite_before` and `antisemite_after`).
- Endpoint: `https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp` (12-DEPLOY-EVIDENCE.json `live_attempts[0].endpoint`; plain HTTPS POST).
- Auth: **off**, as for the antisemitism server. No credential is needed to call it; the open endpoint's cost exposure (a cold call buys about 25 s of 10 GB compute) is the owner's accepted posture.
- Rule for later models (D-19 of Phase 10): each served decide model gets its own server id named for its dataset, `aprender-mcp-decide-<dataset>`.

## 4. Items for the partner to confirm

1. **Batch size 4 against the door.** The served door is 8 texts and 800 built tokens per call (12-DOOR-EVIDENCE.json; the contract door is unchanged, variant (a)). Built rows are P + t tokens with P measured at the value in the numbers table; an equal-length batch of 8 posts of the length the acceptance replay used (rows of just over 100 tokens each) is refused at the token budget, and a batch of 4 posts is admitted whole. experiment-coach sends batches of 4 and splits only on a `classify_max_total_tokens` token refusal, never on a count refusal (12-DOOR-EVIDENCE.json `experiment_coach_note`); that behaviour fits the deployed door. Posts in the labelled file are at most the `max` in the numbers table in built tokens (the post-token percentiles are there too), so a batch of 4 fits for the median and the 95th-percentile post; it need not fit four of the longest posts, which is what the token-refusal split is for.
2. **Label order.** Served order is `[social_affiliative, conspiratorial, other]`; look probabilities up by label name or follow `labels`.
3. **motivation_min_prob.** 0.6 today against the guide's example of 0.85. The measured false-assignment rates and recalls at both thresholds are in the lead tables; they are measured on 100 held-out rows from the same file and are not a calibration sweep on the partner's own sample. The threshold choice is the partner's; a hard false-assignment ceiling is deferred to the post-freeze retrain (section 11).
4. **Transport.** As for the antisemitism server: a plain HTTPS MCP POST with `content-type: application/json` and `accept: application/json, text/event-stream`, a bare `tools/call` of `classify` with `{"texts": [...]}`; no SigV4 and no Function URL (12-ACCEPTANCE-REPLAY.json `request_form`).

## 5. The task and the limits as deployed

Approved by the owner on 2026-10-26 (variant (a), 12-TASK-EVIDENCE.json `owner_reply`). The text is a condensation of Sections 4 to 6 of the guide (about 9.5 thousand guide tokens down to the instruction tokens in the numbers table). It is guide-derived instructions, not posts. A longer variant (b) was shown to the owner and not chosen (its P and door are in the numbers table; no door change follows).

Instructions, verbatim:

> Which function does this post serve? Social affiliative: signals belonging to a group (we, our people, calls to join, mocking critics). Conspiratorial: explains events by hidden control or secret orchestration by a group. Other: neither clear, weak or both. If unsure, choose other.

Criterion descriptions, verbatim, in label order:

- `social_affiliative`: group belonging or loyalty
- `conspiratorial`: hidden control by a group
- `other`: neither clear, weak or mixed

`task_json_sha256`: `f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3` (recomputed from `data/decide/motivation-v1/task.json`; equals 12-TASK-EVIDENCE.json and the artifact's `inputs_sha256.task_json`).

Measured with the served tokenizer (12-TASK-EVIDENCE.json `measured`): P is the prefix length in the numbers table; nothing is cut at the tokenizer (`head_cut` false; no option cut). No accuracy claim is attached to the wording: it is a hypothesis measured by the gate and the bake-off, nothing more (D-10, D-26).

Limits as deployed (the door and the mechanism, D-09): the door B is the served maximum in the numbers table, a contract constant that was not changed (`classify_max_texts`); B times P is under the 800-token budget in the numbers table. A count above B is refused before tokenization naming `classify_max_texts`; a total above 800 built tokens is refused naming `classify_max_total_tokens`. A post longer than the room after the prefix is right-truncated and the result carries `truncated: true`; no labelled post is (the truncated count in the numbers table). Retention note: the instruction head and all three options are kept whole.

## 6. Data and gate (D-13, D-17, D-18, D-21)

- **Data** (12-TASK-EVIDENCE.json `data`): the source file has the number of rows in the numbers table, all distinct texts; no duplicate and no mixed-label group was found. A seed-13, class-stratified, text-disjoint 25 % goes to `eval.jsonl` (train and eval row counts in the numbers table; the per-class counts are the pi_train counts of section 8 and the eval counts of the lead). CSV sha256 in the numbers table. The posts are from the partner's motivation file; none appears in any record or here.
- **Calibration slice and spike (D-17, D-18).** Spike 032 (12-SPIKE-EVIDENCE.json) tested whether the temperature clamp of 5 binds on this small data and whether a larger calibration slice is warranted; outcome `clamp_not_binding`, the slice stays at the contract's 0.25 (12-SLICE-OVERRIDE-EVIDENCE.json `outcome` `not_needed`; the slice fraction is in the numbers table). The spike's own note: at 0.25 its holdout ECE at the clamped T was above 0.10 on about 11 conspiratorial holdout rows, informational and noisy; it was not a rule input.
- **Gate run** (12-GATE-RUN-EVIDENCE.json): one declared run, three seeds, device `mps:0` (the gate record's `device_used`), torch 2.14.0, epochs declared at 12 with early stopping on calibration NLL (best epoch 2 and 5 epochs run on each seed), no class reweighting (D-21, `class_reweighting` "none (D-21)"). The shipped seed is the median-ECE seed. Per seed:

| seed | macro-F1 | ECE after T | margin | passes |
|------|----------|-------------|--------|--------|
| 13 (shipped) | 0.7476 | 0.0936 | 0.3780 | yes |
| 17 | 0.7197 | 0.1197 | 0.3502 | no |
| 23 | 0.7405 | 0.0914 | 0.3709 | yes |

  The seed-23 temperature sat at the clamp of 5 (`clamp_hit`); the shipped seed's did not. Seed 17 is above the ECE bar.
- **Claim scope.** In-distribution held-out 25 % of the same file; not shift robustness. This eval set was never used to choose the recipe (the calibration spike used a train-only holdout, and `eval_jsonl_opened` is false in its record).

## 7. The bake-off (D-05, D-06, D-14, D-25)

The comparison is one run of `scripts/laya_train/bakeoff.py` over the shared 100-row eval split (12-BAKEOFF-EVIDENCE.json, schema `motivation-bakeoff-v1`, `not_a_gate` true; a second run reproduced it byte for byte). The RANKING, PER-CLASS and ECE tables of section 0 carry the numbers; they are not repeated here. Rules, written before any number existed: the ranking is by macro-F1; a tie is broken by the lower false-assignment rate at 0.6; a reference arm that beats Laya's macro-F1 by at least 0.05 is written as a finding for a later phase and never acted on (D-06). Laya's macro-F1, ECE and zero-shot macro-F1 reproduce the gate report bit for bit.

- **Ranking.** Laya first, SetFit second (both macro-F1 values are in the numbers table). The findings list is empty: no reference arm clearly beats Laya. The zero-shot base is reported as a baseline and is not ranked.
- **Per-class precision and recall and ECE** are in the PER-CLASS and ECE tables. A band of fewer than 30 rows would show `not_reported` instead of an interval; every band here has 33 or 34 rows, so each carries an interval. A percentile interval of a biased statistic need not contain its point and none is required to.
- **Per-length-tertile tables** (D-14): the cut points and the band counts are in the numbers table (length is `len(text)` in code points of the eval texts; band 0 up to the first cut, band 1 up to the second, band 2 above). The false-assignment, recall, per-class and ECE results per band are in the tables of section 0. The length-band results are the check for the length shortcut that the partner asked for; with 2, 5 and 7 conspiratorial rows per band they cannot rule it in or out. Laya's false-assignment rate is highest in the longest band, on 14 rows.
- **SetFit** (D-25 deviation): cell `s32e1b16`, seed 13, 32 shots per class selected into a 96-row subsample of the 295 train rows, because s64 is infeasible (after the validation carve the conspiratorial and social_affiliative pools are below 64). It ran on the CPU. Its accuracy on the dump is in the numbers table; macro-F1, false assignment and the length bands come from `bakeoff.py`, aligned to the eval rows by row identity, not by label. Records: 12-SETFIT-CELL-EVIDENCE.json.
- **Kev-4B: blocked (failure class `budget`), by the owner's decision, not by a runtime failure.** Kev-4B (the Qwen3.5 decision classifier) was run through the Rust path built on branch `spike/016-upstream-sync`, worktree commit in the numbers table, with the guide's Sections 4.1 to 4.4, 5 and 6 as the prompt and **Section 4.5 omitted (D-24)**, because with it the guide is above Kev's 8,192-token row limit. That prompt is about 6.6 times Kev's 1,024-token training branch, outside its training distribution. The Rust path itself matched a torch fp32 oracle (spike 033, parity within 1e-4, on the short and the 6.8k-token rows). The cost is what stopped it: each post re-processes the shared guide prefix, so a post costs the per-post seconds in the numbers table on 14 CPU threads, against about one second warm for Laya. The owner started the full run (100 eval rows and all 295 train rows) and stopped it after the rows-scored counts in the numbers table (all 100 eval rows and 34 train rows), judging that cost impractical without a prefix cache and a GPU (12-KEV-OWNER-REPLY-2.json, verbatim). No fit ran, so no calibrated Kev probability exists, `eval-probs.json` was never written and the arm enters the bake-off as `blocked:budget`.
  - **Raw partial result, uncalibrated.** On the 100 eval rows, scored zero-shot with the head's own temperature and nothing fitted, Kev's one-vs-rest AUC is in the numbers table for each class (conspiratorial highest, social_affiliative and other lower), and its argmax accuracy is in the table too. The argmax accuracy is low because the uncalibrated probabilities assign nearly every post to `conspiratorial` (the predicted-class count is in the table), so the ranking signal (AUC) and the decision rule (argmax) disagree by construction. There are only 14 conspiratorial eval rows (and 23 social_affiliative), so every per-class figure is noisy and carries a wide interval that was not computed; the same holds for Laya's and SetFit's per-class tables.
  - **What this does and does not support.** Kev is not ranked. No calibrated Kev number exists, so no claim of parity or of superiority over any arm is made, in either direction, and the raw figures above are not a ranked comparison with Laya. Laya stays the served arm whatever the ranking (D-06).
  - **Reopen route.** A prefix cache of the shared guide prefix (roughly 60 times less compute per post: about 100 new tokens instead of about 6,900) plus batching, possibly on a GPU, as a separate spike that is not started. The resumable partial dump (134 scored rows, identity-bound) is kept locally, and a resume needs a new owner decision. Records: 12-KEV-ARM-EVIDENCE.json, 12-KEV-OWNER-REPLY-2.json, spike 033's README.
- **Findings.** None that changes what is served. A reference arm clearly better than Laya is a finding for a later phase; none exists on these records.

## 8. The prior (D-03, D-22)

**No server-side prior correction was made, and no server change was made for this.** The server returns `softmax(z / T)` with T the fitted temperature (the gate's `t_applied`); the class prior it reflects is the train split's: the pi_train counts of 295 train rows in the numbers table (social_affiliative 66, conspiratorial 41, other 188). Weights were not changed by class (D-21).

The guide's field prior, `pi_field`, is read from the guide's Table 7 (Section 10.2, development corpus of 1,873 posts labelled antisemitic): of the 1,546 posts classified for motivational function, 710 show no evidence and 585 weak evidence only, which the guide treats as Other, and about 7 percent show clear social-affiliative and about 9 percent clear conspiratorial evidence. So `pi_field` is about 0.07 for social_affiliative, about 0.09 for conspiratorial and about 0.84 for other ((710 + 585) / 1,546). The caveats: weak and mixed evidence count as other, so the categories are approximate; the 7 and 9 percent are the guide's rounded figures; and the corpus is the guide's development set of antisemitic-labelled posts, not experiment-coach's live traffic.

Client-side correction, if the partner wants one: p'_k proportional to p_k x pi_field,k / pi_train,k, renormalised over the three labels, with pi_train,k the train-split fraction (counts over 295). Example only, with made-up numbers: for a returned vector (0.60, 0.30, 0.10) the weights pi_field / pi_train are about (0.31, 0.65, 1.32) and the corrected vector is about (0.37, 0.38, 0.26); a returned (0.10, 0.05, 0.85) becomes about (0.03, 0.03, 0.95). The bake-off numbers of section 0 are uncorrected, and they are not recomputed under this correction here.

## 9. Shortcuts: length measured, source batch UNMEASURED

- **Length (D-14): measured**, in the tables of section 0 and the per-band statements of section 7, with the caveat that the bands are small.
- **Source batch: UNMEASURED (D-15, D-04).** The file came from batches (the two functions from one source, `other` from three others) and the column that recorded this was removed; the owner holds the mapping. Nothing was computed by source batch, and the model may be using source style as a signal. This is a known blind spot, to be closed with the mapping for the retrain (12-BAKEOFF-EVIDENCE.json `source_batch`).

## 10. Acceptance replay and timings (reported, not gated)

Record: 12-ACCEPTANCE-REPLAY.json, run 2026-10-26 against the endpoint above with no credential, using only the committed synthetic sentences (no partner text). The identity probe: all four checks true, returned `artifact_sha256` equal to the pin.

| Call | Texts | Result |
|------|-------|--------|
| single-text replays | 1 | every check true, from three starting offsets |
| largest admitted call | the largest multi-text count in the numbers table | every check true after the step-down |
| eight texts | 8 | refused at the token budget (`classify_max_total_tokens`), rows total above 800; the replay stepped down |
| batch of 4 (experiment-coach's size) | 4 | admitted whole, every check true |
| over the count door | 9 | `isError` naming `classify_max_texts`, the limit and the contract |

The checks per replay: one result per text, probabilities as long as the labels and in the unit range, summing to 1 within 1e-5, `label` equal to the argmax, labels in task order, returned sha equal to the pin. Requester tests that need the partner's own labelled sample (the calibration sweep on their data, the by-source result, accuracy on unseen posts) are not run here.

Timings (12-DEPLOY-EVIDENCE.json `live`, from `laya-deploy-verify`): cold, 4 proven-cold samples of the two maximal request shapes took between the min and the max in the numbers table, so the worst slack to the 30 s gateway cap is the figure there, beside Phase 10's worst slack of 4.5 s (antisemitism-v1); warm, 5 calls, p50 and range in the numbers table. Nothing keeps the function warm, so the first call after idle pays the load again.

## 11. Scope and deferred

- **Served output (D-12): the existing `classify` contract only.** Calibrated probabilities per label, no class assigned; the guide's Section 9 audit output is deferred. No thread context is sent or used (the guide classifies with the parent and preceding posts as context; the development data has none).
- **Accuracy on unseen posts is pending** the partner's sample (the guide requires it); the numbers here are on 100 held-out rows of the same file.
- **Serving SetFit or Kev-4B** is a later phase if at all; neither is served, and Kev has no calibrated number.
- **A hard false-assignment ceiling** for the post-freeze retrain is deferred; this phase declares no ceiling and the rate is reported, not gated.
- **Out-of-sample caveat.** The motivation posts are GoldStandard 2024 posts, which is antisemitism-v1's training corpus, so stage-1 plus stage-2 numbers measured on them are not out-of-sample.
- The owner raised a binary conspiratorial-or-not reframing as an option for a later retrain; it was not pursued in this phase.
- No builder or inference code change was made for this pilot; the Lambda-direct recipe remains dropped.

## 12. Dates and conditions

- Task approved 2026-10-26; gate run, deploy, live verification, acceptance replay and bake-off all 2026-10-26 (12-TASK-EVIDENCE.json, 12-GATE-RUN-EVIDENCE.json, 12-DEPLOY-EVIDENCE.json, 12-ACCEPTANCE-REPLAY.json, 12-BAKEOFF-EVIDENCE.json).
- Re-run: none for the gate (one declared run). The deploy needed two interrupted uploads before a third succeeded (12-DEPLOY-EVIDENCE.json `findings`, F-12-12-1, worked around by environment only) and one edge `/health` 404 retry (F-12-12-2); neither cost a gate re-run.
- Condition: the guide is a 25 October 2026 draft; this is a pilot and the retrain follows its freeze.

## Numbers and their records

<!-- numbers begin -->
| quantity | value | record | JSON pointer |
|---|---|---|---|
| gate: fine-tuned macro-F1 (shipped seed) | 0.7476 | 12-GATE-RUN-EVIDENCE.json | /gate/ft_macro_f1 |
| gate: zero-shot macro-F1 (same rows) | 0.3696 | 12-GATE-RUN-EVIDENCE.json | /gate/zero_shot_macro_f1 |
| gate: margin over zero-shot | 0.3780 | 12-GATE-RUN-EVIDENCE.json | /gate/margin |
| gate: post-calibration top-label ECE | 0.0936 | 12-GATE-RUN-EVIDENCE.json | /gate/ece_post |
| gate threshold: minimum macro-F1 margin | 0.0500 | 12-GATE-RUN-EVIDENCE.json | /gate/thresholds/min_macro_f1_margin |
| gate threshold: maximum ECE | 0.1000 | 12-GATE-RUN-EVIDENCE.json | /gate/thresholds/max_ece |
| gate: applied temperature T | 4.6479 | 12-GATE-RUN-EVIDENCE.json | /gate/t_applied |
| gate: final attempt (declared runs) | 1 | 12-GATE-RUN-EVIDENCE.json | /final_attempt |
| gate: shipped seed (median ECE) | 13 | 12-GATE-RUN-EVIDENCE.json | /seeds/shipped |
| gate: epochs declared | 12 | 12-GATE-RUN-EVIDENCE.json | /epochs |
| gate: calibration slice fraction | 0.2500 | 12-GATE-RUN-EVIDENCE.json | /slice_fraction |
| gate: seed 13 epochs run | 5 | 12-GATE-RUN-EVIDENCE.json | /seeds/per_seed/0/epochs_run |
| gate: seed 13 best epoch | 2 | 12-GATE-RUN-EVIDENCE.json | /seeds/per_seed/0/best_epoch |
| gate: seed 17 ECE after T (above the bar) | 0.1197 | 12-GATE-RUN-EVIDENCE.json | /seeds/per_seed/1/ece_post |
| gate: recipe_id | f88f5b71ef925fdabb30d26a15c2eb5195a151dadfe0882c858b73ca7bf54d7e | 12-GATE-RUN-EVIDENCE.json | /inspect/recipe_id |
| bake-off: Laya ECE 95 % interval, lo | 0.0607 | 12-BAKEOFF-EVIDENCE.json | /arms/laya/ece/lo |
| bake-off: Laya ECE 95 % interval, hi | 0.1732 | 12-BAKEOFF-EVIDENCE.json | /arms/laya/ece/hi |
| bake-off: Laya macro-F1 (equals the gate) | 0.7476 | 12-BAKEOFF-EVIDENCE.json | /arms/laya/macro_f1 |
| bake-off: SetFit macro-F1 | 0.6254 | 12-BAKEOFF-EVIDENCE.json | /arms/setfit/macro_f1 |
| bake-off: Laya zero-shot macro-F1 (baseline) | 0.3696 | 12-BAKEOFF-EVIDENCE.json | /arms/laya_zero_shot/macro_f1 |
| Laya false-assignment rate >0.6 | 0.1111 | 12-BAKEOFF-EVIDENCE.json | /arms/laya/false_assignment/0.6/rate |
| Laya false-assignment count >0.6 (of 63 other rows) | 7 | 12-BAKEOFF-EVIDENCE.json | /arms/laya/false_assignment/0.6/numerator |
| Laya false-assignment client_ge count >=0.6 | 7 | 12-BAKEOFF-EVIDENCE.json | /arms/laya/false_assignment/0.6/client_ge/numerator |
| Laya false-assignment rate >0.85 | 0.0635 | 12-BAKEOFF-EVIDENCE.json | /arms/laya/false_assignment/0.85/rate |
| Laya false-assignment count >0.85 | 4 | 12-BAKEOFF-EVIDENCE.json | /arms/laya/false_assignment/0.85/numerator |
| Laya false-assignment client_ge count >=0.85 | 4 | 12-BAKEOFF-EVIDENCE.json | /arms/laya/false_assignment/0.85/client_ge/numerator |
| SetFit false-assignment rate >0.6 | 0.2063 | 12-BAKEOFF-EVIDENCE.json | /arms/setfit/false_assignment/0.6/rate |
| SetFit false-assignment client_ge count >=0.6 | 13 | 12-BAKEOFF-EVIDENCE.json | /arms/setfit/false_assignment/0.6/client_ge/numerator |
| SetFit false-assignment rate >0.85 | 0.0000 | 12-BAKEOFF-EVIDENCE.json | /arms/setfit/false_assignment/0.85/rate |
| SetFit false-assignment client_ge count >=0.85 | 0 | 12-BAKEOFF-EVIDENCE.json | /arms/setfit/false_assignment/0.85/client_ge/numerator |
| eval rows | 100 | 12-BAKEOFF-EVIDENCE.json | /eval/n |
| eval rows: social_affiliative | 23 | 12-BAKEOFF-EVIDENCE.json | /eval/per_class_counts/social_affiliative |
| eval rows: conspiratorial | 14 | 12-BAKEOFF-EVIDENCE.json | /eval/per_class_counts/conspiratorial |
| eval rows: other | 63 | 12-BAKEOFF-EVIDENCE.json | /eval/per_class_counts/other |
| length cut c1 (code points) | 126.0000 | 12-BAKEOFF-EVIDENCE.json | /eval/length/cuts/0 |
| length cut c2 (code points) | 244.0000 | 12-BAKEOFF-EVIDENCE.json | /eval/length/cuts/1 |
| length band 0 rows | 34 | 12-BAKEOFF-EVIDENCE.json | /eval/length/band_counts/0 |
| length band 1 rows | 33 | 12-BAKEOFF-EVIDENCE.json | /eval/length/band_counts/1 |
| length band 2 rows | 33 | 12-BAKEOFF-EVIDENCE.json | /eval/length/band_counts/2 |
| pi_train count: social_affiliative | 66 | 12-BAKEOFF-EVIDENCE.json | /prior/pi_train/counts/social_affiliative |
| pi_train count: conspiratorial | 41 | 12-BAKEOFF-EVIDENCE.json | /prior/pi_train/counts/conspiratorial |
| pi_train count: other | 188 | 12-BAKEOFF-EVIDENCE.json | /prior/pi_train/counts/other |
| pi_train: train rows | 295 | 12-BAKEOFF-EVIDENCE.json | /prior/pi_train/n |
| task: prefix tokens P (measured) | 97 | 12-TASK-EVIDENCE.json | /measured/P |
| task: instruction tokens | 61 | 12-TASK-EVIDENCE.json | /measured/instructions_tokens |
| task: task_json_sha256 | f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3 | 12-TASK-EVIDENCE.json | /task_json_sha256 |
| task: guide docx sha256 (25 October 2026 draft) | 0e1d5a35ac3ccd455c5556fecbc5c1d1edec0e0a1100e2feac8bd16a42f29167 | 12-TASK-EVIDENCE.json | /guide_source/docx_sha256 |
| task: variant (b) not chosen, P | 190 | 12-TASK-EVIDENCE.json | /variant_b_not_chosen/P |
| task: variant (b) not chosen, door | 4 | 12-TASK-EVIDENCE.json | /variant_b_not_chosen/door |
| task: posts truncated | 0 | 12-TASK-EVIDENCE.json | /posts_truncated/count |
| task: post tokens p50 | 45 | 12-TASK-EVIDENCE.json | /post_tokens/p50 |
| task: post tokens p95 | 77 | 12-TASK-EVIDENCE.json | /post_tokens/p95 |
| task: post tokens max | 147 | 12-TASK-EVIDENCE.json | /post_tokens/max |
| data: source rows | 395 | 12-TASK-EVIDENCE.json | /data/counts/rows_in |
| data: train rows | 295 | 12-TASK-EVIDENCE.json | /data/files/train.jsonl/lines |
| data: eval rows | 100 | 12-TASK-EVIDENCE.json | /data/files/eval.jsonl/lines |
| data: split seed | 13 | 12-TASK-EVIDENCE.json | /data/split_seed |
| data: CSV sha256 | bc61c76bd29bcdd4c6466266b4e1161841f2107a4f02f5ff4fd96f1b7a3a4ae7 | 12-TASK-EVIDENCE.json | /data/csv_sha256 |
| door: served_max_texts (B) | 8 | 12-DOOR-EVIDENCE.json | /served_max_texts |
| door: B x P | 776 | 12-DOOR-EVIDENCE.json | /b_times_p |
| door: built-token budget per call | 800 | 12-DOOR-EVIDENCE.json | /budget_total_tokens |
| deploy: artifact_sha256 (the pin) | d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd | 12-DEPLOY-EVIDENCE.json | /artifact_sha256 |
| deploy: tier (MB) | 10240 | 12-DEPLOY-EVIDENCE.json | /tier_mb |
| live: cold samples measured | 4 | 12-DEPLOY-EVIDENCE.json | /live/cold_wall_ms/n_measured |
| live: cold wall min (ms) | 22620 | 12-DEPLOY-EVIDENCE.json | /live/cold_wall_ms/min |
| live: cold wall max (ms) | 25353 | 12-DEPLOY-EVIDENCE.json | /live/cold_wall_ms/max |
| live: cold worst slack to the 30 s cap (ms) | 4647 | 12-DEPLOY-EVIDENCE.json | /live/cold_wall_ms/worst_slack_to_cap_ms |
| live: Phase 10 worst slack (ms) | 4500 | 12-DEPLOY-EVIDENCE.json | /live/cold_wall_ms/phase10_worst_slack_ms |
| live: warm calls | 5 | 12-DEPLOY-EVIDENCE.json | /live/warm_ms/n |
| live: warm p50 (ms) | 1253 | 12-DEPLOY-EVIDENCE.json | /live/warm_ms/p50 |
| live: warm min (ms) | 1226 | 12-DEPLOY-EVIDENCE.json | /live/warm_ms/min |
| live: warm max (ms) | 1314 | 12-DEPLOY-EVIDENCE.json | /live/warm_ms/max |
| replay: largest admitted multi-text call | 7 | 12-ACCEPTANCE-REPLAY.json | /largest_multi_text_n |
| replay: batch-of-4 texts admitted | 4 | 12-ACCEPTANCE-REPLAY.json | /batch_of_4/line/n_texts |
| replay: eight-text rows total (built tokens, refused) | 850 | 12-ACCEPTANCE-REPLAY.json | /timings_reported/tokens_total/n8_refused_rows_total |
| replay: over-door count tried | 9 | 12-ACCEPTANCE-REPLAY.json | /expected_count_refusal/n_texts |
| SetFit cell: shots per class | 32 | 12-SETFIT-CELL-EVIDENCE.json | /selection/shots |
| SetFit cell: selected rows | 96 | 12-SETFIT-CELL-EVIDENCE.json | /selection/selected_rows |
| SetFit cell: accuracy on the dump | 0.6600 | 12-SETFIT-CELL-EVIDENCE.json | /accuracy_from_dump |
| SetFit cell: train seconds | 748 | 12-SETFIT-CELL-EVIDENCE.json | /train_seconds |
| Kev: branch | spike/016-upstream-sync | 12-KEV-ARM-EVIDENCE.json | /branch |
| Kev: worktree commit | 32103ba83ce4165598ae31ddde89b5c9856b43f3 | 12-KEV-ARM-EVIDENCE.json | /commit |
| Kev: rows scored, eval | 100 | 12-KEV-ARM-EVIDENCE.json | /run/rows_scored/eval |
| Kev: rows scored, train | 34 | 12-KEV-ARM-EVIDENCE.json | /run/rows_scored/train |
| Kev: rows planned, train | 295 | 12-KEV-ARM-EVIDENCE.json | /run/rows_planned/train |
| Kev: median seconds per post (warm) | 142.4041 | 12-KEV-ARM-EVIDENCE.json | /run/per_row_warm_s |
| Kev: scoring hours before the stop | 5.3513 | 12-KEV-ARM-EVIDENCE.json | /run/total_hours |
| Kev: prompt tokens (Sections 4.1-4.4, 5, 6) | 6812 | 12-KEV-ARM-EVIDENCE.json | /prompt/qwen_tokens |
| Kev: failure_class | budget | 12-KEV-ARM-EVIDENCE.json | /failure_class |
| Kev partial: one-vs-rest AUC, conspiratorial | 0.9045 | 12-KEV-ARM-EVIDENCE.json | /partial_eval/auc_one_vs_rest/conspiratorial |
| Kev partial: one-vs-rest AUC, social_affiliative | 0.6957 | 12-KEV-ARM-EVIDENCE.json | /partial_eval/auc_one_vs_rest/social_affiliative |
| Kev partial: one-vs-rest AUC, other | 0.6727 | 12-KEV-ARM-EVIDENCE.json | /partial_eval/auc_one_vs_rest/other |
| Kev partial: argmax accuracy (uncalibrated) | 0.1700 | 12-KEV-ARM-EVIDENCE.json | /partial_eval/argmax_accuracy |
| Kev partial: rows predicted conspiratorial | 95 | 12-KEV-ARM-EVIDENCE.json | /partial_eval/predicted_class_counts/conspiratorial |
| Kev partial: rows predicted social_affiliative | 5 | 12-KEV-ARM-EVIDENCE.json | /partial_eval/predicted_class_counts/social_affiliative |
| Kev partial: rows predicted other | 0 | 12-KEV-ARM-EVIDENCE.json | /partial_eval/predicted_class_counts/other |
| Kev partial: eval rows conspiratorial | 14 | 12-KEV-ARM-EVIDENCE.json | /partial_eval/gold_class_counts/conspiratorial |
| Kev partial: eval rows social_affiliative | 23 | 12-KEV-ARM-EVIDENCE.json | /partial_eval/gold_class_counts/social_affiliative |
| pi_field: posts classified for motivational function | 1,546 | guide docx §10.2 Table 7 (sha256 0e1d5a35ac3ccd455c5556fecbc5c1d1edec0e0a1100e2feac8bd16a42f29167) | - |
| pi_field: Other, no evidence | 710 | guide docx §10.2 Table 7 (sha256 0e1d5a35ac3ccd455c5556fecbc5c1d1edec0e0a1100e2feac8bd16a42f29167) | - |
| pi_field: Other, weak evidence only | 585 | guide docx §10.2 Table 7 (sha256 0e1d5a35ac3ccd455c5556fecbc5c1d1edec0e0a1100e2feac8bd16a42f29167) | - |
| pi_field: clear social-affiliative (percent, about) | 7 | guide docx §10.2 Table 7 (sha256 0e1d5a35ac3ccd455c5556fecbc5c1d1edec0e0a1100e2feac8bd16a42f29167) | - |
| pi_field: clear conspiratorial (percent, about) | 9 | guide docx §10.2 Table 7 (sha256 0e1d5a35ac3ccd455c5556fecbc5c1d1edec0e0a1100e2feac8bd16a42f29167) | - |
| pi_field: posts labelled antisemitic in the table | 1,873 | guide docx §10.2 Table 7 (sha256 0e1d5a35ac3ccd455c5556fecbc5c1d1edec0e0a1100e2feac8bd16a42f29167) | - |
<!-- numbers end -->
