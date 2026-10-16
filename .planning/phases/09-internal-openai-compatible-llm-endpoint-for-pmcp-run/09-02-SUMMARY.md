---
phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
plan: 02
subsystem: api
tags: [serde, openai-api, upstream-pr, pmat-801, try_from]

requires:
  - phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
    provides: plan 09-01's conformance contract with rows 001 and 003 staged (no test: key)
provides:
  - role-aware ChatMessage request deserialization (assistant message with absent or null content reads as "") in aprender-serve
  - max_completion_tokens accepted as an alias of max_tokens on ChatCompletionRequest
  - two independent upstream-only branches on origin, each opened as a draft PR to paiml/aprender
  - conformance contract rows FALSIFY-LLM-CONF-001 and -003 bound to their tests
affects: [09-03, 09-05, 09-08, 09-12]

plan_head_before: 58402179ecc1a051f6e1957fc1f49df99d7f2015
plan_head_after: b34eb968862b3010edc9a21a88e1e0fefd345848
actuals:
  tokens: 6900
  tasks: 3
  commits: 4

tech-stack:
  added: []
  patterns:
    - "serde try_from a private wire struct when a field rule depends on a sibling field (role-aware content)"
    - "upstream PR = one commit per fix, own worktree cut from upstream/main, own dedicated test file; phase-only contract edits live in separate chore commits"

key-files:
  created:
    - crates/aprender-serve/src/api/tests/chat_message_content_pmat801.rs
    - crates/aprender-serve/src/api/tests/max_completion_tokens_alias.rs
    - .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/deferred-items.md
  modified:
    - crates/aprender-serve/src/api/mod_create_demo.rs
    - contracts/llm-endpoint-openai-conformance-v1.yaml

key-decisions:
  - "Content tolerance uses a ChatMessageWire + TryFrom wire struct, not a field-level deserialize_with: a per-field deserializer cannot see the sibling role and would also admit a user message with no content as an empty prompt (review round 1, MEDIUM; threat T-09-44)"
  - "The alias is the plain serde alias, so a body with both names is refused with `duplicate field`; observed, pinned, and written into the contract instead of the draft's wrong 'max_tokens wins'"
  - "Both PRs opened as drafts only after the blocking-human checkpoint; user answered open-both-drafts"

requirements-completed: [IR-1, SC-2, SC-5]

coverage:
  - id: D1
    description: "pmcp-agent turn-2 body (assistant message with tool_calls and no content key) parses; user/system/tool without content and numeric content still refused; serialization bytes unchanged"
    requirement: SC-2
    verification:
      - kind: unit
        ref: "crates/aprender-serve/src/api/tests/chat_message_content_pmat801.rs (api::chat_message_content_tests, 5 passed on the upstream worktree and on the phase branch)"
        status: pass
    human_judgment: false
  - id: D2
    description: "max_completion_tokens aliases max_tokens; both-names behaviour pinned"
    requirement: IR-1
    verification:
      - kind: unit
        ref: "crates/aprender-serve/src/api/tests/max_completion_tokens_alias.rs (api::max_completion_tokens_alias_tests, 3 passed)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Crate gates on the phase branch with both fixes: aprender-serve lib suite, fmt, clippy, contract binding checker"
    requirement: SC-5
    verification:
      - kind: other
        ref: "cargo test -p aprender-serve --lib -> 16129 passed, 0 failed; fmt clean; clippy 0 errors (baseline 0); scripts/check_contract_test_binding.sh PASS"
        status: pass
    human_judgment: false

duration: not recorded
completed: 2026-10-16
status: complete
---

# Phase 9 Plan 02: Content tolerance and max_completion_tokens alias Summary

**An assistant tool-call history turn with no `content` key (the exact pmcp-agent turn-2 body that returned 422) now parses via a role-aware serde `try_from`, and `max_completion_tokens` aliases `max_tokens`; each fix is one upstream-only commit, verified in its own upstream/main worktree and opened as a draft PR to paiml/aprender.**

## Performance

- **Duration:** not recorded (no start time was captured)
- **Tasks:** 3 (Task 1 tracer, Task 2 auto, Task 3 blocking-human decision)
- **Phase-branch commits for this plan:** 4 (`58402179e..b34eb9688`)

## Pull requests (Task 3 outcome)

Checkpoint answer from the user: `open-both-drafts`. The orchestrator opened both; I did not run `gh pr create`. Re-checked by me: both OPEN and `isDraft: true`.

| PR | Branch | Upstream commit |
| -- | ------ | --------------- |
| https://github.com/paiml/aprender/pull/5411 (content) | `guyernest:fix/chat-message-content-optional` | `de62ce561768fa05010ceb4d84517803c659b3ea` |
| https://github.com/paiml/aprender/pull/5412 (alias) | `guyernest:fix/max-completion-tokens-alias` | `31db0c39a9466faaec209181c031f11a0852c361` |

Both are one commit over `UP_SHA` = `5b1fd7b085f59c3fb33fde0373d298745abc3046` (upstream/main, re-fetched; the plan's `2f3da5112` was stale), touch only `crates/aprender-serve/` (the source and the fix's own test file), and the alias branch does not contain the content commit. The PR bodies (What is in / What is not in / Known gap / Verification) were drafted at `<scratchpad>/pr-body-content.md` and `<scratchpad>/pr-body-alias.md`; key claims:

- Content PR: wire struct chosen over `deserialize_with` because the latter cannot see `role`; other roles still refuse a missing content; the one full-suite failure is a pre-existing host-specific parity test.
- Alias PR: both names present is a `duplicate field` 422, with an offer to switch to "max_tokens wins" via a manual `Deserialize` if the maintainer prefers.

## Commits

| What | Upstream SHA | Phase-branch SHA |
| ---- | ------------ | ---------------- |
| Content fix | `de62ce561768fa05010ceb4d84517803c659b3ea` | `26adb849881e9f19af1a331cd29e57f7491ff527` (cherry-pick -x, no conflict) |
| Row FALSIFY-LLM-CONF-001 binding | n/a (phase-local) | `c3e985115b2b5613569c13dfa0b5030167083cee` (contract only) |
| Alias fix | `31db0c39a9466faaec209181c031f11a0852c361` | `b9bd4516566a33e664960f94219aab69d64fc47b` (cherry-pick -x, conflict resolved by hand) |
| Row FALSIFY-LLM-CONF-003 binding | n/a (phase-local) | `b34eb968862b3010edc9a21a88e1e0fefd345848` (contract only) |

`plan_head_after` is my last task commit. HEAD has since moved: `f665b2212` (a 09-01 refactor of the conformance reader) and ROADMAP tracking commits landed on top, so a raw `git rev-list --count plan_head_before..HEAD` now exceeds 4. The 4 is the measured count of this plan's own commits. No per-plan ledger file was created, because the earlier isolated attempt was blocked and this run started mid-plan.

## Task 1 evidence (content fix)

Fixture: request #3 of `wire-0.8b-plain.jsonl` (line 4 of the file, `"n": 3`; the draft test file already had the right body and system prompt, copied verbatim).

1. **RED** (tests present, fix absent), in worktree A:
   - `assistant_message_without_content_parses_pmat801_followup`: `turn-2 body must parse: Error("missing field \`content\`", line: 0, column: 0)`
   - `assistant_message_with_null_content_parses`: `Error("invalid type: null, expected a string", line: 0, column: 0)`
   - `non_assistant_message_without_content_is_refused`: also FAILED, on the null case (`role user, null: got invalid type: null, expected a string`). The plan expected Tests 3-5 to pass before the fix; Test 5's null half did not, because the old null error does not mention `content`. After the fix both absent and null report `missing field \`content\``. Same 422.
   - Passing before the fix: `chat_message_serialization_is_byte_identical`, `numeric_content_is_still_refused`. `test result: FAILED. 2 passed; 3 failed`.
2. **GREEN:** `test result: ok. 5 passed; 0 failed`.
3. **Mutation** (`None if wire.role == "assistant"` changed to `None if true`): `non_assistant_message_without_content_is_refused ... FAILED` (`absent content must be refused: ChatCompletionRequest { ... role: "user", content: "" ...`), `4 passed; 1 failed`.
4. **Restored GREEN:** file copied back from `/tmp/p09-02-modcreate.good` (`cmp` confirmed), re-run 5/5. The phase branch also re-ran 5/5 (`16182 filtered out`).
5. No pre-existing test pinned the old 422 for an assistant message; the whole worktree-A suite surfaced none (its one failure is unrelated, below).

Gates in worktree A: `cargo fmt --all -- --check` clean; clippy 1 error against a pristine baseline of 1 (identical `isa.rs:171` finding); `cargo test -p aprender-serve --lib` 16471 passed, 1 failed (pre-existing, see Deferred Issues). The upstream commit was pushed to origin and `ls-remote` equalled HEAD.

## Task 2 evidence (alias)

- RED in worktree B (cut from `upstream/main`, not from branch A): `max_completion_tokens_aliases_max_tokens` failed `left: None, right: Some(12)`; `both_max_tokens_names_present_is_documented` also failed (no error before the alias).
- GREEN: `test result: ok. 3 passed; 0 failed`.
- Both names present: serde derive refuses with `duplicate field`; no value wins.
- Gates in B: fmt clean; clippy 1 vs baseline 1 (same finding); focused tests 3/3. Pushed; `ls-remote` equalled HEAD; A's commit is not an ancestor (`merge-base --is-ancestor` rc 1).
- Phase branch, both fixes present: `cargo test -p aprender-serve --lib` 16129 passed, 0 failed (3 alias and 5 content tests in the run); fmt clean; clippy 0 errors vs the phase baseline 0 (`/tmp/p09-02-phase-clippy-base.count`, taken from `58402179e` before any cherry-pick); `scripts/check_contract_test_binding.sh` PASS and no finding names the conformance contract.

## Deviations from Plan

1. **[Rule 1 - Bug in contract text] "max_tokens wins" was wrong.** The contract's `max_completion_tokens_alias` equation and invariant said that with both names present `max_tokens` wins. Serde alias semantics refuse the body with `duplicate field`. Fixed the formula, invariant and precondition in the row-003 binding commit (contract only), marked as observed in plan 09-02. Commit `b34eb9688`.
2. **[Plan inaccuracy] Test 5's null case is RED before the fix** (see Task 1 evidence 1). The test is kept at the plan's assertion (error text mentions `content`).
3. **[Cherry-pick conflict] alias commit in `mod_create_demo.rs`.** The fork's `ChatCompletionRequest` lacks `response_format`, `logprobs`, `top_logprobs` and the `ResponseFormat` family that upstream has, so the patch context pulled that upstream-only block into the conflict. Resolved by keeping the fork's code (HEAD side) and adding only the alias attribute and the `mod max_completion_tokens_alias_tests` registration. The content cherry-pick auto-merged cleanly. Files conflicted: `crates/aprender-serve/src/api/mod_create_demo.rs` only.
4. **[Process] Worktree B clippy baseline** copied from worktree A's (both pristine at `5b1fd7b08`) instead of re-run.
5. **[Baseline reading] The clippy "1 error" on upstream is pre-existing**: `crates/aprender-serve/src/isa.rs:171:14: unused macro definition: cpu_feature` (also in the worktree-A baseline). The phase branch's clippy count is 0.
6. **[Pre-existing failure] `kernel_registry::parity::committed_parity_receipts_hold_on_this_host`** fails in worktree A. It fails identically on pristine `upstream/main` in worktree B and passes on the phase branch. Logged as a deferred item.
7. **[Process] Agent-branch commit assertion not applied in the upstream worktrees.** The pre-commit rule that a worktree HEAD must be on an `agent-*` branch conflicts with the plan's mandated upstream PR branch names; the protected-branch check passed. No `git clean`, `stash` or `update-ref` was used.
8. **[Packaging] Baselines and upstream base differ from the plan text.** `UP_SHA` is `5b1fd7b08` not `2f3da5112`; worktree A `aprender-wt-pr-content` already existed pristine at that SHA and was reused; the missing phase clippy baseline file was produced before cherry-picking.

Also noted: Edit and Write were never refused in the upstream worktrees; the guard concern in the dispatch did not materialise. The rtk hook filtered `cargo test` and `git` output (lines like `ok fix/cha`), so verification used the test log lines and `git ls-remote` / `rev-parse` rather than libtest summary greps in a few places.

## Deferred Issues

See `deferred-items.md` in this directory:

- **D-09-02-1:** `kernel_registry::parity::committed_parity_receipts_hold_on_this_host` fails on this aarch64 host on pristine upstream (`cpu.matvec.q6_k` max_rel_err 1.65e-6 > receipt tolerance 4e-7). Pre-existing, unrelated.
- **D-09-02-2 (observation):** `crates/apr-cli/src/commands/serve/types.rs:479` declares its own `max_tokens: Option<u32>` with no `max_completion_tokens` alias. It is a different request type from the `aprender-serve` one PR #5412 changes. Which type a deployed route deserializes was not traced here; the deployed probe will show whether the field is still ignored live. Never gates the end-to-end turn (D-05).
- **D-09-02-3:** the pre-existing `isa.rs` clippy error on upstream.

## Known Stubs

None.

## Threat Flags

None. The tolerance is confined to an assistant role (T-09-44, mutation-proven), serialization bytes are pinned (T-09-04), the both-names case is pinned (T-09-05), the PRs were opened only after the human checkpoint (T-09-06), and neither upstream commit touches `contracts/` or `.planning/` (T-09-43, audited with `git diff --name-only` against the merge-base).

## State updates

- `roadmap.update-plan-progress 09`: succeeded (plan 09-02 checked, 5/13 plans executed). Committed with this SUMMARY.
- `state.advance-plan`, `state.record-metric`, `state.add-decision` (x2), `state.record-session`: all FAILED on the pre-existing Phase 4 file (`04-VERIFICATION.md` status `gaps_acknowledged`, outside the tool's closed set). That file was not edited. STATE.md is therefore unchanged by this plan; the orchestrator reconciles it. Decisions intended for STATE.md are in `key-decisions` above. `state.advance-plan` also printed a milestone-lock warning (the claim names phase 09, the verb ran against phase 08).
- `state.update-progress`: skipped by the tool ("phase scope is unscoped").
- `requirements.mark-complete IR-1 SC-2 SC-5`: nothing marked, the IDs are not in REQUIREMENTS.md (they are phase-local IDs).

## Self-Check: PASSED

- Files exist: `chat_message_content_pmat801.rs`, `max_completion_tokens_alias.rs`, `deferred-items.md`.
- Commits `26adb8498`, `c3e985115`, `b9bd45165`, `b34eb9688` are ancestors of HEAD.
- Origin branch tips equal `de62ce561` and `31db0c39a`; PRs #5411 and #5412 verified OPEN and draft.
