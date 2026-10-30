---
phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
plan: 22
subsystem: laya-train-deploy
tags: [laya, finalizer, redeploy, provenance, sidecar, pmcp-run, oauth, d-10, wr-03, gap-closure]

requires:
  - phase: 14-17
    provides: "finalizer class fixes (WR-01, WR-02, IN-01, the Catch reduction of WR-03) in code"
  - phase: 14-18
    provides: "request-side WR-03 sanitization in code (dispatch error text, backend error text, task-store errors)"
  - phase: 14-19
    provides: "the finalizer provenance sidecar bootstrap.build.json and the infra provenance check"
  - phase: 14-20
    provides: "laya_drift.py bashrs-gate, used to lint the gitignored owner helper"
provides:
  - "The finalizer aprender-laya-train-finalizer-dev in ze-kasher-dev is provably the reviewed build: deployed bootstrap sha256 fc796b8b...3fab equals the sidecar written by the synth at 275ad9611 (finalizer.equal true)"
  - "The request function aprender-mcp-laya-train is republished with the request-side WR-03 sinks, OAuth on, grant restored after the recipe saw the 401, and an authenticated tools/list naming exactly train, train_status, validate_dataset"
  - "14-GAP-REDEPLOY-EVIDENCE.json status complete; 14-REDEPLOY-RUNBOOK.md (Section A finalizer, Section B request function)"
affects: [14-VERIFICATION re-run (WR-01/WR-02/WR-03 now live), laya-trainer-deploy recipe (pmcp login precheck follow-up), _laya-crates-root-swap recipe (label and backup-path follow-up)]

plan_head_before: 275ad9611a39f8838c698361e50acedc755f8c9c
plan_head_after: 7e4890ef9efa1def5a4fec7586b2166c7098dae6

actuals:
  tokens: 10100
  tasks: 3
  commits: 5

tech-stack:
  added: []
  patterns:
    - "Deploy verified by hash, not assumed: the post-deploy helper downloads the live package, checks it against the function's CodeSha256, hashes the entry bootstrap, and compares with the build sidecar"
    - "Owner-run deploy: the executor prepares and dry-runs (AWS CALLS: 0), the owner runs each bang-prefixed command, the orchestrator records replies, the executor fills only the derived fields"

key-files:
  created:
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-REDEPLOY-RUNBOOK.md
    - .planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out/14-GAP-REDEPLOY-EVIDENCE.json
  modified: []

key-decisions:
  - "Task 3: the owner chose redeploy-now; the request function was republished the same evening and the whole WR-03 class is live"
  - "Finalizer diff verdict go: exactly one change, Finalizer Code.S3Key 7288da6d...cd14 to c367d471...7b2a; no IAM section, no resource added or removed; the deploy's own diff equals the saved diff"
  - "The grant-absent window is reported as bounds (1659 s to 3664 s), not a number: no file holds a timestamp for attempt 1's revoke or for B4 GRANT OK"

patterns-established:
  - "A recipe that revokes a grant and then calls an external CLI needs that CLI's login checked before the revoke, or a stale token opens the revoke window with nothing published"

requirements-completed: [D-01, D-02, D-10, D-11, D-22]

coverage:
  - id: D1
    description: "Offline readiness bundle (offline suite, synth with sidecar at 275ad9611, dry runs with AWS CALLS: 0, bashrs-gated read-only helper, runbook, evidence skeleton)"
    requirement: D-02
    verification:
      - kind: other
        ref: "Task 1 verifies (OFFLINE SUITE OK 7, TRAINER SYNTH OK env=dev, DRY_RUN infra deploy AWS CALLS: 0, READINESS OK), commit e1050b42a"
        status: pass
      - kind: other
        ref: "python3 -I scripts/laya_deploy/laya_drift.py bashrs-gate models/laya-trainer/redeploy-finalizer-check.sh (re-run at close-out: BASHRS GATE PASS)"
        status: pass
    human_judgment: false
  - id: D2
    description: "Finalizer redeployed and verified by hash: deployed bootstrap sha256 equals the sidecar sha256"
    requirement: D-02
    verification:
      - kind: other
        ref: "models/laya-trainer/redeploy-finalizer-sha.txt: verdict EQUAL, code_sha256_matches_download true; evidence finalizer.equal true"
        status: pass
    human_judgment: false
  - id: D3
    description: "Request function republished under the D-10 order (grant revoked first, 401 observed by the grant recipe, grant restored, authenticated list-tools)"
    requirement: D-10
    verification:
      - kind: manual_procedural
        ref: "owner-run B0-B5; last lines in 14-GAP-REDEPLOY-EVIDENCE.json request_function.steps"
        status: pass
    human_judgment: true
    rationale: "Every step was owner-run against the live account and pmcp.run; the executor only read the logs and the owner's recorded replies"

duration: 80min
completed: 2026-10-30
status: complete
---

# Phase 14 Plan 22: Finalizer and request-function redeploy Summary

**The finalizer running in ze-kasher-dev now has the sha256 the build recorded (fc796b8b...3fab, built at 275ad9611), and the request function was republished with OAuth on, the grant restored after the recipe saw the 401, and an authenticated tools/list naming train, train_status and validate_dataset. WR-01, WR-02, IN-01 and WR-03 (the Catch reduction and the request-side sinks) are live. The executor made no AWS call.**

## Performance

- **Duration:** about 80 min, from Task 1 (readiness at 22:21Z) to close-out (23:41Z), with owner steps in between
- **Started:** 2026-10-30T22:21:00Z
- **Completed:** 2026-10-30T23:41:20Z
- **Tasks:** 3 (1 tracer, 1 owner action, 1 owner decision followed by owner actions)
- **Files modified:** 2 committed (runbook, evidence) plus this summary. The helper, diff, logs and sha file are gitignored under `models/laya-trainer/`

## What is live now

| Component | Fixes now in production | How it is known |
|---|---|---|
| Finalizer `aprender-laya-train-finalizer-dev` | WR-01 (class from records), WR-02 (total finalization), IN-01, WR-03 Catch reduction | Saved diff: one change, the Finalizer `Code.S3Key`. Deploy log ends `INFRA OK deploy` (47.86 s), and its own diff is the same single change. The hash helper downloaded the live package (matched `CodeSha256`), hashed `bootstrap` to `fc796b8b47be559f6bc3a067f0b294b9fe5a1b4ef1594763eaa7d695fcf13fab`, and that equals both the local and the deployed sidecar (`verdict EQUAL`). The deployed sidecar names commit `275ad9611`, closure clean, 12 crates, `aarch64-unknown-linux-gnu`. |
| Request function `aprender-mcp-laya-train` | WR-03 request side: dispatch error text, the backend error text in `train` and `train_status`, task-store errors | B2 attempt 2 `DEPLOY OK aprender-mcp-laya-train` (`deploy-dev.json` `deployed_at` 2026-10-30T23:07:43Z). B3 first 401 at 23:35:22Z. B4 `GRANT OK` after the recipe saw the 401 itself. B5 authenticated `tools/list` ok:true with exactly the three tools. |

Plan-level verification (run at close-out, no AWS call): evidence `status complete`, `finalizer.equal true`, diff verdict `go`, decision `redeploy-now`, every final-attempt step ok, 10+ owner replies all `recorded_by: orchestrator`. The evidence and runbook have no 12-digit run, no ARN prefix, no presigned-URL marker and no token. Between the plan base `275ad9611` and HEAD, only the two `.planning/` files changed: no contract, crate, CDK, script, justfile or workflow change. The working tree has none either, so D-22 holds and the register stays 11.

## Task Commits

1. **Task 1 (tracer): offline readiness bundle, owner helper, runbook**: `e1050b42a` (docs)
2. **Task 2 (owner action): finalizer diff, deploy, hash check, with replies recorded by the orchestrator**: `ac4390876` (docs)
3. **Task 3 (owner decision): redeploy-now recorded**: `1f0b2a591` (docs)
4. **Task 3 (owner action): B0 to B5 replies recorded by the orchestrator**: `43700e889` (docs)
5. **Close-out: executor-owned evidence fields filled**: `7e4890ef9` (docs)

## Files Created/Modified

- `14-REDEPLOY-RUNBOOK.md`: the exact owner commands. Section A covers the finalizer and Section B the request function, each step with an expected output and a STOP condition.
- `14-GAP-REDEPLOY-EVIDENCE.json` (`laya-gap-redeploy-evidence-v1`, status `complete`): readiness, `finalizer` (diff verdict, deploy last line, deployed, sidecar and deployed-sidecar sha256, `equal: true`), `request_function` (decision, steps B0 to B5 including the failed attempt, window bounds) and `owner_replies`. The executor did not touch `owner_replies`; the fill script asserted it byte-equal before and after.

## Decisions Made

- The owner chose **redeploy-now** (Task 3), so nothing is deferred on the request side.
- The grant-absent window has **bounds, not a measurement**. Attempt 1 wrote its log to the same path that attempt 2 overwrote, so no file holds the time of attempt 1's `GRANT REVOKED`. No file holds the time of B4's `GRANT OK` either.
  - **Lower bound, 1659 s (27 min 39 s), from files.** The grant was absent from at least `deployed_at` 23:07:43Z (attempt 2 found nothing to revoke) to `first_401_utc` 23:35:22Z, and B4 ran after B3.
  - **Upper bound, 3664 s (61 min 4 s), from git commit times, not files.** Commit `1f0b2a591` (22:36:40Z) recorded the decision before B0 ran, and commit `43700e889` (23:37:44Z) recorded B4.
- **Open-endpoint interval: at most 1659 s, possibly zero.** That is publish to first observed 401, against 10114 s in 14-13. The attempt 2 output reported `oauth_enabled: true`, so this `--no-oauth` publish did not reset OAuth, unlike 14-13. The endpoint may never have been open.

## Deviations from Plan

### Owner-run issues (recorded, not auto-fixed: the recipe is outside this plan's files)

**1. [Rule 1 - Bug, process] B2 attempt 1 revoked the grant and then failed on a stale pmcp.run login**
- **Found during:** Task 3, B2
- **Issue:** `laya-trainer-deploy` revokes the inline grant first (fail closed, as designed). Then `cargo pmcp deploy` failed with `Token refresh failed: invalid_grant` before any upload: `DEPLOY FAILED rc=1`, nothing published, crates root restored byte-identical. The runbook had no pmcp.run login precondition, so a stale token opened the grant-absent window and published nothing. The owner re-ran `cargo pmcp deploy login --target pmcp-run`, and attempt 2 succeeded.
- **Fix (recommended follow-up, not made here):** add a pmcp.run login and token-freshness precheck to `laya-trainer-deploy` (or a B1.5 runbook step) that runs **before** the grant revoke. A refusal there would cost nothing.
- **Impact:** harmless by construction. The revoke is the safe direction, and train_status was unavailable a little longer. No spend right existed while the grant was absent.

**2. [Cosmetic, follow-up] The swap recipe mislabels the laya deploy and shares the decide backup root**
- **Found during:** Task 3, B2 (`redeploy-request-deploy.log` line 7)
- **Issue:** `_laya-crates-root-swap` (`justfile:2290`) prints `SWAPPED crates: decide config installed alone (backup models/decide/swap-backup/crates)` during a laya-train deploy (the echo is at `justfile:2354`). Its backup directory `BK="models/decide/swap-backup/..."` (`justfile:2297`) sits under the decide state tree.
- **Risk:** the recipe refuses when `$BK` already exists, so a laya swap colliding with an unfinished decide swap of the same root fails closed rather than overwriting anything. `laya_trainer_ops.py` selftest fixtures (line 2866) also carry the old label, so a relabel must update both.
- **Fix:** none in this plan. D-22 and the plan scope exclude recipe changes. Recorded as a follow-up.

---

**Total deviations:** 2 recorded (1 process bug in the owner run, 1 cosmetic). Neither was auto-fixed, because both sit in recipes outside this plan's `files_modified`.
**Impact on plan:** none on the outcome. Both redeploys completed and verified.

## Issues Encountered

- B0's first paste broke on a line-wrapped nested quote (zsh `unmatched`). The owner re-ran the same query with the ARN read into a variable first. The orchestrator recorded this in `owner_replies`.
- The `rtk` hook filtered some `git diff --name-only`, `wc` and `ls` output during close-out. Those checks were re-run through `rtk proxy` or `python3 -I`.

## Still open (not closed by this plan)

- **D-16:** not reproduced (as in 14-16 and 14-21); not planned here.
- **Packing-cancel live check:** not run; not planned here.
- **D-15 amend-per-study:** the `contracts/laya-parity-v1.yaml` amendment chosen in 14-21 still needs its own planned change set and review. `motivation-v1-sm` leaves S3 around 2026-11-28.
- **Recipe follow-ups:** the pmcp.run login precheck before the grant revoke (deviation 1), and the `_laya-crates-root-swap` label and backup path (deviation 2).
- **gsd-tools state verbs:** `state.*` verbs fail on the Phase 4 `04-VERIFICATION` `gaps_acknowledged` status (a known limitation). That file was not edited, and STATE.md was not advanced through `state.*`.

## User Setup Required

None. The M2M file stays outside the repository, `models/laya-trainer/m2m-file-path` holds only its path, and `.env.m2m` was never read.

## Next Phase Readiness

This is the last plan in phase 14 (22/22). The verification gaps WR-01, WR-02 and WR-03 are now closed in production, not just in code, so a `/gsd-verify-work 14` re-run can check them against the live system. D-16, the packing-cancel check and the D-15 amendment remain open, as listed above.

## Self-Check: PASSED

- FOUND: `14-REDEPLOY-RUNBOOK.md`, `14-GAP-REDEPLOY-EVIDENCE.json`
- FOUND in HEAD's ancestry: `e1050b42a`, `ac4390876`, `1f0b2a591`, `43700e889`, `7e4890ef9`
- Plan verification: `PLAN VERIFICATION OK`. bashrs-gate re-run: `BASHRS GATE PASS`. The identifier scan of the committed files is clean.

---
*Phase: 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out*
*Completed: 2026-10-30*
