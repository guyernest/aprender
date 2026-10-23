---
phase: 10-antisemitism-v1-decide-server-for-experiment-coach
verified: 2026-10-23T18:00:00Z
status: gaps_found
score: 4/5 must-haves verified
covered_files:
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-01-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-01-SUMMARY.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-02-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-02-SUMMARY.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-03-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-03-SUMMARY.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-04-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-04-SUMMARY.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-05-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-05-SUMMARY.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-06-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-07-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-07-SUMMARY.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-08-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-08-SUMMARY.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-09-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-09-SUMMARY.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-10-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-10-SUMMARY.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-11-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-11-SUMMARY.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-12-PLAN.md"
  - ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-12-SUMMARY.md"
  - "contracts/decide-tool-boundary-v1.yaml"
  - "crates/aprender-mcp-decide-antisemite-lambda/src/main.rs"
  - "scripts/laya_train/prepare_goldstandard.py"
covered_digest: "v3:sha256:b7c1578d1d112c8a4a2539c0889d331a80b7f2ca64351f5332234342792ea20d"
behavior_unverified: 0
overrides_applied: 0
re_verification: false
gaps:
  - truth: "SC-4: The SetFit comparison cell (s64 drawn from the same train split, evaluated on the same eval split) is recorded beside the Laya numbers under the Phase 5 claims discipline, labelled 'same eval split, subsampled train'"
    status: failed
    reason: "No SetFit number of any kind exists. The cell was built (adapter, e1b16 CPU config, laya-setfit-cell recipe, same-eval-split proof) but never run, because apr eval exceeds this host's memory on the cell's splits (an implementation limit, not a domain refusal). The owner held SC-4 open on 2026-10-22 (decision C: do not run the cell, do not change apr eval, no cloud). Plan 10-06 (side-by-side table) is deferred by the owner for the same reason. HELD-OPEN by the owner, not accepted as met and not waived."
    artifacts:
      - path: ".planning/phases/10-antisemitism-v1-decide-server-for-experiment-coach/10-SETFIT-CELL-EVIDENCE.json"
        issue: "train, eval_validation, eval_test, metric_tag, ece_top_label and laya_beside are all null; outcome is 'blocked'; sc4_status 'refused-open'"
      - path: "crates/aprender-train/src/train/setfit/apr_evaluate.rs"
        issue: "predict_rows memory grows with split size (61 rows 9.4 GB, 241 rows about 40 GB, 852 rows killed at about 90 GB on a 48 GB host); unmodified by owner decision"
    missing:
      - "A contract-first fix for the apr eval memory defect (bound per-batch activation memory in the classify path used by predict_rows), or an eval on a larger-memory host"
      - "A real run of just laya-setfit-cell, then plan 10-06 (apr eval --dump-probabilities, house macro-F1 / per-class F1 / ECE, side-by-side table)"
      - "Or an explicit owner descope of SC-4 recorded in ROADMAP.md (not done; the owner chose hold-open)"
---

# Phase 10: antisemitism-v1 Decide Server for experiment-coach Verification Report

**Phase Goal:** Deliver the first production decider for a sibling project: a Laya artifact `antisemitism-v1` fine-tuned on GoldStandard2024 with labels `[not_antisemitic, antisemitic]` and a condensed guide as `task.json`, passing `laya-finetune-gate-v1` under the three-seed median rule, packed and verified as `.apr`, compared against a SetFit `s64e1b16` cell on the same eval split, and redeployed through the existing pmcp.run path (10,240 MB) with the sha256 handed to experiment-coach and the acceptance replay recorded.
**Verified:** 2026-10-23
**Status:** gaps_found
**Re-verification:** No, initial verification

## Verdict

The production half of the goal is achieved and was re-checked against the code, the artifact on disk and the live endpoint: SC-1, SC-2, SC-3 and SC-5 are VERIFIED. The comparison half is not: SC-4 is FAILED as a criterion (no SetFit number exists) and is HELD OPEN by the owner (decision C, 2026-10-22, recorded in 10-SETFIT-CELL-EVIDENCE.json `sc4_owner_reply` and in ROADMAP.md plan 10-05/10-06 lines). It is not marked satisfied. Because the phase goal names the SetFit comparison, the overall status cannot be `passed`.

## Per-criterion status

| SC | Status | Basis |
|----|--------|-------|
| SC-1 data prep and condensed guide | VERIFIED | see below |
| SC-2 one declared gate run, shipped seed passes both clauses | VERIFIED (2 disclosed warnings) | see below |
| SC-3 deploy-eligible `.apr`, sha256 and labels in hand-off | VERIFIED | see below |
| SC-4 SetFit comparison cell recorded | FAILED, HELD-OPEN (owner, 2026-10-22, decision C) | no number exists |
| SC-5 live through pmcp.run, identity probe, acceptance replay, gates green | VERIFIED | see below |

## Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | SC-1: `prepare_goldstandard.py` writes the data dir by a declared, seeded, stratified, text-group-disjoint 20 % rule; duplicates collapsed; conflicting labels resolved by a declared rule; no tweet text committed or printed | VERIFIED | Script exists (36 KB) with the rules declared in its docstring (nfc-trim-ws-v1 grouping, mixed-label groups dropped whole, seed 13, ceil(0.2 x class groups)). I ran `--selftest`: exit 0, "PREPARE SELFTEST OK", including the sentinel no-text-on-stdout/stderr cases. On the written data: train 8,510 (7,016 / 1,494), eval 2,129 (1,755 / 374), matches 10-TASK-EVIDENCE.json; I recomputed normalized-text overlap between train and eval = 0 and within-file duplicates = 0; eval share 2,129 / 10,639 = 20.0 %. `data/` and `models/` are gitignored (`git check-ignore` confirms) and `git ls-files data models` is empty, so no tweet text is tracked. |
| 2 | SC-1: `task.json` carries the condensed guide within a declared prefix token budget measured with the served tokenizer, recorded, sha256 in the run record | VERIFIED | task.json on disk hashes to `33fd8d7c...2248e3`, equal to 10-TASK-EVIDENCE.json, the gate record and the artifact's `inputs_sha256.task_json`. P = 86 measured with the served tokenizer against the declared P_max 100; `head_cut` false, options kept whole. Criteria order is `not_antisemitic` first. Disclosure: the text paraphrases the IHRA examples with the labelling rules of arXiv 2304.14599; the paper's separately cited 12-paragraph guideline (1910.01214) was not fetched. The owner approved the exact bytes on 2026-10-21. |
| 3 | SC-2: one declared run (seeds 13/17/23, epochs from a one-epoch probe, early stopping); shipped seed passes margin >= 0.05 and ECE <= 0.10 | VERIFIED | 10-GATE-RUN-EVIDENCE.json: `declared_runs` 1, `outcome` gate_pass. Shipped seed 23 (median-ECE of 0.0117 / 0.0456 / 0.0424, which I re-derived): margin 0.2360, ECE after calibration 0.0424, macro-F1 0.7956 vs zero-shot 0.5596, n 2,129. Rust recomputation equals the Python numbers. Epochs 4 declared from the probe (10-PROBE-EVIDENCE.json, rule "4 unless s_per_step <= 1.0"). |
| 4 | SC-2: record carries device read back, measured per-step time, re-score noise record | VERIFIED | `device_used` mps:0 (read back from DEVICE trace lines for all three seeds), torch 2.14.0; `s_per_step` 1.4695 s (threshold 3.0 s) from the probe; `noise` and `pack.rescore_max_abs` 7.55e-06 vs bound 1.44e-05 (zero-shot 1.20e-05 vs 6.03e-05); argmax 2129/2129. |
| 5 | SC-3: `pack_laya` produced and verified a deploy-eligible `.apr`; sha256 and labels in task order in the hand-off | VERIFIED | `models/decide/laya-antisemitism-v1.apr` on disk (846,220,868 bytes) hashes to `2293bebb...c776e`, equal to the gate record `verify_line.artifact_sha256`, the S3 key, the Lambda env pin and 10-HANDOFF.md section 1. Evidence `verify_line.deploy_eligible` true, inspect labels `[not_antisemitic, antisemitic]`. I did not re-run the 18-minute `pack_laya --verify`; the on-disk hash plus the live identity probe stand in for it (see Human Verification: none required). |
| 6 | SC-4: SetFit s64 comparison cell recorded beside the Laya numbers | FAILED, HELD-OPEN | See Gaps. 10-SETFIT-CELL-EVIDENCE.json states "NO CELL WAS RUN: no number exists"; all metric fields null. Honest negative record: no document in the tree claims a SetFit number (lambda README line 165 and 10-HANDOFF.md section 6 both say none exists). Owner reference: decision C, 2026-10-22, `sc4_owner_reply`; plan 10-06 deferred 2026-10-22. |
| 7 | SC-5: live through the existing pmcp.run path at 10,240 MB, identity-checked by the probe with `--expect-sha256` | VERIFIED | My own read-only probe against `https://aprender-mcp-decide-antisemite.us-east.true-mcp.com/mcp` with `--expect-sha256 2293bebb...`: exit 0, `identity_matches` true, one tool named `classify`, labels in order, returned sha equals the pin (cold, load 15.3 s). `/health` 200. Recorded Lambda readback: 10,240 MB, arm64, Active, env sha equals the artifact. |
| 8 | SC-5: acceptance replay (one result per text, labels in task order, probabilities sum to 1, returned sha equals pin) recorded, timings reported not gated | VERIFIED | 10-ACCEPTANCE-REPLAY.json covers 1, 1, 1 and 8 texts plus the count refusal at 9. I re-ran `--replay 8` live: n_results 8, sums within 1e-5 (max deviation 2.6e-8), `label_is_argmax`, `labels_in_task_order`, `identity_matches` all true, 762 built tokens; and `--replay-over-limit` (9 texts) returned `isError` naming the count bound, the limit and the contract. Cold 19.8 to 25.5 s and warm 1.08 to 1.15 s are reported, not gated (D-16). |
| 9 | SC-5: every gate green (fmt, strict clippy on touched crates, tests, `pv validate`, `bashrs lint` on new scripts) | VERIFIED | Re-run by me: `cargo fmt --all -- --check` rc 0; `cargo clippy -p aprender-mcp-decide -p aprender-mcp-decide-lambda -p aprender-mcp-decide-antisemite-lambda --all-targets --no-deps -D warnings` rc 0; `cargo test -p aprender-mcp-decide --lib` 32 passed; `cargo test -p aprender-mcp-decide-lambda -p aprender-mcp-decide-antisemite-lambda --lib` 59 passed; `pv validate contracts/decide-tool-boundary-v1.yaml` "0 error(s), 0 warning(s), Contract is valid"; `just laya-claims-check` "LAYA CLAIMS OK 140 rows". No new `.sh` file was added in the phase range, so `bashrs lint` has no new script to cover. Not re-run by me (taken from 10-10-SUMMARY "SC-5 STATIC GATES OK"): `bash -n` on the 15 changed justfile recipes and clippy on apr-cli, aprender-contrastive-data and aprender-core. |
| 10 | Server rename and retirement (D-19/D-20) leave exactly one serving endpoint | VERIFIED | `GET /health` 200 on `aprender-mcp-decide-antisemite`, 404 on the retired `aprender-mcp-decide` (I checked both). 10-RENAME-DEPLOY-EVIDENCE.json `retirement.state` deleted_confirmed (owner-run, from the admin UI). |

**Score:** 4/5 roadmap success criteria verified (SC-4 held open).

## Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `scripts/laya_train/prepare_goldstandard.py` | declared-rule data prep, `--selftest`, `--measure` | VERIFIED | selftest passes; no text printed |
| `data/decide/goldstandard-2024/{task.json,train.jsonl,eval.jsonl}` | gitignored data dir | VERIFIED | hashes equal the evidence; counts and disjointness re-derived |
| `models/decide/laya-antisemitism-v1.apr` | deploy-eligible artifact | VERIFIED | sha256 equals every record |
| `10-GATE-RUN-EVIDENCE.json`, `10-PROBE-EVIDENCE.json`, `10-TASK-EVIDENCE.json`, `10-DOOR-EVIDENCE.json` | evidence | VERIFIED | internally consistent, cross-checked against disk |
| `crates/aprender-mcp-decide-antisemite-lambda/` | thin per-server crate | VERIFIED | Cargo.toml, main.rs shim, README; tests and clippy green |
| `contracts/decide-tool-boundary-v1.yaml` | 10.0.0, `served_task_min_row_tokens` 86, `classify_max_texts` 8 | VERIFIED | `pv validate` clean; mirror tests pass |
| `10-HANDOFF.md` | D-17 hand-off | VERIFIED | sha, labels, endpoint, task text, gate, replay, timings; SetFit section honestly says blocked |
| `10-SETFIT-CELL-EVIDENCE.json` | cell numbers | STUB (by owner decision) | structurally honest, no numbers |
| `10-VALIDATION.md` | validation contract | STUB | unfilled draft template (`status: draft`, `{placeholder}` fields); not a success criterion |

## Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| task.json | artifact | `inputs_sha256.task_json` | WIRED | `33fd8d7c...` in task, gate and inspect records |
| gate run | `.apr` | pack_laya, shipped seed 23 | WIRED | embedded gate equals gate report |
| `.apr` sha | S3 key / Lambda env pin / live response | `APRENDER_DECIDE_SHA256` | WIRED | live probe returns the same sha |
| contract constants | Rust constants | `tests::bounds_match_contract` | WIRED | 32 tests pass |
| Laya data dir | SetFit canonical dir | `laya_dir_to_canonical` | WIRED but never exercised end to end | same-eval-split proof recorded; training and eval never reached |

## Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Prepare script rules and no-text guarantee | `prepare_goldstandard.py --selftest` | PREPARE SELFTEST OK | PASS |
| Live identity | `probe --url <antisemite endpoint> --apr ... --expect-sha256 2293bebb...` | identity_matches true, rc 0 | PASS |
| Live 8-text replay | `probe ... --replay 8` | 8 results, sums 2.6e-8 deviation, labels in order | PASS |
| Count door | `probe ... --replay-over-limit` | isError naming bound, limit and contract | PASS |
| Retired endpoint silent | `curl /health` on old host | 404 | PASS |

## Probe Execution

The phase declares no `scripts/*/tests/probe-*.sh` files; its "probe" is the Rust `examples/probe.rs` run above. SKIPPED for shell probes (none declared).

## Requirements Coverage

REQUIREMENTS.md is the SetFit milestone's and maps no IDs to this phase (ROADMAP: "Requirements: TBD"). The binding requirements are 10-CONTEXT.md D-01..D-18 and the five success criteria. D-11 (SetFit cell) is the only unmet decision, matching SC-4. No orphaned requirement IDs.

## Anti-Patterns Found

None blocking. Debt-marker scan of the phase's code files (`prepare_goldstandard.py`, the thin lambda `main.rs`) found no unreferenced TBD/FIXME/XXX. `10-VALIDATION.md` is an unfilled template (WARNING, documentation only).

## Warnings (not gaps)

1. **SC-2 early stopping is inert at E = 4.** With first_candidate_epoch 1 and patience 3 the earliest stop is epoch 4, so every seed ran all four epochs and early stopping acted only as best-epoch restore (10-PROBE-EVIDENCE.json `early_stopping_can_end_early` false). Seeds 13 and 23 restored epoch 1, so the shipped model is in effect a one-epoch fine-tune. Disclosed in 10-HANDOFF.md section 5; the criterion's gate clauses are met regardless.
2. **Host load during the gate run was not sampled** (`host_load.sampled_during_run` false), so contention is unmeasured, not ruled out. The probe window was sampled and not contended. Timing is not a gate clause.
3. **Seed 17 hit the temperature clamp** (T = 5.0). Not the shipped seed; the median-ECE rule did not select it.
4. **Open endpoint cost exposure.** Auth is off (owner decision D-13, risk accepted); a cold call buys about 25 s of 10 GB compute. Recorded, owner-accepted.
5. **Old S3 weights object** under `decide/aprender-mcp-decide/` is still present; removal is the owner's call (10-12-SUMMARY, hand-off section 11).
6. **Task text is a paraphrase**, not the fuller published guideline; accuracy of the wording is a hypothesis measured only by the gate (D-07).
7. **Accuracy readout beyond the gate** (plan 10-06) is deferred with SC-4.

## Gaps Summary

One gap, SC-4. Root cause: `apr eval` memory grows with split size and cannot evaluate the 852-row validation or 2,129-row test split of the s64e1b16 cell on this 48 GB host. The adapter, config, recipe and same-eval-split proof are built and tested; the cell has never been trained or evaluated. The owner's 2026-10-22 decision C holds SC-4 open and defers plan 10-06. The gap is not in the deferred-items list of any later milestone phase (Phase 11 is the unrelated embed endpoint), so it is not filtered under Step 9b. Closing it needs the `apr eval` memory fix (contract-first per `crates/aprender-train/CLAUDE.md`) or a larger-memory host, then a real `just laya-setfit-cell` run and plan 10-06; or an explicit owner descope of SC-4 in ROADMAP.md. Until then Phase 10 is complete for the classifier and its deployment, and incomplete for the comparison.

---

_Verified: 2026-10-23_
_Verifier: Claude (gsd-verifier)_
