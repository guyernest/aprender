---
phase: 10-antisemitism-v1-decide-server-for-experiment-coach
plan: 10
subsystem: docs
tags: [laya, docs, contract, claims-ledger, decide-tool-boundary, pmcp-run, documentation-only]

requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: "plan 10-12: aprender-mcp-decide-antisemite live and the previous server deleted by the owner (10-RENAME-DEPLOY-EVIDENCE.json); plan 10-11: the door at 8 (no-op); plan 10-04: the gate-run artifact; plan 10-05: the SetFit cell, built but NOT run"
provides:
  - "decide-tool-boundary-v1 10.0.0: served_task_min_row_tokens 86 (antisemitism-v1), FALSIFY-DECIDE-TOOL-009 re-pointed at the new server, harness and Phase 10 record"
  - "the lambda README, CLAUDE.md decision-model row and eight claims-ledger rows state the deployment the evidence shows"
affects: [10-09]

status: complete
actuals:
  tokens: 5700
  tasks: 2
  commits: 2
plan_head_before: 5e03cb89a91f31b1a6eb663eb72f26d027156857
plan_head_after: ed3c7393a65a7867b5c5462909943fd75400fbaf

key-files:
  created: []
  modified:
    - contracts/decide-tool-boundary-v1.yaml
    - crates/aprender-mcp-decide-lambda/README.md
    - CLAUDE.md
    - scripts/laya_claims.tsv

key-decisions:
  - "Version bumped 9.0.0 -> 10.0.0 although `pv diff` suggested NO bump (see Deviations): the served constant moved, as at v7.0.0, and the plan's verify requires a version change."
  - "No SetFit number appears anywhere: the README and CLAUDE.md say the cell was built but not run, SC-4 is held open by the owner and plan 10-06 is deferred."
  - "The README example carries no result numbers: no live replay of the example sentence is recorded yet (plan 10-09), and this plan touches no AWS or pmcp.run."

requirements-completed: [SC-5, D-05, D-13, D-15, D-16]
requirements-note: "D-15 is satisfied by assertion only: this plan edits no door literal. SC-4 (the SetFit comparison) is NOT advanced by this plan and stays open by the owner's decision."

coverage:
  - id: D1
    description: "served_task_min_row_tokens equals the measured P and the live Rust-side min_row_tokens, with a comment that states tokens_planned as the request TOTAL"
    requirement: "D-05"
    verification:
      - kind: unit
        ref: "Task 1 verify: CONTRACT OK P 86 B 8 TP 800 version 10.0.0; cargo test -p aprender-mcp-decide --lib (32 passed, incl. tests::max_texts_fit_the_budget_at_the_shortest_row and tests::bounds_match_contract)"
        status: pass
    human_judgment: false
  - id: D2
    description: "The documents and the contract name one server, one endpoint, one sha, one door B and one P, and the previous server appears only as retired"
    requirement: "SC-5"
    verification:
      - kind: other
        ref: "Task 2 verify: DOC CROSS-CHECK OK sha 2293bebb29ac B 8 P 86 cold 19759 25499; just laya-claims-check LAYA CLAIMS OK 140 rows; readme_contract 16 passed"
        status: pass
    human_judgment: false
  - id: D3
    description: "Phase-wide static gates: fmt, no new .sh, bash -n on every justfile recipe added or changed, strict clippy on every touched crate"
    requirement: "SC-5"
    verification:
      - kind: other
        ref: "SC-5 STATIC GATES OK (15 recipes, clippy on apr-cli, aprender-contrastive-data, aprender-core, aprender-mcp-decide-antisemite-lambda, aprender-mcp-decide-lambda)"
        status: pass
    human_judgment: false
---

# Phase 10 Plan 10: Contract, README, CLAUDE.md and ledger describe the live server Summary

**decide-tool-boundary-v1 is now 10.0.0 with `served_task_min_row_tokens` 86 and a FALSIFY-DECIDE-TOOL-009 that names `aprender-mcp-decide-antisemite`; the lambda README, CLAUDE.md and eight ledger rows state the deployment 10-RENAME-DEPLOY-EVIDENCE.json records, with the SetFit comparison stated as not run.**

## Performance

- **Duration:** not measured (two tasks, documentation only; the longest step was the phase-wide clippy sweep).
- **Tasks:** 2 of 2. **Commits:** 2 (measured from the ledger: `git rev-list --count 5e03cb89a..HEAD`).
- **Files changed:** 4, none under `crates/*/src`.

## Task 1: the served-task constant (commit 1997449e9)

- **pv diff suggestion:** `Contracts are identical.` pv does not model `constants:` and does not compare the falsification `test_harness` / `evidence` / `implemented_by` fields, so it offered no bump. **Version applied: 9.0.0 -> 10.0.0 (MAJOR)**, because a served constant moved (the v7.0.0 precedent) and the plan's verify requires the version to differ. The history paragraph (v10.0.0) says this.
- **Constant and arithmetic:** `served_task_min_row_tokens` 57 -> **86**. P = 86 from 10-TASK-EVIDENCE.json `measured.P`, equal to the Rust-side `min_row_tokens` the live plan-only record reports (86, both shapes). B = 8 (unchanged, `chosen_max_texts`). B x P = 688 <= 800. `probe --plan-only --maximal distributed` plans 8 texts totalling **800** built tokens (`tokens_planned`): the comment states that as the request TOTAL, not B x P. The superseded stance task's 57 is named as history.
- **FALSIFY-DECIDE-TOOL-009:** `test_harness` is `just laya-deploy-verify models/decide/laya-antisemitism-v1.apr aprender-mcp-decide-antisemite`; `evidence` is the Phase 10 rename record; `implemented_by` is prefixed with the plan 10-12 text (sha256 2293bebb..., 4 proven-cold samples: CONCENTRATED 25499 and 19759 ms, DISTRIBUTED 22963 and 22788 ms) followed by `SUPERSEDED ARTIFACT:` and the unchanged 08-30 text.
- **Ledger:** row `tool.FALSIFY-DECIDE-TOOL-009.harness` re-anchored to the new harness string; `tool.pv-valid`'s finding text changed from "9.0.0 validates" to "10.0.0 validates" (see Deviations).
- **Gates:** pv validate `0 error(s), 0 warning(s)`; `cargo test -p aprender-mcp-decide --lib` 32 passed (max_texts_fit_the_budget_at_the_shortest_row and bounds_match_contract ok); `cargo test -p aprender-mcp-decide-lambda --lib` ok; `just laya-claims-check` `LAYA CLAIMS OK 134 rows`; `make contract-audit-phase8` clean; no tweet text or 12-digit number in the two files; no file under `crates/aprender-mcp-decide/src` or `crates/aprender-mcp-decide-lambda/src` changed.

## Task 2: README, CLAUDE.md and ledger (commit ed3c7393a)

Sentences as written (figures copied from 10-RENAME-DEPLOY-EVIDENCE.json):

- **README Deployed:** live as `aprender-mcp-decide-antisemite` since 2026-10-23 (plan 10-12) on 10,240 MB, serving `antisemitism-v1`; first live 2026-10-22 under the previous id (plan 10-08), which the owner retired on 2026-10-23 (D-19, D-20). One single-line sentence carries the previous endpoint host and the words `was retired by the owner`. Table: new endpoint, auth off (D-13), Task "Antisemitism in tweets ... labels `not_antisemitic`, `antisemitic`", Model `laya-antisemitism-v1.apr` with sha256 `2293bebb29acfd857506c387c521aef8a1211ab000fc179648db93763c6c776e`. "The stance demo (`laya-stance-64.apr`, sha256 24a44d7e...) is no longer served since 2026-10-22 (D-13)."
- **README example:** the synthetic sentence "The weather today is mild and sunny." (the first REPLAY_TEXTS entry), no tweet.
- **README Limits:** 8 texts, 800 built tokens, cold **19.8-25.5 s** over 4 proven-cold samples of the two maximal shapes (download 9.0-9.2 s of the load; worst sample left **4.50 s** under the cap), warm **1.08-1.15 s** (5 calls); the stance figures (22.6-27.0 s, 0.87-0.98 s) are named as the other deployment's. Probe usage lists `[--replay N [--replay-from K]] [--replay-over-limit]` with one sentence each.
- **README Claim scope:** rewritten for antisemitism-v1 (gate_pass: macro-F1 0.7956 vs 0.5596 zero-shot, margin 0.236, ECE 0.0424, n 2129, seed 23); no shift probe was measured; the SemEval 0.1896 probe is attributed to the retired stance demo.
- **CLAUDE.md:** "The server is live on pmcp.run as `aprender-mcp-decide-antisemite` on the 10,240 MB tier; since 2026-10-23 it serves `antisemitism-v1` (... sha256 2293bebb..., labels `[not_antisemitic, antisemitic]`, plan 10-12 ...) and the stance demo above is no longer served: at most 8 texts and 800 built tokens per call. 4 proven-cold samples ... 19.8-25.5 s ... (worst slack 4.5 s); 5 warm calls took 1.08-1.15 s (the stance deployment measured 22.6-27.0 s cold, 0.87-0.98 s warm)." The thin-server list gains `crates/aprender-mcp-decide-antisemite-lambda`. The B phrase stays exactly once. Records list: the two Phase 8 files plus 10-GATE-RUN, 10-RENAME-DEPLOY and 10-LIVE-DEPLOY evidence.
- **SetFit, stated truthfully:** both documents say the SetFit cell (plan 10-05) was built but not run, the owner held SC-4 open and plan 10-06 is deferred; neither contains a SetFit number or a comparison claim.
- **Ledger:** `claude-md.decide-row.tier` re-anchored to the new live sentence (test `10-RENAME-DEPLOY-EVIDENCE.json#live.final`, finding "D-13/D-16 (10-08)"); added `lambda.readme.deployed.model`, `lambda.readme.deployed.task`, `lambda.readme.stance-retired`, `lambda.readme.deployed.server`, `lambda.readme.previous-server-retired` (test `#retirement.state`) and `claude-md.decide-row.antisemitism-v1`. `claude-md.decide-row.bounds` unchanged. 6 rows reference `10-RENAME-DEPLOY-EVIDENCE.json#live.final`.
- **Gates (all from the plan's own verify block, run unmodified):** DOC CROSS-CHECK OK (sha 2293bebb29ac, B 8, P 86, cold 19759-25499 ms; live.outcome verified, final true, retirement deleted_confirmed); `LAYA CLAIMS OK 140 rows`; `readme_contract` 16 passed; `cargo fmt --all -- --check` clean; SC-5 static gates OK (no `.sh` added since 97eb91a54; `bash -n` on the 15 changed recipes, including laya-prepare-goldstandard, laya-probe-epoch, laya-setfit-cell and laya-gates-selftest; strict clippy clean on apr-cli, aprender-contrastive-data, aprender-core, aprender-mcp-decide-antisemite-lambda and aprender-mcp-decide-lambda; bashrs deliberately not run on the justfile, per T-10-51); no tweet text or account id in the four files.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] pv diff offered no version suggestion**
- **Found during:** Task 1.
- **Issue:** the plan says to bump "exactly as pv diff suggests" and expects MAJOR; pv diff printed `Contracts are identical.` for the old and new files because it does not model `constants:` or the falsification harness fields.
- **Fix:** bumped to 10.0.0 on the repo's own precedent (a moved served constant is MAJOR, v7.0.0) and recorded the reason in the contract's history paragraph and above. The plan's verify (version must differ) passes.
- **Commit:** 1997449e9

**2. [Rule 1 - Bug] Ledger row `tool.pv-valid` quoted the old version**
- **Found during:** Task 1.
- **Issue:** its finding text read "decide-tool-boundary-v1 9.0.0 validates"; the bump made it stale (claims-check does not read the finding, so no gate would have caught it).
- **Fix:** finding text changed to "10.0.0 validates" in the same commit.
- **Commit:** 1997449e9

**3. [Rule 2 - Missing critical] README Claim scope and CLAUDE.md "one declared gate run" described the retired stance model**
- **Found during:** Task 2.
- **Issue:** after the live sentence changed, "The served model passed laya-finetune-gate-v1 ... SemEval 0.1896" and "The one declared gate run" would have described a model that is no longer served.
- **Fix:** rewrote the README Claim scope for antisemitism-v1 from 10-GATE-RUN-EVIDENCE.json and added the second gate run to the CLAUDE.md row, with the SetFit comparison stated as not run.
- **Commit:** ed3c7393a

**4. [Plan text not followed] README example result**
- The plan asks for "the numbers that sentence returned live". No record contains a live result for the example sentence (the acceptance replay is plan 10-09, not yet run), and this plan must not touch AWS or pmcp.run. The README says the probabilities are not quoted and names `examples/probe.rs --replay 1`. No number was invented or taken from a spike.

**5. [Plan text adapted] CLAUDE.md B phrase**
- The plan describes a two-part door clause left by plan 10-11; plan 10-11 was a no-op at B = 8, so the row has the single phrase "at most 8 texts and 800 built tokens per call", kept exactly once and still anchored by `claude-md.decide-row.bounds`.

**Total deviations:** 3 auto-fixed plus 2 departures from plan text. **Impact:** none on the gates; every verify block passed unmodified.

## Known Stubs

None.

## Not done here (hand-off)

- `contracts/aprender/binding.yaml` (the `build_maximal_request` note) still says "decide-tool-boundary-v1 9.0.0" and describes the 08-30 stance deployment. It is a dated historical note outside this plan's `files_modified`, so it was left; no gate reads the version there.
- `10-RENAME-DEPLOY-EVIDENCE.json` `live.contract` still reads "decide-tool-boundary-v1 9.0.0": it records the contract version at the time of the deploy and is evidence, not edited.
- The README example has no result numbers until plan 10-09's acceptance replay runs.
- The old S3 object under `decide/aprender-mcp-decide/` remains the owner's call (from plan 10-12).
- SC-4 (SetFit comparison) stays open by the owner's decision; plan 10-06 stays deferred. Plan 10-09 must carry its blocked-SetFit branch.

## Threat Flags

None. Documentation only; no new endpoint, auth path or file access.

## Self-Check: PASSED

The four modified files exist; commits 1997449e9 and ed3c7393a are ancestors of HEAD; both verify blocks ended rc=0.
