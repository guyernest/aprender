# Phase 8: Laya Decision Model: Local Fine-Tune and Thin MCP Server - Context

**Gathered:** 2026-09-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Productise spikes 024–026 as the first member of a **generalized decision-model family**:

1. A user fine-tunes Laya (ModernBERT-large + 2-layer head + shared marker scorer) **locally** on their own
   labelled shots, calibrates it, and gets a gate report.
2. The checkpoint is converted to **.apr** and served in Rust through a thin, task-bound `classify` MCP server.
3. One trained model (TweetEval stance) is **deployed live on pmcp.run** (default Lambda) and verified by a real call.

In-tree, linted, contracted and CI-tested. ModernBERT becomes a reusable aprender-core model, and the decision
layer is a method-neutral crate that Kev/Jev can join later — but only Laya is implemented in this phase.

Not in this phase: a training MCP server, Kev/Jev ports, multilingual or `typed-decisions` bases, a Rust trainer.

</domain>

<decisions>
## Implementation Decisions

### Local training harness
- **D-01:** Training runs in **Python, on Laya's own code** — the spike-024 full fine-tune (`ft_laya.py` recipe:
  rows built by Laya's `Agent._encode_state`, so training rows are byte-identical to inference rows). Rust owns
  inference; the handoff is probability parity to Python fp32. A Rust trainer is not planned.
- **D-02:** Invocation is **`just laya-train` recipes over an in-tree, pinned `uv` project** (lockfile pinning torch,
  transformers, and Laya at `4066d5d` / weights `convaiinnovations/laya` @ `55cf4c4e`). No new `apr` subcommand — the
  CLI registry (`contracts/apr-cli-commands-v1.yaml`) is untouched.
- **D-03:** Device is **auto-selected MPS → CUDA → CPU**, and the report records the device *actually used* (from
  torch, not an env var) — CLAUDE.md Verification Discipline #2. CPU is allowed but is flagged in the report.
- **D-04:** Base checkpoint is the **English root only** in this phase. The base is a **declared field** in the
  artifact manifest (not an implicit assumption), so adding multilingual (mmBERT) and `typed-decisions` later is
  additive. Recipe values are fixed to the spike's: AdamW, encoder lr 2.5e-5, head lr 1e-4, cosine to 1e-6, clip
  1.0, batch 8, loss CE + (−proper_reward, w 0.75); epochs 12 at ≤16 shots/class, a declared value in 4–12 at 64.
  The recipe is written into the artifact before any score is read.

### Dataset in, quality gate out
- **D-05:** Input is **`task.json` + `train.jsonl`**. `task.json` = `{type: "choice", instructions, criteria}` with
  criteria **ordered** (order is the label index — `serde_json` `preserve_order`, per spike 026). `train.jsonl` rows
  are `{text, label}`. The served tool reads the same `task.json` from the artifact.
  — **Reversibility:** costly — the schema is shared by trainer, artifact manifest and server; changing it touches all three plus fixtures.
- **D-06:** **`eval.jsonl` is required** and held out for the gate. Temperature calibration is refit on a **seeded
  held-out slice of the train shots** (spike 024 step 2 — every fine-tuned run is over-confident, ECE 0.17–0.38).
  Eval and calibration data never overlap.
- **D-07:** **Fail-closed gate.** The report must show (a) the fine-tuned model beats zero-shot Laya on `eval.jsonl`
  and (b) post-calibration ECE is under a **declared** ceiling. The deploy recipe **refuses** an artifact without a
  passing report. Thresholds are declared in a contract before any run is read (Phase 5 claims-gate philosophy).
- **D-08:** **One declared seed by default; `--seeds N` produces a variance report** (mean ± sd). The report states
  "single seed" plainly when N = 1 (spike 024 saw 0.545–0.679 across seeds at 64 shots). The shipped model is always
  the declared seed — **never the best seed on eval**.

### MCP tool surface
- **D-09:** The predict server exposes a **task-bound `classify`** tool: the question and labels come from the
  artifact's `task.json`; the caller sends only text. No generic `decide`/`/v1/systemone` surface — a fine-tuned
  model answers one question. One trained model per deployed server (thin-server rule).
  — **Reversibility:** costly — this is the published tool contract agents integrate against.
- **D-10:** `classify` accepts a **list of texts** with a **contract-owned maximum** (a tool-boundary contract in
  the shape of `contracts/forecast-tool-boundary-v1.yaml`). Per-row forwards are acceptable; batched GEMM is an
  optimisation, not a requirement.
- **D-11:** Each result returns **`label` + calibrated `probabilities` over every label in order**; the response
  carries **model identity** (artifact content hash + recipe id) so a caller can prove which model answered.
- **D-12:** Text past the 512-token window is **truncated exactly as Laya's builder does**, with **`truncated: true`**
  on that result (parity with Python; the spike-025 fixture has a 512-token truncation row).

### Crate home, artifact and deploy
- **D-13:** **ModernBERT lives in aprender-core at `crates/aprender-core/src/models/modernbert/`**, beside
  `models/bert/` and shaped like it (config, embeddings, layer, encoder, load-from-.apr). It is a reusable encoder,
  not Laya-specific. aprender-core's own BERT (`models/bert/`, post-norm HF BERT + WordPiece) is the wrong
  architecture on every block and is NOT reused — the port is spike 025's semantics table.
- **D-14:** The decision layer is a new **method-neutral crate `aprender-decide`**: a decision-method seam (artifact
  manifest, `task.json`, the classify contract) with **Laya as the only implementation** (head, scorer, request
  builder, temperature buckets on top of core's ModernBERT). Kev/Jev are deferred. The name is deliberately not
  `llm-`: Laya is an encoder.
  — **Reversibility:** one-way — a crate name becomes permanent once published to crates.io; confirm before the first publish.
- **D-15:** Servers follow the setfit/chronos pairs: **`aprender-mcp-decide`** (stdio, pmcp) and
  **`aprender-mcp-decide-lambda`** (bootstrap). Both join the thin-server `[[bin]]` list in
  `crates/aprender-core/tests/monorepo_invariants.rs` (FALSIFY-MONO-011) — no baseline change is expected.
- **D-16:** CLAUDE.md's realizar-first table gains a **third documented exception row** (decision models), argued
  like SetFit D-09 and Forecast D-07: a 421M encoder with no KV cache and no LLM kernels, whose only parity-proven
  implementation lives with the code; serving a second port through realizar would violate OPS-03.
- **D-17:** The served artifact is **.apr** (converted from Laya's safetensors F16 + tokenizer + configs), stored
  **F16 and widened to f32 at load** — keeps the 0.84 GB artifact and spike 026's ~12 s cold start. Parity is
  proven end to end, **torch → .apr → Rust**, against the spike-025 fixture (probs ≤ 1e-5; spike got 3.8e-6, ids 14/14).
  The .apr also carries (or its manifest references) `task.json`, recipe, calibrated temperatures, gate report and
  sha256 of every input file. An APR schema contract is written in the shape of `contracts/setfit-apr-v1.yaml`.
  — **Reversibility:** costly — the APR schema for ModernBERT/decision artifacts becomes the on-disk format every later method and deployed model reads.
- **D-18:** **Live pmcp.run deploy of one trained model** on default Lambda at 10,240 MB, weights fetched from S3
  at cold start (never baked into the image — spike 026 / `aws-mcp-model-hosting.md`), verified by a real
  `classify` call. The live deploy itself is a **human checkpoint** (outward-facing).
- **D-19:** The demo model is **TweetEval stance** at 16 or 64 shots/class — spike baselines exist to sanity-check
  the gate (full FT F_avg 0.538 ± 0.017 @16, 0.608 ± 0.050 @64; SetFit 0.512 / 0.561).

### Claude's Discretion
- Exact file layout of the uv project and just recipe names.
- How the Python output becomes .apr (a Python-side export vs extending `apr import`/the converter) — pick whichever
  keeps one converter implementation and a clean parity chain; research should compare.
- S3 layout, IAM scoping and the cold-start loader, following the SetFit/Chronos Lambda crates.
- The declared ECE ceiling and zero-shot margin values (must be declared in the contract before the demo run).
- The `classify` list maximum (priced against the Lambda envelope, not guessed — see Phase 7's accepted-region lesson).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Spike evidence (Laya)
- `.claude/skills/spike-findings-aprender/SKILL.md` — findings index; load via `Skill("spike-findings-aprender")`
- `.claude/skills/spike-findings-aprender/references/laya-decision-model.md` — fine-tune recipe, calibration need, what to avoid (head-only adapters)
- `.claude/skills/spike-findings-aprender/references/laya-rust-inference.md` — ModernBERT/head/scorer/builder semantics table, local window |i−j| ≤ 64, parity ladder, productisation checklist
- `.claude/skills/spike-findings-aprender/references/aws-mcp-model-hosting.md` — default Lambda for ≤1 GB models, S3 cold start, never bake weights
- `.claude/skills/spike-findings-aprender/sources/024-laya-vs-kev-few-shot/tools/ft_laya.py` — the training recipe to productise
- `.claude/skills/spike-findings-aprender/sources/025-laya-rust-forward-parity/` — Rust port (`src/laya.rs`), oracle (`tools/oracle.py`)
- `.claude/skills/spike-findings-aprender/sources/026-laya-mcp-default-lambda/src/lib.rs` — `render_options`, request path, S3 loader
- `.planning/spikes/025-laya-rust-forward-parity/fixtures/laya-en_fixture.json` — 14-row parity fixture (ladder `.bin` regenerates via the oracle)

### Architecture rules and precedents
- `CLAUDE.md` §"CRITICAL: Realizar-First Architecture" — the table gaining a third exception row (D-16); SetFit and Forecast rows are the argument template
- `contracts/setfit-apr-v1.yaml` — APR schema/load-rule contract precedent (D-17)
- `contracts/forecast-tool-boundary-v1.yaml` — tool-boundary contract precedent (D-10)
- `contracts/chronos-bolt-parity-v1.yaml` — parity contract precedent (D-17)
- `crates/aprender-core/tests/monorepo_invariants.rs:313-319` — thin-server `[[bin]]` list (D-15)

### Existing code to mirror
- `crates/aprender-core/src/models/bert/` — shape for `models/modernbert/` (D-13)
- `crates/aprender-mcp-setfit/` — thin pmcp predict server template
- `crates/aprender-mcp-chronos-lambda/`, `crates/aprender-mcp-setfit-lambda/` — Lambda bootstrap + deploy config pattern
- `crates/aprender-core/src/format/converter/` — import/convert path the .apr conversion must fit (D-17)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `trueno::blis::gemm_blis` + the spike-020 layout (`Cᵀ = W · Xᵀ`, weight `[out,in]` as A, rows banded over rayon) — the only aprender dependency spike 025 needed.
- `crates/aprender-core/src/format/converter/f16_convert.rs` — F16 handling for the F16-in-.apr decision (D-17).
- `crates/aprender-mcp-setfit/` — in-process predict via a public library door; the Laya server does the same through `aprender-decide`.
- Spike 026's `render_options` / refusal rules (unknown types, <2 options, options beyond `head_max_len` → refused, not defaulted).

### Established Patterns
- Thin server per model; transport-only servers; bounds owned by a contract and not re-checked by the library door.
- Lambda handler binary is named `bootstrap`; cargo-pmcp discovers deployables by that name and has no `--package`
  flag — a new Lambda crate needs `--manifest-path` (SetFit training precedent).
- Workspace lints: `unsafe_code = forbid`, no `unwrap()`, pedantic clippy; coverage must co-evolve with contracts (`#[contract]` + falsification tests).
- New `tests/*.rs` targets are dark until added to the explicit `--test` line in `.github/workflows/ci.yml` — editing CI workflows needs a check-in per CLAUDE.md.

### Integration Points
- `crates/aprender-core/src/models/mod.rs` — register `modernbert`.
- Root `Cargo.toml` workspace members — add `aprender-decide`, `aprender-mcp-decide`, `aprender-mcp-decide-lambda`.
- `crates/aprender-core/tests/readme_contract.rs` — new crates need READMEs and the monorepo link; README crate/contract counts will move.
- The deploy recipe reads the gate report and refuses on fail (D-07).

</code_context>

<specifics>
## Specific Ideas

- The user wants the design **generalized**: ModernBERT supported "in a similar way that other BERT models are
  supported", and the decision crate able to host "other methods that will pop up soon with the success of Jev, Kev,
  and Laya". The trait/seam should be real enough that Kev can join without reshaping the artifact or tool contract —
  but it must not be speculative abstraction; one implementation, designed for a known second.
- Future bases (multilingual mmBERT, `typed-decisions`) are expected "when we get use cases for it".

</specifics>

<deferred>
## Deferred Ideas

- **Multilingual Laya base (mmBERT-base 322M, 256k vocab, different RoPE)** — future phase, when a use case arrives.
- **`typed-decisions` base as a fine-tune starting point** — future phase; its full fine-tune was never measured.
- **Kev / Jev as `aprender-decide` methods** — future phase; Kev's Rust path lives only on unpushed `spike/016-upstream-sync`.
- **A Laya/decide training MCP server** (the `aprender-mcp-setfit-train` shape) — not requested; this phase trains locally.
- **Rust trainer for ModernBERT** — not planned; Python stays the training oracle.
- **Batched-row GEMM for `classify`** — optimisation after the tracer works.

### Reviewed Todos (not folded)
- `aprender-train-gpu-ledger-tests-red.md` — matched on generic keywords only; unrelated.
- `qwen35-9b-hybrid-forward-unimplemented.md` — Kev/Qwen territory; relevant only when Kev joins `aprender-decide`.
- `workspace-test-gate-blocked-by-renacer-validate.md` — CI gate issue, unrelated to this phase's scope (may still affect its CI runs).

</deferred>

---

*Phase: 08-laya-decision-model-local-fine-tune-and-thin-mcp-server*
*Context gathered: 2026-09-25*
