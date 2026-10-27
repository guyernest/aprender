---
phase: 12-motivation-v1-decide-server-for-experiment-coach-laya-pilot
verified: 2026-10-26T23:30:00Z
status: passed
score: 11/11 must-haves verified
covered_files:
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-01-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-01-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-02-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-02-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-03-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-03-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-04-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-04-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-05-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-05-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-06-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-06-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-07-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-07-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-08-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-08-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-09-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-09-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-10-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-10-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-11-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-11-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-12-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-12-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-13-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-13-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-14-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-14-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-15-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-15-SUMMARY.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-16-PLAN.md"
  - ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot/12-16-SUMMARY.md"
  - "contracts/decide-apr-v1.yaml"
  - "contracts/decide-tool-boundary-v1.yaml"
  - "contracts/laya-finetune-gate-v1.yaml"
  - "crates/aprender-core/tests/monorepo_invariants.rs"
  - "crates/aprender-decide/src/artifact.rs"
  - "crates/aprender-decide/src/pack.rs"
  - "crates/aprender-decide/src/verify.rs"
  - "crates/aprender-mcp-decide-motivation-lambda/Cargo.toml"
  - "crates/aprender-mcp-decide-motivation-lambda/src/main.rs"
  - "scripts/laya_train/bakeoff.py"
  - "scripts/laya_train/metrics.py"
  - "scripts/laya_train/prepare_motivation.py"
covered_digest: "v4:sha256:7dfc1455ee931ce2b82232b157e9c22d29bc5ece7862d6a6d21a9682c479e2b0"
behavior_unverified: 0
overrides_applied: 1
overrides:
  - must_have: "Compare a Laya fine-tune, SetFit and Kev few-shot on the same stratified held-out split (Kev-4B arm measured, calibrated, ranked)"
    reason: "Kev-4B arm stopped by the owner after 134 of 395 rows (all 100 eval rows scored uncalibrated, 34 of 295 train rows) because 142 s per post is impractical without a prefix cache and a GPU. Recorded blocked / failure_class budget in 12-KEV-ARM-EVIDENCE.json with the owner's verbatim replies in 12-KEV-OWNER-REPLY-2.json. Plan 12-15 pre-declared 'budget (owner declined)' as a legal blocked class, D-24 allows a blocked arm with nothing faked, and D-06 makes Laya the served arm regardless of the ranking. Fit and eval-probs were never run; no calibrated Kev number exists and none is claimed."
    accepted_by: "owner (replies recorded in 12-KEV-OWNER-REPLY-2.json, 2026-10-26: 'yes, proceed with the three steps')"
    accepted_at: "2026-10-26T00:00:00Z"
gaps: []
---

# Phase 12: motivation-v1 decide server for experiment-coach (pilot) Verification Report

**Phase Goal:** Ship a pilot `motivation-v1` classifier of a post's motivational function (`social_affiliative`, `conspiratorial`, `other`) as its own MCP server, `aprender-mcp-decide-motivation`, deployed to augury-dev (re-scoped by D-23 to profile `ze-kasher-dev`, us-east-1) the way antisemitism-v1 was. The method is chosen by measurement: at 395 rows, compare a Laya fine-tune, SetFit and Kev few-shot on the same stratified held-out split.
**Verified:** 2026-10-26
**Status:** passed
**Re-verification:** No, initial verification

## Verdict in one paragraph

The shipping half of the goal is achieved and independently re-measured: the pilot server is live, serves the pinned artifact, and its gate result reproduces from the raw probability file. The comparison half is two arms measured plus Kev-4B recorded `blocked:budget` by the owner's own recorded decision. I judge that an honest, properly authorised and properly recorded deviation (see Truth 9) rather than a missed goal, because D-06 makes Laya the served arm whatever the ranking, D-24 and plan 12-15 both pre-declared a blocked Kev arm as a legal outcome, and nothing was faked. The method choice therefore rests on a two-arm measurement and the hand-off says so. Advisory review findings (WR-06 in particular) are real but disclosed and do not fail the phase.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | A `motivation-v1` classifier with labels `[social_affiliative, conspiratorial, other]` exists as a packed, verified artifact | VERIFIED | `models/decide/laya-motivation-v1.apr` sha256 recomputed by me = `d202f495...1aecd`, equal to gate, deploy and replay records. Gate record `verify_line.deploy_eligible: true`, `argmax 100/100`, `waived: null`. Labels in order confirmed in the live `tools/list` description. |
| 2 | The gate result is real: margin >= 0.05 over zero-shot and ECE <= 0.10, on one declared run, no waiver | VERIFIED | I recomputed from `models/decide/laya-motivation-v1/eval-probs.json` plus `eval.jsonl` using plain Python: macro-F1 0.7475947 (equals record to all digits), top-label 15-bin ECE 0.09355141 (equals record), eval rows 100 (23/14/63). Margin 0.378 vs 0.05. Declaration commit `e3bef4cb0` (ct 1793049333) precedes run start (1793049437) by 104 s; `declared_runs: 1`, `final_attempt: 1`, `GATE-OWNER-REPLY skipped`. Honest caveat: seed 17 failed ECE (0.1197) and the shipped seed's bootstrap interval is [0.0607, 0.1732]; the gate certifies the point value on 100 rows, in-distribution only. The hand-off says this plainly (section 1). |
| 3 | The pilot waiver mechanism (D-19/D-20) exists, honours only a declared waiver, never waives margin, and still refuses undeclared failure | VERIFIED (behavioural) | I ran `cargo test -p aprender-decide --lib waiver`: 16 passed, including `waiver_never_covers_margin`, `waiver_undeclared_ece_failure_is_still_gate_failed`, `waiver_claimed_by_report_but_undeclared_is_refused`, `waiver_applies_to_mismatch_is_gate_failed`, `waiver_contract_naming_margin_fails_to_load`, `unwaived_manifest_serializes_no_waiver_key`, `waived_run_packs_with_gate_waiver_and_loads_at_every_door`. Phase 10 artifact unchanged: `12-WAIVER-EVIDENCE.json` real `laya-verify` of antisemitism-v1 re-verified, sha unchanged, `waived: null`. Waiver was not exercised on the real run (gate passed). |
| 4 | Data split is seeded, stratified, text-disjoint, shared by all arms, and no post text is recorded | VERIFIED | `train.jsonl` 295 (66/41/188) and `eval.jsonl` 100 (23/14/63); text overlap 0; both file sha256s equal `12-GATE-RUN-EVIDENCE.json` `inputs_sha256`. I searched 4,082 tracked files in the phase, scripts, contracts, decide crates and docs for the first 45 characters of all 395 posts: 0 hits. `data/` is root-anchored gitignored. |
| 5 | The task.json was measured in two variants and the owner approved exact bytes before the gate run; door handled per D-26 | VERIFIED | `data/decide/motivation-v1/task.json` sha256 recomputed = `f74a3ad3...ea3`, equals `12-TASK-OWNER-REPLY.json` (`approve-a f74a3ad3cfc0`), the artifact's `inputs_sha256.task_json` and the waiver `applies_to`. Variant (a) P 97 door 8; variant (b) P 190 door 4 shown and not chosen, so D-26's per-artifact door correctly did not fire. |
| 6 | Folded spike: calibration slice and T <= 5 clamp measured before the gate; clamp binding stops for the owner (D-17/D-18) | VERIFIED | `12-SPIKE-EVIDENCE.json` outcome `clamp_not_binding`; `12-SLICE-OVERRIDE-EVIDENCE.json` `not_needed`, slice 0.25 declared; spike owner reply is a recorded skip. Spike ran on a train-only holdout (`eval_jsonl_opened: false`). Note: seed 23's applied T is exactly 5.0 (`clamp_hit: true`) in the real run, but it is not the shipped seed and passes; the handoff discloses it. |
| 7 | Server is live on pmcp.run as its own id, with the identity chain, and the antisemitism server is untouched | VERIFIED | Independent live check: `tools/list` POST to `https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp` returned HTTP 200 with a `classify` tool whose description carries the approved instruction and the three labels in order. `12-DEPLOY-EVIDENCE.json`: lambda readback 10,240 MB arm64 Active, `env_APRENDER_DECIDE_SHA256` = pin, identity probe true, negative control correctly false, `antisemite_after.unchanged: true` (CodeSha256 identical before and after). Thin crate `aprender-mcp-decide-motivation-lambda` is a 15-line shim with one dependency; registered in FALSIFY-MONO-011 (baseline 10) and the bootstrap duplicate-bin allowlist; I ran `monorepo_invariants` (15 passed) and `check_duplicate_bin_names.sh` (PASS). |
| 8 | Acceptance replay at the deployed door: function and identity, with the effective door honestly stated | VERIFIED (with WR-06 caveat) | `12-ACCEPTANCE-REPLAY.json`: single-text, 7-text, batch-of-4 replays all checks true; 8 texts refused at `classify_max_total_tokens` (rows total 850); 9 texts refused at `classify_max_texts`; sha equals pin. Cold 22.6-25.4 s over 4 proven-cold samples (worst slack 4,647 ms to the 30 s cap), warm 1.23-1.31 s, reported not gated. `tests::every_served_task_fits_its_door` passes, but see Warnings: it is satisfied only for empty posts. |
| 9 | Three-arm comparison on the same split: Laya, SetFit, Kev-4B | VERIFIED (override: Kev-4B blocked:budget, owner-authorised) | Laya and SetFit measured on the same 100 rows by one `bakeoff.py` run (`12-BAKEOFF-EVIDENCE.json`, `not_a_gate: true`); Laya's macro-F1 equals the gate bit for bit and I re-derived Laya's false assignment 7/63 at 0.6 and 4/63 at 0.85. SetFit s32 cell ran (recorded D-25 deviation, s64 infeasible), dump aligned by row identity. Kev-4B: Rust path proven against torch fp32 (max abs dprob 2.8e-6 short rows, 8.9e-7 on a 6,808-token row), leakage check 0 hits, 5.35 h of scoring, then stopped. Authorisation chain is recorded: first reply `option full` authorised the over-1-hour compute (12-KEV-OWNER-REPLY.json); the stop and the three-step routing are verbatim in `12-KEV-OWNER-REPLY-2.json` and supersede the first by sha256; `12-KEV-ARM-EVIDENCE.json` (sha `b117b126...`, equals the sha the bake-off pinned) says `outcome: blocked`, `failure_class: budget`; plan 12-15 line 96 pre-declared `budget` when the owner declined. Task 2 (fit, eval-probs) was not run: D-24/D-11/D-05 for Kev are satisfied only as `blocked: budget`. No calibrated Kev number exists and the hand-off claims none; the raw AUC / accuracy are labelled uncalibrated and unranked. This is a goal-level partial, accepted by override. |
| 10 | The hand-off leads with false assignment per arm and band, states prior correction (D-22), the source-batch blind spot (D-15), pilot status, and the measured door | VERIFIED | `12-HANDOFF.md` section 0 leads with the false-assignment table at 0.6 and 0.85 plus recall; section 8 gives `p'_k ∝ p_k · π_field,k / π_train,k` with train counts 66/41/188 and Table 7 figures and caveat, and states no server change was made; section 9 states source batch UNMEASURED; section 4.1 and section 10 state batch-of-4 admitted and 8 realistic posts refused. Numbers table cross-checked: pin, task sha, eval counts, gate figures and bake-off figures match the records I read. Threshold disclosure (`>` vs client `>=`) is present. |
| 12-DOC | Project documents name the second served model; contract and register guards stay green | VERIFIED | `CLAUDE.md` lines 277-292 name `aprender-mcp-decide-motivation`, sha and crate; I ran `readme_contract` (16 passed), `monorepo_invariants` (15), `aprender-mcp-decide every_served_task` (1), `aprender-contrastive-data --examples` (18, includes the three-label adapter and the Phase-10 default identity test). |

**Score:** 11/11 truths verified (one by recorded override; zero present-but-behaviour-unverified).

### Decision coverage (D-01..D-26, every id cited by a plan)

| ID | Status | Note |
|----|--------|------|
| D-01 | Satisfied | Labelled pilot in the hand-off, CLAUDE.md, crate docs and deploy record |
| D-02 | Satisfied | Register grew 9 to 10, allowlist grew, tests green |
| D-03 | Satisfied | Serving is `softmax(z / T)`; no prior correction; prior documented |
| D-04, D-15 | Satisfied | Length measured, source batch stated unmeasured |
| D-05 | Satisfied for Laya and SetFit; Kev blocked | Macro-F1 ranking with rates reported at 0.5/0.6/0.7/0.85 and a declared tiebreak that did not fire |
| D-06 | Satisfied | Laya served; findings empty |
| D-07, D-11, D-24 | Satisfied as blocked:budget | Branch and SHA `32103ba8` recorded, leakage 0, 4.5 omission recorded, nothing faked; see Truth 9 |
| D-08, D-25 | Satisfied | s8 rehearsal (12.5 GB RSS under the 32 GiB rule) then s32 cell; K-label adapter with Phase-10 default byte identity covered by tests; `--dump-probabilities` used |
| D-09, D-10 | Satisfied | Two variants measured, owner approved bytes `f74a3ad3cfc0` |
| D-12 | Satisfied | `classify` contract only |
| D-13, D-14, D-16 | Satisfied | Seed 13/17/23, median-ECE seed shipped; tertile cuts 126 / 244 recorded; prepare script pins sha and counts |
| D-17, D-18 | Satisfied | Spike `clamp_not_binding` |
| D-19, D-20 | Satisfied | Mechanism built and tested; not used; margin never waivable |
| D-21 | Satisfied | `class_reweighting: none (D-21)` |
| D-22 | Satisfied | In hand-off section 8 |
| D-23 | Satisfied | `ze-kasher-dev`, us-east-1; justfile region diff not depended on (`region_argument_passed: false`) and not touched |
| D-26 | Not triggered | Variant (a) chosen; `served_tasks` record only |

No requirement ids exist in REQUIREMENTS.md for this phase, so there are no orphaned requirements.

### Required Artifacts

| Artifact | Status | Details |
|----------|--------|---------|
| `models/decide/laya-motivation-v1.apr` (gitignored) | VERIFIED | sha matches the pin; 846,196,612 bytes |
| `data/decide/motivation-v1/{task.json,train.jsonl,eval.jsonl}` (gitignored) | VERIFIED | all three sha256 equal the gate and waiver records |
| `crates/aprender-mcp-decide-motivation-lambda/` | VERIFIED | `bootstrap` bin, `publish = false`, single dependency on the shared runtime, wired into the register |
| `scripts/laya_train/prepare_motivation.py`, `bakeoff.py`, `metrics.py` | VERIFIED | substantive (556 / 970 / 697 lines), used by the recorded runs |
| `crates/aprender-decide/src/{verify,pack,artifact}.rs` waiver path | VERIFIED | 16 waiver tests pass |
| `contracts/laya-finetune-gate-v1.yaml` 5.1.1, `decide-tool-boundary-v1.yaml` 10.1.0, `decide-apr-v1.yaml` 3.1.0 | VERIFIED | declaration present and bound by sha256 |
| Evidence records (`12-*-EVIDENCE.json`, owner replies, `12-ACCEPTANCE-REPLAY.json`, `12-HANDOFF.md`) | VERIFIED | all present; internal shas cross-agree |

### Key Link Verification

| From | To | Status | Details |
|------|----|--------|---------|
| Gate declaration commit | gate run start | WIRED | declared 104 s before the run, recipe_id and input shas identical to those run |
| Gate report | packed artifact | WIRED | pack line sha = verify sha = deploy readback env = live replay sha |
| Bake-off Laya arm | gate record | WIRED | macro-F1 bit-equal; I recomputed it independently |
| Hand-off numbers | records | WIRED | numbers table pointers resolve to values I cross-read |
| Live Lambda | artifact in S3 | WIRED | readback `s3_uri_ends_with_prefix_key: true`, env sha = pin |

### Data-Flow Trace (Level 4)

The served probabilities flow from the real packed weights: the gate's rescore of the artifact agrees with the trainer's probabilities to 9.1e-6 (bound 7.3e-5), 100/100 argmax. FLOWING. No hardcoded or static fallback was found in the thin crate (a pure shim).

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Live server answers with the approved task | POST `tools/list` to the endpoint, no credential | HTTP 200, `classify`, instruction and labels in order | PASS |
| Waiver refusals | `cargo test -p aprender-decide --lib waiver` | 16 passed | PASS |
| Served tasks fit door | `cargo test -p aprender-mcp-decide --lib every_served_task` | 1 passed | PASS |
| Register and doc drift | `cargo test -p aprender-core --test monorepo_invariants`; `--test readme_contract` | 15 passed; 16 passed | PASS |
| Duplicate bin allowlist | `bash scripts/check_duplicate_bin_names.sh` | PASS | PASS |
| Gate numbers reproduce | plain-Python recompute from `eval-probs.json` | F1 0.7475947, ECE 0.09355141, FA 7/63 and 4/63 | PASS |

I did not send a `classify` call (a cold call costs about 25 s of 10 GB compute and the recorded replay already covers it); I did not re-run the bake-off script or `laya-claims-check` (the former needs numpy under the project's uv environment; the caller reports 144 rows passing).

### Probe Execution

No phase-declared `probe-*.sh` scripts. The deploy probe binary was run by plans 12-12 and 12-13 and its output is recorded in the evidence; not re-run here (live, costs compute). SKIPPED.

### Requirements Coverage

See the decision table above. All 26 decision ids appear in at least one plan frontmatter and each resolves to a record or code located above.

### Anti-Patterns Found

I did not find debt markers or stubs in the new thin crate. The review (`12-REVIEW.md`, 0 critical, 6 warning, 5 info, all `open`) was read and weighed:

| Finding | Severity | Effect on phase goal |
|---------|----------|----------------------|
| WR-06 served door 8 overstated for P = 97 | Warning | Real: eight built rows leave 24 tokens for all eight posts, so the usable door is about 4 to 5 for real posts (post-token p50 45, p95 77; replay admitted 7 only because its synthetic sentences were 8-11 tokens). The hand-off discloses this in sections 4.1 and 10; the contract field `served_max_texts: 8` and the lambda README sentence still read as if 8 were usable. A client trusting the contract constant would be wrong, one trusting the hand-off would be right. Does not fail the phase; should be corrected in the contract/README. |
| WR-01 waiver has no ECE ceiling | Warning | Unused on the real run (gate passed unwaived). A latent weakness of a release-gating tool; the declaration `motivation-v1-pilot-ece` also remains committed in the contract and is unspent. |
| WR-02 waiver not bound to one attempt | Warning | Same: latent, unused. The one-declared-run rule is process-only. |
| WR-03 dump write after the one-shot test read | Warning | Latent risk to a one-shot SetFit measurement; the real cell completed. |
| WR-04, WR-05 bake-off provenance binding | Warning | I independently recomputed Laya's numbers from the files, so this phase's evidence does not depend on the unbound check. Latent for later runs. |
| IN-01..IN-05 | Info | Stale text, dead constant, justfile messages, band cut float noise; none affects a claim. |

## Warnings and items for the owner

1. **Kev-4B comparison is partial by design.** The method choice is evidenced by Laya vs SetFit only. If the owner wants the three-way comparison, the hand-off's reopen route (prefix cache plus batching, possibly GPU) is a new spike and needs a new owner decision. Kev's raw AUCs (0.90 / 0.70 / 0.67) are uncalibrated and unranked; nobody should read them as parity or superiority.
2. **Small-sample precision of the headline numbers.** 14 conspiratorial and 63 other eval rows, ECE interval up to 0.17, seed 17 above the bar, the Laya false-assignment rate in the longest band 4/14. These are disclosed in the hand-off; they bound what "gate passed" means (in-distribution calibration, not field accuracy). Accuracy on posts the partner holds is pending by design.
3. **WR-06 door overstatement** (above): recommend correcting the contract field or README wording in a follow-up.

## Tracking items (separate from phase pass/fail, not edited here)

- `.planning/STATE.md`, `.planning/ROADMAP.md` (Phase 12 plan checkboxes and the 12-16 progress line) and `.planning/REQUIREMENTS.md` are stale for this phase because `gsd_run state.*` and roadmap queries fail on `.planning/phases/04-*/04-VERIFICATION.md` (status `gaps_acknowledged`, outside the tool's closed set). That file must not be edited; the tracking update needs a manual or tool-fix path.
- `12-VALIDATION.md` frontmatter is still `status: draft`, `nyquist_compliant: false`.
- The foreign uncommitted changes (`.planning/STATE.md`, `.planning/config.json`, `.planning/state.json`, `justfile`) were not touched.

## Human Verification Required

None blocking. The items the phase itself defers to the partner (calibration sweep and accuracy on their own held-out posts, by-source results) are explicitly out of scope per D-04 and D-12, not unverified claims.

## Gaps Summary

No gaps. One must-have (the Kev-4B arm of the three-way comparison) is carried as an owner-accepted override with the authorisation chain recorded; six advisory review warnings are open and disclosed.

---

_Verified: 2026-10-26_
_Verifier: Claude (gsd-verifier)_
