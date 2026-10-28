---
phase: "13"
slug: "motivation-v1-on-augury-dev-second-server-and-enrichment-handoff"
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: "2026-10-28"
---

# Phase 13 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> The register was authored at plan time in the 11 PLAN `<threat_model>` blocks and merged by id here (T-13-SC appears in every plan).
> Mitigations were verified read-only by gsd-security-auditor on 2026-10-28 against the code and the committed evidence records, which are the only evidence for the live and owner-run steps 13-07..13-09.
> The auditor's per-threat file:line and record-key evidence lives in the session record. This file holds the register and the dispositions.

---

## Trust Boundaries

| Boundary | Description | Plan |
|----------|-------------|------|
| operator argument -> recipe shell and rendered TOML | the region value crosses into bash text (just interpolation) and into a TOML file cargo-pmcp deploys | 13-01 |
| selftest -> AWS | the offline selftests must never reach AWS (recorder) | 13-01 |
| partner-produced files -> aprender ingest | the formats declared here are what the ingest will accept from outside the repo | 13-02 |
| partner enrichment file -> training data | an outside file decides train.jsonl rows | 13-03 |
| script output -> logs, evidence | post text must never cross into printed or committed output | 13-03 |
| executor shell environment -> cargo-pmcp / AWS CLI | ambient PMCP_TARGET, AWS_PROFILE, AWS_REGION and the repo-root marker can silently choose a tenant | 13-04 |
| local state files -> later recipes | a state file written for one tenant read as another's | 13-04 |
| partner record files -> the reference script | outside data read in place on the operator's machine | 13-05 |
| owner's M2M client file -> operator processes | a long-lived client secret on local disk becomes a short-lived bearer | 13-06 |
| bearer -> network | the token must reach only the server's host, never argv, logs or evidence | 13-06 |
| unauthenticated internet -> the protected server | what the platform refuses, and what it leaves open (initialize) | 13-06 |
| executor -> augury AWS account (783960389328) | read-only probes, then one S3 PutObject under the owner's credentials | 13-07 |
| executor -> ze-kasher-dev account and server | read-only only | 13-07 |
| internet -> the new endpoint | open between publish and OAuth enablement, protected afterwards | 13-08 |
| owner's M2M client -> executor processes | a client secret on local disk, used only through the helper | 13-08 |
| executor -> augury account | one cargo pmcp deploy (create), read-only IAM and Lambda reads | 13-08 |
| verifier -> augury endpoint | authenticated probes and cold-start bumps bill the augury account and deliberately update the function's environment (DECIDE_COLD_BUMP) | 13-09 |
| verifier -> ze-kasher-dev | read-only probe and reads only | 13-09 |
| this repo -> the owner and the partner | the hand-off leaves the repo and is forwarded | 13-10 |
| phase-13 commands -> the ze-kasher-dev local record | offline selftests and augury recipes share the server id and, for the default profile, the directory | 13-11 |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-13-01 | Tampering | laya-deploy-config / laya-weights-bucket region | high | mitigate | `[positional-arguments]` (no textual interpolation), the region ERE before any interpolation or AWS call, the post-parse assert; the R6 canary proves a quote-and-command value executes nothing (M7 turns it RED) | closed |
| T-13-02 | Tampering | LAYA_DEPLOY_TEMPLATE | medium | mitigate | honoured only under DRY_RUN=1; set in a live run is a refusal, so no deploy can render from an alternate template | closed |
| T-13-03 | Information disclosure | refusal output | low | mitigate | the refusal names a character count, never the value; the R6 cases assert the value is absent from the log | closed |
| T-13-04 | Tampering | an under-specified enrichment format (open key sets, no caps) letting unexpected content into training data | medium | mitigate | closed key sets for header and row, enums for label / evidence / slice / annotation mode, caps on bytes, rows and characters, duplicate post_id refused; bound to the ingest by plan 13-03's mirror check | closed |
| T-13-05 | Information disclosure | post text in refusals or the manifest | medium | mitigate | the declared no-text rule (row number, field, character count only) and a counts-only manifest; enforced by plan 13-03's sentinel selftest | closed |
| T-13-06 | Tampering | eval leakage or a moved eval set | high | mitigate | merge after the unchanged v1 build; eval-overlap drop; eval bytes asserted equal to the base build and to the v1 pin; check_v1_identity in every pinned run; mutations N1/N2 prove both checks fire | closed |
| T-13-07 | Tampering | label poisoning through conflicting sources or a resurrected mixed group | medium | mitigate | conflict groups dropped whole with no precedence; partner mixed groups stay dropped; counts printed and pinned in the manifest; N3/N5 | closed |
| T-13-08 | Information disclosure | post text in refusals, stdout or the manifest | high | mitigate | refusals name row, field and a count only; the sentinel selftest covers success, every one of the 17 refusal paths and the manifest, with a capture control | closed |
| T-13-09 | Tampering | partner data copied into the repo | medium | mitigate | enrichment-in-repo refusal on resolved paths (symlink case tested); files read in place; outputs only under an explicit --out outside the v1 dir; the partner repo's HEAD and status compared before and after; the one t… | closed |
| T-13-10 | Tampering | wrong-tenant deploy or AWS write (stale env, repo-root marker `dev`) | high | mitigate | committed profile->target table, env-agreement refusals, cross-check of ~/.pmcp/config.toml, recipe-side export of the checked values; the wiring guard and K1/K3 prove the check is present and fires | closed |
| T-13-11 | Tampering | the augury publish, or an offline selftest run with the same id, overwriting or deleting the ze-kasher-dev local record (rm -rf of the state snapshot; the eligibility logs) | high | mitigate | per-profile state dir (default paths unchanged), ISOLATION 1-3, K2/K5; laya-deploy-selftest restores the id's eligibility logs; plan 13-11 archived the record with a pinned sha256 manifest in wave 1, re-verified after… | closed (residual R-1) |
| T-13-12 | Repudiation | evidence that cannot say which deployment it describes | medium | mitigate | the triple in every 13-*-EVIDENCE.json; the TARGET RECEIPT banner line from cargo-pmcp's log | closed |
| T-13-13 | Spoofing | a state file or a printed endpoint naming another tenant's host (including a suffix-spoof host) | medium | mitigate | `_laya-endpoint-check` with an anchored suffix; contain in laya-deploy, refuse in laya-deploy-verify before the probe build; K4 | closed |
| T-13-14 | Information disclosure | post text printed or written by the selection script | medium | mitigate | output holds post_id, slice and rank only; counts-only stdout; refusals name row, field and a count; sentinel selftest with a capture control; out-in-repo refusal | closed |
| T-13-15 | Tampering | a selection biased toward confident rows (silent drift from the declared rule), or a prediction becoming a label | medium | mitigate | one implementation of the declared rule with boundary, determinism and seed cases; the note's rule region is generated by --print-rule and compared byte for byte; no label or probability column in the output (D-03) | closed |
| T-13-16 | Information disclosure | client secret or token on argv, in logs or in evidence | high | mitigate | file input with type/owner/mode checks; exec with the token in the child environment only; prints host/status/expires_in/method only; selftests assert absence; A1 proves the absence check fires; evidence regex for cre… | closed |
| T-13-17 | Spoofing | a client file for another server or a look-alike token endpoint | medium | mitigate | token_url host must equal the endpoint host (or an explicit, pattern-checked LAYA_M2M_TOKEN_HOST), https only, path /oauth2/token; loopback only under the selftest flag | closed (residual R-3) |
| T-13-18 | Spoofing (false assurance) | asserting a refusal the platform does not make (initialize is open on augury) | medium | mitigate | 401 asserted on tools/list and tools/call only; initialize and the discovery document recorded as observations | closed |
| T-13-19 | Denial of service / cost | the refusal probe triggering a 10 GB cold classify on an open server | low | mitigate | tools/call is sent only after tools/list returned 401; A5 | closed |
| T-13-20 | Tampering | a post-enablement redeploy or verify probing without a token and containing a healthy function; the auth selftest's cases rewriting either deployment's local files | medium | mitigate | LAYA_M2M_FILE re-exec in laya-deploy and laya-deploy-verify with expires_in floors (overridable); both live branches executed through shims; a pre-set token in the caller's env is refused; `_laya-auth-selftest` restor… | closed (residual R-2) |
| T-13-21 | Tampering | the ze-kasher-dev local record (the only copy of the us-east endpoint record) | high | mitigate | archive plus a sha256 manifest of the original paths and a pre-phase mtime anchor taken in wave 1 (13-11) before any phase-13 command writes them, its sha256 pinned; per-profile state dir and selftest restores (13-04,… | closed |
| T-13-22 | Tampering | wrong-tenant or wrong-account write | high | mitigate | D-18 read-only probe gate; `_laya-target-check` inside every recipe; env -u of every ambient target variable; sts read immediately before the upload; content-addressed key in the augury bucket only | closed |
| T-13-23 | Elevation of privilege | the executor using or refreshing a pmcp.run session it was not given | medium | mitigate | the owner logs in; the gate reads expires_at only; the executor never runs login or configure use | closed |
| T-13-24 | Information disclosure | tokens, secrets or other accounts' ids in committed evidence | medium | mitigate | AWS JSON to /tmp only; evidence regex allows only the two known account ids and rejects JWT shapes | closed |
| T-13-25 | Denial of service / cost | the open endpoint after publish (10 GB cold start per anonymous call) | medium | mitigate | the owner checkpoint follows the recipe's return at once with no optional check before it; the URL is not given to the partner until OAuth is on; the window is measured from the conservative DEPLOYING timestamp to the… | closed |
| T-13-26 | Spoofing | the wrong artifact or binary served under the reused id on augury | high | mitigate | sha pin and content-addressed key, laya-verify before upload and deploy, compile-log check, edge serverId, identity probe, antisemitism-artifact negative control, readback of the env sha | closed |
| T-13-27 | Tampering | the publish landing on another tenant or overwriting ze-kasher-dev state | high | mitigate | target check and recipe-side PMCP_TARGET; `_laya-endpoint-check` on the printed endpoint; the TARGET RECEIPT; ARN region and account in the readback; archive manifest re-verified | closed |
| T-13-28 | Information disclosure | M2M credentials, a token or the secret file's path in the repo or evidence | high | mitigate | file outside the checkout, path only in a gitignored note, reply recorded with the path redacted, helper never prints credentials, evidence regex for credential shapes and absolute paths | closed |
| T-13-29 | Spoofing (false assurance) | treating open initialize as a failure or a pass | low | mitigate | 401 asserted on tools/list and tools/call only; initialize and the discovery document recorded as observations | closed |
| T-13-30 | Repudiation | a sample counted cold without proof, a dropped failed sample, a summary built from a null | medium | mitigate | the 12-13 normalization: every sample kept with explicit nulls and its error, proven_cold derived from the CloudWatch line and probe id, summaries from measured values, all re-derived in the verify | closed |
| T-13-31 | Tampering (accepted, recorded) | the augury function's configuration during cold sampling | low | accept | the DECIDE_COLD_BUMP update is the measurement's mechanism, recorded as configuration_changes; no other function is written | closed (accepted, AR-1) |
| T-13-32 | Information disclosure | the M2M path, a token or post text in records | high | mitigate | the path read from a gitignored note at runtime; the helper prints host/status/expires_in/method only; synthetic replay texts only; regex checks for JWTs, secret values and absolute paths | closed |
| T-13-33 | Tampering | the ze-kasher-dev deployment changed by the augury work | high | mitigate | read-only identity probe and Lambda keys compared with the 13-07 baseline, `unchanged` derived from the three-state records by the verify; the wave-1 archive manifest (13-11) verified and its sha256 compared with the … | closed |
| T-13-34 | Information disclosure | credentials, the M2M path, tokens or post text in the hand-off or any committed file | high | mitigate | values only from records that never held them; the hand-off check and the closure sweep (JWT shapes, secret values, the path, the sentinels, unknown account ids), which the closure verify repeats itself rather than tr… | closed |
| T-13-35 | Repudiation | a hand-off number that no record backs, or the two deployments confused | medium | mitigate | the numbers table names a record file and key per number and the verify reads each key and compares the quoted value; the triple table | closed |
| T-13-36 | Tampering | scope creep into the verifier, the served tool, the register or the thin crate; a closure record whose suite results were never re-checked | medium | mitigate | `git diff --quiet 61586f888 HEAD` over every out-of-scope surface; FALSIFY-MONO-011 and the duplicate-bin guard; the closure verify re-runs twelve suites and reads the two long suites' fresh logs | closed |
| T-13-37 | Repudiation | the D-06 "unchanged" proof certifying a state that a phase-13 command (a killed selftest included) already wrote | high | mitigate | archive and manifest taken in wave 1 before the first writer (every writer plan depends on 13-11; no writer SUMMARY and no `just` process naming the id at archive time); the archived bytes cross-checked against Phase … | closed (residual R-1) |
| T-13-38 | Repudiation (false assurance) | row_verify reading a restored historical eligibility log, so the gate passes without measuring this run | high | mitigate | the deploy selftest exports this run's eligibility output before it restores the historical files; armed_deploy_selftest deletes the export first and reads it only when newer than its armed-start marker; M9 (no export… | closed |
| T-13-39 | Tampering | semantic drift between the declared bounds and the two validators (pv validates metadata only) | medium | mitigate | every enforced bound is a structured value whose patterns are compiled and probed in Task 1's verify; plans 13-03 and 13-05 mirror those structured values and boundary-test them; the d15_amendment is explicit and goes… | closed |
| T-13-40 | Information disclosure | a post fragment in an enrichment FILE NAME, or a code point in an encoding refusal | medium | mitigate | generated source ids instead of file names in every printed line and in the manifest; the encoding refusal re-raised as enrichment-encoding with row and field only; the file-name sentinel case and N7/N8 | closed |
| T-13-41 | Spoofing | PMCP_API_URL in the caller's environment steering cargo-pmcp to another control plane while PMCP_TARGET names augury-dev | high | mitigate | `_laya-target-check` refuses a set PMCP_API_URL that is not the target's api_url and checks the api_url shape for the region; every wired recipe unsets PMCP_API_URL after the check so cargo-pmcp injects the target's o… | closed |
| T-13-42 | Tampering | a correctly named profile configured for another AWS account passing every offline check | high | mitigate | the committed table carries the expected account; `_laya-account-check` compares live sts with it before the first mutation of every wired recipe; the ordered WIRING guard and K7/K9 | closed |
| T-13-43 | Denial of service | the new target refusal inside laya-teardown blocking contain() and leaving a bad deploy serving | medium | mitigate | TEARDOWN REACHABLE with exactly laya-deploy's exported environment, its refused twin, K10; contain()'s fallback names the hand command with profile and region | closed |
| T-13-44 | Repudiation (false assurance) | the note's prose claiming numbers or an "unbiased" reach the code does not support | medium | mitigate | prose numbers must appear in the generated region; the control's population and claim limits are stated in the note and the selection header; band_share_of_pool shows when the band is not selective | closed |
| T-13-45 | Information disclosure | a token-endpoint redirect carrying the client credentials (Basic header or a re-sent body) to another host or over http | high | mitigate | no redirect is ever followed (`REFUSED m2m-redirect`); Basic set with add_unredirected_header; redirect cases for every 3xx code and an http Location with a second listener that must see zero requests; A7 | closed |
| T-13-46 | Spoofing (false assurance) | a Basic-only M2M client accepted as partner-ready although the partner's client sends form-body credentials only | high | mitigate | Basic is a diagnostic attempt only; a Basic-only success is `REFUSED m2m-auth-method` unless --allow-basic (never used for acceptance), which prints NOT partner-ready; plans 13-08 and 13-09 require `(auth form)`; A8 | closed |
| T-13-47 | Tampering | a same-size wrong object under the content-addressed key accepted as uploaded; the real augury config overwritten by a selftest that re-renders its path | medium | mitigate | the remote object downloaded outside the repo and hashed equal to the pin before the publish; the selftest runs before the render, and the post-render region / target / auth selftests must leave the config's sha256 an… | closed |
| T-13-48 | Repudiation (false assurance) | an AccessDenied, expired-credential or transport failure recorded as an absent bucket or Lambda | medium | mitigate | three-state records (present / absent only on NotFound / unreadable with its error code); an unreadable bucket stops the plan; the verify rejects an unreadable record without an error code | closed |
| T-13-49 | Denial of service / cost | an open interval with no bound when the owner never replies, or understated because it was timed from the recipe's return | high | mitigate | exposure_start_utc from the timestamped DEPLOYING line; deadline_utc = start + LAYA_MAX_OPEN_S; a detached watchdog contains the function (reversible) at the deadline unless anonymous tools/list is refused, fail-close… | closed (residual R-4) |
| T-13-50 | Repudiation (false assurance) | an acceptance record that passes a refused 4-text call, trusts hand-written `pass` / `unchanged` flags, or merges the recipe's verdict with the owner's decision | high | mitigate | the 4-text call must succeed at Phase 12's token total; the seven requester tests and both `unchanged` values are recomputed from their lines and records in the verify; verify_attempts[], recipe_verdict and deployment… | closed |
| T-13-51 | Repudiation (false assurance) | a hand-off that hides a degraded outcome (a held verify, a watchdog containment, an unproven `unchanged`) or a sweep that claims more than it can detect | medium | mitigate | the Degraded outcomes section recomputed from the records by the verify; `sweep.limits` states what the pattern sweep cannot prove; raw probe error bodies are a sweep rule so records carry parsed fields only | closed |
| T-13-52 | Repudiation | a resumed run re-pinning a changed manifest, or a half-written archive certified as complete | medium | mitigate | step 0 verifies against the pin already committed and rewrites nothing; the archive is built in a temporary sibling and renamed only after its manifest verifies; the verify compares the working pin with HEAD's and ref… | closed |
| T-13-SC | Tampering | npm/pip/cargo installs | high | mitigate | no install in this plan | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on (high) count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

### Residuals (the mitigation is present; what is left over is recorded)

| ID | Threat Ref | Residual |
|----|------------|----------|
| R-1 | T-13-11, T-13-37 | `laya-gates-selftest` backs up the config, proof and plan files but not `models/decide/eligibility{,-upload}-$SRV.log`; its laya-deploy and laya-upload rows rewrite the default (antisemite) id's logs, and under `LAYA_SELFTEST_SERVER=aprender-mcp-decide-motivation` they would overwrite 2 of the 12 archived ze-kasher-dev files, which the manifest would detect and the archive could restore. Not triggered: the manifest verifies 12/12. Tracked in deferred-items.md. |
| R-2 | T-13-20 | No recipe refuses a redeploy or verify of the OAuth-on server when LAYA_M2M_FILE is absent; covered only by the operational note in 13-HANDOFF.md §9. Containment is reversible. |
| R-3 | T-13-17 | TOKEN_HOST_RE admits any `*.pmcp.run` / `*.amazoncognito.com` host. It is safe only because the host must be chosen explicitly (the environment or the gitignored note) and must equal the client file's token_url (AUTH token_host.shared_pool_note). |
| R-4 | T-13-49 | The exposure watchdog was an ephemeral `/tmp` script; only its STARTED and STOPPED lines are committed (DEPLOY auth.watchdog). |
| R-5 | (F-13-08-3) | The M2M helper has no system CA-bundle fallback, so python.org Python 3.14 needs SSL_CERT_FILE. It fails closed (no credential is sent), so this is availability only. |

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-1 | T-13-31 | Live timing samples bump the motivation function's configuration (4 changes, motivation function only); the other functions are recorded unchanged. The disposition was `accept` in the 13-09 register. | plan register (13-09) | 2026-10-27 |
| AR-2 | 13-08 Threat Flag (unregistered) | The server uses the shared counter-coach Cognito pool, so any client in that pool holding the server's scopes can call it. This was the owner's decision during 13-08 ("Can't you use that token URL?") and is documented to the partner in 13-HANDOFF.md §4. | owner (13-08) | 2026-10-27 |
| AR-3 | (unregistered; m2m_token.py header, AUTH helper.accepted_risk) | The minted token is readable by same-user processes through the probe child's environment; declared in the helper and the AUTH record. | plan 13-06 | 2026-10-27 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-10-28 | 53 | 53 | 0 | gsd-security-auditor (ASVS L1, block_on high) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-10-28

## Security Audit 2026-10-28

| Metric | Count |
|---|---|
| Threats found | 53 |
| Closed | 53 |
| Open | 0 |
