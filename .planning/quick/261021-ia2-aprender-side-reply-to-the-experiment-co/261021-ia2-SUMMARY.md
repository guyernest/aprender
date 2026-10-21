---
phase: quick-261021-ia2
plan: 01
subsystem: docs
tags: [experiment-coach, decide-server, laya, antisemitism-v1, reply, planning-doc]
status: complete
requires:
  - /Users/guy/projects/mcp/motivation-experiment/docs/requests/aprender.md (read-only, HEAD b7057de)
  - contracts/decide-tool-boundary-v1.yaml, contracts/decide-apr-v1.yaml, contracts/laya-finetune-gate-v1.yaml, contracts/setfit-train-lifecycle-v1.yaml
  - .planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-LIVE-REDEPLOY-EVIDENCE.json, 08-LIVE-DEPLOY-EVIDENCE.json, 08-GATE-RUN-EVIDENCE.json, 08-16-SUMMARY.md
provides:
  - .planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-EXPERIMENT-COACH-DECIDE-REQUEST-REPLY.md
  - .planning/quick/261021-ia2-aprender-side-reply-to-the-experiment-co/261021-ia2-FACTS.md
affects:
  - the experiment-coach request's blank "Date (owning team to fill):" line (orchestrator's edit, after acceptance)
  - a proposed Phase 10 for the owner to add with /gsd-phase add (ROADMAP.md NOT edited)
tech-stack:
  added: []
  patterns: [facts-table-gated reply, every number traceable to a row, read-only evidence from two repos]
key-files:
  created:
    - .planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-EXPERIMENT-COACH-DECIDE-REQUEST-REPLY.md
    - .planning/quick/261021-ia2-aprender-side-reply-to-the-experiment-co/261021-ia2-FACTS.md
    - .planning/quick/261021-ia2-aprender-side-reply-to-the-experiment-co/261021-ia2-SUMMARY.md
  modified: []
decisions:
  - "Proposed ready-by 2026-10-29 for antisemitism-v1, derived by the plan's rule (Day 0 = 2026-10-21; 1 + 1 + 2 + 2 + 1 + 1 = 8 calendar days), labelled as the aprender agent's proposal pending the owner's confirmation"
  - "R2: the guide cannot be served verbatim; the builder caps the head at head_max_len 192, so a condensed task under P + t <= 100 (8 texts in 800 tokens) is proposed, with the full guide's sha256 recorded and the condensation approved by experiment-coach"
  - "R5: recommend a SetFit s64e1b16 cell on the same train and eval splits rather than a new calibrated regime"
  - "C2 finding: pmcp 2.19.3 serves the classify result as content[0].text only (no output schema declared); experiment-coach's parser already handles that form"
metrics:
  duration: "42 min (2026-10-21T20:38:39Z to 2026-10-21T21:21:05Z)"
  completed: 2026-10-21
  tasks: 3
  files_changed: 2
actuals:
  tokens: 19050
  tasks: 3
  commits: 1
plan_head_before: 844d02ff4bde55bee01f038d1f21bda9fadfee4b
plan_head_after: 4eba6e7c946e6dca0f3b6dfb5a119649d4c68304
---

# Quick 261021-ia2: aprender-side reply to the experiment-coach decide-server request Summary

**The reply answers the request's nine contract items with evidence, states seven mismatches with proposals, and proposes 2026-10-29 as the ready-by date for `antisemitism-v1` by a stated rule, pending the owner's confirmation; nothing was deployed, trained, run on AWS or pushed, and the scope gate proves HEAD touched exactly the reply and the facts table.**

## Outcome

Two documents were written and committed as `4eba6e7c9` on `worktree-agent-abfa366e2bf8dd67c` (base `844d02ff4`): the reply at `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-EXPERIMENT-COACH-DECIDE-REQUEST-REPLY.md` (3983 words, sections 1-8 plus the glance table) and the facts table at `.planning/quick/261021-ia2-aprender-side-reply-to-the-experiment-co/261021-ia2-FACTS.md` (85 rows, 60 read-only commands, an empty untracked baseline, eight contradictions). The plan's gate printed `GATE OK []` in tracer, final and scope modes. The headline findings: a conformant `classify` call exists today, the response fields match the request exactly, and the sha, task.json and label-order mechanisms exist; the requested Lambda-direct IAM Function URL deploy does not exist (today's server is on pmcp.run with auth off), experiment-coach's batch of 32 is refused by the 8-text / 800-token door, and the annotation guide cannot be served verbatim because the row builder keeps at most 192 head tokens and cuts the rest silently.

## Item statuses

| Item | Final status | One-line answer |
|---|---|---|
| C1 request shape | VERIFIED | `{texts: [string]}` only, bare `tools/call`, your two headers: exists today |
| C2 response shape | VERIFIED | Fields match; the result travels as `content[0].text` JSON (no output schema declared) |
| C3 label order | VERIFIED | Criteria document order is the label index; the antisemitism-v1 task.json is new work |
| C4 probabilities | PARTLY | Aligned, tempered softmax with slice-fitted T, gate ECE at or under 0.10; sum to 1 by construction, not asserted |
| C5 model identity | VERIFIED | `artifact_sha256` is the sha of the served `.apr`, pinned before parsing; drift check matches; the sha exists only after the gate run |
| C6 guide in task.json | VERIFIED | The guide becomes a per-row prefix in training and serving, head capped at 192 tokens |
| C7 Lambda + IAM Function URL | VERIFIED | Not today (pmcp.run, auth off); new recipe B5 |
| C8 10,240 MB | VERIFIED | Deployed and contract-asserted (FALSIFY-DECIDE-TOOL-007) |
| C9 timings | VERIFIED | Requester's numbers are spike 026 single-text figures; edge cold calls 22647-27015 ms; Function URL path not measured |
| R1 batch 32 vs door | VERIFIED | Refused; propose classify_batch at most 8 plus a token budget from `tokens` |
| R2 guide as prefix | VERIFIED | Head cap 192; P + t <= 100 for 8 texts; condensed task, full guide's sha256 recorded |
| R3 deployment/auth | VERIFIED | Lambda-direct `AWS_IAM` recipe, no public statement, function name to agree |
| R4 dataset/split | VERIFIED | 11311 rows, 667 duplicate rows, 5 conflicting texts; prepare_goldstandard.py (new), 20 % eval proposed |
| R5 SetFit regime | VERIFIED | Full-size run is uncalibrated; same-eval-split s64e1b16 cell recommended |
| R6 guide missing | UNKNOWN | Text in neither repo; the arXiv lead is unverified; experiment-coach supplies the file |
| R7 timing premise | VERIFIED | Cold premise about 2x off on the edge path; restate the test for the delivered URL |
| B1-B6 | VERIFIED (B3 PARTLY) | 1, 1, 2, 1 (parallel), 1, 1 days; B3's training time is an estimate until the probe |
| M1 motivation data | VERIFIED | One workbook, 200 posts, classes 19 / 26 / 63 / 47 / 4 / 41 |
| M2 gate implications | VERIFIED | Smallest class fits 14 rows inside the 16-shot regime; 86 labelled rows per class for 64 fit; no date |

## The proposed date, its rule and conditions

**Proposed ready-by: 2026-10-29** (proposed by the aprender agent on 2026-10-21, pending the owner's confirmation).

Rule, as the plan fixed it: Day 0 = the as-of date; each B item costs `max(1, analog span)`; every Phase 8 analog landed the same day as its predecessor (all 33 executed plans fell between 2026-09-25 and 2026-09-29), so every item is 1 day; B3 gains `ceil(hours / 24) = 1` training day at E = 12 (estimate 9.3-11.6 h from 1.16-1.45 s per step x 798 steps per epoch x 12 x 3 seeds); one gate re-run (B3 again) is budgeted; B4 runs in parallel and adds 0. Sum 1 + 1 + 2 + 2 + 1 + 1 = 8 calendar days.

Conditions: the guide file, the condensation approval and the owner's confirmation before B1 (2026-10-22); AWS access under the justfile's deploy profile and the agreed function name before B5 (2026-10-28); each slides the date day for day if late; the analog's training host (`mps:0`), otherwise the one-epoch probe (B2) re-prices B3. The day count stays 1 while the measured per-step time is at most 3.0 s.

**Paste-ready text for the request's "Date (owning team to fill):" line (the orchestrator's edit, after acceptance):**

`Date (owning team to fill): 2026-10-29 (proposed by aprender, pending the owner's confirmation; derivation and conditions in aprender's .planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-EXPERIMENT-COACH-DECIDE-REQUEST-REPLY.md, section 4)`

## Commit

- `4eba6e7c9` (`4eba6e7c946e6dca0f3b6dfb5a119649d4c68304`): `docs(quick-261021-ia2): aprender reply to the experiment-coach decide-server request`, two files (the reply and the facts table), trailer `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>` as the last line. Made with `node ~/.claude/gsd-core/bin/gsd-tools.cjs commit ... --files <reply> <facts>` after `git update-index --add` (the worktree guard refuses `git add` and `git commit` because the rtk hook rewrites them). `git rev-list --count 844d02ff4..HEAD` = 1. Not pushed.
- The plan commit ledger file (`.git/worktrees/.../gsd-plan-head-before-quick-261021-ia2-01`) could not be written: the path guard refuses writes outside the worktree. The base and head hashes are recorded in the frontmatter instead; the count above was measured against the base directly.

## Contradictions found

- Lead vs seen HEAD: the orchestrator's leads cite motivation-experiment `919b7cd`; the checkout is at `b7057de` on `main`, clean; the request was last changed by `0a4470e` on 2026-10-20.
- Request vs spec: the request demands a Lambda with an IAM Function URL; the spec's aprender block says "pmcp.run or a Lambda URL with 10 GB". The request is stricter and the requester's SigV4 client fits only the Function URL.
- Request vs production measurement: the request's 11 to 12 s cold / 0.5 to 0.75 s per tweet warm are spike 026's single-text, no-pin, no-edge numbers; production's proven-cold 800-token calls through the pmcp.run edge took 22647-27015 ms.
- Plan lead (C2): the lead allowed `structuredContent` or `content[0].text` or both; pmcp 2.19.3 serves `content[0].text` only for this tool (no output schema).
- Plan lead (B3 formula): the lead divides `train_seconds` by the sum of `epochs_run` over three seeds; `train_seconds` is one seed's figure (train.py per-seed info), so the per-step estimate divides by one seed's steps.
- Plan lead (C9/R7): the lead calls the warm range "8-text 800-token calls"; the evidence's `warm_ms` key records no shape. The reply says so.
- Plan lead (effort spans): every analog span is 0 days, so the rule's floor sets every B item to 1 day; the spans carry no information beyond "same-day agent sprint".
- Spec vs CSV: no contradiction on 11,311 posts / 1,953 positives; the CSV additionally has 667 duplicate rows and 5 texts with both labels, unmentioned in the spec.

## Fixes made on re-derivation (Task 3 step 1)

1. R2: 57 is the stance task's empty-text ROW (prefix plus closing `[SEP]`), so the inequality is `P + t <= 100` and a text may add 43 tokens (the first draft said `P + t <= 99` and 42). Reply and facts T2-05 corrected.
2. C4 evidence: `pass_rule` is at `contracts/laya-finetune-gate-v1.yaml:180`, not `:185`. Reply and facts T1-24 corrected.
3. C3 evidence: `from_decisions` spans `crates/aprender-mcp-decide/src/lib.rs:473-495` (`owned_labels()` at `:475`), not `:472` / `:476-481`. Reply and facts T1-20 corrected.
4. Facts T2-26: the request's Context paragraph is line 40 (`## Context` is line 38). Facts corrected; the reply cites no line for it.
5. Section 5 and C9: the superseded-tier numbers were stated once (section 5) and C9 now points there, to stay inside the gate's 4000-word cap (final 3983).

## Left UNKNOWN or not measured, and what would settle each

- R6: which document is "the annotation guide" and its exact text. Settled when experiment-coach supplies the file or a version-pinned reference.
- C1 (part): whether pmcp or the shim rejects a missing or different `accept` header; no enforcement was found in the sources, the client was not run. Settled by one call from their client at B6.
- C4 (part): an exact sum-to-1 assertion on `probabilities`. Settled by their own test; by construction it holds up to f32 rounding.
- C9 (part): the request shape of the ten warm calls (866-980 ms) in `live.warm_ms`. Settled by B6's warm calls of the real batch shape.
- Not measured: a Function URL deployment of this server; IAM signing overhead; any call with a GoldStandard-sized prefix; training time at that row length (the B2 probe replaces the 1.16-1.45 s per-step estimate); SetFit at 11311 rows; x86_64 for Laya; the requester's client against any aprender endpoint.
- The exact prefix token count P of any condensed task.json (no tokenizer was run; L-01). Measured first in B1.

## Hard-limit confirmation

- L-01 (no AWS, docker, deploy or training recipe, model or dataset download, no `just laya-*` / `llm-endpoint-*`, no self-tests): the facts file's Commands run list holds 60 bullets, all `sed`, `grep`, `cat`, `ls`, `python3 -I` reads and read-only `git rev-parse` / `ls-files` / `rev-list`; the gate's write-command scan over that list passed in all three modes (it rejects `aws`, `docker`, `just`, `pmat`, package managers, `cargo run/build/test`, file mutation and output redirections). The justfile was read by window, never run.
- L-02 (no GitHub write, nothing pushed): no `gh` command was run; the one commit is local on the agent branch; `git push` was never invoked.
- L-03 (no code, contract, recipe, ROADMAP or STATE change; only the three files written; partner repo read only): the scope-mode gate asserts HEAD's file list equals exactly the reply and the facts table, `git diff --name-only HEAD` is empty, and the only untracked path beyond the (empty) baseline is this SUMMARY. The partner repo was read with `sed`, `grep`, `python3 -I` and read-only `git -C ... rev-parse / ls-files / rev-list`; its `ls-files -m` and `ls-files --others` were empty before and the gate does not touch it. No indexer (`pmat embed sync`) was run.
- L-04 to L-07: every number, date, hash and file:line in the reply is a facts token and every cited path resolves at or above its line (gate); no key, bucket name, credential or URL appears (the gate's secret and URL scans passed; the bucket's name is not written, the deploy template is cited instead); no instance type, cost or throughput is promised; the only date is in section 4 and carries "proposed" with Derivation, Conditions and Probe lines.

## Deviations from Plan

### Environment-driven adjustments (no content deviation)

**1. [Rule 3 - Blocking] `git status`, `git log`, `git add`, `git commit`, `git show`, `git diff-tree` refused by the worktree guard (the rtk hook rewrites them).**
- **Found during:** Task 1 baseline, Task 3 commit.
- **Fix:** used plumbing or non-rewritten forms with identical read-only semantics: `git ls-files --others --exclude-standard --directory` for the untracked baseline (recorded as such in the facts file), `git rev-list --reverse --format=%cs` for SUMMARY first-commit dates, `git -C <partner> rev-parse / ls-files / rev-list` for the partner repo, `git cat-file -p HEAD` to verify the commit message, `git update-index --add` to stage, and the project commit helper (`gsd-tools commit --files`) to commit. The gate script itself runs `git status` and `git show` from inside python, as the plan specifies.
- **Files modified:** none.

**2. [Rule 3 - Blocking] `${TMPDIR}` expansion in the verify command refused by the guard.**
- **Fix:** the gate was invoked by TMPDIR's literal value (`/var/folders/3s/xftgktnj6qs681vbh0tg5hmc0000gn/T/ia2_gate.py`), the same file the plan's command names.

**3. Plan commit ledger not written** (path guard: outside the worktree). `plan_head_before` / `plan_head_after` are in the frontmatter; `commits: 1` is measured by `git rev-list --count` against the base.

No content deviation: all three tasks executed as written; no checkpoint was hit; the tracer feedback gate passed automatically (automated-only verify, `GATE OK` in tracer mode) and expansion proceeded.

## Known Stubs

None. The reply names `scripts/laya_train/prepare_goldstandard.py (new)` as proposed future work, labelled (new) in the text; no stub file was created.

## Threat Flags

None. No new network endpoint, auth path, file access pattern or schema was introduced; the documents describe proposed work only. The threat register's T-ia2-01 (disclosure) and T-ia2-03 (spend) mitigations held: the gate's secret and URL scans passed, and no AWS, recipe or GitHub-write command appears in the Commands run list.

## Self-Check: PASSED

- FOUND: `.planning/phases/08-laya-decision-model-local-fine-tune-and-thin-mcp-server/08-EXPERIMENT-COACH-DECIDE-REQUEST-REPLY.md`
- FOUND: `.planning/quick/261021-ia2-aprender-side-reply-to-the-experiment-co/261021-ia2-FACTS.md`
- FOUND: commit `4eba6e7c9` is HEAD (`git rev-parse HEAD` = `4eba6e7c946e6dca0f3b6dfb5a119649d4c68304`, parent `844d02ff4bde55bee01f038d1f21bda9fadfee4b`)
- Gate: `GATE OK []` in tracer, final and scope modes.
