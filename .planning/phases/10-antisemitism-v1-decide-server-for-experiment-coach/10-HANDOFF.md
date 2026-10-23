# Phase 10 hand-off: antisemitism-v1 decide server

**For:** the owner, and any client that wants to call the classifier. **Written:** 2026-10-23 (plan 10-09).
**Delivery:** this file lives in the aprender phase directory; nothing was written into the partner repo.

Every number below names the record it comes from. Clients pin by endpoint and sha256 and nothing else is promised.

## 1. Identity (D-17, SC-3)

| Item | Value | Record |
|------|-------|--------|
| artifact | `laya-antisemitism-v1.apr` | 10-GATE-RUN-EVIDENCE.json `artifact.path` |
| `artifact_sha256` | `2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e` | gate run, rename deploy and replay records, all equal |
| labels, in task order | `["not_antisemitic", "antisemitic"]` | 10-GATE-RUN-EVIDENCE.json `inspect.labels` |
| recipe_id | `d70c3f3da0daae129f3ee1ce928eb21a0ef71a83a6f09dc18e0621f3cc8adcbc` | same, `inspect.recipe_id` |
| base | `laya-en-root@55cf4c4e` | same, `inspect.base` |
| method / variant | `laya` / `production` | same |

The verify line of the pack step reads `deploy_eligible: true`, `argmax 2129/2129`, `artifact_sha256` as above (10-GATE-RUN-EVIDENCE.json `verify_line`).
Clients pin the returned `model.artifact_sha256` and the endpoint (the request document names its pin variable `EC_CLASSIFIER_SHA`); nothing else is promised.

## 2. Endpoint and auth posture (D-12, D-13, D-19, D-20)

- Server: `aprender-mcp-decide-antisemite` on pmcp.run, 10,240 MB tier, arm64.
- Endpoint: `https://aprender-mcp-decide-antisemite.us-east.true-mcp.com/mcp` (10-RENAME-DEPLOY-EVIDENCE.json; plain HTTPS POST).
- Auth: **off** (the owner's D-13 decision `replace-auth-off`, 2026-10-22). No credential is needed to call it. The open endpoint's cost exposure (a cold call buys about 25 s of 10 GB compute) was accepted by the owner. If auth is turned on later, the credential is delivered out of band and the client sends it as an `Authorization: Bearer` header; the probes read it from `APRENDER_DECIDE_PROBE_TOKEN`.
- The stance demo (`laya-stance-64.apr`, sha256 beginning `24a44d7e`) is no longer served since 2026-10-22 (it was replaced under the previous server id by plan 10-08, D-13).
- The previous server `aprender-mcp-decide` and its endpoint `https://aprender-mcp-decide.us-east.true-mcp.com/mcp` were retired by the owner on 2026-10-23 (D-20; 10-RENAME-DEPLOY-EVIDENCE.json `retirement.state` is `deleted_confirmed`; the old endpoint answers 404 `Server not found`). Do not pin to them; the sha256 is unchanged.
- Rule for later models (D-19): each served decide model gets its own server id named for its dataset, `aprender-mcp-decide-<dataset>`.

## 3. Items for the partner to confirm (D-14, D-15)

This record states what the server is. It does not say who calls it, or how any given client is built.

1. **Transport, for the partner to confirm.** The endpoint is a plain HTTPS MCP POST with two headers, `content-type: application/json` and `accept: application/json, text/event-stream`, a bare `tools/call` of `classify` with `{"texts": [...]}`. There is no SigV4, no Function URL and no `lambda:InvokeFunctionUrl`. The request document describes a SigV4 Function URL transport and carries a decision-table note that "server-to-server MCP through pmcp.run is not a supported path from a tool". Whether and how a given client reconciles those with this endpoint, and who calls the classifier, is for the partner to confirm.
2. **Batch size, as deployed (D-15 as amended 2026-10-21).** In short: a client's batch size (the request document's `classify_batch`) is at most the served classify_max_texts, which is 8. The owner's note that batch size is not critical stands: single-text calls are acceptable at any door, and a door of 1 would simply mean single-text calls. The served door is 8 texts, 800 built tokens per call, 16 KiB per text and a 512-token cap per built row (10-TASK-EVIDENCE.json `chosen_max_texts` 8; 10-RENAME-DEPLOY-EVIDENCE.json `live.plan_only`).
3. **Texts per call.** For equal-length texts it is min(classify_max_texts, floor(800 / built_row_tokens)), with built_row_tokens = P + t, P = 86 (measured, 10-TASK-EVIDENCE.json) and t the post's tokens. From the measured tweet percentiles (p50 56, p95 94; 10-TASK-EVIDENCE.json `tweet_tokens`): a median post is a 142-token row, so 5 per call; a p95 post is a 180-token row, so 4 per call; posts of about 14 tokens or fewer reach the door of 8. Mixed-length batches are admitted when the SUM of their built rows (P plus each text's tokens, each row capped at 512) is at most 800 and the count is at most 8; the server checks the sum, not the longest row. A client budgets by the returned `tokens` field.
4. **Label order.** The served order is `[not_antisemitic, antisemitic]`. The request document's unit fixture lists the labels reversed; that is harmless for a parser that looks probabilities up by label name, which is how the request document's parser is described, but the served order is as stated here.

## 4. The task (D-05, D-06) and the limits as deployed

Approved by the owner on 2026-10-21 (reply `door-8 p90`, variant `p90`). The text paraphrases the IHRA working definition's examples with the labelling rules of the cited paper; the 12-paragraph annotation guideline that paper cites separately (arXiv 1910.01214) was not fetched and is not reproduced.

Citation: Jikeli, Karali, Miehling, Soemer, "Antisemitic Messages? A Guide to High-Quality Annotation and a Labeled Dataset of Tweets", arXiv 2304.14599.

Instructions, verbatim:

> Is this tweet antisemitic by the IHRA definition? Antisemitic: hostile stereotypes of Jews, Jewish power or conspiracy myths, Holocaust denial, blaming Jews collectively. Not antisemitic: criticism of Israel, or reporting antisemitism.

Criterion descriptions, verbatim, in label order:

- `not_antisemitic`: no hostility toward Jews as Jews
- `antisemitic`: hostile to Jews or an antisemitic trope

`task_json_sha256`: `33fd8d7cd51b1b2864033ee313d30ae445e76120cc2b26e3849369c0602248e3` (recomputed from `data/decide/goldstandard-2024/task.json`; equals 10-TASK-EVIDENCE.json and the artifact's `inputs_sha256.task_json`).

Measured with the served tokenizer (10-TASK-EVIDENCE.json `measured`): P = 86 prefix tokens. The instruction and option retention: nothing cut at the tokenizer (`head_cut` false; the instruction head keeps all 54 of its raw tokens, and both options are kept whole at 12 and 14 tokens). No accuracy claim is attached to the wording: it is a hypothesis measured only by the gate (D-07).

Limits generated from the deployed boundary:

| Limit | Value | Source |
|-------|-------|--------|
| texts per call (B) | 8 | `chosen_max_texts`; `live.plan_only.max_texts` |
| built tokens per call | 800 | `live.plan_only.max_total_tokens` |
| bytes per text | 16 KiB (16384) | `live.plan_only.concentrated.max_text_bytes` |
| tokens per built row | 512 (cap) | decide-tool-boundary-v1 |
| room for a post after the prefix | 426 tokens (512 - 86) | `measured.state_room_tokens` |

A post longer than 512 - P = 426 tokens is right-truncated and its result carries `truncated: true`. In the labelled data no post is that long: `posts_truncated` is 0 of 11,311 (the longest is 366 tokens).
A count above 8 is refused before any tokenization with an error naming `classify_max_texts`; a total above 800 built tokens is refused naming the token budget.

## 5. Data and gate (D-03, D-08)

Data (10-TASK-EVIDENCE.json `data`): the source CSV has 11,311 rows; text groups are formed under `nfc-trim-ws-v1`, 667 duplicate rows collapse, the 5 mixed-label groups (13 rows) are dropped whole, and a seed-13 label-stratified, group-disjoint 20 % goes to `eval.jsonl`. Train 8,510 rows (7,016 not_antisemitic, 1,494 antisemitic); eval 2,129 rows (1,755 and 374). CSV sha256 `79f4d0ebc5f5ff529a25c0dfc1f7c8bbca48dbb6b8cff06b0059727da8db992f`.

Gate (10-GATE-RUN-EVIDENCE.json): **one declared run**, three seeds, `outcome` `gate_pass`, `mps:0` (read back from the trace), torch 2.14.0, epochs 4 declared from the probe, wall 16,822.3 s. Thresholds: margin over zero-shot at least 0.05, top-label ECE at most 0.10 (15 bins). The shipped seed is 23, the median-ECE seed.

| seed | macro-F1 | ECE after T | margin | T | pass |
|------|----------|-------------|--------|---|------|
| 13 | 0.7954 | 0.0117 | 0.2358 | 1.411 | yes |
| 17 | 0.8054 | 0.0456 | 0.2458 | 5.0 (clamp hit) | yes |
| 23 (shipped) | 0.7956 | 0.0424 | 0.2360 | 1.510 | yes |

Shipped seed against zero-shot on the same 2,129 eval rows: fine-tuned macro-F1 0.7956, zero-shot macro-F1 0.5596, margin 0.2360; ECE 0.0604 before and 0.0424 after calibration; NLL 0.2789. The Rust re-computation at verify time equals these.

Claim scope, quoted from `contracts/laya-finetune-gate-v1.yaml` `eval_set.claim`: "The gate now certifies margin and calibration on held-out data drawn like the tenant's shots. It does NOT certify robustness to a shifted input population." That is in-distribution calibration on this held-out split, not shift robustness.

Disclosures (10-04 and the probe): host load was not sampled during the gate run, so contention is unmeasured, not ruled out; two of three seeds restored epoch 1, so the shipped model is in effect a one-epoch fine-tune.

## 6. The comparison record (D-11, SC-4): blocked

Record: 10-SETFIT-CELL-EVIDENCE.json. Its label, verbatim:

> same eval split, subsampled train (64 shots/class of 7658 train rows); outside setfit-benchmark-claims-v1's 40-cell matrix; thresholds would be judged under the TweetEval-measured production regime. NO CELL WAS RUN: no number exists.

- Status: **SC-4 is open and NOT met.** `sc4_status` is `refused-open`, which here means the comparison is incomplete and held open; the cause is a blocked implementation, not a refusal (`failure_class` implementation).
- Blocker: `apr eval` exceeds this host's memory on the cell's splits (61 rows used 9.4 GB, 241 rows about 40 GB, and an 852-row run was killed at about 90 GB on a 48 GB host).
- Owner decision of 2026-10-22 (option C): hold SC-4 open; the cell is not run, `apr eval` is not changed, no cloud.
- No table, no number and no comparison exist in this record. Nothing here may be read as a result about that method, in either direction. The side-by-side of plan 10-06 is deferred by the owner.

## 7. Acceptance replay (D-16)

Record: 10-ACCEPTANCE-REPLAY.json, run 2026-10-23 against the endpoint above with no credential, using only the committed synthetic sentences (no tweet text). The identity probe: all four checks true, returned `artifact_sha256` equal to the pin `2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e`.

| Call | Texts | Result | Elapsed |
|------|-------|--------|---------|
| one-text replay, from 0 | 1 | every check true | 806 ms |
| one-text replay, from 1 | 1 | every check true | 803 ms |
| one-text replay, from 2 | 1 | every check true | 804 ms |
| largest admitted call | 8 (762 built tokens) | every check true, no step-down needed | 4187 ms |
| expected count refusal | 9 | `isError` naming `classify_max_texts`, the limit 8 and the contract | 329 ms |

The checks per replay: one result per text, probabilities as long as the labels, in the unit range, summing to 1 within 1e-5 (largest deviation 2.6e-8), `label` equal to the argmax, labels in task order, returned sha equal to the pin. The count refusal at 9 is the proof that the deployed door is 8 and not larger.

Requester tests (the request document's list): one result per text, labels in task order, probabilities aligned to `labels` summing to 1, returned `artifact_sha256` equal to the communicated sha: all pass. The calibration sweep on a labelled sample is the requester's own and is not run here.

## 8. Timings: reported, not gated (D-16)

- Cold, 4 proven-cold samples of the two maximal shapes (laya-deploy-verify, plan 10-12): concentrated 25,499 and 19,759 ms, distributed 22,963 and 22,788 ms; worst slack 4,501 ms to the 30 s gateway cap. Of a cold load of about 15.5 s, about 9.0 to 9.2 s is the S3 download, about 3.5 s the sha256 and about 2.8 s the build.
- Warm, 5 calls: 1,083 to 1,148 ms (p50 1,124 ms).
- This replay run: the first call reached a cold function (`cold;load_ms=13545`, first POST 14,463 ms); the replays after it were warm (table above). One incidental observation, not one of the four samples.
- The request document quotes cold 22.8 to 25.3 s and warm 1.1 s from an earlier measurement; the figures above are the live record's and replace them.
- Nothing keeps the function warm today, so the first call after idle pays the load again.

## 9. Accuracy readout (D-04, D-07)

- Now: the gate report's eval-split numbers for the shipped model, section 5 (macro-F1 0.7956 against zero-shot 0.5596, ECE 0.0424, n 2,129). The gate reports macro-F1 and ECE; an accuracy figure beside them was to come from plan 10-06, which the owner deferred on 2026-10-22 because it depends on the blocked cell of section 6.
- Comparison number: none. The cell is blocked and SC-4 is open (section 6).
- Live labelled set: pending. When the 400-post labelled sample that the request document describes exists, it is a `shift.jsonl` reported probe for a later run, never a gate clause (D-04). The slot is the data directory's `shift.jsonl`, scored after the gate is decided.
- Builder or inference work (the examples-versus-instructions balance, D-07) is decided after this readout, not here.

## 10. Dates and conditions (D-18)

- Proposed ready-by: 2026-10-29. Actual: the artifact was packed and verified `deploy_eligible` on 2026-10-22 and first served under the previous server id the same day; the server under its final id was verified live on 2026-10-23 and the acceptance replay above ran the same day. Ready on 2026-10-23, six days before the proposed date.
- Probe (10-PROBE-EVIDENCE.json): one timed epoch measured `s_per_step` 1.4695 s (threshold 3.0 s), so the declared epochs E = 4. Host-load verdict: `contended` false (84 samples, load average 2.33 to 4.63 on 14 cores, no competing runs). No trigger fired, so 10-PROBE-OWNER-REPLY.json is a skip. The gate run's own load was not sampled (section 5).
- Re-run: none. One declared gate run. The deployment was not clean first time: the first `laya-deploy` under the new server id failed its edge check and was contained, and the deployment was recovered by an owner-run redeploy (10-RENAME-DEPLOY-EVIDENCE.json `live_attempts`, findings F-10-12-1 to F-10-12-3); that cost no gate re-run.
- Condition 1, the owner's `task.json` review before the gate run: met 2026-10-21 (`owner_approved_on`). Condition 2, pmcp.run deploy access: confirmed 2026-10-22 by read-only checks (10-LIVE-DEPLOY-EVIDENCE.json `decision.access_confirmed`).

## 11. Scope (D-01)

- `motivation-v1` waits on this classifier's success. Its gate arithmetic is in section 6 of the Phase 8 reply (08-EXPERIMENT-COACH-DECIDE-REQUEST-REPLY.md).
- The Lambda-direct Function URL recipe was dropped (D-12).
- No door was re-derived for a new envelope: the owner's choice of B = 8 inside the Phase 8 envelope was made at plan 10-01 and plan 10-11 was a recorded no-op.
- No builder or inference change was made.
- Still open and not claimed: SC-4 (section 6). The old content-addressed weights object under the previous server's S3 prefix is still present; removing it is the owner's call (10-12-SUMMARY.md).
