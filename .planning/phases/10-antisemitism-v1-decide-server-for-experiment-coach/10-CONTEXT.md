# Phase 10: antisemitism-v1 Decide Server for experiment-coach - Context

**Gathered:** 2026-10-21 (the owner's answers to aprender's reply, given in the session that produced quick task 261021-ia2; no separate discuss round was run because the owner answered every gray area the reply raised)
**Status:** Ready for planning

<domain>
## Phase Boundary

Deliver the first PRODUCTION decider for a sibling project. experiment-coach (motivation-experiment,
`~/projects/mcp/motivation-experiment`) asked aprender, in `docs/requests/aprender.md`, for a decide server
artifact `antisemitism-v1`: labels `[not_antisemitic, antisemitic]`, trained on GoldStandard2024 with the
annotation guide as the task's instructions and criteria, the calibration gate applied, SetFit run on the same
splits for comparison, the artifact sha256 communicated for pinning, one tool `classify` with the shape the
Phase 8 server already serves. aprender's reply
(`.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-EXPERIMENT-COACH-DECIDE-REQUEST-REPLY.md`,
sections 1-8) checked the contract and surfaced seven mismatches; the owner answered them on 2026-10-21 (section
9 of the reply). This phase is the build those answers authorise.

In scope: data preparation from the CSV, the condensed `task.json`, the one-epoch probe, the three-seed gate run,
pack and verify, the SetFit comparison cell, the redeploy through the existing pmcp.run path, the acceptance
replay and the hand-off record.

Not in this phase: `motivation-v1` (deferred until the first classifier's success is judged); a Lambda-direct IAM
Function URL deployment (dropped by the owner); re-deriving the door bounds for a new envelope; changes to the
decider builder or inference (deferred until this classifier's accuracy readout); any change inside
motivation-experiment (their batch size and client transport are theirs).

</domain>

<decisions>
## Implementation Decisions

Every decision below is either the owner's (2026-10-21, quoted or paraphrased from their answer) or a default
the reply proposed that the owner accepted. The reply's section numbers (C, R, B, M) are the trace.

### Scope and order
- **D-01 (owner):** "Let's start with the first model to classify antisemite posts, and we can build the second
  motivation classifier later based on the success of the first one." `antisemitism-v1` only. Reply section 6
  (motivation-v1) is information for later, not work here.
- **D-02 (request):** labels `[not_antisemitic, antisemitic]` in that order; `task.json` criteria DOCUMENT ORDER
  is the label index (decide-apr-v1 `task_json_schema`), so the criteria object lists `not_antisemitic` first.

### Data (reply R4, accepted by the owner: "Dataset and comparison caveats are accepted")
- **D-03:** Source is `datasets/antisemite/GoldStandard2024.csv` in motivation-experiment (11,311 rows, columns
  `ID,Username,CreateDate,Biased,Keyword,Text`; `Biased` 0: 9,358 / 1: 1,953; 667 duplicate texts, 5 texts
  carrying both labels — reply R4 and its facts rows). A `prepare_goldstandard.py` beside
  `scripts/laya_train/prepare_stance.py` writes the Laya data dir: duplicates collapsed to one row,
  conflicting-label texts resolved by a rule declared in the script before the run (Claude's discretion which
  rule; dropping the text is the default), a seeded, stratified, text-group-disjoint 20 % held-out `eval.jsonl`
  (the back office refuses overlap and draws the calibration slice itself), and no tweet text committed or
  printed (counts and sha256s only, as prepare_stance does). The data dir lands under the gitignored `/data/` root.
- **D-04:** The requester's live 400-post labelled sample (spec 5.4), when it exists, is a `shift.jsonl`-style
  reported probe for a later run, never a gate clause (laya-finetune-gate-v1 `eval_set` A2). Not awaited here.

### task.json and the guide (reply C6, R2, R6; owner's answers)
- **D-05 (owner):** "We can fit the guide to the allotted size, and we can make it concise if needed." The
  instructions and the two criterion descriptions are a CONDENSED form of the guide, sized to a declared prefix
  token budget so that eight tweets still fit the 800-built-token door (the reply's R2 inequality, `P + t <= 100`
  per row at the served `head_max_len` 192 / `max_len` 512). The budget is measured with the served tokenizer
  (Laya-en snapshot 55cf4c4e) before training, recorded in the run record, and the measured number — not the
  estimate — goes into the hand-off.
- **D-06 (owner):** "We can add the guide text as needed based on the published paper." The source text is
  Jikeli, Karali, Miehling, Soemer, "Antisemitic Messages? A Guide to High-Quality Annotation and a Labeled
  Dataset of Tweets" (arXiv 2304.14599; the 12-paragraph IHRA-based guideline). aprender drafts the condensation;
  the owner reviews the exact `task.json` text at a blocking checkpoint BEFORE the gate run, because its bytes
  become part of the artifact identity (`inputs_sha256.task_json`). The paper's citation and the sha256 of the
  condensed text are recorded together.
- **D-07 (owner, direction):** "Ideally, we would like to have for the decider in general a good combination of
  examples and textual instructions (guide). In some cases we will have more examples, and in some cases less,
  and the guide will be more important. Since this is the first production decider, we can test its accuracy and
  see if we need to invest in any of the improvement to the decider builder and inference." Consequence: the
  examples-versus-instructions balance is a per-task parameter, not something this phase redesigns. The phase
  records the accuracy readout (eval split, and the live labelled set when it arrives) in a form that lets the
  owner decide on builder or inference work afterwards; no such work is planned here.

### Training recipe and gate (reply B2, B3; unchanged Phase 8 policy)
- **D-08:** Production variant on the pinned base; three seeds 13/17/23; the shipped seed is the median-ECE seed
  (laya-finetune-gate-v1 `seed_policy`); early stopping; `--epochs` is declared from the one-epoch probe's measured
  per-step time, within the contract's [4, 12]; the device actually used is read back (MPS expected on this host;
  CPU flagged). Thresholds unchanged: margin over zero-shot >= 0.05 and top-label ECE <= 0.10.
- **D-09:** A gate fail HALTS for the owner (as Phase 8 D-19). One re-run is already budgeted in the proposed
  date; a second is the owner's call.
- **D-10:** The one-epoch probe (reply B2) runs first and replaces the 1.16-1.45 s per-step ESTIMATE with a
  measurement at the real row length; its result sets the epoch count and re-prices the date if the per-step
  time exceeds the reply's 3.0 s threshold.

### Comparison (reply R5, B4; owner accepted)
- **D-11:** SetFit is run as a single `s64e1b16` cell drawn from the SAME train split and evaluated on the SAME
  eval split, recorded beside the Laya numbers under the Phase 5 claims discipline and labelled "same eval split,
  subsampled train". No new calibrated regime is created (a full-size SetFit run is outside every calibrated
  regime and the lifecycle refuses it; SetFit training is CPU-only and quadratic in shots).

### Deployment (reply C7, R3, B5; owner's answer)
- **D-12 (owner):** "The deployment is exactly the same as an MCP server to the pmcp.run platform. No new
  deployment model is needed." The artifact is redeployed through the existing path — `just laya-build-bootstrap`,
  `just laya-deploy-config <apr> <auth>`, cargo-pmcp to pmcp.run, plan 08-30 as the analog — on the 10,240 MB
  tier. The Lambda-direct IAM Function URL recipe the reply sketched as B5 is DROPPED.
- **D-13 (open, decided at the deploy checkpoint):** `laya-deploy-config` resolves only the server name
  `aprender-mcp-decide`, so deploying the new artifact there REPLACES the stance demo. Default: replace (the
  pinned sha carries the identity, and the stance demo was a demo). A second server name is recipe work the
  owner must ask for. The auth posture defaults to the existing deployment's (off, D-ITEM-08-18-A, risk accepted
  by the owner) unless the owner says otherwise at the same checkpoint. The live deploy is a human checkpoint
  (Phase 8 D-18).
- **D-14 (consequence for the partner, theirs):** experiment-coach's client signs SigV4 to a Function URL and
  its stack grants `lambda:InvokeFunctionUrl`; a pmcp.run endpoint is a plain HTTPS MCP POST
  (`https://<server>.us-east.true-mcp.com/mcp`, stateless `tools/call`) with the auth posture of that
  deployment. Their spec's decision table also records "server-to-server MCP through pmcp.run is not a
  supported path from a tool". Both are theirs to reconcile; the hand-off record states the endpoint, the auth
  posture and the sha so they can.

### Door, batch and timings (reply R1, R7, C9; owner's answers)
- **D-15 (owner):** "Batch size can be smaller and 8 is fine." The door stays 8 texts / 800 built tokens
  (decide-tool-boundary-v1); experiment-coach lowers `classify_batch` from 32 to at most 8 on their side. No door
  re-derivation in this phase.
- **D-16 (owner):** "No problem with cold start, as we are running it as part of an agent work, which is less
  sensitive to latency." The acceptance replay tests function and identity (one result per text, labels in task
  order, probabilities summing to 1, returned sha equal to the pin); cold and warm timings are REPORTED from the
  deployed function, never gated and never quoted from a spike.

### Hand-off and date (reply sections 4, 8)
- **D-17:** The hand-off record (phase directory) carries: artifact sha256, labels in task order, the endpoint
  and auth posture, the condensed `task.json` text and its sha256, the gate report summary, the SetFit cell
  numbers, the acceptance replay, and the measured timings. The requester pins by sha; nothing else is promised.
- **D-18:** Proposed ready-by 2026-10-29 (reply section 4), not contested by the owner. Its remaining
  conditions: the owner's `task.json` review (D-06) before the gate run, and pmcp.run deploy access under the
  existing profile before the redeploy. A gate re-run beyond the one budgeted slides it.

### Claude's Discretion
- The conflicting-label rule and the exact split seed and grouping rule, declared in the prepare script.
- The condensed guide's draft wording (the owner reviews the final text, D-06).
- How the one-epoch probe is invoked (a flag on `train.py` versus a separate driver) so it writes no run dir
  the gate could later mistake for a production run.
- Whether the SetFit cell reuses the Phase 5 recipes as they are or needs a thin data adapter from the Laya
  data dir; one implementation per operation (OPS-03) either way.
- Evidence file names in the phase directory, following Phase 8's `08-*-EVIDENCE.json` pattern.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### The request and the reply
- `/Users/guy/projects/mcp/motivation-experiment/docs/requests/aprender.md` — the request (its Date line now carries the proposed 2026-10-29)
- `/Users/guy/projects/mcp/motivation-experiment/docs/superpowers/specs/2026-10-20-motivation-experiment-design.md` — sections 4 (Constraints), 5.4 (Classifier contract and go-live gate), 7 (Dependencies on sibling projects)
- `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-EXPERIMENT-COACH-DECIDE-REQUEST-REPLY.md` — sections 1-9 (section 9 is the owner's decisions)
- `.planning/quick/261021-ia2-aprender-side-reply-to-the-experiment-co/261021-ia2-FACTS.md` — every number in the reply with its source

### Phase 8, which this phase builds on
- `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-CONTEXT.md` — D-01..D-19 and their amendments (recipe, gate, seeds, tool surface, deploy)
- `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-LIVE-REDEPLOY-EVIDENCE.json` — the 10,240 MB deployment of record (plan 08-30)
- `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-16-SUMMARY.md` and `08-GATE-RUN-EVIDENCE.json` — the one declared gate run (the training-time analog)
- `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/deferred-items.md` — D-ITEM-08-10-D and D-ITEM-08-18-A (auth posture)
- `scripts/laya_train/README.md`, `prepare_stance.py`, `train.py`, `data.py`, `contract.py` — the back office
- `contracts/laya-finetune-gate-v1.yaml`, `contracts/laya-parity-v1.yaml`, `contracts/decide-apr-v1.yaml`, `contracts/decide-tool-boundary-v1.yaml`
- `crates/aprender-decide/` (`src/artifact.rs` row build, `src/task.rs`, `examples/pack_laya.rs`), `crates/aprender-mcp-decide-lambda/README.md` and `.pmcp/deploy.toml.template`
- `justfile` recipes `laya-build-bootstrap`, `laya-deploy-config`, `laya-deploy-selftest`

### Skills and memory
- `Skill("spike-findings-aprender")`: `references/laya-decision-model.md`, `references/laya-calibration-gate-and-pack-tolerance.md`, `references/aws-mcp-model-hosting.md`
- Project memory: SetFit calibration is CPU-only and quadratic in shots; door bounds are tier policy; tiny-fixture bit-identity is not real-weight identity; the Phase 4 verification status breaks several gsd-tools verbs (pass the phase number explicitly; record STATE.md by hand)

</canonical_refs>

<deferred>
## Deferred Ideas

- `motivation-v1` (labels `[social_affiliative, conspiratorial, other]`, round-1 data 19 / 26 / ~110 usable rows): after this classifier's success is judged (D-01); the reply's section 6 holds the gate arithmetic.
- Door bounds re-derived for a different envelope (a Function URL without the 30 s gateway cap): not needed under D-12.
- Decider builder and inference improvements for the examples-versus-instructions balance (D-07): after the accuracy readout.
- A second decide server name in `laya-deploy-config` (D-13): only if the owner wants the stance demo kept.
- The partner's client transport change and batch-size change (D-14, D-15): theirs.

</deferred>
