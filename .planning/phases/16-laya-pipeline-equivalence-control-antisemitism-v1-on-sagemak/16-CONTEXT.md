# Phase 16: Laya pipeline-equivalence control (antisemitism-v1, CUDA vs Mac) - Context

**Gathered:** 2026-11-01 (from the Phase 15 close-out conversation; no discuss-phase run)
**Status:** Ready for planning
**Seed:** `16-DECLARATION-DRAFT.json` in this directory (draft declaration, band and time estimate accepted by the owner)

## Why this phase exists
Phases 14 and 15 could not tell whether the SageMaker CUDA training pipeline differs from the Mac (MPS) local run, or whether motivation-v1 (295 train rows, 100 eval rows, 3 imbalanced labels) is simply too small and noisy. Phase 15 closed with SC4 (D-16) unmet by owner decision (`accept-inconclusive`), `next_trusted` false. This phase runs a CONTROL on a task without small-data noise.

## Evidence the choice rests on (read-only, 2026-11-01)
- antisemitism-v1 (`data/decide/goldstandard-2024`): 8,510 train, 2,129 eval, 2 labels. Mac (`mps:0`) reference run in `.planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-GATE-RUN-EVIDENCE.json`: recipe `d70c3f3da0daae129f3ee1ce928eb21a0ef71a83a6f09dc18e0621f3cc8adcbc`, 4 epochs, seeds 13/17/23, per-seed margin 0.2358/0.2458/0.2360 (mean 0.2392), shipped seed 23, shipped ece_post 0.0424, seed 17 hit the calibration clamp (T=5.0) without failing, wall 16,822 s.
- tweet-stance-64 is NOT stable (192 train rows, per-seed margins 0.144/0.222/0.248) and is out of scope.
- The dataset is NOT staged in S3 (`models/laya-trainer/datasets-dev.json` lists motivation-v1, tweet-stance-16, tweet-stance-64 only). Local sha256: task.json 33fd8d7c...2248e3, train.jsonl 81a923d4...a79a9e, eval.jsonl f4a0cf7c...b38a8 (equal to the Phase 10 evidence data_sha256).
- Server limits: epochs range [4,12]; MAX_RUNTIME_SECS 21,600 (6 h, `crates/aprender-mcp-laya-train/src/layout.rs`); lease 11 h. Dataset prefixes are immutable.
- Pilot waiver `motivation-v1-pilot-ece` binds the motivation-v1 input hashes only: no waiver applies here.

## Locked decisions (owner, 2026-11-01)
- D-01: Control dataset is antisemitism-v1 (goldstandard-2024), recipe `d70c3f3d...`, 4 epochs, seeds 13/17/23, device cuda, one attempt, no retry class (as the Phase 15 declaration).
- D-02: Judged statistic is the THREE-SEED MEAN margin in the band [0.209, 0.269] (Mac reference 0.2392 +/- 0.030) - owner reply "Acceptable."
- D-03: ECE: the gate bar only (shipped-seed ece_post <= 0.10, gate PASS unwaived). No lower bound.
- D-04: Per-seed CUDA-minus-MPS differences are reported descriptively, never pass/fail.
- D-05: Claim scope is pipeline equivalence on THIS task only. `sets_next_trusted` false. It does not reproduce motivation-v1, does not change D-16, does not lift the Phase 14 server's untrusted state; any redefinition of trust is a separate owner ruling.
- D-06: CUDA wall time is UNMEASURED; estimate 2-6 h against the 6 h cap - owner reply "Acceptable." A run past the cap is a timeout (pre-verdict, not the attempt, re-run needs new owner approval).
- D-07: Every AWS command (stage, validate, train, poll, fetch, hygiene) is owner-run with the bang prefix; the orchestrator records replies verbatim (eliding account ids, buckets, hosts); no executor makes an AWS call. The declaration is committed ALONE before any paid run. Compute above 1 h on a non-lambda-vector host needs the owner's go (CLAUDE.md check-in); approval is per run, recorded by the orchestrator.
- D-08: The existing evaluator `scripts/laya_deploy/laya_repro_eval.py` already supports `judging.margin_statistic: seed_mean` (Phase 15 plan 15-11). Reuse it; do not widen it unless a test-first change is unavoidable and no existing verdict moves.
- D-09: No waiver, no overrides key (D-02 of Phase 15 carries over).

## Reusable machinery (Phase 15, read these patterns before inventing new ones)
`15-GAP-REPRODUCTION-DECLARATION.json` (declaration shape), `15-GAP-RUN-EVIDENCE.json` (owner_commands V/R/S/F/H, owner_replies, classify/evaluate flow), `scripts/laya_deploy/laya_margin_diagnosis.py` (`check-run-go`, `classify-run`, `scan-identifiers`), `just laya-trainer-stage dataset DIR PREFIX ENV PROFILE`, `just laya-trainer-call`, `just laya-fetch`, the CONSUMPTION RULE (classify-run decides, never the owner's description).

## Open items for the planner
- The exact `laya-trainer-stage dataset` arguments and prefix name (draft proposes `datasets/goldstandard-2024/`); stage is an owner-run AWS write.
- Whether `validate_dataset` accepts 8,510 rows (free call after staging; its result gates the paid run).
- How the control's evidence file records per-seed paired CUDA-vs-MPS differences from the Phase 10 reference without re-deriving it by hand (fixture or read-from-evidence, with an integrity check).
- The pack step on Graviton for a 2-label task under amendment A2 is expected to accept; the plan states what happens if it does not.

## Deferred
Building a larger TweetEval set; any trust redefinition; investigating the seed-17 coincidence between Phase 14 and Phase 15 runs; the pilot waiver ruling; Phase 15 STATE.md sync.
