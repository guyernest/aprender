# Phase 13: motivation-v1 on augury-dev (second server) and the first enrichment hand-off - Context

**Gathered:** 2026-10-27 (interactive discuss-phase; the five owner decisions of 2026-10-27 recorded in ROADMAP.md are carried in as D-01..D-05)
**Status:** Ready for planning

<domain>
## Phase Boundary

Deploy the UNCHANGED `motivation-v1` artifact (`laya-motivation-v1.apr`, sha256
d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd, labels
`[social_affiliative, conspiratorial, other]`) as a second decide server on the augury-dev pmcp.run tenant
(AWS account 783960389328, profile `augury-dev`, region eu-west-1), beside the live ze-kasher-dev pilot. The
experiment team runs their stored posts (already classified antisemitic by stage 1) through it, and the expert
annotates the output. The phase also delivers what that round needs to come back as training data: the per-post
record schema, a selection-guidance note, the enrichment-file format, and a `prepare_motivation.py` ingest of that
format as a second source beside the partner's motivation CSV.

In scope: committing the justfile `region` parameter with its own offline check; the augury deploy (a read-only
probe, weights upload, publish, OAuth enablement by the owner, an authenticated acceptance replay and the
unauthenticated refusal check); the record schema; the selection note; the enrichment format; the ingest with its
selftest; and a hand-off to the owner.

Not in this phase: a retrain, a gate run, any change to the served artifact or to the ze-kasher-dev server;
collecting posts or writing labels; any write to motivation-experiment or to the partner's augury stack or secrets;
deploying antisemitism-v1 to augury.

</domain>

<decisions>
## Implementation Decisions

### Owner decisions already made (2026-10-27, from ROADMAP.md, recorded as given)
- **D-01:** The three-class model continues; the binary conspiratorial-or-not reframing is dropped. On the 100-row
  eval, the two functions are confused with each other once (and 0 times in the other direction; 1+2 / 0+0 on the
  other seeds). Every other error is function-vs-other, and the per-class AUC ordering flips by seed, so neither
  class is shown easier (analysis of 2026-10-27 from `models/decide/laya-motivation-v1/eval-probs.json` and the two
  seed dumps).
- **D-02:** Add a second server, not a move: the ze-kasher-dev pilot `aprender-mcp-decide-motivation` (us-east-1)
  stays live and untouched. (The roadmap's "own thin crate, register grows by one" is AMENDED by D-06.)
- **D-03:** The experiment team runs their own stored posts through the new server, and the expert annotates them.
  This phase collects no posts and writes no labels, and no silver label (a model prediction used as a label)
  enters the dataset.
- **D-04:** The review follow-up 12-17..12-19 stays parked; this phase runs on the current verifier and tooling.
- **D-05:** The uncommitted justfile `region` parameter (on `laya-deploy-config` and `laya-weights-bucket`), which
  Phase 12 D-23 excluded, is this phase's work. It is committed with its own check (D-20) and used for eu-west-1.

### Server id and auth
- **D-06:** The augury server REUSES the id `aprender-mcp-decide-motivation` and the existing thin crate
  `crates/aprender-mcp-decide-motivation-lambda`. This amends the roadmap: no new crate, and the FALSIFY-MONO-011
  deployment-unit register and the duplicate-bin allowlist stay unchanged. The two deployments differ by tenant host
  (expected `https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp`, inferred from the partner's
  pattern; trust the endpoint the deploy prints, not this line) and are told apart everywhere by the triple (pmcp.run
  target, AWS account, region), never by id alone. Every evidence record and the hand-off carry that triple. The
  planner must make sure the augury publish cannot overwrite or confuse the ze-kasher-dev deployment's local
  cargo-pmcp state (for example `.pmcp/deployment.toml` or the per-server endpoint record) or its weights object,
  and that every AWS-touching or pmcp-touching recipe is told explicitly which target it addresses. The existing
  server must still answer with sha d202f495… after the augury deploy (a read-only check).
  — **Reversibility:** costly — splitting the deployments later means a new thin crate, register growth, a redeploy
  under a new id and a URL change on the partner side.
- **D-07:** Auth is ON for the augury server, following the partner's runbook (`protectedClassifiers=true`).
  cargo-pmcp cannot render an auth-enabled descriptor for the pmcp-run target, so the server is published with auth
  off and the OWNER enables OAuth in the pmcp.run admin UI straight after the first publish, then creates the M2M
  (client-credentials) client. That is a human checkpoint (autonomous: false). The interval between publish and
  enablement is kept short and recorded. Acceptance then requires both an authenticated replay with the M2M client
  (the secret or token is supplied through a file, never on argv, and never printed or committed) and an
  unauthenticated initialize or call that is REFUSED. aprender's probe and verify tooling has no bearer path today,
  so adding one is in scope. The ze-kasher-dev servers stay auth off.
- **D-08:** The partner side is hand-off only. The owner receives the URL, the pinned sha and the M2M client
  details, for `motivationUrl` / `motivationSha` in `infra/context.augury-dev.json` and the
  `MotivationAuthSecretArn` secret contents. Client credentials go to the owner out of band and never into this
  repo or into evidence. This phase makes no write to motivation-experiment or to the partner's augury stack or
  secrets.

### Enrichment ingest and split
- **D-09:** IMPLEMENT the second-source ingest. `prepare_motivation.py` gains `--enrichment PATH` (repeatable). The
  file is read in place in the partner repo and never copied here. Its sha256 and per-class / per-evidence-level
  counts are printed and pinned in the output. The no-text rule holds (a refusal names a row number, a field and
  at most a character count, never a value), and a planted-sentinel selftest runs on a SYNTHETIC enrichment
  fixture. No real enrichment file exists yet, so the ingest is proven on fixtures only, and the hand-off says so.
- **D-10:** A run WITHOUT `--enrichment` must write exactly the bytes v1's data dir holds today, including the
  owner-approved task.json (sha256 f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3). This is
  enforced by a check, not by care.
- **D-11:** Enriched rows go to TRAIN ONLY. `eval.jsonl` stays frozen and byte-identical to v1's, so the next gate
  is comparable with motivation-v1's numbers. An enriched text whose normalized identity (nfc-trim-ws-v1, the one
  normalizer in `data.py`) matches an eval text is DROPPED and counted. The accepted cost is that eval does not grow
  (about 14 conspiratorial rows).
- **D-12:** Cross-source conflicts extend the existing conflicting-labels rule: an enriched post whose normalized
  text matches a partner-file row (or another enriched row) with a DIFFERENT label drops the whole group, and it is
  counted and reported. There is no source precedence. Matching texts with the same label collapse to one row, as
  today.
- **D-13:** The enrichment format is versioned (for example `motivation-enrichment-v1`) and declared before any file
  exists. Its home is the planner's choice: under `contracts/` only if `pv validate` accepts it as a contract,
  otherwise a schema document plus the ingest's validator (CLAUDE.md: not a bash re-implementation of `pv`).

### Record schema and selection
- **D-14:** The per-post record (the output of a run through the server) carries `post_id`, `source`, `text`,
  `labels` (served order), `probabilities` aligned to `labels`, `artifact_sha256`, the deployment triple (D-06),
  `tokens`, `truncated` and a timestamp. It includes the text because the expert reads it and the ingest normalizes
  and groups on it. The file lives only in the partner repo, and aprender reads it in place and never prints or
  copies it.
- **D-15:** The expert records one of the three labels PLUS the guide's evidence level (clear / weak / mixed). The
  ingest maps to the three served labels by the current rule (guide 10.3: in this dataset, weak and mixed evidence
  count as `other`), declared in the docstring before the run, and counts every (label, evidence level) cell. This
  keeps the weak/mixed decision open for the post-freeze retrain.
- **D-16:** The OWNER chose that the expert SEES the model's prediction while annotating, rather than a blind sheet.
  The enrichment format therefore carries `annotation_mode` (the value for this round, `prediction_shown`). The
  ingest records and counts it and does not act on it. The hand-off of whichever phase retrains on this data must
  state that its labels may be anchored to motivation-v1's predictions.
- **D-17:** The selection note recommends a stratified quota plus an uncertainty band plus a random control, with a
  default budget of about 300 posts. The three parts are: a quota by predicted top label with `conspiratorial`
  over-sampled (the thin class); an uncertainty band (low top-two margin, or a max-probability band straddling
  experiment-coach's `motivation_min_prob` 0.6); and a uniformly random control slice as an unbiased reference. The
  note fixes the proportions, the exact band definition and a seeded selection rule. The partner may change the
  budget. It also tells them not to send only high-probability rows.

### augury deploy path
- **D-18:** A read-only probe first, then go. It checks that `sts` returns 783960389328 for profile `augury-dev`,
  that the cargo-pmcp target `augury-dev` resolves, and that the live augury tenant answers (counter-coach at
  `https://counter-coach.eu-west.augury.pmcp.run/mcp`). If all three pass, the deploy proceeds with no extra
  checkpoint beyond the owner's pmcp.run login (the token lives about an hour; autonomous: false) and the D-07 auth
  step. A tenant or region refusal at deploy time STOPS for the owner as a recorded finding, and nothing is faked.
- **D-19:** Weights REUSE the existing augury bucket `aprender-decide-weights-783960389328-dev` (eu-west-1). This
  was verified read-only on 2026-10-27: it holds `decide/aprender-mcp-decide-antisemite/2293bebb….apr`, uploaded
  2026-10-24, and its name is exactly what `laya-weights-bucket` derives for this account and env. The artifact is
  uploaded content-addressed to `decide/aprender-mcp-decide-motivation/d202f495….apr`. Fallback, allowed by the
  owner: if access or permissions fail, create the bucket with `laya-weights-bucket ... region=eu-west-1`. On
  scale, the layout already scales: one bucket per account and env, a prefix per server, content-addressed objects,
  and each stack granted `s3:GetObject` on its own server's prefix only. More models add prefixes, not buckets or
  policies, and per-model isolation comes from that per-prefix grant.
- **D-20:** The region-parameter commit is proven offline in BOTH regions. `laya-deploy-selftest` (AWS CALLS: 0) is
  extended so that with region eu-west-1 the rendered config carries `region = "eu-west-1"`; the default us-east-1
  output is byte-identical to today's; and a template with zero or two `region = "us-east-1"` lines is refused. The
  RED-turning mutation is re-run (CLAUDE.md verification rule 4: an extended guard needs re-mutation in the new
  scope).

### Carried forward from Phases 10 and 12 (unchanged)
- 10,240 MB tier; deploy through `just laya-build-bootstrap` / `laya-deploy-config` / cargo-pmcp; the server id is
  the argument to the AWS-touching recipes; access checked read-only before live steps, and the executor never
  refreshes credentials.
- F-10-12-1: a server id's first edge `/health` on a tenant can 404 and trigger containment. Even with the same id,
  this is a new tenant, so the plan needs a bounded retry before containing, or an owner-run step. F-10-12-2: a
  sticky 503 was cleared by redeploying.
- The acceptance replay checks function and identity: one result per text, labels in task order, probabilities
  summing to 1, and the returned sha equal to the pin. Cold and warm timings are reported, not gated. The door is
  unchanged (8 texts, 800 built tokens, P 97). Never assert `tokens_planned == texts × P`.
- The partner's data, the record files and the enrichment files are read in place and never copied into this repo.

### Claude's Discretion
- The exact field names of the record and enrichment schemas, and where the format lives (D-13).
- The uncertainty-band definition and the quota proportions in the selection note (D-17), declared in the note.
- How the M2M credential reaches the probe and verify tooling (a file form), and how the unauthenticated refusal
  is probed (D-07).
- The mechanism that keeps the two deployments' local cargo-pmcp state apart (D-06).
- Evidence file names (`13-*-EVIDENCE.json`, following Phases 8, 10 and 12).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Partner (motivation-experiment, outside this repo; read in place, never written)
- `/Users/guy/projects/mcp/motivation-experiment/docs/DEPLOY.md` — "Adding the motivation classifier (augury-dev)", "augury-dev bring-up (runbook)", "M2M secret contents": what the partner does with the URL, sha and M2M client (D-07, D-08)
- `/Users/guy/projects/mcp/motivation-experiment/.pmcp/deploy.augury-dev.toml` — the augury target shape (cargo-pmcp target `augury-dev`, region eu-west-1, host pattern `<server>.eu-west.augury.pmcp.run`, auth published off and enabled in the admin UI)
- `/Users/guy/projects/mcp/motivation-experiment/infra/context.augury-dev.json` — `protectedClassifiers: "true"`, `classifierUrl`; `motivationUrl` / `motivationSha` are added there by the partner
- `/Users/guy/projects/mcp/motivation-experiment/infra/augury-dev-account.txt` — 783960389328
- `/Users/guy/projects/mcp/motivation-experiment/docs/requests/aprender.md` — the original motivation request (commit 0c46b68)
- `/Users/guy/projects/mcp/motivation-experiment/datasets/motivation/Motivation Dataset - Sheet1.csv` — the first source (pinned in `prepare_motivation.py`)
- `/Users/guy/projects/mcp/motivation-experiment/datasets/motivation/Classification_Guide_Motivational_Function_Iris_Yuster_25Oct2026.docx` — §10.3 (weak and mixed evidence count as `other`; D-15), evidence levels

### Phase 12, which this phase builds on
- `.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-CONTEXT.md` — D-01..D-26 (D-23 excluded the region parameter, now taken up by D-05)
- `.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-HANDOFF.md` — the hand-off shape, prior-correction guidance, the false-assignment framing
- `.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-DEPLOY-EVIDENCE.json` — how the ze-kasher-dev motivation server was deployed and verified
- `.planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-ACCEPTANCE-REPLAY.json` — the acceptance-replay shape
- `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-RENAME-DEPLOY-EVIDENCE.json` — F-10-12-1..3
- `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-LEARNINGS.md` — login window, first-health 404, register growth

### Back office, crates, recipes
- `scripts/laya_train/prepare_motivation.py` — the ingest target (D-09..D-12, D-15, D-16); its docstring's declared-rules pattern and planted-sentinel selftest
- `scripts/laya_train/data.py` — `normalize`, `normalized_sha256`, `group_train`, `refuse_overlap`, `calibration_split` (the one normalizer)
- `models/decide/laya-motivation-v1/eval-probs.json` — the D-01 analysis input
- `justfile` — `laya-deploy-config`, `laya-weights-bucket` (uncommitted `region` diff), `laya-deploy`, `laya-deploy-verify`, `laya-deploy-selftest`, `laya-build-bootstrap`
- `crates/aprender-mcp-decide-motivation-lambda/` — the thin crate reused by both deployments (D-06)
- `crates/aprender-mcp-decide-lambda/` — the shared library and README
- `crates/aprender-core/tests/monorepo_invariants.rs` — FALSIFY-MONO-011 (unchanged by D-06)
- `contracts/decide-tool-boundary-v1.yaml` — the door (unchanged), `contracts/decide-apr-v1.yaml` — task.json label order

### Skills and memory
- `Skill("spike-findings-aprender")`: `references/aws-mcp-model-hosting.md`, `references/laya-decision-model.md`
- Project memory: Phase 4 verification status breaks gsd-tools state verbs (STATE is hand-edited); the Bash tool runs zsh

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `prepare_motivation.py`: already has declared rules, sha pins, write-once output, the no-text refusal style and a sentinel selftest. The enrichment source extends these rather than adding a second script.
- `laya-deploy-selftest`: the offline (AWS CALLS: 0) harness to extend for the region check (D-20).
- `laya-weights-bucket`: derives `aprender-decide-weights-<account>-<env>`, which is exactly the existing augury bucket (D-19), and already carries the uncommitted LocationConstraint branch for non-us-east-1 regions.
- The Phase 12 acceptance-replay probe, which lacks a bearer-token path (D-07).

### Established Patterns
- Rules are declared before the run, and a refusal is a result. Counts and sha256s are printed, never post text.
- Live steps go behind owner checkpoints (pmcp.run login, admin-UI auth), with read-only access checks first.
- A deployment is identified by its pinned sha plus where it runs. With a shared id (D-06), "where" becomes the (target, account, region) triple.

### Integration Points
- cargo-pmcp target selection (`cargo pmcp configure use augury-dev`, `PMCP_TARGET`) — the partner swaps a manifest copy per target; aprender's `laya-deploy-config` renders one config per server, which now needs a per-target variant.
- The partner consumes `motivationUrl`, `motivationSha` and the M2M secret (D-08).

</code_context>

<specifics>
## Specific Ideas

- The partner's augury context already names `https://aprender-mcp-decide-antisemite.eu-west.augury.pmcp.run/mcp`,
  and the augury weights bucket holds the antisemite artifact (uploaded 2026-10-24). Whether that server is live on
  augury is UNVERIFIED; do not claim it.
- The note must say plainly that sending only high-probability rows teaches the model nothing new about its own
  errors, and that the random control slice is what makes the next eval-free comparison honest.
- Because the expert sees the prediction (D-16), the hand-off should suggest the expert record their label before
  opening the probability column if the sheet layout allows it. That is a suggestion only, not a requirement.

</specifics>

<deferred>
## Deferred Ideas

- Retraining on the enrichment (motivation-v2) and its gate run: the next phase, once a real enrichment file exists.
- A second, field-drawn eval set held out from the enrichment (rejected for now in favour of a frozen eval, D-11): revisit at the retrain.
- Blind annotation (the owner chose prediction shown, D-16): revisit if the retrain shows anchoring.
- Deploying antisemitism-v1 on augury (the partner's runbook input I1): its own work; the artifact is present in the bucket, and the deployment is unverified.
- Auth on for the ze-kasher-dev servers.
- Wiring the experiment-coach motivation trio (`EC_MOTIVATION_*`): partner work.
- Everything still deferred from Phase 12 (server-side prior correction, per-source results, audit output, raising `calibration_temp_max`, thread context).

### Reviewed Todos (not folded)
- `aprender-train-gpu-ledger-tests-red.md`, `qwen35-9b-hybrid-forward-unimplemented.md`, `spike-laya-calibration-slice-and-temperature-cap.md`, `workspace-test-gate-blocked-by-renacer-validate.md`: keyword matches only. The calibration spike was already folded into Phase 12, and this phase runs no gate.

</deferred>

---

*Phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff*
*Context gathered: 2026-10-27*
