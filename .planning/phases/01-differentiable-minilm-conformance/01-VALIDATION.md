---
phase: 1
slug: differentiable-minilm-conformance
status: ready
nyquist_compliant: true
wave_0_complete: false
created: 2026-08-07
updated: 2026-08-07
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (rustc 1.93.0) + proptest (bounded) + pv contracts + cargo-mutants 25.3.1 |
| **Config file** | Cargo.toml workspace lints, .clippy.toml, Makefile tiers |
| **Quick run command** | `cargo test -p aprender-core --lib --features setfit` |
| **Full suite command** | `make tier2` (after 01-08 wires the conformance suite in) |
| **Estimated runtime** | tier2 target <5s; conformance suite (slice-based) ~10-30s |

---

## Sampling Rate

- **After every task commit:** Run the task's `<automated>` command (module-filtered cargo test)
- **After every plan wave:** Run `make tier2` + `cargo check -p aprender-core --no-default-features && cargo check -p aprender-core --features setfit`
- **Before `/gsd:verify-work`:** `make tier3` including `PV_BIN validate contracts/setfit-encoder-conformance-v1.yaml`; scoped cargo-mutants on autograd/ops + setfit/encoder.rs
- **Max feedback latency:** 60 seconds (default gates; full-weight suite excluded by design)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 1-01-01 | 01 | 1 | ENC-03, ENC-06 | T-1-02 | contract pv-valid; tolerances versioned later via pv diff | contract | `cargo run --release -p aprender-contracts-cli --bin pv -- validate contracts/setfit-encoder-conformance-v1.yaml && cargo check -p aprender-core` | ❌ W0 | ⬜ pending |
| 1-01-02 | 01 | 1 | ENC-03 | T-1-01 | OOV/length errors typed, no panic | unit + FD | `cargo test -p aprender-core --lib embedding_gather && cargo test -p aprender-core --lib additive_attention_mask` | ❌ W0 | ⬜ pending |
| 1-01-03 | 01 | 1 | ENC-03 | T-1-01 | zero-denominator typed error before compute | unit + FD | `cargo test -p aprender-core --lib masked_mean_pool && cargo test -p aprender-core --lib test_all_backward_names` | ❌ W0 | ⬜ pending |
| 1-02-01 | 02 | 1 | ENC-04 | T-1-03 | N/A | unit | `cargo test -p aprender-core --lib module && cargo check --workspace` | ❌ W0 | ⬜ pending |
| 1-02-02 | 02 | 1 | ENC-04, ENC-05 | T-1-04 | mode flip never mutates params (to_bits) | unit | `cargo test -p aprender-core --lib named_module` | ❌ W0 | ⬜ pending |
| 1-03-01 | 03 | 2 | ENC-03 | T-1-01 | eps guard, no NaN | unit + FD | `cargo test -p aprender-core --lib l2_normalize_rows` | ❌ W0 | ⬜ pending |
| 1-03-02 | 03 | 2 | ENC-06 | T-1-01 | length/shape typed errors | unit + FD | `cargo test -p aprender-core --lib cosine_similarity_rows && cargo test -p aprender-core --lib mse_loss && cargo test -p aprender-core --lib test_all_backward_names` | ❌ W0 | ⬜ pending |
| 1-03-03 | 03 | 2 | ENC-03, ENC-04 | T-1-05 | severed batch>1 graph fails loudly | integration | `cargo test -p aprender-core --test batched_graph_spike` | ❌ W0 | ⬜ pending |
| 1-04-01 | 04 | 2 | ENC-01 | T-1-06, T-1-SC | pinned sha, hash-locked env, fixtures not gitignored | infra | `cd scripts/setfit_fixtures && uv lock --check && cd ../.. && git check-ignore -v crates/aprender-core/tests/fixtures/setfit/slice_model.apr; test $? -eq 1` | ❌ W0 | ⬜ pending |
| 1-04-02 | 04 | 2 | ENC-01..06 | T-1-07 | manifest makes regeneration reviewable | infra | `cd crates/aprender-core/tests/fixtures/setfit && shasum -a 256 -c manifest.sha256` | ❌ W0 | ⬜ pending |
| 1-04-03 | 04 | 2 | ENC-01..06 | T-1-07 | tolerances only in versioned contract, own commit | contract | `cargo run --release -p aprender-contracts-cli --bin pv -- validate contracts/setfit-encoder-conformance-v1.yaml` | ❌ W0 | ⬜ pending |
| 1-05-01 | 05 | 3 | ENC-01 | T-1-SC | tokenizers cannot leak into minimal builds | build matrix | `cargo check -p aprender-core --no-default-features && cargo check -p aprender-core --features setfit && sh -c '! cargo tree -p aprender-core --no-default-features -e normal | grep -q tokenizers'` | ❌ W0 | ⬜ pending |
| 1-05-02 | 05 | 3 | ENC-02 | T-1-11 | hostile text -> typed error | fixture parity | `cargo test -p aprender-core --lib --features setfit tokenizer_parity && cargo test -p aprender-core --lib --features setfit sentence_batch` | ❌ W0 | ⬜ pending |
| 1-05-03 | 05 | 3 | ENC-01 | T-1-09, T-1-10 | per-field typed rejection; slice bypass unreachable from public path | unit (mutation matrix) | `cargo test -p aprender-core --lib --features setfit,conformance-fixtures import_pin && cargo test -p aprender-core --lib --features setfit,conformance-fixtures import_slice` | ❌ W0 | ⬜ pending |
| 1-06-01 | 06 | 4 | ENC-03 | T-1-11, T-1-12 | boundary validation once, typed; grads reach all named params | unit + integration | `cargo test -p aprender-core --lib --features setfit,conformance-fixtures encoder_` | ❌ W0 | ⬜ pending |
| 1-06-02 | 06 | 4 | ENC-03, ENC-05 | T-1-12 | modes flip behavior, never params (to_bits) | unit | `cargo test -p aprender-core --lib --features setfit,conformance-fixtures encoder_mode_ && cargo test -p aprender-core --lib --features setfit,conformance-fixtures encoder_encode_` | ❌ W0 | ⬜ pending |
| 1-07-01 | 07 | 5 | ENC-06 | T-1-13 | tensor-valued loss, binary-label validation | unit | `cargo test -p aprender-core --lib --features setfit pair_loss_` | ❌ W0 | ⬜ pending |
| 1-07-02 | 07 | 5 | ENC-04 | T-1-14 | structured freeze groups, no glob DSL, exact boundaries | unit | `cargo test -p aprender-core --lib --features setfit,conformance-fixtures setfit_model_` | ❌ W0 | ⬜ pending |
| 1-08-01 | 08 | 6 | ENC-03 | T-1-07 | tolerances read from contract only; manifest self-check | fixture parity | `cargo test -p aprender-core --features setfit,conformance-fixtures --test setfit_conformance conformance_` | ❌ W0 | ⬜ pending |
| 1-08-02 | 08 | 6 | ENC-04, ENC-06 | T-1-12 | detach-negative gate fails on severed graph, in-band | integration + negative | `cargo test -p aprender-core --features setfit,conformance-fixtures --test setfit_conformance gradient_gate_ && cargo test -p aprender-core --features setfit,conformance-fixtures --test setfit_conformance detach_negative_` | ❌ W0 | ⬜ pending |
| 1-08-03 | 08 | 6 | ENC-01..06 | T-1-15 | gates live in tiers; mutants hunt detachment | infra + mutation | `make tier2 && make setfit-feature-matrix` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

All Wave 0 gaps are owned by scheduled plans (no unowned scaffolding):

- [ ] `contracts/setfit-encoder-conformance-v1.yaml` — plan 01-01 Task 1 (tolerances added by 01-04 Task 3 in its own commit, D-14)
- [ ] `scripts/setfit_fixtures/` uv project + committed `uv.lock` — plan 01-04 Task 1 (D-12)
- [ ] Fixture generator + full ENC-01..06 corpus — plan 01-04 Task 2 (D-15)
- [ ] Slice APR + SHA-256 manifest + `git check-ignore` verification — plan 01-04 Tasks 1-2 (D-09/D-13)
- [ ] `crates/aprender-core/tests/setfit_conformance/` harness — plan 01-08 Task 1
- [ ] Six `tests_*_backward.rs` FD files — plans 01-01/01-03 (D-04)
- [ ] tier2/tier3 wiring + `setfit-feature-matrix` target — plan 01-08 Task 3 (D-26/D-06)
- [ ] Framework install: none (cargo/uv/cargo-mutants present; `pv` via Makefile PV_BIN cargo run)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Full ~90MB real-weight parity (D-10) | ENC-03 | Requires locally fetched pinned checkpoint + network; intentionally excluded from CI (SAFE-02 design constraint, not a gap) | `uv run scripts/setfit_fixtures/fetch_full_weights.py` then `cargo test -p aprender-core --features setfit,conformance-fixtures,model-tests --test setfit_conformance -- --ignored` |

All other phase behaviors have automated verification.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (each owned by a scheduled plan task)
- [x] No watch-mode flags
- [x] Feedback latency < 60s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
