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
- [ ] **Phase 2: Deterministic Pair and Data Protocol** - Make few-shot selection and bounded pair generation reproducible, provenance-complete, leakage-safe, and non-quadratic.
- [ ] **Phase 3: Faithful Two-Stage Trainer and Head** - Deliver an auditable encoder-tuning then unique-row classifier-fitting lifecycle that alone may identify as SetFit.
- [ ] **Phase 4: APR Artifact and Production Parity** - Persist, reload, inspect, predict, evaluate, and serve the exact verified model through shared CPU-first APIs.
- [ ] **Phase 5: Benchmark and Claims Gate** - Produce the complete reproducible 40-cell SetFit-versus-9B-LoRA evidence set and reject incomplete or unequal claims.

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

- [ ] 02-03-PLAN.md — Bytes→typed layer: schema, dual hashes, typestate splits, coalesced dedup, persistable ledger, PreparedDataset<Canonical|Compatibility> profile typestate (wave 3)
- [ ] 02-04-PLAN.md — Measured + contracted SetFit pair-count fixture families (incl. K=N adversarial layout) + shared test models + integrity verifier (wave 3)

**Wave 4** *(blocked on Wave 3 completion)*

- [ ] 02-05-PLAN.md — Philox few-shot selection (frozen LE encoding, labeled SelectedExample) + non-circular manifest payload with persisted ledger + strict Selection::replay + goldens (wave 4)
- [ ] 02-06-PLAN.md — data_tweeteval thin-adapter relocation on the D-05 seam + dataset-attestation ingest boundary + real-duplicate golden + contract growth (wave 4)

**Wave 5** *(blocked on Wave 4 completion)*

- [ ] 02-07-PLAN.md — Pair protocol: fallible capacity, binding hard cap, total degenerate policy, O(K) streaming sampler, public state diagnostics, untrusted-pair DTO, tuple-committing replay hash (wave 5)

**Wave 6** *(blocked on Wave 5 completion)*

- [ ] 02-08-PLAN.md — Honesty gates: in-band leaky/materializing negatives (incl. K=N), trybuild non-constructibility, binding-coverage audit gate, bounded scoped mutation (wave 6)
- [ ] 02-09-PLAN.md — `apr data select` / `apr data pairs` CLI surface: attested ingest, required contracted seed, atomic writes, strict replay + documented workflow (wave 6)

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

**Plans**: TBD

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

**Plans**: TBD

### Phase 5: Benchmark and Claims Gate

**Goal**: Users can audit and recompute a complete, selection-safe TweetEval comparison between the
verified SetFit APR and the existing 9B LoRA path across every contracted shot and seed.
**Depends on**: Phase 4
**Requirements**: EVAL-01, EVAL-02, EVAL-03, EVAL-04, EVAL-05
**Success Criteria** (what must be TRUE):

  1. A user can evaluate ordered predictions with the official `F_avg = (F1_against + F1_favor) / 2`, per-class metrics, three-class macro-F1, MCC, confusion matrix, and validation-only calibration diagnostics bound to explicit ordered labels.
  2. A user can run all 40 shot/seed cells for both SetFit and 9B LoRA—shots `{8,16,32,64}` crossed with the ten contracted seeds—with an identical sampled-ID hash for both methods in every cell.
  3. A user receives one machine-readable row per method/shot/seed run containing dataset/model revisions, selection lock, artifact hash, encoder-update evidence, backend/hardware identity, quality metrics, and consistently bounded resource measurements.
  4. A user can exactly recompute headline means, dispersion, uncertainty, and paired SetFit-versus-LoRA deltas from all stored rows, while any missing, selectively omitted, unmatched, or post-test-selected cell invalidates the report.
  5. A user can compare training time, cold/warm latency, throughput with batch/warmup boundaries, peak memory, artifact size, calibration, and classification quality measured from the same reloaded production artifacts.

**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Differentiable MiniLM Conformance | 9/9 | Complete   | 2026-08-08 |
| 2. Deterministic Pair and Data Protocol | 2/9 | In Progress|  |
| 3. Faithful Two-Stage Trainer and Head | 0/TBD | Not started | - |
| 4. APR Artifact and Production Parity | 0/TBD | Not started | - |
| 5. Benchmark and Claims Gate | 0/TBD | Not started | - |
