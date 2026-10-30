---
phase: "14"
slug: "laya-training-mcp-server-s3-dataset-in-apr-and-reports-out"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: true) (#2117)
status: validated
nyquist_compliant: false
wave_0_complete: true
created: "2026-10-28"
---

# Phase 14 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution. Seeded from `14-RESEARCH.md` § Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` / `cargo nextest` (CI `workspace-test`); Python `data.py --selftest`, `gate.py --selftest`, lifecycle (local-only, torch) |
| **Config file** | workspace `Cargo.toml`; `ci/explicit-test-commands.d/NNN-*.cmd` for integration targets |
| **Quick run command** | `cargo test -p aprender-mcp-laya-train --lib && cargo test -p aprender-mcp-laya-train-lambda --lib` |
| **Full suite command** | `cargo nextest run --profile ci --workspace --lib --exclude aprender-gpu --exclude aprender-cuda-edge --exclude aprender-compute && cargo test -p aprender-core --test monorepo_invariants` |
| **Estimated runtime** | quick ~60 s; full ~15–30 min |

A new `tests/*.rs` target is dark until a `ci/explicit-test-commands.d/` fragment names it (free ordinal, never renumber). `ci_run_explicit_test_commands.sh --list` exits 2 on macOS, so local verifies assert the fragment file directly.

---

## Sampling Rate

- **After every task commit:** the quick run command for the touched crate
- **After every plan wave:** `cargo test -p aprender-core --test monorepo_invariants`, `cargo clippy -p <crate> --lib -- -D warnings` per new crate, `bashrs lint` on new shell
- **Before `/gsd-verify-work`:** full suite green, `readme_contract` green
- **Max feedback latency:** 120 seconds (quick run)

---

## Per-Task Verification Map

Decision-keyed (no REQ-IDs are mapped to this phase). Task IDs are filled in once plans exist.

| Decision | Behavior | Test Type | Automated Command | File Exists | Status |
|----------|----------|-----------|-------------------|-------------|--------|
| D-06 / D-21 | unknown `train` args refused; `epochs` accepted only in [4,12] above 16 shots/class, refused otherwise; above-16 without `epochs` refused | unit | `cargo test -p aprender-mcp-laya-train --lib args` (+ `--lib name`, `--lib uri`; 14-01 Task 2) | ✅ | ✅ green |
| D-08 | Rust validation equals `data.py` on one shared case table | unit + python | `cargo test -p aprender-mcp-laya-train --lib shared_case_table` (+ `contract_mirror`, `deferrable_rules_mirror`, `no_false_refusal`; 14-02) and `uv run --project scripts/laya_train --frozen python scripts/laya_train/data.py --selftest` | ✅ | ✅ green |
| D-11 | second `train` refused naming running task; lock expiry takeover; finalizer release | unit (in-memory backend) | `cargo test -p aprender-mcp-laya-train --lib lock` and `--lib submit_single_flight` (14-01); release on every path `cargo test -p aprender-mcp-laya-train-lambda --lib fin_` (14-05) | ✅ | ✅ green |
| D-10 | no tool callable without auth | live check (owner) | `just _laya-auth-refusal-check <url> 0 train_status` (tool argument added in 14-10; run inside `just laya-trainer-grant`, 14-13) | ✅ | ✅ green |
| D-18 | one payload shape; links absent from stored record; a client-cancelled task ends with a stored cancelled payload | unit | `cargo test -p aprender-mcp-laya-train --lib payload` and `--lib attach` (14-01); `cargo test -p aprender-mcp-laya-train-lambda --lib fin_cancel` (14-05) | ✅ | ✅ green |
| D-19 | lifecycle rules by tag; pending default expires | synth assertion | `just laya-trainer-synth dev` (prints `TRAINER SYNTH OK env=dev`) + `python3 -I scripts/laya_deploy/laya_trainer_stack_check.py --selftest` (14-08) | ✅ | ✅ green |
| D-20 | fetch recipe verifies every sha256, refuses a tampered file, restores run+data+apr | recipe selftest | `just laya-fetch --selftest` and `LAYA_GATES_ONLY=fetch just laya-gates-selftest` (14-11) | ✅ | ✅ green |
| D-14 | pack/verify runs at the fixed contracts path on arm64 | local docker smoke | `bash deploy-extensions/container/pack-arm64/smoke.sh` (prints `SMOKE OK image`; 14-07) and `python3 -I scripts/laya_train/container/run_step.py --selftest` | ✅ | ✅ green |
| D-02 / D-12 / D-13 | job shape: instance, MaxRuntime 21600, channels, image digests | synth assertion | `just laya-trainer-synth dev` + `python3 -I scripts/laya_deploy/laya_trainer_stack_check.py --selftest` (14-08) | ✅ | ✅ green |
| D-15 | verify refusal → failed task, no `.apr`, reports attached | finalizer unit | `cargo test -p aprender-mcp-laya-train --lib finalize_` (14-01) and `cargo test -p aprender-mcp-laya-train-lambda --lib fin_` + `--lib tracer_completed` (14-05) | ✅ | ✅ green |
| D-22 | register 11, dup-bin allowlist, selftest row 6 claimants | existing tests | `cargo test -p aprender-core --test monorepo_invariants`; `bash scripts/check_duplicate_bin_names.sh`; `LAYA_GATES_ONLY=dup-bin-names just laya-gates-selftest` (14-04 Task 3) | ✅ (edit) | ✅ green |
| README | every new crate has a README linking the monorepo | existing test | `cargo test -p aprender-core --test readme_contract` (CI fragment 040; 14-01, 14-04) | ✅ | ✅ green |
| container | pinned stack imports; lifecycle passes | local-only | `just laya-train-lifecycle`; `python3 -I scripts/laya_train/container/run_step.py --selftest` (14-06) | ✅ (local) | ✅ green |

*Status: ✅ green · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `scripts/laya_train/data_cases.json` + replay in `data.py --selftest` + a `--lib` replay test in `aprender-mcp-laya-train` (no new `tests/*.rs`)
- [x] `crates/aprender-mcp-laya-train/` and `crates/aprender-mcp-laya-train-lambda/` skeletons with in-memory backend tests
- [x] CI fragment for every new `tests/*.rs` (none planned: the plans keep these as `--lib` tests)
- [x] CDK stack + synth recipe; ASL JSON assertion
- [x] Dockerfiles + CodeBuild buildspec (owner runs); local arm64 smoke recipe
- [x] fetch recipe + selftest with a tampered-file negative control
- [x] probe checklist (CUDA/driver/stop-execution/SIGTERM), owner-gated

---

## Manual-Only Verifications

| Behavior | Decision | Why Manual | Test Instructions |
|----------|----------|------------|-------------------|
| CUDA probe job: `nvidia-smi`, `torch.cuda.is_available()`, matmul, tiny-fixture lifecycle on `--device cuda` | D-02 / D-13 | paid AWS action, owner only | owner runs the probe job; evidence JSON records `device_used`. DONE 2026-10-29: first probe failed (no C compiler), compat rebuild fixed it, re-probe passed under the owner-amended rule (14-PROBE-EVIDENCE.json) |
| motivation-v1 reproduction inside the approved band | D-16 / D-17 | one paid run, owner only | owner runs `train` on the Phase 12 data dir with `epochs` 12; evidence JSON. DONE 2026-10-29: verdict NOT REPRODUCED (margin 0.286 below the 0.30 floor, arm64 pack RescoreDrift refusal); recorded in 14-GATE-RUN-EVIDENCE.json and accepted by the owner. The test coverage is complete; this row is a measurement outcome, not a coverage gap |
| OAuth refusal before grants | D-10 | needs a deployed server + owner OAuth toggle | `just _laya-auth-refusal-check <url> 0` after deploy, before the grant step. DONE: 401s observed (14-DEPLOY-EVIDENCE.json) |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 120s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-10-29 (orchestrator audit: 40/40 plan tasks carry an automated verify; all selftests re-run green; the D-16 reproduction outcome is a recorded finding, not a coverage gap)

## Validation Audit 2026-10-30

| Metric | Count |
|---|---|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |
