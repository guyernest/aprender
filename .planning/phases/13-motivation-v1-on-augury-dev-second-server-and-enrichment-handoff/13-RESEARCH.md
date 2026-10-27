# Phase 13: motivation-v1 on augury-dev (second server) and the first enrichment hand-off - Research

**Researched:** 2026-10-27
**Domain:** pmcp.run multi-tenant deployment of an existing decide artifact (cargo-pmcp 0.28.1, `just` recipes), OAuth M2M acceptance, and a second-source training-data ingest (`prepare_motivation.py`)
**Confidence:** HIGH on the deploy-path mechanics and the ingest design (read in source, several probes run this session); MEDIUM on the live OAuth enablement and M2M scope behaviour for the new host (not observable before the owner acts)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Owner decisions already made (2026-10-27, from ROADMAP.md, recorded as given)**
- **D-01:** The three-class model continues; the binary conspiratorial-or-not reframing is dropped. On the 100-row eval, the two functions are confused with each other once (and 0 times in the other direction; 1+2 / 0+0 on the other seeds). Every other error is function-vs-other, and the per-class AUC ordering flips by seed, so neither class is shown easier (analysis of 2026-10-27 from `models/decide/laya-motivation-v1/eval-probs.json` and the two seed dumps).
- **D-02:** Add a second server, not a move: the ze-kasher-dev pilot `aprender-mcp-decide-motivation` (us-east-1) stays live and untouched. (The roadmap's "own thin crate, register grows by one" is AMENDED by D-06.)
- **D-03:** The experiment team runs their own stored posts through the new server, and the expert annotates them. This phase collects no posts and writes no labels, and no silver label (a model prediction used as a label) enters the dataset.
- **D-04:** The review follow-up 12-17..12-19 stays parked; this phase runs on the current verifier and tooling.
- **D-05:** The uncommitted justfile `region` parameter (on `laya-deploy-config` and `laya-weights-bucket`), which Phase 12 D-23 excluded, is this phase's work. It is committed with its own check (D-20) and used for eu-west-1.

**Server id and auth**
- **D-06:** The augury server REUSES the id `aprender-mcp-decide-motivation` and the existing thin crate `crates/aprender-mcp-decide-motivation-lambda`. This amends the roadmap: no new crate, and the FALSIFY-MONO-011 deployment-unit register and the duplicate-bin allowlist stay unchanged. The two deployments differ by tenant host (expected `https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp`, inferred from the partner's pattern; trust the endpoint the deploy prints, not this line) and are told apart everywhere by the triple (pmcp.run target, AWS account, region), never by id alone. Every evidence record and the hand-off carry that triple. The planner must make sure the augury publish cannot overwrite or confuse the ze-kasher-dev deployment's local cargo-pmcp state (for example `.pmcp/deployment.toml` or the per-server endpoint record) or its weights object, and that every AWS-touching or pmcp-touching recipe is told explicitly which target it addresses. The existing server must still answer with sha d202f495… after the augury deploy (a read-only check). — **Reversibility:** costly — splitting the deployments later means a new thin crate, register growth, a redeploy under a new id and a URL change on the partner side.
- **D-07:** Auth is ON for the augury server, following the partner's runbook (`protectedClassifiers=true`). cargo-pmcp cannot render an auth-enabled descriptor for the pmcp-run target, so the server is published with auth off and the OWNER enables OAuth in the pmcp.run admin UI straight after the first publish, then creates the M2M (client-credentials) client. That is a human checkpoint (autonomous: false). The interval between publish and enablement is kept short and recorded. Acceptance then requires both an authenticated replay with the M2M client (the secret or token is supplied through a file, never on argv, and never printed or committed) and an unauthenticated initialize or call that is REFUSED. aprender's probe and verify tooling has no bearer path today, so adding one is in scope. The ze-kasher-dev servers stay auth off.
- **D-08:** The partner side is hand-off only. The owner receives the URL, the pinned sha and the M2M client details, for `motivationUrl` / `motivationSha` in `infra/context.augury-dev.json` and the `MotivationAuthSecretArn` secret contents. Client credentials go to the owner out of band and never into this repo or into evidence. This phase makes no write to motivation-experiment or to the partner's augury stack or secrets.

**Enrichment ingest and split**
- **D-09:** IMPLEMENT the second-source ingest. `prepare_motivation.py` gains `--enrichment PATH` (repeatable). The file is read in place in the partner repo and never copied here. Its sha256 and per-class / per-evidence-level counts are printed and pinned in the output. The no-text rule holds (a refusal names a row number, a field and at most a character count, never a value), and a planted-sentinel selftest runs on a SYNTHETIC enrichment fixture. No real enrichment file exists yet, so the ingest is proven on fixtures only, and the hand-off says so.
- **D-10:** A run WITHOUT `--enrichment` must write exactly the bytes v1's data dir holds today, including the owner-approved task.json (sha256 f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3). This is enforced by a check, not by care.
- **D-11:** Enriched rows go to TRAIN ONLY. `eval.jsonl` stays frozen and byte-identical to v1's, so the next gate is comparable with motivation-v1's numbers. An enriched text whose normalized identity (nfc-trim-ws-v1, the one normalizer in `data.py`) matches an eval text is DROPPED and counted. The accepted cost is that eval does not grow (about 14 conspiratorial rows).
- **D-12:** Cross-source conflicts extend the existing conflicting-labels rule: an enriched post whose normalized text matches a partner-file row (or another enriched row) with a DIFFERENT label drops the whole group, and it is counted and reported. There is no source precedence. Matching texts with the same label collapse to one row, as today.
- **D-13:** The enrichment format is versioned (for example `motivation-enrichment-v1`) and declared before any file exists. Its home is the planner's choice: under `contracts/` only if `pv validate` accepts it as a contract, otherwise a schema document plus the ingest's validator (CLAUDE.md: not a bash re-implementation of `pv`).

**Record schema and selection**
- **D-14:** The per-post record (the output of a run through the server) carries `post_id`, `source`, `text`, `labels` (served order), `probabilities` aligned to `labels`, `artifact_sha256`, the deployment triple (D-06), `tokens`, `truncated` and a timestamp. It includes the text because the expert reads it and the ingest normalizes and groups on it. The file lives only in the partner repo, and aprender reads it in place and never prints or copies it.
- **D-15:** The expert records one of the three labels PLUS the guide's evidence level (clear / weak / mixed). The ingest maps to the three served labels by the current rule (guide 10.3: in this dataset, weak and mixed evidence count as `other`), declared in the docstring before the run, and counts every (label, evidence level) cell. This keeps the weak/mixed decision open for the post-freeze retrain.
- **D-16:** The OWNER chose that the expert SEES the model's prediction while annotating, rather than a blind sheet. The enrichment format therefore carries `annotation_mode` (the value for this round, `prediction_shown`). The ingest records and counts it and does not act on it. The hand-off of whichever phase retrains on this data must state that its labels may be anchored to motivation-v1's predictions.
- **D-17:** The selection note recommends a stratified quota plus an uncertainty band plus a random control, with a default budget of about 300 posts. The three parts are: a quota by predicted top label with `conspiratorial` over-sampled (the thin class); an uncertainty band (low top-two margin, or a max-probability band straddling experiment-coach's `motivation_min_prob` 0.6); and a uniformly random control slice as an unbiased reference. The note fixes the proportions, the exact band definition and a seeded selection rule. The partner may change the budget. It also tells them not to send only high-probability rows.

**augury deploy path**
- **D-18:** A read-only probe first, then go. It checks that `sts` returns 783960389328 for profile `augury-dev`, that the cargo-pmcp target `augury-dev` resolves, and that the live augury tenant answers (counter-coach at `https://counter-coach.eu-west.augury.pmcp.run/mcp`). If all three pass, the deploy proceeds with no extra checkpoint beyond the owner's pmcp.run login (the token lives about an hour; autonomous: false) and the D-07 auth step. A tenant or region refusal at deploy time STOPS for the owner as a recorded finding, and nothing is faked.
- **D-19:** Weights REUSE the existing augury bucket `aprender-decide-weights-783960389328-dev` (eu-west-1). This was verified read-only on 2026-10-27: it holds `decide/aprender-mcp-decide-antisemite/2293bebb….apr`, uploaded 2026-10-24, and its name is exactly what `laya-weights-bucket` derives for this account and env. The artifact is uploaded content-addressed to `decide/aprender-mcp-decide-motivation/d202f495….apr`. Fallback, allowed by the owner: if access or permissions fail, create the bucket with `laya-weights-bucket ... region=eu-west-1`. On scale, the layout already scales: one bucket per account and env, a prefix per server, content-addressed objects, and each stack granted `s3:GetObject` on its own server's prefix only. More models add prefixes, not buckets or policies, and per-model isolation comes from that per-prefix grant.
- **D-20:** The region-parameter commit is proven offline in BOTH regions. `laya-deploy-selftest` (AWS CALLS: 0) is extended so that with region eu-west-1 the rendered config carries `region = "eu-west-1"`; the default us-east-1 output is byte-identical to today's; and a template with zero or two `region = "us-east-1"` lines is refused. The RED-turning mutation is re-run (CLAUDE.md verification rule 4: an extended guard needs re-mutation in the new scope).

**Carried forward from Phases 10 and 12 (unchanged)**
- 10,240 MB tier; deploy through `just laya-build-bootstrap` / `laya-deploy-config` / cargo-pmcp; the server id is the argument to the AWS-touching recipes; access checked read-only before live steps, and the executor never refreshes credentials.
- F-10-12-1: a server id's first edge `/health` on a tenant can 404 and trigger containment. Even with the same id, this is a new tenant, so the plan needs a bounded retry before containing, or an owner-run step. F-10-12-2: a sticky 503 was cleared by redeploying.
- The acceptance replay checks function and identity: one result per text, labels in task order, probabilities summing to 1, and the returned sha equal to the pin. Cold and warm timings are reported, not gated. The door is unchanged (8 texts, 800 built tokens, P 97). Never assert `tokens_planned == texts × P`.
- The partner's data, the record files and the enrichment files are read in place and never copied into this repo.

### Claude's Discretion
- The exact field names of the record and enrichment schemas, and where the format lives (D-13).
- The uncertainty-band definition and the quota proportions in the selection note (D-17), declared in the note.
- How the M2M credential reaches the probe and verify tooling (a file form), and how the unauthenticated refusal is probed (D-07).
- The mechanism that keeps the two deployments' local cargo-pmcp state apart (D-06).
- Evidence file names (`13-*-EVIDENCE.json`, following Phases 8, 10 and 12).

### Deferred Ideas (OUT OF SCOPE)
- Retraining on the enrichment (motivation-v2) and its gate run: the next phase, once a real enrichment file exists.
- A second, field-drawn eval set held out from the enrichment (rejected for now in favour of a frozen eval, D-11): revisit at the retrain.
- Blind annotation (the owner chose prediction shown, D-16): revisit if the retrain shows anchoring.
- Deploying antisemitism-v1 on augury (the partner's runbook input I1): its own work; the artifact is present in the bucket, and the deployment is unverified.
- Auth on for the ze-kasher-dev servers.
- Wiring the experiment-coach motivation trio (`EC_MOTIVATION_*`): partner work.
- Everything still deferred from Phase 12 (server-side prior correction, per-source results, audit output, raising `calibration_temp_max`, thread context).
</user_constraints>

<phase_requirements>
## Phase Requirements

No REQ ids are mapped to this phase (ROADMAP: "the plans implement CONTEXT decisions"). The planner maps plans to D-01..D-20.

| Decision | Research Support |
|----------|------------------|
| D-05, D-20 | Section "Q1": the exact uncommitted diff, what the selftest checks today, the extension design and the mutation set |
| D-06 | Section "Q2": cargo-pmcp target resolution, the per-id local files that WOULD be overwritten, the recommended isolation mechanism |
| D-07, D-08 | Section "Q3": probe bearer reality, M2M mint helper design, unauthenticated refusal probe, OAuth discovery facts verified live |
| D-09..D-12, D-15, D-16 | Section "Q4": structure of `prepare_motivation.py`, merge algorithm, byte-identity check (run this session) |
| D-13 | Section "Q5": `pv validate` accepts `kind: schema` (run this session) |
| D-18, D-19 | Sections "Q6" and "Probe results of this session" |
| F-10-12-1/2 | Section "Q7" |
| Evidence/hand-off | Section "Q8" |
</phase_requirements>

## Summary

The deploy half of this phase is mostly assembly of proven parts, but five facts in CONTEXT are wrong or incomplete and change the plan. (1) The probe tooling already has a bearer path: `examples/probe.rs` reads `APRENDER_DECIDE_PROBE_TOKEN` (and an unsafe `--bearer` argv form), and `laya-deploy-verify` inherits that environment. What is missing is only the M2M mint from a mode-600 file and the unauthenticated-refusal probe. (2) The augury tenant is not new to aprender: `aprender-mcp-decide-antisemite` is live there (HTTP 200 healthy, tools/list and tools/call answer 401, Lambda 10240 MB in eu-west-1, created 2026-10-25), deployed with this same justfile. That deployment already overwrote the id-keyed local files for the antisemite id (they now hold the augury record), which is the exact collision D-06 warns about. For the motivation id the id-keyed files still hold the ze-kasher-dev record and will be overwritten by the first augury `laya-deploy-config`/`laya-deploy`. (3) Target selection in this repo comes from the gitignored repo-root `.pmcp/active-target`, which currently says `dev`. The augury deploy must use `PMCP_TARGET=augury-dev` (env wins), never `cargo pmcp configure use`, which would repoint the default for every other recipe. (4) On augury, an unauthenticated `initialize` is ACCEPTED (200); only `tools/list` and `tools/call` answer 401. D-07's refusal check must therefore target `tools/list`/`tools/call`, not `initialize`. (5) `cargo pmcp login` does not exist in 0.28.1 (Phase 12's plan text names it); the command is `cargo pmcp deploy login`, and it follows `PMCP_TARGET`.

The ingest half is a bounded extension of an existing script. The v1 build is deterministic and reproduces byte-identically today (verified this session: no-flag run into a scratch dir, `cmp` identical for all three files). The safe design is to run the unchanged v1 `build()` first (eval and the partner train rows are fixed by it), then merge enrichment rows into train only, with eval-match drops first (D-11) and conflict-group drops second (D-12). `train.jsonl` rows may carry only `text` and `label` (`data.load_rows` refuses other keys), so provenance and counts go to a manifest file included in the write-once file set.

`pv validate` and `pv lint` accept the enrichment format as a `kind: schema` contract (scratch file validated this session), and the README contract-count gate tolerates lag, so a contract home is cheap. The recommended decomposition is two offline waves (justfile region commit with its selftest and mutation; state isolation plus token helper; ingest plus format; selection note and record schema) and one live, owner-gated wave.

**Primary recommendation:** Commit the region parameter first with a dedicated offline `_laya-region-selftest`; isolate per-target local state automatically by profile (`models/decide/augury-dev/…`) after archiving the ze-kasher-dev files; deploy with `PMCP_TARGET=augury-dev AWS_PROFILE=augury-dev AWS_REGION=eu-west-1` after a read-only target guard; accept with an M2M token minted from a mode-600 file into `APRENDER_DECIDE_PROBE_TOKEN` of the probe's environment only; build `--enrichment` as an additive merge over the unchanged v1 build.

## Probe results of this session (read-only, recorded exactly)

| # | What was run | Result | Tag |
|---|--------------|--------|-----|
| P1 | `aws sts get-caller-identity --profile augury-dev` | `783960389328 arn:aws:iam::783960389328:user/Guy` | [VERIFIED: aws cli, 2026-10-27] |
| P2 | `cargo pmcp configure show augury-dev` (offline) | `api_url = https://qxdfkl8vpc.execute-api.eu-west-1.amazonaws.com (source: target)`, `aws_profile = augury-dev`, `region = eu-west-1` | [VERIFIED: cargo pmcp 0.28.1] |
| P3 | `curl https://counter-coach.eu-west.augury.pmcp.run/health` | 200 `{"status":"healthy","serverId":"counter-coach",...}` | [VERIFIED: curl] |
| P4 | `curl https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/health` | 404 `{"status":"not_found","serverId":"aprender-mcp-decide-motivation"}` (the id is not on the tenant yet: this is the F-10-12-1 first-404 state) | [VERIFIED: curl] |
| P5 | `curl https://aprender-mcp-decide-antisemite.eu-west.augury.pmcp.run/health` | 200 healthy, `hasDeployment: true` (the augury antisemite server is LIVE; CONTEXT says unverified) | [VERIFIED: curl] |
| P6 | unauthenticated POST `initialize` / `tools/list` / `tools/call` to the augury antisemite `/mcp` | `initialize` 200; `tools/list` 401 `{"error":"invalid_request","error_description":"No Bearer token provided"}`; `tools/call` 401 same | [VERIFIED: curl] |
| P7 | `GET .../.well-known/oauth-protected-resource` and `.../oauth-authorization-server` on the antisemite host | 200; `token_endpoint` is `https://aprender-mcp-decide-antisemite.eu-west.augury.pmcp.run/oauth2/token`; `scopes_supported` include `https://<host>/mcp/read` and `/mcp/write`; issuer is the counter-coach Cognito pool; `POST /oauth2/token` with `grant_type=client_credentials` and no credentials answers 400 `{"error":"invalid_client"}` | [VERIFIED: curl] |
| P8 | `aws s3api list-objects-v2 --profile augury-dev --bucket aprender-decide-weights-783960389328-dev --prefix decide/`; `get-bucket-location` | one object, `decide/aprender-mcp-decide-antisemite/2293bebb…c776e.apr`, 846220868 bytes, 2026-10-24T07:14:37+00:00; bucket location `eu-west-1` | [VERIFIED: aws cli] |
| P9 | `aws lambda get-function-configuration --profile augury-dev --function-name aprender-mcp-decide-antisemite` | MemorySize 10240, LastModified 2026-10-25T19:05:24Z, `APRENDER_DECIDE_SHA256` 2293bebb…, role `arn:aws:iam::783960389328:role/pmcp-f2d50404-aprender-mcp-decide-ant-ExecutionRole-…` | [VERIFIED: aws cli] |
| P10 | `curl https://aprender-mcp-decide-motivation.us-east.true-mcp.com/health` (the ze-kasher-dev server) | 200 `{"status":"healthy","serverId":"aprender-mcp-decide-motivation",...}` | [VERIFIED: curl] |
| P11 | `~/.pmcp/credentials.toml` `expires_at` (value only) | `2026-10-27T06:35:19.694823+00:00`; it is 18:23Z now, so the token is EXPIRED and the owner must log in before any `cargo pmcp deploy` | [VERIFIED: file read] |

Note: the rtk hook mangles `aws` JSON output; use `rtk proxy aws ...` or `--output text` when scripting probes (memory: rtk hook).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Render per-server deploy config, region, IAM scope | Operator workstation (justfile + python) | — | Local, offline, gitignored output; the selftest proves it with AWS CALLS: 0 |
| Select tenant (pmcp.run target) | Operator workstation env (`PMCP_TARGET`) | `~/.pmcp/config.toml` | cargo-pmcp resolves env > flag > marker file; the marker is a per-checkout default, not a per-deploy choice |
| Place function, edge host, OAuth | pmcp.run platform (augury tenant) | AWS account 783960389328 | The platform deploys into the tenant's account/region; `[aws] region` only feeds the render params |
| Weights storage and scoped read | AWS S3 (augury bucket, eu-west-1) | Stack-declared IAM (`[[iam.statements]]`) | Per-server prefix grant is declared in the stack |
| OAuth enablement and M2M client | Owner in pmcp.run admin UI | Cognito pool (counter-coach pool, per P7) | cargo-pmcp cannot render an auth-enabled descriptor (partner DEPLOY.md); human checkpoint |
| Token mint for acceptance | Operator workstation helper | pmcp.run `/oauth2/token` | File in, token to the probe's environment only |
| Enrichment ingest, split, no-text rule | Operator workstation (`prepare_motivation.py`) | Partner repo (files read in place) | Deterministic data build; partner data never copied |
| Annotation | Partner / expert (out of repo) | — | D-03: this phase collects no posts |

## Standard Stack

No package is installed in this phase; everything below is already present.

### Core
| Tool | Version | Purpose | Why |
|------|---------|---------|-----|
| just | 1.46.0 | recipes (CLAUDE.md global rule: justfile) | `[VERIFIED: just --version]` |
| cargo-pmcp | 0.28.1 | publish to pmcp.run | `[VERIFIED: cargo pmcp --version]`; resolver proof on file is for 0.28.1 (`cargo_pmcp_version=0.28.1`) |
| uv + python `-I` | uv at `~/.local/bin/uv` | run `scripts/laya_train/*` frozen | existing pattern (`laya-prepare-motivation`) |
| numpy | as locked in `scripts/laya_train/uv.lock` | seeded split | already imported by `prepare_motivation.py` |
| pv | 0.69.3 (e070a7f76), at `target/release/pv` (not on PATH) | contract validate/lint | `[VERIFIED: ./target/release/pv --version]` |
| aws cli, curl, jq | present | read-only probes | `[VERIFIED: command -v]` |
| cargo-zigbuild, cargo-lambda, lockf, bashrs | present | bootstrap build, real-weights lock, shell lint | `[VERIFIED: command -v]` |

### Supporting (to be written, no dependency)
| Piece | Purpose | Note |
|-------|---------|------|
| `scripts/laya_deploy/m2m_token.py` (stdlib `urllib`, `json`, `os`, `stat`) | mint an M2M token from a mode-600 JSON file | Same JSON shape the partner stores in Secrets Manager: `{"client_id","client_secret","token_url"[,"scope"]}` |
| `_laya-state-dir` private recipe | per-profile local state directory | see Q2 |
| `_laya-target-check` private recipe | read-only guard: target/profile/region agree | see Q2 |

**Package Legitimacy Audit:** not applicable. This phase installs no external package (`uv --frozen`, stdlib only for the new helper). If the planner later adds one, it must run `gsd_run query package-legitimacy check` first.

## Q1. The justfile `region` diff, `laya-deploy-selftest`, and the D-20 extension

**The diff** [VERIFIED: `git diff justfile`, 18 insertions / 7 deletions, only these hunks]:
- `justfile:1718` `laya-deploy-config apr auth server env="dev" profile="ze-kasher-dev" region="us-east-1":` gains the trailing `region` parameter; the python heredoc receives it as `sys.argv[7]` and does (justfile:1760-1763):
  ```
  region_line = 'region = "us-east-1"'
  if text.count(region_line) != 1:
      sys.exit(f"{template}: expected exactly one '{region_line}' line; restore the template")
  text = text.replace(region_line, f'region = "{region}"')
  ```
  The template `crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template` has exactly one such line (line 54, `grep -c` = 1) [VERIFIED].
- `justfile:2742` `laya-weights-bucket env="dev" profile="ze-kasher-dev" region="us-east-1":` creates a missing bucket with `--region "$R"` and, only when `$R != us-east-1`, `--create-bucket-configuration "LocationConstraint=$R"`; the final echo prints the region.
- Neither `laya-deploy` nor `laya-upload` takes a region; they pass `--profile` only, so the AWS CLI uses the profile's default region. `~/.aws/config` `[profile augury-dev]` has `region = eu-west-1` [VERIFIED: file read]. This worked for the 2026-10-25 antisemite deploy and for P8/P9 today.

**What `laya-deploy-selftest` checks today** (justfile:2256-2736, all with `DRY_RUN=1` and an `aws` recorder first on PATH that must stay empty): recorder control call; fixture pack with golden sha `37d65159b2be0fa091aa840cd56c1a84b73c0bcd9e2df5906d1f8218f5448561`; deploy refusals (placeholder, sha-pin, resolver-proof, deploy/upload eligibility, retired id, no thin crate); the `_laya-crates-root-swap` table (success, forced failure, SIGTERM, two server names, absent root); EDGE HEALTH url/body/get tables (scripted curl shim, 404 retry bounds) and wiring; IAM table (11 variants) and GRANT table; the positive dry run (default-armed on laya-stance-64 inputs, disarmed with `LAYA_DEPLOY_SELFTEST_POSITIVE=0`, as Phase 12 did); bootstrap build; crates-root digest unchanged; final `AWS CALLS: 0`, `DEPLOY MARKERS: 0`, `DEPLOY SELFTEST OK`. It has NO region case: `regen()` (justfile:2313) calls `just laya-deploy-config "$A" off "$SRV"` only, and the weights-bucket recipe is called by no selftest. The server id comes from `LAYA_SELFTEST_SERVER`, default `aprender-mcp-decide-antisemite`.

**Recommended extension** (a new private recipe `_laya-region-selftest`, called by `laya-deploy-selftest` and runnable alone so the mutation runs cost seconds, not the multi-minute bootstrap build):
1. R1 default render vs R2 `... off "$SRV" dev ze-kasher-dev us-east-1`: `cmp` byte-identical, and `tomllib` `aws.region == "us-east-1"`.
2. R3 `... dev augury-dev eu-west-1`: `aws.region == "eu-west-1"` and a line diff against R1 of EXACTLY one line (the region line). This is the "byte-identical default" proof by construction: everything except the region line is equal.
3. R4/R5 zero and two `region = "us-east-1"` lines: needs a template override. Add `TEMPLATE="${LAYA_DEPLOY_TEMPLATE:-crates/...}"` honoured ONLY when `DRY_RUN=1` (the selftest exports it), so no live deploy can read an alternate template. Each case must exit non-zero with `expected exactly one 'region = "us-east-1"' line`, and the pre-seeded output file must be unchanged (sha before = after).
4. R6 reject malformed regions BEFORE interpolation. Today `region` is pasted into TOML text unvalidated (`f'region = "{region}"'`); a value containing a quote or newline would inject TOML. Validate `^[a-z]{2}(-[a-z]+)+-[0-9]$` in both recipes; add a `must_refuse` table (`EU-WEST-1`, `eu-west-1"`, empty, a value with a newline) and add `assert cfg["aws"]["region"] == region` next to the existing post-parse asserts (justfile:1777-1781).
5. Weights-bucket argv: a second shim dir with a scripted fake `aws` (own log, its own PATH scope like the curl shim at justfile:2453-2547) answering `sts get-caller-identity` with an account, `head-bucket` with failure, and recording `create-bucket` argv. Assert us-east-1 has no `--create-bucket-configuration`, eu-west-1 has `LocationConstraint=eu-west-1`. The main recorder must still read zero.
6. The new recipe has `exit`/verdict tokens, so `laya-gates-selftest`'s drift check (justfile:3195-3260) will demand a row in `scripts/laya_gates.tsv`: add `region-config` with the must-fail/must-pass cases above; run with `LAYA_GATES_ONLY=region-config` (prints `LAYA GATES ROWS OK`; the drift check still runs in full).

**RED-turning mutations (run AFTER the region commit, restore with `git checkout -- justfile` and confirm `git diff --quiet justfile`):**
| Mutation | Case that must turn RED |
|----------|------------------------|
| delete the `text = text.replace(region_line, ...)` line | R3 (eu-west-1 render) |
| `!= 1` → `< 1` | R5 (two lines accepted) |
| `!= 1` → `> 1` | R4 (zero lines accepted) |
| flip `[ "$R" = "us-east-1" ]` to `!=` in `laya-weights-bucket` | fake-aws argv table |
| change the default `region="us-east-1"` of `laya-deploy-config` to `eu-west-1` | R1/R2 |
| drop the region regex check | R6 |
Mutating in place is required because nested `just ...` calls resolve the default justfile; a mutated copy would not take effect. Each mutated run must print `DEPLOY SELFTEST FAILED` (or the sub-recipe's failure), and each restored run must print OK (CLAUDE.md verification rules 4 and 7: re-mutate in the new scope; case table).

Also commit hygiene: commit ONLY `justfile` (the working tree has other uncommitted `.planning/` changes) as its own commit before any live step.

## Q2. cargo-pmcp per-target state (D-06)

**How the target is chosen** [VERIFIED: source]:
- Resolution order: `PMCP_TARGET` env > `--target` flag > `.pmcp/active-target` file > none (`cargo-pmcp/src/main.rs:86` "Resolution order: PMCP_TARGET env > this flag > .pmcp/active-target file > none."; `commands/configure/resolver.rs` header "ENV > explicit --flag > active target > .pmcp/deploy.toml" for fields).
- `Commands::Deploy(_)` is "target consuming" (`main.rs:458-466`), so `cargo pmcp deploy login` and `deploy` both resolve the named target and inject `PMCP_API_URL`, `AWS_PROFILE`, `AWS_REGION` from the target ONLY when those env vars are not already set (`resolver.rs:378-396`: injects only `if f.source == TargetSource::Target`). A stale `AWS_PROFILE` or `AWS_REGION` exported in the executor's shell therefore WINS over the target. Guard it.
- The marker is found by walking up from the cwd to the first `Cargo.toml` (`workspace.rs:find_workspace_root`), i.e. the aprender REPO ROOT, not `crates/`. `.pmcp/active-target` at the repo root contains `dev` [VERIFIED: file read]; it is gitignored (`.gitignore:95 **/.pmcp/active-target`). This is why the swap recipe, which removes only `crates/.pmcp/active-target`, never disturbed target selection, and why the 2026-10-26 ze-kasher-dev deploy logged `source = ~/.pmcp/config.toml + .pmcp/active-target` (`models/decide/deploy-aprender-mcp-decide-motivation.log:3-7`, target `dev`, profile `ze-kasher-dev`, us-east-1).
- `~/.pmcp/config.toml` defines both targets [VERIFIED: file read]: `[targets.dev]` (`api_url = https://6vlog1csj4.execute-api.us-east-1.amazonaws.com`, `aws_profile = "ze-kasher-dev"`, `region = "us-east-1"`) and `[targets.augury-dev]` (`api_url = https://qxdfkl8vpc.execute-api.eu-west-1.amazonaws.com`, `aws_profile = "augury-dev"`, `region = "eu-west-1"`). With eight pmcp-run targets defined and no target selected, `configured_api_base_url()` returns `None` and cargo-pmcp falls back to its default API URL (`auth.rs:137-181`): an unselected deploy would not reach either tenant.
- The 2026-10-25 antisemite deploy to augury logged `→ Using target: augury-dev (pmcp-run) ... source = ~/.pmcp/config.toml + .pmcp/active-target` (`models/decide/deploy-aprender-mcp-decide-antisemite.log:3-7`), so at that time the marker had been set to `augury-dev` and later set back to `dev`. A marker flip is exactly the mechanism to avoid.

**Mechanism (recommended):** never run `cargo pmcp configure use`. Every augury-touching recipe invocation runs with `PMCP_TARGET=augury-dev AWS_PROFILE=augury-dev AWS_REGION=eu-west-1`, and `laya-deploy`, `laya-upload`, `laya-deploy-verify`, `laya-grant`, `laya-teardown` start with a read-only `_laya-target-check <profile> <target> <region>` that: (a) parses `~/.pmcp/config.toml` with `tomllib` and asserts `targets.<target>.aws_profile == profile` and `.region == region`; (b) refuses when `PMCP_TARGET` is unset or differs from the expected target for the profile, or when `AWS_PROFILE`/`AWS_REGION` are set and disagree; (c) asserts the rendered config's `[aws] region` equals the region; (d) for live steps, asserts `aws sts get-caller-identity --profile` equals the expected account. The deploy log's banner (`→ Using target: augury-dev`, `aws_profile = augury-dev`, `region = eu-west-1`) is copied into evidence as the after-the-fact proof (the pre-check is the gate; the banner is the receipt).

**The files the ze-kasher-dev motivation deploy wrote, and what the augury run would clobber** [VERIFIED: `ls models/decide`, `justfile` lines cited]. All are keyed by server id only and gitignored (`/data/`, `models/decide/` patterns are untracked):
| File | Written by | Clobber hazard |
|------|-----------|----------------|
| `models/decide/deploy-config-aprender-mcp-decide-motivation.toml` (account 816069129666 bucket, `region = "us-east-1"`) | `laya-deploy-config` (justfile:1746 `OUT=...`) | overwritten by the augury config (bucket 783960389328) |
| `models/decide/deploy-aprender-mcp-decide-motivation.log` | `laya-deploy` (justfile:2041) | overwritten |
| `models/decide/deploy-aprender-mcp-decide-motivation.state/{deployment.toml,stack.ts}` | `laya-deploy` (justfile:2042-2043 `SNAP=...; rm -rf "$SNAP"`) | DELETED then rewritten; `deployment.toml` is the only local record of the us-east endpoint `https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp`, and `laya-deploy-verify` reads its endpoint from this exact path (justfile:3021 `STATE="models/decide/deploy-$SERVER.state/deployment.toml"`) |
| `deploy-health-…motivation.json`, `deploy-probe-…motivation.log`, `eligibility-…motivation.log`, `eligibility-upload-…motivation.log`, `deploy-evidence-…motivation.json` | deploy / upload / verify recipes | overwritten |
| `models/decide/resolver-proof-aprender-mcp-decide-motivation.txt` | `laya-resolver-proof` | NOT target specific (the proof is for crate + cargo-pmcp version); reuse, no clobber risk |
| `crates/.pmcp/deployment.toml`, `crates/deploy/`, `.pmcp/active-target` | cargo-pmcp | restored byte-identically by `_laya-crates-root-swap` (justfile:1836 `STATE=(.pmcp/deploy.toml .pmcp/deployment.toml .pmcp/active-target deploy)`); no new hazard |
| repo-root `.pmcp/deployment.toml` (`aprender-setfit-predict`) | not touched by decide deploys | none |

Precedent that this is real, not hypothetical: the antisemite id's files in `models/decide/` today are the AUGURY record (`deploy-aprender-mcp-decide-antisemite.state/deployment.toml` endpoint `https://aprender-mcp-decide-antisemite.eu-west.augury.pmcp.run/mcp`; the config carries `region = "eu-west-1"`), so the ze-kasher-dev antisemite local record was already replaced by the 2026-10-25 augury deploy. Phase 12's `12-DEPLOY-EVIDENCE.json` `antisemite_before` still holds the us-east facts only because it was captured before.

**Recommended isolation (A'): automatic per-profile state directory.** A pure private recipe `_laya-state-dir profile` prints `models/decide` for `ze-kasher-dev` (so every default path is byte-for-byte unchanged) and `models/decide/<profile>` for any other profile (validated `[a-z0-9-]+`). `laya-deploy-config`, `laya-deploy`, `laya-upload`, `laya-deploy-verify` and the printed commands of `laya-teardown` derive CFG, LOG, SNAP, VLOG, health/probe/evidence paths from it. All of these recipes already take `profile`. Selftest table: ze-kasher-dev → `models/decide`; augury-dev → `models/decide/augury-dev`; `../x`, empty, uppercase → refused; and a must-pass proof that a default-profile `laya-deploy-config` writes the same path as before. This also fixes the augury-dev antisemite precedent for new runs without touching its current files.
Alternative (smaller, procedural, weaker): an env var `LAYA_STATE_DIR` defaulting to `models/decide`, set by hand for augury steps. It fails open if forgotten, so if chosen it needs a refusal when `PROFILE != ze-kasher-dev` and the variable is unset.

**Mandatory Wave-0 step before any augury recipe:** archive the ze-kasher-dev motivation files (the table above, plus `models/decide/laya-motivation-v1.apr` is NOT copied: record its sha) into `models/decide/ze-kasher-dev-archive-<date>/` with a `shasum -a 256` manifest, then re-verify the manifest after the phase. The archive is gitignored; its manifest sha goes into `13-DEPLOY-EVIDENCE.json`. It exists because the id-keyed files are otherwise the only local copy of the us-east endpoint record.

**Weights object:** the ze-kasher-dev object is in a different account's bucket (`aprender-decide-weights-816069129666-dev`, per the existing config). The augury upload writes `s3://aprender-decide-weights-783960389328-dev/decide/aprender-mcp-decide-motivation/<sha>.apr`, a different bucket and account, so no weights collision is possible. `laya-deploy` (6a) already refuses a config whose bucket is not `aprender-decide-weights-<account>-<env>` for the profile's account (justfile:2028-2032).

**Partner repo mechanism (for reference, not to copy):** it swaps `.pmcp/deploy.augury-dev.toml` over `.pmcp/deploy.toml` for the publish with `PMCP_TARGET=augury-dev` and restores the dev bytes (`docs/DEPLOY.md` "Publish targets", `.pmcp/deploy.augury-dev.toml` header). Its own doc admits the generated `deployment.toml` is shared between targets and "has not been observed" to be safe (DEPLOY.md:100); aprender's per-id snapshot copy plus the new per-profile directory is stronger.

**Region support on pmcp.run:** the platform places the function by tenant, not by `[aws] region` (that value only feeds `RenderParams.region`, `deployment/targets/pmcp_run/deploy.rs:354`). eu-west-1 deployability is therefore proven by an existing tenant, not hoped for: the augury antisemite Lambda exists in eu-west-1 at 10240 MB (P9).

## Q3. Acceptance replay, bearer path, M2M token, unauthenticated refusal (D-07)

**What exists** [VERIFIED: `crates/aprender-mcp-decide-lambda/examples/probe.rs` header and USAGE; `src/probe.rs:193-194`]:
- The Phase 12 acceptance replay is the probe itself, not a separate script: `probe --url <ep> --apr <apr> --replay N [--replay-from K]`, `--replay-over-limit`, and the identity probe (initialize, tools/list, tools/call). `12-ACCEPTANCE-REPLAY.json` (schema `laya-acceptance-replay-v1`) was assembled by hand from those JSON lines; its `auth_posture` reads `off (no bearer was sent; APRENDER_DECIDE_PROBE_TOKEN unset)`.
- The probe ALREADY accepts a bearer: `const TOKEN_ENV: &str = "APRENDER_DECIDE_PROBE_TOKEN";`, `--bearer <token>` (argv, so do not use), and `src/probe.rs:193-194` `if let Some(token) = self.bearer { req = req.bearer_auth(token); ...`. The header says the token is never printed. `laya-deploy-verify` runs `"$PROBE" ...` with the inherited environment, so exporting the variable authenticates its cold samples as well. CONTEXT D-07's "no bearer path today" is therefore only half true: the env path exists; the file/M2M path and the refusal probe do not.

**What to add (minimum):**
1. `scripts/laya_deploy/m2m_token.py FILE` (stdlib only). Refuses unless: the file is a regular file with mode 600 or 400 (`os.stat`; the partner's `smoke.sh` does the same with `stat -c %a || stat -f %Lp`), it parses as a JSON object with non-empty string `client_id`, `client_secret`, `token_url`, `token_url` is `https` (loopback http only in its own selftest), and the host matches the expected endpoint host (so a client file for another server is not used by mistake). Mints with `grant_type=client_credentials` as a FORM BODY (client_id, client_secret in the body, `scope` only if present in the JSON); on 4xx prints only the HTTP status and the OAuth `error` code to stderr, never the body or any credential. The token goes only to stdout for the caller to place in an env var; stderr and evidence carry only `host`, HTTP code and `expires_in`. Token shape verified for the sibling server: HTTP 200 with keys `access_token`, `expires_in`, `token_type` (partner evidence `~/projects/mcp/counter-coach/prod-m2m-run-2.txt`; `[CITED: motivation-experiment docs/DEPLOY.md "M2M secret contents"]`). The partner file does not say form vs HTTP Basic; the partner's own smoke uses form body first and falls back to Basic. Mirror that, record which one worked.
2. A recipe `laya-deploy-accept` (or extend `laya-deploy-verify`) that runs: `TOKEN="$(python3 scripts/laya_deploy/m2m_token.py "$M2M_FILE")"` then `APRENDER_DECIDE_PROBE_TOKEN="$TOKEN" probe ...` for the identity probe and the replays, and the `LAYA_M2M_FILE` path as an environment variable (never an argument carrying the secret; the PATH to the file is not secret). `set +x` inside the recipe (partner smoke does too).
3. The unauthenticated-refusal probe: POST `tools/list` and `tools/call` with NO bearer and require HTTP 401 for both; also record that `initialize` answers 200 (an observation, NOT a pass criterion). Evidence (P6/P7): on augury today `initialize` is open, `tools/list` and `tools/call` answer 401 with `www-authenticate: Bearer error="invalid_request", error_description="No Bearer token provided"`. A check written as "unauthenticated initialize is refused" would FAIL on a correctly configured server. Plain `curl -s -o /dev/null -w '%{http_code}'` is enough (no secret involved); do it from the recipe so it is repeatable.
4. A credential-free OAuth-enabled check that the executor can run before the owner reports done: `GET https://<host>/.well-known/oauth-protected-resource` returns 200 with `"resource": "<endpoint>"` once OAuth is on (P7 for the antisemite host; the motivation host answers 404 now). Poll it every 15 s after publish to measure the auth window (below).

**Token endpoint shape:** `https://<host>/oauth2/token` on the pmcp.run proxy (P7: `token_endpoint` in the discovery document), scopes `https://<host>/mcp/read` and `/mcp/write` (P7). The M2M client must be granted the scope of the NEW host. The partner doc says one counter-coach client "serves both" counter-coach and the antisemite classifier because they share a pool, but marks it unconfirmed ("confirm with `just smoke augury-dev m2m`"); for the motivation host assume a client is needed whose scopes include `https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp/read` `[ASSUMED]` and let the helper's 400 `invalid_scope`/`invalid_client` name the problem.

**Owner commands (checkpoint):** `cargo pmcp login` is NOT a subcommand in 0.28.1 (`error: unrecognized subcommand 'login'`, run this session); Phase 12's plan text names it. The login is `PMCP_TARGET=augury-dev cargo pmcp deploy login` (`deploy login` exists; `Commands::Deploy` is target-consuming, `main.rs:458-466`). Cached evidence that the login follows the target: `~/.pmcp/pmcp-run-config.json` is keyed to the augury API url with an `eu-west-1` Cognito domain. The credential file `~/.pmcp/credentials.toml` has ONE `[pmcp-run]` slot: logging in to augury replaces the ze-kasher-dev session; any later ze-kasher-dev pmcp operation needs a fresh login (note it in the hand-off). OAuth enablement can also be done with `cargo pmcp deploy oauth enable --server aprender-mcp-decide-motivation --copy-from aprender-mcp-decide-antisemite` (`--shared-pool`/`--copy-from` options exist per `oauth enable --help`) `[ASSUMED]` equivalent to the admin UI path; D-07 names the admin UI, so offer the CLI only as an owner option, and `cargo pmcp deploy oauth status --server <id>` as a read-back.

**The auth window.** Publish ends with the function reachable without credentials (the deploy log of the antisemite run says "No authentication required. Anyone can access this server."). Record: `T_publish_done` (the DEPLOYED line), `T_owner_reports_oauth_on`, and `T_first_401` (first poll where unauthenticated `tools/list` returns 401); `auth_window_s = T_first_401 - T_publish_done` is an upper bound. The recipe's own identity probe runs unauthenticated during the window (justfile:2106-2107), which is correct only because auth is still off then. Landmine: after OAuth is on, ANY re-run of `laya-deploy` runs that probe without a token and would CONTAIN a healthy function; any post-enablement redeploy (F-10-12-2's remedy) must export `APRENDER_DECIDE_PROBE_TOKEN` first, and whether a redeploy with `--no-oauth` leaves OAuth enabled is unobserved `[ASSUMED]`: re-check `oauth status` and the discovery document afterwards.

## Q4. `prepare_motivation.py` / `data.py`: where `--enrichment` plugs in (D-09..D-12, D-15, D-16)

**Structure** [VERIFIED: file read]: `read_rows` (CSV, 1-based row numbers, refusals `csv-*` naming row/field/count only); `build(csv_path)` groups by `data.normalized_sha256`, drops mixed-label groups (`mixed`), keeps first-seen order (`kept`), draws eval by `np.random.RandomState(SPLIT_SEED)` stratified per class with `eval_count = ceil(n/4)` in integers, writes `task.json` (`json.dumps(..., ensure_ascii=False, indent=2) + "\n"`), `train.jsonl`, `eval.jsonl` through `common.jsonl_bytes`; `check_pin(summary)` against `PINNED` (`csv_sha256` `bc61c76b…4ae7`, 395 rows); `check_approved(files)` against `APPROVED_TASK_SHA256 = "f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3"`; `write_once` (from `prepare_stance`) refuses an existing non-empty `--out` unless every file this run writes already holds those bytes, and refuses any other non-dotfile; `selftest()` plants `SENTINEL = "ZQXJ-sentinel-motivation-7mw"` in synthetic rows and asserts it never reaches captured stdout/stderr on every refusal path, with a control that the capture sees a sentinel. `main()` (lines 499-538) prints `prepared <dir> (<status>): <json summary>` plus per-file line counts and sha256, then `PREPARE OK`.

**Constraints from `data.py`** [VERIFIED: `data.py:172-222, 263-273`]: `load_rows` accepts ONLY keys `text` and `label` (`ROW_KEYS`), so enrichment provenance cannot ride in `train.jsonl`. `group_train` raises `train-conflicting-labels` on a normalized-text group with more than one label, so conflicts must be resolved before writing (D-12). `calibration_split` re-draws the calibration slice from whatever `train.jsonl` holds (seed 13), so adding rows changes the slice: expected and not an ingest concern.

**Verified byte-identity baseline (run this session):** `uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --out <scratch>` wrote files that `cmp` identical to `data/decide/motivation-v1/`: `task.json` `f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3`, `train.jsonl` `f1d84f19ec38ed85356bcbcd1eae6ef8ad07fb97e014f61a17d37adc0d68d378` (295 lines), `eval.jsonl` `ee95d71fcb5d90d515b9fc6741e5df51087e763fa7f251ace83d9f11d5ccd575` (100 lines; eval counts conspiratorial 14, other 63, social_affiliative 23). The scratch copy was deleted. `--selftest` passes (`PREPARE MOTIVATION SELFTEST OK`).

**Design: an additive merge AFTER the unchanged v1 build.**
1. `files, summary = build(csv_path)` exactly as today; add one internal helper that also returns `kept` group hashes, `eval_hashes`, and `mixed_hashes` WITHOUT changing the bytes it returns. Eval and the partner train rows are thereby fixed before any enrichment is read, which is the only way to keep `eval.jsonl` frozen (an enrichment row must never enter the stratified draw, which indexes `kept` and so would change the permutation).
2. `read_enrichment(path)` (new, same refusal style: `DataError("enrichment-…", …)` naming row number, field, character count only; `refuse_unencodable` from `data.py` for text). Format: JSON Lines, first line a header object `{"format": "motivation-enrichment-v1", "annotation_mode": "prediction_shown", ...}`, then one object per post (see Q5). Size cap and line-count cap before parsing. Reject an enrichment path resolving under `REPO` (`enrichment-in-repo`): D-09 says never copied here, and `/data/` is the only ignored home.
3. Mapping (D-15, declared in the docstring before the run). The guide's own text [VERIFIED: docx paragraphs, read this session] says "The class Other covers three situations ... no evidence of either motivational function, weak evidence only ..., and clear evidence of both (mixed)", "Weak evidence ... does not, on its own, assign a class" and "Posts with weak evidence of the other function keep their class". Two consequences for the format: (a) the `evidence` value must describe the evidence for the RECORDED function, otherwise a clear-social post with weak conspiratorial evidence (which keeps its class) would map to `other`; (b) a fourth value `none` is needed for `other` with no evidence, since D-15's clear/weak/mixed cannot express it. Rule: `label in {social_affiliative, conspiratorial}` and `evidence == clear` → that label; `evidence in {weak, mixed}` → `other`; `label == other` with `evidence in {none, weak, mixed}` → `other`; `label == other` with `evidence == clear`, or a function label with `none`, is an incoherent pair → refuse (`enrichment-label-evidence`, row and field only). Count every `(label, evidence)` cell, and every `annotation_mode` (recorded, never acted on, D-16). Note CONTEXT cites the rule as "guide 10.3"; in the docx 10.3 is the example-set section and the Other-class text sits in the class definitions (paragraph indices 285-288 of the extracted text): cite the quoted sentences, not "10.3", in the docstring.
4. Group over the union: partner train groups, then enrichment groups in file order (files in command-line order), keyed by `normalized_sha256`.
   - first, eval match → drop the enriched row, count `dropped_eval_overlap` (D-11; eval rows are never modified, whatever the enriched label);
   - second, match with a partner group that was DROPPED as mixed in the v1 build (`mixed_hashes`) → drop and count `dropped_partner_conflict_group`, so enrichment cannot resurrect a group the partner file already refused;
   - third, group by hash across partner train + enrichment: all one label → collapse to one row (the partner's exact text if the partner has one, else the first enriched text) and count `collapsed_same_label`; more than one label → drop the WHOLE group, including the partner train row (D-12; there is no precedence) and count `dropped_conflict_groups` / `dropped_conflict_rows`;
   - new rows appended after the partner's surviving train rows, in file order.
5. Assert before writing: `group_train(train)`, `refuse_overlap(train, eval)`, `calibration_split(...)`, all three criteria present in train, eval bytes equal the base build's eval bytes (and, for the pinned CSV, equal `ee95d71f…`), task.json still approved.
6. Output: the same three files plus an `enrichment-manifest.json` INCLUDED IN `files` so `write_once` accepts it (a foreign file would be refused). Contents: counts only and shas: each enrichment file's sha256, per-class and per-evidence-level counts, the cell table, `annotation_mode` counts, all drop/collapse counts, `eval_sha256` and `base_train_sha256` (the v1 shas), the final `train_sha256`. `train.py` reads only named files (`train.py:748-762`), so the manifest is inert for training. Refuse `--enrichment` with the default `--out` (the v1 dir): write-once would refuse anyway because train differs, but say so up front with a rule name.
7. Print the enrichment sha256 and counts; "pinned in the output" is satisfied by the manifest plus the printed line. Do not hard-code an enrichment pin in the script (no real file exists); optionally accept `--enrichment-sha256 HEX` per file, refusing a mismatch, so the retrain run can pin it.

**Where the no-flag path stays byte-identical:** `main()` calls the merge only `if args.enrichment`. Add three constants `V1_TASK_SHA256` (already `APPROVED_TASK_SHA256`), `V1_TRAIN_SHA256 = "f1d84f19…78"`, `V1_EVAL_SHA256 = "ee95d71f…575"` and, when the CSV is the pinned file (not `--no-pin`) and no enrichment is given, assert the written files hash to them (`check_v1_identity(files)`, exit 1 `PREPARE FAILED`). That turns D-10 into a check inside every run, not a one-off command. RED mutation: change the merge so it perturbs ordering or drops a row even when `--enrichment` is absent, or insert a stray byte into `jsonl_bytes`; the identity check must fail. Also keep the selftest's existing "two builds are byte-identical" case.

**Selftest additions (synthetic fixture, sentinel in every text, no real post):** enrichment fixture helpers beside `_synthetic_csv`; cases: v1 files unchanged (cmp) when no flag; eval byte-identical with a flag; an enriched row whose text equals an eval text (including a whitespace/NFC variant) is dropped and counted; same text + same label as a partner train row collapses; same text + different label drops the group and the partner's train row; two enriched rows in two files with different labels drop the group; a text matching a partner mixed group stays dropped; weak/mixed evidence maps to other; every incoherent label/evidence pair refused; annotation_mode counted and not acted on; header refusals (wrong `format`, missing `annotation_mode`, unknown key, duplicate key); row refusals name row number and field only; the sentinel never appears in stdout/stderr on success and on every refusal path, with the existing control; `--out` equal to the v1 dir refused; enrichment under `REPO` refused; manifest contains no text (assert the sentinel is absent from it). Also add the selftest to `laya-train-selftest`'s loop or CI fragments? It is currently in neither (`grep` of justfile/ci/gates: only the recipe wrapper at justfile:1337-1340); run it directly in the plan's verify commands, and note that `ci/explicit-test-commands.d/` fragments are how a new test becomes CI-visible.

**justfile trap:** `laya-prepare-motivation *args` expands `{{args}}` unquoted (justfile:1337-1340). The partner file names contain spaces (`Motivation Dataset - Sheet1.csv`), so an `--enrichment` path with spaces would word-split. Make the recipe `[positional-arguments]` and call `"$@"`, or document direct `uv run` use for enrichment runs.

## Q5. D-13: where the format lives

[VERIFIED: `crates/aprender-contracts/src/schema/kind.rs:54-69`] `ContractKind` has variants `Kernel, Registry, ModelFamily, ModelFamilyVariant, Tokenizer, TrainingLoop, PretrainingCorpus, TrainingPreconditionGate, CorpusAssembly, Pattern, Schema, Kaizen, BeatBenchmark`. `Schema` is documented "a generic reference/schema document — exempt from provability, validated only for `metadata.id`, `metadata.version`, `metadata.description`, and `metadata.references`"; `Registry` is "a data registry (lookup tables, enum definitions, config bounds)".

[VERIFIED: ran `./target/release/pv validate` and `pv lint` on a scratch `kind: schema` contract in the scratchpad] `0 error(s), 0 warnings` / `Contract is valid` and `Result: PASS` (8 armed gates passed; ontology-dependent gates are "not armed" outside the repo's `contracts/`). `pv` is not on PATH here; call `./target/release/pv` (or `cargo run -p aprender-contracts-cli`). Inside the real `contracts/` tree the ontology gates (sigma, relations, valid-under) arm, so a real file should carry `metadata.valid_under: world: committed` like `decide-apr-v1.yaml` and be linted in place: `./target/release/pv lint contracts/` at execution time (not provable here).

**Recommendation:** `contracts/motivation-enrichment-v1.yaml`, `kind: schema`, declaring: `format_id`, header fields, row fields with types and allowed values (`label` in the three served names in served order; `evidence` in `clear|weak|mixed|none`; `annotation_mode` in `prediction_shown|blind`), the mapping table (D-15), the refusal rule names, and the D-11/D-12 merge semantics as prose with the same rule names the ingest prints. The ingest's validator is the single implementation (no second parser in bash). Cost: one more contract; the README count gate is `claimed <= actual` ("lag allowed, overstatement RED", `readme_contract.rs:158-205`), so the count need not move in this phase (re-derive it if touched: 1904 files today). The contract's `references` point to `scripts/laya_train/prepare_motivation.py`. Fallback if the owner dislikes a contract for a data format: a section in `scripts/laya_train/README.md` plus the validator, with the same field table.

**Record schema (D-14) and enrichment rows, recommended field names (Claude's discretion):**
- Record (written by the partner's run through the server; lives only in the partner repo): `post_id`, `source`, `text`, `labels`, `probabilities`, `artifact_sha256`, `deployment` = `{pmcp_target, aws_account, region}` (the D-06 triple, plus `endpoint`), `tokens`, `truncated`, `scored_at`, and recommended additions `selection_slice` (`control|uncertainty|quota`) and `selection_seed` so the retrain can weight or hold out the control slice.
- Enrichment row: `post_id` (joins back to the record), `text`, `label` (served names), `evidence`, and `selection_slice` echoed. Header: `format`, `annotation_mode`, `artifact_sha256` (the model whose predictions the expert saw), `guide` (designation string). `text` is repeated in the enrichment row so the ingest never has to join two partner files.

## Q6. Weights upload path (D-19) and the live sequence

- `laya-upload apr run data base server env profile` (justfile:2778-2833): runs `laya-verify` on the exact file FIRST, then derives `KEY="decide/$SERVER/$H.apr"`, `BUCKET="aprender-decide-weights-${ACCOUNT}-${ENV}"` from the PROFILE's account, skips if the head-object size equals, else `aws s3 cp --profile "$P" --only-show-errors --sse AES256`, then re-checks size. No region argument: it relies on the profile's region (`eu-west-1` for augury-dev) and S3 redirects.
- Phase 12's upload to ze-kasher-dev lost its connection mid-multipart twice (F-12-12-1) and succeeded on the third try with `AWS_MAX_ATTEMPTS=10 AWS_RETRY_MODE=standard` set in the environment (`12-DEPLOY-EVIDENCE.json` `upload.tries`). `~/.aws/config` `[profile augury-dev]` already sets `s3 max_concurrent_requests = 2`, `multipart_chunksize = 16MB`, `multipart_threshold = 16MB`. Set the retry variables from the start; the object is 846,196,612 bytes (`12-DEPLOY-EVIDENCE.json` `uploaded_line`) and the recipe is idempotent.
- The live command shape (Phase 12's, from `live_attempts[0].command`): `lockf -k -t 300 /tmp/aprender-laya-real-weights.lock just laya-deploy models/decide/laya-motivation-v1.apr models/decide/laya-motivation-v1 data/decide/motivation-v1 $BASE aprender-mcp-decide-motivation dev <profile>`, with `$BASE` = `~/.cache/huggingface/hub/models--convaiinnovations--laya/snapshots/55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851` (present: `encoder/ tokenizer/ typed-decisions/`). The executor passes `augury-dev` as the profile and exports the three environment variables of Q2.
- Budget (corrects Phase 10's pessimism): Phase 12's complete `laya-deploy` ran 22:10:00Z to 22:14:36Z, 4 min 36 s, including the re-verify (`12-DEPLOY-EVIDENCE.json` `live_attempts[0]`), versus the "about 22 minutes" in Phase 10's learnings for antisemitism-v1 (a much larger eval). The token gate of Phase 12 (`expires_at` re-read, at least 45 minutes left) fits comfortably; keep it. The upload (verify plus transfer) took 310 s on the final try.
- The `laya-weights-bucket` fallback is not needed (P8: the bucket exists, eu-west-1). `s3:GetObject` is granted by the stack on `decide/aprender-mcp-decide-motivation/*` only (`_laya-iam-check`, `laya-grant`), and `laya-deploy`'s own `laya-grant` read-back asserts it (the log line is quoted in Phase 12's evidence).
- Triple evidence that cannot be confused with the us-east server: `aws lambda get-function-configuration --profile augury-dev --function-name aprender-mcp-decide-motivation --query FunctionArn` must read `arn:aws:lambda:eu-west-1:783960389328:function:aprender-mcp-decide-motivation` `[ASSUMED format, standard ARN]`; plus the endpoint host `eu-west.augury.pmcp.run` from the deploy printout and the banner's target/profile/region.

## Q7. F-10-12-1/2 handling

- F-10-12-1 is ALREADY implemented since Phase 12 plan 12-04 (commit `4a6ff5ee4`): `laya-deploy` calls `_laya-edge-health-get` (justfile:2091), which retries HTTP 404 only, bounded by `LAYA_EDGE_HEALTH_TRIES` (default 40), `LAYA_EDGE_HEALTH_SLEEP` (15 s), `LAYA_EDGE_HEALTH_MAX_SECONDS` (900 s cap that also accounts for the 30 s curl timeout). Any other status or a transport failure contains at once. Phase 12's ze-kasher-dev run recorded `"health_404_retries": 1`. Phase 10 observed about 8 minutes for a brand-new id on a tenant, which the 900 s default covers; the augury motivation id is brand-new on its tenant (P4 returns 404 today), so expect the retry branch and record the retry count. Do not change the bounds.
- F-10-12-2 (sticky `503 Server is in error state`, cleared only by re-running the deploy step, `10-RENAME-DEPLOY-EVIDENCE.json`): the identity probe loop retries that exact refusal `LAYA_EDGE_SETTLE_TRIES` times (default 8, 15 s apart, about 2 minutes) and otherwise contains (justfile:2102-2118). Phase 8 saw the edge settle in under a minute. If the 8 tries are exhausted the recipe CONTAINS (reserved concurrency 0). Plan for it as a recorded owner step, not an executor retry: the recovery is `aws lambda delete-function-concurrency --profile augury-dev --function-name aprender-mcp-decide-motivation` and a fresh `laya-deploy` (about 5 minutes of re-verify and deploy). Budget the token window for one such re-run, and remember the post-OAuth probe-token rule from Q3.

## Q8. Evidence conventions and the hand-off shape

- Evidence files are `<phase>-<NAME>-EVIDENCE.json` with a `schema` string (`laya-deploy-evidence-v2`, `laya-acceptance-replay-v1`, `laya-deploy-owner-reply-v1`), fields `phase`, `plan`, `server`, `profile`, `region`, `env`, `auth`, `tier_mb`, `artifact_path`, `artifact_sha256`, `config`, `selftest` (with `aws_calls`), `upload` (with `tries`), `live_attempts[]` (with `token_recheck_line`, `command`, `recipe_lines`, `health_404_retries`, `identity_probe`, `negative_control`), `findings[]` (`F-<phase>-<plan>-<n>`: `id`, `title`, `evidence`, `status`), `live_function_after`, `measured_on`, and an owner reply file `<phase>-<NAME>-OWNER-REPLY.json` for each human checkpoint. Replay file: `identity`, `replays[]`, `batch_of_4`, `expected_count_refusal`, `requester_tests` (each `{result, proof}`), `timings_reported`, `partner_repo_unchanged`/`partner_repo.head_before`.
- Phase 13 files (names are discretion): `13-REGION-EVIDENCE.json` (offline D-20: selftest last line, `aws_calls: 0`, mutation table with the observed failing lines), `13-PROBE-EVIDENCE.json` (P1-P11 above re-run at execution), `13-DEPLOY-EVIDENCE.json`, `13-OAUTH-OWNER-REPLY.json`, `13-ACCEPTANCE-REPLAY.json` (schema `laya-acceptance-replay-v1` plus `auth_posture: on`, `unauthenticated_refusal: {tools_list: 401, tools_call: 401, initialize: 200 (observed, not asserted)}`, `token: {token_url_host, http_code, expires_in}` and nothing else), `13-INGEST-EVIDENCE.json` (selftest case count, the v1 byte-identity table, mutation results), `13-HANDOFF.md`. EVERY file carries `"triple": {"pmcp_target": "augury-dev", "aws_account": "783960389328", "region": "eu-west-1", "endpoint": "<as printed by the deploy>"}`.
- `12-HANDOFF.md` shape (section list read this session): 0 lead numbers; 1 status; 2 identity (sha, labels in order, recipe_id, task sha); 3 endpoint and auth posture; 4 items for the partner to confirm; 5 task and limits as deployed; 6 data and gate; 7-9 bake-off, prior, shortcuts; 10 acceptance replay and timings; 11 scope and deferred; 12 dates; numbers table with record names. Phase 13's hand-off: identity (unchanged sha d202f495…, same labels order), the two-deployment table (triple per row, so the partner cannot confuse them), `motivationUrl` / `motivationSha` values, the M2M handling instructions (out of band; never in this repo), what the ingest proves (fixtures only), the D-16 anchoring statement, the selection note, the record/enrichment format pointers, and the single-slot `credentials.toml` note.
- Never put the M2M `client_id`/`client_secret`, a token, or a post text in evidence. The partner's tree is read-only: capture `git -C <partner> rev-parse HEAD` and `status --porcelain` before and after (Phase 12 did, `partner_repo_unchanged: True`).

## Architecture Patterns

### Deploy and acceptance flow (data/control flow)

```
 owner terminal                      operator workstation (executor)                    tenant: augury-dev (783960389328, eu-west-1)
 --------------                      ------------------------------                    ---------------------------------------------
                                     [offline] region commit + _laya-region-selftest
                                     [offline] archive ze-kasher-dev local files (manifest)
                                     [read-only] P1-P5,P8,P10 probes  ---------------->  sts / health / s3 ls / (unauth curl)
                                     _laya-target-check (config.toml vs env vs profile)
                                     laya-upload (laya-verify first) ----------------->  s3://aprender-decide-weights-783960389328-dev/
                                                                                             decide/aprender-mcp-decide-motivation/<sha>.apr
 PMCP_TARGET=augury-dev
 cargo pmcp deploy login  ---------> ~/.pmcp/credentials.toml ([pmcp-run], 1 slot)
                                     laya-deploy (PMCP_TARGET env, lockf, swap root)
                                       cargo pmcp deploy --no-oauth ------------------>  pmcp.run API (augury) -> Lambda 10240 MB
                                       laya-grant (read-only IAM check) --------------->  role policy
                                       _laya-edge-health-get (404 retried) ------------>  GET /health  -> 200 serverId
                                       probe identity (NO bearer; auth still off) ------>  POST /mcp
                                     T_publish_done recorded; poll .well-known (404 -> 200 when OAuth on)
 admin UI: enable OAuth,
 create M2M client; write JSON
 to mode-600 file outside repo ----> T_owner_reports_oauth_on
                                     unauth POST tools/list, tools/call  ------------->  401 (initialize stays 200: observed)
                                     m2m_token.py FILE ------------------------------->  POST <host>/oauth2/token (form)
                                       APRENDER_DECIDE_PROBE_TOKEN in the probe's env
                                     probe identity, --replay 1/7/4, --replay-over-limit, verify cold samples ->  /mcp (Bearer)
                                     read-only: ze-kasher-dev identity probe -------->  (us-east.true-mcp.com, sha d202f495…)
                                     archive manifest re-check; evidence + hand-off
```

### Recommended file layout

```
justfile                                   # region commit; _laya-state-dir, _laya-target-check, _laya-region-selftest, laya-deploy-accept
scripts/laya_deploy/m2m_token.py           # stdlib mint helper with --selftest (loopback stub server)
scripts/laya_train/prepare_motivation.py   # + --enrichment, check_v1_identity, merge, selftest cases
scripts/laya_gates.tsv                     # + region-config (and state-dir / m2m rows if those recipes are gate-shaped)
contracts/motivation-enrichment-v1.yaml    # kind: schema (D-13)
.planning/phases/13-.../13-SELECTION-NOTE.md, 13-RECORD-SCHEMA.md (or one doc), 13-*-EVIDENCE.json, 13-HANDOFF.md
models/decide/augury-dev/                  # gitignored per-profile state (new)
models/decide/ze-kasher-dev-archive-<date>/  # gitignored archive + MANIFEST
```

### Pattern: selection note (D-17), recommended concrete values (Claude's discretion; to be declared in the note)

Budget 300 posts, no post sent twice (dedupe by `data.normalized_sha256` identity, the same normalizer, so a stored post that equals a training text is not re-annotated for nothing). Recommend 120 uncertainty (40 percent), 105 quota (35 percent: 45 predicted `conspiratorial`, 30 `social_affiliative`, 30 `other`), 75 random control (25 percent); the partner may scale the budget keeping the shares. Uncertainty band: top-two margin `p1 - p2 < 0.20` OR `0.45 <= p1 <= 0.75` (straddles `motivation_min_prob` 0.6 on both sides). Selection is order-independent and reproducible: rank posts by `sha256(f"{seed}:{post_id}")` (seed fixed in the note) within each slice's eligible pool, draw control FIRST from the whole scored pool, then uncertainty, then quota, never repeating a post. Record `selection_slice` on every row. Say plainly (CONTEXT specifics) that high-probability-only sampling teaches the model little about its own errors and that only the control slice supports an unbiased estimate on the field population; the other slices are biased by design and need inverse-inclusion weights if used for population estimates. Rationale for the quota split: training prevalence is conspiratorial 41 / 295 train rows (`[VERIFIED: prepare_motivation.py output]` train counts conspiratorial 41, other 188, social_affiliative 66), the thin class. A small reference implementation plus a synthetic selftest is recommended so the prose rule cannot be read two ways.

### Anti-patterns to avoid
- **`cargo pmcp configure use augury-dev`** in this checkout: rewrites the repo-root marker that every other recipe depends on.
- **Asserting "unauthenticated initialize is refused"**: false on this platform (P6).
- **Trusting id alone** in evidence or file names (the antisemite precedent already lost its us-east local record).
- **Resolving enrichment conflicts by source precedence** (D-12 forbids it) or letting the stratified eval draw see enrichment rows.
- **Putting provenance keys in `train.jsonl`**: `data.load_rows` refuses unknown keys.
- **Passing the token with `--bearer`** (argv) or logging a token-bearing command.

## Don't Hand-Roll

| Problem | Don't build | Use instead | Why |
|---------|-------------|-------------|-----|
| Contract validation | a bash/yq checker for the enrichment format | `pv validate` / `pv lint` on a `kind: schema` contract + the ingest's validator | CLAUDE.md: never re-implement `pv` |
| Text identity / grouping | a second normalizer | `data.normalize`, `data.normalized_sha256`, `data.group_train`, `data.refuse_overlap` | one normalizer (nfc-trim-ws-v1) |
| Write-once output | custom file writing | `prepare_stance.write_once` | existing refusal semantics and dotfile handling |
| Bearer on the wire | a new HTTP client | the probe's existing `APRENDER_DECIDE_PROBE_TOKEN` | already implemented, never printed |
| Edge 404 retry | a new retry loop | `_laya-edge-health-get` | bounded, tested, in the gates table |
| Target/region truth | parsing `cargo pmcp configure show` text | `tomllib` over `~/.pmcp/config.toml` (schema_version 1) or `configure list --format json` | stable, offline |
| Hostile region strings | `re`-free interpolation | one validated regex shared by both recipes | TOML injection otherwise |

**Key insight:** every hazard here is a place where an identifier (server id, marker file, path with spaces, evidence label) silently stands in for a tuple (target, account, region) or a set of files; fix it where the tuple is formed, once.

## Runtime State Inventory

The phase is not a rename, but D-06 reuses an id across tenants, which has the same failure class (state that a source edit does not move).

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | `models/decide/*-aprender-mcp-decide-motivation.*` hold the ze-kasher-dev record (config with account 816069129666 bucket, state dir with the us-east endpoint, logs, evidence). The antisemite id's equivalents already hold the augury record. | archive + per-profile directory (Q2); no data migration |
| Live service config | ze-kasher-dev Lambda `aprender-mcp-decide-motivation` (us-east, auth off) answers sha d202f495… (P10); augury tenant has no motivation id yet (P4). OAuth config on the new id lives in the pmcp.run admin UI/Cognito, not git. | read-only identity probe before and after; record owner enablement time |
| OS-registered state | none found (no launchd/cron for these recipes; `/tmp/aprender-laya-real-weights.lock` is a transient `lockf` file, absent now) | none |
| Secrets/env vars | `~/.pmcp/credentials.toml` single `[pmcp-run]` slot (expired 06:35Z); `AWS_PROFILE`/`AWS_REGION`/`PMCP_TARGET` in the executor shell win over the target; M2M client JSON must live in a mode-600 file outside the repo | login by owner; env guard; token helper never prints |
| Build artifacts | `crates/.pmcp/` (setfit state) and `crates/deploy/` swapped and restored by `_laya-crates-root-swap`; `models/decide/swap-backup/` is empty now (a leftover would refuse the next deploy); `~/.pmcp/pmcp-run-config.json` cache is per api_url, 1 h TTL | confirm `swap-backup` empty before and after; no action on the cache |

## Common Pitfalls

### Pitfall 1: id-keyed local files overwritten (D-06)
**What goes wrong:** the first augury `laya-deploy-config`/`laya-deploy` replaces the ze-kasher-dev config, endpoint record and logs; `rm -rf "$SNAP"` deletes the state dir.
**Why:** every path is `models/decide/<kind>-$SERVER.<ext>` with no tenant component (justfile:1746, 2041-2043).
**Avoid:** archive with manifest, then per-profile state directory (Q2); selftest asserts the default profile's paths did not move.
**Warning signs:** a `deploy-*.state/deployment.toml` whose endpoint host does not match the profile.

### Pitfall 2: wrong tenant selected silently
**What goes wrong:** with no `PMCP_TARGET`, the repo-root marker `dev` picks the ze-kasher-dev API; with a stale `AWS_PROFILE`, the target's profile loses.
**Avoid:** `_laya-target-check` before every live recipe; copy the banner into evidence.

### Pitfall 3: the refusal probe tests the wrong method
**What goes wrong:** an unauthenticated `initialize` returns 200 on a correctly protected server.
**Avoid:** assert 401 on `tools/list` and `tools/call` (P6); record `initialize` as an observation.

### Pitfall 4: containment of a healthy function after OAuth is on
**What goes wrong:** a re-run of `laya-deploy` after enablement probes without a token, fails, and `contain` sets reserved concurrency 0.
**Avoid:** export `APRENDER_DECIDE_PROBE_TOKEN` for any post-enablement deploy run; note in the plan that the F-10-12-2 remedy (redeploy) is such a run.

### Pitfall 5: expired or single-slot pmcp.run session
**What goes wrong:** the session expired at 06:35Z today (P11); logging in to augury replaces any ze-kasher-dev session.
**Avoid:** token gate (45 minutes) immediately before each live attempt; the executor never logs in; correct command is `cargo pmcp deploy login`, not `cargo pmcp login`.

### Pitfall 6: `{{args}}` loses quoting
**What goes wrong:** `just laya-prepare-motivation --enrichment "/path with spaces/x.jsonl"` word-splits (justfile:1337-1340).
**Avoid:** `[positional-arguments]` and `"$@"`.

### Pitfall 7: resurrecting a dropped group, or moving eval
**What goes wrong:** merging enrichment before the split changes the permutation and eval; merging after, an enriched row matches a partner group that v1 dropped as mixed and brings it back.
**Avoid:** merge after the unchanged build, with `mixed_hashes` and `eval_hashes` from it (Q4).

### Pitfall 8: region interpolation
**What goes wrong:** `region` is pasted into TOML unvalidated; a bad value passes the one-line check and corrupts the config or the CLI call.
**Avoid:** regex-validate in both recipes; assert it after parsing.

### Pitfall 9: the Bash tool is zsh and rtk filters output
**What goes wrong:** `PIPESTATUS`, word splitting and guard case tables misbehave (memory); `aws` JSON and `diff` status are mangled by the rtk hook.
**Avoid:** run recipe bodies in their `#!/usr/bin/env bash` shebang (they are); in scripted probes use `rtk proxy` or `--output text`, `cmp` instead of `diff` for status, and never read `$?` through a pipe (CLAUDE.md rule 1).

### Pitfall 10: gate-table drift
**What goes wrong:** new gate-shaped recipes (`exit 1|2|3`, verdict tokens) without a `scripts/laya_gates.tsv` row fail `laya-gates-selftest`'s drift check.
**Avoid:** add the rows in the same plan that adds the recipes.

### Pitfall 11: shared-root concurrency
**What goes wrong:** the swap refuses while `models/decide/swap-backup/crates` exists; two deploys cannot overlap.
**Avoid:** one live recipe at a time; check the directory is empty before and after.

## Code Examples

### Target guard (read-only, offline) - sketch
```bash
# Source: derived from ~/.pmcp/config.toml (VERIFIED file) and cargo-pmcp resolver precedence (VERIFIED source)
python3 - "$PROFILE" "$EXPECT_TARGET" "$EXPECT_REGION" <<'PY'
import os, sys, tomllib, pathlib
profile, target, region = sys.argv[1:4]
cfg = tomllib.loads((pathlib.Path.home() / ".pmcp" / "config.toml").read_text())
t = cfg["targets"][target]
assert t["aws_profile"] == profile and t["region"] == region, (target, "does not match profile/region")
if os.environ.get("PMCP_TARGET") != target:
    sys.exit(f"REFUSED target: PMCP_TARGET must be {target}")
for k, want in (("AWS_PROFILE", profile), ("AWS_REGION", region)):
    if os.environ.get(k) not in (None, "", want):
        sys.exit(f"REFUSED target: {k} is set and is not {want}")
print(f"target ok: {target} -> {profile} {region}")
PY
```

### Authenticated probe without argv exposure - sketch
```bash
# Source: examples/probe.rs TOKEN_ENV (VERIFIED); helper to be written
set +x
TOKEN="$(python3 scripts/laya_deploy/m2m_token.py "$LAYA_M2M_FILE")" || exit 1
APRENDER_DECIDE_PROBE_TOKEN="$TOKEN" "$PROBE" --url "$ENDPOINT" --apr "$APR" --expect-sha256 "$H"
unset TOKEN
```

### Unauthenticated refusal check - sketch
```bash
for m in tools/list tools/call; do
  body="{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$m\"$( [ "$m" = tools/call ] && printf ',"params":{"name":"classify","arguments":{"texts":["x"]}}' )}"
  code="$(curl -s -m 30 -o /dev/null -w '%{http_code}' -X POST "$ENDPOINT" \
          -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' -d "$body")"
  [ "$code" = 401 ] || { echo "REFUSED auth-open: unauthenticated $m answered HTTP $code" >&2; exit 1; }
done
```
(The `tools/call` of one trivial text is refused before any compute; if the server were open it would run one cheap classify, so run `tools/list` first and abort without sending `tools/call` when it is not 401.)

### Merge core (ingest) - sketch
```python
# Source: design per Q4; data.py helpers are VERIFIED (normalize, normalized_sha256, group_train, refuse_overlap)
base_files, base_summary, ctx = build_v1(csv_path)          # unchanged bytes + ctx.eval_hashes, ctx.mixed_hashes, ctx.train_rows
groups = {}                                                  # hash -> {"text", "labels", "sources"}
for t, y in ctx.train_rows: ...                              # partner survivors first (CSV order)
for src in enrichment_sources:                               # file order
    for n, row in src.rows:
        h = normalized_sha256(row.text)
        if h in ctx.eval_hashes:      drops["eval_overlap"] += 1; continue
        if h in ctx.mixed_hashes:     drops["partner_conflict_group"] += 1; continue
        groups.setdefault(h, ...).add(row.label_idx)
# conflict groups (more than one label) are removed whole; same-label groups collapse to one row
```

## State of the Art

| Old | Current | Impact |
|-----|---------|--------|
| Phase 12 plan text: `cargo pmcp login` | `cargo pmcp deploy login` (0.28.1) | owner instructions must use the latter |
| Single `/health` GET (F-10-12-1) | bounded 404-only retry in `laya-deploy` (since 12-04) | no new work, only record the count |
| One tenant (ze-kasher-dev) | two tenants, one id | tuple identity, not id identity |
| Partner doc: "no publish to augury-dev has been run yet" (DEPLOY.md:64, partner side) | aprender's antisemite IS published there (P5/P9) | the partner's own runbook input I1 partly satisfied; not this phase's claim to make |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The motivation M2M client needs scopes of the NEW host (`https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp/read`); the counter-coach client may not suffice | Q3 | Acceptance blocked until the owner creates/grants a client; helper surfaces `invalid_scope` |
| A2 | `cargo pmcp deploy oauth enable --server <id> --copy-from <antisemite id>` is equivalent to the admin-UI enablement and shares the counter-coach pool | Q3 | Owner uses the UI instead; no phase impact |
| A3 | A redeploy with `--no-oauth` after OAuth was enabled might reset OAuth | Q3 / Pitfall 4 | If it does, the auth window reopens; re-check `oauth status` and the discovery document after any redeploy |
| A4 | The augury Lambda's `FunctionArn` is `arn:aws:lambda:eu-west-1:783960389328:function:aprender-mcp-decide-motivation` | Q6 | Triple evidence cites the real ARN read at execution instead |
| A5 | The token endpoint for the new host is `https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/oauth2/token` (pattern verified on the antisemite host, P7; the new host answers 404 until deployed) | Q3 | The owner's admin UI value (Cognito hosted domain) is the fallback, as the partner doc says |
| A6 | Credentials go as a form body (partner smoke tries form first, then Basic) | Q3 | helper retries Basic once and records which worked |
| A7 | Phase 12 timing (4.6 min `laya-deploy`) holds on the augury tenant | Q6 | token gate still protects; a longer run only needs a fresh login |
| A8 | Recommended selection-note numbers (120/105/75, band 0.20 and 0.45-0.75, quotas 45/30/30) | Q5 pattern | the note is discretionary (D-17); owner may adjust |
| A9 | The guide's evidence taxonomy needs a `none` value for `other` with no evidence | Q4 | If the expert records only clear/weak/mixed, `none` is simply unused |

## Open Questions (RESOLVED)

1. **Does the existing counter-coach M2M client work for the new host?** Known: pool shared (P7 issuer is the counter-coach pool); scopes are per resource-server host. Unclear until the owner enables OAuth. Recommendation: plan for a new client; make the helper's failure message precise.
   RESOLVED: 13-08 Task 3. The owner creates an M2M client for the new host that accepts form-body credentials. The executor proves it with an authenticated identity call minted `(auth form)`. An `invalid_scope` / `invalid_client` / `m2m-auth-method` refusal is a checkpoint naming only the status and the OAuth error code.
2. **Where does the owner want the M2M file?** Recommendation: a mode-700 temp dir outside the checkout (partner runbook step form), path passed via `LAYA_M2M_FILE`, removed after.
   RESOLVED: a mode-600 (or 400) file outside the checkout, passed only as `LAYA_M2M_FILE`. 13-06 Task 1: `scripts/laya_deploy/m2m_token.py` checks the opened descriptor (regular, owner, exact mode). 13-08 Task 3: a path under the repository is refused, and the path is kept only in the gitignored `models/decide/augury-dev/m2m-file-path`.
3. **Should `laya-deploy-verify` (4 cold samples) run authenticated in this phase?** Phase 12 ran it (`12-DEPLOY-EVIDENCE.json live`). Recommendation: yes, with the token env, because it is the only source of cold-sample evidence on the new tenant; it is reported, not gated.
   RESOLVED: 13-09 Task 1. It runs authenticated (`LAYA_M2M_FILE=... just laya-deploy-verify ... augury-dev 2`, re-exec'd under the helper from 13-06). The timings are reported, not gated.
4. **Contract vs doc for the format (D-13)?** Recommendation: contract (`kind: schema`); owner may prefer a doc. Neither blocks the ingest.
   RESOLVED: 13-02. `contracts/motivation-enrichment-v1.yaml`, `kind: schema`, accepted by `pv validate` and `pv lint`, with structured bounds that the 13-03 and 13-05 validators mirror.
5. **Does the owner want the selection helper shipped?** D-03 forbids collecting posts here, not shipping a rule; recommend a tiny reference implementation with a synthetic selftest.
   RESOLVED: 13-05. `scripts/laya_train/select_motivation.py` is a reference implementation with a synthetic-record selftest and `--print-rule`. `13-SELECTION-NOTE.md` embeds the generated rule section.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| AWS profile `augury-dev` | upload, grant, verify, evidence | yes (P1) | account 783960389328 | none (owner must refresh SSO/keys; executor never refreshes) |
| pmcp.run session for augury | `cargo pmcp deploy` | NO (expired 2026-10-27T06:35Z) | — | owner `PMCP_TARGET=augury-dev cargo pmcp deploy login` right before the deploy; blocking |
| cargo-pmcp | publish | yes | 0.28.1 | — |
| cargo-zigbuild, cargo-lambda | bootstrap build | yes | — | — |
| Laya base snapshot | `laya-verify` | yes | `55cf4c4e…` | — |
| `data/decide/motivation-v1`, `models/decide/laya-motivation-v1{,.apr}` | verify, upload, probe `--apr` | yes (807 MB artifact) | sha d202f495… (pinned) | — |
| Partner repo (read-only) | enrichment fixtures are synthetic; partner CSV for identity check | yes | HEAD `0c46b68` expected | — |
| pv | contract validation | yes at `target/release/pv`, not on PATH | 0.69.3 | `cargo run -p aprender-contracts-cli` |
| lockf, just, uv, bashrs, jq, curl | recipes | yes | just 1.46.0 | — |
| Owner admin UI (OAuth, M2M client) | D-07 | owner-only | — | blocking checkpoint |

**Missing with no fallback:** the pmcp.run session (owner), and the OAuth/M2M step (owner).
**Missing with fallback:** `pv` on PATH.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Python `--selftest` under `uv run --project scripts/laya_train --frozen python -I` (numpy + pyyaml only); `just` selftest recipes with an AWS recorder; `pv validate`/`lint`; `cargo test -p aprender-core --test monorepo_invariants` |
| Config | `scripts/laya_train/pyproject.toml`; `scripts/laya_gates.tsv`; `ci/explicit-test-commands.d/*.cmd` (a new test is CI-dark until a fragment names it) |
| Quick run | `uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --selftest` (about a second); `just _laya-region-selftest` (seconds, planned) |
| Full suite | `LAYA_DEPLOY_SELFTEST_POSITIVE=0 just laya-deploy-selftest` (minutes; builds the bootstrap with zigbuild) and `LAYA_GATES_ONLY=region-config,state-dir just laya-gates-selftest` (drift check in full) |

### Decision -> Test Map
| Decision | Behavior | Type | Command | Exists |
|----------|----------|------|---------|--------|
| D-05/D-20 | eu-west-1 render, us-east-1 byte-identical, zero/two-line refusal, region regex, weights-bucket argv | offline (AWS CALLS: 0) | `just _laya-region-selftest`; `just laya-deploy-selftest` | NO, Wave 0 |
| D-20 mutation | each mutation in Q1 turns it RED, restore turns it OK | mutation | procedure of Q1 with `git checkout -- justfile` | NO |
| D-06 | per-profile paths; default paths unchanged; archive manifest re-verified after the phase | offline + evidence | `just _laya-state-dir ze-kasher-dev` equals `models/decide`; `shasum -a 256 -c MANIFEST` | NO |
| D-06 | target/profile/region agreement guard refuses mismatches | offline table | `_laya-target-check` must-refuse cases (unset `PMCP_TARGET`, wrong `AWS_PROFILE`, wrong region) | NO |
| D-06 | existing server still sha d202f495… | live read-only | `probe --url https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp --apr models/decide/laya-motivation-v1.apr --expect-sha256 d202f495…` before and after | recipe exists |
| D-06 | no register growth | integration | `cargo test -p aprender-core --test monorepo_invariants`; `bash scripts/check_duplicate_bin_names.sh` | yes |
| D-07 | token helper: refuses mode 644, non-https, missing keys, wrong host; never prints secret/token on stderr; 4xx prints status only | unit with loopback stub | `python3 scripts/laya_deploy/m2m_token.py --selftest` | NO |
| D-07 | unauthenticated tools/list and tools/call are 401; initialize recorded | live | refusal snippet (Q3) | NO |
| D-07 | authenticated replay: identity, replay 1/7/4, over-limit refusal | live | `probe ... --replay N` with `APRENDER_DECIDE_PROBE_TOKEN` | probe exists |
| D-09..D-12,D-15,D-16 | the ingest case table (Q4) with sentinel and control | unit (synthetic) | `prepare_motivation.py --selftest` | extend |
| D-10 | no-flag bytes equal v1 | check inside every run + external cmp | `prepare_motivation.py --out <scratch>` then `cmp` against `data/decide/motivation-v1/{task,train,eval}` (verified identical 2026-10-27); shas `f74a3ad3…`, `f1d84f19…`, `ee95d71f…` | partial |
| D-10 mutation | perturbing the no-flag path turns the identity check RED | mutation | insert a stray byte in `jsonl_bytes`, run | NO |
| D-11 | eval byte-identical with a flag; overlaps dropped | unit | selftest cases | NO |
| D-13 | format contract valid | contract | `./target/release/pv validate contracts/motivation-enrichment-v1.yaml && ./target/release/pv lint contracts/` | NO |
| D-17 | selection rule deterministic, quotas, slices disjoint | unit (if the reference is shipped) | `select_records.py --selftest` | NO |
| F-10-12-1 | 404 retried then contained | offline | existing EDGE HEALTH get table | yes |

### Sampling Rate
- Per task commit: the quick commands for the touched module.
- Per wave merge: the full suite line above plus `cargo test -p aprender-core --test monorepo_invariants`.
- Phase gate: all offline commands green, every mutation observed RED and restored GREEN, then the live checks recorded in evidence.
- Never read a status through a pipe (CLAUDE.md verification rule 1): capture to a log, then `rc=$?`.

### Wave 0 Gaps
- [ ] `_laya-region-selftest` (+ `LAYA_DEPLOY_TEMPLATE` under `DRY_RUN=1`, region regex, fake-aws shim) and its gates row.
- [ ] `_laya-state-dir`, `_laya-target-check` recipes and tables; archive step and manifest.
- [ ] `scripts/laya_deploy/m2m_token.py` with `--selftest`.
- [ ] `prepare_motivation.py` enrichment cases, `check_v1_identity`, manifest; `--selftest` extended.
- [ ] `contracts/motivation-enrichment-v1.yaml`.
- [ ] A `ci/explicit-test-commands.d/` fragment if CI visibility of the new Python selftest is wanted (take a free ordinal; never renumber; `ci_run_explicit_test_commands.sh --list` exits 2 on macOS, assert the fragment directly).

### Live checks (human-gated, evidence in `13-*-EVIDENCE.json`)
Read-only probes P1-P5/P8/P10 re-run; upload; owner login; deploy; auth window timestamps; owner OAuth enablement; unauthenticated 401s; authenticated identity + replays; verify cold/warm timings (reported, not gated); ze-kasher-dev identity after; archive manifest re-check; partner repo `HEAD`/`status` unchanged.

## Security Domain

### Applicable ASVS categories (security_asvs_level 1, enforcement on)
| ASVS Category | Applies | Control |
|---------------|---------|---------|
| V2 Authentication | yes | OAuth on the server, M2M client-credentials for acceptance; open window recorded; token via file/env, never argv |
| V3 Session Management | limited | short-lived access token (`expires_in`), minted per run, not cached to disk |
| V4 Access Control | yes | per-prefix `s3:GetObject` only (existing `_laya-iam-check`/`laya-grant`); per-profile target guard prevents acting on the wrong account |
| V5 Input Validation | yes | enrichment file validator (closed key sets, enums, size caps), region regex, state-dir name regex |
| V6 Cryptography | limited | no new crypto; TLS to `https` token and MCP hosts only; sha256 pins |
| V7/V8 Error handling, data protection | yes | the no-text rule (counts, row numbers, character counts only), sentinel selftest, post text never copied or committed, manifest holds counts only |

### Known threats
| Pattern | STRIDE | Mitigation |
|---------|--------|-----------|
| Secret or token on argv / in logs / in evidence | Information disclosure | file input mode 600, env-only token for the child, `set +x`, stderr carries host+status only, evidence has no credential field; a selftest asserts absence |
| Open endpoint between publish and OAuth enablement (10 GB cold start per call) | Denial of service / cost | measure and record `auth_window_s`; owner enables immediately; URL not published until enablement |
| Wrong-tenant deploy or evidence | Tampering / Repudiation | target guard, banner in evidence, triple in every record |
| Config injection through `region` | Tampering | regex validation + post-parse assert |
| Enrichment poisoning (conflicting labels, eval leakage) | Tampering | D-11/D-12 rules, drop counts printed, eval frozen and asserted |
| Post text leakage via refusals | Information disclosure | refusals name row, field, count only; sentinel selftest with a capture control |
| Path confusion (`--enrichment` under the repo, `--out` = v1 dir) | Tampering | `enrichment-in-repo` and `out-is-v1` refusals |
| Unauthenticated `initialize` mistaken for "open" or "closed" | Spoofing (false assurance) | assert on `tools/list`/`tools/call` |

## Sources

### Primary (HIGH confidence)
- `justfile` (this repo, read this session): recipes at 1337-1340, 1718-1786, 1824-1901, 1947-2119, 2122-2239, 2256-2736, 2742-2833, 2844-2878, 3000-3120; `git diff justfile`.
- `crates/aprender-mcp-decide-lambda/examples/probe.rs`, `src/probe.rs:193-194`; `crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template`; `crates/aprender-mcp-decide-motivation-lambda/{Cargo.toml,src/main.rs}`.
- `scripts/laya_train/prepare_motivation.py`, `data.py:45-310`, `common.py`, `prepare_stance.py:161-178`, `train.py:741-790`.
- cargo-pmcp 0.28.1 source at `~/Development/mcp/sdk/rust-mcp-sdk/cargo-pmcp/src` (`main.rs:70-110, 458-466, 600-650`, `commands/configure/resolver.rs`, `workspace.rs`, `deployment/targets/pmcp_run/auth.rs:100-260`, `deploy.rs:340-360`) and `cargo pmcp ... --help` output.
- `crates/aprender-contracts/src/schema/kind.rs:54-97`; `crates/aprender-core/tests/readme_contract.rs:158-205`.
- `.planning/phases/12-*/12-DEPLOY-EVIDENCE.json`, `12-ACCEPTANCE-REPLAY.json`, `12-HANDOFF.md`; `.planning/phases/10-*/10-LEARNINGS.md`, `10-RENAME-DEPLOY-EVIDENCE.json`.
- `models/decide/deploy-aprender-mcp-decide-{motivation,antisemite}.{log,state/deployment.toml}`; `~/.pmcp/config.toml` and `credentials.toml` (expiry only), `~/.aws/config` (augury profile region/s3 settings).
- Live read-only probes P1-P11 (table above), including the OAuth discovery documents.

### Secondary (MEDIUM confidence)
- `[CITED: /Users/guy/projects/mcp/motivation-experiment/docs/DEPLOY.md lines 57-110]` and `.pmcp/deploy.augury-dev.toml` header; `scripts/smoke.sh` M2M mint description; `~/projects/mcp/counter-coach/prod-m2m-run-2.txt` (token response keys).
- `[CITED: Classification_Guide_Motivational_Function_Iris_Yuster_25Oct2026.docx]` paragraphs on the Other class, weak and mixed evidence.

### Tertiary (LOW confidence)
- Behaviour after OAuth is enabled for redeploys, scopes for the new host, form-vs-Basic: flagged `[ASSUMED]` above.

## Metadata

**Confidence breakdown:**
- Deploy-path mechanics and the state collision: HIGH (source read, files read, antisemite precedent observed).
- Region/selftest extension and mutation set: HIGH (code read; the selftest itself was not run, to stay read-only and avoid the multi-minute bootstrap build and file writes).
- Ingest design and byte-identity: HIGH (baseline reproduced; design not yet implemented).
- OAuth/M2M for the new host: MEDIUM (pattern verified on the sibling host only).
- Selection-note numbers: MEDIUM (discretionary).

**Research date:** 2026-10-27
**Valid until:** about 7 days for live facts (pmcp.run tenant state, token expiry), 30 days for repo mechanics.
