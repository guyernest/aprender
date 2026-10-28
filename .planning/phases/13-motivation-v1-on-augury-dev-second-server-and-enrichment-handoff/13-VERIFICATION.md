---
phase: 13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff
verified: 2026-10-28T09:30:00Z
status: passed
score: 11/11 must-haves verified
covered_files:
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-01-PLAN.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-01-SUMMARY.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-02-PLAN.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-02-SUMMARY.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-03-PLAN.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-03-SUMMARY.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-04-PLAN.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-04-SUMMARY.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-05-PLAN.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-05-SUMMARY.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-06-PLAN.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-06-SUMMARY.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-07-PLAN.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-07-SUMMARY.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-08-PLAN.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-08-SUMMARY.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-09-PLAN.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-09-SUMMARY.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-10-PLAN.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-10-SUMMARY.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-11-PLAN.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-11-SUMMARY.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-ACCEPTANCE-REPLAY.json
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-BASELINE-EVIDENCE.json
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-CLOSURE-EVIDENCE.json
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-DEPLOY-EVIDENCE.json
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-DEPLOY-OWNER-REPLY.json
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-HANDOFF.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-INGEST-EVIDENCE.json
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-OAUTH-OWNER-REPLY.json
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-PROBE-EVIDENCE.json
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-REVIEW.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-SELECTION-NOTE.md
  - .planning/phases/13-motivation-v1-on-augury-dev-second-server-and-enrichment-handoff/13-VERIFY-OWNER-REPLY.json
  - README.md
  - contracts/census.json
  - contracts/contracts.nt
  - contracts/motivation-enrichment-v1.yaml
  - justfile
  - scripts/laya_deploy/m2m_token.py
  - scripts/laya_gates.tsv
  - scripts/laya_train/prepare_motivation.py
  - scripts/laya_train/select_motivation.py
covered_digest: "v4:sha256:a71ffa0e929e4be277aaf1df6372ac321cec58aeb91c316e5361b9c3bc4e0cc1"
behavior_unverified: 0
overrides_applied: 0
human_verification:
  - test: "Owner decides the disposition of review finding CR-01 (an auth-on redeploy has no guard) BEFORE any redeploy of the augury motivation server: fix it (a persisted oauth-enabled marker that refuses a token-less laya-deploy, plus a post-deploy _laya-auth-refusal-check that fails loudly) or record it as deferred with the hand-off's manual procedure as the accepted control"
    expected: "13-REVIEW-DISPOSITION.md CR-01 moves from open to fixed or deferred with a reason; until then nobody runs laya-deploy against augury-dev"
    why_human: "It is a risk-acceptance decision on a redeploy path that this phase did not exercise; the live deployment is not affected (see the CR-01 assessment in the report)"
  - test: "Owner confirms the d15_amendment of contracts/motivation-enrichment-v1.yaml (evidence level `none`, evidence describes the RECORDED function, refusal of other+clear / social_affiliative+none / conspiratorial+none) before the partner builds the annotation sheet"
    expected: "The owner confirms, or asks for a different convention (which would be motivation-enrichment-v2); the contract's status line stops reading `pending`"
    why_human: "The contract itself declares the item pending the owner's confirmation; the enrichment round cannot safely start on a format the owner has not accepted"
  - test: "Owner decides whether the uncertainty band's inert margin clause (with three labels summing to 1, `p1 - p2 < 0.20` adds nothing beyond `p1 <= 0.75`) stays as declared"
    expected: "Keep as is (the note and selftest already say so) or request new constants and a regenerated note"
    why_human: "A rule-design choice for the selection guidance; the code and the note agree byte for byte, so nothing is wrong mechanically"
---

# Phase 13: motivation-v1 on augury-dev (second server) and the first enrichment hand-off: Verification Report

**Phase Goal:** Deploy the unchanged motivation-v1 artifact (sha256 d202f495...) as a SECOND decide server from augury-dev (eu-west-1) beside the ze-kasher-dev pilot, so the experiment team can run their stored posts through it and send the output to the expert; and deliver the per-post record schema, a selection-guidance note and the enrichment-file format that `prepare_motivation.py` ingests as a second source. No retrain, no gate run, no change to the served artifact or to the ze-kasher-dev server.
**Verified:** 2026-10-28
**Status:** human_needed (every goal truth verified; three owner decisions open, none of them a gap)
**Re-verification:** No (initial verification)

Constraints honoured: no AWS, pmcp.run or network call; ze-kasher-dev not touched; the M2M credential file not read; `laya-gates-selftest` and `laya-deploy-selftest` not run; STATE.md, config.json and state.json not edited. The live truths (1-4) rest only on the committed records. I re-derived them from those records and cross-checked them against local bytes where local bytes exist.

ROADMAP has no `success_criteria` for this phase. The truths below are derived from the goal and merged with the PLAN must_haves (D-01..D-20).

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | The UNCHANGED artifact is served on augury-dev / 783960389328 / eu-west-1 under `aprender-mcp-decide-motivation`, as a second deployment | ✓ VERIFIED | Local `models/decide/laya-motivation-v1.apr` hashes to d202f495... (re-hashed). Upload: head-object 846196612 bytes, and the remote copy was downloaded and hashed equal to the pin (`remote_identity.matches`). The publish printed `https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp` with target receipt `augury-dev (pmcp-run)`, and the local `models/decide/augury-dev/.../deployment.toml` names the same endpoint. The Lambda readback after OAuth reports eu-west-1, account 783960389328, 10240 MB, arm64, Active, env sha = pin. The config sha e0c92cf0... matches its record (re-hashed). |
| 2 | Auth is ON: an unauthenticated tools/list and tools/call are refused, and an M2M token minted with form-body credentials is accepted | ✓ VERIFIED | `auth.refusal_line`: 401/401 at 05:01:09Z, re-checked at the end of the replay (05:41:22Z, after the 4 cold-bump config updates). The authenticated identity matches the pin, and every mint line ends `(auth form)`. The negative control (antisemitism artifact) minted, reached the server and was refused on identity, not on 401. Open interval: at most 681 s, under the 1800 s cap. The watchdog was stopped and contained nothing. |
| 3 | The server is usable by the experiment team: function and identity checks pass on authenticated calls, and the door is unchanged | ✓ VERIFIED | I recomputed all seven requester tests from the replay lines (n = 1, 1, 1, 7, 4): one result per text, 3 probabilities, max sum deviation 2.2e-8, label is the argmax, labels in task order, sha = pin, and 9 texts refused naming classify_max_texts. The batch of 4 succeeded at 423 tokens, equal to Phase 12. Verify attempt 1 rc 0: `DEPLOY VERIFY OK`, 4 proven-cold samples between 18.9 and 22.1 s. The door is 8 texts, 800 tokens, P 97. |
| 4 | The ze-kasher-dev pilot is live and untouched (D-02, D-06) | ✓ VERIFIED | Re-derived `ze_kasher_before.lambda.keys == ze_kasher_after.lambda.keys`: True. After-read identity sha = pin. The augury antisemite before/after keys are also equal. I re-ran the archive manifest locally: `shasum -a 256 -c` gives 12/12 OK, and sha256(MANIFEST) b01bc721... equals the 13-11 pin. The ze-kasher-dev reads were run by the owner, and the executor made no ze-kasher-dev call (F-13-07-1). |
| 5 | Per-post record schema `motivation-record-v1` declared (post_id, source, text, labels in served order, aligned probabilities, artifact_sha256, deployment triple, tokens, truncated, scored_at) | ✓ VERIFIED | `contracts/motivation-enrichment-v1.yaml` formats.motivation-record-v1 has closed keys with structured patterns, length 3, sum_tolerance 1e-5 and `this_round` triple. `select_motivation.py --selftest` (which mirrors the contract) passes. |
| 6 | Selection-guidance note: stratified by predicted function, an uncertainty band and a random control, not only high-probability rows | ✓ VERIFIED | `13-SELECTION-NOTE.md` §1 says plainly that sending only high-probability rows teaches nothing new, and that the control slice keeps the comparison honest; §1b bounds what the control can claim. I regenerated the rule region with `select_motivation.py --print-rule`: `cmp` reports it byte-identical to the note's marker region. |
| 7 | Enrichment format `motivation-enrichment-v1` declared, and `prepare_motivation.py --enrichment` ingests it as a second source (train only, eval frozen, conflict and eval-overlap drops, D-15 mapping, annotation_mode counted) | ✓ VERIFIED | Selftest: `PREPARE MOTIVATION SELFTEST OK`. Independent spot-check on my own synthetic file outside the repo (3 rows: sa+clear, consp+weak, an eval text): rows_added 2, dropped_eval_overlap 1, consp+weak mapped to other (other 188→189), `cmp` eval.jsonl equal to v1, and the manifest carries the sha and per-cell counts with no text. |
| 8 | A run without `--enrichment` writes exactly v1's bytes (D-10) | ✓ VERIFIED | Ran the no-flag build: `unchanged (byte-identical)`. task f74a3ad3, train f1d84f19 and eval ee95d71f all match the pins. |
| 9 | No retrain, no gate run, no change to the served artifact or served tooling | ✓ VERIFIED | `git diff 61586f888 HEAD` is empty for `crates/` (thin crate, shared lib, decide crates), decide-tool-boundary-v1, decide-apr-v1, laya-finetune-gate-v1, train.py, gate.py, data.py and the dup-bin allowlist. The phase's non-planning changes are justfile, the scripts, the new contract and the census/README. No retrain or gate commit appears in the phase's commits. |
| 10 | D-05/D-20: the region parameter is committed and proven offline in both regions; the triple keeps per-target local state apart (D-06) | ✓ VERIFIED | Re-ran `_laya-region-selftest` (REGION SELFTEST OK), `_laya-target-selftest` (TARGET SELFTEST OK) and `_laya-auth-selftest` (AUTH SELFTEST OK, AWS CALLS: 0). The sha256 of the models/decide logs and the augury-dev state was equal before and after, and the archive manifest still verified. |
| 11 | D-08 hand-off: motivationUrl, motivationSha and the MotivationAuthSecretArn shape, from records only, with no credential in the repo | ✓ VERIFIED | `13-HANDOFF.md` §4. I parsed all 96 record-keyed table rows and re-read each key: 0 mismatches. My sweep of the committed phase files: no JWT, no client_secret value, and the M2M path note is absent. The only real 12-digit accounts are the two deployment accounts (123456789012 and 111122223333 are documentation placeholders). |

**Score:** 11/11 truths verified (0 present-but-behavior-unverified)

### CR-01 assessment (requested explicitly)

CR-01 is real. I confirmed it in the code: `laya-deploy` derives `--no-oauth` from `[auth] enabled = false` (justfile:2347-2349). The augury config still says false, because OAuth was enabled out of band. Nothing persists that fact, so a redeploy has two bad outcomes:
- Without `LAYA_M2M_FILE`, it contains the healthy function.
- With the file set, it may silently reopen the server (RESEARCH A3, unobserved). The recipe never calls the refusal check.

**It does not block the goal.** The goal is a server that is deployed and usable by the team now. The one publish has already happened, and OAuth was enabled after it. The subsequent 4 cold-bump `update-function-configuration` calls did not reopen it: 401/401 was re-observed at 05:41:22Z, after the verify. No plan must_have requires a redeploy guard.

**It is a follow-up hardening item for redeploys.** It must be decided before anyone redeploys the augury server. The hand-off §9 documents the manual control (set `LAYA_M2M_FILE`, then run `_laya-auth-refusal-check`), but that is the "guard not on the decision surface" pattern of CLAUDE.md rule 5. I have therefore routed it as an owner decision, not a gap.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `contracts/motivation-enrichment-v1.yaml` | record + enrichment formats, mapping, merge rules, refusals | ✓ VERIFIED | 289 lines, structured bounds, d15_amendment marked pending |
| `scripts/laya_train/prepare_motivation.py` | `--enrichment`, check_v1_identity, manifest | ✓ VERIFIED | selftest OK; spot-check behaves as declared |
| `scripts/laya_train/select_motivation.py` | seeded rule, record validator, `--print-rule` | ✓ VERIFIED | selftest OK; note region regenerated equal |
| `scripts/laya_deploy/m2m_token.py` | file-to-env token helper | ✓ VERIFIED | `M2M TOKEN SELFTEST OK`; used live (8 mints in the replay) |
| `justfile` | region param, triple guards, state dir, auth recipes | ✓ VERIFIED | three selftests OK; wired into laya-deploy / laya-deploy-verify |
| `13-SELECTION-NOTE.md` | partner guidance | ✓ VERIFIED | — |
| `13-HANDOFF.md` | owner hand-off | ✓ VERIFIED | — |
| `13-DEPLOY-EVIDENCE.json`, `13-ACCEPTANCE-REPLAY.json`, owner replies | live records | ✓ VERIFIED | internally consistent; derived fields recomputed |

### Key Link Verification

| From | To | Via | Status |
|------|----|-----|--------|
| LAYA_M2M_FILE | probe Authorization header | m2m_token.py exec → APRENDER_DECIDE_PROBE_TOKEN | WIRED (offline chain in the auth selftest; live mints recorded) |
| contract | prepare/select constants | selftest mirror cases | WIRED (both selftests pass) |
| select_motivation.py constants | 13-SELECTION-NOTE.md rule region | `--print-rule` | WIRED (cmp equal) |
| `_laya-state-dir augury-dev` | models/decide/augury-dev/... | laya-deploy / verify | WIRED (augury state in its own dir; ze-kasher-dev manifest intact) |
| 13-11 manifest | "unchanged" checks | shasum -c + pin | WIRED (re-run: OK, pin equal) |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Ingest selftest | `prepare_motivation.py --selftest` | PREPARE MOTIVATION SELFTEST OK | ✓ PASS |
| v1 byte identity | `prepare_motivation.py` (no flag) | unchanged (byte-identical), three pins equal | ✓ PASS |
| Second-source ingest | `--enrichment <synthetic> --out <scratch>` | rows_added 2, eval overlap dropped 1, eval cmp-equal | ✓ PASS |
| Selector selftest | `select_motivation.py --selftest` | SELECT MOTIVATION SELFTEST OK | ✓ PASS |
| Note = code | `--print-rule` vs note region | cmp equal | ✓ PASS |
| Token helper | `m2m_token.py --selftest` | M2M TOKEN SELFTEST OK | ✓ PASS |
| Region / target / auth guards | `just _laya-{region,target,auth}-selftest` | all OK, AWS CALLS: 0, state sha unchanged | ✓ PASS |
| ze-kasher-dev local record | `shasum -a 256 -c MANIFEST.sha256` | 12/12 OK, manifest sha = pin | ✓ PASS |

Live behavior (truths 1-4) was not re-probed, because the constraints forbid network calls. The records are point-in-time observations from 2026-10-28 04:49Z to 05:48Z.

### Probe Execution

No `scripts/*/tests/probe-*.sh` is declared by this phase. The decide probe (`examples/probe.rs`) is a live-network probe, and its results are recorded in the evidence files. Step 7c: not applicable.

### Requirements Coverage

ROADMAP maps no REQ-ID to Phase 13, and REQUIREMENTS.md has no Phase 13 rows, so there are no orphaned requirements. The plans cite CONTEXT decisions instead. All 20 (D-01..D-20) are claimed by at least one plan, and each maps to a verified truth above:
- D-01, D-03, D-04: truths 9 and 11
- D-02, D-06: truths 1, 4 and 10
- D-05, D-20: truth 10
- D-07, D-08: truths 2 and 11
- D-09..D-16: truths 5, 7 and 8
- D-17: truth 6
- D-18, D-19: truth 1 (P1-P3 passed, and the existing bucket was used with no fallback)

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| justfile | 2239-2283, 2347-2353 | CR-01: an auth-on redeploy has no guard | ⚠️ Warning (owner decision) | Redeploy path only; see the assessment |
| justfile / m2m_token.py / prepare_motivation.py / select_motivation.py | per 13-REVIEW.md | WR-01..WR-09, IN-01..IN-07 still open (token lifetime and exposure, case-variant path bypass on APFS, env credentials, multi-line probe guard, WR-05 the "offline" auth selftest reads the real ~/.pmcp/config.toml, WR-07 the manifest drops the guide/model provenance, WR-08 `--out` unconstrained, WR-09 a traceback can leak a path) | ⚠️ Warning | Hardening; none falsifies a goal truth |
| 13-VALIDATION.md | 7 vs 111 | frontmatter `nyquist_compliant: false`, while the sign-off checklist ticks "`nyquist_compliant: true` set" and commit 0a8823908 says "nyquist-compliant" | ℹ️ Info | Contradicts the orchestrator's premise; a record inconsistency, not a goal gap |
| scripts/laya_deploy/m2m_token.py | — | F-13-08-3: no fallback to the system CA bundle (needs SSL_CERT_FILE on this box) | ℹ️ Info | Documented in the hand-off §9 |
| justfile (laya-gates-selftest) | — | deferred-items.md: does not restore the antisemite eligibility logs | ℹ️ Info | Pre-existing; documented |

There are no TBD/FIXME/XXX/TODO markers in the phase's added lines.

Observation: the owner's M2M client file sits under the partner checkout's directory. I checked it read-only, without reading the file: it is gitignored and untracked there. Info only.

### Human Verification Required

#### 1. CR-01 disposition before any augury redeploy
**Test:** Decide whether to fix CR-01 (oauth-enabled marker plus a post-deploy refusal check) or to defer it, with the hand-off's manual procedure as the accepted control.
**Expected:** 13-REVIEW-DISPOSITION.md CR-01 is no longer `open`.
**Why human:** A risk-acceptance decision on an unexercised redeploy path.

#### 2. Confirm the d15_amendment
**Test:** Confirm evidence level `none`, the "evidence of the RECORDED function" meaning, and the three refused pairs before the partner builds its sheet.
**Expected:** Confirmed, or a v2 is requested.
**Why human:** The contract declares it pending the owner's confirmation.

#### 3. Uncertainty-band margin clause
**Test:** Keep `p1 - p2 < 0.20 OR p1 in [0.45, 0.75]` as declared (in effect `p1 <= 0.75`), or change the constants.
**Expected:** An owner decision.
**Why human:** A design choice; the code and the note agree.

### Gaps Summary

No gaps. All eleven goal truths hold:
- The unchanged artifact is live on augury-dev / eu-west-1 with OAuth on and an authenticated acceptance.
- The ze-kasher-dev pilot is provably unchanged.
- The record schema, the selection note, the enrichment format and its ingest exist, are wired and are behaviorally exercised.
- Nothing was retrained, no gate ran, and the served tooling is untouched.

The status is `human_needed` only for the three owner decisions above. Of these, CR-01 must be closed before the next redeploy of the augury server, and the d15_amendment must be confirmed before the partner builds its annotation sheet.

---

_Verified: 2026-10-28_
_Verifier: Claude (gsd-verifier)_
