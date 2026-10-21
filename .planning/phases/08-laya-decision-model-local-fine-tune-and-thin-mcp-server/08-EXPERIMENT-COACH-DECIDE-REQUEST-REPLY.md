# aprender's reply to the experiment-coach request for the `antisemitism-v1` decide server

**As of:** 2026-10-21 (UTC); aprender `844d02ff4` on `worktree-agent-abfa366e2bf8dd67c`; motivation-experiment `b7057de` on `main` (the request last changed by `0a4470e`, 2026-10-20). Request: `/Users/guy/projects/mcp/motivation-experiment/docs/requests/aprender.md`.

## How to read this

Answer first, then what exists, then evidence you can open. **Status**: VERIFIED, every claim checked against a source named under Evidence; PARTLY, the answer says which part was checked; UNKNOWN, not established from the two repos. **Exists today**: yes, usable now; partly, the mechanism exists but not the artifact or path; no, new work. aprender paths are repo-relative with `:line`, motivation-experiment paths absolute; (new) marks a path not yet existing. Nothing was deployed, trained or run on AWS and no GitHub write was made; every number comes from a file or read-only command recorded in `.planning/quick/261021-ia2-aprender-side-reply-to-the-experiment-co/261021-ia2-FACTS.md`.

## Answers at a glance

| Item | Answer in one line | Status |
|---|---|---|
| C1 | Conformant call exists today | VERIFIED |
| C2 | Fields match; served as `content[0].text` | VERIFIED |
| C3 | Criteria order rule exists; artifact does not | VERIFIED |
| C4 | Aligned, tempered softmax; sum by construction | PARTLY |
| C5 | Sha of the served `.apr`, pinned before parsing | VERIFIED |
| C6 | Guide is a per-row prefix, 192-token head cap | VERIFIED |
| C7 | pmcp.run, auth off; IAM Function URL is new work | VERIFIED |
| C8 | 10,240 MB deployed and contract-asserted | VERIFIED |
| C9 | Spike figures; edge cold calls 22647-27015 ms | VERIFIED |
| R1 | Batch 32 refused; 8 texts, 800 tokens | VERIFIED |
| R2 | Long guide cut; starves the token budget | VERIFIED |
| R3 | Lambda-direct IAM recipe proposed | VERIFIED |
| R4 | 667 duplicates, 5 conflicts; prepare script proposed | VERIFIED |
| R5 | Full-size SetFit uncalibrated; s64 cell proposed | VERIFIED |
| R6 | Guide text in neither repo | UNKNOWN |
| R7 | Cold premise about 2x off | VERIFIED |
| B1 | Data preparation: 1 day | VERIFIED |
| B2 | One-epoch probe: 1 day | VERIFIED |
| B3 | Gate run, pack, verify: 2 days | PARTLY |
| B4 | SetFit cell, parallel: 1 day | VERIFIED |
| B5 | IAM Function URL recipe: 1 day | VERIFIED |
| B6 | Acceptance replay: 1 day | VERIFIED |
| M1 | One workbook, 200 posts, six classes | VERIFIED |
| M2 | Smallest class fits 14 rows | VERIFIED |

## 1. Contract check

### C1 - request shape: `classify` takes `{ texts: [string] }` only

**Answer:** Yes. The one tool is `classify`, its arguments exactly `{"texts": [string, ...]}` with unknown fields refused; your bare `tools/call` without `initialize` is the documented call and your two headers are the README's (limits: R1).
**Exists today:** yes
**Status:** VERIFIED - no accept-header check found in the shim or pmcp 2.19.3; your client was not run against it.
**Evidence:**
- `/Users/guy/projects/mcp/motivation-experiment/crates/core/src/classifier.rs:110-113`, `:195-196`.
- `crates/aprender-mcp-decide/src/lib.rs:63`, `:84-91`, `:727-736`; `crates/aprender-mcp-decide-lambda/README.md:93`, `:101-102`; `crates/aprender-mcp-decide-lambda/src/main.rs:125-141`; facts T1-10, T1-11.

### C2 - response shape: `model{artifact_sha256, recipe_id, method, base}`, `labels`, `results[{label, probabilities, tokens, truncated}]`

**Answer:** Yes, field for field, `labels` in task order, `results` in input order. Transport: pmcp serializes the result into `content[0].text` (JSON) with no `structuredContent`, since the tool declares no output schema; your parser's fallback branch reads that form.
**Exists today:** yes
**Status:** VERIFIED
**Evidence:**
- `crates/aprender-mcp-decide/src/lib.rs:430-466`, `:795-811`; `crates/aprender-mcp-decide/src/tests.rs:896-915`; pmcp 2.19.3 (cargo registry) src/server/mod.rs lines 2537-2538 and 2554-2555.
- `/Users/guy/projects/mcp/motivation-experiment/crates/core/src/classifier.rs:115-135`, `:8-29`; `crates/aprender-mcp-decide-lambda/README.md:106-110`.

### C3 - `labels` in task order

**Answer:** Yes. Task order is the document order of `criteria` in task.json, the label index everywhere; row labels are names, never indices. For `antisemitism-v1` the criteria are `not_antisemitic` then `antisemitic`.
**Exists today:** partly - the rule exists; the `antisemitism-v1` task.json is new work (B1).
**Status:** VERIFIED
**Evidence:**
- `contracts/decide-apr-v1.yaml:107-109`, `:120-122`; `crates/aprender-mcp-decide/src/lib.rs:473-495`; `crates/aprender-mcp-decide/src/tests.rs:1046-1057`; `scripts/laya_train/data.py:5-14`.

### C4 - `probabilities` aligned to `labels`, calibrated, summing to 1

**Answer:** Aligned: one entry per label in `labels` order, `label` the argmax. Calibrated: `softmax(z / T)`, T fitted by NLL on a seeded, stratified, text-group-disjoint 25 % slice (at least 2 rows per class), clamped to [0.5, 5.0], bound bit for bit to the gate report; the gate passes only with post-calibration ECE at or under 0.10 and a macro-F1 margin of at least 0.05. Sum to 1 holds by the softmax construction in f32.
**Exists today:** yes
**Status:** PARTLY - alignment and calibration verified; sum-to-1 by construction, not asserted.
**Evidence:**
- `crates/aprender-decide/src/laya/mod.rs:9`, `:441-458`; `crates/aprender-decide/src/laya/temperature.rs:28-59`; `contracts/decide-apr-v1.yaml:227-230`.
- `contracts/laya-finetune-gate-v1.yaml:149-151`, `:156-159`, `:180`; `crates/aprender-mcp-decide/src/lib.rs:447-449`; `scripts/laya_train/data.py:24-30`.

### C5 - model identity and the sha communicated for pinning

**Answer:** `artifact_sha256` is the sha256 of the whole served `.apr`, verified against the deploy's pin before a byte is parsed; `recipe_id` is the sha256 of the `recipe.json` blob, `method` the constant `laya`, `base` the declared base. Your `EC_CLASSIFIER_SHA` plus `DriftChecked` match this. The sha is communicated in the deploy evidence JSON and the hand-off (B6); it cannot exist before the gate run (B3). `inputs_sha256` is a different object (the inputs' hashes).
**Exists today:** partly - the mechanism exists; the `antisemitism-v1` sha does not.
**Status:** VERIFIED
**Evidence:**
- `crates/aprender-mcp-decide/src/lib.rs:431-440`; `crates/aprender-mcp-decide/src/tests.rs:1070-1105`; `crates/aprender-mcp-decide-lambda/README.md:38-40`, `:114-118`; `contracts/decide-apr-v1.yaml:142`, `:255-258`.
- `/Users/guy/projects/mcp/motivation-experiment/crates/core/src/env.rs:36-42`; `/Users/guy/projects/mcp/motivation-experiment/crates/core/src/classifier.rs:218-243`; `/Users/guy/projects/mcp/motivation-experiment/infra/lib/experiment-coach-stack.ts:59-62`.

### C6 - task.json carrying the annotation guide

**Answer:** The mechanism exists: task.json is `{type: "choice", instructions, criteria}`, its bytes are hashed into the artifact identity, and its instructions plus rendered criteria become a row prefix every scored text is appended to, in training and serving alike. The guide costs tokens on every row and request; the builder caps the head at `head_max_len` 192 tokens and cuts the rest silently (R2).
**Exists today:** partly - a guide-sized prefix has never been built or served.
**Status:** VERIFIED
**Evidence:**
- `contracts/decide-apr-v1.yaml:93-101`, `:107-109`, `:255-258`; `crates/aprender-decide/src/artifact.rs:966-975`; `crates/aprender-decide/src/laya/builder.rs:4-18`, `:181-204`, `:216-219`; `scripts/laya_train/train.py:320`, `:747-748`; `contracts/decide-tool-boundary-v1.yaml:302-306`.

### C7 - a Lambda with a Function URL and IAM auth, account 816069129666, us-east-1

**Answer:** No, not today. The server runs on pmcp.run (cargo-pmcp, `type = "pmcp-run"`) behind its edge, auth off by the owner's decision. The pieces exist: the `bootstrap` binary on lambda_http 0.13.0, whose default features handle Function URL events; S3 weights under a stack-declared `s3:GetObject`; a Phase 9 recipe that creates a Function URL in this account, but with `--auth-type NONE` and public permissions. A Lambda-direct `AWS_IAM` Function URL deploy with no public statement is new work (B5). Your stack grants `lambda:InvokeFunctionUrl` (`AWS_IAM`) and `lambda:InvokeFunction` on `classifierFunctionArns` in the same account, so an identity-policy grant suffices; your fixture names `decide-classifier`, and since the ARN is a stack prop the name is yours to set. The spec allows "pmcp.run or a Lambda URL"; the request is stricter, and your SigV4 client fits only the Function URL.
**Exists today:** no
**Status:** VERIFIED
**Evidence:**
- `crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template:43`, `:115-126`; `crates/aprender-mcp-decide-lambda/README.md:76-88`; `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/deferred-items.md:313-320`, `:615-623`; `crates/aprender-mcp-decide-lambda/Cargo.toml:45`, `crates/aprender-mcp-decide-lambda/src/main.rs:31`, `:259`; lambda_http 0.13.0 README lines 16 and 249 (facts T1-40); `justfile:4509`, `:4524-4534`, `:1814`.
- `/Users/guy/projects/mcp/motivation-experiment/infra/lib/experiment-coach-stack.ts:262-283`; `/Users/guy/projects/mcp/motivation-experiment/infra/test/stack.test.ts:21`; `/Users/guy/projects/mcp/motivation-experiment/crates/core/src/classifier.rs:138-160`; `/Users/guy/projects/mcp/motivation-experiment/docs/superpowers/specs/2026-10-20-motivation-experiment-design.md:588-591`.

### C8 - the 10,240 MB tier

**Answer:** Yes. The deploy template sets `memory_mb = 10240`, the boundary contract's `lambda_memory_mb` is 10240, and a test asserts them equal (FALSIFY-DECIDE-TOOL-007) because every door constant is priced for that size; the tier was restored from 3,008 MB in contract v7.0.0 (plan 08-30, 2026-09-28). A Function URL deploy must carry the same number.
**Exists today:** yes
**Status:** VERIFIED
**Evidence:**
- `crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template:52-62`; `contracts/decide-tool-boundary-v1.yaml:32-37`, `:88-100`, `:317-321`, `:636-638`.

### C9 - the timings you will test: cold 11 to 12 s, 0.5 to 0.75 s per tweet warm, Lambda at 10 GB

**Answer:** Those are spike 026's single-text measurements on a bare default Lambda at 10,240 MB, no sha256 pin, no gateway (cold 11.3-12.0 s; warm 0.75 s Graviton2, 0.49 s Graviton3). Production at 10,240 MB through the pmcp.run edge measured, over 8 proven-cold maximal 800-token requests, 22647-27015 ms per cold call, the worst 2985 ms under the 30000 ms cap; ten warm calls took 866-980 ms (shape not recorded). The superseded 3,008 MB tier's numbers are listed in section 5. A Function URL deployment (no edge, no 30 s cap, IAM signing) is not measured. See R7.
**Exists today:** partly - measured on the pmcp.run path, not the requested one.
**Status:** VERIFIED - the warm calls' shape is the one unrecorded detail.
**Evidence:**
- `/Users/guy/projects/mcp/motivation-experiment/docs/requests/aprender.md:30`; `/Users/guy/projects/mcp/motivation-experiment/docs/superpowers/specs/2026-10-20-motivation-experiment-design.md:228-233`; `.claude/skills/spike-findings-aprender/references/aws-mcp-model-hosting.md:46`.
- `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-LIVE-REDEPLOY-EVIDENCE.json` `live.tier_mb`, `live.cold_wall_ms`, `live.cold_samples[*]`, `live.warm_ms.range`; `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-LIVE-DEPLOY-EVIDENCE.json` `memory_mb`, `cold_samples[*].elapsed_ms`, `warm.p50_ms`; `crates/aprender-mcp-decide-lambda/README.md:121-129`.

## 2. Mismatches and risks

### R1 - your batch of 32 against the 8-text, 800-token door

**Answer:** experiment-coach chunks calls by `classify_batch`, default 32. The door refuses more than `classify_max_texts` 8 texts and any call whose built rows exceed `classify_max_total_tokens` 800; a 32-text call is refused outright. The bounds are tier policy for the 30 s pmcp.run gateway cap (`floor((30000 - 15000 - 1100 - 4000) / 12) = 825`, declared down to 800; 8 x 57 = 456). A Function URL path has a different envelope; re-deriving the bounds is an owner decision this reply does not promise.
**Status:** VERIFIED
**Proposal:** Set `classify_batch` to at most 8 and budget tokens per call from the returned `tokens` field (a chunk's sum at or under 800); the request shape is unchanged.
**Evidence:**
- `/Users/guy/projects/mcp/motivation-experiment/crates/core/src/config.rs:32`, `:99`; `/Users/guy/projects/mcp/motivation-experiment/crates/core/src/sweep.rs:1109-1115`; `/Users/guy/projects/mcp/motivation-experiment/crates/mcp/src/tools.rs:352`.
- `contracts/decide-tool-boundary-v1.yaml:283-306`, `:32-37`, `:173-180`, `:101-110`; `/Users/guy/.claude/projects/-Users-guy-Development-machine-learning-aprender/memory/aprender-forecast-door-bounds-are-tier-policy.md`.

### R2 - the guide as a per-row prefix

**Answer:** A row is `[CLS] "{t} question: {instructions}" [SEP] [MASK] option0 [MASK] option1 [SEP] text [SEP]`. Each option keeps at most 48 tokens; the head keeps `max(8, 192 - options)` tokens of the instructions, so a longer guide is cut with no flag (`truncated` covers only the text, against `max_len` 512). The prefix is at most 195 tokens and is built into every training row too. Tokens per call = sum over texts of (P + t), P the empty-text row (prefix plus closing `[SEP]`) and t the text; 8 texts fit 800 only if `P + t <= 100`. The stance task's P is 57, so a text may add 43 tokens; a maximal head gives P = 196 and only `floor(800 / 196) = 4` empty texts fit. The exact P needs the tokenizer and is the first thing B1 measures; nothing was tokenized here.
**Status:** VERIFIED
**Proposal:** A condensed `instructions` plus two short criterion descriptions under a stated budget: pick the largest tweet length t you want in a full batch and hold `P <= 100 - t`. Record the full guide's sha256 in the run and approve the condensation yourselves: the task.json bytes are part of the artifact identity (`inputs_sha256.task_json`) and the spec's intent is "the served task is the guide".
**Evidence:**
- `crates/aprender-decide/src/laya/builder.rs:4-18`, `:26-33`, `:44-46`, `:181-204`, `:216-219`; `contracts/decide-apr-v1.yaml:93-101`, `:255-258`; `contracts/decide-tool-boundary-v1.yaml:286-289`, `:302-306`; `scripts/laya_train/train.py:320`; `/Users/guy/projects/mcp/motivation-experiment/docs/superpowers/specs/2026-10-20-motivation-experiment-design.md:343-346`.

### R3 - deployment path and auth

**Answer:** Today: pmcp.run via cargo-pmcp, auth off by the owner's decision. Requested: a Lambda-direct `AWS_IAM` Function URL. No decide recipe does that (C7).
**Status:** VERIFIED
**Proposal:** B5: a recipe that creates or updates the function at 10,240 MB arm64 with the env vars the shim reads (`APRENDER_DECIDE_S3_URI`, `APRENDER_DECIDE_SHA256`), a Function URL with `--auth-type AWS_IAM`, no public permission statement, the `s3:GetObject` statement on the role, a readback and an evidence JSON. The function name is to be agreed (your fixture says `decide-classifier`); no resource policy is planned, since your roles' identity policies grant the invoke (C7).
**Evidence:**
- `crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template:43`, `:115-126`; `crates/aprender-mcp-decide-lambda/README.md:15-36`; `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/deferred-items.md:313-320`, `:615-623`; `justfile:4509`, `:4524-4534`; `/Users/guy/projects/mcp/motivation-experiment/infra/lib/experiment-coach-stack.ts:262-283`; `/Users/guy/projects/mcp/motivation-experiment/infra/test/stack.test.ts:21`.

### R4 - dataset and split rules for GoldStandard2024

**Answer:** The CSV has 11311 rows: Biased 0: 9358, 1: 1953 (matching the spec); keywords Jews 6365, Israel 4134, ZioNazi 529, Kikes 283; 10644 distinct texts, so 667 rows are duplicates, and 5 texts carry both labels. The back office refuses identical texts with different labels, eval text overlapping train text, and a criterion with no eval row; it draws a seeded, stratified, text-group-disjoint 25 % calibration slice; and since `shots_per_class = max(class_counts)` is far above 16, `--epochs` in [4, 12] must be declared. No prepare script for this dataset exists.
**Status:** VERIFIED
**Proposal:** `scripts/laya_train/prepare_goldstandard.py (new)`, a sibling of `prepare_stance.py`: task.json (criteria `not_antisemitic` then `antisemitic`), train.jsonl and eval.jsonl (optionally shift.jsonl), a declared seed, duplicates collapsed by the contract's text hash, the 5 conflicting texts dropped and counted, a stratified text-group-disjoint held-out eval split, no tweet text committed. The eval size is the owner's; 20 % is proposed (2128 of 10639 usable texts, about 390 positives, 8511 left to train), the only precedent being the demo's 459 held-out rows against 192 training rows.
**Evidence:**
- facts T2-07; `/Users/guy/projects/mcp/motivation-experiment/docs/superpowers/specs/2026-10-20-motivation-experiment-design.md:341-343`; `scripts/laya_train/data.py:1-30`, `:210-219`; `scripts/laya_train/train.py:749-757`; `scripts/laya_train/contract.py:216-240`; `contracts/laya-finetune-gate-v1.yaml:193`, `:205-212`; `scripts/laya_train/prepare_stance.py:1-30`.

### R5 - SetFit "on the same splits"

**Answer:** SetFit training is CPU-only by design (a CUDA device is refused) and its pair budget is quadratic in shots (`neg_cap = 3n^2`). The lifecycle contract judges a run only inside a calibrated (seed, cell) regime: `s8e1b4`, `s16e2b8`, `s8e1b16`, `s16e1b16`, `s32e1b16`, `s64e1b16`; anything else fails closed with `UncalibratedRegime`. A SetFit run on 8511 training rows is outside every calibrated regime.
**Status:** VERIFIED
**Proposal:** Recommended: a SetFit `s64e1b16` cell drawn (seed 13) from the SAME train split, evaluated on the SAME eval split, reported as "same eval split, subsampled train" (its analog took 29 min). Alternative, not promised: a new calibrated regime, a contract change the owner would fund.
**Evidence:**
- `/Users/guy/.claude/projects/-Users-guy-Development-machine-learning-aprender/memory/aprender-setfit-calibration-is-cpu-only.md`; `contracts/setfit-train-lifecycle-v1.yaml:244`, `:471-480`, `:617`, `:1281`; `.claude/skills/spike-findings-aprender/references/laya-decision-model.md:35`.

### R6 - the annotation guide is not in the request repo

**Answer:** In motivation-experiment "annotation guide" occurs only in the request, the phase-1 plan and the spec; no file carries the guide's text, a filename or a reference. aprender needs the bytes: they become the task.json prefix (C6) and part of the artifact identity. The orchestrator's web search of 2026-10-21 points to Jikeli, Karali, Miehling and Soemer (arXiv 2304.14599), not opened or verified by the executor.
**Status:** UNKNOWN - which document is "the guide" and its exact text; settled when you supply the file.
**Proposal:** experiment-coach supplies the guide as a file (or a version-pinned reference) and approves the condensed form from R2 before B1 starts.
**Evidence:**
- facts T2-17; `/Users/guy/projects/mcp/motivation-experiment/docs/requests/aprender.md:5`; `/Users/guy/projects/mcp/motivation-experiment/docs/superpowers/plans/2026-10-20-experiment-coach-phase1.md:91`; `/Users/guy/projects/mcp/motivation-experiment/docs/superpowers/specs/2026-10-20-motivation-experiment-design.md:343-345`.

### R7 - the timing premise

**Answer:** Your cold figure is the spike's single-text number without a pin or an edge; production's proven-cold 800-token calls through the edge took 22647-27015 ms, about 2x your premise (sha256 check alone 3483-3522 ms). Warm calls took 866-980 ms, below your per-tweet figure only if they carried eight texts, which the evidence does not record.
**Status:** VERIFIED
**Proposal:** Restate the acceptance test as cold and warm calls of your real batch shape (8 texts) against the delivered URL, measured by you, with aprender's readback evidence alongside.
**Evidence:**
- `.claude/skills/spike-findings-aprender/references/aws-mcp-model-hosting.md:46`; `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-LIVE-REDEPLOY-EVIDENCE.json` `live.cold_wall_ms`, `live.cold_samples[*].sha_ms`, `live.warm_ms.range`.

## 3. Build breakdown

Each item names its Phase 8 analog plan(s); the span is the first-commit date of their SUMMARY files minus the previous plan's (read-only `git rev-list`). Every analog landed the same day as its predecessor (all 33 executed plans fell between 2026-09-25 and 2026-09-29, a weekend inside), so every span is 0 and each item is 1 day. Agent-sprint spans, owner present.

### B1 - inputs and data preparation

**Answer:** task.json (`not_antisemitic`, `antisemitic`, the condensed guide), train.jsonl, eval.jsonl, the guide's sha256 and the measured prefix token count. Needs the guide file and your condensation approval (R6, R2).
**Status:** VERIFIED
**Effort basis:** 08-02 (back office, 2026-09-25), 08-08 (`prepare_stance.py`, 2026-09-25), 08-29 (data rules, 2026-09-28): spans 0, so 1 day.
**Evidence:**
- facts T2-19, T2-20; `scripts/laya_train/prepare_stance.py:1-30`; `scripts/laya_train/data.py:1-30`.

### B2 - the one-epoch training-time probe

**Answer:** One seed, one epoch, the real task.json and rows: it measures seconds per step at the real row length and replaces B3's estimate. Needs B1.
**Status:** VERIFIED
**Effort basis:** 08-16's run (684 s wall for three seeds at 192 rows, 2026-09-27): 1 day.
**Evidence:**
- `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-16-SUMMARY.md:105-107`, `:145`.

### B3 - the declared three-seed gate run, pack, verify, the sha

**Answer:** `just laya-train`, three seeds, declared `--epochs`, the median-ECE seed ships; pack and verify give the `.apr` and its sha256. Training-time ESTIMATE: the demo's `train_seconds=104.5` is one seed's figure; that seed ran 5 epochs of `ceil(144 / 8) = 18` steps (144 fit rows = 192 minus the 48-row slice, computed), so 90 steps and 1.16 s per step (1.45 if the 4-epoch seed is meant). GoldStandard: `n_fit_gs` about 6383 (8511 training texts minus the 25 % slice), 798 steps per epoch; hours = per-step x 798 x E x 3 / 3600: E = 4 gives 3.1 h (3.9 at 1.45 s), E = 12 gives 9.3 h (11.6). Per-step time grows with row length and differs by device, so this is an estimate until B2 runs.
**Status:** PARTLY - the inputs are read; the result is an estimate.
**Effort basis:** 08-16 (2026-09-27; 684 s wall, pack 232 s, verify 214 s): span 0, so 1 day, plus `ceil(hours / 24) = 1` training day at E = 12: 2 days.
**Evidence:**
- `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-16-SUMMARY.md:105-107`, `:134-136`, `:145`; `scripts/laya_train/train.py:382`, `:392-393`, `:924-926`; `scripts/laya_train/contract.py:216-240`; `justfile:1327`.

### B4 - the SetFit comparison cell

**Answer:** R5's `s64e1b16` cell, same train and eval splits, CPU-only, parallel to B3 and B5.
**Status:** VERIFIED
**Effort basis:** the Phase 5 cells (29 min at 64 stance shots): 1 day.
**Evidence:**
- `.claude/skills/spike-findings-aprender/references/laya-decision-model.md:35`; `contracts/setfit-train-lifecycle-v1.yaml:471-480`.

### B5 - the Lambda-direct IAM Function URL deploy recipe

**Answer:** R3's recipe, readback and evidence JSON, patterned on `llm-endpoint-url` with NONE replaced by `AWS_IAM`, reusing `laya-build-bootstrap`, `laya-upload` and the S3 read grant. Needs AWS access, the agreed function name and B3's artifact.
**Status:** VERIFIED
**Effort basis:** 08-07 (2026-09-25), 08-10 (2026-09-26), 08-17 (2026-09-27), 08-30 (2026-09-28): spans 0, so 1 day.
**Evidence:**
- `justfile:4422`, `:4509`, `:4524-4534`, `:1759-1772`, `:2384`; `crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template:115-126`; facts T2-19.

### B6 - acceptance replay and hand-off

**Answer:** The request's four tests replayed against the delivered URL (your client or a SigV4 call), then the sha and the evidence JSON handed over.
**Status:** VERIFIED
**Effort basis:** 08-25 (2026-09-28) and 08-30 (2026-09-28): spans 0, so 1 day.
**Evidence:**
- `/Users/guy/projects/mcp/motivation-experiment/docs/requests/aprender.md:26-30`; `justfile:2599`; facts T2-19.

## 4. Proposed ready-by date

Rule: Day 0 is the as-of date; each B item costs `max(1, its analog's span in days)`; B1, B2, B3, B5, B6 add serially; B4 runs in parallel and adds only what exceeds the B3 plus B5 window; B3 gains `ceil(hours / 24)` training days at E = 12; one gate re-run (B3 again) is budgeted (the gate's history: the stance demo failed the ECE clause twice, ece_post 0.3773 and 0.2224). Calendar days (the analogs ran through a weekend).

| Item | Analog | Span (days) | Serial or parallel | Days | Calendar |
|---|---|---|---|---|---|
| B1 | 08-02, 08-08, 08-29 | 0 | serial | 1 | 2026-10-22 |
| B2 | 08-16 | 0 | serial | 1 | 2026-10-23 |
| B3 | 08-16 (+ 1 training day) | 0 | serial | 2 | 2026-10-24 to 2026-10-25 |
| Re-run | B3 again | 0 | serial | 2 | 2026-10-26 to 2026-10-27 |
| B4 | Phase 5 cells | 0 | parallel (inside B3 + B5) | 0 | Days 3-7 |
| B5 | 08-07, 08-10, 08-17, 08-30 | 0 | serial | 1 | 2026-10-28 |
| B6 | 08-25, 08-30 | 0 | serial | 1 | 2026-10-29 |

**Proposed ready-by:** 2026-10-29 (proposed by the aprender agent on 2026-10-21, pending the owner's confirmation)
**Derivation:** 1 + 1 + 2 + 2 + 1 + 1 = 8 calendar days after 2026-10-21, B4 adding 0, gives 2026-10-29.
**Conditions:** the guide file, the condensation approval and the owner's confirmation of this phase before B1 (if any arrives after 2026-10-22, the date slides day for day); AWS access under the justfile's deploy profile (`ze-kasher-dev`) and the agreed function name before B5 (if either arrives after 2026-10-28, the date slides day for day); the analog's training host (`mps:0`), otherwise B2 re-prices B3.
**Probe:** B2's one-epoch run replaces the 1.16-1.45 s per-step estimate with a measurement at the real row length; the training day count stays 1 while the measured per-step time is at most 3.0 s, each 3.0 s beyond it adding a day to B3 and to the re-run.

## 5. What is measured and what is not

Measured (sources as cited in C9 and B3):
- Spike 026, default Lambda 10,240 MB, direct (local proxy, no pin, no edge), single text: cold 11.3-12.0 s, warm 0.75 s Graviton2 / 0.49 s Graviton3 (`.claude/skills/spike-findings-aprender/references/aws-mcp-model-hosting.md:46`).
- Production 10,240 MB, pmcp.run edge, maximal 800-token requests (2 and 8 texts), binaries `007edcb6f` and `b615d8244`: cold 22647-27015 ms (download 8954-9139, sha 3483-3522, build 2808-2928, classify 6588-8911, gateway overhead 624-2751 ms); 10 warm calls 866-980 ms, shape unrecorded (`08-LIVE-REDEPLOY-EVIDENCE.json` `live.*`).
- Superseded 3,008 MB tier, pmcp.run edge, 120-token requests: cold 24168-29350 ms, warm p50 1418 ms single text, one external cold call 31.05 s (`08-LIVE-DEPLOY-EVIDENCE.json`).
- Training 684 s wall, `train_seconds=104.5` one seed at 192 rows on `mps:0`, pack 232 s, verify 214 s (08-16); SetFit 29 min at 64 stance shots, CPU.

Not measured: a Function URL deployment of this server; IAM signing overhead; any call with a GoldStandard-sized prefix; training time at that row length; SetFit at 11311 rows; x86_64 for Laya; your client against any aprender endpoint; the shape of the ten warm calls.

## 6. motivation-v1

### M1 - training data today

**Answer:** `datasets/motivation/` holds one workbook, `Round1_200_Posts_Classified.xlsx`, sheets About, Re-check, All 200, Social-affiliative (clear), Conspiratorial (clear), Mixed (XML row counts 16, 98, 201, 20, 27, 5, headers included). The spec's round-1 classes over 200 posts: social-affiliative 19, conspiratorial 26, other weak 63, other none 47, mixed 4, not classified 41; served labels `[social_affiliative, conspiratorial, other]` (not-classified excluded, mixed by the precedence rule).
**Status:** VERIFIED
**Evidence:**
- facts T2-27; `/Users/guy/projects/mcp/motivation-experiment/docs/superpowers/specs/2026-10-20-motivation-experiment-design.md:370-385`.

### M2 - what aprender's gate implies

**Answer:** Under the served labels the usable rows are 19 + 26 + 110 + 4 = 159 (arithmetic). The 25 % slice takes `ceil(0.25 x 19) = 5` from the smallest class, leaving 14 to fit, inside the at-most-16-shots regime whose epochs are fixed at 12 and where the ECE clause failed twice on 16-shot stance data (gate history; spike 027). eval.jsonl must hold every criterion. The counts to collect are the owner's call; one target, arithmetic: the demo's 64 fit rows per class need `ceil(64 / 0.75) = 86` labelled rows per class before the slice. No date is proposed for motivation-v1.
**Status:** VERIFIED
**Evidence:**
- `contracts/laya-finetune-gate-v1.yaml:158-159`, `:191`, `:192`, `:209`; `scripts/laya_train/contract.py:216-240`; `scripts/laya_train/data.py:210-219`; facts T2-29.

## 7. How aprender will carry the build

**Proposed phase:** Phase 10 - `antisemitism-v1` decide server for experiment-coach (ROADMAP.md has no `### Phase 10`; Phase 8 at `.planning/ROADMAP.md:636`, Phase 9 at `.planning/ROADMAP.md:753`). Goal: a gate-passing Laya artifact on GoldStandard with the condensed guide as its task, served from a Lambda-direct IAM Function URL at 10,240 MB in account 816069129666, sha communicated, acceptance tests replayed.

- Plan 1 - data and task.json: the guide file, the condensation decision as a blocking checkpoint, `prepare_goldstandard.py (new)`, the declared split, the prefix token count (B1).
- Plan 2 - the one-epoch probe and the three-seed gate run with `--epochs` set from it (B2, B3).
- Plan 3 - pack, verify, the sha, and the SetFit `s64e1b16` cell on the same eval split (B3, B4).
- Plan 4 - the Lambda-direct `AWS_IAM` Function URL recipe, readback and evidence (B5).
- Plan 5 - acceptance replay against the delivered URL and the hand-off record (B6).

Dependencies on experiment-coach: the guide file and condensation approval (Plan 1), the function name and your batch-size change (Plans 4 and 5). This reply does not edit ROADMAP.md; the owner runs `/gsd-phase add`.

## 8. Open items and owners

- Guide file and condensed-form approval: experiment-coach; not supplied (R6, R2).
- Function name: experiment-coach with aprender; fixture says `decide-classifier`, not agreed (R3).
- `classify_batch` from 32 to at most 8 plus a token budget: experiment-coach; not changed (R1).
- Eval split size: the owner; 20 % proposed (R4).
- SetFit regime: the owner; same-eval-split `s64e1b16` cell recommended, a new regime not promised (R5).
- Door bounds for the Function URL envelope: the owner; not started (R1).
- Confirmation of the date 2026-10-29 and of the phase: the owner (sections 4 and 7).
