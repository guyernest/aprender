# Phase 13 hand-off: motivation-v1 on augury-dev (second server) and the first enrichment round

**For:** the owner, to forward to the partner (motivation-experiment / experiment-coach) and to the expert. **Written:** 2026-10-28 (plan 13-10).
**Delivery:** this file lives in the aprender phase directory. Nothing was written into the partner repository, the partner's augury stack or its secrets.

Every number and status below is read from a structured record. The two tables marked as generated (the lead table of section 0 and the `Numbers and their records` table at the end) are written by a script that reads each value at its record key, and the plan's verify reads every row again and compares it. Prose figures repeat values from those tables. No credential, token, post text or M2M file path appears in this file. The only AWS account ids in it are the two deployment accounts.

## 0. Lead numbers

<!-- generated:lead begin -->
| claim | value | record | key |
|---|---|---|---|
| artifact sha256 (the pin, motivationSha) | d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd | 13-DEPLOY-EVIDENCE.json | artifact_sha256 |
| augury endpoint (motivationUrl) | https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp | 13-DEPLOY-EVIDENCE.json | triple.endpoint |
| auth: open interval upper bound from the DEPLOYING line (s) | 681 | 13-DEPLOY-EVIDENCE.json | auth.auth_window_s |
| auth: open interval from the end of the publish (s) | 461 | 13-DEPLOY-EVIDENCE.json | auth.auth_window_from_publish_done_s |
| live: cold wall max (ms) | 22110 | 13-DEPLOY-EVIDENCE.json | live.cold_wall_ms.max |
| live: warm p50 (ms) | 1131 | 13-DEPLOY-EVIDENCE.json | live.warm_ms.p50 |
| replay: largest admitted multi-text call (texts) | 7 | 13-ACCEPTANCE-REPLAY.json | largest_multi_text_n |
| replay: the 4-text call, tokens_total | 423 | 13-ACCEPTANCE-REPLAY.json | batch_of_4.line.tokens_total |
| door: served max texts | 8 | 13-ACCEPTANCE-REPLAY.json | chosen_max_texts |
| door: built-token budget per call | 800 | 13-DEPLOY-EVIDENCE.json | live.plan_only.concentrated.max_total_tokens |
| door: prefix tokens P | 97 | 13-ACCEPTANCE-REPLAY.json | P |
<!-- generated:lead end -->

The open interval is an upper bound. It runs from the DEPLOYING line of the publish to the first unauthenticated 401 the executor observed. The owner enabled OAuth before that first poll, so the true interval ended earlier (section 5). Cold and warm timings are reported, not gated.

## 0b. Degraded outcomes

none.

Checked against the records, field by field: the deployment acceptance decision is `accepted (recipe verified)`; the publish watchdog was stopped after the refusal and never contained the function; both after-reads (the ze-kasher-dev motivation server and the augury antisemite function) were derived equal to their before-reads; and the one publish attempt succeeded, so no earlier attempt was refused.

The following are recorded events, not degraded outcomes, listed so that nothing is hidden:

- Two M2M mint attempts were refused locally before the third succeeded. The first was the token host (F-13-08-2: the owner's client file names the shared counter-coach pool host, section 4). The second was this machine's Python, which has no CA bundle (F-13-08-3, still open, section 9). In both, no credential left the machine and no token was minted (13-DEPLOY-EVIDENCE.json `auth.m2m_attempts`).
- The first edge `/health` of the new id on augury answered 404 once and the bounded retry then got 200 (F-10-12-1, the expected first-404 state of a new id on a tenant).
- The 13-07 ze-kasher-dev identity probe's rc file reads 1 but is inverted: it was pasted with a leading shell negation, and the probe itself exited 0 with the pinned sha (13-DEPLOY-EVIDENCE.json `ze_kasher_before.identity_probe_rc`). The 13-09 after-read rc files are true statuses.

## 1. Status

- **A second server, not a move (D-02).** motivation-v1 now runs on the augury-dev pmcp.run tenant beside the ze-kasher-dev pilot. The ze-kasher-dev server is live and unchanged: the owner's read-only after-read returned the pinned sha and Lambda keys equal to the before-read.
- **The artifact is unchanged.** It is the same `laya-motivation-v1.apr` as Phase 12 (section 2). It was uploaded to the augury weights bucket, downloaded back and hashed equal to the pin before the publish. No retrain, gate run or task change happened in this phase.
- **OAuth is on** for the augury server (D-07). The ze-kasher-dev servers stay auth off.
- **Accepted.** The authenticated deploy verify passed on its first attempt, and the authenticated acceptance replay passed all seven requester tests (section 6).
- motivation-v1 is still the Phase 12 PILOT on the 25 October 2026 draft guide; a retrain is expected after the guide is frozen.

## 2. Identity

| Item | Value |
|------|-------|
| artifact | `laya-motivation-v1.apr` |
| `artifact_sha256` (the pin) | `d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd` |
| labels, in served order | `["social_affiliative", "conspiratorial", "other"]` |
| `task_json_sha256` | `f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3` |
| server id | `aprender-mcp-decide-motivation` (the same id on both tenants, section 3) |

The pin and the labels are those of 13-DEPLOY-EVIDENCE.json (`artifact_sha256`, `auth.authenticated_identity.expected_labels`) and of contracts/motivation-enrichment-v1.yaml `served_task`; the task sha is that of 12-HANDOFF.md and of the contract. Clients pin by endpoint and sha256, and nothing else is promised.

## 3. The two deployments

| Deployment | pmcp.run target | AWS account | Region | Endpoint | Auth posture |
|------------|-----------------|-------------|--------|----------|--------------|
| ze-kasher-dev pilot (Phase 12, unchanged) | dev | 816069129666 | us-east-1 | https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp | off |
| augury (this phase) | augury-dev | 783960389328 | eu-west-1 | https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp | OAuth on |

The two deployments share the server id `aprender-mcp-decide-motivation` and the thin crate `crates/aprender-mcp-decide-motivation-lambda` (D-06), and serve the same artifact. They are told apart only by the triple (pmcp.run target, AWS account, region) and the endpoint it gives, never by the id alone. Every Phase 13 record carries the triple. The augury endpoint is the one the publish printed, not an inferred one.

## 4. For the partner (D-08)

The partner's augury stack takes three values (motivation-experiment `docs/DEPLOY.md`, read in place):

- **`motivationUrl`** = `https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp`
- **`motivationSha`** = `d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd`
- **`MotivationAuthSecretArn`**: the secret holds the M2M client JSON in the same shape as the other augury secrets:

  ```json
  {"client_id": "<client id>", "client_secret": "<client secret>", "token_url": "https://counter-coach.eu-west.augury.pmcp.run/oauth2/token"}
  ```

  plus the optional `"scope"`. **The owner supplies the client id and secret out of band; this repository holds no credential.**

What was proven, and what it means for the secret:

- **Token URL.** The client was proven against the token endpoint of the shared counter-coach Cognito pool, `https://counter-coach.eu-west.augury.pmcp.run/oauth2/token`, which the owner named (F-13-08-2). It is NOT the per-server host that the motivation server's discovery documents advertise (`aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/oauth2/token`). Both hosts' discovery documents name the same issuer (one Cognito pool in eu-west-1), and the motivation server accepted the counter-coach-minted token. The per-server token host was not used for a mint, so it is unproven here.
- **Who can call it.** Because the pool is shared, any counter-coach-pool client that holds the motivation server's scopes can call the server, not only a client created for it.
- **Form-body credentials.** The client was proven with FORM-BODY credentials (`client_secret_post`; the recorded mint method is `form`). That is what the partner's client sends (motivation-experiment `crates/core/src/auth.rs`). HTTP Basic was not needed and not used.
- **Token lifetime.** The pool issues tokens with `expires_in` 900 s. A client that caches tokens must refresh within that window.
- **Runbook order.** OAuth is enabled on each server before any role grant is attached (DEPLOY.md "augury-dev bring-up"). For this server OAuth is already on, so the partner's grants may follow.
- **Partner HEAD read at acceptance:** `c632c749b02011b964c28f1a3a65f943033ff4d7` (13-ACCEPTANCE-REPLAY.json `partner_repo.head`; read-only `rev-parse HEAD` and `status --porcelain`, with the status equal before and after).
- **This phase wrote nothing to motivation-experiment or to the augury stack or its secrets.** Filling `motivationUrl` / `motivationSha` in `infra/context.augury-dev.json`, putting the client JSON into the secret and republishing are the partner's and the owner's steps.

Request form, unchanged from Phase 12 apart from the bearer: an HTTPS MCP POST with `Authorization: Bearer <M2M access token>`, a bare `tools/call` of `classify` with `{"texts": [...]}`; no SigV4 and no Function URL (13-ACCEPTANCE-REPLAY.json `request_form`).

## 5. Auth posture

- **Publish with auth off, enablement by the owner (D-07).** cargo-pmcp cannot render an auth-enabled descriptor for the pmcp-run target, so the server was published with `--no-oauth`. The owner then enabled OAuth (the method, admin UI or CLI, was not stated: 13-OAUTH-OWNER-REPLY.json) and supplied the M2M client file.
- **The open interval.** Its upper bound in seconds, measured from the DEPLOYING line to the first observed 401, and the same interval measured from the end of the publish are in the lead table. The maximum the plan allowed was 1800 s, enforced by a detached watchdog that would have contained the function at the deadline. The watchdog was stopped by its stop file after the refusal was observed; it never contained the function.
- **What an anonymous caller sees.** Unauthenticated `initialize` answers 200: that is open by platform design and is observed, not asserted. Unauthenticated `tools/list` and `tools/call` answer 401. The same refusal was re-checked after the acceptance replay (13-ACCEPTANCE-REPLAY.json `unauthenticated_refusal`).
- **The M2M client is proven** by an authenticated identity call that returned the pinned sha and labels, and by an authenticated negative control: with the antisemitism-v1 artifact as the expected identity, the token minted, the server answered (no 401) and the probe refused on identity, because the served sha is the motivation pin.

## 6. Acceptance and timings (reported, not gated)

Record: 13-ACCEPTANCE-REPLAY.json, run 2026-10-28 against the augury endpoint with an M2M token, one mint per call, using only the probe's committed synthetic sentences (no partner text was sent or recorded).

- **The seven requester tests all pass:** one result per text; probabilities as long as the labels; probabilities summing to 1 within 1e-5; `label` equal to the argmax; labels in task order; the returned sha equal to the pin; and the count door refusing above 8 texts.
- **The 4-text call** (experiment-coach's batch size) was admitted whole with every check true, at the same built-token total as Phase 12's batch of 4 (lead table).
- **Eight equal-length texts** were refused at the token budget (`classify_max_total_tokens`), and the replay stepped down; the largest admitted multi-text call is in the lead table. Nine texts were refused at the count door naming `classify_max_texts`, the limit and the contract.
- **The door is unchanged:** 8 texts, 800 built tokens per call, P 97 (the plan-only probes of the deploy verify read the served door; numbers table).
- **Deployment acceptance:** `accepted (recipe verified)`. Basis: the recipe's own verdict on its first attempt, `DEPLOY VERIFY OK aprender-mcp-decide-motivation: 4 cold samples (both shapes), every one < 30000 ms with CloudWatch cold evidence`. No owner decision was needed (13-VERIFY-OWNER-REPLY.json `skipped` true).
- **Timings.** Four proven-cold samples of the two maximal request shapes (800 built tokens each) and five warm calls; the cold minimum, maximum and worst slack to the 30 s gateway cap, and the warm p50, are in the tables. Phase 12's ze-kasher-dev figures sit beside them in the numbers table for comparison. Nothing keeps the function warm, so the first call after idle pays the load again.

## 7. The first enrichment round

What the experiment team and the expert need, all declared before any real file exists:

- **The formats.** The per-post record `motivation-record-v1` (one JSON object per post scored by the server, kept only in the partner repository) and the enrichment file `motivation-enrichment-v1` (a header line, then one row per annotated post with the expert's `label`, the `evidence` for the recorded function and the `selection_slice`). Both are declared in `contracts/motivation-enrichment-v1.yaml`, which also holds the (label, evidence) mapping, the merge rules and the refusal names.
- **The selection guidance:** `13-SELECTION-NOTE.md` (stratified quota, uncertainty band and random control, default budget 300, seeded and order-independent, implemented once in `scripts/laya_train/select_motivation.py`). It says plainly not to send only high-probability rows, and why the random control slice matters.
- **The ingest command**, run by aprender once the expert's file exists:

  ```bash
  uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py \
      --enrichment "<file>" --enrichment-sha256 <sha256 of that file> --out data/decide/<new dir>
  ```

  `--enrichment` is repeatable and reads the file in place (a path inside the aprender repository is refused); `--enrichment-sha256` pins it. Rules, as declared:
  - Enriched rows go to train only. `eval.jsonl` stays frozen and byte-identical to motivation-v1's, so the next gate is comparable with motivation-v1's numbers.
  - An enriched text that matches an eval text is dropped and counted. A text that carries more than one mapped label across sources is dropped whole, the partner row included, and counted; no source takes precedence.
  - Weak and mixed evidence map to `other` (guide 10.3), and every (label, evidence) cell is counted in the manifest, so that decision stays open for the post-freeze retrain.
  - The output holds counts and sha256 values only, never a text; a refusal names a row number and a field, never a value.
- **The ingest is proven on SYNTHETIC fixtures only**, because no real enrichment file exists (13-INGEST-EVIDENCE.json `proven_on`; selftest count and fixture merge in the numbers table). A run without `--enrichment` still writes motivation-v1's exact bytes.
- **DECISION FOR THE OWNER, before the partner builds the annotation sheet** (contracts/motivation-enrichment-v1.yaml `d15_amendment`). The format goes beyond the literal clear / weak / mixed list of D-15 in three ways:
  1. It adds the evidence level `none`, for an `other` post with no evidence of either function (the guide's first Other situation cannot be written with clear, weak or mixed).
  2. `evidence` describes the RECORDED function, not the strongest evidence anywhere in the post (a clear social-affiliative post with weak conspiratorial evidence records `social_affiliative` + `clear`).
  3. It refuses three pairs: `other` + `clear`, `social_affiliative` + `none` and `conspiratorial` + `none`.

  The ingest and the selector implement this as committed. Please confirm it, or reply if the expert's sheet needs another convention; a change is a `motivation-enrichment-v2`. The D-15 mapping itself (weak and mixed count as other) is not part of this question.
- **A second, smaller rule question for the owner (13-05 finding).** With three labels that sum to one, the uncertainty band's margin clause (`p1 - p2 < 0.20`) adds no post beyond its probability clause, so the band is in effect `p1 <= 0.75`. The rule was kept as declared, and the note and a selftest state this. Making the margin clause matter is a rule change (new constants and a regenerated note); say if you want one.
- **Anchoring.** The labels of this round are collected with the model's prediction shown (`annotation_mode` `prediction_shown`, the owner's choice, D-16), so the hand-off of whichever phase retrains on them must state that they may be anchored to motivation-v1's predictions. The ingest records and counts the mode and does not act on it. A suggestion only, not a requirement: if the sheet layout allows it, the expert records the label before opening the probability column.
- **What this phase did not do (D-03):** no post was collected, no label was written and no silver label (a model prediction used as a label) was admitted. The record and enrichment files will live in the partner repository; aprender reads them in place and never copies or prints them.

## 8. Decisions carried

- **D-01: the three-class model continues**, and the binary conspiratorial-or-not reframing is dropped. The basis, from 13-CONTEXT.md (the analysis of 2026-10-27 over the eval probabilities and the two other seed dumps): on the 100-row eval the two functions are confused with each other once (and 0 times in the other direction; 1+2 / 0+0 on the other seeds), every other error is function-versus-other, and the per-class AUC ordering flips by seed, so neither class is shown to be easier.
- **D-04: the review follow-up 12-17..12-19 stays parked.** This phase ran on the current verifier and tooling. The closure check (13-CLOSURE-EVIDENCE.json) shows the verifier, the served tool, the door contract, the thin crate and the deployment-unit register untouched since the phase base.

## 9. Operational notes

- **One pmcp-run credential slot.** `~/.pmcp/credentials.toml` holds one pmcp-run session, and the augury-dev login replaced the ze-kasher-dev one. Any ze-kasher-dev pmcp operation needs a fresh login first. The augury login command is `PMCP_TARGET=augury-dev cargo pmcp deploy login`, run by the owner in their own terminal.
- **Redeploying the augury server.** Any redeploy after OAuth was enabled needs `LAYA_M2M_FILE` set (the recipe verifies through the authenticated path), and must be followed by `just _laya-auth-refusal-check <endpoint> 0`. A `--no-oauth` redeploy may reset OAuth (RESEARCH A3, unobserved), and that check is what would show it.
- **Short token validity.** The counter-coach pool issues 900 s tokens, below the verify recipe's default floor; `LAYA_M2M_MIN_EXPIRES=900` lowers the floor, as plan 13-09 did.
- **Token host note.** The owner-named token host lives in the gitignored `models/decide/augury-dev/m2m-token-host`; `LAYA_M2M_TOKEN_HOST` overrides it.
- **Local TLS trust on this machine (F-13-08-3, open).** The python.org Python 3.14 that the token helper runs under ships no CA bundle, so every https mint fails certificate verification unless `SSL_CERT_FILE=/etc/ssl/cert.pem` (the macOS system trust store) is set; verification stays on. The helper is not fixed: a later change should fall back to the system bundle.
- **Local state per profile.** ze-kasher-dev state stays under `models/decide/`; augury-dev state is under `models/decide/augury-dev/`. The ze-kasher-dev motivation record was archived and pinned before this phase wrote anything (13-BASELINE-EVIDENCE.json), and it verified at the end.
- **The antisemite id's default-path local record** already holds an augury record from the 2026-10-25 antisemite deploy (observed in RESEARCH, unchanged by this phase).
- **Known tooling defect (deferred-items.md).** `laya-gates-selftest` rewrites the antisemite id's two eligibility logs and does not restore them. Executors restored them from backups each time. The fix (add both paths to the sweep's backup and restore) is open. Plan 13-01 also lost the earlier bytes of those two logs before its restore existed; they were tiny-fixture refusal output by size, and are unrecoverable.
- **Found and fixed at closure.** The full `laya-gates-selftest` sweep's dup-bin-names row expected the `bootstrap` duplicate to name 4 packages; Phase 12's motivation crate made it 5 (already in the allowlist), so the full sweep was RED on that row since before this phase. The row now expects 5 (plan 13-10); the guard and the allowlist are unchanged, and the full sweep then passed all 44 rows (13-CLOSURE-EVIDENCE.json).
- **Manifest compatibility (13-INGEST-EVIDENCE.json open item): closed.** With a synthetic `enrichment-manifest.json` added to a copy of the tiny fixture's data dir, `laya-pack-fixture` produces the same bytes as on a clean copy and `laya-verify` refuses with the same first line (suite enrichment-manifest-compat, 13-CLOSURE-EVIDENCE.json `manifest_compat`). By code reading, train.py reads only named files (train.py:748).
- The manifest key is `manifest.filename` (not `file`) in the contract, a 13-02 deviation recorded there.

## 10. Scope and deferred

From 13-CONTEXT.md's deferred list, unchanged:

- Retraining on the enrichment (motivation-v2) and its gate run: the next phase, once a real enrichment file exists.
- A second, field-drawn eval set held out from the enrichment (rejected for now in favour of a frozen eval, D-11): revisit at the retrain.
- Blind annotation (the owner chose prediction shown, D-16): revisit if the retrain shows anchoring.
- Deploying antisemitism-v1 on augury (the partner's runbook input I1): its own work. The artifact is in the augury bucket and an antisemite function exists there (observed read-only, not claimed by this phase).
- Auth on for the ze-kasher-dev servers.
- Wiring the experiment-coach motivation trio (`EC_MOTIVATION_*`): partner work.
- Everything still deferred from Phase 12: server-side prior correction, per-source results, audit output, raising `calibration_temp_max`, thread context.
- The Phase 13 selftests stay local-only by the owner's decision of 2026-10-27 (no CI fragment).

## 11. Dates

- 2026-10-27: phase planned; the ze-kasher-dev record baselined (plan 13-11).
- 2026-10-28: region parameter, enrichment contract, ingest, triple guards, selection note and token helper committed (plans 13-01 to 13-06); weights uploaded and the owner logged in (13-07); publish, OAuth enablement and the M2M proof (13-08); authenticated verify, acceptance replay and the after-reads (13-09); this hand-off and the closure (13-10).
- The exposure start, the first 401 and the replay's closing refusal times are in the numbers table, in UTC.

## Numbers and their records

<!-- numbers begin -->
| claim | value | record | key |
|---|---|---|---|
| augury: pmcp.run target | augury-dev | 13-DEPLOY-EVIDENCE.json | triple.pmcp_target |
| augury: AWS account | 783960389328 | 13-DEPLOY-EVIDENCE.json | triple.aws_account |
| augury: region | eu-west-1 | 13-DEPLOY-EVIDENCE.json | triple.region |
| augury: endpoint (motivationUrl) | https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp | 13-DEPLOY-EVIDENCE.json | triple.endpoint |
| ze-kasher-dev: pmcp.run target | dev | 13-DEPLOY-EVIDENCE.json | deployments.ze_kasher_dev.pmcp_target |
| ze-kasher-dev: AWS account | 816069129666 | 13-DEPLOY-EVIDENCE.json | deployments.ze_kasher_dev.aws_account |
| ze-kasher-dev: region | us-east-1 | 13-DEPLOY-EVIDENCE.json | deployments.ze_kasher_dev.region |
| ze-kasher-dev: endpoint | https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp | 13-DEPLOY-EVIDENCE.json | deployments.ze_kasher_dev.endpoint |
| artifact sha256 (the pin, motivationSha) | d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd | 13-DEPLOY-EVIDENCE.json | artifact_sha256 |
| tier (MB) | 10240 | 13-DEPLOY-EVIDENCE.json | tier_mb |
| upload: object bytes (head-object) | 846196612 | 13-DEPLOY-EVIDENCE.json | upload.head_object.content_length |
| upload: remote copy hashed equal to the pin | True | 13-DEPLOY-EVIDENCE.json | remote_identity.matches |
| publish: edge /health 404 retries | 1 | 13-DEPLOY-EVIDENCE.json | live_attempts.-1.health_404_retries |
| publish: DEPLOYING line (UTC, exposure start) | 2026-10-28T04:49:48Z | 13-DEPLOY-EVIDENCE.json | auth.exposure_start_utc |
| auth: posture now | on | 13-DEPLOY-EVIDENCE.json | auth.posture |
| auth: enabled by | owner | 13-DEPLOY-EVIDENCE.json | auth.enabled_by |
| auth: OAuth enablement method | not stated | 13-OAUTH-OWNER-REPLY.json | steps.0.method |
| auth: maximum open interval allowed (s) | 1800 | 13-DEPLOY-EVIDENCE.json | auth.max_open_s |
| auth: first unauthenticated 401 observed (UTC) | 2026-10-28T05:01:09Z | 13-DEPLOY-EVIDENCE.json | auth.first_401_utc |
| auth: open interval upper bound from the DEPLOYING line (s) | 681 | 13-DEPLOY-EVIDENCE.json | auth.auth_window_s |
| auth: open interval from the end of the publish (s) | 461 | 13-DEPLOY-EVIDENCE.json | auth.auth_window_from_publish_done_s |
| auth: watchdog outcome | stopped after the refusal | 13-DEPLOY-EVIDENCE.json | auth.watchdog_outcome |
| auth: unauthenticated initialize (observed) | 200 | 13-DEPLOY-EVIDENCE.json | auth.observed.initialize |
| auth: token host advertised by the motivation discovery | aprender-mcp-decide-motivation.eu-west.augury.pmcp.run | 13-DEPLOY-EVIDENCE.json | auth.discovery.token_endpoint_host |
| auth: token host proven (shared counter-coach pool) | counter-coach.eu-west.augury.pmcp.run | 13-DEPLOY-EVIDENCE.json | auth.authenticated_identity.token_host |
| auth: mint method (client_secret_post) | form | 13-DEPLOY-EVIDENCE.json | auth.authenticated_identity.auth_method |
| auth: authenticated identity matches the pin | True | 13-DEPLOY-EVIDENCE.json | auth.authenticated_identity.identity_line.identity_matches |
| auth: negative control identity matches | False | 13-DEPLOY-EVIDENCE.json | auth.negative_control.identity_matches |
| auth: negative control probe exit status | 1 | 13-DEPLOY-EVIDENCE.json | auth.negative_control.rc |
| live: outcome | verified | 13-DEPLOY-EVIDENCE.json | live.outcome |
| live: deployment acceptance | accepted (recipe verified) | 13-DEPLOY-EVIDENCE.json | live.deployment_acceptance.decision |
| live: cold samples measured (proven cold) | 4 | 13-DEPLOY-EVIDENCE.json | live.cold_wall_ms.n_measured |
| live: cold wall min (ms) | 18850 | 13-DEPLOY-EVIDENCE.json | live.cold_wall_ms.min |
| live: cold wall max (ms) | 22110 | 13-DEPLOY-EVIDENCE.json | live.cold_wall_ms.max |
| live: cold worst slack to the 30 s cap (ms) | 7890 | 13-DEPLOY-EVIDENCE.json | live.cold_wall_ms.worst_slack_to_cap_ms |
| live: warm calls | 5 | 13-DEPLOY-EVIDENCE.json | live.warm_ms.n |
| live: warm min (ms) | 1105 | 13-DEPLOY-EVIDENCE.json | live.warm_ms.min |
| live: warm p50 (ms) | 1131 | 13-DEPLOY-EVIDENCE.json | live.warm_ms.p50 |
| live: warm max (ms) | 1175 | 13-DEPLOY-EVIDENCE.json | live.warm_ms.max |
| live: cold-bump configuration updates (motivation function only) | 4 | 13-DEPLOY-EVIDENCE.json | live.configuration_changes.count |
| door: built-token budget per call | 800 | 13-DEPLOY-EVIDENCE.json | live.plan_only.concentrated.max_total_tokens |
| Phase 12 ze-kasher-dev: cold wall min (ms) | 22620 | 12-DEPLOY-EVIDENCE.json | live.cold_wall_ms.min |
| Phase 12 ze-kasher-dev: cold wall max (ms) | 25353 | 12-DEPLOY-EVIDENCE.json | live.cold_wall_ms.max |
| Phase 12 ze-kasher-dev: warm p50 (ms) | 1253 | 12-DEPLOY-EVIDENCE.json | live.warm_ms.p50 |
| ze-kasher-dev after-read: served sha | d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd | 13-DEPLOY-EVIDENCE.json | ze_kasher_after.identity_probe.artifact_sha256 |
| ze-kasher-dev after-read: unchanged (derived) | True | 13-DEPLOY-EVIDENCE.json | ze_kasher_after.unchanged |
| augury antisemite function after-read: unchanged (derived) | True | 13-DEPLOY-EVIDENCE.json | augury_antisemite_after.unchanged |
| replay: token host | counter-coach.eu-west.augury.pmcp.run | 13-ACCEPTANCE-REPLAY.json | token.host |
| replay: token method | form | 13-ACCEPTANCE-REPLAY.json | token.method |
| replay: token expires_in (s) | 900 | 13-ACCEPTANCE-REPLAY.json | token.expires_in |
| replay: mints (one per call) | 8 | 13-ACCEPTANCE-REPLAY.json | mints |
| replay: served max texts | 8 | 13-ACCEPTANCE-REPLAY.json | chosen_max_texts |
| replay: prefix tokens P | 97 | 13-ACCEPTANCE-REPLAY.json | P |
| replay: largest admitted multi-text call (texts) | 7 | 13-ACCEPTANCE-REPLAY.json | largest_multi_text_n |
| replay: largest admitted call, tokens_total | 745 | 13-ACCEPTANCE-REPLAY.json | timings_reported.tokens_total.n7 |
| replay: texts refused at the token budget | 8 | 13-ACCEPTANCE-REPLAY.json | refused_token_budget.0.n |
| replay: token-budget refusal bound | classify_max_total_tokens | 13-ACCEPTANCE-REPLAY.json | refused_token_budget.0.line.refusal_bound |
| replay: the 4-text call, texts | 4 | 13-ACCEPTANCE-REPLAY.json | batch_of_4.line.n_texts |
| replay: the 4-text call, tokens_total | 423 | 13-ACCEPTANCE-REPLAY.json | batch_of_4.line.tokens_total |
| replay: the 4-text call, elapsed (ms) | 2787 | 13-ACCEPTANCE-REPLAY.json | batch_of_4.line.elapsed_ms |
| replay: over-door count tried | 9 | 13-ACCEPTANCE-REPLAY.json | expected_count_refusal.n_texts |
| replay: test one result per text | pass | 13-ACCEPTANCE-REPLAY.json | requester_tests.one_result_per_text.result |
| replay: test probabilities length equals labels | pass | 13-ACCEPTANCE-REPLAY.json | requester_tests.probabilities_length_equals_labels.result |
| replay: test probabilities sum to 1 within 1e-5 | pass | 13-ACCEPTANCE-REPLAY.json | requester_tests.probabilities_sum_to_1_within_1e-5.result |
| replay: test label is argmax | pass | 13-ACCEPTANCE-REPLAY.json | requester_tests.label_is_argmax.result |
| replay: test labels in task order | pass | 13-ACCEPTANCE-REPLAY.json | requester_tests.labels_in_task_order.result |
| replay: test returned sha equals the pin | pass | 13-ACCEPTANCE-REPLAY.json | requester_tests.returned_sha_equals_pin.result |
| replay: test count bound refuses above the door | pass | 13-ACCEPTANCE-REPLAY.json | requester_tests.count_bound_refuses_above_the_door.result |
| replay: closing unauthenticated tools/list | 401 | 13-ACCEPTANCE-REPLAY.json | unauthenticated_refusal.tools_list |
| replay: closing unauthenticated tools/call | 401 | 13-ACCEPTANCE-REPLAY.json | unauthenticated_refusal.tools_call |
| replay: closing refusal first 401 (UTC) | 2026-10-28T05:41:22Z | 13-ACCEPTANCE-REPLAY.json | unauthenticated_refusal.first_401_utc |
| replay: deployment acceptance | accepted (recipe verified) | 13-ACCEPTANCE-REPLAY.json | deployment_acceptance.decision |
| verify checkpoint skipped (no owner decision needed) | True | 13-VERIFY-OWNER-REPLY.json | skipped |
| partner repo HEAD read at acceptance | c632c749b02011b964c28f1a3a65f943033ff4d7 | 13-ACCEPTANCE-REPLAY.json | partner_repo.head |
| partner repo unchanged (status before equals after) | True | 13-ACCEPTANCE-REPLAY.json | partner_repo_unchanged |
| D-18 probe: sts account for profile augury-dev | 783960389328 | 13-PROBE-EVIDENCE.json | probes.P1.result |
| ingest: proven on | synthetic fixtures only; no real enrichment file exists (D-09) | 13-INGEST-EVIDENCE.json | proven_on |
| ingest: selftest ok cases | 147 | 13-INGEST-EVIDENCE.json | selftest.ok_cases |
| ingest: selftest FAIL cases | 0 | 13-INGEST-EVIDENCE.json | selftest.fail_cases |
| ingest: run without --enrichment | unchanged (byte-identical) | 13-INGEST-EVIDENCE.json | v1_identity.no_flag_status |
| ingest: frozen eval.jsonl sha256 | ee95d71fcb5d90d515b9fc6741e5df51087e763fa7f251ace83d9f11d5ccd575 | 13-INGEST-EVIDENCE.json | v1_identity.eval |
| ingest: synthetic fixture rows added | 4 | 13-INGEST-EVIDENCE.json | fixture_merge.rows_added |
| ingest: synthetic fixture eval byte-identical | True | 13-INGEST-EVIDENCE.json | fixture_merge.eval_byte_identical |
| ingest: annotation_mode policy | recorded and counted, never acted on (D-16) | 13-INGEST-EVIDENCE.json | annotation_mode_policy |
| ze-kasher-dev local record: archive manifest sha256 (13-11 pin) | b01bc7216ddd1e349b079a2bf9aa314a2f8030a0730ed94233d348429a44ad90 | 13-BASELINE-EVIDENCE.json | archive.manifest_sha256 |
<!-- numbers end -->
