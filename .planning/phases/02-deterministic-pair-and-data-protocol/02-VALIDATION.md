---
phase: 2
slug: deterministic-pair-and-data-protocol
status: ready
nyquist_compliant: true
wave_0_complete: false
created: 2026-08-08
updated: 2026-08-08
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) + proptest + trybuild + pv (contract validation) |
| **Config file** | Cargo.toml (workspace) / .pmat-gates.toml |
| **Quick run command** | `cargo test -p <crate-under-change> --lib` |
| **Full suite command** | `make tier2` then tier3 contract validation over $(CONTRACTS) |
| **Estimated runtime** | ~60 seconds (quick), ~5 min (full) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p <crate-under-change>` (new crate is seconds)
- **After every plan wave:** Run `make tier2` + `make contrastive-data-boundary` (once it exists)
- **Before `/gsd:verify-work`:** `make tier3` green including pv validate over BOTH phase contracts
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 02-01.T1 | 02-01 | 1 | DATA-01/02 | T-02-01/02 | Baseline lands as-is, tests green | unit | `cargo test -p apr-cli --lib data_tweeteval` | ✅ (uncommitted → committed by task) | ⬜ pending |
| 02-01.T2 | 02-01 | 1 | DATA-01/02 | T-02-06 | Contract pv-valid + tier3-reachable | contract | `cargo run --release -p aprender-contracts-cli --bin pv -- validate contracts/tweet-eval-stance-benchmark-v1.yaml` | ✅ | ⬜ pending |
| 02-02.T1 | 02-02 | 2 | DATA-02..06 | T-02-04 | Publishable crate, typed error surface | unit + scripts | `cargo check -p aprender-contrastive-data && bash scripts/check_include_files.sh` | ❌ W0 (created by task) | ⬜ pending |
| 02-02.T2 | 02-02 | 2 | DATA-02..06 | T-02-06 | Phase contract pv-valid day one | contract | `pv -- validate contracts/contrastive-pair-protocol-v1.yaml` | ❌ W0 (created by task) | ⬜ pending |
| 02-02.T3 | 02-02 | 2 | DATA-05 | T-02-05 | D-04 boundary enforced in a tier | make gate | `make contrastive-data-boundary` | ❌ W0 (created by task) | ⬜ pending |
| 02-03.T1 | 02-03 | 3 | DATA-01/02 | T-02-08 | deny_unknown_fields; dual hashes | unit/property | `cargo test -p aprender-contrastive-data schema` / `hash` | ❌ new | ⬜ pending |
| 02-03.T2 | 02-03 | 3 | DATA-02/06 | T-02-07/10 | Typestate boundary + ledger | unit | `cargo test -p aprender-contrastive-data split` / `ledger` | ❌ new | ⬜ pending |
| 02-03.T3 | 02-03 | 3 | DATA-02 | T-02-09 | Dedup exclude-and-record | unit + golden | `cargo test -p aprender-contrastive-data dedup` | ❌ new | ⬜ pending |
| 02-04.T1 | 02-04 | 3 | DATA-04/05 | T-02-12 | Measured-vs-contracted fixture families | fixture gen | `shasum -a 256 -c .../setfit_reference/manifest.sha256` | ❌ new | ⬜ pending |
| 02-04.T2 | 02-04 | 3 | DATA-04/05 | T-02-11 | Fixture integrity in-test | integration | `cargo test -p aprender-contrastive-data --test reference_fixtures` | ❌ new | ⬜ pending |
| 02-05.T1 | 02-05 | 4 | DATA-03 | T-02-14/16 | Indexed pure RNG, no CSPRNG claim | unit/property | `cargo test -p aprender-contrastive-data rng` | ❌ new | ⬜ pending |
| 02-05.T2 | 02-05 | 4 | DATA-03 | T-02-15 | Witness-gated deterministic selection | unit/property | `cargo test -p aprender-contrastive-data select` | ❌ new | ⬜ pending |
| 02-05.T3 | 02-05 | 4 | DATA-03 | T-02-13 | Materialized manifest + goldens | unit + golden | `cargo test -p aprender-contrastive-data manifest` | ❌ new | ⬜ pending |
| 02-06.T1 | 02-06 | 4 | DATA-01/02 | T-02-17/20 | Thin adapter, byte-parity, dedup in manifest | unit + golden | `cargo test -p apr-cli --lib data_tweeteval` | ✅ (refactor) | ⬜ pending |
| 02-06.T2 | 02-06 | 4 | DATA-02 | T-02-18/19 | Real-duplicate live golden + contract growth | opt-in network + contract | `cargo test -p apr-cli --lib data_tweeteval -- --ignored` + pv validate | ✅ (extended) | ⬜ pending |
| 02-07.T1 | 02-07 | 5 | DATA-04/05 | T-02-22 | Canonical pair, capacity, fail-closed budget | unit + fixture | `cargo test -p aprender-contrastive-data pairs` | ❌ new | ⬜ pending |
| 02-07.T2 | 02-07 | 5 | DATA-04/05 | T-02-21 | Streaming sampler, singleton policy | property | `cargo test -p aprender-contrastive-data pairs` | ❌ new | ⬜ pending |
| 02-07.T3 | 02-07 | 5 | DATA-04/05 | T-02-23/24 | Replay record + streamed hash + dump | unit + golden | `cargo test -p aprender-contrastive-data manifest` | ❌ new | ⬜ pending |
| 02-08.T1 | 02-08 | 6 | DATA-05/06 | T-02-25/26 | In-band negatives RED + mirrors GREEN | in-band negative | `cargo test -p aprender-contrastive-data --test negative_leaky` / `--test negative_materializing` | ❌ new | ⬜ pending |
| 02-08.T2 | 02-08 | 6 | DATA-06 | T-02-27 | Leakage not constructible | trybuild | `cargo test -p aprender-contrastive-data --test ui` | ❌ new | ⬜ pending |
| 02-08.T3 | 02-08 | 6 | DATA-05/06 | T-02-28 | Mutation-hardened suite | mutation + gates | `make tier2 && make contrastive-data-boundary` | ❌ new | ⬜ pending |
| 02-09.T1 | 02-09 | 6 | DATA-03/04/05 | T-02-32 | CLI surface routed | unit | `cargo check -p apr-cli` + `--help` assertions | ❌ new | ⬜ pending |
| 02-09.T2 | 02-09 | 6 | DATA-03/04/05 | T-02-29/30/31 | Adapter-only commands, replay-stable hashes | unit + fixture | `cargo test -p apr-cli --lib data_contrastive` | ❌ new | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Commit the in-flight D-06 baseline work currently uncommitted on the branch (data_tweeteval.rs, eval diff, contract, docs example) — **covered by plan 02-01 Task 1 (Wave 1)**
- [ ] Restructure `contracts/tweet-eval-stance-benchmark-v1.yaml` so `pv validate` passes (PROVABILITY-001: add proof_obligations/kani_harnesses per `setfit-encoder-conformance-v1.yaml` shape precedent) — **covered by plan 02-01 Task 2 (Wave 1)**
- [ ] Append phase contracts to the Makefile `$(CONTRACTS)` explicit list so tier3 exercises them — **covered by plans 02-01 Task 2 (tweet-eval) and 02-02 Task 3 (contrastive-pair-protocol)**

*Existing cargo test infrastructure covers unit/property testing; every task above has an `<automated>` command whose prerequisites are created either by Wave-1/2 plans (contract/crate/Makefile) or by the task itself (test files land with the code).*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Pinned TweetEval source acquisition from network | DATA-01 | Requires network fetch of pinned upstream source; CI stays offline (SAFE-02) | Run `cargo test -p apr-cli --lib data_tweeteval -- --ignored` with network; verify SHA-256 hashes match the manifest and the exclusion record shows exactly the train:70 ≡ validation:3 duplicate (plan 02-06 Task 2) |

*All other phase behaviors have automated verification.*

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (plans 02-01/02-02)
- [x] No watch-mode flags
- [x] Feedback latency < 120s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending execution
