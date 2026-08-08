# Phase 1: Differentiable MiniLM Conformance - Pattern Map

**Mapped:** 2026-08-07
**Files analyzed:** 22 new/modified files
**Analogs found:** 20 / 22 (2 with no in-repo analog: uv fixture generator, tokenizer-adapter is partial)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/aprender-core/src/autograd/ops/embedding.rs` (new) | autograd op | gather/transform | `crates/aprender-core/src/models/qwen2/mod.rs:148-165` + `autograd/grad_fn.rs:1100-1130` | exact |
| `crates/aprender-core/src/autograd/ops/masking.rs` (new) | autograd op (constant builder) | transform | `crates/aprender-core/src/nn/dropout/mod.rs:110-135` (constant-mask + `mul`) | exact |
| `crates/aprender-core/src/autograd/ops/pooling.rs` (new) | autograd op | reduction | `crates/aprender-core/src/autograd/ops/mod.rs:339-361` (`mean`) + `grad_fn.rs` reduction backwards | exact |
| `crates/aprender-core/src/autograd/ops/normalize.rs` (new) | autograd op | rowwise transform | `crates/aprender-core/src/autograd/ops/mod.rs:24-50` (`add` shape) + `grad_fn.rs:1146+` (`SoftmaxLastDimBackward` rowwise loop) | exact |
| `crates/aprender-core/src/autograd/ops/similarity.rs` (new) | autograd op ×2 (cosine, MSE) | reduction | same op pattern; backward rowwise loop per `SoftmaxLastDimBackward` | exact |
| `crates/aprender-core/src/autograd/grad_fn.rs` (modify: 4 new `*Backward` structs) | autograd backward | transform | `EmbeddingBackward` at `grad_fn.rs:1100-1130` | exact |
| `crates/aprender-core/src/autograd/ops/tests_*_backward.rs` (new, 6 files) | test (gradcheck) | transform | `crates/aprender-core/src/nn/conv/tests_pool_flatten_backward_gradflow.rs` | exact |
| `crates/aprender-core/src/nn/module.rs` (modify: named traversal + `set_training`) | trait | n/a | itself (`module.rs:31-96`) + leaf impl `nn/linear.rs:285-303` | exact |
| `crates/aprender-core/src/nn/{linear.rs, normalization/, dropout/mod.rs, container.rs}` (modify: named impls) | module impls | n/a | `nn/linear.rs:285-297` (`parameters`/`parameters_mut` ordering) | exact |
| `crates/aprender-core/src/setfit/mod.rs` (new) | facade (bound model type) | request-response | `models/bert/` module layout (config/load/encoder split) | role-match |
| `crates/aprender-core/src/setfit/error.rs` (new) | error type | n/a | `models/bert/load.rs:94-109` (`BertLoadError`) | exact |
| `crates/aprender-core/src/setfit/import.rs` (new) | service (loader/validator) | file-I/O | `models/bert/load.rs:114-199` (`read_tensor`, `detect_bert_prefix`, `load_embeddings_from_reader`) | exact |
| `crates/aprender-core/src/setfit/tokenizer.rs` (new) | adapter (HF tokenizers wrapper) | batch transform | `crates/aprender-bench-tokenizer/src/lib.rs` (only in-repo HF `tokenizers` consumer) | partial |
| `crates/aprender-core/src/setfit/encoder.rs` (new) | model/module | batch forward | `models/bert/layer.rs:78` (structure ONLY) + `nn/transformer/attention_gqa.rs:373-376` + `nn/dropout/mod.rs:110-135` | role-match (must diverge: graph-connected, typed errors) |
| `crates/aprender-core/src/setfit/loss.rs` (new) | op composition | reduction | `autograd/ops/mod.rs` op pattern; ANTI-analog: `nn/loss.rs` / `nn/self_supervised.rs` (f32-returning — do NOT reuse) | role-match |
| `crates/aprender-core/tests/setfit_conformance/` (new) | integration test (fixture parity) | file-I/O | `tests/falsification_spec_v10_tests.rs:1-17` (feature gating) + `tests/contracts/*.rs` naming | role-match |
| `crates/aprender-core/tests/fixtures/setfit/` (new: JSON + slice APR + SHA-256 manifest) | fixture data | n/a | no direct precedent; `aprender-bench-tokenizer/src/lib.rs:40-61` shows env-override + repo-relative resolution | partial |
| `contracts/setfit-encoder-conformance-v1.yaml` (new) | contract | n/a | `contracts/encoder-forward-v1.yaml` (schema shape) + `contracts/lora-adapter-trains-base-frozen-v1.yaml` (frozen-vs-trainable + detach-negative style) | exact |
| `crates/aprender-core/src/generated_contracts.rs` (regenerate) | generated | n/a | itself — `pv codegen contracts/ -o src/generated_contracts.rs` (file header, lines 1-4) | exact |
| `crates/aprender-core/Cargo.toml` (modify: `setfit` feature) | config | n/a | existing feature lines, e.g. `audio = ["rustfft", "thiserror"]`, `model-tests = []` (~line 197-211) | exact |
| Root `Cargo.toml` (modify: workspace `tokenizers` dep) | config | n/a | existing `[workspace.dependencies]` entries | exact |
| `Makefile` (modify: tier3 `pv validate` for new contract) | config | n/a | `Makefile:185-197` (tier2), `Makefile:801` (`PV_BIN`), `Makefile:849` (validate loop) | exact |
| `scripts/setfit_fixtures/` (new: uv project + Python generator) | script | batch | **no analog** (no pyproject/uv.lock anywhere under `scripts/`) | none |

## Pattern Assignments

### The six autograd ops: `autograd/ops/{embedding,masking,pooling,normalize,similarity}.rs` (op, transform)

**Analog:** `crates/aprender-core/src/autograd/ops/mod.rs`

**Imports pattern** (`ops/mod.rs:9-18`):
```rust
use std::sync::Arc;

use super::grad_fn::{
    AbsBackward, AddBackward, /* ... */ SumBackward, TanhBackward, TransposeBackward, ViewBackward,
};
use super::tensor::Tensor;
use super::{is_grad_enabled, with_graph};
```

**Core pattern — forward + GradFn + graph record** (`ops/mod.rs:27-50`, the `add` op; every op in the file follows this exact 3-step shape):
```rust
#[must_use]
pub fn add(&self, other: &Tensor) -> Tensor {
    // 1. compute forward
    let data = trueno::blis::elementwise::add_alloc(self.data(), other.data());
    let mut result = Tensor::from_vec(data, self.shape());

    // 2+3. record to graph if needed
    if is_grad_enabled() && (self.requires_grad_enabled() || other.requires_grad_enabled()) {
        result.requires_grad_(true);
        let grad_fn = Arc::new(AddBackward {
            x_shape: self.shape().to_vec(),
            y_shape: other.shape().to_vec(),
        });
        result.set_grad_fn(grad_fn.clone());

        with_graph(|graph| {
            graph.register_tensor(self.clone());
            graph.register_tensor(other.clone());
            graph.record(result.id(), grad_fn, vec![self.id(), other.id()]);
        });
    }
    result
}
```
Single-input variant (drop the second `register_tensor`, single-element input vec): see `neg` at `ops/mod.rs:147-169`. Reduction-to-scalar variant (result shape `[1]`): see `mean` at `ops/mod.rs:341-360` — the direct template for `mse_loss`.

**Divergences required by CONTEXT decisions:** the new ops return `Result<Tensor, TypedError>` (D-03 checked denominator, OOV fail-closed) instead of `#[must_use] -> Tensor`; keep the graph-record block byte-similar. Wire new files from `ops/mod.rs` — note existing style uses `include!("activation.rs")` at `ops/mod.rs:363`; either `include!` or `mod` is acceptable, follow the planner's choice consistently.

**Backward struct pattern** (`autograd/grad_fn.rs:23-38` trait + `grad_fn.rs:1100-1130` `EmbeddingBackward`):
```rust
pub trait GradFn: Send + Sync {
    fn backward(&self, grad_output: &Tensor) -> Vec<Tensor>;
    fn name(&self) -> &'static str;
}

pub(crate) struct EmbeddingBackward {
    pub(crate) indices: Vec<u32>,
    pub(crate) vocab_size: usize,
    pub(crate) hidden_size: usize,
}

impl GradFn for EmbeddingBackward {
    fn backward(&self, grad_output: &Tensor) -> Vec<Tensor> {
        let g = grad_output.data();
        let h = self.hidden_size;
        let mut grad_w = vec![0.0f32; self.vocab_size * h];
        for (i, &tok) in self.indices.iter().enumerate() {
            let row = tok as usize;
            if row >= self.vocab_size { continue; }
            let g_off = i * h;
            let w_off = row * h;
            for j in 0..h {
                grad_w[w_off + j] += g[g_off + j]; // scatter-ADD, never overwrite
            }
        }
        vec![Tensor::new(&grad_w, &[self.vocab_size, h])]
    }
    fn name(&self) -> &'static str { "EmbeddingBackward" }
}
```
New structs (`MaskedMeanPoolBackward`, `L2NormalizeRowsBackward`, `CosineSimilarityBackward`, `MseBackward`) go in `grad_fn.rs` next to their kin, `pub(crate)`, name ending in `Backward` (enforced by `test_all_backward_names` in `autograd/tests_matmul_backward.rs:101-184` — add the new names there). For rowwise backward loops (L2 norm, cosine) copy the rows×features iteration of `SoftmaxLastDimBackward` (`grad_fn.rs:1146-1170`).

---

### `autograd/ops/embedding.rs` — batched gather specifically (op, gather)

**Analog:** `crates/aprender-core/src/models/qwen2/mod.rs:148-165` (`record_embedding_backward`)

```rust
fn record_embedding_backward(&self, input_ids: &[u32], result: &mut Tensor) {
    use crate::autograd::{is_grad_enabled, with_graph};
    use std::sync::Arc;

    if is_grad_enabled() && self.weight.requires_grad_enabled() {
        result.requires_grad_(true);
        let grad_fn = Arc::new(crate::autograd::grad_fn::EmbeddingBackward {
            indices: input_ids.to_vec(),   // flatten [B,S] -> B*S for the batched op
            vocab_size: self.vocab_size,
            hidden_size: self.hidden_size,
        });
        result.set_grad_fn(grad_fn.clone());
        with_graph(|graph| {
            graph.register_tensor(self.weight.clone());
            graph.record(result.id(), grad_fn, vec![self.weight.id()]);
        });
    }
}
```
**Reuse `EmbeddingBackward` as-is** (it is index-list based, hence batch-agnostic).
**Required divergence:** qwen2's forward does an OOB "escape" — warns and emits zeros (`qwen2/mod.rs:100-108`, N-09). The new op must instead **fail closed with a typed error on OOV ids** (CONTEXT known trap; also do NOT copy `aprender-train/src/transformer/embedding.rs` zero-fill).

---

### `autograd/ops/tests_*_backward.rs` — six finite-difference tests (test, transform)

**Analog:** `crates/aprender-core/src/nn/conv/tests_pool_flatten_backward_gradflow.rs` (175 lines — copy the whole harness shape)

**Constants + gradcheck driver** (lines 26-27, 74-104):
```rust
const FD_EPS: f32 = 1e-3;
const TOL: f32 = 2e-2;

fn gradcheck_input<F>(name: &str, x_data: &[f32], x_shape: &[usize], fwd: F)
where F: Fn(&Tensor) -> Tensor {
    autograd::clear_graph();
    let x = Tensor::new(x_data, x_shape).requires_grad();
    let xid = x.id();
    let y = fwd(&x);
    let c = coeff(y.numel());
    let loss = scalar_loss(&y, &c);
    loss.backward();

    let grad = autograd::get_grad(xid)
        .unwrap_or_else(|| panic!("{name}: input received NO gradient — autograd graph severed"));
    assert_eq!(grad.shape(), x_shape, "{name}: grad shape mismatch");
    assert!(grad.data().iter().all(|v| v.is_finite()), "{name}: non-finite grad");
    assert!(grad.data().iter().any(|&v| v.abs() > 1e-9), "{name}: all-zero grad");

    for i in 0..x_data.len() {
        let num = (perturbed_loss(x_data, x_shape, i, FD_EPS, &fwd, &c)
            - perturbed_loss(x_data, x_shape, i, -FD_EPS, &fwd, &c))
            / (2.0 * FD_EPS);
        assert_close(grad.data()[i], num, &format!("{name} dL/dx[{i}]"));
    }
}
```
Supporting helpers to copy: `coeff` (detached non-uniform coefficients, lines 30-32), `scalar_loss` (lines 35-38), `perturbed_loss` under `autograd::no_grad` (lines 42-60), `assert_close` relative-error (lines 62-69). The doc header pattern (lines 1-20) names the OBLIG-* obligations and the PMAT bug class — replicate for each new op.

**Test-file wiring pattern:** either `include!` from a hub file (`autograd/grad_fn_tests.rs:5-6` does `include!("tests_elementwise_backward.rs");`) or `#[cfg(test)] #[path = "..."] mod tests;` (`nn/linear.rs:315-321`). NOTE per D-04 these tests carry op-level FD tolerances only; **fixture-comparison epsilons must NOT appear in test files** — those live in the contract YAML (D-14).

---

### `nn/module.rs` — named traversal + `set_training` (trait, n/a)

**Analog:** itself — `crates/aprender-core/src/nn/module.rs:31-96`

**Existing trait surface to extend, not replace** (lines 31-52, 66-81):
```rust
pub trait Module: Send + Sync {
    fn forward(&self, input: &Tensor) -> Tensor;

    fn parameters(&self) -> Vec<&Tensor> { vec![] }
    fn parameters_mut(&mut self) -> Vec<&mut Tensor> { vec![] }

    fn train(&mut self) { /* Default: no-op */ }
    fn eval(&mut self) { /* Default: no-op */ }
    fn training(&self) -> bool { true }
}
```
New default methods (`named_parameters`, `named_parameters_mut`, `set_training`) follow the same "empty/no-op default, leaf overrides" convention. Sketch approved in RESEARCH.md Pattern 3.

**Leaf impl ordering precedent** (`nn/linear.rs:285-297`) — named order must match positional order:
```rust
fn parameters(&self) -> Vec<&Tensor> {
    match &self.bias {
        Some(b) => vec![&self.weight, b],
        None => vec![&self.weight],
    }
}
```
Named leaf names: `weight`/`bias`; composites prefix with HF dotted path (D-18, e.g. `encoder.layer.0.attention.self.query.weight`). Train/eval leaf precedent with real state: `nn/dropout/mod.rs:136-145` (`self.training = true/false`).

---

### `setfit/error.rs` + `setfit/import.rs` (error type + loader/validator, file-I/O)

**Analog:** `crates/aprender-core/src/models/bert/load.rs`

**Typed error pattern** (lines 94-109) — no thiserror, plain struct + Display + Error:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BertLoadError {
    pub tensor: String,
    pub reason: String,
}

impl std::fmt::Display for BertLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BertLoadError({}: {})", self.tensor, self.reason)
    }
}

impl std::error::Error for BertLoadError {}
```
`setfit/error.rs` defines its own enum spanning import/tokenize/forward failures and `#[from]`-wraps or converts `BertLoadError` (RESEARCH.md Q5 recommendation: wrap, don't modify `bert/`).

**Checked tensor read pattern** (lines 114-145) — every fetch validates presence, dtype, and numel before constructing a Tensor:
```rust
fn read_tensor(reader: &AprV2Reader, name: &str, expected_shape: &[usize])
    -> Result<Tensor, BertLoadError>
{
    let entry = reader.get_tensor(name).ok_or_else(|| BertLoadError {
        tensor: name.to_string(),
        reason: "tensor not present in APR file".to_string(),
    })?;
    let data = reader.get_tensor_as_f32(name).ok_or_else(|| BertLoadError {
        tensor: name.to_string(),
        reason: format!("get_tensor_as_f32 failed for dtype {:?}", entry.dtype),
    })?;
    let expected_numel: usize = expected_shape.iter().product();
    if data.len() != expected_numel {
        return Err(BertLoadError { tensor: name.to_string(), reason: format!(
            "element count mismatch: got {}, expected {} (shape {:?})",
            data.len(), expected_numel, expected_shape) });
    }
    Ok(Tensor::from_vec(data, expected_shape))
}
```
**Prefix detection precedent** (lines 147-162): `detect_bert_prefix` probes `bert.embeddings.word_embeddings.weight` — sentence-transformers checkpoints are the `""`-prefix branch. **Per-section loader shape** (lines 175-199): `load_embeddings_from_reader(embeddings, reader, config)` reads each named tensor against config-derived expected shapes. Reuse these helpers from `setfit/import.rs`; add the pinned-revision/config-equality/tokenizer-hash checks in the new module. The slice-APR test-only constructor (Pitfall 3) bypasses ONLY the architecture-pin equality, never these structural checks.

**Config validation target** (`models/bert/config.rs:57-69`): `BertConfig::minilm_l6()` is the exact pin — 384/6/12/1536/30522/512/2/1e-12/pad 0. ENC-01 equality checks compare parsed config.json against this preset.

---

### `setfit/encoder.rs` — `BertSentenceEncoder` (model/module, batch forward)

**Analogs:** structure from `models/bert/layer.rs:78` (`pub fn forward(&self, hidden: &Tensor, attn_mask: Option<&Tensor>) -> Tensor` — attn→residual→LN→FFN→residual→LN composition), attention from `nn/transformer/attention_gqa.rs:373-376`, dropout from `nn/dropout/mod.rs`.

**Attention entry point** (`attention_gqa.rs:373-376`) — already batched `[B,S,E]` with optional additive mask:
```rust
/// Self-attention: query, key, value are the same.
#[must_use]
pub fn forward_self(&self, x: &Tensor, attn_mask: Option<&Tensor>) -> (Tensor, Tensor) {
    self.forward_qkv(x, x, x, attn_mask)
}
```
`forward_qkv` internally calls `scaled_dot_product_attention(&q, &k, &v, attn_mask, self.dropout_p, self.training)` (`attention_gqa.rs:359-368`) — feed the `additive_attention_mask` op output here.

**Graph-safe masking/dropout trick — PMAT-922 pattern** (`nn/dropout/mod.rs:110-135`): build constants as tensors, apply via the autograd-aware `mul`, never bake computed values into `Tensor::new`:
```rust
// PMAT-922: mask is a non-grad CONSTANT tensor; `input.mul(&mask)` is numerically
// identical but records a MulBackward edge. The previous Tensor::new(&data, ...)
// path severed the graph and froze every parameter upstream.
let mask = Tensor::new(&mask_data, input.shape());
input.mul(&mask)
```
Seeded dropout for the RNG policy: `Dropout::with_seed(p, seed)` (`nn/dropout/mod.rs:77-88`).

**Required divergences from `models/bert/`:** (a) do NOT call `models/bert/embeddings.rs::forward` — it asserts and uses unchecked slices; use the new `embedding_gather` op + typed errors; (b) every intermediate goes through autograd-aware ops (`add`, `mul`, module forwards) — no `.data()` reads materialized into fresh tensors; (c) implement `Module` (the old BERT never did).

**Contract annotation on the forward** — copy `models/qwen2/mod.rs:121-139`:
```rust
#[provable_contracts_macros::contract("embedding-algebra-v1", equation = "embedding_lookup")]
#[must_use]
pub fn forward(&self, input_ids: &[u32]) -> Tensor {
    contract_pre_embedding_lookup!(input_ids);
    // ... compute ...
    contract_post_embedding_lookup!(result.data());
    result
}
```
For Phase 1 the contract id is `setfit-encoder-conformance-v1` with per-equation macros generated by `pv codegen contracts/ -o src/generated_contracts.rs` (regeneration command in `generated_contracts.rs:1-4` header).

---

### `setfit/tokenizer.rs` — `MiniLmTokenizer` + `SentenceBatch` (adapter, batch transform)

**Analog (partial):** `crates/aprender-bench-tokenizer/src/lib.rs` — the only in-repo consumer of HF `tokenizers`. Useful pieces: dependency form (`crates/aprender-bench-tokenizer/Cargo.toml:18` uses `tokenizers = { version = "0.22", default-features = false, features = [...] }` — Phase 1 pins `0.23.1` with only `fancy-regex` per D-05) and path-resolution style (`lib.rs:40-61`: env-var override, then repo-relative candidates, `Option`-returning).

No in-repo precedent exists for a typed `SentenceBatch`; follow the RESEARCH.md sketch (ids, type ids, attention mask, truncation facts, provenance) and validate the whole batch once at the encoder boundary with the `setfit/error.rs` enum (Pitfall 8).

---

### `setfit/loss.rs` — pair cosine-MSE (op composition, reduction)

**Analog:** thin composition over the new `cosine_similarity_rows` + `mse_loss` ops (Pattern 1 above).
**ANTI-analog — do not copy or call:** `crates/aprender-core/src/nn/loss.rs` and `nn/self_supervised.rs` return `f32`, not graph-connected tensors; reuse would recreate PF-001 (CONTEXT known trap). The scalar-loss reduction shape to copy is `mean` (`autograd/ops/mod.rs:341-360`) producing `Tensor[1]` with a recorded backward.

---

### `tests/setfit_conformance/` + controlled-step gate (integration test, file-I/O)

**Feature-gating analog** (`crates/aprender-core/tests/falsification_spec_v10_tests.rs:1-17`):
```rust
//! **GATED BY `model-tests` FEATURE** — these tests do NOT run with `cargo test`.
//! Run with: `cargo test --features model-tests --test falsification_spec_v10_tests <TEST_NAME>`
#![cfg(feature = "model-tests")]
```
Apply the same shape: the default conformance suite gates on `#![cfg(feature = "setfit")]`; the full ~90MB parity suite (D-10) additionally follows the `model-tests = []` empty-feature precedent (`crates/aprender-core/Cargo.toml:211`) — declare e.g. alongside `setfit` and mark heavy tests `#[ignore]`.

**Controlled AdamW step** (`nn/optim/mod.rs:350-356`, AdamW shares Adam's shape at :219):
```rust
/// Perform optimization step with direct tensor access.
pub fn step_with_params(&mut self, params: &mut [&mut Tensor]) {
    self.t += 1;
    for (idx, param) in params.iter_mut().enumerate() {
        self.update_param(param, idx);
    }
    self.initialized = true;
}
```
Freezing = exclusion from the `params` slice + `requires_grad(false)`; frozen byte-identity asserted via `f32::to_bits` comparison (RESEARCH.md Code Example, D-21).

**Detach-negative + frozen-proof style** — copy the falsification framing from `contracts/lora-adapter-trains-base-frozen-v1.yaml:38-66`: three guards (loss collapses; trainable params changed AND received finite non-zero grad; frozen params EXACTLY unchanged and no grad), explicitly RED-confirmed by reverting to a `Tensor::new`-severed forward. The D-24 detached encoder variant is the in-band RED twin of this pattern.

---

### `contracts/setfit-encoder-conformance-v1.yaml` (contract, n/a)

**Schema-shape analog:** `contracts/encoder-forward-v1.yaml` — sections in order: `metadata` (version/created/description/references/`depends_on` list — this is where the six referenced contracts go per D-23), `equations.{name}.formula/domain/codomain/invariants/preconditions`, `proof_obligations` (typed entries; note the `tolerance: 0.0001` field on the equivalence obligation at line 48 — the D-14 tolerance table extends this exact mechanism), `falsification_tests` (id/rule/prediction/test/if_fails), optional `kani_harnesses`, `qa_gate`.

**Falsification-test style analog:** `contracts/lora-adapter-trains-base-frozen-v1.yaml:67-79` — each entry carries a runnable `test_harness: "cargo test -p ... <test_name>"`, `expected_output`, and a diagnostic `if_fails` naming the sever class.

Validate with `pv` only: `Makefile:801` `PV_BIN := cargo run --release -p aprender-contracts-cli --bin pv --`; the existing tier gate loop at `Makefile:849` runs `$(PV_BIN) validate "$$contract"` — add the new contract there for tier3/tier4 (D-26).

---

### Build config: `Cargo.toml` feature + Makefile wiring (config, n/a)

**Feature declaration analog** (`crates/aprender-core/Cargo.toml` ~197-211):
```toml
audio = ["rustfft", "thiserror"]  # Enable audio processing (mel spectrogram, resampling)
model-tests = []  # Enable heavy model/inference tests (requires models/ dir, ollama, GPU)
```
New: `setfit = ["tokenizers"]` with `tokenizers = { workspace = true, optional = true }`; root workspace pin `tokenizers = { version = "0.23.1", default-features = false, features = ["fancy-regex"] }` (RESEARCH.md Installation block).

**Tier2 analog** (`Makefile:185-197`): `PROPTEST_CASES=5 QUICKCHECK_TESTS=5 cargo test --lib` + `cargo clippy -- -D warnings`. New lib-level tests enter tier2 automatically once the feature is in the test invocation; keep the slow gated suite out of it (D-26).

## Shared Patterns

### 1. Graph-recording (THE pattern — applies to all six ops + loss)
**Source:** `crates/aprender-core/src/autograd/ops/mod.rs:34-47`
```rust
if is_grad_enabled() && input.requires_grad_enabled() {
    result.requires_grad_(true);
    let grad_fn = Arc::new(SomeBackward { /* captured ctx */ });
    result.set_grad_fn(grad_fn.clone());
    with_graph(|graph| {
        graph.register_tensor(input.clone());
        graph.record(result.id(), grad_fn, vec![input.id()]);
    });
}
```
Any `Tensor::from_vec`/`Tensor::new` on a computed value WITHOUT an adjacent `set_grad_fn` is the PMAT-913/914/922/931 severed-graph bug class — the phase's reason to exist.

### 2. Central finite-difference gradcheck harness
**Source:** `crates/aprender-core/src/nn/conv/tests_pool_flatten_backward_gradflow.rs:26-104`
**Apply to:** all six `tests_*_backward.rs` files (D-04). Asserts grad is present (severed-graph guard), shape-matched, finite, non-zero, AND matches central differences at every element — never a tautological `is_some`.

### 3. Typed error struct/enum, no panics on hostile input
**Source:** `crates/aprender-core/src/models/bert/load.rs:94-145`
**Apply to:** `setfit/error.rs`, `setfit/import.rs`, all op entry points with failure modes (zero denominator, OOV, oversize, mismatched lengths). `unwrap()` is lint-banned; `unsafe` forbidden; no `assert!` on public-reachable paths.

### 4. `#[contract]` annotation + generated macros
**Source:** `crates/aprender-core/src/models/qwen2/mod.rs:121-139`; regen command in `src/generated_contracts.rs:1-4` (`pv codegen contracts/ -o src/generated_contracts.rs`)
**Apply to:** each new autograd op and the encoder forward (D-27).

### 5. Constant-mask via autograd-aware `mul` (PMAT-922)
**Source:** `crates/aprender-core/src/nn/dropout/mod.rs:110-135`
**Apply to:** `additive_attention_mask` (constant tensor, flows through existing broadcast-add in SDPA — no backward struct needed), any masking inside pooling.

### 6. Feature-gated test entry
**Source:** `crates/aprender-core/tests/falsification_spec_v10_tests.rs:16` (`#![cfg(feature = "model-tests")]`) + `Cargo.toml:211` (`model-tests = []`)
**Apply to:** `tests/setfit_conformance/` (gate on `setfit`), D-10 full-weight suite (heavy feature + `#[ignore]`).

### 7. Test-file inclusion
**Sources:** `autograd/grad_fn_tests.rs:5-6` (`include!("tests_elementwise_backward.rs");`) and `nn/linear.rs:315-321` (`#[cfg(test)] #[path = "linear_tests.rs"] mod tests;`)
**Apply to:** wiring the six new backward-test files. CB-510 note: after adding any `include!()` file, run `bash scripts/check_include_files.sh` and `git check-ignore -v <path>` (must exit 1).

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `scripts/setfit_fixtures/` (uv project, Python generator, slice tool) | script | batch | No pyproject.toml/uv.lock exists anywhere under `scripts/` — this is the repo's first uv project. Follow RESEARCH.md D-12 commands verbatim (`uv init`, pinned adds, committed lockfile). Keep it Python-first (bashrs not installed). |
| `crates/aprender-core/tests/fixtures/setfit/` (JSON corpus + slice APR + SHA-256 manifest) | fixture data | n/a | No committed JSON-fixture corpus precedent in aprender-core tests; nearest is the env-override path resolution in `aprender-bench-tokenizer/src/lib.rs:40-61`. Manifest hashing uses the `sha2` workspace dep. Verify `git check-ignore -v` exits 1 for every fixture (`.gitignore` root-anchors `/*.apr` and `/models/`, so `crates/**` fixtures are safe — but verify per CB-510). |

## Metadata

**Analog search scope:** `crates/aprender-core/src/{autograd,nn,models/bert,models/qwen2,setfit-adjacent}`, `crates/aprender-core/tests/`, `crates/aprender-bench-tokenizer/`, `contracts/`, `Makefile`, `Cargo.toml` (root + core), `scripts/`
**Files scanned:** ~35 (12 read in full or targeted excerpt)
**Pattern extraction date:** 2026-08-07
