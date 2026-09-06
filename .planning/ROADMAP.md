# Roadmap: Aprender Native SetFit Classification

## Overview

This milestone delivers a native Rust SetFit lifecycle through five hard capability gates. It first
proves that the pinned MiniLM encoder is numerically conformant and genuinely differentiable, then
locks deterministic leakage-safe data and pair semantics, builds the faithful two-stage trainer,
turns its exact result into a self-contained production APR used by every Rust/CLI/HTTP surface, and
only then permits complete TweetEval comparison claims against the existing 9B LoRA baseline.

## Phases

**Phase Numbering:**

- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Differentiable MiniLM Conformance** - Prove the pinned encoder's shared batched Rust path matches fixtures and updates named parameters through the graph. (completed 2026-08-08)
- [ ] **Phase 2: Deterministic Pair and Data Protocol** - Make few-shot selection and bounded pair generation reproducible, provenance-complete, leakage-safe, and non-quadratic. (code-complete 2026-08-09; verification returned `human_needed` — 4 items pending in `02-HUMAN-UAT.md`)
- [x] **Phase 3: Faithful Two-Stage Trainer and Head** - Deliver an auditable encoder-tuning then unique-row classifier-fitting lifecycle that alone may identify as SetFit. (all 10 plans code-complete 2026-08-11; verification returned `human_needed` — 5/5 roadmap criteria verified, 47/50 must-have truths. **All 5 items adjudicated 2026-08-14**: `03-HUMAN-UAT.md` is `status: complete`, `03-VERIFICATION.md` reads `passed`. The 4 code-review blockers were VERIFICATION-layer weaknesses, not implementation defects — see `03-REVIEW.md`.) (completed 2026-08-14)
- [ ] **Phase 4: APR Artifact and Production Parity** - Persist, reload, inspect, predict, evaluate, and serve the exact verified model through shared CPU-first APIs. (all 22 plans code-complete 2026-08-16; verification returned `gaps_found` — 0/5 roadmap criteria fully met (2 FAILED, 3 PARTIAL), 94/95 must-have truths, and 04-11's per-crate mutation gate unmet. Five gap-closure plans 04-18..04-22 landed 2026-08-16 for the user-scoped subset that does NOT depend on F-10. **UAT ran 2026-08-16 at `b3f816c25`: 12 tests, 12 passed, 0 issues — `04-UAT.md`.** Verification reconciled to `gaps_acknowledged` with its verdict UNAMENDED. Closed since the report: BIND-004/backend_identity, WR-08, WR-09, WR-10, both D-04-11-A production survivors, F-07. **Still NOT closed:** F-10 (→ Phase 5, keeps SC1/SC3 UNMET and OPS-01/OPS-02 NOT MET), the per-crate mutation baselines + aggregate score (→ standalone compute ticket by human ruling; no mutation score exists for Phase 4), WR-01 (`fs::rename` not race-free), and SAFE-02's "in CI" clause (the 16 ci.yml setfit legs have never executed). **Blocked on `/gsd:secure-phase 04`** — security enforcement is ON and no `04-SECURITY.md` exists; `04-REVIEW.md` holds six unreferenced Warnings incl. a symlink-following non-exclusive temp file in the selection-lock write path. **This box has now been wrongly `[x]`-ed TWICE by `roadmap.update-plan-progress`, which marks a phase complete as soon as summary_count reaches plan_count — before any verifier runs and regardless of unmet must-haves. Do not re-check it until secure-phase lands and F-10 closes.**)
- [ ] **Phase 5: Benchmark and Claims Gate** - Produce the complete reproducible 40-cell SetFit-versus-9B-LoRA evidence set and reject incomplete or unequal claims.
- [ ] **Phase 6: Native Time-Series Forecasting Stack** - Ship pure-Rust Prophet, NeuralProphet and Chronos-Bolt forecasters behind stateless `forecast` MCP tools, each proven to parity with its Python original. (added 2026-09-05 from ten VALIDATED spikes; independent of Phases 1–5)

## Phase Details

### Phase 1: Differentiable MiniLM Conformance

**Goal**: Developers have one contracted, graph-connected MiniLM sentence encoder whose tokenizer,
batched forward path, train/eval behavior, gradients, and controlled updates are proven before any
SetFit trainer is exposed.
**Depends on**: Nothing (first phase)
**Requirements**: ENC-01, ENC-02, ENC-03, ENC-04, ENC-05, ENC-06
**Success Criteria** (what must be TRUE):

  1. A developer can import the pinned `all-MiniLM-L6-v2` revision and receives typed errors for any unsupported architecture, tokenizer, pooling, normalization, or configuration mutation.
  2. A developer can batch-tokenize once and run single or mixed-length padded inputs through the same `Transformer -> masked mean pooling -> L2 normalization` path, with ordered token facts and fixture-matching embeddings that are invariant to valid padding within declared tolerances.
  3. A developer can enumerate stable named parameter groups, switch the full encoder between train and deterministic evaluation behavior, and prove that registered parameters do not change merely because the mode changes.
  4. On a controlled non-degenerate pair batch, every contracted trainable embedding, attention, FFN, and normalization component receives a finite non-zero gradient and changes after one optimizer step, while frozen components remain byte-identical and sentence embeddings move in the loss-reducing direction.
  5. The cosine-similarity MSE objective remains a finite graph-connected tensor and its forward values and gradients, together with all new gather/mask/pool/normalize primitives, match frozen reference and finite-difference fixtures; deliberate detachment makes the gate fail.

**Plans**: 9 plans in 6 waves

Plans:
**Wave 1**

- [x] 01-01-PLAN.md — Conformance contract skeleton (ten equations) + gather/mask/pool autograd ops (wave 1)
- [x] 01-02-PLAN.md — Module trait named traversal (positional-fallback default) + train/eval propagation (wave 1)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 01-09-PLAN.md — Attention-mask broadcast repair + exact erf GELU op (wave 2)
- [x] 01-04-PLAN.md — Fixture corpus, slice APR, SHA-256 manifest, tolerance-first contract commit (wave 2)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 01-03-PLAN.md — normalize/cosine/MSE ops + batched mixed-length graph spike (wave 3)
- [x] 01-05-PLAN.md — setfit feature, tokenizer boundary (SentenceBatch), typed pinned import (wave 3)

**Wave 4** *(blocked on Wave 3 completion)*

- [x] 01-06-PLAN.md — BertSentenceEncoder graph-connected forward + mode/dropout contract (wave 4)

**Wave 5** *(blocked on Wave 4 completion)*

- [x] 01-07-PLAN.md — Pair cosine-MSE loss, SetFitMiniLm bound type, freeze groups (wave 5)

**Wave 6** *(blocked on Wave 5 completion)*

- [x] 01-08-PLAN.md — Conformance gates (all-trainable + frozen), detach-negative, mutation + tier wiring (wave 6)

### Phase 2: Deterministic Pair and Data Protocol

**Goal**: Users can prepare and replay exact few-shot training inputs and bounded contrastive pairs
without split leakage, provenance ambiguity, silent row loss, or Cartesian-product growth.
**Depends on**: Phase 1
**Requirements**: DATA-01, DATA-02, DATA-03, DATA-04, DATA-05, DATA-06
**Success Criteria** (what must be TRUE):

  1. A user can acquire the pinned TweetEval abortion-stance source and produce canonical 587/66/280 train/validation/test JSONL plus exact labels, hashes, and provenance without committing tweet text; malformed, duplicate-ID, conflicting, or unknown data fails with typed errors, while cross-split duplicate *content* is excluded from the training pool and recorded in the manifest (D-27).
  2. A user can select exactly 8, 16, 32, or 64 unique canonical-training examples per class for every contracted seed and replay the same ordered selected-ID manifest and semantic hashes.
  3. A user can replay positive and negative pair manifests whose endpoints are distinct selected training IDs, whose targets agree with class identity, whose unordered identities cannot conflict, and whose singleton-class behavior is explicit and versioned.
  4. A user can impose a per-epoch pair budget, and increasing the example count under a fixed budget retains `O(examples + pair budget)` state and storage instead of materializing a Cartesian product.
  5. Validation/test endpoints, cross-split duplicate content, and the merged SetFit compatibility test used for model selection are rejected fail-closed, while the manifest proves canonical train/validation/test isolation. (Per D-27, "rejected fail-closed" means excluded from the training pool — no selection or pair may span split roles, and pool exhaustion below `shots_per_class` is a typed error; prepare-time duplicate *content* is excluded and recorded, not fatal.)

**Plans**: 9 plans in 6 waves

Plans:
**Wave 1**

- [x] 02-01-PLAN.md — Hash-attested D-06 baseline in its own PR + pv-valid tweet-eval contract + $(CONTRACTS) wiring + CLAUDE.md `pv diff` fix (wave 1)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 02-02-PLAN.md — aprender-contrastive-data scaffold (3 missing workspace deps + full module skeleton + widened error enum) + contrastive-pair-protocol-v1 contract + positive-allowlist D-04 gates (wave 2)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 02-03-PLAN.md — Bytes→typed layer: schema, dual hashes, typestate splits, coalesced dedup, persistable ledger, PreparedDataset<Canonical|Compatibility> profile typestate (wave 3)
- [x] 02-04-PLAN.md — Measured + contracted SetFit pair-count fixture families (incl. K=N adversarial layout) + shared test models + integrity verifier (wave 3)

**Wave 4** *(blocked on Wave 3 completion)*

- [x] 02-05-PLAN.md — Philox few-shot selection (frozen LE encoding, labeled SelectedExample) + non-circular manifest payload with persisted ledger + strict Selection::replay + goldens (wave 4)
- [x] 02-06-PLAN.md — data_tweeteval thin-adapter relocation on the D-05 seam + dataset-attestation ingest boundary + real-duplicate golden + contract growth (wave 4)

**Wave 5** *(blocked on Wave 4 completion)*

- [x] 02-07-PLAN.md — Pair protocol: fallible capacity, binding hard cap, total degenerate policy, O(K) streaming sampler, public state diagnostics, untrusted-pair DTO, tuple-committing replay hash (wave 5)

**Wave 6** *(blocked on Wave 5 completion)*

- [x] 02-08-PLAN.md — Honesty gates: in-band leaky/materializing negatives (incl. K=N), trybuild non-constructibility, binding-coverage audit gate, bounded scoped mutation (wave 6)
- [x] 02-09-PLAN.md — `apr data select` / `apr data pairs` CLI surface: attested ingest, required contracted seed, atomic writes, strict replay + documented workflow (wave 6)

### Phase 3: Faithful Two-Stage Trainer and Head

**Goal**: Users can reproducibly tune the encoder and then fit one stable multiclass head on each
unique tuned embedding exactly once, with SetFit identity and test access enforced by lifecycle
evidence.
**Depends on**: Phase 2
**Requirements**: TRN-01, TRN-02, TRN-03, TRN-04, TRN-05, TRN-06, TRN-07, SAFE-03
**Success Criteria** (what must be TRUE):

  1. A developer can run only the legal `Prepared -> EncoderTuned -> HeadFitted -> ArtifactReloadedAndVerified` transitions, and invalid learning, batching, warmup, clipping, length, pairing, freeze, regularization, seed, or device configuration fails before training begins.
  2. A run cannot identify itself as SetFit or export a model until named encoder gradients, parameter deltas, embedding deltas, and pair-loss behavior pass; frozen probes, centroids, and other non-updating baselines remain explicitly labeled as such.
  3. After encoder tuning, dropout is disabled and each unique selected row is encoded exactly once in evaluation/no-gradient mode before one deterministic L2-regularized multinomial softmax head is fit for any ordered `K >= 2` labels; pair multiplicity cannot reweight the head data.
  4. The shared binary/multiclass head reports explicit convergence or typed failure, finite logits and probabilities summing to one, stable ordered-label semantics, and reference-matching regularization behavior.
  5. Two clean CPU runs reproduce selected IDs, ordered pairs and batches, step count, declared loss trace, semantic hashes, and predictions, and canonical test access remains blocked until canonical-validation selection emits a selection-lock record.

**Plans**: 10 plans in 7 waves

Plans:
**Wave 1** *(**orchestrator owns the branch**: create `gsd/phase-3-two-stage-trainer` from the
current HEAD of `gsd/phase-2-contract-gate` BEFORE dispatching this wave, and record the base SHA in
the phase SUMMARY set — no plan creates it, all three CHECK and stop. Sequential execution is the
recommended default; per-executor `git worktree` if true parallelism is wanted. See each wave-1
plan's `<wave_1_concurrency>` block.)*

- [x] 03-01-PLAN.md — f64 L-BFGS widening with a frozen f32 golden trajectory and the four-channel non-finite matrix (wave 1)
- [x] 03-02-PLAN.md — Keyed Philox dropout with the forward-ordinal (branch) coordinate + fixed-pool GEMM thread-count falsification gate (wave 1)
- [x] 03-03-PLAN.md — aprender-train setfit feature + typestate skeleton + 12-knob config with validated deserialization + scheduler/reduce/epoch primitives (wave 1)

**Wave 2** *(blocked on Wave 1 completion)*

- [x] 03-04-PLAN.md — MultinomialLogisticRegression head + central-difference gradient suite + sklearn factor-of-2 falsification + multinomial-head-v1 contract (wave 2)
- [x] 03-05-PLAN.md — tune_encoder loop with a pinned step order and in-band execution digests + evidence capture + the epsilon calibration matrix (wave 2)

**Wave 3** *(blocked on Wave 2 completion)*

- [x] 03-06-PLAN.md — setfit-train-lifecycle-v1 contract (per-class epsilon + calibration regime) + armed evidence gate + in-band negatives + FrozenProbeRun (SAFE-03) (wave 3)

**Wave 4** *(blocked on Wave 3 completion)*

- [x] 03-07-PLAN.md — Encode-once head input with an encode ledger + fit_head transition (pair multiplicity inexpressible) + pair-weighted in-band negative (wave 4)

**Wave 5** *(blocked on Wave 4 completion)*

- [x] 03-08-PLAN.md — Sealed SetFitCodec seam + complete bundle + bytes-reconstruction path + ArtifactReloadedAndVerified + recorded-digest accessors (wave 5)

**Wave 6** *(blocked on Wave 5 completion)*

- [x] 03-09-PLAN.md — Trusted validation evaluator + candidate-committing SelectionLock + object-bound CanonicalTestToken (TRN-07) (wave 6)

**Wave 7** *(blocked on Wave 6 completion)*

- [x] 03-10-PLAN.md — trybuild non-constructibility proofs (7 cases) + cross-process two-clean-runs gate + adjusted-score mutation + full-suite closing audit (wave 7)

### Phase 4: APR Artifact and Production Parity

**Goal**: Users deploy the exact trained SetFit model as one verified offline APR whose shared core
implementation produces equivalent results through library, CLI, evaluation, and HTTP serving
surfaces on the mandatory CPU profile.
**Depends on**: Phase 3
**Requirements**: APR-01, APR-02, APR-03, APR-04, APR-05, OPS-01, OPS-02, OPS-03, OPS-04, OPS-05, OPS-06, SAFE-01, SAFE-02
**Success Criteria** (what must be TRUE):

  1. A user can save, inspect, and load one checksummed F32 `setfit-apr-v1` containing the complete encoder, exact tokenizer bytes/hash, preprocessing policy, head, ordered labels, resolved configuration, training evidence, and provenance without Python, network access, or sidecars; malformed, oversized, incomplete, inconsistent, or non-finite artifacts fail before prediction.
  2. Training closes its in-memory model, reloads the APR through the production core loader, and proves exact tokenizer/configuration/tensor state plus tolerance-bounded embeddings, logits, and probabilities and exact labels; every production consumer rejects anything short of `ArtifactReloadedAndVerified`.
  3. A Rust caller and an `apr` user can complete the CPU `train -> APR -> inspect -> eval -> predict` lifecycle through stable fallible APIs and machine-readable output, while generic APR commands auto-detect SetFit and reuse the shared core tokenizer, pooling, and model path.
  4. Rust, CLI, and native HTTP callers can classify ordered single or mixed-length batches and receive matching labels, full probabilities, optional logits, margins, token/truncation facts, latency, backend identity, and the same artifact hash in predictions and readiness.
  5. A developer can run offline executable contracts and the supported CPU build/test feature matrix to detect detached gradients, invalid data/math, leakage, label drift, artifact mismatches, and core/CLI/HTTP parity failures; an explicitly requested unavailable device fails instead of silently falling back or misreporting its backend.

**Plans**: 22 plans in 12 waves — 17 delivered, then 5 gap-closure plans added 2026-08-16 after `04-VERIFICATION.md` returned `gaps_found` (revised 2026-08-15 after cross-AI review — 04-REVIEWS.md; then 04-17 added mid-phase at a user checkpoint to land the two public-API doors — `into_artifact_bytes` and the sealed `SetFitCredential` — that waves 5-6 proved missing, which shifted the dependent plans one wave later)

**Branch base**: `gsd/phase-2-contract-gate` @ d66678e7a (Phase 3 complete + UAT + CR-01..04 fixes).
The orchestrator creates `gsd/phase-4-apr-parity` from that HEAD before dispatching wave 1; wave
merges land back on `gsd/phase-2-contract-gate` (Phase 3 precedent). No PR to `main` — human's call.

Plans:
**Wave 1**

- [x] 04-01-PLAN.md — setfit-apr-v1 contract: normative storage map (incl. head tensors), SetFitArtifactDoc field list, doc<->bundle bijection table, cap/probes/tolerances, backend-identity grammar, lock lifecycle + $(CONTRACTS)/audit wiring + CLAUDE.md realizar-first SetFit row (wave 1)

**Wave 2** *(blocked on Wave 1; three parallel plans, zero file overlap)*

- [x] 04-02-PLAN.md — Core artifact writer: canonical tensors + setfit.head.weight/bias + U8 tokenizer blob, one-key deterministic metadata, embedded synthetic probes + cross-process determinism proofs (wave 2)
- [x] 04-13-PLAN.md — SetFitBundle provenance (field 20) read off the run + schema-version bump — makes byte-canonical closure achievable (wave 2)
- [x] 04-14-PLAN.md — aprender-train public API for the CLI: SetFitTrainConfig::to_request (validated override merge) + SelectionLock::from_canonical_bytes (durable lock reconstruction) (wave 2)

**Wave 3** *(blocked on Wave 2)*

- [x] 04-03-PLAN.md — Production loader: bounded reader + fail-closed ladder + probe replay + VerifiedSetFitModel typestate + induced-corruption suite + trybuild non-constructibility (wave 3)

**Wave 4** *(blocked on Wave 3; two parallel plans, zero file overlap)*

- [x] 04-04-PLAN.md — ClassifyRequestDocument + ClassifyResponse with enforced validation (D-08) + classify + execution-derived backend identity (D-12) (wave 4)
- [x] 04-05-PLAN.md — AprCodec sealed adapter with a proven 20-field bijection + typed CodecError::Artifact + APR-03 round trip at Tolerance::EXACT (wave 4)

**Wave 5** *(blocked on Wave 4; three parallel plans, zero file overlap)*

- [x] 04-06-PLAN.md — `apr setfit train`: setfit feature, namespace, config-file-first with validated override merge, bounded artifact reader, atomic write, fail-closed device gate (wave 5)
- [x] 04-12-PLAN.md — OPS-01 public-API lifecycle proof (train -> save -> load -> embed -> classify -> inspect) + cargo-tree boundary evidence (wave 5)
- [x] 04-16-PLAN.md — reload_verified_run_from_apr: the fresh-process door to a verified run, minting only by re-entering the existing trusted policy (wave 5)

**Wave 6** *(blocked on Wave 5; two parallel plans, zero file overlap)*

- [x] 04-07-PLAN.md — Generic `apr predict` (JSON request document) + inspect APR-05 recovery + eval validation-lock artifact and gated canonical test access (TRN-07 positive, D-16) (wave 6)
- [x] 04-08-PLAN.md — Serve surface: setfit feature, AppState slot, always-installed /v1/classify with 503 handler, readiness hash, bounded startup read, oneshot tests (wave 6)

**Wave 7** *(blocked on Wave 6; two parallel plans, zero file overlap)*

- [x] 04-09-PLAN.md — Three-surface parity harness on one shared request document + frozen goldens + in-band skewed negative + ONE tier3 spawned-serve smoke with a specified port protocol (wave 7)
- [x] 04-15-PLAN.md — Spawned-binary OPS-02 lifecycle chain (train -> inspect -> validation lock -> test eval -> predict) + generic APR tooling compatibility (D-01 / A3) (wave 7)

**Wave 8** *(blocked on Wave 7)*

- [x] 04-10-PLAN.md — Make gates with one filter per invocation and ran-something guards + four-crate x three-profile SAFE-02 matrix + tier wiring + OPS-01 boundary gate (wave 8)

**Wave 9** *(blocked on Wave 8; NOT autonomous — human checkpoint)*

- [x] 04-11-PLAN.md — ci.yml extension proposed as a patch file then human-approved, per-crate mutation gate, closing requirements audit incl. TRN-07 (wave 9)

**Wave 10** *(added mid-phase at a user checkpoint; the two public-API doors waves 5-6 proved missing)*

- [x] 04-17-PLAN.md — `SetFitRun::into_artifact_bytes` (the consuming bytes door, borrowck-enforced read-then-take) + the sealed `SetFitCredential` trait, which unblocked 04-12 and 04-16 (wave 10)

**Gap closure** *(added 2026-08-16 after `04-VERIFICATION.md` returned `gaps_found`; user-scoped to six items. Does NOT close F-10 — OPS-01 and OPS-02 stay NOT MET and are Phase 5 work per the blocking note below)*

**Wave 11** *(four parallel plans, zero `files_modified` overlap)*

- [x] 04-18-PLAN.md — WR-09 + WR-08: bound `apr inspect`'s attacker-controlled metadata allocation by the shared 16 MiB cap, and make the over-cap case one typed refusal so predict/eval/inspect/serve stop contradicting each other about one file (wave 11)
- [x] 04-19-PLAN.md — `backend_identity` binding: point the registry row at the shipped `ExecutionBackend::identity`, earn the `implemented` flip with a compile-witnessed resolution guard, clear the last BIND-004 (wave 11)
- [x] 04-20-PLAN.md — WR-10: run `apr eval --lock-out`'s no-clobber gate before the dataset ingest instead of after the full multi-candidate sweep, restoring the ordering discipline `setfit_train.rs:12-20` states; end-to-end evidence via a spawned decoy case, since the dispatch tag gate makes an untagged live probe unreachable (wave 11)
- [x] 04-21-PLAN.md — the two named mutation survivors in `api/setfit_handlers.rs`, re-measured at HEAD and diagnosed from varied inputs; each kill confirmed by a `-F`-scoped cargo-mutants re-run (wave 11)

**Wave 12** *(blocked on Wave 11 — 04-21 also edits the Makefile, and the Makefile is this plan's subject)*

- [x] 04-22-PLAN.md — F-07: run bashrs for real over the Makefile and `scripts/`, make `bashrs-lint-makefile` capable of failing, wire one scoped baseline-non-increase gate founded on shell semantics (two error findings are measured bashrs false positives), and triage the repo-wide backlog with an owner — no skipped check reported as passing (wave 12)

### Phase 5: Benchmark and Claims Gate

**Goal**: Users can audit and recompute a complete, selection-safe TweetEval comparison between the
verified SetFit APR and the existing 9B LoRA path across every contracted shot and seed.
**Depends on**: Phase 4
**Blocked by a Phase 3 gate until a contract edit lands**: Phase 3's SetFit-identity gate
freezes its per-parameter update thresholds against a CALIBRATED REGIME, and by user decision
that regime contains only the fixture encoder's architecture fingerprint. A benchmark run against
the production `all-MiniLM-L6-v2` therefore returns `UncalibratedRegime` and fails closed rather
than returning a verdict. Unblocking it requires calibrating on the production encoder and adding
its fingerprint to `contracts/setfit-train-lifecycle-v1.yaml` — a deliberate, `pv diff`-flagged
contract edit per Phase 3 D-10(c), never an inline relaxation by a Phase 5 executor.
**Requirements**: EVAL-01, EVAL-02, EVAL-03, EVAL-04, EVAL-05
**UI hint**: no
**Success Criteria** (what must be TRUE):

  1. A user can evaluate ordered predictions with the official `F_avg = (F1_against + F1_favor) / 2`, per-class metrics, three-class macro-F1, MCC, confusion matrix, and validation-only calibration diagnostics bound to explicit ordered labels.
  2. A user can run all 40 shot/seed cells for both SetFit and 9B LoRA—shots `{8,16,32,64}` crossed with the ten contracted seeds—with an identical sampled-ID hash for both methods in every cell.
  3. A user receives one machine-readable row per method/shot/seed run containing dataset/model revisions, selection lock, artifact hash, encoder-update evidence, backend/hardware identity, quality metrics, and consistently bounded resource measurements.
  4. A user can exactly recompute headline means, dispersion, uncertainty, and paired SetFit-versus-LoRA deltas from all stored rows, while any missing, selectively omitted, unmatched, or post-test-selected cell invalidates the report.
  5. A user can compare training time, cold/warm latency, throughput with batch/warmup boundaries, peak memory, artifact size, calibration, and classification quality measured from the same reloaded production artifacts.

**Branch base**: Phase 5 continues on `gsd/phase-2-contract-gate` per the 02-01 policy (phases
2-4 all ride this branch; no PR has been opened — opening one is the human's call).

**Plans**: 11/14 plans executed in 8 waves — replanned 2026-08-17 after 05-01's measurements refuted 05-03's premise. At production step count (s64 = 1536 steps) five of six parameter classes have NO legal ε under the contracted 10×/10× rule (only `layer_norm_weight` survives); recorded as `05-CONTEXT.md` D-16..D-18. 05-03 was reshaped from "prepare → approve → commit" to "derive candidates → SELECT → commit" (D-04 ceremony intact) and now decides which of two already-contracted lower bounds binds. 05-14 was ADDED (wave 1, before 05-03) to make the evidence gate fail-closed on window collapse — it currently exits `rc=0` while printing `EMPTY` five times, because separation is asserted while `supports_margin` is only reported. No other plan's wave or depends_on changed; 05-14 is numbered 14 rather than inserted so the dependency graph is not renumbered.

Plans:
**Wave 1** *(the F-10 unblock work — D-01 — plus the independent numerics substrate 05-04, whose t_critical.json fixture is the source of 05-05's frozen contract literal; nothing F-10-downstream runs until the edit lands)*

- [x] 05-01-PLAN.md — Production calibration measurement: freeze E/B from pinned setfit 1.1.3, timed s8 probe, boundary matrix, ε windows + proposed regime entry (NOT autonomous: conditional >1hr compute check-in)
- [x] 05-02-PLAN.md — Per-regime Thresholds restructuring (table_for lookup, fixture semantics byte-identical, len==1 preserved)
- [x] 05-04-PLAN.md — Numerics substrate: multiclass top-label ECE + Brier (calibration-v1-bound), f64 paired-t + frozen t_{0.975,9}, scipy/sklearn fixtures in the pinned uv env
- [x] 05-14-PLAN.md — **(added 2026-08-17, D-18)** Fail-closed evidence gate: a class with no legal ε must FAIL the run, not merely print `EMPTY`. Rule-agnostic (enforces "the emitted window is non-empty for every class this regime gates", not the arithmetic), so 05-03's rule choice leaves it holding. Doubles as 05-03's own verification — RED today, GREEN once a production table lands, on identical input; the verify derives its expected status from the contract's seed list rather than hardcoding an assertion that would invert. Protects the 12 persisted evidence files by digest (12/12) and asserts no `to_canonical_bytes` surface was removed

**Wave 2** *(blocked on Wave 1 for 05-03 and for 05-05 — which copies its frozen t literal from 05-04's t_critical.json; 05-06 is an independent foundation)*

- [x] 05-03-PLAN.md — **(revised 2026-08-17; now also depends on 05-14)** Derive candidate ε bases → blocking human SELECT (six enumerated options incl. HALT) → the three-place synchronized contract/code/test edit at the D-04 checkpoint (NOT autonomous), one commit, pv diff evidence. Chooses which of two already-contracted lower bounds binds — the DERIVATION invariant's 1e-8 near-null bound (now unsatisfiable at 1536 steps) or the noise-floor clearance invariant (satisfiable for all six classes; already the operative bound for `layer_norm_weight`, whose derivation row reads `worst 1e-8 = 0.000e0`). Any multiplier on the noise floor is recorded as CHOSEN, never cited — the bound is contracted, the factor is not. "Relax the safety factors" is refuted by arithmetic (largest admissible factor ~0.31, largest product ~3.13 against a contracted 100 — an inverted margin where a near-null run would PASS). Includes the 11-site `sole()` migration 05-02 deliberately armed, with `sole()` deleted rather than re-armed. Recommends measuring `s64:31`/`s64:53` first (~5.5 h): under the near-null bound those passes provably could not change the verdict, but under the noise-floor bound they SET the worst floor and can
- [x] 05-05-PLAN.md — setfit-benchmark-claims-v1.yaml + BenchRow/RunManifest (method-tagged, deny_unknown_fields, digest-verify-before-return) + $(CONTRACTS)/audit wiring
- [x] 05-06-PLAN.md — `apr finetune --task classify --selection-manifest` + explicit seed/val_split/early-stop control + A6 probe + run_classify_core shared entry + the LoRA reload preflight (tracer: adapter save → fresh-process reload → ordered probability vector; hard gate before any 9B compute)

**Wave 3** *(blocked on Wave 2)*

- [x] 05-07-PLAN.md — Production chain proof: spawned train→inspect→eval(lock)→eval(test)→predict ladder green (04-15's rung 4 flips 6→0) + F-10 blast-radius prose re-audit
- [x] 05-08-PLAN.md — evaluate_rows_from_artifact per-row evaluator door + QualityBlock assembly (F_avg/MCC/confusion/validation-only ECE+Brier)

**Wave 4** *(blocked on Wave 3)*

- [x] 05-09-PLAN.md — `apr setfit bench run`: one cell per method, contracted resource protocol (cold/warm/throughput/peak-RSS), --record transport ingest, 40-cell driver script

**Wave 5** *(blocked on Wave 4 — the claims gate must EXIST before any expensive cell is generated; cross-AI review consensus item 4)*

- [x] 05-10-PLAN.md — bench_gate fail-closed verify (incl. recomputed lock/ledger provenance) + closed-form aggregation + `bench report` (estimation-first, mechanism-labelled) + six in-band doctored negatives + non-vacuous Make/binding gates

**Wave 6** *(blocked on Wave 5 — 40 GPU cells run only after the gate that judges them exists, and after one pilot cell passes it)*

- [ ] 05-11-PLAN.md — lambda-vector checkpoint (A1: access + 9B base-weight hash, NOT autonomous) then the 40 LoRA GPU cells, transported + --record-ingested

**Wave 7** *(blocked on Wave 6 — shared run-manifest file)*

- [ ] 05-12-PLAN.md — The 40 SetFit CPU cells from reloaded production artifacts (NOT autonomous: compute pre-auth gate; sequential by design — parallel cells would invalidate EVAL-05 resource numbers); 80/80 manifest closure + pairing spot-check

**Wave 8** *(blocked on Wave 7)*

- [ ] 05-13-PLAN.md — The 80-row report (exact-recompute demonstrated bit-for-bit) + D-11 qa refusal message + phase closing audit

### Phase 6: Native Time-Series Forecasting Stack

**Goal**: Users can forecast a time series in one stateless MCP call — `ds[]`, `y[]`, horizon in;
forecast with bands and components out — from pure-Rust Prophet and NeuralProphet ports and an
embedded zero-shot Chronos-Bolt, each proven to parity with its Python original and served the
way SetFit is served (thin pmcp servers, stdio + streamable-HTTP, Lambda-shaped).
**Depends on**: none of Phases 1–5 functionally — an independent track that reuses the Phase 4
thin-MCP pattern (`crates/aprender-mcp-setfit`, `crates/aprender-mcp-setfit-lambda`) and may run
alongside Phase 5's remaining GPU waves.
**Requirements**: TBD
**Requirements note**: forecasting has no REQ-IDs in `.planning/REQUIREMENTS.md` (that document is
the SetFit milestone's). The binding requirements are the five `prophet-forecast-mcp` decisions in
`.planning/spikes/MANIFEST.md`, transcribed as D-01..D-05 in `06-CONTEXT.md`, plus the Success
Criteria below.
**UI hint**: no (the demo page is a static, spike-proven MCP client copied into each server crate;
not a product UI)
**Spike evidence**: ten VALIDATED spikes (001–010), packaged as `Skill("spike-findings-aprender")`
in `.claude/skills/spike-findings-aprender/`; raw experiments, oracle fixtures and run outputs in
`.planning/spikes/`.
**Success Criteria** (what must be TRUE):

  1. A user can call one stateless `forecast` tool on `aprender-mcp-forecast` (stdio and streamable-HTTP) with `ds`, `y`, `horizon`, `freq` and `model: prophet | neuralprophet`, and receive `yhat`, `yhat_lower`, `yhat_upper`, `trend`, named components, timing and a diagnostics object in under 2 s for a 3 000-point daily series; every malformed input (unknown field, fewer than 10 or more than 20 000 points, unsorted, duplicate or impossible dates, constant `y`, horizon 0 or above 3 650, unknown `freq`, logistic growth without a valid `cap`) is a validation error, never a silent default.
  2. The Prophet port in `aprender-forecast` reproduces Python Prophet 1.4.0 on the committed Peyton Manning, air passengers, retail sales and `wp_log_R` fixtures as tests that run in CI: data preparation 0.0 diff, objective at Python's MAP within 1e-9, Python's parameters through the Rust predict path within 1e-10, fitted objective no worse than Python's + 0.5, future forecast inside Prophet's own Newton-vs-L-BFGS band, components reconstructing `yhat`, and 80 % band widths within 2 % of Python's.
  3. The NeuralProphet port reaches 365-day-ahead holdout MAE ≤ 0.47 on Peyton with the lag-free model (Python NeuralProphet 0.9.0: 0.461) and beats the naive one-step baseline with `n_lags = 30`, using a graph-connected Huber loss and data preparation that matches the committed NeuralProphet oracle fixture.
  4. A user can call the same `forecast` shape on `aprender-mcp-chronos` with Chronos-Bolt-tiny f16 weights embedded in the binary: the nine native quantiles match the Python `chronos-forecasting` 2.3.1 oracle within 2 % of the series std through the server (1e-6 absolute with f32 weights), a 2 048-point context forecasts in under 100 ms, `horizon > 64` is refused unless `allow_long_horizon: true` and then carries a warning, the release binary is under 30 MB, and cold start to first forecast over stdio is under 150 ms.
  5. Eight concurrent requests to the Prophet/NeuralProphet streamable-HTTP server return responses bit-identical to their sequential results in less than half the sequential wall time (router pool), and every gate is green: both servers' e2e tests, the workspace lib tests, `cargo clippy -- -D warnings` on the new crates, `cargo fmt --all -- --check`, and `pv validate` on every new contract.

**Branch base**: continues on `gsd/phase-2-contract-gate` per the 02-01 policy. The NEON GEMM
kernel (spike 008) is already in this tree and on `perf/neon-gemm-8x6-microkernel` in the
`aprender-neon-upstream` worktree; opening that upstream PR is a human checkpoint, not a Phase 6
task.

**Plans**: 3/9 plans executed in 7 waves (planned 2026-09-05, revised twice after plan-check, then revised again 2026-09-05 after cross-AI review — 06-06 declares its `crate::chronos` dependency on 06-05 and moves to wave 3, and the root-manifest profile decision moves into 06-01 so no wave-2 plan edits the `Cargo.toml` its siblings read; tracer-first — 06-01 proves one Prophet path end-to-end before any expansion; three `autonomous: false` plans carry the phase's human decisions: the FALSIFY-MONO-011 `[[bin]]` allowlist treatment (06-02, wave 1), the D-13 memory-clause amendment that D-14's `dot8` single-row routing forces (06-05, wave 3 — D-13 and D-14 cannot both hold as written, and a locked CONTEXT decision is not the planner's to rewrite alone; recorded as `D-ITEM-06-08` and grep-gated in 06-09) and the `.github/workflows/ci.yml` embedded-weights leg (06-08, wave 6). Four contracts, not three: `forecast-tool-boundary`, `prophet-parity`, `neuralprophet-parity`, `chronos-bolt-parity` — NeuralProphet's SC3 bars get their own file rather than riding in the Prophet contract. Fixtures are copied in-crate; Lambda wrappers deferred; `SmoothL1Loss` filed as a core ticket.

**Cross-AI review pass (codex + gemini, 06-REVIEWS.md at plans commit `88da44dcd`)**: six findings incorporated — the NeuralProphet port moved out of the tracer into 06-04 (the one structural change; 06-06 gains it as a dependency); the Chronos "only transposed weights" truth restated to match the code being ported, with a test that proves single rows actually reach `dot8`; an architecture-keyed provisional f32 bar plus a `measure-x86-first` checkpoint option, because every CI job is x86_64 and the 1e-6 bar was measured on aarch64 with 4.6 % margin; `just fetch-chronos-tiny` made verify-always with a tamper control, and `chronos-gate` now calls it unconditionally; the pool speed-up assertion moved out of the unit test into the host-gated `forecast-pool-ratio` benchmark; Wave 2 sequential execution made an enforced precondition. Two of the reviewers' most emphatic findings — `[profile.dev.package.X]` not reaching test builds, and `cargo test -- a b` dropping the second filter — were REFUTED by experiment and are recorded as rejected in the affected plans so they cannot be re-raised as new.)

Plans:
**Wave 1** *(no file overlap; run one at a time on this shared branch — export `CARGO_INCREMENTAL=0`, the disk is at 94 %)*

- [x] 06-01-PLAN.md — TRACER (T1): `aprender-forecast` (dates with a strict `parse_date`, types, fit, forecast door, Prophet port) + `aprender-mcp-forecast` (pmcp server, stdio + streamable-HTTP, one e2e happy path) + root members; T2: 17 fixtures copied byte-verified, Peyton rung-2 parity test, both crate READMEs, the MEASURED `[profile.dev.package.aprender-forecast]` decision (taken in wave 1 so wave 2 reads a stable root manifest). `model: neuralprophet` is a declared refusing stub filled by 06-04 — the np.rs port left the tracer in the `--reviews` replan so the tracer is one honest end-to-end slice (wave 1)
- [x] 06-02-PLAN.md — HUMAN DECISION (blocking): FALSIFY-MONO-011 `[[bin]]` allowlist treatment for the six thin MCP deployment units, applied with a two-sided control; the three README drift fixes (two missing READMEs, the setfit monorepo link) (wave 1, NOT autonomous)

**Wave 2** *(blocked on 06-01; two plans with zero `files_modified` overlap that MUST be RUN ONE AT A TIME, not concurrently — they share one Cargo target directory, one `target/.package-cache` build lock and one disk at 94 % after three ENOSPC halts; each carries a Task-1 precondition that halts if another cargo build is running. Sequential execution is the instruction, not a recommendation — both cross-AI reviewers, 2026-09-05. 06-05 left this wave entirely: it is Wave 3 on a declared 06-04 dependency, because both plans append to the same `pub mod` block in `crates/aprender-forecast/src/lib.rs`)*

- [x] 06-03-PLAN.md — `contracts/prophet-parity-v1.yaml` + the full seven-fixture Prophet ladder as `--lib` tests reading the contract + the warm debug ladder wall recorded under the profile 06-01 decided — no manifest edit (wave 2)
- [ ] 06-04-PLAN.md — the np.rs port + the real `model: neuralprophet` arm and its refusals (moved here from 06-01) + `contracts/neuralprophet-parity-v1.yaml` + NP parity (oracle data prep, lag-free MAE ≤ 0.47, AR-Net beats naive, Huber connectivity, mini-batch/tape invariants) (wave 2)

**Wave 3** *(blocked on 06-01 and 06-04 — 06-05 appends `bolt`/`safetensors`/`chronos` to the same `pub mod` block in `crates/aprender-forecast/src/lib.rs` that 06-04 adds `np` to, and a shared file is a real dependency, not a scheduling preference; NOT autonomous — human checkpoint on the D-13 memory clause)*

- [ ] 06-05-PLAN.md — `just fetch-chronos-tiny` (pinned revision + sha256 verified on EVERY run, with a tamper control; f16 derived and hashed) + `build.rs` cfg(chronos_weights) + bolt/safetensors/chronos-door ports (both weight layouts, with the single-row `dot8` routing proven) + `contracts/chronos-bolt-parity-v1.yaml` (arch-keyed f32 bar) + gated Bolt ladder with the two-sided counted-skip proof (wave 3, NOT autonomous: a blocking human decision ratifies the D-13 memory-clause amendment before `bolt.rs` is ported)

**Wave 4** *(blocked on 06-01, 06-04 and 06-05 — `types::tests::chronos_bounds_match_contract` reads `crate::chronos`, and the NP happy-path e2e needs the `neuralprophet` arm 06-04 now lands)*

- [ ] 06-06-PLAN.md — `contracts/forecast-tool-boundary-v1.yaml` + the complete D-11 refusal e2e set + strict schema + router pool (`pooled_app`, `--pool 8`) + equality-under-load + spawned-binary stdio e2e (wave 4)

**Wave 5** *(blocked on 06-05 and 06-06)*

- [ ] 06-07-PLAN.md — `aprender-mcp-chronos`: embedded-weights build.rs, resolve_model, server, page, `--coldstart`/`--bench`, gated e2e through the server (1e-6 f32 / 2 % f16, `allow_long_horizon` + warning, forwards 46), shared-shape invariant, embedded-build proof (wave 5)

**Wave 6** *(blocked on 06-06 and 06-07; NOT autonomous — human checkpoint on ci.yml)*

- [ ] 06-08-PLAN.md — host-gated `just` recipes (chronos-gate, embed-build, bench, coldstart, forecast-bench, pool-ratio, mase-rolling-origin) + the spike-006 MASE example + `06-EVIDENCE.md` measured on aarch64 release + the ci.yml proposal as a patch → blocking human decision (wave 6, NOT autonomous)

**Wave 7** *(blocked on everything)*

- [ ] 06-09-PLAN.md — Makefile `$(CONTRACTS)`/`PHASE6_CONTRACTS`/`contract-audit-phase6` (tier3) + binding rows (zero BIND-) + README counts re-derived + CLAUDE.md Realizar-first exception row (D-07) + both drift gates green + the CI decision applied + `deferred-items.md` + SmoothL1Loss ticket + closing clippy/fmt/check/nextest sweep (wave 7)

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5; Phase 6 is an independent track that may run alongside Phase 5's remaining GPU waves

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Differentiable MiniLM Conformance | 9/9 | Complete   | 2026-08-08 |
| 2. Deterministic Pair and Data Protocol | 9/9 | Complete   | 2026-08-09 |
| 3. Faithful Two-Stage Trainer and Head | 10/10 | Complete   | 2026-08-14 |
| 4. APR Artifact and Production Parity | 22/22 | UAT passed, awaiting secure-phase |  |
| 5. Benchmark and Claims Gate | 11/14 | In Progress|  |
| 6. Native Time-Series Forecasting Stack | 3/9 | In Progress|  |
