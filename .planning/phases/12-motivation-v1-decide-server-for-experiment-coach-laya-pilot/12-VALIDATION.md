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

---

## Wave 0 Requirements

- [ ] `scripts/laya_train/prepare_motivation.py` with `--selftest` / `--selftest-tokenizer`
- [ ] waiver cases in `crates/aprender-decide/src/verify/tests.rs` (plus manifest strip/forge cases)
- [ ] metrics functions + `bakeoff.py --selftest`
- [ ] SetFit adapter K≥2 tests; `--dump-probabilities` tests
- [ ] spike harness guard refusing `eval.jsonl`
- [ ] `laya-deploy-selftest` EDGE HEALTH 404-retry rows

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
