---
phase: "8"
slug: "laya-decision-model-local-fine-tune-and-thin-mcp-server"
# status lifecycle: draft (seeded by plan-phase) → validated (set by validate-phase §6)
# audit-milestone §5.5 distinguishes NOT-VALIDATED (draft) from PARTIAL (validated + nyquist_compliant: false) (#2117)
status: draft
nyquist_compliant: false
wave_0_complete: false
created: "2026-09-25"
---

# Phase 8 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Source: `08-RESEARCH.md` §Validation Architecture.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust libtest / cargo-nextest (`--profile ci`); trybuild for the private-constructor proof; self-checking Python harness (exits non-zero on failure) |
| **Config file** | none new — CI profile `--profile ci` |
| **Quick run command** | `cargo test -p aprender-decide --lib && cargo test -p aprender-core --lib models::modernbert` |
| **Full suite command** | `cargo nextest run --profile ci --workspace --lib --exclude aprender-gpu --exclude aprender-cuda-edge --exclude aprender-compute` + `cargo test -p aprender-core --test monorepo_invariants --test readme_contract` |
| **Estimated runtime** | ~60 s quick; full suite several minutes |

---

## Sampling Rate

- **After every task commit:** Run the quick run command
- **After every plan wave:** Run the full suite + `make contract-validate` (new contracts added to `CONTRACTS`)
- **Before `/gsd-verify-work`:** Full suite green, gated full-model parity run pasted, `just laya-pack` re-score passing, live deploy checkpoint
- **Max feedback latency:** 120 seconds (quick run)

---

## Per-Task Verification Map

Task IDs are filled in by the planner; rows below are keyed by decision.

| Decision | Behavior | Threat Ref | Test Type | Automated Command | File Exists | Status |
|----------|----------|------------|-----------|-------------------|-------------|--------|
| D-13 | Tiny-config ModernBERT+Laya matches fp32 fixture (ids, layers, logits, probs ≤ 1e-5) | — | unit | `cargo test -p aprender-decide --lib laya::parity::tiny` | ❌ W0 | ⬜ pending |
| D-13 | Window mutation breaks first local layer only | — | unit | `cargo test -p aprender-core --lib models::modernbert::window_mutation` | ❌ W0 | ⬜ pending |
| D-13 | `layer_types` agreement; loader refuses missing tensor by name | — | unit | `cargo test -p aprender-core --lib models::modernbert::load` | ❌ W0 | ⬜ pending |
| D-17 | Full-model parity vs spike-025 fixture (env-gated `LAYA_MODEL_DIR`) | Unverified model served | integration | `LAYA_MODEL_DIR=... cargo test -p aprender-decide --release --test laya_parity` | ❌ W0 | ⬜ pending |
| D-17 | .apr byte determinism; `pack(unpack(x)) == x` | Tampered artifact | unit | `cargo test -p aprender-decide --lib artifact::determinism` | ❌ W0 | ⬜ pending |
| D-17 | Load ladder refuses each induced negative | Oversized/hostile artifact | unit | `cargo test -p aprender-decide --lib artifact::ladder` | ❌ W0 | ⬜ pending |
| D-17 | Private constructor unreachable | Unverified model served | trybuild | `cargo test -p aprender-decide --test ui` | ❌ W0 | ⬜ pending |
| D-05 | Criteria order survives both `preserve_order` backings | Label permutation | unit | `cargo test -p aprender-decide --lib task` | ❌ W0 | ⬜ pending |
| D-10/D-12 | Bounds refuse at N+1, accept maximal legal request | Cost/DoS | unit | `cargo test -p aprender-mcp-decide --lib` | ❌ W0 | ⬜ pending |
| D-09/D-11 | stdio E2E: one `classify` tool, identity + ordered probs | — | integration | `cargo test -p aprender-mcp-decide --test e2e_stdio` | ❌ W0 | ⬜ pending |
| D-15 | Server packages in `deployment_unit_bins`, `publish = false` | — | existing gate | `cargo test -p aprender-core --test monorepo_invariants` | ✅ | ⬜ pending |
| D-16 | CLAUDE.md cited paths exist; README counts | — | existing gate | `cargo test -p aprender-core --test readme_contract` | ✅ | ⬜ pending |
| D-07/D-08 | Gate refuses failing report; deploy refuses mismatched threshold/sha | Wrong binary deployed | recipe self-test | `just laya-gate-selftest` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `scripts/laya_train/` uv project (pyproject, `.python-version`, `uv.lock`) — after human-verify of flagged PyPI packages
- [ ] Tiny fixture generator + committed `crates/aprender-decide/tests/fixtures/laya_tiny.json`
- [ ] Copy of spike-025 fixture into `crates/aprender-decide/tests/fixtures/`
- [ ] Contract YAMLs (`decide-apr-v1`, `laya-parity-v1`, `decide-tool-boundary-v1`) with thresholds, bounds, tolerances — committed before any run is read
- [ ] `nextest list` proof the modernbert/decide tests compile into the CI lib run
- [ ] Decision on the cargo-pmcp package-resolution trap before the deploy wave

---

## Manual-Only Verifications

| Behavior | Decision | Why Manual | Test Instructions |
|----------|----------|------------|-------------------|
| Training run records device, recipe before scores, F16 reload | D-01..D-04 | Needs local GPU minutes and real data | `just laya-train data/decide/tweet-stance-16`; inspect report |
| Live cold call identity == H; maximal legal request < 30 s cold | D-18 | Live pmcp.run deploy, credentials | `just laya-deploy-verify` at human checkpoint |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
