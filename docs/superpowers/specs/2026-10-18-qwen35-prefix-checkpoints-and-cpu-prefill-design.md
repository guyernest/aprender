# Qwen3.5 prefix reuse and CPU prefill in `apr serve`: options and a build order

Status: design for discussion, 2026-10-18. Nothing here is built. Every code fact below was
read on `upstream/main` (`e293f183b`, version 0.71.3) and checked against the deployed tag
`v0.71.4`; this fork's checkout is 0.69.3 and lacks most of it, so a contribution is cut from
`upstream/main`, not from here.

## 1. Intent

pmcp.run runs one self-hosted LLM per agent. Across an agent's calls the instructions and the
tool schemas are the same bytes in the same order, so the token prefix shared between calls is
most of the prompt. Prefill is the cost on a CPU host, and it runs at the decode rate:

| Measured (Qwen3.5-0.8B Q4_K_M, Lambda arm64 Graviton3, apr 0.71.4) | Time |
|---|---|
| ~420-token prompt, no reuse (A, then B with a different user line) | 101.9 s each |
| identical prompt repeated | 1.8 s |
| conversation turn 2 that extends turn 1 (tool-call turn) | 4.6–8.1 s |
| pmcp.run's brief: six-tool agent, 881 tokens, cold | 125–250 s |
| pmcp.run's brief: team of one, 1,324 tokens, cold | 343 s (past the 280 s transport limit) |

The goal is to make every call after the first pay only for its own new tokens, on CPU and GPU
hosts alike, with changes small enough to land upstream in 0.73.

## 2. What upstream already has (and does not)

| Mechanism | State on `upstream/main` 0.71.3 and `v0.71.4` |
|---|---|
| Session prefix reuse | `Session` (`crates/aprender-serve/src/session.rs`) keeps `processed`; a prompt that strictly extends it prefills the suffix. |
| #4214 checkpoint | ONE copy per session. `Qwen35Forward::checkpoint_at` picks the position before the prompt's last `<|im_start|>` (the generation header). `prepare_prompt` restores it when a prompt does not extend the state but starts with the checkpoint's tokens. The copy (`Qwen35Checkpoint`) holds only the recurrent half, conv windows plus DeltaNet states, and `kv_len`; KV rows stay in the state and are rewound by `OwnedQuantizedKVCache::truncate`. CPU and CUDA both have it. |
| Multi-entry reuse | CUDA batch scheduler only: `Qwen35BatchEngine` (#5279 P7) keeps up to 16 finished slots' device states, each with a copy at the same single boundary. Nothing on the CPU session. |
| CPU batched prefill | `Qwen35Model::forward_prefill_qwen35` exists (#4228, folded in #4655): layer-major over 64-token chunks, Q4_K/Q5_K/Q6_K through the multi-row K-quant×Q8_K GEMM, bit-identical to the per-token path (FALSIFY-4228-004/005). **No production caller.** `Qwen35Forward::try_forward` batches only under `#[cfg(feature = "cuda")]`; the CPU session loops `forward_single_qwen35` per token. Same on `v0.71.4`. This is #5475 exactly. |
| Reporting | `Turn::reused` exists and `apr chat` prints it. `apr serve` reports `timings.prompt_n` as the whole prompt and has no `usage.prompt_tokens_details.cached_tokens`. #4214's done-when (2026-10-15 comment) is "a repeated-prefix shot shows `timings.prompt_n < prompt_tokens`, measured by #5305's harness". |
| Tickets | #4214 reopened 2026-10-15 by a maintainer comment naming "0.73 under #3598, owner `aprender-df`, branch from `8f601caff2`"; the issue's own fields were not updated: milestone `backlog`, label `parked`, no assignee, no branch, no PR (checked 2026-10-18). #5475 (CPU per-token prefill) and #5476 (shared prefix lost when the tail differs) filed 2026-10-18, milestone 0.73.0, P2, unassigned; that milestone holds 232 open issues, so membership is a bucket, not a commitment. #5305 is the pp/tg harness. #4228 closed, parked. |

## 3. The Qwen3.5 constraint

Every fourth layer is full attention; the other three of four are Gated DeltaNet. The attention
KV cache is additive in position and can be rewound by truncation. The DeltaNet state is a
fixed-size recurrence that cannot be rewound at all, so a prefix cache for this architecture is
"truncate the KV rows, copy back the recurrent half as it was at the boundary". The upstream
checkpoint is already shaped this way. What makes it cheap is that the recurrent half does not
grow with the prefix:

| Model (HF `text_config`) | Recurrent half per checkpoint, as copied today | Trimmed to DeltaNet layers | KV per cached token | 900-token prefix, KV only |
|---|---|---|---|---|
| 0.8B: 24 layers (18 DeltaNet, 6 attention), 16 k-heads, 16 v-heads, 128×128, kv 2×256 | 25.7 MiB | 19.3 MiB (18 MiB DeltaNet, 1.3 MiB conv) | 24 KiB | 21 MiB |
| 4B: 32 layers (24 + 8), 16 k-heads, 32 v-heads, 128×128, kv 4×256 | 67 MiB | 50 MiB (48 + 2.3) | 64 KiB | 56 MiB |

"As copied today" is what `Qwen35State::checkpoint()` clones: `Qwen35State::new` allocates a
conv window and a DeltaNet state for every layer, attention layers included (they stay zero),
and the copy takes all of them. Copying only the DeltaNet layers is a free quarter off, and
PR-C should take it.

Two consequences the design relies on:

- **Where to snapshot.** vLLM's hybrid prefix caching snapshots the Mamba state only on a block
  grid, and on Qwen3.5-4B that grid is large enough that prompts under ~528 tokens got no hits
  at all (vLLM issue #40696); the fix there is an opt-in checkpoint at the shared-prefix
  junction. For agent traffic the natural boundaries are the chat template's message starts,
  `<|im_start|>`, which upstream already locates by token id. Snapshot there, not on a grid.
- **Numerics.** On the CPU the batched prefill is bit-identical to the per-token forward, and a
  checkpoint restore copies exact floats and truncates rows, so a turn served from a checkpoint
  produces the same logits as a turn prefilled from zero. pmcp.run observed tool decisions
  flipping on a 2-token prompt change; a cache that changed numerics would add a second source
  of flips. On CUDA a prefill split at a boundary runs cuBLAS over different row counts, so
  parity is bounded (the F2 guard's tolerance), not bitwise. This is a stated test obligation
  (§7), not an assumption.

## 4. Options

| # | Option | Platform | What it buys | Cost / risk |
|---|---|---|---|---|
| O1 | **Wire `forward_prefill_qwen35` into the CPU session.** In `try_forward`, a span of more than one token on `Backend::Cpu` goes through the batched prefill; `APR_QWEN35_SESSION_PREFILL=per-token` keeps the old path; `batched_prefills` counts it; the serve log prints the same `[qwen35] batched prefill` line the GPU prints. | CPU (Lambda, Fargate, LMI) | The first turn and every non-cached suffix. #4228's own measurement was 20.8–25.1 tok/s vs 5.0–6.2 on a 48-thread x86 host; the gain on 6 Graviton3 vCPUs is **unmeasured** and is the first thing to measure (the ignored `qwen35_prefill_throughput_probe` already exists). | ~40 lines. Bit-identity is already proven by FALSIFY-4228-004/005. Memory: a 64-row chunk of activations per layer, small. |
| O2 | **Checkpoint stack.** Generalise the #4214 single copy to a stack of copies at every `<|im_start|>` of the prompt (capped, keeping the shallowest and the deepest). `prepare_prompt` restores the deepest checkpoint that is a prefix of the new prompt; `step` from position `start` drops every checkpoint deeper than `start`. Arch-agnostic in `Session`; `ArchForward::checkpoint_at` becomes "checkpoint points", `save`/`restore` take an index. Both backends reuse their existing copy code (`Qwen35State::checkpoint`, `Qwen35CudaModel::save_checkpoint`). | CPU and CUDA session | A different conversation that shares the system+tools block resumes from the block's end (the case #5476 measured at 101 s today). A tool-call reply that re-renders differently resumes from the last message boundary, not from zero. A warm-up call that carries only the system block leaves a usable checkpoint. No KV copies at all: rows below the restored point are still in the state. | 26 MiB (0.8B) or 67 MiB (4B) per checkpoint as copied today, 19 / 50 MiB trimmed; a cap of 8 is 150–535 MiB. Only ONE chain is held: X, Y, X re-prefills X's tail (not its prefix). Changing `checkpoint_at` into checkpoint points also touches the batch engine's `batch_checkpoint_at` call site. Needs the owner of #4214 on board; the done-when there is unchanged by this. |
| O3 | **Report reuse.** `usage.prompt_tokens_details.cached_tokens = Turn::reused` on `/v1/chat/completions` (the OpenAI field pmcp.run's clients already parse), and `timings.prompt_n` = tokens actually prefilled, with the reused count beside it. | all | pmcp.run's per-call fingerprint can log hit or miss per call, and its replay harness can tell a numerics change from a cache miss. Satisfies #4214's done-when wording directly. | ~30 lines plus a test on the streamed terminal chunk. |
| O4 | **Persisted checkpoint for a cold start.** Serialize the shallowest checkpoint plus its KV rows `[0, k)` to a file keyed by model sha256, apr version, target triple and the token prefix; `apr serve --checkpoint-dir DIR` writes it when first taken and, on the first prompt that starts with its tokens, loads it instead of prefilling. The deployer (our Lambda image) syncs DIR with S3 at start. | Lambda cold start; any restart | A 0.8B 420-token system+tools block is ~30 MiB: under a second at Lambda's measured 80–95 MB/s S3 rate, against ~100 s of prefill. It is the only option that touches pmcp.run's 125–343 s cold numbers beyond the 40 s weight pull. | A format and a flag upstream may not want; must refuse on any key mismatch; a Graviton2 host restoring a Graviton3 file is correct only if the forward is bitwise across the two (unverified: key on the CPU feature set until it is). Depends on O2's shallow checkpoint. |
| O5 | **Branch pool on the CPU.** Keep whole states (recurrent half and the KV tail) for several conversations, as P7 does on CUDA. | CPU | X, Y, X without re-prefilling X's tail. | 24–64 KiB per cached token on top of O2. Only matters when one process serves several conversations at once, which pmcp.run's one-agent-per-LLM mapping avoids. Defer. |
| O6 | **Explicit control.** `prompt_cache_key` selecting a named stack, or a `/v1/prefix` pin endpoint. | all | Needed only if one process must hold several agents' prefixes at once. | Deployment pattern already covers it: a `max_tokens: 0` request with the system block warms the stack (works today: budget 0 prefills and decodes nothing). Defer. |
| O7 | **Faster CPU prefill beyond O1.** DeltaNet per value head in parallel (spike 020 measured the per-token DeltaNet loop serial at 288 ms on one thread while five waited); chunk size tuning. | CPU | More tok/s on the suffix and the cold first turn. | Only after O1's measurement says where the time goes on Graviton3. |

GPU hosts: a CUDA `apr serve` without `--batch` serves every chat request from the `Session`,
so O1 is moot there (the batched prefill exists) and O2 and O3 apply as written. With `--batch`
(opt-in, per `serve_plan_output.rs`) untraced chat requests go to the batch scheduler instead,
which never touches the `Session`: there the P7 pool already resumes from one copy per slot,
and the engine counts `reused_rows` but returns only `PhaseTimings` per request. PR-B therefore
also threads the per-request reused count through `CudaBatchRequest`'s timing channel; a P7
stack (O2 inside the batch engine) is a follow-up, not part of PR-C. pmcp.run's one-LLM-per-agent
host does not need `--batch`.

## 5. What a pmcp.run turn would cost

Projection, not a measurement, from the numbers in §1: with O2 a turn after the first on a warm
container pays only its new tokens, which the measured extension path puts at 4.6–8.1 s on the
0.8B Lambda function; the cold first turn still pays the block (about 100 s at 4.1 tok/s) unless
O4 loads it or O1 shortens it. O1's factor on Graviton3 is the open number.

## 6. Build order and the upstream path

Small, separable PRs cut from `upstream/main`, one in flight at a time (our last three were
closed for WIP-cap clearance, not on merit), each with a `contracts/` YAML, FALSIFY tests and a
measurement receipt, as the GEMM contribution was shipped:

1. **Comment on #4214 and #5476 first.** #4214's reopen comment names an owner (`aprender-df`)
   and a done-when that is O3, but the issue is still `backlog`/`parked`/unassigned and as of
   2026-10-18 no PR or `df/*` branch for it exists (checked `gh pr list` and `git ls-remote`).
   Nothing on any of the three tickets lands by itself. The comment offers O3 and the stack,
   and asks which base to cut from, so PR-B is a contribution to their work rather than a
   collision in `Timings`/`PhaseTimings`.
2. **Measure.** Run `qwen35_prefill_throughput_probe` on the 0.8B and 4B Q4_K_M at 6 and
   14 threads on the M4 and on a Graviton3 host (the dev Lambda function, or a `c7g` instance).
   One number decides whether O1 is a PR or a note.
3. **PR-A: O1** (#5475). Wiring plus the log line and a test that the CPU session's
   `batched_prefills` increments. Nobody else owns #5475. **Opened 2026-10-18 as
   paiml/aprender#5710** (branch `perf/qwen35-cpu-session-batched-prefill`, worktree
   `../aprender-prefill-upstream`, three commits on `e293f183b`). Measured on the M4 Pro:
   kernel 2.5–2.65× on 0.8B and 1.8× on 4B; serve TTFT on the 421-token two-tool prompt
   35.0 → 15.3 s at 6 threads, the same tool chosen in both arms. Comments posted the same
   day: #4214 (issuecomment-6256580657, asks which base PR-B cuts from) and #5476
   (issuecomment-6256580847, the stack proposal).
4. **PR-B: O3**, on the base the #4214 owner names. The streamed terminal chunk of
   `/v1/chat/completions` already carries `usage` (spike 030's bench read it there), so
   `cached_tokens` has a home on both shapes; the batch-scheduler count rides the same PR.
5. **PR-C: O2**, after the comment's answer.
6. **Issue, then PR-D: O4**, only after O2 lands; the file format is the part to agree first.

The Aprender-side Lambda image gains from each PR by re-pinning the release that carries it;
nothing in the deploy recipe changes until O4. The schedule risk is review latency, not the
release cadence: upstream tagged v0.70.1, v0.70.2, v0.70.3, v0.71.1, v0.71.3 and v0.71.4
between 2026-10-04 and 2026-10-14, a patch tag every two to three days and a minor about
weekly; the 0.72.0 milestone was due 2026-10-17 and is not yet tagged, and 0.73.0's
2026-11-06 date is a plan date for a 232-issue bucket. A PR merged on `upstream/main` reaches a
tag within days, so a re-pin follows each merge, not the 0.73 date. Nothing reaches a tag
without a PR: no one is assigned to #4214, #5475 or #5476. Only if a PR stalls in review is
there a reason for a second workstream that builds the Lambda image from `upstream/main` plus a
patch queue, which changes the deploy recipe Phase 9 plans 09-08 and 09-10 rest on; this fork
(0.69.3) is not a base to backport onto.

## 7. Test obligations

- CPU: restore-and-extend equals prefill-from-zero equals per-token, bitwise, on the 0.8B and 4B
  (extend FALSIFY-4228-004/005 with a checkpoint in the middle).
- CUDA: the same within the F2 guard's tolerance.
- The stack drops every checkpoint deeper than a rewritten position, and the cap keeps the
  shallowest and the deepest.
- Spike 030's 22 tool requests replayed with the cache cold and warm give byte-identical
  responses.
- O3's `cached_tokens` is 0 on a miss and equals the restored length on a hit, on both the
  blocking and the streamed shape.
- O4 refuses a file whose model hash, version, target or tokens differ, loudly.

## 8. Decisions that are the operator's

1. **The clock.** Upstream PRs only (effect on pmcp.run: a re-pin to the first tag after each
   merge, days at the measured cadence, gated by review time), or also the second workstream in
   §6 that builds the Lambda image from `upstream/main` with a patch queue and changes the
   deploy recipe. Waiting without writing the PRs delivers nothing: no ticket is assigned.
2. Whether to spend the Graviton3 measurement (§6 step 2) before PR-A, or go straight to PR-A
   on the x86 numbers upstream already accepted.
3. Whether O4 (persisted checkpoint) is worth proposing upstream, or should stay an
   Aprender-side deployment feature (a warm-up call after each cold start is the no-code
   alternative, at the same 100 s once per container).
4. Whether pmcp.run's two clients parse `usage` on the streamed terminal chunk (then O3 reaches
   their fingerprint as designed) or only `timings` (then the `timings` fields carry the count
   too). An ask for the pmcp.run team, not a code question.

Calls made in this document rather than asked: O2 and O3 are scoped to the `Session` path, with
the `--batch` scheduler getting only the reuse count (§4); the #4214 comment goes before any PR
(§6 step 1); checkpoints trim to DeltaNet layers in PR-C (§3).

## 9. Left to the builder

The stack cap and eviction policy within "keep shallowest and deepest"; the exact `timings`
field names beside `prompt_n`; the checkpoint file layout; whether O1 also serves `apr run`'s
one-call session (it should, through the same `try_forward`); one call to
`POST /v1/chat/prompt-ids` before PR-C to confirm the rendered prompt carries the `<|im_start|>`
id at every message start, which the stack's boundaries depend on.

Sources read for this document: `upstream/main` `session.rs`, `qwen35_session.rs`,
`forward_qwen35.rs` (`forward_prefill_qwen35`, `Qwen35Checkpoint`), `runtime.rs`,
`qwen35_batch_engine.rs`, `mod_create_demo.rs` (`Timings`); HF `config.json` for
Qwen3.5-0.8B and 4B; spike references `llm-endpoint-agent-turn-cost.md` and
`qwen35-decision-inference.md`; issues #4214, #4228, #5305, #5475, #5476; vLLM docs
"automatic prefix caching" and issue #40696.
