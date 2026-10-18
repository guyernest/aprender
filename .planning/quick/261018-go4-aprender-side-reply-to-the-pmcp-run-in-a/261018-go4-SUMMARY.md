---
phase: quick-261018-go4
plan: 01
subsystem: pmcp-run-hand-off
tags: [document-task, evidence, model-ladder, hand-off]
requires:
  - 09-PMCP-RUN-HANDOFF.md
  - the pmcp.run brief 2026-10-18-in-account-llm-model-ladder-aprender-brief.md
provides:
  - .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-PMCP-RUN-MODEL-LADDER-REPLY.md
  - .planning/quick/261018-go4-aprender-side-reply-to-the-pmcp-run-in-a/261018-go4-FACTS.md
affects: []
key-files:
  created:
    - .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-PMCP-RUN-MODEL-LADDER-REPLY.md
    - .planning/quick/261018-go4-aprender-side-reply-to-the-pmcp-run-in-a/261018-go4-FACTS.md
  modified: []
decisions: []
metrics:
  completed: 2026-10-18
  tasks: 3
  files: 2
status: complete
commits: 1
plan_head_before: 3330a4142e91a4c57dd5b6bda13eb9982ea702f7
plan_head_after: 503686883d947ffe95c1fb6cdcecb809385e4914
actuals:
  tokens: 17007
  tasks: 3
  commits: 1
---

# Quick 261018-go4: Aprender reply to the pmcp.run model-ladder brief

**One-liner:** A 3,062-word, nine-answer reply (Ask 1-4, Question 1-5) grounded in the repo and read-only GitHub, with a 63-row facts table, that corrects two premises of the brief: #5451 is closed unmerged (its change now rides in #5544) and the CUDA-router issue is already filed (#5478).

## Outcome

All three tasks done. The scope-mode gate prints `GATE OK []`. The reply and the facts table are committed as `503686883` (one commit, on `gsd/phase-2-contract-gate`, not pushed). Nothing was deployed, run on AWS, or written to GitHub. No status is VERIFIED outright: every item is PARTLY because each has a part that rests on reading code without running it, or on a fact no source settles.

| Item | Final status | The answer in one line |
|---|---|---|
| Ask 1 | PARTLY | 2B Lambda is a pin change plus staged weights, but the recipe cannot name `aprender-llm-<size>-<host>` and a second run replaces the first; no Fargate recipe exists |
| Ask 2 | PARTLY | No GPU recipe; no instance family can be named from repo evidence; the five CUDA-path routers ignore `APR_API_KEY_HASH` (#5478) |
| Ask 3 | PARTLY | Qwen3.5 9B/27B green in the 0.69.1 receipts on RTX 4090 and GB10 hosts; tool calling exercised only on Qwen3.5 0.8B/2B/4B |
| Ask 4 | PARTLY | #5475 and #5476 exist (open, filed 2026-10-17); no date for the release with the gate |
| Question 1 | PARTLY | Only 0.8B deployed and measured; 2B and 4B are a pin change away but untried |
| Question 2 | PARTLY | Deploy fresh; spike functions differ in release, naming, role, bucket and checks; existence UNKNOWN |
| Question 3 | PARTLY | x86_64 and aarch64 CUDA builds are released; no record covers A10G or T4G devices |
| Question 4 | PARTLY | No `/ping` or `/invocations` in the server; no shim, SageMaker container or ECS recipe in the repo |
| Question 5 | PARTLY | No date committed; #5451 closed, #5544 open with the gate, no tag contains it; latest tag v0.71.4 equals our pin |

Tracer (Task 1, Ask 4 and Question 5) passed its gate before expansion; no checkpoint was needed.

## Contradictions found (against the brief, the task description and the 2026-10-16 hand-off)

1. **#5451 is CLOSED unmerged** (2026-10-18T10:28:20Z, WIP-cap clearance; branch kept). The hand-off and brief treat it as the open PR to await. The same GGUF-router gate is in **#5544** (open, supersedes #5411) and in #5411's head. **#5412** (alias) is also closed, as superseded. The re-pin therefore waits on a release containing #5544, not #5451.
2. **The CUDA-router issue is filed**: #5478 (open, milestone 0.73.0), plus #5479 (malformed key hash fails open; fold PR open). The hand-off says "drafted, not filed". The second draft (auth-contract drift) shows no matching issue.
3. **"No date is committed anywhere citable"** (task premise) is only partly true: GitHub milestones carry train plan dates (0.72.0 due 2026-10-17, no tag yet; 0.73.0 due 2026-11-06, "plan dates, not measured"). None is attached to #5544. The reply reports both and offers no date.
4. **Tickets for the asks exist**: #5475 and #5476 were filed on 2026-10-17, after the hand-off; spikes 030 and 031 say the CPU path has no ticket. An older closed ticket, #4228 (NOT_PLANNED, 2026-09-25), measured a batched CPU Q4_K_M prefill on Qwen3.5-4B (20.8-25.1 tok/s against 5.0-6.2 per token on a 48-thread host); its code and test are not in the tree.
5. **Spike 030 says batched CPU prefill is engaged by default** and gives no speedup; the source at this commit has it cuda-only (`qwen35_session.rs:458`), and the identical A/B timings are what a per-token CPU path gives.
6. **"2B is the smallest certified size; #4030 is why 0.8B is not"**: #4030 is closed (2026-09-25) and the 0.69.1 receipts show the 0.8B rung passing qa. The reply repeats the repo's wording as the repo's wording and says it was not re-derived.
7. The deployed 0.8B function is on release v0.71.4 as baseline profile; the spike functions are on 0.70.3: neither has the gate, as the hand-off says.

## Fixes made while re-deriving claims against original sources (Task 3 step 1)

| Item | What was wrong | Now |
|---|---|---|
| Line cites in Ask 1, Question 1-3, 5 | Several cited lines past the end of the file or the wrong line (spike 031 README :231/:150/:136, spike 030 :148/:149/:117, pin.env :3, entrypoint :17, fargate_run :25) | Corrected to :214/:149, :88/:90/:116, pin.env:5, entrypoint:29, fargate_run:28 |
| Question 5 headline | "No date is committed anywhere citable" overstated, since milestones carry plan dates | "No date is committed for the release that carries the auth gate" |
| Ask 2 | "Your default is correct" read as a recommendation | "Your default stands ... provided nothing else can reach the host" |
| Question 3 | "build comes from an RTX 4090 host" implied the binary targets that host | "built on ... (no GPU is needed to build)", plus the matrix line `binary-release.yml:241` |
| Ask 3 | "Other architectures load on the ladder" included tinyllama, which is receipt inventory, not a rung | "appear in the ladder or the receipts" |
| Cadence method | Rc tags were excluded without saying so | Stated "release tags without an rc suffix"; upstream tag sets compared with local ones (equal) |
| Facts T1-14, T1-20 | Said "no milestone or absent" for #5411 without checking; stale head | Checked milestone null; current head 43afb3e178 recorded |
| Facts T2-25, T2-36, T2-31, T2-30 | auth.rs and qwen35_session.rs line numbers off; prefix-reuse values rounded; cold-start S3 pull attributed to the wrong call | Corrected from the files and the JSON |
| Baseline | `.planning/quick/` collapsed to a directory before the FACTS file existed, so the scope check flagged the pre-existing PLAN.md as stray | Expanded the baseline to the PLAN.md line with an explanatory note and amended the unpushed commit; the gate itself is unchanged |

## Claims left UNKNOWN or not measured, and what would settle each

- Release date for the gate: nothing citable; a merge of #5544 and a tag would settle it.
- Whether the spike-031 functions still exist: one read-only AWS list (no AWS call allowed here).
- Whether the current entrypoint and image run on Fargate: one task run.
- Whether an A10G (sm_86) or T4G (sm_75) device runs the released CUDA builds: a dogfood cell on each; also `cuobjdump` on the arm64 CUDA tarball would test spike 030's secondary claim about embedded PTX.
- Tool calling on any non-Qwen model: a probe through `/v1/chat/completions`.
- Whether a CUDA forward for qwen35moe shipped (#3977 is closed, the ladder entry is unchanged): a ladder sweep.
- SageMaker's `/ping` and `/invocations` requirement and its per-invocation limit: the SageMaker documentation (not in this repo).
- Everything in "Not measured" in the reply (4B on Lambda, GPU agent turn, x86 hosts, multi-conversation reuse, tool-call reliability on a fixed set for 2B and 4B).

## Hard-limit confirmation

- **L-01 (no AWS, no recipe):** The Commands run list in the facts file holds only git, gh GET reads, python3 -I file reads, `ladder_pin.py` (a read-only script, not a recipe), grep and sed. No `aws`, `docker` or `just llm-endpoint-*` command was run; the gate rejects those in the list and passed.
- **L-02 (no GitHub writes, no push):** only `gh issue view`, `gh pr view`, `gh issue/pr list --search`, `gh release view/list` and `gh api` GETs; `git ls-remote` and `git fetch --dry-run` as reads. Nothing was pushed (the commit is local).
- **L-03 (no code changes):** scope-mode gate: HEAD touches exactly the reply and the facts table; no tracked file modified; no untracked file beyond the baseline plus this SUMMARY.
- L-04 to L-07: every issue number, date, price, hex string and hardware token in the reply is a facts row (gate-enforced); no instance type or size is recommended; the one price for g5g is labelled an unverified list-price note; the reply has a measured and not-measured section.

## Deviations from Plan

None to the plan's tasks. One process note: the plan's `gsd_run query init/state` calls were not run (quick task; known gsd-tools failure on the Phase 4 verification file left untouched as instructed); STATE.md, ROADMAP.md and PLAN.md were not edited. The commit used plain git, not the `query commit` helper.

## Known Stubs

None. No code was written.

## Threat Flags

None. No new network endpoint, auth path or schema was introduced.

## Self-Check: PASSED

- FOUND: .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-PMCP-RUN-MODEL-LADDER-REPLY.md
- FOUND: .planning/quick/261018-go4-aprender-side-reply-to-the-pmcp-run-in-a/261018-go4-FACTS.md
- FOUND: commit 503686883 (ancestor of HEAD); scope gate `GATE OK []`
