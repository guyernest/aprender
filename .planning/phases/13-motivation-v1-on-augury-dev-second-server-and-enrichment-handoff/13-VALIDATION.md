---
phase: "13"
slug: "motivation-v1-on-augury-dev-second-server-and-enrichment-handoff"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: validated
nyquist_compliant: true
wave_0_complete: true
created: "2026-10-27"
---

# Phase 13 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Python `--selftest` modes under `uv run --project scripts/laya_train --frozen python -I` (numpy + pyyaml) and stdlib `python3 -I`; `just` selftest recipes behind an aws recorder (AWS CALLS: 0); `scripts/laya_gates.tsv` rows swept by `just laya-gates-selftest`; `pv validate` / `pv lint`; `cargo test` integration targets |
| **Config file** | `scripts/laya_train/pyproject.toml` + `uv.lock`; `scripts/laya_gates.tsv`; `contracts/motivation-enrichment-v1.yaml` (plan 13-02) |
| **Quick run command** | the touched module's selftest: `uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --selftest`, `... select_motivation.py --selftest`, `python3 -I scripts/laya_deploy/m2m_token.py --selftest`, `just _laya-region-selftest`, `just _laya-target-selftest`, `just _laya-auth-selftest` |
| **Full suite command** | `LAYA_DEPLOY_SELFTEST_POSITIVE=0 just laya-deploy-selftest` and `lockf -k -t 3600 /tmp/aprender-laya-real-weights.lock just laya-gates-selftest`, plus `cargo test -p aprender-core --test monorepo_invariants` |
| **Estimated runtime** | quick: 1-60 s each; full: the deploy selftest takes several minutes (it builds the arm64 bootstrap and nests the region, target and auth selftests); the full gates sweep takes tens of minutes (the armed deploy selftest's multi-GB laya-verify leg plus the disarmed rows), run in the background with its log pinned |

Spec-less probe fallback: SKIPPED (visible). The phase has no requirement ids to probe; edge cases come from CONTEXT D-01..D-20 and RESEARCH, and every D-NN is cited by at least one plan's must_haves (decision coverage 20/20).

---

## Sampling Rate

- **After every task commit:** Run the touched module's quick selftest (above)
- **After every plan wave:** Run the full suite command (deploy selftest with the positive run disarmed, the gates sweep under the real-weights lock)
- **Before `/gsd-verify-work`:** Full suite must be green (plan 13-10 Task 2 records it in 13-CLOSURE-EVIDENCE.json)
- **Max feedback latency:** 60 seconds for the quick selftests
- **Status capture:** never read `$?` through a pipe (CLAUDE.md rule 1); `cmd > log 2>&1; rc=$?`; `cmp`, not `diff`, for byte identity (rtk hook); the Bash tool runs zsh, recipe bodies run bash

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 13-01-01 | 01 | 1 | D-05, D-20 | T-13-01, T-13-02 | region read positionally and validated before interpolation; template override only under DRY_RUN; both config paths restored; `_laya-state-snapshot` unchanged around the selftest | offline selftest | `just _laya-region-selftest` + snapshot cmp + pre-phase render cmp | ✅ | ✅ green |
| 13-01-02 | 01 | 1 | D-20, D-05 | T-13-01, T-13-03, T-13-38 | zero/two-line templates, malformed and injecting regions refused without echo; live-mode template refusal (R7); bucket argv per region; the deploy selftest exports this run's eligibility result before restoring the historical logs | offline selftest + gates rows | `just _laya-region-selftest`; `LAYA_GATES_ONLY=region-selftest,weights-bucket-region,prepare-motivation-args just laya-gates-selftest`; deploy selftest with a planted historical eligibility log + snapshot | ✅ | ✅ green |
| 13-01-03 | 01 | 1 | D-20 | T-13-01, T-13-38 | M1..M12 turn the guard RED in its new scope (M9/M10: row_verify RED without the export or reading the historical path; the restored run accepts the deployed artifact); the pre-phase render's nested-just composition recorded | mutation | in-place mutation + `git checkout -- justfile`; `LAYA_GATES_ONLY=verify` under the real-weights lock; evidence check | ✅ | ✅ green |
| 13-02-01 | 02 | 1 | D-13, D-14, D-15, D-16, D-01, D-03 | T-13-04, T-13-39 | closed formats, enums and caps declared; every enforced bound structured (patterns compile and are probed); the d15_amendment explicit | contract | `"$PV" validate contracts/motivation-enrichment-v1.yaml` + pyyaml declaration and structured-bound check | ✅ | ✅ green |
| 13-02-02 | 02 | 1 | D-13 | T-13-04 | corpus derived files current, ratchet unmoved; the corpus-wide lint names no error for the new contract | contract hygiene | `"$PV" lint` (file and corpus-wide observation), census cmp, `pv extract contracts --check` (clean worktree), `check_ont_ratchet.sh`, ont7 + readme_contract tests | ✅ (gates exist) | ✅ green |
| 13-11-01 | 11 | 1 | D-06, D-02 | T-13-37, T-13-52 | the ze-kasher-dev local record archived (temporary sibling, then rename) BEFORE any writer; ordering and a cross-check against Phase 12's committed record, mtimes supporting; the manifest's sha256 pinned once, never re-pinned on a resume | local baseline | `shasum -a 256 -c MANIFEST.sha256` + evidence check (cross-check, ordering, pin vs HEAD, copies cmp-equal) | ✅ | ✅ green |
| 13-03-01 | 03 | 2 | D-09, D-10, D-11 | T-13-06, T-13-09, T-13-40 | no-flag bytes equal v1 by a check; eval frozen under --enrichment; in-repo enrichment refused; source ids, never file names | unit + real-CSV run | `prepare_motivation.py --selftest`; no-flag `unchanged (byte-identical)`; fixture merge cmp (sentinel in the file name) | ✅ (selftest exists, extended) | ✅ green |
| 13-03-02 | 03 | 2 | D-11, D-12, D-15, D-16, D-09 | T-13-07, T-13-08, T-13-40 | every rule and all 17 refusals text-free, no other name; D-16 semantic manifest equality; structured contract mirror with boundary cases | unit | `prepare_motivation.py --selftest` (>= 75 cases) | ✅ (extended) | ✅ green |
| 13-03-03 | 03 | 2 | D-10 | T-13-06 | N1..N9 RED (coupled with gate row prepare-motivation-args, which no wave-2 sibling sweeps) | mutation | in-place mutation + evidence check | ✅ | ✅ green |
| 13-04-01 | 04 | 2 | D-06 | T-13-10, T-13-11, T-13-41 | explicit target with the account; PMCP_API_URL guard; per-profile state; isolation; a planted augury config restored by the region selftest | offline selftest | `just _laya-target-selftest` (ISOLATION 1-3), AUGURY RESTORE block, live `_laya-target-check augury-dev eu-west-1` and the PMCP_API_URL refusal (read-only) | ✅ | ✅ green |
| 13-04-02 | 04 | 2 | D-06, D-19 | T-13-10, T-13-11, T-13-13, T-13-42, T-13-43 | every AWS-touching recipe wired IN ORDER (target, account, first AWS line); live account check; foreign endpoint refused before the probe build; teardown reachable from containment; ambient target vars cannot steer offline cases; the motivation-id selftest leaves the ze-kasher-dev record unchanged | offline selftest + gates rows | `just _laya-target-selftest` under stale ambient values (TARGET, ACCOUNT, ordered WIRING, TEARDOWN REACHABLE); filtered gates sweep (excludes prepare-motivation-args, coupled with 13-03); deploy selftest for the motivation id + snapshot; 13-11 manifest + pin after it | ✅ | ✅ green |
| 13-04-03 | 04 | 2 | D-06 | T-13-10, T-13-11, T-13-41, T-13-42, T-13-43 | K1..K10 and R-AUG RED; default profile unchanged (nested-just composition recorded) | mutation | in-place mutation + evidence check; 13-11 manifest + pin | ✅ | ✅ green |
| 13-05-01 | 05 | 2 | D-17, D-14, D-03 | T-13-14, T-13-15 | seeded rule, disjoint slices, text-free output, no label column; shuffled input gives the same selected rows; structured record mirror with boundary cases; bounded control population | unit + synthetic run | `select_motivation.py --selftest`; 1500-record synthetic run | ✅ | ✅ green |
| 13-05-02 | 05 | 2 | D-17, D-16 | T-13-15, T-13-44 | the note's rule region is the verbatim `--print-rule` output; prose adds no number | doc check | `--print-rule` regenerated and compared with the marked region; prose-number check | ✅ | ✅ green |
| 13-06-01 | 06 | 3 | D-07 | T-13-16, T-13-17, T-13-45, T-13-46 | token from a mode-600 file (descriptor checks) reaches only the probe's environment and the wire; no redirect followed; form-body minting required (Basic only with --allow-basic, NOT partner-ready) | unit + loopback chain | `m2m_token.py --selftest` (redirect, auth-method, file-mode, argv, expires cases); `just _laya-auth-selftest` (CHAIN; must fail under stub mismatch) | ✅ | ✅ green |
| 13-06-02 | 06 | 3 | D-07, D-06 | T-13-18, T-13-19, T-13-20 | 401 on tools/list then tools/call; both live re-exec branches executed through shims; token-host precedence; no argv bearer; the auth selftest restores every file it writes under both state dirs | offline selftest + gates rows | `just _laya-auth-selftest` (RE-EXEC LIVE, TOKEN HOST) + snapshot; filtered gates sweep; deploy selftest for the motivation id + snapshot; 13-11 manifest + pin after it | ✅ | ✅ green |
| 13-06-03 | 06 | 3 | D-07 | T-13-16, T-13-45, T-13-46 | A1..A10 RED | mutation | in-place mutation + evidence check; 13-11 manifest + pin | ✅ | ✅ green |
| 13-07-01 | 07 | 4 | D-18, D-19, D-06, D-02, D-05 | T-13-21, T-13-22, T-13-24, T-13-47, T-13-48 | read-only probe gate with three-state baselines; the selftest BEFORE the render and a post-render restore check; the wave-1 archive manifest (13-11) re-verified; augury bucket only; the remote object's sha256 | live (read-only + one PutObject) | evidence check + `shasum -a 256 -c MANIFEST.sha256` + the 13-11 pin + the config's recorded sha256 | n/a (live) | ✅ green (record-verified) |
| 13-07-02 | 07 | 4 | D-18 | T-13-23 | owner login; expires_at only; the owner prepared for the bounded open window | checkpoint | token gate line >= 45 min | n/a | ✅ green (record-verified) |
| 13-08-01 | 08 | 5 | D-06, D-18, D-02, D-07 | T-13-26, T-13-27, T-13-49 | publish on augury-dev timed from the DEPLOYING line; endpoint check; the recipe's identity; the watchdog started; the owner asked before any optional check | live | evidence check + `_laya-endpoint-check` (exposure start, deadline, watchdog record) | n/a (live) | ✅ green (record-verified) |
| 13-08-02 | 08 | 5 | D-07, D-02 | T-13-25, T-13-29, T-13-49 | open interval measured from the exposure start; unauthenticated tool calls 401 within the bound; watchdog stopped; readback and isolation after OAuth | live + checkpoint | `_laya-auth-refusal-check <endpoint> <seconds to the deadline, at most 600>` + evidence check + manifest and its 13-11 pin | n/a | ✅ green (record-verified) |
| 13-08-03 | 08 | 5 | D-07, D-08 | T-13-28 | authenticated identity minted with form-body credentials; authenticated negative control; token-host note; no credential or path committed | live + checkpoint | `laya-probe-auth` logs + evidence regex | n/a | ✅ green (record-verified) |
| 13-09-01 | 09 | 6 | D-07 | T-13-30, T-13-31, T-13-50 | authenticated timings, reported not gated; verify_attempts[], recipe_verdict and deployment_acceptance separate | live | evidence normalization check | n/a | ✅ green (record-verified) |
| 13-09-02 | 09 | 6 | D-07 | T-13-50 | owner decision after a failed verify (skip when verified); a second decision after a failed re-verify | checkpoint | reply record check | n/a | ✅ green (record-verified) |
| 13-09-03 | 09 | 6 | D-07, D-06, D-02, D-03, D-08 | T-13-32, T-13-33, T-13-50 | authenticated replay with a successful 4-text call; requester tests and `unchanged` derived from their records; ze-kasher-dev unchanged since before the phase; partner repo unchanged | live | replay record check (derived requester tests, derived unchanged) + manifest and its 13-11 pin + partner status cmp | n/a | ✅ green (record-verified) |
| 13-10-01 | 10 | 7 | D-08, D-06, D-09, D-16, D-17, D-01, D-03, D-04 | T-13-34, T-13-35, T-13-51 | record-true hand-off without credentials: record-keyed numbers, degraded outcomes, form-body statement, the d15_amendment for the owner | doc check | hand-off check against the records (every table row read from its key) | n/a | ✅ green (record-verified) |
| 13-10-02 | 10 | 7 | D-04, D-06, D-02 | T-13-34, T-13-36, T-13-51 | final tree green; out-of-scope surfaces unchanged; sweep clean with its limits stated; the manifest-compat fixture check; all checked by the verify itself | suite + diff | the verify re-runs twelve suites (monorepo_invariants, dup-bin guard, census, clean-worktree graph check, both prepare_motivation runs, select / m2m / region / target / auth selftests, enrichment-manifest-compat), reads the deploy-selftest and full gates-sweep logs (fresh, verdict last), re-checks the 13-11 manifest and pin, repeats the sweep; `git diff --quiet 61586f888 HEAD -- ...` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Each Wave 0 item is created by the task that first needs it (tracer-first), not by a separate scaffold plan:
- [x] `_laya-region-selftest` (+ LAYA_DEPLOY_TEMPLATE under DRY_RUN=1, region ERE, R7 live refusal, BUCKET fake-aws table), `_laya-state-snapshot`, the deploy selftest's eligibility export and armed_deploy_selftest's freshness check, and the gates rows — plan 13-01
- [x] `contracts/motivation-enrichment-v1.yaml` (structured bounds, d15_amendment) — plan 13-02
- [ ] the baseline of the ze-kasher-dev local record (`models/decide/ze-kasher-dev-archive-20261027/MANIFEST.sha256`, built in a temporary sibling, cross-checked against Phase 12's committed record, mtimes supporting, the pin in `13-BASELINE-EVIDENCE.json`) — plan 13-11, wave 1, before any phase-13 writer; re-verified by 13-04, 13-06, 13-07, 13-08, 13-09 and 13-10
- [x] `prepare_motivation.py` enrichment cases, `check_v1_identity`, the manifest, the structured contract mirror — plan 13-03
- [x] `_laya-target-check` (accounts, PMCP_API_URL), `_laya-account-check`, `_laya-state-dir`, `_laya-endpoint-check`, `_laya-target-selftest` and their rows — plan 13-04
- [x] `select_motivation.py --selftest` / `--print-rule` — plan 13-05
- [x] `scripts/laya_deploy/m2m_token.py --selftest` / `--stub-server`, `laya-probe-auth`, `_laya-auth-refusal-check`, `_laya-auth-selftest` (RE-EXEC LIVE shims) and their rows — plan 13-06
- No package install (uv --frozen, stdlib); no `ci/explicit-test-commands.d/` fragment is added: the laya selftests stay local-only like every Phase 8-12 laya selftest (CI never installs the laya_train stack). The cross-AI review (Round 1, 35fc2f803) asked for CI wiring; the OWNER decided on 2026-10-27 to keep them local-only, recorded as a deferral in 13-10-PLAN.md's ledger.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| pmcp.run login for the augury-dev tenant | D-18 | the session belongs to the owner; the executor never logs in | owner runs `PMCP_TARGET=augury-dev cargo pmcp deploy login`; the token gate reads expires_at only (13-07 Task 2) |
| OAuth enablement on the new server | D-07 | cargo-pmcp cannot render an auth-enabled descriptor for pmcp-run; admin UI only | owner enables OAuth right after the publish (asked the moment the deploy recipe returns; a watchdog contains the function at the exposure start + LAYA_MAX_OPEN_S deadline); the executor then runs `_laya-auth-refusal-check` (13-08 Task 2) |
| M2M client creation and its mode-600 file | D-07, D-08 | credentials belong to the owner and go to the partner out of band | owner creates a client that accepts form-body credentials, writes the JSON outside the checkout, replies with the path (and a Cognito token host if any); the executor proves an authenticated identity call minted `(auth form)` and an authenticated negative control (13-08 Task 3) |
| Owner-run upload / deploy when the permission classifier refuses the executor | D-18 | Phase 10 precedent ("[Production Deploy]") | owner runs the printed command; the executor confirms read-only (head-object, identity, readback) |
| Decision after a failed deploy-verify | D-07 | a live endpoint's fate is the owner's call | contain / hold-serving / reverify-once (13-09 Task 2), skipped when verified; after a failed re-verify only contain / hold-serving |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 60s (quick selftests)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-10-28 (validate-phase audit; no gaps)

---

## Validation Audit 2026-10-28

Evidence used:
- **Quick selftests:** re-run on HEAD 6e5164104 (prepare / select / m2m / region / target / auth), all rc 0, and the antisemite eligibility log was cmp-equal before and after.
- **Mutation records, every mutant RED:** REGION M1..M12 (12), INGEST N1..N9 (9), TARGET K1..K10 + R-AUG (11), AUTH A1..A10 (10).
- **Full suite:** 14 suites rc 0 in 13-CLOSURE-EVIDENCE.json, including the disarmed deploy selftest and the 44-row gates sweep.
- **Live and checkpoint tasks (13-07..13-09):** executed once against augury-dev and verified by checks against their committed records. They cannot be re-run offline, which is why they are marked "record-verified".

## Validation Audit 2026-10-28

| Metric | Count |
|---|---|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |
