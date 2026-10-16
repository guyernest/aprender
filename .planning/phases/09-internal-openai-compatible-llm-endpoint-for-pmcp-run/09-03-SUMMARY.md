---
phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
plan: 03
subsystem: api
tags: [auth, apr-serve, gguf, upstream-pr, falsify-auth-001]

requires:
  - phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
    provides: 09-01 conformance contract with row FALSIFY-LLM-CONF-002 staged; 09-02 serialized ahead because both edit that contract
provides:
  - build_gguf_router(state, config, auth_gate) seam in apr-cli serve/server.rs; serve_router wires AuthGate::from_env()
  - four oneshot tests (gguf_router_auth_tests) on the real realizar router over a model-free AppState
  - one upstream-only commit on origin branch fix/gguf-router-auth-gate, opened as draft PR paiml/aprender#5451
  - contract row FALSIFY-LLM-CONF-002 bound to its tests, scope exception extended
affects: [09-04, 09-07, 09-08]

plan_head_before: cdbe4ed4fd73e51ace62287c920e0ca67669291c
plan_head_after: 7e7e111f3
actuals:
  tokens: 5500
  tasks: 3
  commits: 2

tech-stack:
  added: []
  patterns:
    - "gate a router behind a pure seam (build_*_router(state, config, gate)) so the wiring is testable with oneshot and no socket or weights"
    - "upstream PR = one commit in its own worktree cut from upstream/main; phase-only contract binding is a separate chore commit after a cherry-pick -x"

key-files:
  created:
    - .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-03-pr-body-auth.md
    - .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-03-issue-gpu-router-auth.md
    - .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-03-issue-auth-contract-drift.md
  modified:
    - crates/apr-cli/src/commands/serve/server.rs
    - contracts/llm-endpoint-openai-conformance-v1.yaml
    - .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/deferred-items.md

key-decisions:
  - "/health is gated like every other route (FALSIFY-AUTH-001); the Lambda Web Adapter counts any 100-499 readiness status as ready, so a 401 still admits the function"
  - "The CUDA-path router call sites stay ungated in this PR (CPU-only deployment, no CUDA host to test); recorded in contract row 002 and deferred-items.md"
  - "Only the PR was opened (user answer open-draft); the two drafted upstream issues were deliberately not filed"

requirements-completed: [IR-4, SC-3, SC-5]

coverage:
  - id: D1
    description: "GGUF router answers 401 + {error:unauthorized} + WWW-Authenticate: Bearer without or with a wrong key on chat, /health and /v1/models; admits the right key; disabled gate stays open"
    requirement: SC-3
    verification:
      - kind: unit
        ref: "crates/apr-cli/src/commands/serve/server.rs (commands::serve::handlers::gguf_router_auth_tests, 4 passed in the upstream worktree and on the phase branch)"
        status: pass
    human_judgment: false
  - id: D2
    description: "apr-cli gates on the phase branch: full lib suite, three falsify_auth binaries, fmt, clippy delta, nextest enumeration"
    requirement: SC-5
    verification:
      - kind: other
        ref: "cargo test -p apr-cli --lib -> 7521 passed, 0 failed, 14 ignored; falsify_auth_001/002/003 -> 4/3/4 ok; fmt clean; clippy 0 vs baseline 0; nextest lists 4 gguf_router_auth_tests"
        status: pass
    human_judgment: false
  - id: D3
    description: "Upstream PR purity: one commit over the merge-base, only server.rs changed, pushed"
    requirement: IR-4
    verification:
      - kind: other
        ref: "git rev-list --count abf66f908..HEAD = 1; git diff --name-only = crates/apr-cli/src/commands/serve/server.rs; ls-remote origin equals HEAD; worktree clean"
        status: pass
    human_judgment: false

duration: not recorded (36,215 s wall clock including a usage-limit pause)
completed: 2026-10-16
status: complete
---

# Phase 9 Plan 03: GGUF router auth gate Summary

**`apr serve` on a GGUF (including the Qwen3.5 hybrid and the embedding server) now serves realizar's router behind `AuthGate::from_env()` through a testable `build_gguf_router` seam; the fix is one upstream-only commit, opened as draft PR paiml/aprender#5451, and carried onto the phase branch by `cherry-pick -x` with contract row 002 bound in its own commit.**

## Pull request (Task 3 outcome)

User answer at the checkpoint: `open-draft`. The orchestrator opened the PR, not me, and I did not run `gh pr create` or `gh issue create`. Verified by the orchestrator: OPEN and draft.

- PR: https://github.com/paiml/aprender/pull/5451 (`guyernest:fix/gguf-router-auth-gate`)
- The two drafted issues are saved but **deliberately not filed** (see deferred-items D-09-03-1).

## Commits

| What | Upstream SHA | Phase-branch SHA |
| ---- | ------------ | ---------------- |
| Seam, wiring, four tests | `f4ad4c06da14df1992aabf88667bd64c6b1f9015` | `489246e11` (cherry-pick -x, no conflict) |
| Row FALSIFY-LLM-CONF-002 binding | n/a (phase-local) | `7e7e111f3` (contract only) |

`UP_SHA` when the worktree was cut: `abf66f9086590f5ca9a5d09fe3122ccb1ffff6d7`. By the time of the checkpoint `upstream/main` was `f8e372b78362d4682ad148a8e1b9703ee75dc600`; the diff between the two touched nothing under `apr-cli/src/commands/serve/`, so the branch still applies and the merge-base is `abf66f908`.

`commits: 2` counts this plan's phase-branch commits. `plan_head_before..HEAD` is larger because plan 09-13 ran concurrently and its commits and a merge landed in between.

## Evidence

- **RED:** `error[E0425]: cannot find function 'build_gguf_router' in this scope` (`server.rs:447`).
- **GREEN:** `gguf_router_auth_tests`: 4 passed, 0 failed. The plan's assumption that `/health` returns 200 on a model-free `AppState::demo_mock()` was wrong: it returns 503 with `{"status":"loading"}`. The first GREEN run had 2 failures for that reason; I changed the two assertions to `!= 401` plus `status == "loading"`, which still proves the handler was reached and not the gate.
- **Upstream worktree (`aprender-wt-pr-auth`):** falsify_auth_001/002/003 4/3/4 passed; `cargo fmt --all -- --check` clean; clippy 0 errors vs pristine baseline 0.
- **Phase branch:** `cargo test -p apr-cli --lib` 7521 passed, 0 failed, 14 ignored; the three falsify_auth binaries ok; fmt clean; clippy 0 vs baseline 0 with nothing naming `server.rs`; `cargo nextest list` shows 4 `gguf_router_auth_tests::`; `scripts/check_contract_test_binding.sh` PASS with no finding naming the conformance contract; contract YAML parses and row 002 has its `test:` line.
- The 401 envelope is unchanged; `routes.rs` and `contracts/apr-serve-api-key-auth-v1.yaml` are untouched.

## Deviations from Plan

1. **[Rule 1 - Plan inaccuracy] `/health` on the model-free state is 503, not 200.** See Evidence. Two test assertions changed; the claim they prove is unchanged.
2. **[Rule 2 - Scope exception wording] The CUDA-path call sites are five, not four.** They are `handlers_include_01.rs:86/197/283`, `handler_gpu_completion.rs:753` and `server.rs:563` (`start_gguf_server_gpu_batched`), all behind `feature = "cuda"`. The plan and row 002 named only the first two files. The binding commit adds `start_gguf_server_gpu_batched` to row 002's `rule`; the two file names the plan's verify parses are still present.
3. **[Finding] The auth-contract drift is wider than the plan assumed.** `build_gpu_router` has no caller; the only production `AuthGate::from_env()` in `serve/` was `handlers.rs:1519`; `routes.rs::create_router` uses `AuthGate::disabled()` and has no production caller; `--auth-disabled` exists only in a doc comment. The PR body does not repeat the plan's claim that the SafeTensors router was already gated.
4. **[Process] Agent-branch commit assertion not applied in the upstream worktree** (the PR branch name is mandated by the plan); the protected-branch check passed. No `git clean`, `stash` or `update-ref`. The rtk hook swallowed some libtest summary lines, so I read the logs directly.
5. **[Process] Clippy baselines** were taken in both trees before any edit: 0 and 0 (the `isa.rs` warning on upstream is a warning, not an error under `--no-deps` for apr-cli).
6. **[Housekeeping] Build cache removed** after the checkpoint: `~/.cache/aprender-p09-pr-target` (7.7 GB) was referenced only by plans 09-02 and 09-03, both complete, and no process held it. The three `aprender-wt-pr-*` worktrees stay on disk.
7. **[Housekeeping] Drafts copied into the phase directory** for durability: `09-03-pr-body-auth.md`, `09-03-issue-gpu-router-auth.md`, `09-03-issue-auth-contract-drift.md`.

## Gaps

- **No mutation check was run.** I did not remove the `auth::layer` wrapper to confirm the 401 tests fail without it; each rebuild takes minutes. The RED step proves the tests need the seam, and the 401 assertions cannot pass on an ungated router, but that is reasoning, not an executed mutation.
- Both `build_gguf_router` and the CUDA exception are proven only on CPU. The deployed 401 proofs (plans 09-07 and 09-08) are what cover the route that actually serves.
- The server still starts open when neither `APR_API_KEY_HASH` nor `APR_API_KEY` is set (existing `from_env` semantics, stated in the PR's Known gap). Mitigation is plan 09-04's entrypoint refusal; it is not yet verified here.

## Deferred Issues

See `deferred-items.md`: D-09-03-1 (issue drafts unfiled), D-09-03-2 (five CUDA-gated routers ungated), D-09-03-3 (auth contract drift).

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-09-07 is mitigated (the test mounts chat, `/v1/models` and `/health` through the real seam), T-09-45 is mitigated (audited one-commit, `server.rs`-only branch), T-09-46 is transferred and recorded in row 002 and deferred-items.

## State updates

Recorded in the commit that adds this file; any `state.*` failure on the known Phase 4 `04-VERIFICATION.md` blocker is listed in the completion report, and that file was not touched.

## Self-Check: PASSED

- `489246e11` and `7e7e111f3` are ancestors of HEAD; `f4ad4c06d` equals origin's `fix/gguf-router-auth-gate`.
- `crates/apr-cli/src/commands/serve/server.rs` contains `fn build_gguf_router(`.
- The three draft files and this summary exist in the phase directory.
