# Qwen3.5 CPU Session Batched Prefill (PR-A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the resident Qwen3.5 session on the CPU prefill every multi-token span through the
existing batched prefill (`forward_prefill_qwen35`, #4228), prove it bit-identical and counted,
measure it, and open the upstream PR for #5475, after posting the two coordination comments the
design calls for.

**Architecture:** `Qwen35Forward::try_forward` batches only under `#[cfg(feature = "cuda")]` and
only on `Backend::Gpu`. One new CPU-only helper, `try_batched_prefill_cpu`, sits beside the
untouched GPU helper and feeds the same `batched_prefills` counter and the same log-line shape.
Nothing in `Session` changes: the #4214 two-span split already applies on both backends, so the
counts a CPU session reports are the GPU test's counts. A new session test is the CPU twin of
`gpu_serve_prefill_is_batched_and_token_identical_to_the_one_token_path`, with the session's own
per-token switch as the control and `assert_eq!` on tokens, because on the CPU the batched prefill
is bitwise the per-token loop (FALSIFY-4228-004/005). A small `contracts/` YAML and a measured
receipt ride with the PR, as the GEMM contribution did.

**Tech Stack:** Rust 1.93.0 (upstream `rust-toolchain.toml`), `aprender-serve` (lib `realizar`),
cargo test / clippy / fmt, in-tree `pv` (`aprender-contracts-cli`), `gh`.

**Spec:** `docs/superpowers/specs/2026-10-18-qwen35-prefix-checkpoints-and-cpu-prefill-design.md`
(§4 O1, §6 steps 1–3, §7, §8 call 1). This plan is PR-A only; PR-B (O3) waits for the #4214 answer
and PR-C (O2) follows PR-A, each with its own plan.

## Global Constraints

- Cut from `upstream/main` (`e293f183b`, 0.71.3) in a fresh worktree; never from this fork's
  branches (checkout is 0.69.3 and `upstream/main` is not an ancestor).
- Worktree `/Users/guy/Development/machine-learning/aprender-prefill-upstream`, branch
  `perf/qwen35-cpu-session-batched-prefill`, pushed to `origin` (`guyernest/aprender`), PR against
  `paiml/aprender` `main`. One PR in flight.
- Toolchain pinned 1.93.0; `cargo fmt --all -- --check` and
  `cargo clippy -p aprender-serve --lib -- -D warnings` clean; `unwrap()` is banned
  (`expect` in tests only).
- The GPU helper `try_batched_prefill` and its log line stay byte-identical; a CUDA build cannot be
  compiled here, so the cuda-gated code is not edited.
- `APR_QWEN35_SESSION_PREFILL=per-token` keeps restoring the one-token loop on the CPU too.
- Tokens are asserted equal, never within a tolerance: CPU batched prefill ≡ per-token bitwise.
- Models: `APR_QWEN35_MODEL_DIR` points at a scratchpad directory of symlinks to the HF-cache
  GGUFs (`Qwen3.5-0.8B-Q4_K_M.gguf` 532,517,120 bytes, `Qwen3.5-4B-Q4_K_M.gguf`
  2,740,937,888 bytes). Tests that cannot find the model SKIP; a receipt run must not skip.
- Use `/opt/homebrew/bin/git` and `rtk proxy gh` (the rtk hook rewrites output); `/usr/bin/grep`.
- Commit messages end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`; the PR
  body ends with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
- Nothing is sent to pmcp.run. Only the two comments in Task 1 and the PR in Task 6 are
  GitHub writes.

## Review Focus

1. **A turn that extends from a nonzero position** (turn 2 of a chat) and **a resumed checkpoint**
   (the same chat again, turn 3): the batched span starts at `pos0 > 0` and must read the KV rows
   and recurrent state already held; wrong `start_pos` plumbing gives plausible but different
   tokens. Pinned: Task 2 test, turns 2 and 3 (`reused`, counts 4 and 5, tokens equal).
2. **A raw prompt after a chat** (no `<|im_start|>`, does not extend): reset in place, one batched
   span to the last token, the last token alone through the one-token path, counter +1 not +2.
   Pinned: Task 2 test, turn 4.
3. **The per-token switch on the CPU**: must still give the one-token loop and identical tokens.
   Pinned: Task 2 test control session, and the RED control in Task 4 (env var set → count 0 → RED).
4. **A GPU session that fell back to the CPU mid-life** (CUDA builds): the GPU helper returns
   `None` (backend is now `Cpu`) and the CPU helper must then batch. Not testable here; the code
   shape in Task 3 makes it fall out (CPU helper is tried after the GPU helper, keyed on backend).
5. **A span that would overflow the state**: `forward_prefill_qwen35` refuses before touching the
   state (FALSIFY-4228-007); the session reserves `prompt + budget` up front so it cannot reach
   it. Pinned by the existing test; Task 3 re-runs it.

---

### Task 1: Post the coordination comments on #4214 and #5476

**Files:** none (GitHub writes only).

**Interfaces:**
- Produces: a comment URL on each issue, quoted in the PR body (Task 6).

- [ ] **Step 1: Post on #4214**

```bash
rtk proxy gh issue comment 4214 --repo paiml/aprender --body-file - <<'EOF'
Picking up the two halves of this from the pmcp.run side (an agent endpoint on `apr serve`, CPU-only Lambda, where a 420-token system+tools prefix costs 101 s cold at 0.71.4 and 1.8 s on the same prompt again, so this checkpoint is the biggest lever we have).

1. **Opening today, as #5475:** the CPU session goes through #4228's `forward_prefill_qwen35` for every multi-token span. On `main` `try_forward` batches only under `cfg(feature = "cuda")`, so a CPU serve still loops `forward_single_qwen35` and prefill runs at the decode rate. Wiring plus a CPU twin of `gpu_serve_prefill_is_batched_and_token_identical_to_the_one_token_path`, tokens asserted equal (the CPU prefill is bitwise the per-token loop by FALSIFY-4228-004/005).

2. **Your done-when here (`timings.prompt_n < prompt_tokens`) is a small follow-up we can take next:** `Turn::reused` already exists; the chat route would report it as `usage.prompt_tokens_details.cached_tokens` (the OpenAI shape, on the blocking response and the streamed terminal chunk, which already carries `usage`) and set `timings.prompt_n` to the tokens actually prefilled. Which base should it cut from: `main` as of now, or the `8f601caff2` named above? And is `timings.prompt_n` the field you want moved, or a new field beside it?

After that, #5476 proposes generalising this one checkpoint to a stack at every `<|im_start|>` (deepest-prefix resume, no KV copies), which is what makes a shared system+tools prefix hit across *different* conversations.
EOF
```

- [ ] **Step 2: Post on #5476**

```bash
rtk proxy gh issue comment 5476 --repo paiml/aprender --body-file - <<'EOF'
Proposed shape, after the #5475 wiring lands and the #4214 owner answers on the reporting half:

- Generalise the #4214 checkpoint to a **stack of checkpoints at every `<|im_start|>` of the prompt** (capped; keep the shallowest and the deepest). `prepare_prompt` restores the deepest checkpoint that is a prefix of the new prompt; `step` from position `start` drops every checkpoint deeper than `start`. Arch-agnostic in `Session`; `checkpoint_at` becomes "checkpoint points", `save`/`restore` take an index; both backends reuse their existing copy code.
- Cost is only the recurrent half per checkpoint (conv + DeltaNet state, fixed size): ~26 MiB on 0.8B and ~67 MiB on 4B as `Qwen35State::checkpoint` copies today (all layers), ~19 / 50 MiB if trimmed to the DeltaNet layers. KV rows are never copied: rows below the restored point are still in the state and are rewound by truncation.
- Why message boundaries rather than a block grid: vLLM's hybrid prefix cache snapshots Mamba state per block and gets 0 % hits under ~528 tokens on Qwen3.5-4B (vllm#40696); agent traffic shares its prefix exactly at message starts, which `checkpoint_at` already locates by token id.
- Numerics: on the CPU a resumed turn equals a from-zero prefill bitwise, so caching cannot add a second source of tool-choice flips; on CUDA it is bounded by the F2 guard's tolerance.

Happy to cut it on whichever base the #4214 work lands on.
EOF
```

- [ ] **Step 3: Verify both comments landed**

Run: `for n in 4214 5476; do rtk proxy gh issue view $n --repo paiml/aprender --json comments --jq '.comments[-1] | "\(.author.login) \(.createdAt) \(.url)"'; done`
Expected: two lines, author `guyernest`, timestamps from now. Keep the two URLs for Task 6.

---

### Task 2: Worktree, baseline proof, and the failing CPU session test

**Files:**
- Create: worktree `/Users/guy/Development/machine-learning/aprender-prefill-upstream` (branch `perf/qwen35-cpu-session-batched-prefill` from `upstream/main`)
- Create: `/private/tmp/claude-501/-Users-guy-Development-machine-learning-aprender/d0054c91-c40d-4435-b1d4-b59c1eea11f9/scratchpad/models/{Qwen3.5-0.8B-Q4_K_M.gguf,Qwen3.5-4B-Q4_K_M.gguf}` (symlinks)
- Modify: `crates/aprender-serve/src/gguf/inference/forward/qwen35_session_tests.rs` (in the worktree): the `MODEL_PATH` const (11 uses) and one new test

**Interfaces:**
- Produces: `fn model_path() -> String` in the test module (`APR_QWEN35_MODEL_DIR/Qwen3.5-0.8B-Q4_K_M.gguf`, default `/home/noah/models/Qwen3.5-0.8B-Q4_K_M.gguf`, unchanged for upstream's hosts); test `cpu_serve_prefill_is_batched_and_token_identical_to_the_one_token_path`.
- Consumes: `Qwen35Session::load_cached(&Path, &MappedGGUFModel, no_gpu: bool)`, `Session::engine_mut()` (`pub(crate)`), the private field `Qwen35Forward::per_token_prefill` (visible: the test file is a child module), `Session::batched_prefills()`, `Turn { tokens, reused, .. }`, helpers `encode`, `greedy`, `user_turn`, `one_shot_cpu` already in the file.

- [ ] **Step 1: Create the worktree and the model directory**

```bash
cd /Users/guy/Development/machine-learning/aprender
/opt/homebrew/bin/git worktree add -b perf/qwen35-cpu-session-batched-prefill ../aprender-prefill-upstream upstream/main
M=/private/tmp/claude-501/-Users-guy-Development-machine-learning-aprender/d0054c91-c40d-4435-b1d4-b59c1eea11f9/scratchpad/models
mkdir -p "$M"
ln -sf ~/.cache/huggingface/hub/models--unsloth--Qwen3.5-0.8B-GGUF/snapshots/6ab461498e2023f6e3c1baea90a8f0fe38ab64d0/Qwen3.5-0.8B-Q4_K_M.gguf "$M/"
ln -sf ~/.cache/huggingface/hub/models--unsloth--Qwen3.5-4B-GGUF/snapshots/e87f176479d0855a907a41277aca2f8ee7a09523/Qwen3.5-4B-Q4_K_M.gguf "$M/"
```

Expected: `git -C ../aprender-prefill-upstream log --oneline -1` prints `e293f183b`; `stat -L` on both links gives the sizes in Global Constraints.

- [ ] **Step 2: Prove the baseline on this box (the batched kernel is bit-identical here, and the model loads)**

Run (in the worktree): `APR_QWEN35_MODEL_DIR=$M APR_QWEN35_REQUIRE_MODEL=1 cargo test --release -p aprender-serve --lib falsify_4228_004_prefill_bit_identical_0_8b -- --nocapture`
Expected: `test result: ok. 1 passed`, no `SKIP:` line.

- [ ] **Step 3: Replace the `MODEL_PATH` const with `model_path()` and add the failing test**

In `qwen35_session_tests.rs`: replace `const MODEL_PATH: &str = "/home/noah/models/Qwen3.5-0.8B-Q4_K_M.gguf";` with

```rust
/// `$APR_QWEN35_MODEL_DIR/Qwen3.5-0.8B-Q4_K_M.gguf`, else the host path the rest of
/// the Qwen3.5 tests are specified against (as `forward_qwen35_prefill_tests` does).
fn model_path() -> String {
    match std::env::var("APR_QWEN35_MODEL_DIR") {
        Ok(dir) => format!("{dir}/Qwen3.5-0.8B-Q4_K_M.gguf"),
        Err(_) => "/home/noah/models/Qwen3.5-0.8B-Q4_K_M.gguf".to_string(),
    }
}
```

and update every `MODEL_PATH` use (`mapped_or_skip!`, each `load_cached(std::path::Path::new(MODEL_PATH), ...)`, the GPU module) to `model_path()` / `std::path::Path::new(&model_path())`. Then add, outside the `#[cfg(feature = "cuda")]` module, next to the two `cpu_*` tests:

```rust
/// #5475: the CPU route prefills through the batched prefill too (#4228's
/// `forward_prefill_qwen35`, bitwise the one-token loop by FALSIFY-4228-004/005),
/// so tokens are asserted equal, not close. Engagement is read off the counter;
/// the control is the session's own per-token switch. The counts follow the
/// #4214 split exactly as in the GPU twin.
#[test]
fn cpu_serve_prefill_is_batched_and_token_identical_to_the_one_token_path() {
    let mapped = mapped_or_skip!();
    let path = std::path::PathBuf::from(model_path());
    let mut batched = Qwen35Session::load_cached(&path, &mapped, true).expect("load");
    let mut one_token = Qwen35Session::load_cached(&path, &mapped, true).expect("load");
    assert!(!batched.on_gpu() && !one_token.on_gpu(), "--no-gpu is the CPU route");
    one_token.engine_mut().per_token_prefill = true;
    let config = greedy(8);

    // Turn 1: a fresh chat prefills in two batched spans, history then generation header.
    let p1 = encode(&mapped, &user_turn("Name the capital of Peru."));
    let b1 = batched.generate(&p1, &config, &mut |_| true).expect("turn 1");
    let o1 = one_token.generate(&p1, &config, &mut |_| true).expect("turn 1");
    assert_eq!(batched.batched_prefills(), 2, "both spans of turn 1's prompt went through the batched prefill");
    assert_eq!(one_token.batched_prefills(), 0, "the control prefilled per token");
    assert_eq!(b1.tokens, o1.tokens, "turn 1: batched == one-token");
    assert_eq!(b1.tokens, one_shot_cpu(&mapped, &p1, &config), "and both are the reference loop");

    // Turn 2: extends turn 1, so both spans batch from a nonzero position.
    let mut p2 = b1.tokens.clone();
    p2.extend(encode(&mapped, &format!("<|im_end|>\n{}", user_turn("And of Chile?"))));
    let b2 = batched.generate(&p2, &config, &mut |_| true).expect("turn 2");
    let o2 = one_token.generate(&p2, &config, &mut |_| true).expect("turn 2");
    assert_eq!(b2.reused, b1.tokens.len() - 1, "turn 2 extended the state");
    assert_eq!(batched.batched_prefills(), 4, "both spans of turn 2's new suffix went through the batched prefill, from a nonzero position");
    assert_eq!(b2.tokens, o2.tokens, "turn 2: batched == one-token");
    assert_eq!(b2.tokens, one_shot_cpu(&mapped, &p2, &config), "and both are the reference loop");

    // Turn 3: the same chat again. It does not extend the state, but it starts with
    // the #4214 checkpoint turn 2 left (its history, before the generation header),
    // so the session restores it and batches only the header: one more span, and the
    // tokens of a resumed CPU state are turn 2's, bitwise.
    let b3 = batched.generate(&p2, &config, &mut |_| true).expect("turn 3");
    let o3 = one_token.generate(&p2, &config, &mut |_| true).expect("turn 3");
    assert!(b3.reused > 0 && b3.reused < p2.len(), "turn 3 resumed from the checkpoint: reused {}", b3.reused);
    assert_eq!(batched.batched_prefills(), 5, "the generation header after a restored checkpoint is one batched span");
    assert_eq!(b3.tokens, b2.tokens, "turn 3: a resumed state decodes what the fresh one did");
    assert_eq!(b3.tokens, o3.tokens, "turn 3: batched == one-token");

    // Turn 4: a raw prompt that extends nothing: reset in place, one batched span up to
    // its last token, then that token alone through the one-token path (never counted).
    let p4 = encode(&mapped, "The capital of Peru is");
    let b4 = batched.generate(&p4, &config, &mut |_| true).expect("turn 4");
    let o4 = one_token.generate(&p4, &config, &mut |_| true).expect("turn 4");
    assert_eq!(b4.reused, 0, "a raw prompt extends nothing");
    assert_eq!(batched.batched_prefills(), 6, "a raw prompt adds one batched span, its last token is a one-token step");
    assert_eq!(one_token.batched_prefills(), 0, "the control never batched");
    assert_eq!(b4.tokens, o4.tokens, "turn 4: batched == one-token");
    assert_eq!(b4.tokens, one_shot_cpu(&mapped, &p4, &config), "and both are the reference loop");
}
```

- [ ] **Step 4: Run it to verify it fails on the counter, not on tokens**

Run: `APR_QWEN35_MODEL_DIR=$M cargo test --release -p aprender-serve --lib cpu_serve_prefill_is_batched -- --nocapture 2>&1 | tail -15`
Expected: FAIL at the first `batched_prefills()` assertion with `left: 0`, `right: 2`.

- [ ] **Step 5: Run the other CPU session tests to be sure the path change did not break them**

Run: `APR_QWEN35_MODEL_DIR=$M cargo test --release -p aprender-serve --lib qwen35_session_tests 2>&1 | tail -5`
Expected: only the new test fails; every `cpu_*` and unit test passes, GPU tests skip or are compiled out.

(No commit yet: the test and the wiring go in one commit in Task 3.)

---

### Task 3: Wire the CPU batched prefill

**Files:**
- Modify: `crates/aprender-serve/src/gguf/inference/forward/qwen35_session.rs` (`try_forward` ~L519–538; new fn after `try_batched_prefill` ~L588; import at L24; doc of `QWEN35_SESSION_PREFILL_ENV` L39 and of the `per_token_prefill` / `batched_prefills` fields ~L352–355 if they say "GPU")
- Test: `qwen35_session_tests.rs` (Task 2's test)

**Interfaces:**
- Produces: `fn try_batched_prefill_cpu(&mut self, new: &[u32], pos0: usize) -> std::result::Result<Option<Vec<f32>>, Step>` on `Qwen35Forward` (private). `Ok(None)` when the backend is not `Backend::Cpu(Some(_))` or `per_token_prefill` is set; otherwise one `forward_prefill_qwen35(new, state, pos0)` call, the log line, `batched_prefills += 1`, `Ok(Some(logits))`.
- Consumes: `Qwen35Model::forward_prefill_qwen35(&self, tokens: &[u32], cache: &mut Qwen35State, start_pos: usize) -> Result<Vec<f32>>`; `QWEN35_PREFILL_CHUNK` from `crate::gguf::forward_qwen35`; `From<RealizarError> for Step` (exists).

- [ ] **Step 1: Change `try_forward` so the batched branch is tried on both backends**

Replace the cuda-gated block with:

```rust
// A prompt, or a turn's new suffix, goes through the batched prefill: #3596's on the
// GPU, #4228's on the CPU (#5475). Decode steps (one new token) stay one-token.
if tokens.len().saturating_sub(start) > 1 {
    #[cfg(feature = "cuda")]
    if let Some(logits) = self.try_batched_prefill(&tokens[start..], start)? {
        return Ok(logits);
    }
    if let Some(logits) = self.try_batched_prefill_cpu(&tokens[start..], start)? {
        return Ok(logits);
    }
}
```

The GPU helper is untouched; when it declines (`None`: not on the GPU, per-token, or no fit) the CPU helper runs, and declines itself unless the backend is the CPU.

- [ ] **Step 2: Add `try_batched_prefill_cpu`**

Doc: "`None` when the session is not on the CPU or was told to prefill per token. On the CPU the batched prefill is the per-token loop bit for bit (FALSIFY-4228-004/005), so there is no fallback to take: an error is the turn's, and the state is discarded by the caller." The log line, to match the GPU line's shape so a serve log says which prefill it took:

```rust
eprintln!(
    "[qwen35] batched prefill: {} tokens in {ms:.0} ms ({:.0} tok/s, chunk {QWEN35_PREFILL_CHUNK} rows, on the CPU, from position {pos0})",
    new.len(),
    new.len() as f64 * 1000.0 / ms.max(1e-9),
);
```

Add `QWEN35_PREFILL_CHUNK` to the `use crate::gguf::forward_qwen35::{...}` import. Fix any doc comment that says the switch or the counter is GPU-only.

- [ ] **Step 3: Run the new test**

Run: `APR_QWEN35_MODEL_DIR=$M cargo test --release -p aprender-serve --lib cpu_serve_prefill_is_batched -- --nocapture 2>&1 | tail -12`
Expected: PASS; the log shows six `[qwen35] batched prefill: … on the CPU, from position N)` lines, with `from position 0` on the first and a nonzero position on the third, fourth and fifth.

- [ ] **Step 4: Run the surrounding suites**

Run: `APR_QWEN35_MODEL_DIR=$M APR_QWEN35_REQUIRE_MODEL=1 cargo test --release -p aprender-serve --lib -- qwen35_session_tests forward_qwen35_prefill_tests 2>&1 | tail -5`
Expected: all pass (the 4B prefill test runs too, ~minutes), zero failures.
Then: `cargo test -p aprender-serve --lib session 2>&1 | tail -3` (the mock-`ArchForward` suites, no model). Expected: pass.

- [ ] **Step 5: Format and lint**

Run: `cargo fmt --all -- --check && cargo clippy -p aprender-serve --lib -- -D warnings 2>&1 | tail -3`
Expected: no output from fmt; clippy `Finished` with no warnings.

- [ ] **Step 6: Commit**

```bash
/opt/homebrew/bin/git add crates/aprender-serve/src/gguf/inference/forward/qwen35_session.rs crates/aprender-serve/src/gguf/inference/forward/qwen35_session_tests.rs
/opt/homebrew/bin/git commit -m "perf(qwen35): the CPU session prefills through the batched prefill (Refs #5475)

try_forward batched a multi-token span only under cfg(feature = \"cuda\") and only
on Backend::Gpu, so a CPU serve looped forward_single_qwen35 over the prompt and
prefill ran at the decode rate. #4228's forward_prefill_qwen35 is bitwise the
per-token loop (FALSIFY-4228-004/005) and had no production caller. The CPU
helper feeds the same counter and prints the same log-line shape; the per-token
switch still restores the loop.

Test: the CPU twin of the GPU serve-prefill test, tokens asserted equal, counts
following the #4214 split (2, 4, 5 on a restored checkpoint, 6 on a raw prompt).
The session tests take the model from
APR_QWEN35_MODEL_DIR as the prefill tests do; the default path is unchanged.

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 4: Contract YAML with the measured RED controls

**Files:**
- Create: `contracts/qwen35-cpu-session-prefill-v1.yaml` (in the worktree)

**Interfaces:**
- Produces: contract `qwen35-cpu-session-prefill` v1.0.0, kind `pattern`, scope
  `crates/aprender-serve/src/gguf/inference/forward/qwen35_session.rs`, `depends_on:
  [qwen35-batched-prefill-v1]` is NOT used (that one is CUDA-scoped); reference it in
  `metadata.references` instead, with `paiml/aprender#5475`, `#4228`, `#4214`, `#3596`.

- [ ] **Step 1: Measure the two RED controls the contract will cite**

Run: `APR_QWEN35_MODEL_DIR=$M APR_QWEN35_SESSION_PREFILL=per-token cargo test --release -p aprender-serve --lib cpu_serve_prefill_is_batched 2>&1 | /usr/bin/grep -E "panicked|left|right|test result" | head -5`
Expected: FAIL (rc 101) at `both spans of turn 1's prompt went through the batched prefill`, `left: 0`. Record the line.
Then the Task 2 Step 4 result is the second control (the `Backend::Cpu` arm absent → `left: 0`); cite it as measured there.

- [ ] **Step 2: Write the contract**

Shape as `contracts/qwen35-batched-prefill-v1.yaml` (metadata, name, version, scope, description, equations, proof_obligations, falsification_tests, qa_gate). Equations:
- `C-QCSP-001` "The CPU session's multi-token span goes through forward_prefill_qwen35": formula `len(span) > 1 and backend == Cpu and not per_token ⇒ batched_prefills += 1 per span`.
- `C-QCSP-002` "Served tokens are the one-token loop's, bitwise": formula `generate(batched) == generate(per_token) == reference_generate` on fresh, extending and raw turns.
- `C-QCSP-003` "The per-token switch restores the loop": `APR_QWEN35_SESSION_PREFILL=per-token ⇒ batched_prefills == 0 and tokens unchanged`.
Falsification tests `FALSIFY-QCSP-001..003`, each with `test:` = the exact `cargo test` command (with `APR_QWEN35_MODEL_DIR` and `APR_QWEN35_REQUIRE_MODEL=1`) and `if_fails:`. `qa_gate.falsification`: the two measured REDs from Step 1 and Task 2 Step 4, dated 2026-10-18.

- [ ] **Step 3: Validate with the in-tree `pv`**

Run: `cargo run -q -p aprender-contracts-cli --bin pv -- validate contracts/qwen35-cpu-session-prefill-v1.yaml`
Expected: valid, no errors (fix the schema per the message if not; do not script around it).

- [ ] **Step 4: Commit**

```bash
/opt/homebrew/bin/git add contracts/qwen35-cpu-session-prefill-v1.yaml
/opt/homebrew/bin/git commit -m "contracts(qwen35): CPU session batched prefill, with its RED controls (Refs #5475)

Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 5: Measurement receipt (probe and serve A/B on the M4)

**Files:**
- Create: `/private/tmp/claude-501/-Users-guy-Development-machine-learning-aprender/d0054c91-c40d-4435-b1d4-b59c1eea11f9/scratchpad/receipt/{probe-0.8b.log,probe-4b.log,serve-batched.log,serve-per-token.log,RECEIPT.md}`
- Nothing committed to the branch.

**Interfaces:**
- Produces: the before/after table for the PR body (Task 6).

- [ ] **Step 1: Kernel-level probe, 0.8B and 4B, at the pmcp.run prompt size and the #4228 size**

Run (worktree), for `N in 420 850`, `T in 6 10` (`RAYON_NUM_THREADS=$T`; 6 is the Lambda-like count spike 030 used, 10 is this M4's performance cores):
`RAYON_NUM_THREADS=$T APR_QWEN35_PREFILL_MODEL=$M/Qwen3.5-0.8B-Q4_K_M.gguf APR_QWEN35_PREFILL_N=$N APR_QWEN35_PREFILL_REPS=2 cargo test --release -p aprender-serve --lib qwen35_prefill_throughput_probe -- --ignored --nocapture 2>&1 | /usr/bin/grep '^rep'`
Then the 4B at `N=420`, `T=6`, `REPS=1` only (per-token at 4B is ~3 tok/s, so one rep is ~3 min).
Expected: lines `rep r n=N: batched X tok/s, per-token Y tok/s, speedup Zx`; record every line in the logs.

- [ ] **Step 2: Serve-level A/B, same binary, the switch as the control**

Build: `cargo build --release --bin apr` in the worktree; `APR=$(pwd)/target/release/apr` (or the redirected target dir: read it from `cargo metadata --format-version 1 | python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])'`). Prove it: `"$APR" --version` prints 0.71.3 and the branch's short SHA.
Request: spike 030's two-tool chat request (`.planning/spikes/030-qwen35-agent-turn-cost-cpu/`, the ~420-token system + 2 tool schemas + 1 user line), `max_tokens: 1`, `temperature: 0`, `stream: false`.
For each arm (`batched`: no env; `per-token`: `APR_QWEN35_SESSION_PREFILL=per-token`), with `RAYON_NUM_THREADS=6`: start `"$APR" serve run $M/Qwen3.5-0.8B-Q4_K_M.gguf --port 18080` (check `"$APR" serve --help` for the exact form), wait for `Model ready`, POST the request twice with `curl -s -w '%{time_total}'`, read `usage.prompt_tokens` and the server's `[qwen35] batched prefill` lines, stop the server.
Expected: both arms return the same `content`; the batched arm's log shows `on the CPU` lines and the per-token arm shows none; time_total differs by roughly the probe's speedup.

- [ ] **Step 3: Write `RECEIPT.md`**

One table per level (probe: model × N × threads → batched, per-token, speedup; serve: arm → prompt_tokens, time_total ×2, content equal). Note the host (M4 Pro, 14 cores, macOS 25.6.0), toolchain, branch SHA, and that Lambda arm64 numbers follow after the next tag re-pins.

---

### Task 6: Review, push, open the PR, record

**Files:**
- Modify (this repo, not the worktree): `docs/superpowers/specs/2026-10-18-qwen35-prefix-checkpoints-and-cpu-prefill-design.md` §6 (one line: PR-A opened as #NNNN on DATE); memory file `aprender-upstream-qwen35-prefix-and-prefill-state.md` (PR number, worktree path).

- [ ] **Step 1: Fresh-eyes review of the branch**

Dispatch one `code-reviewer` subagent on `git diff upstream/main...HEAD` in the worktree with the spec §4 O1 and §7, asking only for defects that would change tokens, counts, the GPU path, or clippy status. Fix anything real; re-run Task 3 Steps 3–5.

- [ ] **Step 2: Push and open the PR**

```bash
/opt/homebrew/bin/git push -u origin perf/qwen35-cpu-session-batched-prefill
rtk proxy gh pr create --repo paiml/aprender --base main --head guyernest:perf/qwen35-cpu-session-batched-prefill \
  --title "perf(qwen35): the CPU session prefills through the batched prefill (Refs #5475)" --body-file <body>
```

Body (≤ 40 lines): **Summary** (what `try_forward` did, what it does now, GPU path untouched, switch kept); **Measurement** (the two tables from `RECEIPT.md`, plus #4228's 48-thread x86 row quoted with its date); **Tests** (the new CPU twin and its counts; the two measured REDs; suites run); **Contract** (`contracts/qwen35-cpu-session-prefill-v1.yaml`, `pv validate` clean); **Follow-ups** (#4214 comment URL: `cached_tokens`/`prompt_n`; #5476 comment URL: the checkpoint stack); `Closes #5475`; the generated-with line.

- [ ] **Step 3: Record**

Append to the design §6 step 3 the PR number and date; update the memory file's "nothing lands unless we write the PR" line with the PR number and the worktree path; commit the design edit in this repo (`docs(09): PR-A opened as paiml/aprender#NNNN`). Report: PR URL, the receipt table, the two comment URLs, and what waits on the #4214 answer.
