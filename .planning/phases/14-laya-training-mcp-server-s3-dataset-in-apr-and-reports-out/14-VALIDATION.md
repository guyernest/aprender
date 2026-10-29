---
phase: "14"
slug: "laya-training-mcp-server-s3-dataset-in-apr-and-reports-out"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
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
| D-06 / D-21 | unknown `train` args refused; `epochs` accepted only in [4,12] above 16 shots/class, refused otherwise; above-16 without `epochs` refused | unit | `cargo test -p aprender-mcp-laya-train --lib args` | ❌ W0 | ⬜ pending |
| D-08 | Rust validation equals `data.py` on one shared case table | unit + python | `cargo test -p aprender-mcp-laya-train --test validation_cases` and `uv run --project scripts/laya_train --frozen python scripts/laya_train/data.py --selftest` | ❌ W0 | ⬜ pending |
| D-11 | second `train` refused naming running task; lock expiry takeover; finalizer release | unit (in-memory backend) | `cargo test -p aprender-mcp-laya-train-lambda --lib lock` | ❌ W0 | ⬜ pending |
| D-10 | no tool callable without auth | live check (owner) | `just _laya-auth-refusal-check <url> 0` | ✅ | ⬜ pending |
| D-18 | one payload shape; links absent from stored record | unit | `cargo test -p aprender-mcp-laya-train --lib payload` | ❌ W0 | ⬜ pending |
| D-19 | lifecycle rules by tag; pending default expires | synth assertion | `just synth-laya-train` + template assertion | ❌ W0 | ⬜ pending |
| D-20 | fetch recipe verifies every sha256, refuses a tampered file, restores run+data+apr | recipe selftest | `just laya-fetch-selftest` | ❌ W0 | ⬜ pending |
| D-14 | pack/verify runs at the fixed contracts path on arm64 | local docker smoke | `docker run --platform linux/arm64 <img> pack_laya inspect <fixture.apr>` | ❌ W0 | ⬜ pending |
| D-02 / D-12 / D-13 | job shape: instance, MaxRuntime 21600, channels | synth assertion | `just synth-laya-train` + state-machine JSON assertion | ❌ W0 | ⬜ pending |
| D-15 | verify refusal → failed task, no `.apr`, reports attached | finalizer unit | `cargo test -p aprender-mcp-laya-train-lambda --lib finalize` | ❌ W0 | ⬜ pending |
| D-22 | register 11, dup-bin allowlist, selftest row 6 claimants | existing tests | `cargo test -p aprender-core --test monorepo_invariants`; `bash scripts/check_duplicate_bin_names.sh`; `just laya-gates-selftest` | ✅ (edit) | ⬜ pending |
| container | pinned stack imports; lifecycle passes | local-only | `just laya-train-lifecycle` | ✅ (local) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `scripts/laya_train/data_cases.json` + replay in `data.py --selftest` + `crates/aprender-mcp-laya-train/tests/validation_cases.rs`
- [ ] `crates/aprender-mcp-laya-train/` and `crates/aprender-mcp-laya-train-lambda/` skeletons with in-memory backend tests
- [ ] CI fragment for every new `tests/*.rs`
- [ ] CDK stack + synth recipe; ASL JSON assertion
- [ ] Dockerfiles + CodeBuild buildspec (owner runs); local arm64 smoke recipe
- [ ] fetch recipe + selftest with a tampered-file negative control
- [ ] probe checklist (CUDA/driver/stop-execution/SIGTERM), owner-gated

---

## Manual-Only Verifications

| Behavior | Decision | Why Manual | Test Instructions |
|----------|----------|------------|-------------------|
| CUDA probe job: `nvidia-smi`, `torch.cuda.is_available()`, matmul, tiny-fixture lifecycle on `--device cuda` | D-02 / D-13 | paid AWS action, owner only | owner runs the probe job; evidence JSON records `device_used` |
| motivation-v1 reproduction inside the approved band | D-16 / D-17 | one paid run, owner only | owner runs `train` on the Phase 12 data dir with `epochs` 12; evidence JSON |
| OAuth refusal before grants | D-10 | needs a deployed server + owner OAuth toggle | `just _laya-auth-refusal-check <url> 0` after deploy, before the grant step |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
