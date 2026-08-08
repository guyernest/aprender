---
phase: 1
slug: differentiable-minilm-conformance
status: ready
nyquist_compliant: true
wave_0_complete: false
created: 2026-08-07
updated: 2026-08-08
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
>
> **Revised 2026-08-08** after `/gsd:plan-phase 1 --reviews`. Changes: new plan 01-09 (attention-mask
> broadcast repair + exact erf GELU); 01-03 moved to wave 3 behind 01-09; 01-08 Task 2 split into an
> all-trainable gradient/step gate and a separate frozen-policy proof; every multi-filter
> `cargo test` command corrected to a single positional filter.
>
> **Re-revised 2026-08-08** after plan-checker re-verification. Changes: 01-04 Task 1 root-anchors the
> `tokenizer.json` cargo exclude and asserts fixture presence in `cargo package --list` (CB-510 class);
> D-08 restored to its structural form by sealing the tokenizer/import/encoder constructors to
> `pub(crate)` (user decision, sha256 boundary check retained as defense in depth); `setfit` feature
> closed over `dep:sha2`; the tier3 pv invocation moved into the tier3 RECIPE (it was previously
> unreachable from any tier); `SetFitError` gains an `Op(OpError)` variant; tolerance-constant
> generation routed through `pv` / the in-tree `aprender-contracts` parser instead of a hand-rolled
> YAML reader; task count corrected 21 -> 23; `pv` pre-build added to Wave 0.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (rustc 1.93.0) + proptest (bounded) + pv contracts + cargo-mutants 25.3.1 |
| **Config file** | Cargo.toml workspace lints, .clippy.toml, Makefile tiers |
| **Quick run command** | `cargo test -p aprender-core --lib --features setfit` |
| **Full suite command** | `make tier2` (after 01-08 wires the conformance suite in) |
| **Estimated runtime** | tier2 target <5s; conformance suite (slice-based) ~10-30s |

**Cargo filter rule (verified 2026-08-08):** `cargo test -p <crate> --lib <a> <b>` exits with
`error: unexpected argument '<b>' found`. Every command below passes **at most one** positional
test-name filter; multiple filters are chained with `&&` as separate invocations.

---

## Sampling Rate

- **After every task commit:** Run the task's `<automated>` command (module-filtered cargo test)
- **After every plan wave:** Run `make tier2` + `cargo check -p aprender-core --no-default-features && cargo check -p aprender-core --features setfit`
- **Before `/gsd:verify-work`:** `make tier3` — which, after 01-08 Task 3, invokes pv over `contracts/setfit-encoder-conformance-v1.yaml` from the tier3 recipe itself (capture the status directly, never through a pipe — CLAUDE.md rule 1); scoped cargo-mutants on autograd/ops + setfit/encoder.rs + nn/transformer/positional_encoding.rs; one recorded D-10 full-weight run
- **Max feedback latency:** 60 seconds for the cargo-test gates (full-weight suite excluded by design).
  **Two documented exceptions**, both `pv` invocations: tasks **1-01-01** and **1-04-03** verify with
  `cargo run --release -p aprender-contracts-cli --bin pv -- validate ...`. A COLD release build of the
  contracts CLI substantially exceeds 60s, so the budget does not apply to their first invocation.
  Mitigation: pre-build `pv` once as a Wave 0 step (`cargo build --release -p aprender-contracts-cli`);
  every invocation after that reuses the binary and returns in seconds. The same pre-build makes the
  tier3 contract step (01-08 Task 3) cheap. Record the pre-build as done before running either task.

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 1-01-01 | 01 | 1 | ENC-03, ENC-06 | T-1-02, T-1-16 | contract pv-valid; ENC-04 gate satisfiable (aggregate + two-sided exemptions); tolerances versioned later via pv diff | contract | `cargo run --release -p aprender-contracts-cli --bin pv -- validate contracts/setfit-encoder-conformance-v1.yaml && cargo check -p aprender-core` | ❌ W0 | ⬜ pending |
| 1-01-02 | 01 | 1 | ENC-03 | T-1-01 | OOV/length/zero-dim/overflow/non-binary-mask errors typed, no panic; checked_mul before allocation | unit + FD | `cargo test -p aprender-core --lib embedding_gather && cargo test -p aprender-core --lib additive_attention_mask` | ❌ W0 | ⬜ pending |
| 1-01-03 | 01 | 1 | ENC-03 | T-1-01 | zero-denominator typed error before compute | unit + FD | `cargo test -p aprender-core --lib masked_mean_pool && cargo test -p aprender-core --lib test_all_backward_names` | ❌ W0 | ⬜ pending |
| 1-02-01 | 02 | 1 | ENC-04 | T-1-03 | named==positional arity/order holds for NON-overriding implementors (positional-fallback default) | unit | `cargo test -p aprender-core --lib nn::module && cargo check --workspace` | ❌ W0 | ⬜ pending |
| 1-02-02 | 02 | 1 | ENC-04, ENC-05 | T-1-04, T-1-17 | mode flip never mutates params (to_bits); MHA semantic names so freeze cannot address the wrong tensor; names unique | unit | `cargo test -p aprender-core --lib named_module` | ❌ W0 | ⬜ pending |
| 1-09-01 | 09 | 2 | ENC-03 | T-1-18, T-1-05 | broadcast mask correct at B>1/H>1/T!=S AND graph-preserved (padded keys can no longer leak into attention) | unit + graph | `cargo test -p aprender-core --lib attention_mask_broadcast_ && cargo test -p aprender-core --lib transformer` | ❌ W0 | ⬜ pending |
| 1-09-02 | 09 | 2 | ENC-03 | T-1-19 | exact erf GELU cannot be silently replaced by the tanh variant (differential + independent oracle) | unit + FD | `cargo test -p aprender-core --lib gelu_exact_ && cargo test -p aprender-core --lib test_all_backward_names` | ❌ W0 | ⬜ pending |
| 1-04-01 | 04 | 2 | ENC-01 | T-1-06, T-1-SC, T-1-25 | pinned sha + per-file upstream digests re-verified fail-closed; hash-locked env; fixtures not gitignored AND not stripped by the cargo exclude (root-anchored `/tokenizer.json`, CB-510); D-10 path yields a loadable APR | infra | `cd scripts/setfit_fixtures && uv lock --check && cd ../.. && git check-ignore -v crates/aprender-core/tests/fixtures/setfit/slice_model.apr; test $? -eq 1 && cargo package -p aprender-core --list --allow-dirty \| grep -q 'tests/fixtures/setfit/tokenizer.json'` | ❌ W0 | ⬜ pending |
| 1-04-02 | 04 | 2 | ENC-01..06 | T-1-07, T-1-20 | manifest makes regeneration reviewable; tolerance floors prevent a zero tolerance; exemption list is measured data | infra | `cd crates/aprender-core/tests/fixtures/setfit && shasum -a 256 -c manifest.sha256 && python3 -c "import json,glob; [json.load(open(f)) for f in glob.glob('*.json')]"` | ❌ W0 | ⬜ pending |
| 1-04-03 | 04 | 2 | ENC-01..06 | T-1-07 | tolerances + zero_grad_floor only in versioned contract, own commit; codegen drift checked | contract | `cargo run --release -p aprender-contracts-cli --bin pv -- validate contracts/setfit-encoder-conformance-v1.yaml` | ❌ W0 | ⬜ pending |
| 1-03-01 | 03 | 3 | ENC-03 | T-1-01 | fallible signature; eps guard; piecewise derivative correct BELOW the clamp too | unit + FD | `cargo test -p aprender-core --lib l2_normalize_rows` | ❌ W0 | ⬜ pending |
| 1-03-02 | 03 | 3 | ENC-06 | T-1-01 | length/shape/rank/eps/non-finite typed errors; both clamp branches FD-checked | unit + FD | `cargo test -p aprender-core --lib cosine_similarity_rows && cargo test -p aprender-core --lib mse_loss` | ❌ W0 | ⬜ pending |
| 1-03-03 | 03 | 3 | ENC-03, ENC-04 | T-1-05, T-1-16 | severed batch>1 graph fails loudly; regression guard for the 01-09 mask repair; gate is satisfiable | integration | `cargo test -p aprender-core --test batched_graph_spike` | ❌ W0 | ⬜ pending |
| 1-05-01 | 05 | 3 | ENC-01 | T-1-SC | tokenizers cannot leak into minimal builds; `setfit` is dependency-CLOSED (`dep:sha2`) so the feature compiles standalone; `SetFitError::Op(OpError)` exists for 01-06/01-07; A-01 change is exactly one line | build matrix | `cargo check -p aprender-core --no-default-features && cargo check -p aprender-core --features setfit && sh -c '! cargo tree -p aprender-core --no-default-features -e normal | grep -q tokenizers'` | ❌ W0 | ⬜ pending |
| 1-05-02 | 05 | 3 | ENC-02 | T-1-11, T-1-21 | hostile text -> typed error; batch carries tokenizer identity; canonical ids preserved | fixture parity | `cargo test -p aprender-core --lib --features setfit tokenizer_parity && cargo test -p aprender-core --lib --features setfit sentence_batch` | ❌ W0 | ⬜ pending |
| 1-05-03 | 05 | 3 | ENC-01 | T-1-09, T-1-10, T-1-22, T-1-21 | >=15-field typed rejection incl. activation/dropouts/position-type/pad-id; unknown metadata tolerated; remap range-validated; slice bypass unreachable from the pin path; both constructors `pub(crate)` so no out-of-crate caller can build a MiniLmImport (D-08 seal) | unit (mutation matrix) | `cargo test -p aprender-core --lib --features conformance-fixtures import_pin && cargo test -p aprender-core --lib --features conformance-fixtures import_slice` | ❌ W0 | ⬜ pending |
| 1-06-01 | 06 | 4 | ENC-03, ENC-04 | T-1-11, T-1-21, T-1-12 | `from_import` sealed to pub(crate) (D-08 structural); boundary validation once (incl. tokenizer identity as defense in depth and min(256, max_positions)); real-slice mixed-batch grads finite with non-zero component aggregates | unit + integration | `cargo test -p aprender-core --lib --features conformance-fixtures encoder_` | ❌ W0 | ⬜ pending |
| 1-06-02 | 06 | 4 | ENC-03, ENC-05 | T-1-12, T-1-19 | modes flip behavior, never params (to_bits); attention dropout seeded without changing existing MHA callers; exact GELU in the FFN | unit | `cargo test -p aprender-core --lib --features conformance-fixtures encoder_mode_ && cargo test -p aprender-core --lib --features conformance-fixtures encoder_encode_ && cargo test -p aprender-core --lib mha_seeded_dropout_` | ❌ W0 | ⬜ pending |
| 1-07-01 | 07 | 5 | ENC-06 | T-1-13 | tensor-valued loss; non-finite label rejected before binary check; own contract equation | unit | `cargo test -p aprender-core --lib --features setfit pair_loss_` | ❌ W0 | ⬜ pending |
| 1-07-02 | 07 | 5 | ENC-04 | T-1-14, T-1-23, T-1-21 | SetFitMiniLm is the sole public constructor (crate-wide seal grep-asserted); conformance accessors expose borrows only; structured freeze groups, no glob DSL, exact boundaries, replacement semantics, no partial application, empty-match guard | unit | `cargo test -p aprender-core --lib --features conformance-fixtures setfit_model_` | ❌ W0 | ⬜ pending |
| 1-08-01 | 08 | 6 | ENC-03 | T-1-07 | tolerances derived from the contract via `pv` / the in-tree `aprender-contracts` parser (no bespoke YAML reader), with an agreement test that has been shown to turn red; manifest self-check; canonical ids drive the Rust path; harness constructs only via SetFitMiniLm | fixture parity | `cargo test -p aprender-core --features conformance-fixtures --test setfit_conformance conformance_` | ❌ W0 | ⬜ pending |
| 1-08-02 | 08 | 6 | ENC-04, ENC-06 | T-1-12, T-1-16, T-1-24 | all-trainable and frozen proofs on separate clean models; detach-negative shares the positive gate's helper with an explicitly requires_grad leaf | integration + negative | `cargo test -p aprender-core --features conformance-fixtures --test setfit_conformance gradient_gate_ && cargo test -p aprender-core --features conformance-fixtures --test setfit_conformance frozen_gate_ && cargo test -p aprender-core --features conformance-fixtures --test setfit_conformance detach_negative_` | ❌ W0 | ⬜ pending |
| 1-08-03 | 08 | 6 | ENC-01..06 | T-1-15, T-1-12, T-1-26 | gates live in tiers — the pv contract validation is in the tier3 RECIPE, proven by a `make tier3` run (CONTRACTS-list membership alone is unreachable from any tier); mutants hunt detachment within a declared budget (incl. the repaired mask) | infra + mutation | `make tier2 && make setfit-feature-matrix && sed -n '/^tier3:/,/Tier 3: PASSED/p' Makefile \| grep -q -e 'contract-validate' -e 'PV_BIN'` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

All Wave 0 gaps are owned by scheduled plans (no unowned scaffolding):

- [ ] `contracts/setfit-encoder-conformance-v1.yaml` (ten equations incl. `apply_additive_mask`, `gelu_exact`, `pair_cosine_mse`) — plan 01-01 Task 1 (tolerances + `zero_grad_floor` added by 01-04 Task 3 in its own commit, D-14)
- [ ] `scripts/setfit_fixtures/` uv project + committed `uv.lock` — plan 01-04 Task 1 (D-12)
- [ ] Fixture generator + full ENC-01..06 corpus incl. `activation_reference.json`, `parameter_order`, `analytically_zero`, `upstream_manifest.json` — plan 01-04 Task 2 (D-15)
- [ ] Slice APR (2 heads x 32, original head boundaries preserved) + SHA-256 manifest + `git check-ignore` verification — plan 01-04 Tasks 1-2 (D-09/D-13)
- [ ] `crates/aprender-core/tests/setfit_conformance/` harness + generated tolerance constants — plan 01-08 Task 1
- [ ] Per-op finite-difference test files under `autograd/ops/` — plans 01-01 / 01-03 / 01-09 (D-04)
- [ ] `crates/aprender-core/src/nn/transformer/tests_attention_mask_broadcast.rs` — plan 01-09 Task 1
- [ ] tier2/tier3 wiring + `setfit-feature-matrix` target — plan 01-08 Task 3 (D-26/D-06)
- [ ] **Pre-build `pv` once**: `cargo build --release -p aprender-contracts-cli` — removes the cold-build
      cost from tasks 1-01-01 / 1-04-03 and from the tier3 contract step, keeping the 60s feedback budget
      honest for every subsequent invocation (added 2026-08-08)
- [ ] Framework install: none (cargo/uv/cargo-mutants present; `pv` via Makefile PV_BIN cargo run; erf available in-tree via the existing `batuta-common` dependency)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Full ~90MB real-weight parity (D-10) | ENC-03 | Requires locally fetched pinned checkpoint + network; intentionally excluded from CI (SAFE-02 design constraint, not a gap). **Required once before phase completion** — plan 01-08 Task 3 step (6) records the revision, APR sha256, and result in the SUMMARY, or lists it as an explicitly unmet completion item | `uv run scripts/setfit_fixtures/fetch_full_weights.py` then `cargo test -p aprender-core --features setfit,conformance-fixtures,model-tests --test setfit_conformance -- --ignored` |

All other phase behaviors have automated verification.

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (**23 tasks** across 9 plans — 3+2+3+3+3+2+2+3+2, matching the 23 rows in the map above; every one carries an `<automated>` command. The prior "21" was a stale count, corrected 2026-08-08)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (each owned by a scheduled plan task)
- [x] No watch-mode flags
- [x] Feedback latency < 60s
- [x] Every command passes at most one positional cargo test filter (verified 2026-08-08)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
</content>
