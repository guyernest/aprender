# Phase 10: antisemitism-v1 Decide Server for experiment-coach - Research

**Researched:** 2026-10-21
**Domain:** Laya back office (scripts/laya_train) on a new 11k-row binary dataset; the decide-apr-v1 pack/verify ladder; the existing pmcp.run redeploy path (`aprender-mcp-decide`, 10,240 MB); a SetFit s64e1b16 comparison cell through the Phase 2/5 attested-data doors
**Confidence:** HIGH on the back office, the row builder, the deploy path and the door arithmetic (every claim read from the file this session, and the two tokenizer measurements below reproduced the contract's own 57); MEDIUM on time/memory projections (every one is an estimate until the D-10 probe); MEDIUM-LOW on the SetFit cell (the Phase 5 machinery is TweetEval-bound at two doors; the generic doors exist but no GoldStandard dataset has ever passed them)

**Read-only session.** No training run, no download, no AWS or pmcp.run call, no `just` recipe executed. The only computations were `python3 -I` counts over the partner CSV and a tokenizer-only measurement with the pinned Laya tokenizer (no model forward). Counts and hashes only; no tweet text was printed or copied.

## Summary

Phase 10 is a re-run of Phase 8's pipeline on a dataset 44x larger per epoch, with one new prepare script, one new probe driver, a condensed-guide `task.json`, and a data adapter for the SetFit comparison. The back office (`train.py`, `data.py`, `contract.py`, `gate.py`) binds nothing to the stance demo: the only stance-specific branch is `f_avg`, which is `None` for any other label list (train.py:787-789, verify.rs:2939-2949). A production run on GoldStandard is contract-clean provided `--epochs` in [4, 12] is declared (shots_per_class = 7017 >> 16) and `--seeds` is left at the contract's 3.

Three facts measured this session reshape the plan. (1) **The prefix budget is a two-sided bracket, not a target.** The stance task's empty-text row measures 57 with the served tokenizer (the contract's `served_task_min_row_tokens: 57` reproduced, so the method is right); a bare two-label task with a one-line question measures 24; the maximal two-option prefix measures 196. GoldStandard tweets tokenize to p50 56, p95 94, p99 130 (n = 11,311). The server REFUSES TO BUILD when `8 x P > 800` (`check_served_task_fits`, lib.rs:751-766), so P <= 100 is a hard ceiling; the D-05 intent "eight tweets fit" holds for the MEDIAN tweet only when P <= 44 and for NO P at the p95 tweet. The owner's D-06 checkpoint must see these numbers. (2) **The deploy path is blocked until the resolver proof is re-run**: the installed cargo-pmcp is 0.28.1 (a path install from `~/Development/mcp/sdk/rust-mcp-sdk`), the committed proof says 0.24.3, and `laya-deploy` refuses on that mismatch (justfile:1838-1840). (3) **The SetFit bench row cannot carry this cell**: `apr setfit bench run` binds the TweetEval manifest revision, the 40-cell claims contract, `F_avg = (F1_against + F1_favor)/2` and a `[3][3]` confusion matrix. The generic doors (`PreparedDataset::<Canonical>::from_labeled_rows`, `apr data select`, `apr setfit train`, `apr eval setfit --split test`) are label-count generic, so the cell runs through them behind a new thin adapter and is recorded in a phase evidence file under the Phase 5 recording discipline, not as a bench row.

**Primary recommendation:** Plan six plans in the reply's order (data + task.json with the owner's checkpoint; probe; gate run; pack/verify + SetFit cell; redeploy with the resolver re-proof and the contract edit for the new served task; acceptance replay + hand-off), and put the three measured facts above into Plan 1's checkpoint text and Plan 5's preconditions.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Scope and order
- **D-01 (owner):** "Let's start with the first model to classify antisemite posts, and we can build the second motivation classifier later based on the success of the first one." `antisemitism-v1` only. Reply section 6 (motivation-v1) is information for later, not work here.
- **D-02 (request):** labels `[not_antisemitic, antisemitic]` in that order; `task.json` criteria DOCUMENT ORDER is the label index (decide-apr-v1 `task_json_schema`), so the criteria object lists `not_antisemitic` first.

#### Data (reply R4, accepted by the owner: "Dataset and comparison caveats are accepted")
- **D-03:** Source is `datasets/antisemite/GoldStandard2024.csv` in motivation-experiment (11,311 rows, columns `ID,Username,CreateDate,Biased,Keyword,Text`; `Biased` 0: 9,358 / 1: 1,953; 667 duplicate texts, 5 texts carrying both labels — reply R4 and its facts rows). A `prepare_goldstandard.py` beside `scripts/laya_train/prepare_stance.py` writes the Laya data dir: duplicates collapsed to one row, conflicting-label texts resolved by a rule declared in the script before the run (Claude's discretion which rule; dropping the text is the default), a seeded, stratified, text-group-disjoint 20 % held-out `eval.jsonl` (the back office refuses overlap and draws the calibration slice itself), and no tweet text committed or printed (counts and sha256s only, as prepare_stance does). The data dir lands under the gitignored `/data/` root.
- **D-04:** The requester's live 400-post labelled sample (spec 5.4), when it exists, is a `shift.jsonl`-style reported probe for a later run, never a gate clause (laya-finetune-gate-v1 `eval_set` A2). Not awaited here.

#### task.json and the guide (reply C6, R2, R6; owner's answers)
- **D-05 (owner):** "We can fit the guide to the allotted size, and we can make it concise if needed." The instructions and the two criterion descriptions are a CONDENSED form of the guide, sized to a declared prefix token budget so that eight tweets still fit the 800-built-token door (the reply's R2 inequality, `P + t <= 100` per row at the served `head_max_len` 192 / `max_len` 512). The budget is measured with the served tokenizer (Laya-en snapshot 55cf4c4e) before training, recorded in the run record, and the measured number — not the estimate — goes into the hand-off.
- **D-06 (owner):** "We can add the guide text as needed based on the published paper." The source text is Jikeli, Karali, Miehling, Soemer, "Antisemitic Messages? A Guide to High-Quality Annotation and a Labeled Dataset of Tweets" (arXiv 2304.14599; the 12-paragraph IHRA-based guideline). aprender drafts the condensation; the owner reviews the exact `task.json` text at a blocking checkpoint BEFORE the gate run, because its bytes become part of the artifact identity (`inputs_sha256.task_json`). The paper's citation and the sha256 of the condensed text are recorded together.
- **D-07 (owner, direction):** "Ideally, we would like to have for the decider in general a good combination of examples and textual instructions (guide). In some cases we will have more examples, and in some cases less, and the guide will be more important. Since this is the first production decider, we can test its accuracy and see if we need to invest in any of the improvement to the decider builder and inference." Consequence: the examples-versus-instructions balance is a per-task parameter, not something this phase redesigns. The phase records the accuracy readout (eval split, and the live labelled set when it arrives) in a form that lets the owner decide on builder or inference work afterwards; no such work is planned here.

#### Training recipe and gate (reply B2, B3; unchanged Phase 8 policy)
- **D-08:** Production variant on the pinned base; three seeds 13/17/23; the shipped seed is the median-ECE seed (laya-finetune-gate-v1 `seed_policy`); early stopping; `--epochs` is declared from the one-epoch probe's measured per-step time, within the contract's [4, 12]; the device actually used is read back (MPS expected on this host; CPU flagged). Thresholds unchanged: margin over zero-shot >= 0.05 and top-label ECE <= 0.10.
- **D-09:** A gate fail HALTS for the owner (as Phase 8 D-19). One re-run is already budgeted in the proposed date; a second is the owner's call.
- **D-10:** The one-epoch probe (reply B2) runs first and replaces the 1.16-1.45 s per-step ESTIMATE with a measurement at the real row length; its result sets the epoch count and re-prices the date if the per-step time exceeds the reply's 3.0 s threshold.

#### Comparison (reply R5, B4; owner accepted)
- **D-11:** SetFit is run as a single `s64e1b16` cell drawn from the SAME train split and evaluated on the SAME eval split, recorded beside the Laya numbers under the Phase 5 claims discipline and labelled "same eval split, subsampled train". No new calibrated regime is created (a full-size SetFit run is outside every calibrated regime and the lifecycle refuses it; SetFit training is CPU-only and quadratic in shots).

#### Deployment (reply C7, R3, B5; owner's answer)
- **D-12 (owner):** "The deployment is exactly the same as an MCP server to the pmcp.run platform. No new deployment model is needed." The artifact is redeployed through the existing path — `just laya-build-bootstrap`, `just laya-deploy-config <apr> <auth>`, cargo-pmcp to pmcp.run, plan 08-30 as the analog — on the 10,240 MB tier. The Lambda-direct IAM Function URL recipe the reply sketched as B5 is DROPPED.
- **D-13 (open, decided at the deploy checkpoint):** `laya-deploy-config` resolves only the server name `aprender-mcp-decide`, so deploying the new artifact there REPLACES the stance demo. Default: replace (the pinned sha carries the identity, and the stance demo was a demo). A second server name is recipe work the owner must ask for. The auth posture defaults to the existing deployment's (off, D-ITEM-08-18-A, risk accepted by the owner) unless the owner says otherwise at the same checkpoint. The live deploy is a human checkpoint (Phase 8 D-18).
- **D-14 (consequence for the partner, theirs):** experiment-coach's client signs SigV4 to a Function URL and its stack grants `lambda:InvokeFunctionUrl`; a pmcp.run endpoint is a plain HTTPS MCP POST (`https://<server>.us-east.true-mcp.com/mcp`, stateless `tools/call`) with the auth posture of that deployment. Their spec's decision table also records "server-to-server MCP through pmcp.run is not a supported path from a tool". Both are theirs to reconcile; the hand-off record states the endpoint, the auth posture and the sha so they can.

#### Door, batch and timings (reply R1, R7, C9; owner's answers)
- **D-15 (owner):** "Batch size can be smaller and 8 is fine." The door stays 8 texts / 800 built tokens (decide-tool-boundary-v1); experiment-coach lowers `classify_batch` from 32 to at most 8 on their side. No door re-derivation in this phase.
- **D-16 (owner):** "No problem with cold start, as we are running it as part of an agent work, which is less sensitive to latency." The acceptance replay tests function and identity (one result per text, labels in task order, probabilities summing to 1, returned sha equal to the pin); cold and warm timings are REPORTED from the deployed function, never gated and never quoted from a spike.

#### Hand-off and date (reply sections 4, 8)
- **D-17:** The hand-off record (phase directory) carries: artifact sha256, labels in task order, the endpoint and auth posture, the condensed `task.json` text and its sha256, the gate report summary, the SetFit cell numbers, the acceptance replay, and the measured timings. The requester pins by sha; nothing else is promised.
- **D-18:** Proposed ready-by 2026-10-29 (reply section 4), not contested by the owner. Its remaining conditions: the owner's `task.json` review (D-06) before the gate run, and pmcp.run deploy access under the existing profile before the redeploy. A gate re-run beyond the one budgeted slides it.

### Claude's Discretion
- The conflicting-label rule and the exact split seed and grouping rule, declared in the prepare script.
- The condensed guide's draft wording (the owner reviews the final text, D-06).
- How the one-epoch probe is invoked (a flag on `train.py` versus a separate driver) so it writes no run dir the gate could later mistake for a production run.
- Whether the SetFit cell reuses the Phase 5 recipes as they are or needs a thin data adapter from the Laya data dir; one implementation per operation (OPS-03) either way.
- Evidence file names in the phase directory, following Phase 8's `08-*-EVIDENCE.json` pattern.

### Deferred Ideas (OUT OF SCOPE)
- `motivation-v1` (labels `[social_affiliative, conspiratorial, other]`, round-1 data 19 / 26 / ~110 usable rows): after this classifier's success is judged (D-01); the reply's section 6 holds the gate arithmetic.
- Door bounds re-derived for a different envelope (a Function URL without the 30 s gateway cap): not needed under D-12.
- Decider builder and inference improvements for the examples-versus-instructions balance (D-07): after the accuracy readout.
- A second decide server name in `laya-deploy-config` (D-13): only if the owner wants the stance demo kept.
- The partner's client transport change and batch-size change (D-14, D-15): theirs.
</user_constraints>

<phase_requirements>
## Phase Requirements

No REQ-IDs are mapped (`.planning/REQUIREMENTS.md` is the SetFit milestone's). The binding requirements are the ROADMAP's five Success Criteria; they are keyed SC-1..SC-5 here and in Validation Architecture.

| ID | Description (ROADMAP "### Phase 10", abridged) | Research Support |
|----|-------------|------------------|
| SC-1 | `prepare_goldstandard.py` writes the data dir by a declared, seeded, stratified, text-group-disjoint rule (20 % eval; 667 duplicates collapsed; 5 conflicts resolved by rule), never prints text; `task.json` carries the condensed guide within a declared, MEASURED prefix budget, its sha256 in the run record | Q1 (data.py rules, prepare_stance structure, measured CSV facts), Q2 (prefix bracket 24..196, hard ceiling 100, median-fit 44; measurement command) |
| SC-2 | One declared production run (seeds 13/17/23, `--epochs` from the probe, early stopping) with a gate report whose shipped seed passes both clauses; device read back, per-step time, noise record; a fail halts | Q3 (probe driver), Q4 (run-dir layout, exit codes, cost projections), Pitfalls 3-5 |
| SC-3 | `pack_laya` produces and verifies a deploy-eligible `.apr`; sha256 and labels in the hand-off | Q5 (pack/verify command lines, refusals, `inspect` for the hand-off fields) |
| SC-4 | SetFit s64 cell on the same train/eval splits recorded under the Phase 5 claims discipline, labelled "same eval split, subsampled train" | Q6 (which doors are generic, which are TweetEval-bound, the adapter, the recording file) |
| SC-5 | Live through the existing pmcp.run path at 10,240 MB, probe `--expect-sha256`, acceptance replay recorded, timings reported not gated; every gate green | Q7 (08-30 sequence, resolver re-proof, contract edit for the served task), Q8 (probe checks vs the requester's tests; partner parser), Q9 (gates) |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

Directives that bind this phase (each read from `./CLAUDE.md` this session):
- Realizar-first has a documented exception for decision models: `crates/aprender-decide` + the thin servers own classification; `aprender-serve` is untouched. SafeTensors is read only by the back-office packer (`aprender_decide::pack`), never by a server. Do not widen either.
- `pv` for every contract touched (`pv validate`, `pv diff` for the semver suggestion; two filesystem paths, never a git revision). `pv` is NOT on PATH on this box (Environment Availability): use `cargo run --release -p aprender-contracts-cli --bin pv --` (Makefile `PV_CARGO_RUN`, Makefile:2187) or install it first.
- `bashrs lint` for any new shell script; sourced libraries option-neutral; never read `$?` through a pipe (`|| rc=$?` form); the Bash tool runs zsh (memory): guard case tables must be run under bash.
- Pin the binary: any `apr` call goes through `. scripts/apr_bin.sh || exit 1` and `"$APR"` (the SetFit cell needs an `apr` built with `--features setfit` from HEAD).
- "Derive the numbers; do not quote them": every count in the hand-off comes from a command or file named beside it.
- Verification Discipline 2 and 6: never label a run by intent (cite the `DEVICE ... used=` line); one failing input is an anecdote (the gate's three seeds exist for this).
- `unwrap()` is banned (`.clippy.toml`); workspace lints `clippy::all + pedantic = "warn"`, strict clippy per touched crate with `-D warnings`; `cargo fmt --all -- --check`.
- Code search policy: `pmat query` (not run this session: read-only, no `pmat embed sync`); CB-510 root-anchored ignore patterns — the new data dir lives under `/data/` (root `.gitignore:67`) and run dirs under `/models/` (`.gitignore:62-63`).
- Publishing/readme drift gate: `crates/aprender-core/tests/readme_contract.rs` fails the build on a CLAUDE.md path that does not exist (FALSIFY-DOCS-CLAUDE-001, readme_contract.rs:462-468); the Phase 8 claims ledger `scripts/laya_claims.tsv` (`just laya-claims-check`, 133 rows) pins CLAUDE.md and README sentences about the live model.
- Autonomous mode: live AWS/pmcp.run writes and the gate's halt are owner checkpoints (Phase 8 D-18/D-19 pattern; 10-CONTEXT D-09, D-13).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| CSV -> Laya data dir (dedup, conflict rule, 20 % split, task.json) | Back office (Python, `scripts/laya_train`) | — | `prepare_stance.py` precedent; torch-free; the gitignored `/data/` root |
| Prefix-token measurement (P) and per-row token stats | Back office (Python, the pinned tokenizer via `laya.common.build_sequence`) | Rust `Builder` (parity-proven port) | Same tokenizer.json; train.py builds rows with `Agent._encode_state` (train.py:320), so the Python measurement IS the training row |
| One-epoch probe | Back office driver (new `probe_epoch.py` importing `train.train_seed`) | — | Writes no run dir; the spike-027 `run_cell.py` precedent |
| Three-seed gate run, calibration, noise record | Back office (`train.py`, MPS) | — | D-08; the only declared run |
| Pack / verify / inspect | Rust (`aprender-decide` example `pack_laya`) | — | decide-apr-v1 deploy_eligibility; the ONLY eligibility check |
| SetFit cell | Rust `apr` (`data select`, `setfit train`, `eval setfit`) behind a thin adapter | Back office writes the adapter's input | OPS-03: the crate's attested doors, not a re-implementation |
| Redeploy | justfile recipes + cargo-pmcp + S3 | Lambda shim (`aprender-mcp-decide-lambda`) | D-12; plan 08-30 analog |
| Acceptance replay | Rust probe example + a replay record | Partner's `classifier.rs` parser (theirs) | D-16 |
| Hand-off record | Phase directory JSON/MD | Partner reads it | D-17 |

## Standard Stack

No new packages. Everything is pinned already (Package Legitimacy Audit: nothing to audit).

### Core (all present on this box; versions read this session)
| Component | Version / pin | Purpose | Evidence |
|---------|---------|---------|--------------|
| `scripts/laya_train` uv project | Python 3.13.7, torch 2.14.0, transformers 5.17.0, tokenizers 0.23.1, laya `4066d5d`, base weights `55cf4c4e` | prepare, probe, train, gate | `scripts/laya_train/README.md:31-42` [VERIFIED]; `.venv` present, `uv 0.9.5` |
| `aprender-decide` example `pack_laya` | workspace | pack / verify / inspect | `crates/aprender-decide/examples/pack_laya.rs:1-45` [VERIFIED] |
| `aprender-mcp-decide-lambda` example `probe` | workspace | identity probe, `--plan-only`, cold samples | `examples/probe.rs:1-30` [VERIFIED] |
| `just` recipes `laya-*` | just 1.46.0 | the deploy path | justfile line numbers in Q7 |
| cargo-pmcp | **0.28.1 installed** (path install from `~/Development/mcp/sdk/rust-mcp-sdk`, source `cargo-pmcp/Cargo.toml` version 0.28.1); **proof says 0.24.3** | `cargo pmcp deploy` | `~/.cargo/.crates.toml`; `models/decide/resolver-proof.txt` line `cargo_pmcp_version=0.24.3` [VERIFIED] |
| cargo-zigbuild 0.23.4 | installed | `laya-build-bootstrap` | `command -v` this session |
| aws-cli 2.29.0, profile `ze-kasher-dev` listed | installed | S3 upload, readbacks | `aws configure list-profiles` this session (no AWS call made) |
| all-MiniLM-L6-v2 snapshot `1110a243fdf4706b3f48f1d95db1a4f5529b4d41` | present in `~/.cache/huggingface/hub` | SetFit `--model-dir` | matches the production regime's slice revision `@1110a243` (`setfit-train-lifecycle-v1.yaml:475`) [VERIFIED] |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| New `probe_epoch.py` driver | `train.py --variant synthetic-fixture --seeds 1 --epochs 1 --base <snapshot> --base-sha256 <sha>` | Legal (resolve_epochs accepts [0,12] for that variant, contract.py:220-229; `Base` takes a local dir, train.py:243-250) but it writes a full run dir, scores all eval rows three times and writes the noise record: the scoring tail (~20-40 min at 2,128 rows) dwarfs the one epoch it measures, and it leaves a `variant: synthetic-fixture` run dir under `models/decide`. Rejected. |
| Rust measurement of P (`Builder::from_bytes(tokenizer_bytes, 512, 192)`, builder.rs:89) | Python `build_sequence` | Both are the served tokenizer; Python is the exact training path (train.py:320) and needs no new Rust code. Use Python; cross-check with `probe --plan-only` once the `.apr` exists (Q2). |
| SetFit bench row (`apr setfit bench run`) | `apr setfit train` + `apr eval setfit --split test` behind an adapter | The bench binds TweetEval and 3 classes (Q6). Only the generic path is admissible. |

## Package Legitimacy Audit

No package is installed by this phase (no `pip`, `uv add`, `cargo add`, `npm`). Audit not applicable. **Packages removed due to [SLOP] verdict:** none. **Packages flagged [SUS]:** none.

## The nine questions, answered with evidence

### Q1. Data preparation

**What `data.py` validates and refuses** [VERIFIED: scripts/laya_train/data.py:93-129, 172-220, 222-228, 263-307; README.md:135-152]:
- `load_task`: exactly `{type, instructions, criteria}` (`TASK_KEYS = ("type", "instructions", "criteria")`, data.py:48), `type == "choice"`, non-empty instructions, >= 2 unique criterion names, each description a string or null; criteria parsed with `object_pairs_hook=list` so document order is the label index.
- `load_rows`: exactly `{text, label}` (`ROW_KEYS = ("text", "label")`, data.py:49), label a criterion NAME (an index is refused `*-row-label`), non-empty text, no blank lines, valid UTF-8, no lone surrogates; `eval` additionally refuses a criterion with no eval row (`eval-class-coverage`, data.py:210-219).
- `refuse_overlap(train, ev, role)`: any eval (or shift) text whose `nfc-trim-ws-v1` hash equals a train hash -> `REFUSED eval-train-overlap` (data.py:222-228). Normalization: NFC, trim, collapse every Unicode White_Space run to one U+0020 (data.py:44-47, 61-76).
- `group_train`: train rows grouped by normalized hash; a group whose copies carry different labels -> `REFUSED train-conflicting-labels` (data.py:263-273). **A duplicate text with ONE label is accepted** (it lands as several rows in one group).
- `calibration_split(rows, 0.25, 2, seed=13, k)`: draws whole GROUPS, stratified, seeded with `np.random.RandomState(13)`; per class `need = max(2, ceil(0.25 x n_class))`; refuses `train-class-too-small` when the slice would leave no fit row (data.py:283-307). The trainer draws this itself; the prepare script must not.
- train.py refuses before any model loads: `--epochs` missing or outside [4,12] above 16 shots/class (`REFUSED epochs`, contract.py:216-240 — verbatim: `"REFUSED epochs: above 16 shots/class (%d) --epochs is required, in [%d, %d]"`), `--seeds` other than 3 in production, a non-empty `--out` (`REFUSED out-dir`, train.py:762-763), `--base` in production (train.py:237-240).

**How `prepare_stance.py` is structured** [VERIFIED: scripts/laya_train/prepare_stance.py:1-27, 54-57, 114-178, 181-203]: module docstring declares the rule; `fail()` exits 1 for a pinned-dataset inconsistency, `DataError` -> one `REFUSED <rule>` line and exit 2; `build()` returns `{file name: bytes}` plus a counts-only summary; `task = {"type": "choice", "instructions": INSTRUCTIONS, "criteria": DESCRIPTIONS}` serialized `json.dumps(task, ensure_ascii=False, indent=2) + "\n"` (line 131-132); rows via `common.jsonl_bytes([(text, label_name)])`; `write_once(out, files)` refuses an existing non-empty dir unless byte-identical (ignores dotfiles, refuses any OTHER extra file such as a stale `shift.jsonl`, lines 161-178); prints `prepared <dir> (...)`, one `<name> <lines> lines sha256 <hex>` per file, `PREPARE OK`. It cross-checks `list(DESCRIPTIONS) == criteria_order` (line 121).

**Measured facts of the CSV** (python3 -I, csv module, counts only; this session) [VERIFIED: command output]:
- 11,311 rows; header `ID,Username,CreateDate,Biased,Keyword,Text`; `Biased` 0: 9,358 / 1: 1,953; IDs unique; no empty text; no label outside {0,1}.
- 10,644 distinct texts under nfc-trim-ws-v1 (667 duplicate rows). **5 texts carry both labels; they hold 13 rows** with label multisets `{0:1,1:1}`, `{0:1,1:1}`, `{0:1,1:2}`, `{0:2,1:2}`, `{0:1,1:1}` — two are exact ties, so a majority rule cannot resolve all five; **drop them** (the CONTEXT default) and record the count. After dropping: **8,771 not_antisemitic + 1,868 antisemitic = 10,639 usable distinct texts** (17.6 % positive).
- Character length p50 208, p99 341, max 968; word count p50 31, p99 55, max 97. (Token lengths: Q2.)

**What `prepare_goldstandard.py` must therefore do** (prescriptive):
1. Read the CSV with `csv.DictReader` (newline=""), normalize with `data.normalize`, group by `data.normalized_sha256`; keep the FIRST exact text of each group as the row text (store the exact string; the back office hashes it the same way).
2. Drop the 5 mixed-label groups (declared rule; count printed); map `Biased` `"0"` -> `not_antisemitic`, `"1"` -> `antisemitic`.
3. Stratified group-disjoint 20 % eval draw with `np.random.RandomState(SEED)` over groups per class (`ceil(0.2 x n_class)` groups per class -> about 1,754 + 374 = 2,128 eval texts; 8,511 train). Declare `SEED` (recommend 13, the contract's `declared_seed`, so the split seed and the calibration seed are the same documented number; the two draws are independent because the calibration split runs over the train file only). Because every group is a single distinct text after step 1, "group-disjoint" and "text-disjoint" coincide; keep the group machinery anyway so the script is correct if step 1 ever changes.
4. Assert what train.py will assert, before writing: no eval/train normalized overlap, both criteria present in eval, class counts, and `max(class_counts) > 16` (so the run needs `--epochs`).
5. Write `task.json` (criteria `not_antisemitic` first), `train.jsonl`, `eval.jsonl` through `write_once`; no `shift.jsonl` (D-04: later). Output dir `data/decide/goldstandard-2024` (under `/data/`, gitignored). Print counts and sha256s only; `PREPARE OK`.
6. Add a `--selftest` that runs the rule on a synthetic CSV (no tweet text) and asserts: duplicate collapse, the mixed-label drop, eval disjointness, stratification within one group per class, and `write_once` refusal — the `data.py --selftest` pattern.
7. A `just laya-prepare-goldstandard [csv]` recipe mirroring `laya-prepare-stance` (justfile:1305), defaulting the CSV path to the partner repo path; the script refuses a missing file with the `apr data`-style hint.

**Does any contract or demo assertion bind the gate to the stance demo?** No. `train.py:787-789` sets `f_avg_labels` only when `labels == demo.criteria_order`; otherwise every `f_avg` is null and the gate reads `macro_f1` only (gate contract `pass_rule`, laya-finetune-gate-v1.yaml:181). `contract.recipe_json` carries `shots_per_class` as an int with no cap (contract.py:275-297); the Rust verifier recomputes it as the largest class count of train.jsonl and compares (verify.rs:1447-1464). `run_dir_files` / `expected_keys` are conditional on seeds and shift only (contract.py:154-200). `eval_set.generic_rule` (yaml:192) is the only eval requirement: held-out, every criterion present. **A production run on GoldStandard is contract-clean once `--epochs E` (4..12) is passed.**

### Q2. The task prefix budget

**How the row is built** [VERIFIED: crates/aprender-decide/src/laya/builder.rs:1-20, 165-204, 213-225; the Python original scripts/laya_train/.venv/.../laya/common.py:94-146; option rendering crates/aprender-decide/src/task.rs:234-243]:
- Row: `[CLS] "{t} question: {ins}" [SEP] [MASK] opt0 [MASK] opt1 [SEP] state [SEP]`, `t = "choice"`.
- Options render as `"{name}: {description}"` (or the bare name when the description is null): task.rs:234-243 verbatim: `.map_or_else(|| c.name.clone(), |d| format!("{}: {d}", c.name))`. Each option is `[MASK]` + **the first 48 tokens** of `" " + option` (`OPTION_MAX_TOKENS: usize = 48`, builder.rs:27; Python `max_length=48`, common.py:124). **A criterion description beyond 48 tokens is cut silently** at the tokenizer.
- Head budget = `head_max_len (192) - sum(option tokens)`; the shrink (`max(4, (192-16)/K)` per option) only triggers when the budget drops under 16, impossible with two options (max 98 option tokens -> budget 94); the head keeps `max(8, budget)` tokens of `"choice question: {instructions}"` — **instructions beyond the budget are cut silently** (`head_ids.into_iter().take(keep)`, builder.rs:196).
- `finish`: `room = max_len - len(prefix) - 1`; state right-truncated to `room`; `truncated` is set only when the STATE did not fit (builder.rs:216-219). So the built row length is exactly `P + t` where `P` = the empty-state row (prefix + closing `[SEP]`) and `t` = the state's tokens, capped at 512.

**Measured this session with the served tokenizer** (`AutoTokenizer` from the snapshot's `tokenizer/`, `laya.common.build_sequence(tok, "", q, 512, 192)`; run under `uv run --project scripts/laya_train --frozen python -I`; no model loaded) [VERIFIED: command output]:

| Task | P (empty-state row) | markers |
|---|---|---|
| Served stance task (`data/decide/tweet-stance-64/task.json`) | **57** — equals the contract's `served_task_min_row_tokens: 57` (decide-tool-boundary-v1.yaml:306), so the method reproduces the contract's own measurement | 3 |
| Two bare labels, instructions "Is this tweet antisemitic?" | **24** | 2 |
| Maximal: 400-word instructions + two 400-word descriptions | **196** (the 195+1 bound of facts T2-04/T2-05 confirmed) | 2 |
| Name-only options + 400-word instructions | 196 (the head absorbs the whole 192 budget) | 2 |

GoldStandard texts in Laya tokens (n = 11,311, `encode_text(..., add_special_tokens=False)` after the mask-token replace): **min 2, p50 56, p75 73, p90 86, p95 94, p99 130, max 366, mean 57.6.**

**The arithmetic that follows** (door: 8 texts, 800 built tokens, `classify_max_texts: 8`, `classify_max_total_tokens: 800`, decide-tool-boundary-v1.yaml:288, 297):

| P | Share of tweets with `P + t <= 100` (so 8 such tweets fit one call) | Texts per 800-token call at the p50 tweet | at the p95 tweet |
|---|---|---|---|
| 24 (bare) | 0.78 (derived from the table: t <= 76) | 10 (capped at 8) | 6 |
| 30 | 0.713 | 9 -> 8 | 6 |
| **40** | 0.561 | **8** | 5 |
| 50 | 0.427 | 7 | 5 |
| 60 | 0.286 | 6 | 5 |
| 80 | 0.047 | 5 | 4 |
| **100** | 0.000 | 5 | 4 |
| 120 | 0.000 | 4 | 3 |

Three consequences the planner must put in front of the owner at the D-06 checkpoint:
1. **Hard ceiling P <= 100.** `check_served_task_fits` (crates/aprender-mcp-decide/src/lib.rs:751-766) refuses to BUILD the server when `min_row x max_texts > max_total_tokens`; a P of 101 means every Lambda cold start fails (503) and `laya-deploy`'s identity probe contains the deployment. The same bound is asserted by `tests::max_texts_fit_the_budget_at_the_shortest_row` (tests.rs:188-200) against the CONTRACT constant, and `probe --plan-only --maximal distributed` refuses to plan (probe.rs:507-517). Also max row = P + 366 must stay <= 512 for the longest tweet to escape `truncated`: P <= 146 (satisfied by the ceiling).
2. **D-05's "eight tweets fit" means P <= 44 for the MEDIAN tweet, and is unattainable at the p95 tweet for any P.** This is a property of the data (tweets average 58 tokens), not of the guide. The honest hand-off statement is "texts per call = floor(800 / (P + t)); experiment-coach budgets by the returned `tokens` field" (the reply's R1 proposal), with the measured P and the tweet percentiles beside it.
3. **The two criterion descriptions are capped at 48 tokens each regardless of P** — the guide's substance has to live in `instructions`; descriptions are one-line definitions.

Recommended budget to propose at the checkpoint: **P <= 60** (a ~35-token instruction sentence plus two <= 10-token descriptions), presented with the row of the table above and the alternative P <= 44; the owner picks. Record the chosen budget, the measured P, and the per-call expectation in the run record and the hand-off (D-05).

**How to measure P for a candidate `task.json` before training.** No existing probe does it without an `.apr` (`probe --plan-only` needs `--apr`). Add a `--measure` mode to `prepare_goldstandard.py` (or a sibling `measure_task.py`) that loads the snapshot tokenizer and calls `laya.common.build_sequence(tok, "", {"t": "choice", "ins": instructions, "crit": criteria}, 512, 192)`, prints `P`, the marker count (must equal K = 2), per-option token counts (flag any option at 48 = cut), the head's retained tokens vs the full instruction token count (flag a cut), and the train/eval row-length percentiles for the candidate. After packing, cross-check with `probe --plan-only --apr <new.apr> --maximal distributed` (plans 8 empty texts -> `tokens_planned` must equal `8 x P`; this is how the contract's 57 was measured, decide-tool-boundary-v1.yaml:302-305). The `transformers` import is already in the uv lock; this mode needs torch only transitively (no model forward).

**How the stance 57 was measured:** `probe --plan-only --maximal distributed` on `laya-stance-64.apr`: "8 empty texts plan to 456 tokens" (decide-tool-boundary-v1.yaml:302-305) [VERIFIED].

### Q3. The one-epoch probe (D-10)

**Can `train.py` run one epoch on one seed without a production-looking run dir?** Not in the production variant: `resolve_epochs` requires 4..12 above 16 shots/class (contract.py:230-239) and `resolve_seeds` requires exactly 3 (README.md:149: "production `--seeds` other than 3" refused). The synthetic-fixture variant would accept `--seeds 1 --epochs 1 --base <snapshot dir> --base-sha256 <sha>` (contract.py:220-224; train.py:243-250) but writes a full run dir and the whole scoring tail (see Alternatives). **Use a separate driver** (Claude's discretion): `scripts/laya_train/probe_epoch.py`, modelled on spike 027's `run_cell.py` (`.planning/spikes/027-laya-calibration-slice-and-tcap/run_cell.py:46, 199-204`, which imports `train` and marks its output `NOT-A-GATE-RUN.txt`):
- Validate the data dir exactly as `train.main` does (data.py `load_task`, `load_rows`, `refuse_overlap`, `calibration_split` with the contract seed; `contract.check_numbers()`), so the probe measures the real fit rows (about 6,383 of 8,511 after the 25 % slice: per class `ceil(0.25 x 7017) = 1755` and `ceil(0.25 x 1494) = 374` -> 2,129 slice rows, 6,382 fit rows; derive, do not quote).
- Build `Base(SimpleNamespace(variant="production", base=None, base_sha256=None))` (train.py:233-245), `requested = train.request_device(None)`, then call `train.train_seed(13, base, requested, question, fit_rows, k, epochs=1, stopping="fixed_epochs")` (signature train.py:303). It returns `(agent, info)` with `info["train_seconds"]`, `info["steps"]`, `info["steps_planned"]`, `info["fit_rows"]`, `info["device_used"]` (train.py:390-395) and logs `DEVICE seed=13 requested=... used=... torch=...` and `TRAIN seed=13 fit_rows=... steps=.../... seconds=...` (train.py:314, 396-397). Per-step = `train_seconds / steps`.
- Also measure, from `agent._encode_state(t, ["q"], internal)[0]["ids"]` over the fit rows, the row-length max/mean/p95 and `P` (empty text) — the same tokenizer path train.py uses.
- Write ONLY `.planning/phases/10-.../10-PROBE-EVIDENCE.json` {device_used, torch, fit_rows, steps, train_seconds, s_per_step, row_tokens{p50,p95,max}, P, task_json_sha256, data sha256s, projected hours per E in 4..12 for 3 seeds}. No `--out`, no `recipe.json`, no checkpoint, nothing under `models/decide`.
- `fixed_epochs` so no calibration monitor runs; one epoch = `ceil(n_fit / 8)` steps (about 798). Expected wall: 798 x (1.2..3.0 s) = 16..40 min plus the base load [ASSUMED; the probe is the measurement].

**What `train.py` already logs per epoch/step** [VERIFIED: grep of `log(` sites]: no per-step line. Per epoch only under early stopping: `EPOCH seed=%d epoch=%d ce_last=... calib_nll_at_t*=... t*=... best_epoch=...` (train.py:375-377). Per seed: `DEVICE` (314), `STOP` (387-388), `TRAIN ... seconds=%.1f` (396-397), `CHECKPOINT COMPLETE` (666), `SCORING START` (668), `CALIBRATION` (671), `SEED` (845), then `MEDIAN` (848), `DELETED` (855), `VARIANCE` (867), `NOISE` (598), `WARN pack will refuse` (601), `SHIFT PROBE` (639), `RESULT` (917), `SEEDS ... train_seconds=%s total_seconds=%.1f` (924-926), `GATE PASS`/`GATE FAIL` (928/930). `train_seconds` on the SEEDS line is ONE seed's figure (the shipped seed's `info`), not the sum (facts T2-22).

**What the 08-16 run measured** [VERIFIED: 08-16-SUMMARY.md:105-128; 08-GATE-RUN-EVIDENCE.json `wall_seconds` 684, `device_used` "mps:0", `torch_version` "2.14.0"]: 684 s wall for three seeds at 192 training rows (144 fit rows, 18 steps/epoch), `epochs_run` 4/5/5 (patience stop, best epochs 1/2/2), `train_seconds=104.5` for the shipped seed 17 (5 epochs = 90 steps -> 1.16 s/step; facts T2-23), device `mps:0`. Rows there were <= ~110 tokens; GoldStandard rows will be P + up to 366 (p95 ~ P + 94), so the per-step time is expected higher. Estimate only.

### Q4. The gate run at this dataset size

**Steps and time** (formula from train.py:333: `steps = epochs x ceil(len(items) / 8)`): about 798 steps per epoch; `hours = s_per_step x 798 x E x 3 / 3600` for the training part alone -> at 1.5 s/step: E=4 3.99 h, E=6 5.99 h, E=12 11.97 h [COMPUTED from a hypothetical s/step; the probe replaces it]. Under `early_stopping` (`first_candidate_epoch: 1`, `patience_epochs: 3`, `min_delta: 0.001`, `eval_every_epochs: 1`, laya-finetune-gate-v1.yaml:241-248 [VERIFIED]) the earliest stop is epoch 4; with `--epochs 4` every seed runs all 4 (the stop rule is `e - best_epoch >= 3 or e == max`). **Recommendation:** declare `--epochs 4` unless the probe's per-step time is <= 1.0 s (then 6); 798 steps per epoch is 44x the demo's 18, so the demo's best-epoch 1-2 pattern argues against spending 12 epochs on a first production run. Record the reasoning in the run record (D-08 says the probe sets E).

**Per-epoch monitor cost** under early stopping: `calibration_logits` runs a forward over the calibration slice (~2,129 rows, 267 batches) on the training device each epoch (train.py:268-281) plus a bounded T fit — a minute or two per epoch [ASSUMED].

**Memory**: `collate` pads each batch to its longest row (train.py:216-228), so activation memory follows the longest row in the batch (<= 512 tokens) rather than the dataset; the items list holds token-id lists only. The early-stopping `best_state` is a CPU copy of the full model (train.py:372) — about 1.7 GB per seed in fp32 (421 M params) [ASSUMED]. The 08-16 run fit on this host at <= 110-token rows; at <= 512-token rows the attention/activation footprint grows ~4-5x for batch 8 [ASSUMED]. No max-rows cap exists anywhere in data.py/train.py/contract.py (grep this session).

**The scoring tail** at 2,128 eval rows: per seed the eval scores once on CPU fp32 (the F16 reload), the zero-shot scores once, and the A1 noise record re-scores every eval row in float64 for the shipped checkpoint AND the base (`write_noise_record`, train.py ~step 9; README.md:94-101). The skill measured the f64 record at "+40-70 s per 280 rows" (laya-calibration-gate-and-pack-tolerance.md, Proposed bars A) -> 5-9 min per model at 2,128 rows, 10-18 min for both [COMPUTED from a measured rate on a different row length; ASSUMED transfer]. With three fp32 eval passes and reloads, budget ~45-60 min for the tail [ASSUMED]. Pack re-scores every eval row from the packed bytes and from the base: 08-16 pack 232 s and verify 214 s at 459 rows -> ~18 min each at 2,128 rows [COMPUTED, ASSUMED linear]. `laya-upload` and `laya-deploy` each run `laya-verify` again (justfile:2402-2416, 1842-1856), and `laya-deploy-selftest`'s positive leg too: budget four verifies.

**Run-dir layout** [VERIFIED: README.md:111-116, 118-133; train.py:815-855]: `<out>/recipe.json` first, `seeds/seed-13|17|23/{checkpoint/, eval-probs.json}`, then the median seed's checkpoint moves to `<out>/checkpoint/` (`os.replace`, train.py:851) and the other two checkpoints are deleted (`DELETED seed=%d checkpoint`); `eval-probs.json`, `zero-shot-probs.json`, `probes.json`, `rescore-noise.json`, `variance-report.json`, `gate-report.json`, `task.json`. Recommended paths: data `data/decide/goldstandard-2024`, run `models/decide/laya-antisemitism-v1`, artifact `models/decide/laya-antisemitism-v1.apr`. A run dir is written once (`REFUSED out-dir`); a re-run needs a new path (suffix `-run2`), and the evidence must name which run is THE declared run (D-09).

**Gate fail** [VERIFIED: train.py:93 `EXIT_PASS, EXIT_REFUSED, EXIT_GATE_FAIL = 0, 2, 3`; 927-931]: the run dir is complete (gate-report.json with `pass: false`), `GATE FAIL` is logged, exit 3; `just laya-train` propagates it. The plan's task must capture `rc` from a file, not a pipe, and on 3 stop at a `checkpoint:decision` for the owner (D-09), as 08-16 Task 2 did. Pack/verify refuse a failed report with exit 3 (`GateFailed`, pack_laya.rs:34-36).

**Data-change guard**: train.py re-hashes the data dir before writing the report and refuses `data-changed` (exit 2) if any file changed during the run (train.py:283-291); do not touch the data dir while training.

### Q5. Pack and verify

**Command lines** [VERIFIED: justfile:1435-1452; pack_laya.rs:3-8]:
```
just laya-pack   <run> <data> <base> <out.apr>      # cargo run --release -p aprender-decide --example pack_laya -- pack --run .. --data .. --base .. --out ..
just laya-verify <apr> <run> <data> <base>          # ... verify FILE --run .. --data .. --base ..   (the ONLY eligibility check)
cargo run --release -p aprender-decide --example pack_laya -- inspect <apr>   # identity only: sha256, recipe_id, method, base, variant, labels, embedded_gate
```
`<base>` = `~/.cache/huggingface/hub/models--convaiinnovations--laya/snapshots/55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851` (present).

**What pack checks before writing** [VERIFIED: pack_laya.rs:11-24]: production variant, the contract recipe block, the contract base (identity fields and the base dir's files against the pins), input hashes, the split, both probability files, a Rust re-score of every eval row from the packed bytes and from the base, the gate recomputed from the verified probabilities, the median seed re-derived from the per-seed files; a recipe without `seed_selection` -> `SeedPolicyMissing`. Refusal: one line `REFUSED <Variant> <detail> (nothing written)`; exit 3 if the recomputed gate failed, 2 otherwise. Success prints `PACKED <path> sha256=<H> rescore_max_abs=.. zs_rescore_max_abs=.. rescore_bound=.. zs_rescore_bound=.. noise=.. zs_noise=.. shipped_seed=<s> argmax=<n>/<n>`.

**Re-score bar** [VERIFIED: contracts/laya-parity-v1.yaml:98, 104, 179-180]: `bound = max(1e-5, pack_rescore_noise_k (4) x noise)` per checkpoint, ceiling `pack_rescore_bound_max_abs: 1.0e-3`, AND `argmax(p_rust) == argmax(p_torch32)` for every eval row. At 2,128 rows the max of the noise tail grows (the skill: "a larger eval set makes the bar harder") but the bound scales with the measured noise; the exact-argmax clause does not scale — a near-tie row (|p0 - p1| below ~1e-5) can flip between Rust and torch and REFUSE the pack (Pitfall 4). The trainer prints `WARN pack will refuse` when it already sees a bound over the ceiling or an argmax disagreement in its own f64 record (train.py:601).

**`verify` output** [VERIFIED: pack_laya.rs:181-205]: one JSON line `{artifact_sha256, deploy_eligible, recomputed{zs_macro_f1, ft_macro_f1, ece_post, margin}, rescore_max_abs, zs_rescore_max_abs, rescore_bound, zs_rescore_bound, noise, zs_noise, shipped_seed, argmax}` — this is the deploy_eligibility record (decide-apr-v1) the hand-off should quote.

**What the hand-off needs from this step** (D-17): `artifact_sha256` (verify line, equals `shasum -a 256` of the file), `labels` in task order (`inspect`), `recipe_id` (`inspect`; the sha256 of recipe.json), `base` (`laya-en-root@55cf4c4e`), the embedded gate summary, the `inputs_sha256.task_json` (gate-report.json) and the condensed text. 08-GATE-RUN-EVIDENCE.json is the record shape to copy (`artifact{path, sha256, bytes}`, `pack{...}`, `deploy_eligible`, `noise{...}`, `seeds.per_seed[]`, `device_used`, `wall_seconds`).

### Q6. The SetFit comparison cell (D-11)

**Which binary runs a cell and what it needs** [VERIFIED: scripts/run_bench_cells.sh:4-10, 213-214, 327-331]: `"$APR" data select --data DATA_DIR --shots 64 --seed 13 --output <dir>` then `"$APR" setfit bench run --method setfit --shots 64 --seed 13 --data DATA_DIR --selection <dir>/selection-manifest.json --bench-dir BENCH --model-dir <MiniLM checkout>`. `apr` must be built with `--features setfit` (apr-cli `setfit = ["training", "aprender/setfit", ...]`, Cargo.toml:132; the setfit commands are `#[cfg(feature = "setfit")]`, lib.rs:60, commands/mod.rs:199-201) and pinned through `scripts/apr_bin.sh`.

**The data door** [VERIFIED: crates/apr-cli/src/commands/data_contrastive.rs:284-300, 242-256; data_tweeteval.rs:667-700]: `read_attested_canonical(dir)` reads `benchmark-manifest.json`, requires a numeric `schema_version` in the crate's supported set and a `dataset_attestation` object, then reads `{role}.jsonl` for the roles the attestation declares (allowlist `train`, `validation`, `test`, `compatibility_test`). The canonical profile is three splits: `CanonicalSplits { train, validation, test }` (prepared.rs:71-75). Rows are `LabeledExample { id, input, label (usize), label_text (== label_names[label]), source_split }` with `deny_unknown_fields` (schema.rs:36-52). The dataset-building door is PUBLIC and label-count generic: `PreparedDataset::<Canonical>::from_labeled_rows(train, validation, test, &CanonicalDeclarations{train, validation, test, label_names}, &mut ledger)` (prepared.rs:267-285), `encode_jsonl()` (prepared.rs:410) and `DatasetAttestation::from_prepared(&dataset)` (attestation.rs:107). The only 3-class literals in the crate are test fixtures (`LABEL_TEXTS: [&str; 3]`, select.rs:766).

**What is TweetEval-bound** [VERIFIED]: `apr data tweet-eval-stance` (the only existing preparer) reads paired `*_text.txt/*_labels.txt` with `label_names()` = none/against/favor (data_tweeteval.rs:69-73, 410); `apr setfit bench run` reads `dataset_revision_from_manifest` (setfit_bench.rs:1131-1133), writes rows under `CLAIMS_CONTRACT_ID` (setfit-benchmark-claims-v1, the 40-cell matrix, bench_row.rs `is_contracted`), and its `QualityBlock` is `f_avg = (F1_against + F1_favor) / 2` ("the HEADLINE metric"), "Three-class macro F1", `[3][3]` confusion matrix (bench_row.rs:316-341). **A GoldStandard cell must not be written as a bench row**: it is outside the contracted matrix and the block cannot describe two classes.

**What is generic**: `apr data select` (run_select, data_contrastive.rs:495-540: shots and seed validated against the protocol — seed 13 and 64 shots are contracted values), `apr setfit train --config <toml> --data DIR --selection FILE --model-dir DIR --output FILE [--seed 13] [--dry-run]` (setfit_train.rs:629-700; the lifecycle `prepare -> tune_encoder -> fit_head -> verify_artifact` holds every gate incl. the calibration regime), and `apr eval setfit --artifact FILE --data DIR --selection FILE --split test` (eval/setfit.rs:79-101, 197-220; `macro_f1(truth, predicted, classes)` is K-generic, evaluate.rs:395, 439). The reference cell config is `epochs = 1`, `batch_size = 16` (setfit_train.rs:785-794 fixture), i.e. `s64e1b16` once the selection carries 64 shots.

**The regime check** [VERIFIED: setfit-train-lifecycle-v1.yaml:471-476; thresholds.rs:106-107]: `cell_label = "s" shots "e" epochs "b" batch`; JUDGE iff the run's arch fingerprint, seed and cell are inside a calibrated regime; the production regime is `minilm-slice-h384-l6-a12-i1536-v30522@1110a243|seeds=13,17,23,29,31,37,41,43,47,53|cells=s8e1b16,s16e1b16,s32e1b16,s64e1b16`. The regime id does NOT include the dataset, so `s64e1b16` at seed 13 on GoldStandard is judged under the production thresholds — which were MEASURED on TweetEval stance (yaml:620-629). The invariance argument there says the sparse-class ratios are vocabulary-size independent, "not sufficient on its own" to extend the set; a GoldStandard batch may still trip an evidence-threshold refusal. **That refusal is a result, not a bug**: record "SetFit lifecycle refused: <typed error>" and stop; widening a threshold inline is forbidden by that contract (D-10(c)) and D-11 funds no new regime.

**Plan shape (prescriptive):**
1. A thin Rust adapter, new, in `crates/apr-cli` as a `setfit`-gated example or a `scripts/`-driven `cargo run --example` under `aprender-contrastive-data`: read the Laya `train.jsonl` / `eval.jsonl` (`{text, label}`), carve a seeded, stratified, text-disjoint VALIDATION slice out of the Laya TRAIN rows (never from eval; recommend 10 % of train, seed 13), map `eval.jsonl` -> `test`, write `train/validation/test.jsonl` via `encode_jsonl` and a `benchmark-manifest.json` holding `schema_version` (the crate constant `DATASET_ATTESTATION_SCHEMA_VERSION`) and `dataset_attestation` (`DatasetAttestation::from_prepared`). One implementation per operation: the crate computes every hash, the adapter only maps rows. Output under `/data/decide/goldstandard-2024-setfit/`.
2. Tracer BEFORE the cell: `apr data select --shots 64 --seed 13` on the adapted dir, then `apr setfit train ... --dry-run` (setfit_train.rs:655-671) proves the attested boundary and the selection replay accept a two-label dataset without loading the encoder.
3. The cell: `apr setfit train` (CPU; the memory note: CUDA is refused by design; s64 was 29 min on stance for 3 x 64 shots; a 2 x 64 cell has the same `neg_cap = 3n^2` pair budget per the lifecycle's formula, yaml:244, so budget ~30 min [ASSUMED]), then `apr eval setfit --split test` on the artifact. Record macro-F1 and per-class F1 on the test split (= the Laya eval set, 2,128 rows). Top-label ECE: `apr eval setfit` output keys were not confirmed this session [ASSUMED it prints predictions/metrics, not ECE]; if absent, compute the house ECE from the eval's probabilities with `scripts/laya_train/metrics.py ece_top_label` (the same formula the Laya gate uses, README.md:54) so the two columns are comparable — flag this as a tracer check in the plan.
4. Recording under the Phase 5 claims discipline (the thing Phase 5 actually enforces is: every number names its cell, seed, selection semantic hash, dataset fingerprint, artifact sha and the command that produced it, and the claim scope is stated): `10-SETFIT-CELL-EVIDENCE.json` with `cell: s64e1b16`, `seed: 13`, `regime_id`, `selection_semantic_hash`, `dataset_fingerprint`, `artifact_sha256`, `test_rows: 2128`, `macro_f1`, `per_class_f1`, `ece_top_label (how computed)`, `wall_seconds`, `device: cpu`, `label: "same eval split, subsampled train (64 shots/class of 8,511); outside setfit-benchmark-claims-v1's 40-cell matrix; thresholds judged under the TweetEval-measured production regime"`, plus the Laya numbers beside it (shipped seed's `ft_macro_f1`, `ece_post`, `margin`).

### Q7. The redeploy through pmcp.run (D-12, D-13)

**The exact sequence plan 08-30 used** [VERIFIED: 08-LIVE-REDEPLOY-EVIDENCE.json `live_attempts[*].config/command/verify`; justfile recipes]:
1. `just laya-deploy-config <apr> off aprender-mcp-decide dev ze-kasher-dev` -> writes the gitignored `crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml` from the template with `name`, `APRENDER_DECIDE_S3_URI = s3://aprender-decide-weights-<account>-dev/decide/aprender-mcp-decide/<sha>.apr`, `APRENDER_DECIDE_SHA256`, the scoped `s3:GetObject` ARN, `[auth] enabled` (justfile:1577-1620). Needs `aws sts get-caller-identity --profile ze-kasher-dev` (one read call) unless `DRY_RUN=1`.
2. The S3 object must already exist: `just laya-upload <apr> <run> <data> <base>` (justfile:2384-2447) runs `laya-verify` FIRST (eligibility), then `aws s3 cp --sse AES256` to the content-addressed key, idempotent on size match. The bucket `aprender-decide-weights-<account>-dev` was created in 08-17 (`laya-weights-bucket`, justfile:2354). **The upload is a recipe, not manual.**
3. `just laya-deploy <apr> <run> <data> <base> aprender-mcp-decide dev ze-kasher-dev` (justfile:1796-1960): refusals before any AWS call — config present, no placeholder, sha pin == file and == key, IAM statement scoped, **resolver proof present and `cargo_pmcp_version` == installed**, eligibility (`laya-verify` again), S3 object size; then `touch main.rs`, `cargo pmcp deploy --manifest-path crates --regenerate-stack --no-post-deploy-test --no-oauth` inside `_laya-crates-root-swap` (cargo-pmcp compiles the bootstrap itself; the compile log must name only `aprender-mcp-decide-lambda`), writes `models/decide/deploy-aprender-mcp-decide.state/deployment.toml` (endpoint), runs `just laya-grant` (read-only IAM readback), edge `/health` must name `serverId aprender-mcp-decide`, then the identity probe `probe --url <endpoint> --apr <apr> --expect-sha256 <H>` with up to 8 retries on "Server is in error state"; any identity failure -> `just laya-teardown` (reserved concurrency 0).
4. `just laya-deploy-verify <apr> aprender-mcp-decide ze-kasher-dev 2` (justfile:2599-2700): `probe --plan-only` both shapes, then per sample a `DECIDE_COLD_BUMP` env update (`aws lambda update-function-configuration`), a `--cold-first --maximal <shape>` probe, CloudWatch `decide.load performed_load=true probe_id=<uuid>` proof; then the identity probe and 5 warm calls; writes `models/decide/deploy-evidence-aprender-mcp-decide.json`.
5. `just laya-build-bootstrap` (justfile:1759-1783) builds the arm64 `bootstrap` with cargo-zigbuild and checks it; it is run by `laya-deploy`'s DRY_RUN branch and by `laya-deploy-selftest`; live, cargo-pmcp builds.

**Credentials**: steps 1-4 need the AWS profile `ze-kasher-dev` (listed locally) and a valid `cargo pmcp login` for the pmcp.run account owning the server (08-30-PLAN.md:22-25). None was exercised this session.

**How the server name is fixed**: `laya-deploy-config` and `laya-deploy` refuse any server other than `aprender-mcp-decide` (justfile:1591, 1812) because only that name resolves to the decide package from the shared crates root (`find_lambda_package_dir` prefers `<root>/<name>-lambda`, deploy.toml.template:7-21). **Deploying the new `.apr` under that name replaces the stance demo's artifact pin and binary in place** (same function, new `APRENDER_DECIDE_*` env and a fresh deploy): the endpoint `https://aprender-mcp-decide.us-east.true-mcp.com/mcp` is unchanged, the stance sha `24a44d7e...` stops being served, and experiment-coach's `DriftChecked` keys on the new sha.

**Blocker found: the resolver proof is stale.** `models/decide/resolver-proof.txt` records `cargo_pmcp_version=0.24.3`, `sdk_commit=e0561f8c...`, `proven_at=2026-09-27`; `cargo pmcp --version` is **0.28.1** (path install from `~/Development/mcp/sdk/rust-mcp-sdk`, whose `cargo-pmcp/Cargo.toml` is `version = "0.28.1"`, HEAD `d0348451`, `builder.rs` last changed `b65d354f` 2026-10-03). `laya-deploy` refuses: `"the proof is for cargo-pmcp 0.24.3, the installed tool is 0.28.1; re-run just laya-resolver-proof"` (justfile:1838-1840). Re-run `just laya-resolver-proof ~/Development/mcp/sdk/rust-mcp-sdk <commit whose cargo-pmcp/Cargo.toml says 0.28.1, e.g. d0348451> <scratch dir>` (justfile:1493-1560: archives the SDK at that commit, injects `scripts/laya_deploy/cargo_pmcp_resolver_proof.rs`, runs the resolver test against THIS workspace, writes the proof). The resolver code moved since 0.24.3 (a single-crate fallback, builder.rs:392-395), which is exactly why the proof re-executes it. Do this in Plan 5 before any live step; `DRY_RUN=1 just laya-deploy-selftest` then proves the chain offline (`LAYA_ELIGIBLE_APR/_RUN/_DATA/_BASE` point its positive leg at the NEW artifact, justfile:2007-2013).

**Contract and docs that change when the served task changes** [VERIFIED]: `contracts/decide-tool-boundary-v1.yaml` `served_task_min_row_tokens: 57` is "the shortest built row of the served stance task" (line 302-306) and `FALSIFY-DECIDE-TOOL-009.test_harness` names `models/decide/laya-stance-64.apr` (line 654). The new task's P must be re-measured (Q2) and both lines edited: a contract edit -> `pv validate`, `pv diff <old> <new>` for the semver bump, and `cargo test -p aprender-mcp-decide --lib tests::max_texts_fit_the_budget_at_the_shortest_row` must still hold (8 x P <= 800). `crates/aprender-mcp-decide-lambda/README.md` "Deployed" table (lines 76-88: task, model sha) and CLAUDE.md's decision-model row (lines 253-264: "a TweetEval abortion-stance model ... live on pmcp.run") describe the stance deployment; `scripts/laya_claims.tsv` rows 81 (`tool.FALSIFY-DECIDE-TOOL-009.harness`) and 111 (`claude-md.decide-row.tier`) pin those sentences, so `just laya-claims-check` must be re-run after the edits. Add the new evidence file paths to CLAUDE.md only once they exist (readme_contract).

**How 08-LIVE-REDEPLOY-EVIDENCE.json was produced**: by the plan's own `python3` evidence steps reading `models/decide/deploy-evidence-aprender-mcp-decide.json`, the probe logs and read-only `aws lambda get-function-configuration` / `get-function-concurrency` outputs, with account ids scrubbed (`auth_precheck: "... (account scrubbed)"`), accreting one `live_attempts[]` entry per deploy and a final `live` block with `cold_samples[]`, `cold_wall_ms`, `warm_ms`, `accepted_region_cold`, `final: true` (08-30-SUMMARY.md "Files Created/Modified"). Copy that schema as `10-LIVE-DEPLOY-EVIDENCE.json` (`schema = "laya-live-redeploy-v1"`, `server`, `artifact_sha256`, `offline_dry_run`, `decision` (D-13: replace / auth posture), `live_attempts[]`, `live{}`).

### Q8. Acceptance replay (D-16, D-17)

**What `examples/probe.rs` already checks** [VERIFIED: probe.rs:1-30, 120-260; src/probe.rs:89-116, 352-420]: `--apr` loads the local artifact through the same ladder and derives the expected sha and labels; `--expect-sha256` must agree with `--apr`; the identity probe runs `initialize -> tools/list -> tools/call classify` and reports `one_tool_named_classify`, `description_has_labels_in_order` (the description segment `The labels, in this order: [...]` equals the labels), `response_labels_match` (the `labels` array equals, same order), `identity_matches` (`model.artifact_sha256 == expected`), with `elapsed_ms`, `tokens_total`, `x-decide-load` header; `--maximal`/`--cold-first` add the sized request and `tokens_total == tokens_planned`. Exit non-zero on any false check. The bearer token is never printed.

**What is missing for the requester's tests** (request `docs/requests/aprender.md` "Tests we will run"): the probe never asserts (a) `results.len() == texts.len()`, (b) each `probabilities.len() == labels.len()`, (c) `sum(probabilities) ~ 1` within a tolerance, (d) `label == labels[argmax]`. The server's own tests assert (b) and (d) (tests.rs:1046-1057, facts T1-25) and sum-to-1 holds by `softmax(z / T)` construction in f32 (laya/mod.rs:441-458) but is asserted nowhere (facts T1-25 "PARTLY"). **Recommendation (one implementation):** add a `--replay <texts.json>` mode to `examples/probe.rs` (or a `replay` check inside the identity probe) that sends a batch of 2..8 texts (a fixed, non-tweet, committed sample set to keep tweet text out of the repo and logs) and reports `{one_result_per_text, probabilities_len_match, sums_within_1e-5, label_is_argmax, labels_in_task_order, identity_matches, elapsed_ms}` as one JSON line — then run it against the live endpoint and copy the line into `10-ACCEPTANCE-REPLAY.json`. Tolerance: f32 softmax over two classes sums to 1 within a few ULPs; declare `|sum - 1| <= 1e-5`.

**How the partner's client parses** [VERIFIED read-only: motivation-experiment `crates/core/src/classifier.rs:8-29, 110-135, 218-243, 335-350`]: `request_body` is a bare `tools/call` of `classify` with `{"texts": [...]}`; `parse_response` takes `result.structuredContent` if present and non-null, else parses `result.content[0].text` as JSON (pmcp 2.19.3 serves `content[0].text` only for this tool, facts T1-14/T1-15); it deserializes `ClassifyResponse { model{artifact_sha256, recipe_id, method, base}, labels, results[{label, probabilities: Vec<f32>, tokens, truncated}] }`; `prob_of(i, label)` looks probabilities up BY LABEL NAME (never by position); `DriftChecked` refuses any `model.artifact_sha256 != EC_CLASSIFIER_SHA`. Their unit fixture lists `labels: ["antisemitic","not_antisemitic"]` (reversed, classifier.rs:346) — harmless because they index by name, but the hand-off should state the served order `[not_antisemitic, antisemitic]` explicitly (D-02). Their live client signs SigV4 for a Function URL (classifier.rs:138-160); against pmcp.run it is a plain POST with `content-type: application/json` and `accept: application/json, text/event-stream` — the README's documented call (lambda README:90-104). The replay should send exactly those two headers so the hand-off shows THEIR request form passing.

### Q9. Threats and gates for the plans

**ASVS L1 items relevant here** (`security_enforcement: true`, level 1, `.planning/config.json`):
- V5 input/data handling: tweet text never committed, printed or logged — prepare prints counts and sha256s (prepare_stance.py:26); the server logs no request text (lambda README:56-57: "Request text is never logged"); the probe/replay sample set must be synthetic, not tweets; evidence JSONs carry hashes and counts only. The partner CSV is read in place (`python3 -I`), never copied into the repo.
- V6/identity: the artifact identity is the sha256 of the whole `.apr`, verified before a byte is parsed (lambda README:38-40); the pin is the deploy config's `APRENDER_DECIDE_SHA256`; the hand-off communicates ONE sha and the replay proves the live server returns it.
- Secrets: no credentials in records — scrub the AWS account id as 08-30 did (the bucket name embeds it; `deploy.toml` is gitignored for that reason, template:3-5); the probe bearer token (none under auth-off) is never printed; `cargo pmcp login` state stays local.
- Availability/cost (auth off, D-ITEM-08-18-A): every cold call buys ~25 s of 10 GB compute; containment is `just laya-teardown` (reserved concurrency 0); the replay should not loop cold samples beyond the recipe's 2 per shape.
- Supply chain: no new packages; the SDK checkout used for the resolver proof is pinned by commit in the proof file.

**Project gates the plans must run** (each with its failing direction):
- `cargo fmt --all -- --check` (rc != 0 fails) — touched crates: `aprender-mcp-decide-lambda` (probe replay), `apr-cli` or `aprender-contrastive-data` (SetFit adapter).
- `cargo clippy -p <crate> --lib -- -D warnings` for each touched crate, plus `--examples` for the example targets (`unwrap()` banned).
- `cargo test -p aprender-mcp-decide-lambda --lib` (47 passed at 08-30), `cargo test -p aprender-mcp-decide --lib` (incl. `tests::max_texts_fit_the_budget_at_the_shortest_row` against the edited constant), `cargo test -p aprender-decide --lib` (112 at 08-16).
- `pv validate contracts/decide-tool-boundary-v1.yaml` and `pv diff /tmp/old.yaml contracts/decide-tool-boundary-v1.yaml` (semver suggestion) via `cargo run --release -p aprender-contracts-cli --bin pv --` since `pv` is not on PATH; `make contract-audit-phase8` (08-30 ran it without the exemption).
- `bashrs lint` on any new `.sh` (none planned if the adapter is Rust and the drivers are Python; `just` recipe bodies are bash — `bashrs make lint`-style checks do not cover justfile; keep recipes to the existing idioms: `set -euo pipefail`, `|| rc=$?`).
- `scripts/check_include_files.sh` only if a new `include!()` file is added (none expected).
- `cargo test -p aprender-core --test readme_contract` if CLAUDE.md or README paths change; `just laya-claims-check` after any CLAUDE.md/README/contract-harness sentence change (rows 81, 111).
- Back office: `just laya-train-selftest` (torch-free self-tests + lifecycle), `uv run ... python scripts/laya_train/data.py --selftest`, the new `prepare_goldstandard.py --selftest`.
- Deploy: `DRY_RUN=1 just laya-deploy-selftest` (offline; ends `DEPLOY SELFTEST OK` with the positive leg armed on the new artifact), `just laya-gates-selftest` (33-row gate table, `scripts/laya_gates.tsv`).

## Architecture Patterns

### System Architecture Diagram

```
partner CSV (read-only, 11,311 rows)
   |  prepare_goldstandard.py: normalize -> group -> drop 5 mixed groups -> stratified 20 % eval draw
   v
data/decide/goldstandard-2024/{task.json (condensed guide), train.jsonl 8,511, eval.jsonl 2,128}
   |                                   \
   |  measure (tokenizer only): P, row percentiles --> owner checkpoint D-06 (P <= 100 hard; P <= 44 "8 median tweets")
   |  probe_epoch.py: 1 epoch, seed 13, fixed_epochs --> 10-PROBE-EVIDENCE.json (s/step, device) --> --epochs E
   v
just laya-train data run --epochs E  (3 seeds on MPS, early stopping, median-ECE ships, f64 noise record)
   |-- exit 3 (GATE FAIL) --> owner checkpoint (D-09) ---------------------------------+
   v exit 0                                                                            |
just laya-pack run data base out.apr --> just laya-verify (deploy_eligible JSON line) --> 10-GATE-RUN-EVIDENCE.json
   |                                                                                   |
   |   SetFit branch: adapter -> attested canonical dir -> apr data select s64 seed13 -> apr setfit train (CPU)
   |                  -> apr eval setfit --split test --> 10-SETFIT-CELL-EVIDENCE.json ("same eval split, subsampled train")
   v
re-prove resolver (cargo-pmcp 0.28.1) -> DRY_RUN laya-deploy-selftest -> edit decide-tool-boundary-v1 (served_task_min_row_tokens, 009 harness)
   -> laya-deploy-config <apr> off -> laya-upload -> owner checkpoint (D-13: replace, auth off) -> laya-deploy -> laya-deploy-verify
   v
probe --expect-sha256 + replay (one result per text, sums ~ 1, labels order, sha) -> 10-ACCEPTANCE-REPLAY.json
   v
10-HANDOFF.md: sha, labels, endpoint, auth, task.json text + sha, gate summary, SetFit cell, replay, timings (reported)
```

### Recommended file layout
```
scripts/laya_train/prepare_goldstandard.py     # new: the declared split rule, --selftest, --measure (P and row stats)
scripts/laya_train/probe_epoch.py              # new: D-10 driver; imports train.train_seed; writes no run dir
crates/apr-cli/examples/ (setfit-gated) or crates/aprender-contrastive-data/examples/
                                               # new: laya_dir_to_canonical adapter (rows in, crate computes hashes)
crates/aprender-mcp-decide-lambda/examples/probe.rs   # modify: --replay mode (one result per text, sums, argmax)
contracts/decide-tool-boundary-v1.yaml         # modify: served_task_min_row_tokens, FALSIFY-DECIDE-TOOL-009 harness
justfile                                       # modify: laya-prepare-goldstandard, laya-probe-epoch, laya-setfit-cell
data/decide/goldstandard-2024/                 # gitignored output; models/decide/laya-antisemitism-v1{,.apr}
.planning/phases/10-.../10-{PROBE,GATE-RUN,SETFIT-CELL,LIVE-DEPLOY}-EVIDENCE.json, 10-ACCEPTANCE-REPLAY.json, 10-HANDOFF.md
```

### Pattern 1: declare the rule, then run once
**What:** Every data rule (conflict drop, split seed, budget) is written in the script's docstring and the contract/record BEFORE the run; the run dir is written once; the artifact's identity binds `task.json` bytes. **When:** Plans 1-3. **Source:** Phase 8 D-07 / `write_once` (prepare_stance.py:161-178), `REFUSED out-dir` (train.py:762).

### Pattern 2: measure with the served path, control against a known value
**What:** The tokenizer measurement reproduced the contract's 57 before being trusted for the new task; the probe's per-step figure comes from `train.train_seed` itself. **Source:** this session's measurement; Verification Discipline 2.

### Pattern 3: refusal is a result
**What:** A gate fail (exit 3), a pack refusal (`argmax`/bound), or a SetFit `UncalibratedRegime`/evidence refusal is recorded verbatim and stops the plan at an owner checkpoint; no threshold is moved inline. **Source:** D-09; setfit-train-lifecycle-v1 D-10(c) prose (yaml:620-629); laya-parity-v1 A1.

### Anti-Patterns to Avoid
- **Writing the GoldStandard SetFit cell as a `setfit-benchmark-claims-v1` bench row** — the row schema is three-class and the contract's matrix is TweetEval's.
- **Drawing the validation slice for SetFit from the Laya eval set** — the "same eval split" claim dies.
- **A `task.json` whose descriptions carry the guide** — anything past 48 tokens per option is cut silently.
- **Reporting `train_seconds` as the three-seed total** — it is one seed's.
- **Running the probe through the production `train.py`** — impossible (`--epochs` >= 4, `--seeds` 3) and a synthetic-fixture run leaves a run dir.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Text normalization / overlap / split | a second normalizer in the prepare script | `data.normalize`, `data.normalized_sha256`, `data.group_train` | Rust `split_whitespace` set, not Python `str.split` (data.py:16-18, 44-47) |
| Prefix/row token counts | a token estimate from words | `laya.common.build_sequence(tok, "", q, 512, 192)` and `encode_text` | reproduces the contract's 57 exactly |
| Dataset attestation for SetFit | hand-written `benchmark-manifest.json` hashes | `PreparedDataset::<Canonical>::from_labeled_rows` + `DatasetAttestation::from_prepared` + `encode_jsonl` | the fingerprint/exclusion/digest rules live in the crate |
| Eligibility | `inspect` or a sha check | `just laya-verify` (decide-apr-v1 deploy_eligibility) | the ONLY eligibility check (justfile:1440-1444) |
| Cold-sample proof | timing a curl | `just laya-deploy-verify` | CloudWatch `performed_load=true probe_id` proof per sample |
| Top-label ECE for the SetFit column | a new ECE | `scripts/laya_train/metrics.py ece_top_label` (= `aprender::calibration::expected_calibration_error_top_label`) | the house formula with 15 bins |

## Runtime State Inventory

Not a rename phase, but the redeploy REPLACES live state, so the categories are answered:

| Category | Items Found | Action Required |
|----------|-------------|-----------------|
| Stored data | S3 `decide/aprender-mcp-decide/24a44d7e....apr` (stance) stays; the new key is content-addressed beside it | none (the old object is inert; optional cleanup is the owner's) |
| Live service config | the pmcp.run function `aprender-mcp-decide` env `APRENDER_DECIDE_S3_URI/SHA256` (stance pin), `DECIDE_COLD_BUMP` | rewritten by `laya-deploy`; readback in the evidence |
| OS-registered state | none (verified: no launchd/pm2 in the recipes) | none |
| Secrets/env vars | AWS profile `ze-kasher-dev`, `cargo pmcp login` token | unchanged; not recorded |
| Build artifacts | `models/decide/resolver-proof.txt` (STALE: 0.24.3 vs 0.28.1), `.pmcp/deploy.toml` (stance), `models/decide/plan-*.json`, `deploy-evidence-*.json` | re-prove; regenerate config; overwritten by the recipes |

## Common Pitfalls

### Pitfall 1: the prefix overflows the door
**What goes wrong:** P > 100 builds fine locally, packs, verifies — and the server refuses to build on Lambda (`check_served_task_fits`), every call 503, `laya-deploy` contains the function. **Avoid:** measure P before training (Q2 mode), assert `8 x P <= 800` in the prepare script's `--measure`, and re-check with `probe --plan-only --maximal distributed` on the packed `.apr` before `laya-deploy-config`. **Warning sign:** `tokens_planned != 8 x P`, or `plan-only` refusing.

### Pitfall 2: the guide lives in the descriptions
**What goes wrong:** descriptions longer than 48 tokens are silently cut; the artifact identity still hashes the full text, so the served task is not what the owner approved. **Avoid:** `--measure` flags any option at exactly 48 tokens and any instruction cut; keep descriptions one line.

### Pitfall 3: reading one seed, or the wrong seconds
**What goes wrong:** `train_seconds` is one seed's; the median is selected with eval labels; a single MPS run near the bar is a coin flip (skill, 027). **Avoid:** report all three `SEED` lines and the `MEDIAN` line; the wall time is `total_seconds`.

### Pitfall 4: pack refuses on an argmax tie
**What goes wrong:** at 2,128 eval rows a near-tie row flips argmax between Rust and torch32; `pack` refuses (laya-parity-v1 `argmax_exact` on real rows, yaml:180) though the gate passed. **Avoid:** read the trainer's `NOISE ... argmax=n/n` and any `WARN pack will refuse` line BEFORE packing; if it fires, the record is the finding and the owner decides (the bar is a contract; do not edit it inline).

### Pitfall 5: the stale resolver proof
**What goes wrong:** `laya-deploy` refuses at check (4) after an hour of pack/verify/upload. **Avoid:** re-prove first (Q7); `DRY_RUN=1 just laya-deploy-selftest` exercises the proof path offline.

### Pitfall 6: the SetFit evidence gate refuses GoldStandard
**What goes wrong:** the production thresholds were measured on TweetEval; a GoldStandard batch may trip an evidence-threshold refusal after ~30 min CPU. **Avoid:** `--dry-run` tracer first; treat a refusal as the recorded comparison outcome.

### Pitfall 7: `pv` is not on PATH
**What goes wrong:** a plan step `pv validate ...` fails as command-not-found and a `|| true` hides it. **Avoid:** `cargo run --release -p aprender-contracts-cli --bin pv -- validate ...`, rc captured.

### Pitfall 8: the partner's fixture order
**What goes wrong:** their unit fixture lists `["antisemitic","not_antisemitic"]`; a reader assumes that is the served order. **Avoid:** the hand-off states `labels = ["not_antisemitic", "antisemitic"]` and that their parser indexes by name.

## Code Examples

### Measuring P and the row distribution (the method that reproduced 57)
```python
# scripts/laya_train/prepare_goldstandard.py --measure  (run: uv run --project scripts/laya_train --frozen python -I ...)
from transformers import AutoTokenizer
from laya.common import build_sequence, encode_text
tok = AutoTokenizer.from_pretrained(str(SNAPSHOT / "tokenizer"))           # the served tokenizer.json
q = {"t": "choice", "ins": task["instructions"], "crit": dict(task["criteria"])}
ids, markers = build_sequence(tok, "", q, 512, 192)                        # empty state -> P
assert len(markers) == len(task["criteria"])                                # one marker per criterion
P = len(ids)                                                                # 57 for the stance task
t = len(encode_text(tok, text.replace(tok.mask_token, " "), add_special_tokens=False)["input_ids"])
# built row = P + t (capped 512); 8 texts fit iff sum(P + t_i) <= 800
```

### The probe driver's core (no run dir)
```python
# scripts/laya_train/probe_epoch.py
import contract, data, train
from types import SimpleNamespace
task = data.load_task(d / "task.json"); rows = data.load_rows(d / "train.jsonl", task, "train")
ev = data.load_rows(d / "eval.jsonl", task, "eval"); data.refuse_overlap(rows, ev)
fit_ids, calib_ids, _, _ = data.calibration_split(rows, 0.25, 2, contract.declared_seed(), len(task["labels"]))
base = train.Base(SimpleNamespace(variant="production", base=None, base_sha256=None))
agent, info = train.train_seed(13, base, train.request_device(None), data.laya_question(task),
                               [rows[i] for i in fit_ids], len(task["labels"]), epochs=1, stopping="fixed_epochs")
s_per_step = info["train_seconds"] / info["steps"]        # info also carries device_used, fit_rows, steps_planned
```

### The gate run and the tail (commands of record)
```bash
just laya-train data/decide/goldstandard-2024 models/decide/laya-antisemitism-v1 --epochs 4 > /tmp/p10-train.log 2>&1; rc=$?   # 0 pass, 3 fail, 2 refused
BASE=~/.cache/huggingface/hub/models--convaiinnovations--laya/snapshots/55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851
just laya-pack   models/decide/laya-antisemitism-v1 data/decide/goldstandard-2024 "$BASE" models/decide/laya-antisemitism-v1.apr
just laya-verify models/decide/laya-antisemitism-v1.apr models/decide/laya-antisemitism-v1 data/decide/goldstandard-2024 "$BASE"
cargo run --release -p aprender-decide --example pack_laya -- inspect models/decide/laya-antisemitism-v1.apr   # labels, recipe_id, base
cargo run --release -p aprender-mcp-decide-lambda --example probe -- --plan-only --apr models/decide/laya-antisemitism-v1.apr --maximal distributed  # tokens_planned == 8 x P
```

### The redeploy (plan 08-30's commands, new artifact)
```bash
just laya-resolver-proof ~/Development/mcp/sdk/rust-mcp-sdk d0348451 /tmp/p10-proof      # cargo-pmcp 0.28.1
DRY_RUN=1 LAYA_ELIGIBLE_APR=... LAYA_ELIGIBLE_RUN=... LAYA_ELIGIBLE_DATA=... LAYA_ELIGIBLE_BASE="$BASE" just laya-deploy-selftest
just laya-deploy-config models/decide/laya-antisemitism-v1.apr off aprender-mcp-decide dev ze-kasher-dev
just laya-upload  models/decide/laya-antisemitism-v1.apr models/decide/laya-antisemitism-v1 data/decide/goldstandard-2024 "$BASE"
# owner checkpoint (D-13), then:
just laya-deploy  models/decide/laya-antisemitism-v1.apr models/decide/laya-antisemitism-v1 data/decide/goldstandard-2024 "$BASE" aprender-mcp-decide dev ze-kasher-dev
just laya-deploy-verify models/decide/laya-antisemitism-v1.apr aprender-mcp-decide ze-kasher-dev 2
cargo run --release -p aprender-mcp-decide-lambda --example probe -- --url https://aprender-mcp-decide.us-east.true-mcp.com/mcp --apr models/decide/laya-antisemitism-v1.apr --expect-sha256 <H>
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| 3,008 MB tier, 2 texts / 120 tokens | 10,240 MB, 8 texts / 800 tokens (decide-tool-boundary-v1 9.0.0) | 2026-09-28 (08-30) | the door this phase inherits |
| Single-seed gate | median-ECE of 13/17/23 (1.4.0 A3) | 2026-09-27 | every production run trains 3 seeds |
| 1e-5 fixed re-score bar | noise-referenced `max(1e-5, 4 x noise)` + exact argmax (A1) | 2026-09-27 | pack cost includes the f64 record |
| cargo-pmcp 0.24.3 (proof) | 0.28.1 installed | 2026-10-03 | re-prove before deploying |

**Deprecated/outdated:** the reply's B5 Lambda-direct IAM Function URL recipe (dropped by D-12); the requester's 11-12 s cold / 0.5-0.75 s warm figures (spike 026 single-text numbers; production measured 22.6-27.0 s cold, 0.87-0.98 s warm, 08-LIVE-REDEPLOY-EVIDENCE `live`).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Per-step time on GoldStandard rows is 1.2-3.0 s on `mps:0` (08-16 was 1.16-1.45 s at <= 110-token rows) | Q3/Q4 | the date (D-10 re-prices at > 3.0 s); the probe replaces it |
| A2 | The scoring tail at 2,128 rows is ~45-60 min and pack/verify ~18 min each (linear scaling from 459-row measurements) | Q4/Q5 | plan wall-time budgets; no correctness impact |
| A3 | MPS memory fits batch 8 at <= 512-token rows with AdamW states and a CPU `best_state` copy | Q4 | an OOM on the first epoch; the probe exposes it |
| A4 | A 2-class s64e1b16 cell costs about the stance cell's 29 min CPU (same `neg_cap = 3n^2` pair budget) | Q6 | SetFit wall time only |
| A5 | `apr eval setfit --split test` prints macro-F1 (K-generic) but not top-label ECE; the house ECE is computed from its probabilities | Q6 | if it prints neither probabilities nor ECE, the SetFit ECE column needs `evaluate_rows_from_artifact` through a small example |
| A6 | The production SetFit evidence thresholds (measured on TweetEval) accept a GoldStandard batch | Q6 | the cell is refused; recorded as the outcome (no inline widening) |
| A7 | A near-tie argmax flip at pack is a real risk at 2,128 rows (not observed at 459) | Pitfall 4 | pack refusal -> owner decision |
| A8 | `cargo pmcp login` for the pmcp.run account is still valid and the SDK commit `d0348451` builds the resolver test | Q7 | the live deploy waits on a human-action checkpoint |
| A9 | The arXiv 2304.14599 text is the guide the owner means (D-06 says so; the paper was not opened this session) | Q2 | the owner's checkpoint reviews the exact text anyway |

## Open Questions

1. **Which P does the owner accept?** Known: 24 <= P <= 100 (bare question .. hard ceiling); 8 median tweets fit at P <= 44; at P = 60 about 6 texts per call at the median tweet. Unclear: the owner's trade between guide length and texts per call. Recommendation: Plan 1's blocking checkpoint presents the table and a 60-token draft and a 44-token draft; the measured P of the approved text goes into the run record.
2. **`--epochs`**: known: [4, 12]; recommendation 4 unless the probe measures <= 1.0 s/step (then 6); recorded with the probe's numbers.
3. **SetFit ECE source** (A5): a tracer task reads `apr eval setfit --split test` output keys on the stance artifact `models/setfit-abortion-s17x8.apr` + `data/tweet-eval-stance` BEFORE the GoldStandard cell.
4. **D-13 at the deploy checkpoint**: replace (default) and auth off; the hand-off must say the stance demo is no longer served.
5. **The live 400-post labelled set** (D-04): not awaited; the hand-off names `shift.jsonl` as the slot for it.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| uv + `scripts/laya_train/.venv` (Python 3.13.7, torch 2.14.0) | prepare, probe, train | yes | uv 0.9.5 | — |
| Laya base snapshot `55cf4c4e...` | train, pack, verify, deploy | yes (`~/.cache/huggingface/hub/...`) | — | — |
| MPS device | train | expected (host unchanged since 08-16); read back by `DEVICE ... used=` | — | CPU (flagged; re-prices the date) |
| cargo / rustc | all Rust | yes | 1.93.0 | — |
| cargo-zigbuild | `laya-build-bootstrap` | yes | 0.23.4 | — |
| cargo-pmcp | `laya-deploy` | yes, but proof stale | 0.28.1 (proof 0.24.3) | re-prove (Q7) |
| SDK checkout for the proof | `laya-resolver-proof` | yes | `~/Development/mcp/sdk/rust-mcp-sdk` @ d0348451, cargo-pmcp 0.28.1 | — |
| aws-cli + profile `ze-kasher-dev` | config, upload, verify | cli 2.29.0; profile listed; credentials not exercised | — | human-action checkpoint |
| pmcp.run login | `cargo pmcp deploy` | not checked (no call) | — | human-action checkpoint |
| `pv` | contract edits | **not on PATH** | — | `cargo run --release -p aprender-contracts-cli --bin pv --` |
| bashrs, just, shasum | gates, recipes | yes | 6.66.3, 1.46.0 | — |
| all-MiniLM-L6-v2 snapshot `1110a243...` | SetFit `--model-dir` | yes | — | — |
| `apr` built with `--features setfit` from HEAD | SetFit cell | must be built (`scripts/apr_bin.sh` pins it) | — | — |
| partner CSV | prepare | yes (read-only, 11,311 rows) | motivation-experiment @ b7057de | — |
| Disk | run dir ~3 x 0.85 GB during training + 0.85 GB artifact | not re-measured (08-16: 26 GiB free at 98 %) | — | check `df -h .` first (08-16 pattern) |

**Missing dependencies with no fallback:** none. **Missing with fallback:** `pv` (cargo run); the stale resolver proof (re-run the recipe).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (libtest) per crate; Python `--selftest` entry points (numpy + pyyaml, no pytest) in `scripts/laya_train`; `just` self-test recipes |
| Config file | per-crate `Cargo.toml`; `scripts/laya_train/pyproject.toml` (frozen `uv.lock`) |
| Quick run command | `cargo test -p aprender-mcp-decide-lambda --lib`; `uv run --project scripts/laya_train --frozen python scripts/laya_train/data.py --selftest` |
| Full suite command | `just laya-train-selftest && just laya-gates-selftest && cargo test -p aprender-decide --lib && cargo test -p aprender-mcp-decide --lib && cargo test -p aprender-mcp-decide-lambda --lib` |

### Phase Requirements -> Test Map (each with its failing direction)
| Req ID | Behavior | Test Type | Automated Command | Fails when |
|--------|----------|-----------|-------------------|------------|
| SC-1 | prepare rule: dedup, 5-group drop, 20 % stratified disjoint eval, write-once, no text printed | unit (Python selftest) | `uv run --project scripts/laya_train --frozen python scripts/laya_train/prepare_goldstandard.py --selftest` | a synthetic CSV's expected counts/hashes differ, an overlap survives, or a re-run with different bytes is NOT refused — Wave 0 |
| SC-1 | the real data dir passes train.py's own validation | integration (no model) | `python -c 'import data,...; load_task; load_rows; refuse_overlap; calibration_split'` or the probe's validation step | any `REFUSED <rule>` |
| SC-1 | prefix budget measured and within the bracket | measurement | `prepare_goldstandard.py --measure --task <task.json>` asserting `8 x P <= 800` and markers == 2 | P > 100 or an option/instruction cut flagged |
| SC-1 | `task.json` sha in the run record | check | `python -c 'json.load(gate-report.json)["inputs_sha256"]["task_json"] == sha256(task.json)'` | mismatch |
| SC-2 | the probe wrote only the evidence file | check | `test ! -e models/decide/laya-antisemitism-v1 && test -f 10-PROBE-EVIDENCE.json` | a run dir exists before the declared run |
| SC-2 | one declared run, 3 seeds, device read back, exit 0 | integration | `grep -c '^RECIPE WRITTEN' log == 1`; `grep '^DEVICE .* used=mps' log`; `rc` from the file == 0 (3 -> owner checkpoint) | more than one RECIPE line, CPU, or rc != 0 |
| SC-2 | the report decides what its metrics say; the median is the median | unit (existing) | `uv run ... python scripts/laya_train/gate.py --selftest` (plus `gate.verify_report` on the real report) | `REFUSED pass`/`REFUSED seeds` |
| SC-3 | deploy-eligible `.apr` | integration | `just laya-verify <apr> <run> <data> <base>` -> last JSON line `deploy_eligible: true`, `artifact_sha256 == shasum` | exit 2/3 or `false` |
| SC-3 | labels in task order in the artifact | check | `pack_laya inspect <apr>` `labels == ["not_antisemitic","antisemitic"]` | reordered |
| SC-4 | SetFit doors accept a two-label attested dir | tracer | `apr data select --data <adapted> --shots 64 --seed 13 --output <d>` then `apr setfit train ... --dry-run` | any `ValidationFailed` / `UncalibratedRegime` |
| SC-4 | the cell's test split IS the Laya eval set | check | compare sha256 of the adapted `test.jsonl` texts (normalized) against `eval.jsonl` texts; counts 2,128 and per-class equal | any difference |
| SC-4 | the comparison is recorded with its label | check | `python -c 'e=json.load(open("10-SETFIT-CELL-EVIDENCE.json")); assert "same eval split, subsampled train" in e["label"] and e["cell"]=="s64e1b16"'` | missing |
| SC-5 | contract edit consistent | unit | `cargo test -p aprender-mcp-decide --lib tests::max_texts_fit_the_budget_at_the_shortest_row`; `pv validate contracts/decide-tool-boundary-v1.yaml` (via cargo run) | 8 x P > 800 or pv errors |
| SC-5 | offline deploy chain | integration (no AWS) | `DRY_RUN=1 LAYA_ELIGIBLE_* ... just laya-deploy-selftest` ends with the exact line `DEPLOY SELFTEST OK` and `AWS CALLS: 0` | a `(positive dry run SKIPPED: ...)` suffix or FAIL |
| SC-5 | live identity | integration (live, owner-approved) | `probe --url <endpoint> --apr <apr> --expect-sha256 <H>` exit 0 with all four checks true | any false check |
| SC-5 | acceptance replay | integration (live) | `probe --replay` (new) -> `one_result_per_text && sums_within_1e-5 && label_is_argmax && labels_in_task_order && identity_matches` | any false |
| SC-5 | cold/warm timings reported, not gated | evidence | `laya-deploy-verify ... 2` -> `deploy-evidence-aprender-mcp-decide.json` copied into `10-LIVE-DEPLOY-EVIDENCE.json` | missing samples (a slow sample is reported, not a failure — except the recipe's own 30 s containment rule) |
| SC-5 | project gates | gate | `cargo fmt --all -- --check`; `cargo clippy -p <touched> --lib --examples -- -D warnings`; the three crate `--lib` suites; `just laya-claims-check`; `cargo test -p aprender-core --test readme_contract` | any non-zero rc |

### Sampling Rate
- **Per task commit:** the touched crate's `cargo test -p <crate> --lib` + `cargo clippy -p <crate> -- -D warnings`; for Python edits the file's `--selftest`.
- **Per wave merge:** `just laya-train-selftest`; `just laya-gates-selftest`; `cargo fmt --all -- --check`.
- **Phase gate:** the Full suite command above plus `DRY_RUN=1 just laya-deploy-selftest` green before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `scripts/laya_train/prepare_goldstandard.py --selftest` — covers SC-1 (new)
- [ ] `scripts/laya_train/probe_epoch.py` with a `--dry-run` that validates the data dir and prints the planned steps without loading the model — covers SC-2's probe precondition (new)
- [ ] the SetFit adapter's unit test: two-label rows round-trip through `from_labeled_rows` -> `encode_jsonl` -> `read_attested_canonical` (new; `cargo test -p <crate>`)
- [ ] `probe.rs --replay` unit test against the tiny loopback fixture (the existing `probe_fails_when_response_labels_differ` pattern, src/probe.rs:796-823)
- [ ] a `scripts/laya_gates.tsv` row for the new `8 x P <= 800` measurement gate, so `laya-gates-selftest` sweeps it

## Security Domain

### Applicable ASVS Categories (L1)
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no (auth off by owner decision, D-13 / D-ITEM-08-18-A) | recorded posture in the hand-off; containment recipe |
| V3 Session Management | no (stateless streamable HTTP) | — |
| V4 Access Control | yes (S3 read) | the stack-declared `s3:GetObject` on `decide/aprender-mcp-decide/*` only (`_laya-iam-check`, `laya-grant` readback) |
| V5 Input Validation | yes | `ClassifyArgs` `deny_unknown_fields`, the 8/16 KiB/800 door (unchanged); prepare script refuses malformed rows via data.py |
| V6 Cryptography | yes | sha256 artifact pin verified before parse; sha256 of task.json and data files in the record; no hand-rolled hashing (`common.sha256_*`) |
| V8 Data Protection | yes | tweet text never committed/printed/logged; account id scrubbed from evidence; `deploy.toml` gitignored |

### Known Threat Patterns for this phase
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Serving an artifact other than the approved one | Tampering / Spoofing | content-addressed S3 key + `APRENDER_DECIDE_SHA256` pin + identity probe `--expect-sha256`; the partner's `DriftChecked` |
| A task.json edited after approval | Tampering | `inputs_sha256.task_json` in gate-report and the artifact; the hand-off quotes the approved sha |
| Tweet text leaking into git or logs | Information Disclosure | prepare prints counts/hashes; replay uses synthetic texts; data under `/data/` (gitignored); server logs no request text |
| Cost amplification on the open endpoint | Denial of Service | `just laya-teardown`; the recipe's 30 s containment rule; no replay loops beyond the recipe's samples |
| Live AWS action without consent | Elevation of Privilege | blocking decision checkpoint before `laya-upload`/`laya-deploy` (08-30 T-08-30-01 pattern) |

## Sources

### Primary (HIGH confidence — read this session)
- `scripts/laya_train/{train.py, data.py, contract.py, gate.py, prepare_stance.py, common.py, README.md}`; the pinned `laya/common.py` and `laya/agent.py` in the uv venv.
- `crates/aprender-decide/src/laya/builder.rs`, `src/task.rs`, `src/artifact.rs:950-990`, `src/verify.rs` (grep), `examples/pack_laya.rs`; `crates/aprender-mcp-decide/src/lib.rs:747-766`, `src/tests.rs:188-200`; `crates/aprender-mcp-decide-lambda/{examples/probe.rs, src/probe.rs, README.md, .pmcp/deploy.toml.template}`.
- `contracts/{laya-finetune-gate-v1, laya-parity-v1, decide-tool-boundary-v1, setfit-train-lifecycle-v1}.yaml` (cited line ranges); `crates/aprender-train/src/train/setfit/{thresholds.rs, bench_row.rs, evaluate.rs}`; `crates/apr-cli/src/commands/{setfit_train.rs, setfit_bench.rs, data_contrastive.rs, data_tweeteval.rs, eval/setfit.rs}`; `crates/aprender-contrastive-data/src/{prepared.rs, attestation.rs, schema.rs, split.rs}`; `scripts/run_bench_cells.sh`.
- `justfile` recipes `laya-train`, `laya-pack`, `laya-verify`, `laya-deploy-config`, `laya-build-bootstrap`, `laya-deploy`, `laya-upload`, `laya-deploy-verify`, `laya-resolver-proof`, `laya-deploy-selftest`, `laya-gates-selftest`, `laya-claims-check`.
- Phase 8 records: `08-EXPERIMENT-COACH-DECIDE-REQUEST-REPLY.md`, `261021-ia2-FACTS.md`, `08-16-SUMMARY.md`, `08-GATE-RUN-EVIDENCE.json`, `08-30-PLAN.md`, `08-30-SUMMARY.md`, `08-LIVE-REDEPLOY-EVIDENCE.json` (key dump), `deferred-items.md:313-320, 615-630`.
- Partner (read-only): `docs/requests/aprender.md`, spec sections 5.4 and 7, `crates/core/src/classifier.rs:1-60, 100-250, 330-400`; the CSV counted with `python3 -I` (no text printed).
- Measurements this session: CSV counts; tokenizer-only P and tweet-length percentiles (`uv run ... python -I` with `laya.common.build_sequence`), reproducing the contract's 57.
- Memory notes: SetFit CPU-only and quadratic; tiny-fixture bit-identity; door bounds are tier policy.

### Secondary (MEDIUM)
- `.claude/skills/spike-findings-aprender/references/{laya-decision-model.md, laya-calibration-gate-and-pack-tolerance.md}` (measured rates on other row counts, used for the A2 projections).

### Tertiary (LOW)
- None. No web lookups were needed; the `research-plan` seam was not invoked (every question was answerable from the two repos).

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new packages; every tool version read from the box.
- Architecture / sequence: HIGH — commands of record read from the recipes and the 08-30 evidence.
- Door arithmetic and prefix budget: HIGH — measured with the served tokenizer and controlled against the contract's 57.
- Time/memory projections: LOW-MEDIUM — all `[ASSUMED]` until the D-10 probe.
- SetFit cell: MEDIUM-LOW — the generic doors exist; nothing two-label has ever passed them; the bench row is ruled out.

**Research date:** 2026-10-21
**Valid until:** 2026-11-04 (the cargo-pmcp version and the partner repo move; re-check both before Plan 5)
