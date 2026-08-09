# Requirements: Aprender Native SetFit Classification

**Defined:** 2026-08-07
**Core Value:** A small labeled dataset can produce an accurate, fast, reproducible classifier
that trains and runs entirely through Aprender's native Rust and APR lifecycle.

## v1 Requirements

### Encoder Conformance

- [ ] **ENC-01**: A developer can import the pinned
  `sentence-transformers/all-MiniLM-L6-v2` revision into a typed SetFit encoder contract, while
  unsupported architecture, tokenizer, pooling, and configuration variants fail with typed errors

- [ ] **ENC-02**: A developer can batch-tokenize texts once and obtain ordered token IDs, type IDs,
  attention masks, truncation facts, and stable input provenance for training and inference

- [ ] **ENC-03**: A developer can encode single or mixed-length padded batches through one shared
  `Transformer -> masked mean pooling -> L2 normalization` path with fixture-verified outputs

- [ ] **ENC-04**: A developer can enumerate named encoder parameters, select frozen and trainable
  groups, and observe finite non-zero gradients and parameter changes for every contracted trainable
  component after a controlled optimizer step

- [ ] **ENC-05**: A developer can switch the encoder recursively between deterministic evaluation
  behavior and training behavior with dropout, without changing which parameters are registered

- [ ] **ENC-06**: A developer can compute a finite tensor-valued cosine-similarity MSE pair loss
  that remains connected to the encoder graph and matches frozen forward and gradient fixtures

### Data and Pairing

- [x] **DATA-01**: A user can acquire or mirror the pinned TweetEval abortion-stance dataset and
  produce canonical train, validation, and test JSONL with exact labels, counts, hashes, and source
  provenance without committing tweet text

- [x] **DATA-02**: A user receives a typed failure for malformed rows, duplicate IDs, unknown labels,
  invalid class counts, conflicting source roles, any pair or selection that would span splits, and
  a training pool that can no longer supply `shots_per_class` after cross-split exclusion.
  Prepare-time cross-split duplicate *content* is excluded from the training pool and recorded, not
  fatal — see D-27 (resolves the DATA-02 / D-18 conflict; `pv`-checked via the exclusion record)

- [x] **DATA-03**: A user can select exactly 8, 16, 32, or 64 unique canonical-training examples
  per class using each contracted seed and receive a stable selected-ID manifest

- [ ] **DATA-04**: A user can replay deterministic positive and negative pair generation where
  positive labels match, negative labels differ, endpoints differ, unordered identities are
  canonical, and singleton-class behavior is explicit

- [ ] **DATA-05**: A user can set a maximum pair budget per epoch and pair generation remains
  `O(examples + pair budget)` in state and storage rather than materializing a Cartesian product

- [x] **DATA-06**: A user cannot use validation/test examples as training pairs or use the merged
  SetFit compatibility test split for model selection without an explicit fail-closed error

### Training and Classifier

- [ ] **TRN-01**: A developer can run a typed SetFit lifecycle whose legal stages are
  `Prepared -> EncoderTuned -> HeadFitted -> ArtifactReloadedAndVerified`

- [ ] **TRN-02**: A user can configure and validate encoder learning rate, epochs, batch size,
  warmup, gradient clipping, maximum length, pair policy/budget, freeze policy, head regularization,
  root seed, and device before training begins

- [ ] **TRN-03**: A user receives proof that named encoder gradients, parameter deltas, embedding
  deltas, and pair-loss behavior passed before a run may identify itself as SetFit or export a model

- [ ] **TRN-04**: A user can fit one deterministic L2-regularized multinomial softmax classifier for
  any ordered label set with `K >= 2`, with finite logits/probabilities and explicit convergence or
  failure

- [ ] **TRN-05**: The classifier head is fit exactly once per unique selected training example using
  the tuned encoder in evaluation/no-gradient mode, so pair multiplicity cannot reweight the head
  dataset

- [ ] **TRN-06**: Two clean CPU runs with identical inputs reproduce selected IDs, pair ordering,
  batch ordering, training step count, semantic hashes, predictions, and the declared deterministic
  portions of the loss trace

- [ ] **TRN-07**: A user can select configurations and checkpoints using canonical validation only,
  and a selection-lock record is created before canonical test access is permitted

### APR Artifact Lifecycle

- [ ] **APR-01**: A user can save one checksummed F32 `setfit-apr-v1` artifact containing all encoder
  tensors, exact tokenizer bytes/hash, pooling/normalization/truncation policy, classifier tensors,
  ordered labels, resolved configuration, training evidence, and data/model provenance

- [ ] **APR-02**: A user can load the SetFit APR offline without Python, a model hub, or sidecar
  files, and malformed, incomplete, oversized, non-finite, or semantically inconsistent artifacts
  fail before prediction

- [ ] **APR-03**: Training closes the in-memory model, reloads the written APR through the production
  core loader, and verifies exact tokenizer/configuration/tensor state plus tolerance-bounded
  embeddings, logits, and probabilities and exact labels

- [ ] **APR-04**: Evaluation, registration, benchmarking, prediction, and serving accept only an
  `ArtifactReloadedAndVerified` model, never an unpersisted trainer object or training checkpoint

- [ ] **APR-05**: A user can inspect an APR and recover encoder/tokenizer revision and hashes,
  pooling/truncation policy, label order, head configuration, data fingerprint, seeds, update
  evidence, artifact hash, and compatibility schema version

### Rust, CLI, and Serving Interfaces

- [ ] **OPS-01**: A Rust caller can train, save, load, embed, classify, and inspect a SetFit model
  through stable fallible library APIs without depending on CLI implementation modules

- [ ] **OPS-02**: A user can complete a CPU `train -> APR -> inspect -> eval -> predict` lifecycle
  through `apr` with structured errors and machine-readable JSON output

- [ ] **OPS-03**: Generic APR inspection, evaluation, and prediction commands auto-detect the SetFit
  architecture and call the shared core model rather than reconstructing tokenizer or pooling logic

- [ ] **OPS-04**: A user can classify one or many texts through the Rust API and CLI and receive
  ordered labels, full probability vectors, optional logits, winning margin, token/truncation facts,
  artifact identity, backend identity, and latency

- [ ] **OPS-05**: A user can load the same APR into native HTTP serving and submit ordered mixed-
  length batches while readiness and responses report the loaded classifier artifact hash

- [ ] **OPS-06**: CPU training and inference work without accelerator features, and an unavailable
  explicitly requested device fails rather than silently falling back or misreporting the backend

### Evaluation and Benchmark Claims

- [ ] **EVAL-01**: A user can evaluate ordered single-label predictions with TweetEval's official
  `F_avg`, per-class metrics, three-class macro-F1, MCC, confusion matrix, and validation-only
  calibration diagnostics

- [ ] **EVAL-02**: A user can run every combination of 8, 16, 32, and 64 shots per class with the ten
  contracted seeds for both SetFit and the 9B LoRA baseline using an identical sampled-ID hash in
  each comparison cell

- [ ] **EVAL-03**: A user receives one machine-readable row per method/shot/seed run containing
  dataset/model revisions, selection lock, artifact hash, encoder-update evidence, backend/hardware,
  quality metrics, and resource metrics

- [ ] **EVAL-04**: A user can recompute headline means, dispersion, uncertainty, and paired SetFit-
  versus-LoRA deltas exactly from all 40 stored comparison cells, and missing or selectively omitted
  cells invalidate the report

- [ ] **EVAL-05**: A user can compare training time, warm/cold prediction latency, throughput with
  batch/warmup boundaries, peak memory, artifact size, calibration, and classification quality from
  the reloaded production artifacts

### Contracts and Safety

- [ ] **SAFE-01**: A developer can run executable contracts and numerical fixtures that detect
  detached gradients, invalid masks/IDs/labels/classes, pair/split leakage, non-finite math, label
  drift, artifact mismatches, and train/CLI/HTTP parity failures

- [ ] **SAFE-02**: A developer can verify the supported CPU build/test feature matrix in CI without
  Python or network access, while reference-fixture generation remains a separate pinned developer
  workflow

- [ ] **SAFE-03**: A user cannot label a frozen linear probe, centroid classifier, or other
  non-updating encoder baseline as SetFit in artifacts, reports, or benchmark output

## v2 Requirements

### Encoder and Objective Expansion

- **EXT-01**: A developer can add another sentence-transformer family through a named adapter and
  architecture-specific parity corpus

- **EXT-02**: A user can select alternative contracted encoder objectives such as InfoNCE, SupCon,
  CoSENT, or triplet loss

### Optimization and Deployment

- **ACC-01**: A user can train and infer with optional accelerator backends that pass declared
  numerical-parity, feature-combination, determinism, and backend-reporting contracts

- **QUANT-01**: A user can derive a quantized SetFit artifact with independent quality, calibration,
  parity, and performance evidence

- **CACHE-01**: A user can reuse token or embedding caches addressed by the complete dataset,
  tokenizer, encoder, pooling, normalization, and configuration fingerprint

### Task and Explanation Expansion

- **TASK-01**: A user can train multilabel or hierarchical SetFit classifiers with task-specific
  losses, metrics, APIs, and artifact semantics

- **EXPL-01**: A user can request separately validated token attribution, counterfactual, or exemplar
  explanations with explicit fidelity and privacy contracts

## Out of Scope

| Feature | Reason |
|---------|--------|
| Frozen probe or centroid presented as SetFit | It omits the defining contrastive encoder update |
| Python/PyTorch/ONNX production fallback | It violates the pure-Rust, offline, single-runtime lifecycle |
| Exhaustive Cartesian pair materialization | It has quadratic memory behavior and implicit weighting |
| Hyperparameter tuning on canonical test or merged compatibility test | It leaks evaluation data and invalidates claims |
| Generic arbitrary Hugging Face/remote-code compatibility | V1 supports one fail-closed contracted encoder family |
| Multiple production heads | One multinomial head keeps binary/multiclass semantics, persistence, and probabilities uniform |
| Multilabel, hierarchical, token/span, or generative classification in v1 | These require different targets, metrics, APIs, and artifacts |
| Quantization before F32 parity | Reduced precision would obscure algorithm and serialization discrepancies |
| GPU-required v1 | Few-shot CPU use is core; accelerators follow complete CPU lifecycle proof |
| Persistent embedding cache in v1 | Stale embeddings can silently survive encoder or preprocessing changes |
| Causal token-level explanation claims in v1 | Score evidence is useful; causal attribution needs separate validation |
| Automated search, continual/distributed training, or registry automation | These broaden orchestration before the core lifecycle is trustworthy |
| Vendored TweetEval text | On-demand pinned acquisition avoids licensing and provenance risk |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| ENC-01 | Phase 1 | Pending |
| ENC-02 | Phase 1 | Pending |
| ENC-03 | Phase 1 | Pending |
| ENC-04 | Phase 1 | Pending |
| ENC-05 | Phase 1 | Pending |
| ENC-06 | Phase 1 | Pending |
| DATA-01 | Phase 2 | Complete |
| DATA-02 | Phase 2 | Complete |
| DATA-03 | Phase 2 | Complete |
| DATA-04 | Phase 2 | Pending |
| DATA-05 | Phase 2 | Pending |
| DATA-06 | Phase 2 | Complete |
| TRN-01 | Phase 3 | Pending |
| TRN-02 | Phase 3 | Pending |
| TRN-03 | Phase 3 | Pending |
| TRN-04 | Phase 3 | Pending |
| TRN-05 | Phase 3 | Pending |
| TRN-06 | Phase 3 | Pending |
| TRN-07 | Phase 3 | Pending |
| APR-01 | Phase 4 | Pending |
| APR-02 | Phase 4 | Pending |
| APR-03 | Phase 4 | Pending |
| APR-04 | Phase 4 | Pending |
| APR-05 | Phase 4 | Pending |
| OPS-01 | Phase 4 | Pending |
| OPS-02 | Phase 4 | Pending |
| OPS-03 | Phase 4 | Pending |
| OPS-04 | Phase 4 | Pending |
| OPS-05 | Phase 4 | Pending |
| OPS-06 | Phase 4 | Pending |
| EVAL-01 | Phase 5 | Pending |
| EVAL-02 | Phase 5 | Pending |
| EVAL-03 | Phase 5 | Pending |
| EVAL-04 | Phase 5 | Pending |
| EVAL-05 | Phase 5 | Pending |
| SAFE-01 | Phase 4 | Pending |
| SAFE-02 | Phase 4 | Pending |
| SAFE-03 | Phase 3 | Pending |

**Coverage:**

- v1 requirements: 38 total
- Mapped to phases: 38
- Unmapped: 0

---
*Requirements defined: 2026-08-07*
*Last updated: 2026-08-07 after roadmap creation*
