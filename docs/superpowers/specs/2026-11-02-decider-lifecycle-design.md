# Decider lifecycle: the contract between the feedback, training and serving connectors

Date: 2026-11-02. Status: draft for the owner's approval. Umbrella document; the
feedback connector's own design is `~/projects/mcp/feedback-coach/docs/superpowers/specs/2026-11-02-feedback-coach-design.md`.

Attribution convention used throughout: **decided** marks what the owner said in the
brainstorming conversation; **my call** marks a choice the author made and the owner has
not yet confirmed; **builder's call** marks what is deliberately left open.

## 1. Intent and why

The platform is built from MCP connectors and ReAct-loop agents that use their tools. It
serves enterprises that have few data scientists or developers and rely on business
analysts who build processes with no-code and low-code tools (Power Apps, Alteryx,
MuleSoft). Those analysts want to use ChatGPT, Claude Desktop or a similar chat to drive
the training and deployment of classification models ("deciders") and to connect them to
agents and workflows (decided).

Today a decider is produced by a chain that only an operator can run: a spreadsheet is
cleaned by hand into the trainer's four-file layout on a Mac, staged into S3 by a
`justfile` recipe, trained by the Laya training MCP server on SageMaker, packed and
verified, uploaded, deployed to a pmcp.run server with another recipe, and verified
against the stored evaluation scores with a third. Every step works (Phase 16 proved the
SageMaker path reproduces the Mac reference and that a packed artifact serves the run's
own probabilities to within 2e-6), but none of it is reachable from a chat, and there is
no loop from the decisions people make on the model's output back into the next model.

The Phase 16 comparison also showed what the loop must get right. The SageMaker-trained
`antisemitism-v1` candidate (sha `22796ff9…`) against the live model (sha `2293bebb…`)
on the same 2,129 held-out rows: at a fixed 0.5 cutoff the candidate looked worse because
the two models are calibrated on different scales; at matched precision it finds more up
to 95% precision (62.0% vs 53.7% of positives at ≥80% precision, 50.5% vs 43.0% at ≥90%,
41.7% vs 36.1% at ≥95%) and fewer at 98% (30.5% vs 34.0%); the AUROC is a tie (0.898 vs
0.901); and the difference is one run against one run, about p = 0.10. Under the
analysts' rule for this task ("a false flag is worse than a miss") the candidate is not
clearly better, so the owner kept it on the dev endpoint as staging and will promote only
on a clear improvement (decided). The lifecycle below is the deliberate version of that
episode: a fair comparison on a frozen test set, an operating point chosen per model from
the analyst's requirement, and promotion as a recorded decision with prerequisites.

## 2. The organizing principle: three roles, three connectors

The logic is split along the traditional roles of a machine-learning team (decided):

| Role | Connector | Owns | Never does |
|------|-----------|------|------------|
| Data engineer: gathers the data | **feedback-coach** (new, `~/projects/mcp/feedback-coach`, its own repo, not part of aprender) | review samples, annotations, provenance, exporting a *part* of the dataset into the trainer's storage | split, format, train, judge a model |
| Data scientist: builds the model with best practices | **training connector** (existing: `crates/aprender-mcp-laya-train*`) | the dataset definition (task, labels, parts), analysis, split and the four-file materialization, training, the report card, the comparison verdict | collect labels, deploy |
| ML-Ops engineer: serves the model | **serving connector**: an `aprender` built-in server type on pmcp.run (`~/Development/mcp/sdk/pmcp-run/built-in/`), one server per model, built from configuration; the existing per-model crates `crates/aprender-mcp-decide*` migrate to it later | the server configuration as the current-model record, build-time eligibility checks, serving verification, promotion, rollback, holds, "what is live where" | train, judge quality (it checks that the judgments exist) |

No fourth connector. The owner rejected a separate ops connector: the ML-Ops role is the
serving server (decided). A serving server could be namespace-based and host several
models, but because loading the weights takes 18–27 s cold the first version stays one
server per model (decided).

Serving is a **built-in server type**, not a hand-written crate per model (decided).
pmcp.run builds servers of a type (SQL, OpenAPI, GraphQL, GraphRAG, workbook) from
configuration only; an `aprender` type for `.apr` models is added there as part of this
design effort, and the existing deciders are migrated to it later (decided).
Provisioning a new decider's server is then the platform's job: the analyst creates it
in the pmcp.run wizard from a configuration that names the model.

The chat LLM (ChatGPT, Claude Desktop) does the spreadsheet cleaning and maps the
analyst's columns to the connectors' tool schemas (decided). Connectors own the data
science; the chat owns the conversation.

## 3. Goals, non-goals, anti-goals

Goals
- An analyst can, from a chat, build a first decider from a few hundred expert-labeled
  rows, read a report card in plain words with acceptable ranges, choose an operating
  point from their own rule about which mistake is worse, route uncertain items to
  people at a volume the business can carry, feed the reviewers' decisions back as new
  training data, train a candidate, and promote it only when it is clearly better.
- Every number in a report card is reproducible from recorded inputs: dataset version,
  parts, split seed, run, eval set sha.
- Every connector stays modular and small. Existing MCP connectors do not grow complex
  behaviour they did not have; `classify` returns scores only and keeps no state
  (decided).
- A new decider's server is created from configuration in the platform, not by writing
  a crate (decided).

Non-goals
- Verified identity for chat uploads and promotions in the first versions (recorded as
  unverified strings; see §8). feedback-coach's annotators are verified principals.
- Many models per serving server (later, gated on cold start).
- Active learning beyond "uncertain items go to people" and a random audit sample.
- Replacing the Mac-side `just laya-*` recipes before the connector tools exist; the
  recipes stay the operator's path until each tool is live.

Anti-goals (a build that does these has failed even if it works)
- A model promoted to prod during a running experiment: a model change mid-experiment
  confounds it (decided). Holds exist for this.
- A candidate compared with the live model on a different test set, or at a fixed
  cutoff across differently calibrated models.
- Expert labels silently overridden by later labels, or conflicts resolved without a
  record.
- Training data drawn only from the review queue with no way to see the selection bias.
- The feedback connector learning how to split or format datasets, or the training
  connector learning how to deploy.

## 4. The lifecycle, step by step

1. **First dataset.** The analyst's cleaned spreadsheet reaches the training connector
   as the first *part* of a named dataset, with the task (labels, instructions). The
   connector analyzes it and answers in plain words: rows per label, duplicates,
   conflicts, empty or very short texts, imbalance, and advice ("class B has 14 rows,
   add at least 20 more").
2. **Train and evaluate.** The connector materializes a dataset *version* (the four
   files the trainer already reads), trains several seeds, gates, packs, and produces
   the *report card*: precision, recall, F1 and ECE with acceptable ranges, a cutoff
   table (what precision and recall each cutoff gives), and a review-budget table (how
   accuracy improves when 10/20/30% of items go to people) (decided: the metrics and the
   ranges; my call: the two tables, taken from the Phase 16 analysis).
3. **Operating point.** The analyst states which mistake is worse and how many reviews
   the business can carry; the cutoffs for auto-accept, auto-clear and send-to-review
   come from the report card. In the first version the agent or workflow applies them
   (decided: a different agent or connector decides on review, not `classify`).
4. **Serve.** The serving server scores items; `classify` is unchanged.
5. **Review.** A domain expert opens a campaign in feedback-coach (which class, which
   score band, who labels, how disagreements are settled). The agents that call the
   decider read the open campaigns and send candidates; feedback-coach applies the
   campaign's criteria and audit share in code. Experts annotate through a review
   widget in their chat client, or an Excel sheet from a CSV batch (feedback-coach
   spec, §4–§7).
6. **Export a part.** feedback-coach writes the newly annotated rows as a new part into
   the trainer's dataset storage (decided: directly into the trainer's S3; decided: only
   the new annotations, in a structure the trainer picks up).
7. **Retrain and compare.** The training connector analyzes all parts, materializes a
   new version on the same frozen eval set, trains a candidate and compares it with the
   model currently live. The verdict is one of *clearly better*, *not clearly better*,
   *worse*, with intervals, under the analyst's cost rule.
8. **Promote.** A new configuration naming the candidate is published on the prod
   server; the type's build-time check verifies the prerequisites (§5.6) before the
   platform deploys it, and the platform's configuration history records who published
   and why. A held server refuses.

## 5. Contracts between the connectors

These are the surfaces two connectors share. Everything else is internal to one
connector and left to its own design.

### 5.1 Text normalization (shared rule)

Deduplication and overlap checks across connectors must agree. The rule is the
trainer's existing `normalized_sha256`: NFC, trim, collapse runs of whitespace to one
space, then sha256 of the UTF-8 bytes (`crates/aprender-mcp-laya-train/src/validate.rs`,
mirrored in `scripts/laya_train/data.py`). feedback-coach derives sample ids from the
same rule so a text already in a dataset is recognized as the same row.

### 5.2 A part (what the data engineer hands the data scientist)

A part is an immutable, append-only unit of labeled rows under the trainer's dataset
prefix (my call on the layout; the trainer's segment grammar `^[a-z0-9][a-z0-9._-]{0,63}$`
with 1–4 segments under `datasets/` applies):

```
datasets/<dataset>/parts/<part-id>/rows.jsonl
datasets/<dataset>/parts/<part-id>/part.json
```

- `<dataset>` is the trainer-side dataset name, one segment.
- `<part-id>` is `<utc-stamp>-<sha12>`, e.g. `20261102t213000z-3f9a1c0b7d2e`: the UTC
  time of creation in lowercase and the first 12 hex of `rows.jsonl`'s sha256. The same
  rows exported twice produce the same suffix, which makes a repeat visible.
- A part is created with a conditional put (fails if the prefix exists) and is never
  modified. A corrected part is a new part.

`rows.jsonl`, one JSON object per line:

| field | required | meaning |
|-------|----------|---------|
| `text` | yes | the item, non-empty |
| `label` | yes | the current true label; must be one of the dataset's labels |
| `sample_id` | feedback-coach: yes; chat upload: no | feedback-coach's id: `s` followed by 16 hex of the normalized-text sha, namespace-scoped (the letter keeps spreadsheets from reading the id as a number) |
| `source` | no | `upload`, `uncertain`, `random_audit`, `conflict_review` |
| `annotator` | no | feedback-coach: the verified principal whose label decided, or `majority`; chat upload: unverified string (§8) |
| `annotated_at` | feedback-coach: yes; chat upload: no | RFC 3339 UTC; from feedback-coach, the time the sample's derived label last changed (it may combine several annotators' records) |
| `resolved` | no | `true` for an explicit human resolution: an adjudicated label, or a row from a `conflict_review` campaign; read by §6.2 rule 2 |
| `scored_by` | no | sha256 of the model artifact that scored the item when it was added |
| `predicted_label`, `score` | no | the model's prediction and the probability of it |

The trainer writes only `text` and `label` into `train.jsonl` and `eval.jsonl` (those
files accept exactly `{"text","label"}` today and stay that way). Three provenance
fields are semantic, not decorative, because the supersession rule in §6.2 reads them:
`producer` (from `part.json`), `sample_id` and `annotated_at`; rule 2 also reads
`resolved`. Everything else is kept for reports.

`part.json`:

```json
{
  "schema": "laya-part/v1",
  "dataset": "antisemitism",
  "part_id": "20261102t213000z-3f9a1c0b7d2e",
  "producer": {"connector": "feedback-coach", "namespace": "antisemitism", "version": "<build fingerprint>"},
  "created_at": "2026-11-02T21:30:00Z",
  "row_count": 143,
  "rows_sha256": "<64 hex>",
  "label_counts": {"antisemitic": 61, "not_antisemitic": 82},
  "selection": {"annotated_from": "...", "annotated_to": "...", "source": null, "label": null}
}
```

The analyst's own spreadsheet reaches the trainer as a part too, with
`producer.connector = "chat"` and `source = "upload"`. How the rows travel from the chat
to the trainer (tool arguments in bounded batches, or a presigned upload) is the training
connector's design; Phase 14 deferred the presigned PUT and it is still open.

### 5.3 A dataset version (what the data scientist trains on)

```
datasets/<dataset>/versions/<version-id>/task.json
datasets/<dataset>/versions/<version-id>/train.jsonl
datasets/<dataset>/versions/<version-id>/eval.jsonl
datasets/<dataset>/versions/<version-id>/manifest.json
datasets/<dataset>/versions/<version-id>/provenance.jsonl   (my call; optional)
```

A version prefix is a valid `dataset_uri` for today's `train` tool with no trainer
change: three segments, the three required files, and the trainer ignores files it does
not read. So the first increment of the trainer work is the materialize step, not a
rewrite of the reader.

`manifest.json` records: the ordered list of parts, the eval generation (§6.1), the
split rule and seed, every data-science parameter used and whether it was a default
(decided: parameters can be set from outside and otherwise take best-practice defaults;
my call: defaults are recorded, never silent), the counts per label in train and eval,
the rows excluded as conflicts, the rows dropped for eval overlap, and the per-file
sha256 the trainer pins anyway.

### 5.4 The report card and the comparison verdict

Produced by the training connector; read by people, by the agent that applies cutoffs,
and by the serving connector's promotion check.

- Per model: macro and per-label precision, recall, F1; ECE after calibration; the
  acceptable range next to each number (decided); the cutoff table; the review-budget
  table; the frozen eval set's sha and row count; intervals from bootstrap over the eval
  rows (my call).
- Comparison (`candidate` vs `baseline`): refuses unless both runs scored the same
  `eval.jsonl` sha; matched-precision table; review-budget table side by side; a verdict
  in {`clearly_better`, `not_clearly_better`, `worse`}.

  Inputs and the verdict rule (my call, settling the builder check's fifth and sixth
  questions):
  - A dataset may declare one `positive_label`. With it, the comparison is binary: the
    cost rule is one of `false_flag_worse`, `miss_worse`, `symmetric`, required, with
    no default (a default would silently choose the analyst's business rule). The rule
    sets the decision metric: `false_flag_worse` compares recall at a precision floor;
    `miss_worse` compares precision at a recall floor; `symmetric` compares F1. The
    floor is a parameter (default 0.95 for `false_flag_worse`, 0.90 for `miss_worse`,
    recorded as defaults like every other parameter). The review budget selects the
    row of the review-budget table the verdict is also reported at; it does not change
    the decision metric.
  - Without a `positive_label` (for example `motivation-v1`'s three labels), the
    decision metric is macro-F1, the per-label precision and recall tables are shown
    one-against-the-rest, and the cost rule is not asked for. A per-label cost rule is
    a later feature.
  - The verdict is computed by paired bootstrap over the eval rows of the difference in
    the decision metric: `clearly_better` when the 90% interval of the difference lies
    entirely above zero, `worse` when entirely below, `not_clearly_better` otherwise.
    The interval and the point difference are printed; "one run against one run" is
    said when it is so. Phase 16's candidate would come out `not_clearly_better` under
    `false_flag_worse` at a 0.95 floor (41.7% vs 36.1% recall, p ≈ 0.10).
  - The acceptable ranges on the report card are two kinds: the gate bars from
    `contracts/laya-finetune-gate-v1.yaml` (fixed per contract) and the business
    ranges, which are per dataset with defaults from the owner's bars (error up to 15%
    acceptable at the start, 90% accuracy good, 95% better) and recorded in the
    manifest.

Words the report uses: "good enough", "needs more examples of X", "these two classes
are confused, check their labels", "the new examples improved the model slightly /
significantly" (decided: the vocabulary is the analyst's).

### 5.5 The model record

One record per trained candidate, written by the training connector, referenced by the
serving connector: `model_id`, artifact sha256, recipe id, dataset version id and its
parts, eval sha, gate result, pack record reference (the Graviton pack task's
`pack-outcome.json` bound to the sha is the deploy eligibility today), report card
reference, and the comparison verdicts it took part in.

### 5.6 Environments, the current-model record and promotion on a built-in server

What a built-in server is (read-only survey of `~/Development/mcp/sdk/pmcp-run/` on
2026-11-02, agent-reported): one Rust Lambda per server, generated by CodeBuild from
the type's templates, with `config.toml` embedded at compile time (`include_str!`) and
deployed as its own stack by `cargo pmcp deploy`. The configuration lives in the
platform's `ServerConfig` record, with a `ConfigVersion` history (who saved, when) and a
`BuildRecord` per build. A configuration change takes effect only through
`triggerBuild`, a rebuild and redeploy; nothing is reloaded per request. Memory and
timeout are written per type by the buildspec (512 MB/30 s for most types, 1,024 MB/60 s
for GraphRAG); the platform validator accepts up to 10,240 MB and 900 s, but no type
uses the large tier yet. Per-tool gating by group exists in the GraphRAG type
(`required_role` read from `x-pmcp-user-groups`, `admin` bypasses), and the proxy
stamps a verified `x-pmcp-user-id` / `-email` / `-groups` on every call.

Consequences, my calls:

- **The configuration is the current-model record.** `[model]` names the artifact
  sha256, its S3 location, the labels and the recipe; `[bounds]` the per-call limits
  (8 texts, 800 built tokens today); `[records]` the bucket and prefix where model
  records and verifications live; `[hold]` an optional reason; `environment` is
  `staging` or `prod`. The platform's `ConfigVersion` history is the audit trail (who
  published which sha, when, with what reason) and a previous version is the rollback.
- **Topology (kept):** each decider has a staging server in `ze-kasher-dev`
  (`us-east-1`) and a prod server in `augury-dev` (`eu-west-1`), both built-in servers
  of the `aprender` type.
- **`stage` and `promote` are configuration publishes, checked at build time.** A
  publish that names a new artifact sha on a staging server requires a pack record
  bound to the sha and a gate pass. On a prod server it also requires a comparison
  verdict whose baseline is the sha the previous published version of this server named
  (or an explicit `first_model` when none), on the same eval sha; a staging verification
  record for the sha; no hold in the previous version unless this publish clears it
  with a reason; and a stated reason. The build fails naming the missing prerequisite
  instead of deploying an ineligible model. "Clearly better" is shown, not enforced:
  a `not_clearly_better` candidate may be promoted with a reason.
- **Tools on the server:** `classify` (unchanged, open), `status` (sha, labels,
  environment, build id, hold, the records it can see) and `verify` (scores a sample of
  the frozen eval rows against the training run's stored probabilities and writes the
  verification record under `[records]`). `verify` is `required_role = "admin"`.
- **A hold is a configuration field.** It is a rule the build-time check applies to the
  next publish, not a lock against an editor who clears it; the history shows who
  cleared it. A stronger hold needs a per-server state store, which the platform has
  designed but not built.
- **Who publishes.** In the first version the analyst publishes the configuration in
  the pmcp.run UI, or an operator does with `cargo pmcp`; `triggerBuild` is an
  admin-only platform mutation and no chat-reachable path to it was found. Chat-driven
  promotion waits for a platform-side tool, the dev team's call (§9, type design §11).

Where the records live (unchanged): the model record and comparison verdicts are
written by the trainer under `models/<model_id>/` in the trainer's bucket; the staging
server writes its verification record under `models/<model_id>/verifications/` (a
same-account write grant derived from `[records]`, the way the SQL type derives its
grants from its configuration).

### 5.7 Namespace ↔ dataset ↔ server

A feedback-coach namespace names its trainer dataset. The live model is read from the
serving server's status, not copied into the namespace (my call). Several namespaces
share one feedback-coach deployment; one server per namespace is the escalation when
security or permission levels differ (decided). The same rule applies to the other
coaches (decided).

## 6. Data-science rules the training connector enforces

### 6.1 The eval set is drawn once and frozen

The first materialization of a dataset fixes its eval set, by one of two
`eval_source` values (my call, settling the builder check's second question):

- `draw` (the default for a new decider): a stratified eval set is drawn from the
  parts present (default share 20%, floor 10 rows per label with a warning below it,
  fixed recorded seed; all overridable).
- `existing:<prefix>`: the eval set is taken verbatim from an existing trainer prefix's
  `eval.jsonl`, and that prefix's `train.jsonl` is imported as part 0 with
  `source: upload`. This is how the deciders that are already live (`antisemitism-v1`
  on GoldStandard2024's 2,129 held-out rows, `motivation-v1`) enter the lifecycle:
  their stored eval scores are on exactly that split, so they can be baselines. A
  re-drawn eval set would include rows the live model trained on and bias the
  comparison in its favour.

Later parts go to training only. A later part's row whose normalized text is already
in the eval set is dropped from training and reported (the trainer already refuses
`eval-train-overlap`).

Project 2's first version has exactly one eval generation per dataset. Starting a new
generation (the only route for `random_audit` rows into eval, §6.4) needs a tool that
scores an existing artifact on a new eval set; the re-score path exists in Rust but
runs on the Graviton pack task, not in the trainer's Lambda, so it is a later feature,
and comparisons across generations are refused as not comparable when it arrives.

### 6.2 Supersession is not conflict; conflicts go back to people

The trainer refuses `train-conflicting-labels` inside one file today. Across parts the
materialize step applies these rules, in order, per normalized text (my call, settling
the builder check's first question):

1. **Supersession, within one producer.** Two rows from the same `producer`
   (connector and namespace) with the same `sample_id` are one annotation history: the
   row with the latest `annotated_at` stands, the earlier ones are recorded in the
   manifest as `superseded`. This is what "update the annotation of a sample" means:
   a correction of one's own label is not a disagreement. Without this rule every
   relabel after an export would be a permanent conflict.
2. **Resolution rows win.** A row with `resolved: true` is an explicit human
   resolution (an adjudicated label, or a row from a `conflict_review` campaign) and
   stands over every row for that text from any producer. It is recorded as
   `resolved`. Keying on the flag rather than on `source` lets an existing sample be
   resolved by adjudication without being added again.
3. **Conflict, across producers.** After 1 and 2, two different labels for the same
   text (for example the analyst's upload against a reviewer's annotation) exclude
   that text from the version. The manifest lists the text key, both labels and both
   producers; the analyze report shows them in plain words. The later label never wins
   silently.

Closing a conflict in the first versions is a chat step, not an automatic push: the
analyst looks at the listed items. If the text is already a feedback-coach sample, an
adjudicator of its campaign adjudicates it (rule 2, or rule 1 if the label simply
changes to agree). If it is not, the analyst adds it to a feedback-coach campaign with
purpose `conflict_review` and annotates it (rule 2); such campaigns accept samples
without a prediction. An automatic hand-off of
conflicts from the trainer to feedback-coach stays a later version.

### 6.3 Small data is reported with intervals

Datasets are hundreds of rows, hand-annotated by domain experts, on models that excel at
few-shot learning (decided). The report card carries intervals, and the comparison says
"one run against one run" when that is what it is. The Phase 16 numbers above are the
worked example.

### 6.4 Selection bias is visible

Rows that reached people because the model was unsure are not a random sample. Every
row carries `source`; the report card can split accuracy by source once the audit sample
exists, and `random_audit` rows are the only review-fed rows allowed into a new eval
generation (my call).

### 6.5 Parameters: settable, defaulted, recorded

Eval share, stratification, seed, per-label floor, the conflict rule, and whether a part
may feed eval are tool parameters. Absent, they take best-practice defaults (decided).
The manifest and the analyze report state the values used and which were defaults.

## 7. Security and permission boundaries

- **Isolation by key, not by auth.** In the coach family a tenant or namespace appears in
  every key, path and id and no tool reads across namespaces; anyone who can call the
  server can address any namespace. Acceptable among trusted analysts; the known
  escalation is one server per namespace (decided).
- **The one shared storage.** feedback-coach writes new part prefixes under
  `datasets/*/parts/*` in the trainer's bucket and nothing else: create-only, no
  overwrite, no delete, no reads outside the prefix it is creating. Same account
  (`ze-kasher-dev`) today, so an IAM grant, not a cross-account role.
- **Weights and records reach prod by a copy, not a cross-account read.** No built-in
  type reads S3 across accounts and none assumes a role, so the artifact and the
  records a prod publish needs must be in a bucket in the prod account before the
  build. My call: the type's build step copies them from the trainer's bucket under a
  bucket policy that grants the prod builder role read on `decide/*` and `models/*`;
  until that exists, the operator's `laya-upload` does the copy. The trainer never
  writes into prod. The staging server, in the trainer's account, writes only under
  `models/<model_id>/verifications/`.
- **One server, per-tool roles** (my call, replacing the earlier two-door idea now that
  the platform has the pattern): `classify` is open; `verify` and any future
  write-capable tool carry `required_role = "admin"` in the configuration, enforced by
  the type from the proxy's `x-pmcp-user-groups`. The identity the proxy stamps
  (`x-pmcp-user-id`, `-email`) is verified, so "who verified" and "who published" are
  real identities on the serving side, as the annotator principal is in feedback-coach.
- **Promotion rights are the platform's.** Publishing a configuration is an admin
  action in pmcp.run; no connector holds deploy credentials. The IAM the generated stack
  needs (read on the artifact bucket, write on the records prefix) is derived from the
  configuration by the type's CDK template, scoped to those prefixes.

## 8. Known limits, stated rather than hidden

- `promoted_by`, `reason` and a chat upload's `annotator` are caller-supplied strings.
  experiment-coach trusts a caller-supplied `approver` the same way; this design repeats
  the limit for the first versions and records the platform's proxy identity alongside
  when it arrives, so a later version can enforce it. feedback-coach's annotators are
  verified principals with roles checked in code (its spec, §5.1); a label an
  annotator's LLM submits is still recorded under that annotator.
- Prefix immutability in the trainer is a staging convention (the stage recipe refuses
  a non-empty prefix); there is no S3 object lock. Parts and versions follow the same
  convention with conditional creates.
- The trainer takes exactly one `dataset_uri`; parts are composed by the materialize
  step, not read by the training job.
- feedback-coach ships its review widget in v1 (the review-coach pattern); the
  training and serving connectors ship none in the first versions.
- A hold on a built-in server is a configuration rule, not a lock (§5.6).
- The 10,240 MB tier passes the platform's validator but no built-in type has run on
  it; the `aprender` type is the first, and its cold start on that path is unmeasured
  until the spike in §9.
- Promotion is not chat-reachable in the first version; it is a configuration publish
  in the platform UI with the checks at build time.

## 9. Order of work

Three projects, each with its own plan, in this order (decided: feedback-coach first):

1. **feedback-coach v1** (own repo, GSD project). Namespaces, samples, annotations,
   filtered queries, part export. Proof: a part lands in the trainer bucket with the
   layout in §5.2 and its rows parse under the trainer's row grammar. Dataset names
   starting `fcproof-` (one per proof run) are reserved for that proof and project 2
   never imports them;
   the first real lifecycle dataset is `antisemitism`, created by project 2 from
   `datasets/goldstandard-2024/` (§6.1).
2. **Training connector: datasets from parts** (aprender phase). Dataset and part tools,
   analyze, materialize to a version prefix, frozen eval, conflicts out, parameters with
   defaults, report card with the two tables and intervals, compare with verdict, model
   record. The existing `train` path, validation and pinning run unchanged on the version
   prefix.
3. **The `aprender` built-in server type** (its own design document:
   `docs/superpowers/specs/2026-11-03-aprender-built-in-server-type-design.md`; this
   umbrella fixes only the contract). Ownership, decided 2026-11-03: the pmcp.run dev
   team builds the platform side (builder image, CodeBuild, state machine, Admin UI,
   GraphQL) from that document; the aprender side delivers the core crate
   `aprender-mcp-builtin` at a pinned git revision. It generalizes
   today's per-model crate: `config.toml` as in §5.6, weights fetched from S3 at cold
   start and checked against the sha, `classify` with the existing bounds, `status`,
   `verify`, per-tool roles, and the build-time eligibility check. Touchpoints the
   survey found, as a list: the `serverType` enum in the data model and the build
   trigger's allowed-types check; a state-machine branch and SSM parameters for the
   builder project; a buildspec that writes 10,240 MB and the timeout into
   `deploy.toml`; a builder image; templates (`Cargo.toml`, `main.rs`, the CDK app that
   derives IAM from the configuration); a Cargo workspace `built-in/aprender-api/` with
   the core crate on the platform's pmcp pin (2.19 today; the aprender crates must
   build against it); the wizard, editor and type registry in the UI; tests next to the
   other types'. The existing deciders keep running on their crates until the type is
   proven on a new server, then migrate (decided). Two things to measure first, as a
   spike: the CodeBuild time of a crate that depends on `aprender-decide` and the
   ModernBERT encoder, and the cold start of a zip-deployed 10,240 MB built-in server
   fetching 846 MB from S3 (today's per-model crates measure 18.6–27.3 s on the same
   path). The Mac recipes remain until this is live.

Chat-driven creation and promotion (a chat creating a built-in server or publishing a
configuration) needs a platform-side tool over the admin `triggerBuild` mutation; none
exists today. Since 2026-11-03 the dev team owns the GraphQL and Admin UI side, so this
is their call and not a dependency of project 3 (type design §11): v1 promotion is a
UI or `cargo pmcp` publish with the checks at build time.

Open check that belongs to project 2: whether ChatGPT and Claude Desktop can hand a
file to a connector (presigned upload) or whether rows must travel as tool arguments in
bounded batches.

## 10. Left to the builders

- Storage engines and key layouts inside each connector, within the conventions of its
  family (feedback-coach: DynamoDB, append-only annotations; trainer: S3 under the
  existing bucket; serving: the current-model record).
- The exact bootstrap and interval methods in the report card.
- The per-call bounds (rows per tool call, CSV size), sized to the 30 s pmcp.run gateway
  cap and to what a chat can carry.
- ~~How the `aprender` type's builder image obtains the `aprender-decide` crate~~:
  answered in the type design (§6.1): a git dependency pinned by revision on the public
  fork, the lock seeded from the fork, the dependency compile seeded into the builder
  image.
- Whether `provenance.jsonl` is written per version or derived on demand.

## Appendix: facts this design rests on

- Training connector today: `train(dataset_uri, name, epochs?)`, `validate_dataset`,
  `train_status`; one prefix; files `task.json`, `train.jsonl`, `eval.jsonl`, optional
  `shift.jsonl`; caps 32 MiB per file, 64 MiB total, 200,000 rows; rows exactly
  `{"text","label"}`; validation mirrored in Rust and Python with a shared case table;
  per-file sha256 pinned through the state machine, checked in the container, recorded
  in `gate-report.json` and the manifest; `train-conflicting-labels`,
  `eval-train-overlap` and `eval-class-coverage` refused (survey of 2026-11-02, read-only).
- Serving today: `classify(texts)`, at most 8 texts and 800 built tokens; response carries
  `model.artifact_sha256`, `recipe_id`, per-row `label`, `probabilities`, `tokens`,
  `truncated`; deploy pinned to a sha by a generated TOML; `laya-deploy-verify` compares
  served probabilities with the run's stored `eval-probs.json` (2.03e-6 observed on the
  dev endpoint, 2026-11-02); cold start 18.6–27.3 s against a 30 s gateway cap.
- pmcp.run built-in servers (read-only survey of `~/Development/mcp/sdk/pmcp-run/`,
  2026-11-02): types `openapi`, `graphql`, `sql`, `mcp_api`, `graph_rag`, `workbook`;
  one generated Rust Lambda per server, configuration embedded at compile time,
  `ServerConfig` / `ConfigVersion` / `BuildRecord` in DynamoDB, `triggerBuild` admin
  mutation, buildspec-fixed memory and timeout, zip deployments only (no container
  images), no cross-account S3 or role assumption in any type, per-tool
  `required_role` in GraphRAG only, verified identity headers stamped by the proxy,
  per-server state store for builder types designed but not implemented.
- Coach family conventions (counter-coach, experiment-coach): Rust on pmcp, axum and
  lambda_http, `bootstrap` binary to pmcp.run, CDK TypeScript storage, hand-authored
  `.pmcp/deploy.toml`, typed-struct returns, `deny_unknown_fields`, exact-set tool lists,
  "ledger first, then derived state", refuse rather than default the tenant, `put_new`
  idempotency, preview-then-commit, two doors over one `Deps`.
