---
phase: "13"
slug: "motivation-v1-on-augury-dev-second-server-and-enrichment-handoff"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
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
| **Estimated runtime** | quick: 1-60 s each; full: several minutes (the deploy selftest builds the arm64 bootstrap; the default gates sweep runs one real-weights leg) |

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
| 13-01-01 | 01 | 1 | D-05, D-20 | T-13-01, T-13-02 | region read positionally and validated before interpolation; template override only under DRY_RUN | offline selftest | `just _laya-region-selftest` + pre-phase render cmp | ❌ W0 (created by the task) | ⬜ pending |
| 13-01-02 | 01 | 1 | D-20, D-05 | T-13-01, T-13-03 | zero/two-line templates, malformed and injecting regions refused without echo; bucket argv per region | offline selftest + gates rows | `just _laya-region-selftest`; `LAYA_GATES_ONLY=region-selftest,weights-bucket-region,prepare-motivation-args just laya-gates-selftest`; deploy selftest | ❌ W0 | ⬜ pending |
| 13-01-03 | 01 | 1 | D-20 | T-13-01 | M1..M8 turn the guard RED in its new scope; the pre-phase render's nested-just composition recorded (helpers identical, or a worktree render) | mutation | in-place mutation + `git checkout -- justfile`; evidence check | ❌ W0 | ⬜ pending |
| 13-02-01 | 02 | 1 | D-13, D-14, D-15, D-16, D-01, D-03 | T-13-04 | closed formats, enums and caps declared | contract | `"$PV" validate contracts/motivation-enrichment-v1.yaml` + pyyaml declaration check | ❌ W0 | ⬜ pending |
| 13-02-02 | 02 | 1 | D-13 | T-13-04 | corpus derived files current, ratchet unmoved | contract hygiene | `"$PV" lint`, census cmp, `pv extract contracts --check` (clean worktree), `check_ont_ratchet.sh`, ont7 + readme_contract tests | ✅ (gates exist) | ⬜ pending |
| 13-11-01 | 11 | 1 | D-06, D-02 | T-13-37 | the ze-kasher-dev local record archived with a manifest BEFORE any phase-13 command writes it; every original older than the phase base commit; the manifest's sha256 pinned | local baseline | `shasum -a 256 -c MANIFEST.sha256` + evidence check (mtime anchor, pin, copies cmp-equal) | ❌ W0 | ⬜ pending |
| 13-03-01 | 03 | 2 | D-09, D-10, D-11 | T-13-06, T-13-09 | no-flag bytes equal v1 by a check; eval frozen under --enrichment; in-repo enrichment refused | unit + real-CSV run | `prepare_motivation.py --selftest`; no-flag `unchanged (byte-identical)`; fixture merge cmp | ✅ (selftest exists, extended) | ⬜ pending |
| 13-03-02 | 03 | 2 | D-11, D-12, D-15, D-16, D-09 | T-13-07, T-13-08 | every rule and all 17 refusals text-free; contract mirror | unit | `prepare_motivation.py --selftest` (>= 60 cases) | ✅ (extended) | ⬜ pending |
| 13-03-03 | 03 | 2 | D-10 | T-13-06 | N1..N6 RED (coupled with gate row prepare-motivation-args, which no wave-2 sibling sweeps) | mutation | in-place mutation + evidence check | ❌ W0 | ⬜ pending |
| 13-04-01 | 04 | 2 | D-06 | T-13-10, T-13-11 | explicit target; per-profile state; isolation | offline selftest | `just _laya-target-selftest` (ISOLATION 1-3), live `_laya-target-check augury-dev eu-west-1` (read-only) | ❌ W0 | ⬜ pending |
| 13-04-02 | 04 | 2 | D-06, D-19 | T-13-10, T-13-11, T-13-13 | every AWS-touching recipe wired (drift guard); foreign endpoint refused; ambient target vars cannot steer offline cases; the motivation-id selftest leaves the ze-kasher-dev record unchanged | offline selftest + gates rows | `just _laya-target-selftest` under stale ambient values; filtered gates sweep (excludes prepare-motivation-args, coupled with 13-03); deploy selftest for the motivation id; 13-11 manifest + pin after it | ❌ W0 | ⬜ pending |
| 13-04-03 | 04 | 2 | D-06 | T-13-10, T-13-11 | K1..K5 RED; default profile unchanged (nested-just composition recorded) | mutation | in-place mutation + evidence check; 13-11 manifest + pin | ❌ W0 | ⬜ pending |
| 13-05-01 | 05 | 2 | D-17, D-14, D-03 | T-13-14, T-13-15 | seeded rule, disjoint slices, text-free output, no label column | unit + synthetic run | `select_motivation.py --selftest`; 1500-record synthetic run | ❌ W0 | ⬜ pending |
| 13-05-02 | 05 | 2 | D-17, D-16 | T-13-15 | note numerically equal to the code | doc check | python import of the script vs the note | ❌ W0 | ⬜ pending |
| 13-06-01 | 06 | 3 | D-07 | T-13-16, T-13-17 | token from a mode-600 file reaches only the probe's environment and the wire | unit + loopback chain | `m2m_token.py --selftest`; `just _laya-auth-selftest` (CHAIN; must fail under stub mismatch) | ❌ W0 | ⬜ pending |
| 13-06-02 | 06 | 3 | D-07, D-06 | T-13-18, T-13-19, T-13-20 | 401 on tools/list then tools/call; re-exec with token; no argv bearer; the auth selftest restores every default-state-dir file it writes | offline selftest + gates rows | `just _laya-auth-selftest`; filtered gates sweep; deploy selftest for the motivation id; 13-11 manifest + pin after it | ❌ W0 | ⬜ pending |
| 13-06-03 | 06 | 3 | D-07 | T-13-16 | A1..A5 RED | mutation | in-place mutation + evidence check; 13-11 manifest + pin | ❌ W0 | ⬜ pending |
| 13-07-01 | 07 | 4 | D-18, D-19, D-06, D-02, D-05 | T-13-21, T-13-22, T-13-24 | read-only probe gate; the wave-1 archive manifest (13-11) re-verified before and after the render and selftest; augury bucket only | live (read-only + one PutObject) | evidence check + `shasum -a 256 -c MANIFEST.sha256` + the 13-11 pin | n/a (live) | ⬜ pending |
| 13-07-02 | 07 | 4 | D-18 | T-13-23 | owner login; expires_at only | checkpoint | token gate line >= 45 min | n/a | ⬜ pending |
| 13-08-01 | 08 | 5 | D-06, D-18, D-02 | T-13-26, T-13-27 | publish on augury-dev, endpoint check, identity, negative control, readback | live | evidence check + `_laya-endpoint-check` + manifest and its 13-11 pin | n/a (live) | ⬜ pending |
| 13-08-02 | 08 | 5 | D-07 | T-13-25, T-13-29 | open interval measured; unauthenticated tool calls 401 | live + checkpoint | `_laya-auth-refusal-check <endpoint> 600` + evidence check | n/a | ⬜ pending |
| 13-08-03 | 08 | 5 | D-07, D-08 | T-13-28 | authenticated identity; no credential or path committed | live + checkpoint | `laya-probe-auth` log + evidence regex | n/a | ⬜ pending |
| 13-09-01 | 09 | 6 | D-07 | T-13-30, T-13-31 | authenticated timings, reported not gated | live | evidence normalization check | n/a | ⬜ pending |
| 13-09-02 | 09 | 6 | D-07 | — | owner decision after a failed verify (skip when verified) | checkpoint | reply record check | n/a | ⬜ pending |
| 13-09-03 | 09 | 6 | D-07, D-06, D-02, D-03, D-08 | T-13-32, T-13-33 | authenticated replay; ze-kasher-dev unchanged since before the phase; partner repo unchanged | live | replay record check + manifest and its 13-11 pin + partner status cmp | n/a | ⬜ pending |
| 13-10-01 | 10 | 7 | D-08, D-06, D-09, D-16, D-17, D-01, D-03, D-04 | T-13-34, T-13-35 | record-true hand-off without credentials | doc check | hand-off check against the records | n/a | ⬜ pending |
| 13-10-02 | 10 | 7 | D-04, D-06, D-02 | T-13-34, T-13-36 | final tree green; out-of-scope surfaces unchanged; sweep clean; all checked by the verify itself | suite + diff | the verify re-runs eleven suites (monorepo_invariants, dup-bin guard, census, clean-worktree graph check, both prepare_motivation runs, select / m2m / region / target / auth selftests), reads the deploy-selftest and full gates-sweep logs (fresh, verdict last), re-checks the 13-11 manifest and pin, repeats the sweep; `git diff --quiet 61586f888 HEAD -- ...` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Each Wave 0 item is created by the task that first needs it (tracer-first), not by a separate scaffold plan:
- [ ] `_laya-region-selftest` (+ LAYA_DEPLOY_TEMPLATE under DRY_RUN=1, region ERE, BUCKET fake-aws table) and its gates rows — plan 13-01
- [ ] `contracts/motivation-enrichment-v1.yaml` — plan 13-02
- [ ] the baseline of the ze-kasher-dev local record (`models/decide/ze-kasher-dev-archive-20261027/MANIFEST.sha256`, the pre-phase mtime anchor, the pin in `13-BASELINE-EVIDENCE.json`) — plan 13-11, wave 1, before any phase-13 command writes those paths; re-verified by 13-04, 13-06, 13-07, 13-08, 13-09 and 13-10
- [ ] `prepare_motivation.py` enrichment cases, `check_v1_identity`, the manifest, the contract mirror — plan 13-03
- [ ] `_laya-target-check`, `_laya-state-dir`, `_laya-endpoint-check`, `_laya-target-selftest` and their rows — plan 13-04
- [ ] `select_motivation.py --selftest` — plan 13-05
- [ ] `scripts/laya_deploy/m2m_token.py --selftest` / `--stub-server`, `laya-probe-auth`, `_laya-auth-refusal-check`, `_laya-auth-selftest` and their rows — plan 13-06
- No package install (uv --frozen, stdlib); no `ci/explicit-test-commands.d/` fragment is added: the laya selftests stay local-only like every Phase 8-12 laya selftest (CI never installs the laya_train stack), which this phase does not claim to change.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| pmcp.run login for the augury-dev tenant | D-18 | the session belongs to the owner; the executor never logs in | owner runs `PMCP_TARGET=augury-dev cargo pmcp deploy login`; the token gate reads expires_at only (13-07 Task 2) |
| OAuth enablement on the new server | D-07 | cargo-pmcp cannot render an auth-enabled descriptor for pmcp-run; admin UI only | owner enables OAuth right after the publish; the executor then runs `_laya-auth-refusal-check` (13-08 Task 2) |
| M2M client creation and its mode-600 file | D-07, D-08 | credentials belong to the owner and go to the partner out of band | owner creates the client, writes the JSON outside the checkout, replies with the path; the executor proves an authenticated identity call (13-08 Task 3) |
| Owner-run upload / deploy when the permission classifier refuses the executor | D-18 | Phase 10 precedent ("[Production Deploy]") | owner runs the printed command; the executor confirms read-only (head-object, identity, readback) |
| Decision after a failed deploy-verify | D-07 | a live endpoint's fate is the owner's call | contain / hold-serving / reverify-once (13-09 Task 2), skipped when verified |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s (quick selftests)
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** {pending / approved YYYY-MM-DD}
