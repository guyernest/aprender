---
phase: "12"
slug: "motivation-v1-decide-server-for-experiment-coach-laya-pilot"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-10-26"
---

# Phase 12 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution. Source: `12-RESEARCH.md` § Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` / nextest; Python `--selftest` scripts under uv; `just` selftest recipes; `pv validate` |
| **Config file** | `Cargo.toml` workspace; `scripts/laya_train/pyproject.toml`; `ci/explicit-test-commands.d/` |
| **Quick run command** | `cargo test -p aprender-decide --lib` / `uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --selftest` |
| **Full suite command** | `just laya-train-selftest && just laya-deploy-selftest && cargo test -p aprender-decide -p aprender-mcp-decide -p aprender-mcp-decide-lambda --lib && cargo test -p aprender-core --test monorepo_invariants --test readme_contract && bash scripts/check_duplicate_bin_names.sh` |
| **Estimated runtime** | ~600 seconds (full suite, excluding training runs) |

---

## Sampling Rate

- **After every task commit:** the quick command for the touched module
- **After every plan wave:** the full suite command
- **Before `/gsd-verify-work`:** full suite green, plus one real `just laya-verify` of BOTH `laya-antisemitism-v1.apr` (byte-stability under the waiver change) and the motivation artifact
- **Max feedback latency:** ~120 seconds for quick commands

---

## Per-Task Verification Map

Filled by the planner per task; the decision-level map is:

| Decision | Behavior | Test Type | Automated Command | File Exists |
|----------|----------|-----------|-------------------|-------------|
| D-16/D-13 | split rule, pins, no text printed, eval counts {23,14,63} | unit (synthetic CSV + sentinel) | `prepare_motivation.py --selftest` | ❌ W0 |
| D-09 | measured P and admitted door for both variants | tokenizer selftest + `--measure` | `prepare_motivation.py --selftest-tokenizer` | ❌ W0 |
| D-10 | approved bytes == data-dir task.json == artifact `inputs_sha256.task_json` | evidence check | `shasum -a 256` vs `12-TASK-EVIDENCE.json` | — |
| D-17/D-18 | spike reads no `eval.jsonl`; binding rule applied | spike guard + evidence | spike harness refuses an eval path; `12-SPIKE-EVIDENCE.json` | ❌ W0 |
| D-19/D-20 | declared waiver honoured; undeclared refused; margin never waived; existing artifacts byte-stable | unit | `cargo test -p aprender-decide --lib verify::tests::` + fail-closed vectors | ❌ W0 |
| D-19 | contract schema | contract | `pv validate contracts/laya-finetune-gate-v1.yaml contracts/decide-apr-v1.yaml` | ✅ |
| D-05/D-14/D-22 | P/R, false-assignment at thresholds, length tertiles, ECE CI | unit | `metrics.py --selftest`; `bakeoff.py --selftest` | ❌ W0 |
| D-08 | SetFit adapter K=3; Phase 10 dir re-adapts byte-identically | unit + rehearsal | `cargo test -p aprender-contrastive-data --examples`; s8 rehearsal under `/usr/bin/time -l` | ❌ W0 |
| D-07/D-11 | Kev branch SHA recorded; 0 eval texts in prompt/fit rows | evidence assertion | `python -I` check over `12-KEV-ARM-EVIDENCE.json` | ❌ W0 |
| D-02 | register 10, allowlist exact, thin crate is a shim | integration | `cargo test -p aprender-core --test monorepo_invariants`; `bash scripts/check_duplicate_bin_names.sh` | ✅ (edit) |
| F-10-12-1 | 404→200 retried; 404-forever and 500 contained | offline selftest | `just laya-deploy-selftest` | ✅ (extend) |
| Carried | live identity, label order, sum to 1, sha == pin | live (human checkpoint) | `just laya-deploy-verify ... aprender-mcp-decide-motivation ...` | ✅ |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

### Per-task map (planner, 2026-10-26; revised 2026-10-26 from the cross-AI review, REVIEWS 0853a8450)

Every task's `<verify>` carries an `<automated>` command with a `<fails_when>`; checkpoint tasks verify the recorded reply file.
Rows marked (rev) changed in the review revision.

| Plan / Task | Decisions | Verify (key command or check) | Status |
|-------------|-----------|-------------------------------|--------|
| 12-01 T1 (tracer) (rev) | D-16, D-13, D-09 | `prepare_motivation.py --selftest` (seven malformed inputs incl. a sentinel in Class, invalid UTF-8 and a csv.Error parser case; no refusal echoes a value) / `--selftest-tokenizer`; drafts a/b `--measure` with `post_tokens` (no `tweet_tokens`); scratch dir through data.py validators; all-395-post leak scan; `LAYA_GATES_ONLY=prefix-budget just laya-gates-selftest` (drift check only) | ⬜ |
| 12-01 T2 (decision) | D-09, D-10 | 12-TASK-OWNER-REPLY.json with the sha prefix of the bytes shown | ⬜ |
| 12-01 T3 (rev) | D-10, D-13, D-01, D-23 | re-run byte-identical; enforced `--measure`; 12-TASK-EVIDENCE.json consistency; D-23 hunk-level check: `git diff HEAD -- justfile` == foreign patch AND Counter(start..HEAD range) == Counter(own patch) | ⬜ |
| 12-02 T1 (tracer) (rev) | D-19 | `cargo test -p aprender-decide --lib waiver` (accept + undeclared), clippy, wiring checks; variant_name / Display arms | ⬜ |
| 12-02 T2 (rev) | D-19, D-20 | full lib suite incl. 13 waiver tests (duplicate ids, malformed hashes, exit codes) + case table + bindings table (union of both bases) + every-field sweep (both bases); fullest_copy / permissive_policy byte-identical to the start commit; armed fail_closed_vectors; laya-train-selftest; pv validate; census `cmp`; `pv extract contracts --check` | ⬜ |
| 12-03 T1 (tracer) (rev) | D-19 | `cargo test -p aprender-decide --lib artifact::` (waived pack at every door, strip/add/change refused, no waiver key when unwaived, determinism); both generic binding tests reach packed_waived() | ⬜ |
| 12-03 T2 (rev) | D-19, D-20 | gate.py selftest (failed_clauses, matching_pilot_waiver); laya-train-selftest incl. `WAIVED REPORT ASSEMBLY OK`; recipe_id_for reproduces the deployed run; train.py + gate.py training-path AST unchanged; lifecycle recomputes has_waiver | ⬜ |
| 12-03 T3 | D-19 | one real `just laya-verify` of laya-antisemitism-v1.apr: eligible, waived null; pv + graph + census | ⬜ |
| 12-04 T1 (tracer) | D-02 | monorepo_invariants (register N to N+1), duplicate-bin check, shim test, readme_contract, resolver proof, `BOOTSTRAP aarch64 OK` | ⬜ |
| 12-04 T2 (rev) | F-10-12-1, D-23 | `laya-deploy-selftest` for both ids (8 EDGE HEALTH get cases incl. 503, config, elapsed-cap; stale body file removed; AWS CALLS: 0); edge-health-get gates row GREEN; D-23 hunk-level range check | ⬜ |
| 12-05 T1 (tracer) (rev) | D-05, D-13, D-14 | `metrics.py --selftest`; `bakeoff.py --selftest`; direct checks: ECE interval 0 <= lo <= hi <= 1 + determinism (no point containment), check_distribution, client_ge (`>=`) | ⬜ |
| 12-05 T2 (rev) | D-05, D-06 | `bakeoff.py --selftest` at least 17 cases (SetFit row identity incl. two swapped same-label rows, distribution refusals, per-band not_reported, render determinism, sentinel) | ⬜ |
| 12-06 T1 (tracer) (rev) | D-25 | adapter example tests; Phase 10 dir re-adapted byte-identically; three-label select + `setfit train --dry-run`; test.jsonl = eval rows in order, byte for byte, ids `test:<i>` | ⬜ |
| 12-06 T2 (rev) | D-25 | `cargo test -p apr-cli --features setfit --lib eval` (6 dump tests, nine-key dump with row_ids / text_sha256 / dataset_fingerprint), clippy, fmt; release build with --features setfit, apr_bin.sh, `eval --help` lists --dump-probabilities | ⬜ |
| 12-07 T1 (tracer) (rev) | D-17, D-21 | probe evidence (device from trace, E by the rule) OR the probe_failed record (no spike evidence); `run_spike.py --guard-selftest`; one identity-bound spike cell after the rules commit | ⬜ |
| 12-07 T2 (rev) | D-17, D-18 | 12-SPIKE-EVIDENCE.json state completed (re-derived from nine identity-valid runs) or grid_incomplete (missing cells, null aggregates); skipped on probe_failed | ⬜ |
| 12-07 T3 (decision) (rev) | D-18 | v2 skip or owner reply for the derived state (probe_failed / grid_incomplete / completed); never opens the spike file on probe_failed; reruns once each | ⬜ |
| 12-08 T1 (tracer) / T2 (rev) | D-17 | branch record with policy_revision; on implemented: 4 override tests (incl. the integration fixture refused under the override minimum and malformed entries), trainer slice counts on the real train rows, laya-verify of antisemitism-v1, pv/graph/census `cmp` | ⬜ |
| 12-09 T1 (tracer) / T2 (rev) | D-09, D-26 | decide lib + e2e_stdio + lambda lib tests; served_tasks consistency; (b) 7 tests incl. served_for(0), ungated measure, door + 1 refusal; derived limits in build_server / ClassifyService::served / parse_args / probe; pv/graph/census; laya-claims-check; decide-lambda README door sentence (b) or unchanged (a) | ⬜ |
| 12-10 T1 (tracer) / T2 (rev) | D-08, D-25 | step 0 build + apr_bin + `eval --help`; 4 GiB RSS control; s8 rehearsal with a 36 GiB watchdog against the 32 GiB rule (quality excluded); D-23 range check; s32 cell same-pass control, row identity and dataset fingerprint, or blocked record | ⬜ |
| 12-11 T1 (tracer) (rev) | D-17, D-19 | declaration read identically by Python and Rust; declaration commit precedes the run; policy_revision; skeleton v3 with both attempt triples; refused-epochs path; census `cmp` | ⬜ |
| 12-11 T2 (rev) | D-13, D-19, D-20, D-21 | per-attempt write-once records indexed by sha256 (cap two; attempt 2 only under an authorization); outcome + failure_kind; rc 3 + PILOT WAIVER + PACKED for the waived path; verify line with `waived`; slice counts at the declared fraction; plan-only door check; index mirrors the final attempt | ⬜ |
| 12-11 T3 (decision) (rev) | D-20 | final disposition v2 (skip iff the final attempt is eligible) separate from the write-once rerun authorization; options by failure kind | ⬜ |
| 12-12 T1 (tracer) / T2 (human-action) / T3 (rev) | D-23 | A and R from the gate record; D-23 equivalence (every recipe but the owner's two, listed via `--dump`, `just --show` exit 0 + cmp; config cmp); upload executor- or owner-run with read-only confirmation; token gate + recheck before every live attempt; live identity, negative control, readback us-east-1, antisemite unchanged | ⬜ |
| 12-13 T1 (tracer) / T2 (decision) / T3 (rev) | D-12, D-09 | live{} with every sample normalized (explicit nulls, error, graviton), timings from measured values only, evidence-file freshness, configuration_changes; acceptance replay (7 requester tests, the 4-text line passing every check, count refusal at B + 1) | ⬜ |
| 12-14 T1 (tracer) / T2 / T3 (decision) (rev) | D-07, D-11, D-24 | disk around `cargo clean`; lock regenerated offline before `--locked`; direct state-dict export; 4B parity within 1e-4 on short rows and one ~6.8k-token row; projected peak RSS; prompt, D-11 removal branch and leakage; identity-bound timed row; owner budget | ⬜ |
| 12-15 T1 (tracer) / T2 (rev) | D-07, D-11, D-24 | identity header on scores.jsonl (resume refused on mismatch, torn line dropped); chosen rows scored once; train-only fit from raw-logit log-probabilities with the fit_temperature tuple unpacked and a copied fit_bias; eval-probs aligned to eval.jsonl | ⬜ |
| 12-16 T1 (tracer) / T2 / T3 (rev) | D-03, D-05, D-14, D-15, D-22 | bake-off evidence (Laya bit-identical to the gate; bounded ECE intervals, no containment; second run `cmp`); hand-off tables re-rendered and compared, numbers table pointer-resolved, threshold disclosure; readme_contract + laya-claims-check | ⬜ |

-------------|-----------|-------------------------------|--------|
| 12-01 T1 (tracer) | D-16, D-13, D-09 | `prepare_motivation.py --selftest` / `--selftest-tokenizer`; drafts a/b `--measure`; scratch dir through data.py validators; all-395-post leak scan; `LAYA_GATES_ONLY=prefix-budget just laya-gates-selftest` | ⬜ |
| 12-01 T2 (decision) | D-09, D-10 | 12-TASK-OWNER-REPLY.json with the sha prefix of the bytes shown | ⬜ |
| 12-01 T3 | D-10, D-13, D-01, D-23 | re-run byte-identical; enforced `--measure`; 12-TASK-EVIDENCE.json consistency; owner's justfile diff preserved | ⬜ |
| 12-02 T1 (tracer) | D-19 | `cargo test -p aprender-decide --lib waiver` (accept + undeclared), clippy, wiring checks | ⬜ |
| 12-02 T2 | D-19, D-20 | full lib suite incl. 10 waiver tests + case table + bindings sweep; armed fail_closed_vectors; laya-train-selftest; pv validate; census; `pv extract contracts --check` | ⬜ |
| 12-03 T1 (tracer) | D-19 | `cargo test -p aprender-decide --lib artifact::` (waived pack at every door, strip/add/change refused, no waiver key when unwaived, determinism) | ⬜ |
| 12-03 T2 | D-19, D-20 | gate.py selftest; laya-train-selftest; recipe_id_for reproduces the deployed run; training path AST unchanged | ⬜ |
| 12-03 T3 | D-19 | one real `just laya-verify` of laya-antisemitism-v1.apr: eligible, waived null; pv + graph + census | ⬜ |
| 12-04 T1 (tracer) | D-02 | monorepo_invariants (register N to N+1), duplicate-bin check, shim test, readme_contract, resolver proof, `BOOTSTRAP aarch64 OK` | ⬜ |
| 12-04 T2 | F-10-12-1, D-23 | `laya-deploy-selftest` for both ids (5 EDGE HEALTH get cases, AWS CALLS: 0); edge-health-get gates row GREEN | ⬜ |
| 12-05 T1 (tracer) | D-05, D-13, D-14 | `metrics.py --selftest`; `bakeoff.py --selftest`; direct metric checks | ⬜ |
| 12-05 T2 | D-05, D-06 | `bakeoff.py --selftest` at least 12 cases (refusals, rules, sentinel) | ⬜ |
| 12-06 T1 (tracer) | D-25 | adapter example tests; Phase 10 dir re-adapted byte-identically; three-label select + `setfit train --dry-run` | ⬜ |
| 12-06 T2 | D-25 | `cargo test -p apr-cli --features setfit --lib eval` (5 dump tests), clippy, fmt | ⬜ |
| 12-07 T1 (tracer) | D-17, D-21 | probe evidence (device from trace, E by the rule); `run_spike.py --guard-selftest`; one spike cell after the rules commit | ⬜ |
| 12-07 T2 | D-17, D-18 | 12-SPIKE-EVIDENCE.json re-derived from the nine runs by the declared rules | ⬜ |
| 12-07 T3 (decision) | D-18 | skip record or owner reply bound to the evidence hashes | ⬜ |
| 12-08 T1 (tracer) / T2 | D-17 | branch record; on implemented: override tests, laya-verify of antisemitism-v1, pv/graph | ⬜ |
| 12-09 T1 (tracer) / T2 | D-09, D-26 | decide lib + e2e_stdio + lambda lib tests; served_tasks consistency; (b) served_for tests; pv/graph/census; laya-claims-check | ⬜ |
| 12-10 T1 (tracer) / T2 | D-08, D-25 | s8 rehearsal under `/usr/bin/time -l` against the 32 GiB rule; s32 cell same-pass control or blocked record | ⬜ |
| 12-11 T1 (tracer) | D-17, D-19 | declaration read identically by Python and Rust; declaration commit precedes the run; refused-epochs path | ⬜ |
| 12-11 T2 | D-13, D-19, D-20, D-21 | outcome-specific gate record; verify line with `waived`; plan-only door check | ⬜ |
| 12-11 T3 (decision) | D-20 | skip or owner reply | ⬜ |
| 12-12 T1 (tracer) / T2 (human-action) / T3 | D-23 | upload record; token gate at least 45 min; live identity, negative control, readback us-east-1, antisemite unchanged | ⬜ |
| 12-13 T1 (tracer) / T2 (decision) / T3 | D-12, D-09 | live{} normalized timings; acceptance replay (7 requester tests, count refusal at B + 1) | ⬜ |
| 12-14 T1 (tracer) / T2 / T3 (decision) | D-07, D-11, D-24 | disk around `cargo clean`; 4B parity within 1e-4; prompt and leakage; timed row and projections; owner budget | ⬜ |
| 12-15 T1 (tracer) / T2 | D-07, D-11, D-24 | chosen rows scored once; train-only fit; eval-probs aligned to eval.jsonl | ⬜ |
| 12-16 T1 (tracer) / T2 / T3 | D-03, D-05, D-14, D-15, D-22 | bake-off evidence (Laya bit-identical to the gate); hand-off content checks; readme_contract + laya-claims-check | ⬜ |

---

## Wave 0 Requirements

- [ ] `scripts/laya_train/prepare_motivation.py` with `--selftest` / `--selftest-tokenizer` (plan 12-01 Task 1)
- [ ] waiver cases in `crates/aprender-decide/src/verify/tests.rs` (plan 12-02) plus manifest strip/forge cases (plan 12-03 Task 1)
- [ ] metrics functions + `bakeoff.py --selftest` (plan 12-05)
- [ ] SetFit adapter K≥2 tests; `--dump-probabilities` tests (plan 12-06)
- [ ] spike harness guard refusing `eval.jsonl` (plan 12-07 Task 1)
- [ ] `laya-deploy-selftest` EDGE HEALTH 404-retry rows (plan 12-04 Task 2)

---

## Manual-Only Verifications

| Behavior | Decision | Why Manual | Test Instructions |
|----------|----------|------------|-------------------|
| task.json wording approval | D-10 | owner judgement | owner reviews exact bytes at blocking checkpoint |
| live deploy | carried (Phase 8 D-18) | needs owner's pmcp.run login | owner logs in, runs/approves `laya-deploy`, then `laya-deploy-verify` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
